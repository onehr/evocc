//! The known-bits lattice of Section 8.10.
//!
//! This is the producer's implementation. The checker has its own, written
//! separately from the same table, because Section 11.6 puts the analysis on
//! the "not shared" side of the boundary: a common defect here would make both
//! sides agree that an unsound premise was sound, which fails dangerous. The
//! duplication is the point, not drift.
//!
//! Every transfer below is one row of the Section 8.10 table and nothing more.
//! The table is deliberately weak, and strengthening a row here would make the
//! producer claim facts a checker recomputing the same table cannot confirm.

/// What is known about the bits of one `bits<N>` value.
///
/// A bit is known exactly where `mask` has a one, and then its value is the
/// corresponding bit of `value`. A bit set in `value` outside `mask` has no
/// meaning, so the constructors keep `value & !mask` empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownBits {
    width: u16,
    mask: u64,
    value: u64,
}

fn width_mask(width: u16) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

impl KnownBits {
    /// Nothing known: the top of the lattice.
    #[must_use]
    pub const fn top(width: u16) -> Self {
        Self {
            width,
            mask: 0,
            value: 0,
        }
    }

    /// Everything known: `ConstValue`, the bottom of the lattice.
    #[must_use]
    pub fn constant(width: u16, raw: u64) -> Self {
        Self {
            width,
            mask: width_mask(width),
            value: raw & width_mask(width),
        }
    }

    /// A fact with an explicit mask. Bits of `value` outside `mask` are dropped
    /// rather than kept, so two facts that mean the same thing compare equal.
    #[must_use]
    pub fn new(width: u16, mask: u64, value: u64) -> Self {
        let keep = width_mask(width);
        let mask = mask & keep;
        Self {
            width,
            mask,
            value: value & mask,
        }
    }

    #[must_use]
    pub const fn width(self) -> u16 {
        self.width
    }

    #[must_use]
    pub const fn mask(self) -> u64 {
        self.mask
    }

    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }

    /// The bits known to be one.
    #[must_use]
    pub const fn ones(self) -> u64 {
        self.mask & self.value
    }

    /// The bits known to be zero.
    #[must_use]
    pub const fn zeros(self) -> u64 {
        self.mask & !self.value
    }

    #[must_use]
    pub fn is_constant(self) -> bool {
        self.mask == width_mask(self.width)
    }

    /// `self ⊑ other`: every bit known in `other` is known here with the same
    /// value (Section 8.10 "Order"). Lower is more known.
    #[must_use]
    pub fn is_at_or_below(self, other: Self) -> bool {
        self.width == other.width
            && other.mask & !self.mask == 0
            && (self.value ^ other.value) & other.mask == 0
    }

    /// The meet of two facts about the same class: every bit known in either is
    /// known in the result. `None` when they disagree on a bit both know, which
    /// Section 8.10 makes an invalid union rather than a choice.
    #[must_use]
    pub fn meet(self, other: Self) -> Option<Self> {
        if self.width != other.width {
            return None;
        }
        let both = self.mask & other.mask;
        if (self.value ^ other.value) & both != 0 {
            return None;
        }
        Some(Self {
            width: self.width,
            mask: self.mask | other.mask,
            value: (self.value | other.value) & (self.mask | other.mask),
        })
    }

    /// The combination of two facts about *different* values that reach one
    /// class from different paths: a bit is known only where both know it and
    /// agree, so this loses knowledge where `meet` gains it.
    ///
    /// The two are easy to confuse, because Section 8.10 spells the merge of
    /// two classes already proved equal "meet" and spells the combination over
    /// incoming edges "meet" as well, in the dataflow sense where the
    /// combination is what loses precision. They are opposite operations and
    /// the difference is soundness: two values proved equal may each contribute
    /// what it knows, while two values arriving on different edges may
    /// contribute only what they agree on. `None` only for a width mismatch,
    /// which is a malformed pairing rather than a disagreement.
    #[must_use]
    pub fn join(self, other: Self) -> Option<Self> {
        if self.width != other.width {
            return None;
        }
        let agreed = self.mask & other.mask & !(self.value ^ other.value);
        Some(Self {
            width: self.width,
            mask: agreed,
            value: self.value & agreed,
        })
    }

    // -- transfers ----------------------------------------------------------

    /// Known where either input is known zero, or both are known one.
    #[must_use]
    pub fn transfer_and(self, other: Self) -> Self {
        let known_one = self.ones() & other.ones();
        let known_zero = self.zeros() | other.zeros();
        Self::new(self.width, known_one | known_zero, known_one)
    }

    /// Known where either input is known one, or both are known zero.
    #[must_use]
    pub fn transfer_or(self, other: Self) -> Self {
        let known_one = self.ones() | other.ones();
        let known_zero = self.zeros() & other.zeros();
        Self::new(self.width, known_one | known_zero, known_one)
    }

    /// Known where both inputs are known.
    #[must_use]
    pub fn transfer_xor(self, other: Self) -> Self {
        let mask = self.mask & other.mask;
        Self::new(self.width, mask, self.value ^ other.value)
    }

    /// The trailing run of bits known in both inputs, by ordinary carry
    /// propagation until the first unknown bit.
    ///
    /// The low `k` bits of a sum depend on nothing above bit `k`, which is what
    /// makes computing the whole sum and keeping the trailing run correct.
    #[must_use]
    pub fn transfer_add(self, other: Self) -> Self {
        self.carry(other, u64::wrapping_add)
    }

    /// The same run, by borrow propagation.
    #[must_use]
    pub fn transfer_sub(self, other: Self) -> Self {
        self.carry(other, u64::wrapping_sub)
    }

    fn carry(self, other: Self, combine: fn(u64, u64) -> u64) -> Self {
        let trailing = trailing_ones(self.mask & other.mask, self.width);
        Self::new(
            self.width,
            trailing_mask(trailing),
            combine(self.value, other.value),
        )
    }

    /// Section 11.6 `covers(?m, ?x)`, with `self` the mask `?m`: every bit that
    /// is not known zero in `?x` is known one in `?m`.
    ///
    /// This is a side condition over two facts, not a row of the Section 8.10
    /// table, so it derives nothing; it only reads what the table already
    /// established.
    #[must_use]
    pub fn covers(self, value: Self) -> bool {
        self.width == value.width && !value.zeros() & width_mask(self.width) & !self.ones() == 0
    }

    /// Section 11.6 `known_zero(?z)`: every bit known and every value bit zero.
    #[must_use]
    pub fn is_known_zero(self) -> bool {
        self.is_constant() && self.value == 0
    }

    /// The trailing known-zero run is the sum of the inputs' trailing
    /// known-zero runs; nothing else.
    #[must_use]
    pub fn transfer_mul(self, other: Self) -> Self {
        let zeros = trailing_ones(self.zeros(), self.width)
            .saturating_add(trailing_ones(other.zeros(), other.width))
            .min(u32::from(self.width));
        Self::new(self.width, trailing_mask(zeros), 0)
    }
}

impl KnownBits {
    // -- the shift and width rows, added at revision 1.9.43 -----------------
    //
    // A shift whose amount the analysis does not fully know, or knows to be at
    // or above the width, transfers to top: Section 8.10 leaves that case to
    // the shift's own definedness guard and states nothing about a guarded
    // operation.

    /// The amount a shift is by, when the table admits it.
    fn shift_amount(amount: Self, width: u16) -> Option<u32> {
        if !amount.is_constant() || amount.value() >= u64::from(width) {
            return None;
        }
        u32::try_from(amount.value()).ok()
    }

    /// Bit `i` is known zero for `i < k` and is input bit `i - k` above.
    #[must_use]
    pub fn transfer_shl(self, amount: Self) -> Self {
        let Some(shift) = Self::shift_amount(amount, self.width) else {
            return Self::top(self.width);
        };
        Self::new(
            self.width,
            (self.mask << shift) | trailing_mask(shift),
            self.value << shift,
        )
    }

    /// Bit `i` is input bit `i + k` while that is in range, known zero above.
    #[must_use]
    pub fn transfer_lshr(self, amount: Self) -> Self {
        let Some(shift) = Self::shift_amount(amount, self.width) else {
            return Self::top(self.width);
        };
        let vacated = width_mask(self.width) & !trailing_mask(u32::from(self.width) - shift);
        Self::new(
            self.width,
            (self.mask >> shift) | vacated,
            self.value >> shift,
        )
    }

    /// The same, except that the bits vacated are the input's sign bit and are
    /// known only when it is.
    #[must_use]
    pub fn transfer_ashr(self, amount: Self) -> Self {
        let Some(shift) = Self::shift_amount(amount, self.width) else {
            return Self::top(self.width);
        };
        let sign = 1_u64 << (self.width - 1);
        let vacated = width_mask(self.width) & !trailing_mask(u32::from(self.width) - shift);
        let known_sign = self.mask & sign != 0;
        let ones = if self.value & sign == 0 { 0 } else { vacated };
        Self::new(
            self.width,
            (self.mask >> shift) | if known_sign { vacated } else { 0 },
            (self.value >> shift) | ones,
        )
    }

    /// The low `W` bits are the input's; the upper bits are known zero.
    #[must_use]
    pub fn transfer_zext(self, width: u16) -> Self {
        let added = width_mask(width) & !width_mask(self.width);
        Self::new(width, self.mask | added, self.value)
    }

    /// The low `W` bits are the input's; the upper bits are the input's sign
    /// bit, known only when it is.
    #[must_use]
    pub fn transfer_sext(self, width: u16) -> Self {
        let sign = 1_u64 << (self.width - 1);
        let added = width_mask(width) & !width_mask(self.width);
        let known_sign = self.mask & sign != 0;
        let ones = if self.value & sign == 0 { 0 } else { added };
        Self::new(
            width,
            self.mask | if known_sign { added } else { 0 },
            self.value | ones,
        )
    }

    /// Bit `i` is input bit `i`, for `i` below the new width.
    #[must_use]
    pub fn transfer_trunc(self, width: u16) -> Self {
        Self::new(width, self.mask, self.value)
    }
}

/// How many bits, counting from bit zero, are set before the first clear one.
fn trailing_ones(bits: u64, width: u16) -> u32 {
    let limit = u32::from(width);
    (!bits).trailing_zeros().min(limit)
}

fn trailing_mask(count: u32) -> u64 {
    if count >= 64 {
        u64::MAX
    } else {
        (1_u64 << count) - 1
    }
}

/// The comparison a known-bits pair may decide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Eq,
    Ne,
    ULt,
    ULe,
    SLt,
    SLe,
}

impl KnownBits {
    /// `icmp.*`: fully known when the comparison is decidable from the known
    /// bits of both operands, top otherwise.
    ///
    /// Equality is decidable exactly when a bit both operands know differs, or
    /// when both are constants. The ordered comparisons are decided from the
    /// interval the known bits describe. Section 8.10 fixes what this row
    /// yields but not how to find it, so this is the obvious procedure and no
    /// stronger one.
    #[must_use]
    pub fn transfer_icmp(self, other: Self, comparison: Comparison) -> Self {
        let decided = match comparison {
            Comparison::Eq | Comparison::Ne => {
                let both = self.mask & other.mask;
                if (self.value ^ other.value) & both != 0 {
                    Some(false)
                } else if self.is_constant() && other.is_constant() {
                    Some(true)
                } else {
                    None
                }
            }
            Comparison::ULt => self.unsigned_order(other, false),
            Comparison::ULe => self.unsigned_order(other, true),
            Comparison::SLt => self.signed_order(other, false),
            Comparison::SLe => self.signed_order(other, true),
        };
        let Some(holds) = decided else {
            return Self::top(1);
        };
        let answer = match comparison {
            Comparison::Ne => !holds,
            _ => holds,
        };
        Self::constant(1, u64::from(answer))
    }

    /// The smallest and largest values the unknown bits allow.
    fn unsigned_bounds(self) -> (u64, u64) {
        let free = !self.mask & width_mask(self.width);
        (self.ones(), self.ones() | free)
    }

    fn unsigned_order(self, other: Self, allow_equal: bool) -> Option<bool> {
        let (low, high) = self.unsigned_bounds();
        let (other_low, other_high) = other.unsigned_bounds();
        decide(
            i128::from(low),
            i128::from(high),
            i128::from(other_low),
            i128::from(other_high),
            allow_equal,
        )
    }

    /// Signed order, decided only when both sign bits are known.
    ///
    /// With an unknown sign bit the values an operand may take are not one
    /// interval in signed order, so the interval rule does not apply and the
    /// answer is top. Deciding more would need a case split the table does not
    /// license.
    fn signed_order(self, other: Self, allow_equal: bool) -> Option<bool> {
        let sign = 1_u64 << (self.width - 1);
        if self.mask & sign == 0 || other.mask & sign == 0 {
            return None;
        }
        let width = self.width;
        let decode = move |raw: u64| {
            let raw = raw & width_mask(width);
            if raw & sign == 0 {
                i128::from(raw)
            } else {
                i128::from(raw) - (1_i128 << width)
            }
        };
        let (low, high) = self.unsigned_bounds();
        let (other_low, other_high) = other.unsigned_bounds();
        decide(
            decode(low),
            decode(high),
            decode(other_low),
            decode(other_high),
            allow_equal,
        )
    }
}

fn decide(
    low: i128,
    high: i128,
    other_low: i128,
    other_high: i128,
    allow_equal: bool,
) -> Option<bool> {
    if allow_equal {
        if high <= other_low {
            return Some(true);
        }
        if low > other_high {
            return Some(false);
        }
    } else {
        if high < other_low {
            return Some(true);
        }
        if low >= other_high {
            return Some(false);
        }
    }
    None
}
