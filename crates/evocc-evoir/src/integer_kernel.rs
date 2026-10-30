//! EvoIR integer kernel.

use super::*;

impl IntegerKernel {
    pub(super) fn for_value(value: &Value, block: BlockId) -> Result<Self, InterpreterError> {
        Self::for_type(&value.ty, block)
    }

    /// The executed widths are 1 to 64 and 128, and nothing between.
    ///
    /// Section 1.1 admits every positive width; execution admits the two
    /// ranges a value model exists for. A width outside them is reported as
    /// `not-executed: wide-integer`, which is the only diagnostic the
    /// specification names for the case.
    pub(super) fn for_type(ty: &EvoIRType, block: BlockId) -> Result<Self, InterpreterError> {
        let EvoIRType::Bits(width @ (1..=64 | 128)) = ty else {
            let width = match ty {
                EvoIRType::Bits(width) => *width,
                _ => 0,
            };
            return Err(InterpreterError::UnsupportedBitWidth { block, width });
        };
        let mask = if *width >= 128 {
            u128::MAX
        } else {
            (1_u128 << width) - 1
        };
        Ok(Self {
            width: *width,
            mask,
        })
    }

    pub(super) fn normalize(self, value: u128) -> u128 {
        value & self.mask
    }

    pub(super) fn add(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_add(right))
    }

    pub(super) fn sub(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_sub(right))
    }

    pub(super) fn mul(self, left: u128, right: u128) -> u128 {
        self.normalize(left.wrapping_mul(right))
    }

    pub(super) fn shl(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            0
        } else {
            self.normalize(self.normalize(left) << amount)
        }
    }

    pub(super) fn lshr(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            0
        } else {
            self.normalize(left) >> amount
        }
    }

    pub(super) fn ashr(self, left: u128, amount: u128) -> u128 {
        if amount >= u128::from(self.width) {
            if self.signed(left) < 0 {
                self.mask
            } else {
                0
            }
        } else {
            self.encode_signed(self.signed(left) >> amount)
        }
    }

    pub(super) fn signed_add_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_add(self.signed(right))
            .is_some_and(|sum| self.signed_fits(sum))
    }

    pub(super) fn signed_sub_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_sub(self.signed(right))
            .is_some_and(|difference| self.signed_fits(difference))
    }

    pub(super) fn signed_mul_defined(self, left: u128, right: u128) -> bool {
        self.signed(left)
            .checked_mul(self.signed(right))
            .is_some_and(|product| self.signed_fits(product))
    }

    pub(super) fn signed_minimum(self) -> i128 {
        if self.width >= 128 {
            i128::MIN
        } else {
            -(1_i128 << (self.width - 1))
        }
    }

    pub(super) fn signed_div_rem_no_overflow(self, left: u128, right: u128) -> bool {
        !(self.signed(left) == self.signed_minimum() && self.signed(right) == -1)
    }

    pub(super) fn decoded_shift_amount(
        self,
        amount_kernel: Self,
        amount: u128,
        amount_signed: bool,
    ) -> Option<u64> {
        let mathematical = if amount_signed {
            amount_kernel.signed(amount)
        } else {
            i128::try_from(amount_kernel.normalize(amount)).ok()?
        };
        if mathematical < 0 || mathematical >= i128::from(self.width) {
            None
        } else {
            u64::try_from(mathematical).ok()
        }
    }

    pub(super) fn shift_count_defined(
        self,
        amount_kernel: Self,
        amount: u128,
        amount_signed: bool,
    ) -> bool {
        self.decoded_shift_amount(amount_kernel, amount, amount_signed)
            .is_some()
    }

    /// EvoIR 3.3.1: a valid count, a non-negative left operand, and a shifted
    /// result representable in signed `bits<N>`.
    ///
    /// The representability test is the round trip rather than a widened
    /// product, because at `N = 128` there is no wider signed type to compute
    /// the product in. For a non-negative operand the two agree exactly: the
    /// shift loses no bit and leaves the sign clear precisely when the value
    /// is below `2^(N - 1 - amount)`.
    pub(super) fn signed_shl_defined(
        self,
        left: u128,
        amount_kernel: Self,
        amount: u128,
        amount_signed: bool,
    ) -> bool {
        let Some(amount) = self.decoded_shift_amount(amount_kernel, amount, amount_signed) else {
            return false;
        };
        if self.signed(left) < 0 {
            return false;
        }
        let amount = u128::from(amount);
        let shifted = self.shl(left, amount);
        self.signed(shifted) >= 0 && self.lshr(shifted, amount) == self.normalize(left)
    }

    pub(super) fn signed_fits(self, value: i128) -> bool {
        if self.width >= 128 {
            return true;
        }
        let minimum = -(1_i128 << (self.width - 1));
        let maximum = (1_i128 << (self.width - 1)) - 1;
        (minimum..=maximum).contains(&value)
    }

    pub(super) fn signed(self, value: u128) -> i128 {
        let value = self.normalize(value);
        if self.width >= 128 {
            return value as i128;
        }
        let sign_bit = 1_u128 << (self.width - 1);
        if value & sign_bit == 0 {
            value as i128
        } else {
            (value as i128) - (1_i128 << self.width)
        }
    }

    pub(super) fn encode_signed(self, value: i128) -> u128 {
        self.normalize(value as u128)
    }

    pub(super) fn sdiv(
        self,
        left: u128,
        right: u128,
        block: BlockId,
    ) -> Result<u128, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "signed division",
            });
        }
        if left == self.signed_minimum() && right == -1 {
            return Err(InterpreterError::SignedDivisionOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left / right))
    }

    pub(super) fn udiv(
        self,
        left: u128,
        right: u128,
        block: BlockId,
    ) -> Result<u128, InterpreterError> {
        let left = self.normalize(left);
        let right = self.normalize(right);
        if right == 0 {
            return Err(InterpreterError::DivisionByZero {
                block,
                operation: "unsigned division",
            });
        }
        Ok(left / right)
    }

    pub(super) fn srem(
        self,
        left: u128,
        right: u128,
        block: BlockId,
    ) -> Result<u128, InterpreterError> {
        let left = self.signed(left);
        let right = self.signed(right);
        if right == 0 {
            return Err(InterpreterError::RemainderByZero {
                block,
                operation: "signed remainder",
            });
        }
        if left == self.signed_minimum() && right == -1 {
            return Err(InterpreterError::SignedRemainderOverflow {
                block,
                width: self.width,
            });
        }
        Ok(self.encode_signed(left % right))
    }

    pub(super) fn urem(
        self,
        left: u128,
        right: u128,
        block: BlockId,
    ) -> Result<u128, InterpreterError> {
        let left = self.normalize(left);
        let right = self.normalize(right);
        if right == 0 {
            return Err(InterpreterError::RemainderByZero {
                block,
                operation: "unsigned remainder",
            });
        }
        Ok(left % right)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Operand rewriting
// ---------------------------------------------------------------------------
