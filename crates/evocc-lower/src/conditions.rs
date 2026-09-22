//! Bounded, source-width ranges within a single short-circuit expression.
//! A result is usable only after the original condition has been evaluated.

use super::*;
use evocc_types::FloatKind;

// The fourth partition is essential: != includes unordered, whereas each
// ordered relational comparison excludes it. Keys name stable source values
// at one format, not merely two expressions with similar printed text.
struct FloatingPartition {
    operands: (SymbolId, SymbolId, FloatKind),
    mask: u8,
}

#[derive(Clone, Copy)]
struct Range {
    low: i128,
    high: i128,
    ty: CIntegerType,
}

impl Range {
    fn full(ty: CIntegerType) -> Option<Self> {
        Some(Self {
            low: if ty.is_signed() { ty.signed_min() } else { 0 },
            high: if ty.is_signed() {
                ty.signed_max()
            } else {
                i128::try_from(ty.mask()).ok()?
            },
            ty,
        })
    }

    // Decline wrapping, narrowing and disjoint images. This is a conservative
    // interval fact, never a change to C's actual conversion semantics.
    fn convert(self, ty: CIntegerType) -> Option<Self> {
        let full = Self::full(ty)?;
        (self.low >= full.low && self.high <= full.high).then_some(Self { ty, ..self })
    }
}

impl FunctionLowerer<'_> {
    pub(super) fn floating_partition_result(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        match self.floating_partition(expression, 16)?.mask {
            0 => Some(false),
            15 => Some(true),
            _ => None,
        }
    }

    fn floating_partition(
        &self,
        expression: &CHirExpr<'_>,
        depth: u8,
    ) -> Option<FloatingPartition> {
        use evocc_chir::floating::FloatingBuiltin::*;
        let depth = depth.checked_sub(1)?;
        let (left, right, mut mask) = match expression {
            CHirExpr::Unary(UnaryOp::Not, operand, _) => {
                let mut partition = self.floating_partition(operand, depth)?;
                partition.mask ^= 15;
                return Some(partition);
            }
            CHirExpr::Binary(op @ (BinaryOp::And | BinaryOp::Or), left, right, _) => {
                let mut a = self.floating_partition(left, depth)?;
                let b = self.floating_partition(right, depth)?;
                if a.operands != b.operands {
                    return None;
                }
                a.mask = if *op == BinaryOp::And {
                    a.mask & b.mask
                } else {
                    a.mask | b.mask
                };
                return Some(a);
            }
            CHirExpr::Binary(op, left, right, _) => {
                let mask = match op {
                    BinaryOp::Lt => 1,
                    BinaryOp::Eq => 2,
                    BinaryOp::Gt => 4,
                    BinaryOp::Le => 3,
                    BinaryOp::Ne => 13,
                    BinaryOp::Ge => 6,
                    _ => return None,
                };
                (*left, *right, mask)
            }
            CHirExpr::Floating(expression) if expression.op.is_comparison() => {
                let [left, right] = expression.operands.as_slice() else {
                    return None;
                };
                let mask = match expression.op {
                    IsLess => 1,
                    IsLessEqual => 3,
                    IsGreater => 4,
                    IsGreaterEqual => 6,
                    IsLessGreater => 5,
                    IsUnordered => 8,
                    _ => return None,
                };
                (*left, *right, mask)
            }
            _ => return None,
        };
        let (mut a, kind) = self.stable_floating_operand(left, depth)?;
        let (mut b, other) = self.stable_floating_operand(right, depth)?;
        if kind != other {
            return None;
        }
        if a > b {
            std::mem::swap(&mut a, &mut b);
            mask = (mask & 10) | ((mask & 1) << 2) | ((mask & 4) >> 2);
        }
        Some(FloatingPartition {
            operands: (a, b, kind),
            mask,
        })
    }

    fn stable_floating_operand(
        &self,
        expression: &CHirExpr<'_>,
        depth: u8,
    ) -> Option<(SymbolId, FloatKind)> {
        let depth = depth.checked_sub(1)?;
        match expression {
            CHirExpr::Cast(ty, operand, _) => {
                let value = self.stable_floating_operand(operand, depth)?;
                (self.unit.types().get(*ty)?.kind == TypeKind::Float(value.1)).then_some(value)
            }
            CHirExpr::Ident { symbol, .. } => {
                if self.memory_symbols.contains(symbol)
                    || self.global_registers.contains_key(symbol)
                {
                    return None;
                }
                let place =
                    ScalarPlace::bind(self.unit.symbols(), self.unit.types(), *symbol).ok()?;
                let ty = self.unit.types().get(place.ty())?;
                if ty.qualifiers.is_volatile || ty.qualifiers.is_atomic {
                    return None;
                }
                let TypeKind::Float(kind @ (FloatKind::F32 | FloatKind::F64)) = ty.kind else {
                    return None;
                };
                Some((*symbol, kind))
            }
            // Changed formats, object reads, calls and updates have no common
            // stable partition here. In particular a pointer is not a key.
            _ => None,
        }
    }

    pub(super) fn short_circuit_range_result(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        let CHirExpr::Binary(logical @ (BinaryOp::And | BinaryOp::Or), left, right, _) = expression
        else {
            return None;
        };
        let CHirExpr::Binary(op, CHirExpr::Ident { symbol, .. }, constant, _) = left else {
            return None;
        };
        if self.memory_symbols.contains(symbol) || self.global_registers.contains_key(symbol) {
            return None;
        }
        let place = ScalarPlace::bind(self.unit.symbols(), self.unit.types(), *symbol).ok()?;
        let source = self.unit.types().get(place.ty())?;
        if source.qualifiers.is_volatile || source.qualifiers.is_atomic {
            return None;
        }
        let integer =
            canonical_integer(self.unit.types(), self.target, place.ty(), left.span()).ok()?;
        let constant = constant_integer(self.unit, self.target, constant)?;
        let common = usual_arithmetic_type(integer, constant.ty());
        let full = Range::full(integer)?.convert(common)?;
        let value = constant.cast(common).try_mathematical_value()?;
        // LT/EQ/GT partitions of the promoted left comparison. Only the
        // partitions which enter the right operand constrain that operand.
        let mask = match op {
            BinaryOp::Lt => 1u8,
            BinaryOp::Eq => 2,
            BinaryOp::Gt => 4,
            BinaryOp::Le => 3,
            BinaryOp::Ne => 5,
            BinaryOp::Ge => 6,
            _ => return None,
        };
        let desired = *logical == BinaryOp::Or;
        let selected = if desired { mask ^ 7 } else { mask };
        for (bit, low, high) in [
            (1, full.low, value.checked_sub(1)?.min(full.high)),
            (2, value.max(full.low), value.min(full.high)),
            (4, value.checked_add(1)?.max(full.low), full.high),
        ] {
            if selected & bit == 0 || low > high {
                continue;
            }
            let input = Range {
                low,
                high,
                ty: integer,
            };
            let result = self.guarded_integer_range(right, *symbol, input, 16)?;
            if if desired {
                result.low <= 0 && result.high >= 0
            } else {
                result.low != 0 || result.high != 0
            } {
                return None;
            }
        }
        Some(desired)
    }

    fn guarded_integer_range(
        &self,
        expression: &CHirExpr<'_>,
        symbol: SymbolId,
        input: Range,
        depth: u8,
    ) -> Option<Range> {
        let depth = depth.checked_sub(1)?;
        if let Some(value) = constant_integer(self.unit, self.target, expression) {
            let value_math = value.try_mathematical_value()?;
            return Some(Range {
                low: value_math,
                high: value_math,
                ty: value.ty(),
            });
        }
        match expression {
            CHirExpr::Ident { symbol: found, .. } if *found == symbol => Some(input),
            CHirExpr::Cast(ty, operand, _) => {
                let target =
                    canonical_integer(self.unit.types(), self.target, *ty, expression.span())
                        .ok()?;
                self.guarded_integer_range(operand, symbol, input, depth)?
                    .convert(target)
            }
            CHirExpr::Binary(op, left, right, _) => {
                let a = self.guarded_integer_range(left, symbol, input, depth)?;
                let b = self.guarded_integer_range(right, symbol, input, depth)?;
                let ty = usual_arithmetic_type(a.ty, b.ty);
                let a = a.convert(ty)?;
                let b = b.convert(ty)?;
                let result = match op {
                    BinaryOp::Add => {
                        return Range {
                            low: a.low.checked_add(b.low)?,
                            high: a.high.checked_add(b.high)?,
                            ty,
                        }
                        .convert(ty)
                    }
                    BinaryOp::Sub => {
                        return Range {
                            low: a.low.checked_sub(b.high)?,
                            high: a.high.checked_sub(b.low)?,
                            ty,
                        }
                        .convert(ty)
                    }
                    BinaryOp::Lt if a.high < b.low => true,
                    BinaryOp::Lt if a.low >= b.high => false,
                    BinaryOp::Le if a.high <= b.low => true,
                    BinaryOp::Le if a.low > b.high => false,
                    BinaryOp::Gt if a.low > b.high => true,
                    BinaryOp::Gt if a.high <= b.low => false,
                    BinaryOp::Ge if a.low >= b.high => true,
                    BinaryOp::Ge if a.high < b.low => false,
                    BinaryOp::Eq | BinaryOp::Ne if a.low == a.high && b.low == b.high => {
                        (a.low == b.low) == (*op == BinaryOp::Eq)
                    }
                    BinaryOp::Eq | BinaryOp::Ne if a.high < b.low || b.high < a.low => {
                        *op == BinaryOp::Ne
                    }
                    _ => return None,
                };
                Some(Range {
                    low: i128::from(result),
                    high: i128::from(result),
                    ty: CIntegerType::INT,
                })
            }
            // Calls, assignments, updates, other reads and changing widths
            // do not inherit the first operand's variable bounds.
            _ => None,
        }
    }
}
