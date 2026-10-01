//! The checker's own known-bits lattice, written from the Section 8.10 table.
//!
//! Section 11.6 puts the analysis on the "not shared" side of the boundary: a
//! defect common to producer and checker would make both agree that an unsound
//! premise was sound, and a certificate would be accepted. Sharing fails
//! dangerous here, so this file imports nothing from the optimizer and repeats
//! the table rather than calling it.
//!
//! The duplication is deliberate down to the representation. A fact is carried
//! as the two sets of bits it actually asserts -- known zero and known one --
//! rather than as a mask and a value, so a transfer written here has to be
//! derived from the table again instead of transcribed. The certificate's wire
//! form is mask and value, and `from_premise` is the only place the two meet.

/// What is known about the bits of one `bits<N>` value.
///
/// `zeros` and `ones` are disjoint by construction; a bit in neither is
/// unknown, which is the top of the lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fact {
    width: u16,
    zeros: u64,
    ones: u64,
}

fn span(width: u16) -> u64 {
    if width >= 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

/// How many bits, counting from bit zero, are set before the first clear one.
fn run(bits: u64, width: u16) -> u32 {
    (!bits).trailing_zeros().min(u32::from(width))
}

fn low(count: u32) -> u64 {
    if count >= 64 {
        u64::MAX
    } else {
        (1_u64 << count) - 1
    }
}

impl Fact {
    /// Independently derived g0 rows. This module imports no producer or
    /// attribution code; it keeps the checker's disjoint zero/one sets.
    pub(crate) fn guard(self, operation: &str, other: Option<Self>) -> Option<Self> {
        if !(1..=64).contains(&self.width) {
            return None;
        }
        let same_width = || other.filter(|f| f.width == self.width);
        let proved = match operation {
            "divisor_nonzero" => self.ones != 0,
            "signed_div_rem_no_overflow" => {
                let rhs = same_width()?;
                let minimum = 1_u64 << (self.width - 1);
                !self.admits_pattern(minimum) || !rhs.admits_pattern(span(rhs.width))
            }
            "signed_add_defined" => {
                let rhs = same_width()?;
                let (a, b) = (self.signed_extrema(), rhs.signed_extrema());
                self.signed_fits(a.0 + b.0) && self.signed_fits(a.1 + b.1)
            }
            "signed_sub_defined" => {
                let rhs = same_width()?;
                let (a, b) = (self.signed_extrema(), rhs.signed_extrema());
                self.signed_fits(a.0 - b.1) && self.signed_fits(a.1 - b.0)
            }
            "signed_mul_defined" => {
                let rhs = same_width()?;
                let (a, b) = (self.signed_extrema(), rhs.signed_extrema());
                for x in [a.0, a.1] {
                    for y in [b.0, b.1] {
                        if !self.signed_fits(x * y) {
                            return Some(Self::top(1));
                        }
                    }
                }
                true
            }
            "shift_count_defined.unsigned" => {
                let k = other.filter(|k| (1..=64).contains(&k.width))?;
                (span(k.width) & !k.zeros) < u64::from(self.width)
            }
            "shift_count_defined.signed" => {
                let k = other.filter(|k| (1..=64).contains(&k.width))?;
                let sign = 1_u64 << (k.width - 1);
                k.zeros & sign != 0 && (span(k.width) & !k.zeros) < u64::from(self.width)
            }
            _ => return None,
        };
        Some(if proved {
            Self::constant(1, 1)
        } else {
            Self::top(1)
        })
    }

    fn admits_pattern(self, pattern: u64) -> bool {
        self.zeros & pattern == 0 && self.ones & !pattern == 0
    }

    fn signed_extrema(self) -> (i128, i128) {
        let sign = 1_u64 << (self.width - 1);
        let half = 1_i128 << (self.width - 1);
        let maximum = i128::from(span(self.width) & !self.zeros);
        let minimum = i128::from(self.ones);
        if self.zeros & sign != 0 {
            (minimum, maximum)
        } else if self.ones & sign != 0 {
            (minimum - 2 * half, maximum - 2 * half)
        } else {
            (minimum - half, maximum - half)
        }
    }

    fn signed_fits(self, value: i128) -> bool {
        let half = 1_i128 << (self.width - 1);
        value >= -half && value < half
    }

    /// Nothing known.
    pub(crate) const fn top(width: u16) -> Self {
        Self {
            width,
            zeros: 0,
            ones: 0,
        }
    }

    /// Every bit known: the `ConstValue` of Section 8.10.
    pub(crate) fn constant(width: u16, raw: u64) -> Self {
        let ones = raw & span(width);
        Self {
            width,
            zeros: span(width) & !ones,
            ones,
        }
    }

    /// The fact a certificate's `class:mask:value` premise asserts.
    ///
    /// `None` when the pair is not a fact about a value of this width: a bit
    /// set outside the mask, or either field reaching past the width.
    pub(crate) fn from_premise(width: u16, mask: u64, value: u64) -> Option<Self> {
        if value & !mask != 0 || (mask | value) & !span(width) != 0 {
            return None;
        }
        Some(Self {
            width,
            zeros: mask & !value,
            ones: value,
        })
    }

    pub(crate) const fn width(self) -> u16 {
        self.width
    }

    /// The value every bit of a fully known fact holds.
    pub(crate) const fn raw(self) -> u64 {
        self.ones
    }

    const fn known(self) -> u64 {
        self.zeros | self.ones
    }

    pub(crate) fn is_fully_known(self) -> bool {
        self.known() == span(self.width)
    }

    /// Section 11.6 `known_zero(?z)`.
    pub(crate) fn is_known_zero(self) -> bool {
        self.zeros == span(self.width)
    }

    /// Section 11.6 `covers(?m, ?x)`, with `self` the mask: every bit not known
    /// zero in `value` is known one here.
    pub(crate) fn covers(self, value: Self) -> bool {
        self.width == value.width && span(self.width) & !value.zeros & !self.ones == 0
    }

    /// Section 8.10 order: `self` is at or below `claim` when every bit `claim`
    /// knows, `self` knows with the same value.
    ///
    /// This is the direction revision 1.9.40 corrected. A premise is confirmed
    /// only when the checker's own fact is the stronger one; a premise that
    /// asserts more than the checker derived asserts something unverified.
    pub(crate) fn is_at_or_below(self, claim: Self) -> bool {
        self.width == claim.width && claim.zeros & !self.zeros == 0 && claim.ones & !self.ones == 0
    }

    /// Section 8.10 merge: every bit known in either is known in the result,
    /// and a disagreement on a bit both know is an invalid union.
    pub(crate) fn meet(self, other: Self) -> Option<Self> {
        if self.width != other.width || self.zeros & other.ones != 0 || self.ones & other.zeros != 0
        {
            return None;
        }
        Some(Self {
            width: self.width,
            zeros: self.zeros | other.zeros,
            ones: self.ones | other.ones,
        })
    }

    // -- the transfer table --------------------------------------------------

    /// Known where either input is known zero, or both are known one.
    /// What two facts arriving on different edges agree on: a bit is known
    /// only where both know it and give it the same value.
    ///
    /// This is not `meet`. `meet` combines two facts about one value already
    /// proved equal and may keep what either knows; this combines facts about
    /// two different values reaching one argument and may keep only what both
    /// know. Section 8.10 spells them with the same word, in the two senses the
    /// dataflow literature gives it, and the difference is soundness rather
    /// than taste: a bit known on one edge alone says nothing about the value
    /// that arrives on the other.
    pub(crate) fn join(self, other: Self) -> Self {
        if self.width != other.width {
            return Self::top(self.width);
        }
        Self {
            width: self.width,
            zeros: self.zeros & other.zeros,
            ones: self.ones & other.ones,
        }
    }

    pub(crate) fn and(self, other: Self) -> Self {
        Self {
            width: self.width,
            zeros: self.zeros | other.zeros,
            ones: self.ones & other.ones,
        }
    }

    /// Known where either input is known one, or both are known zero.
    pub(crate) fn or(self, other: Self) -> Self {
        Self {
            width: self.width,
            zeros: self.zeros & other.zeros,
            ones: self.ones | other.ones,
        }
    }

    /// Known where both inputs are known.
    pub(crate) fn xor(self, other: Self) -> Self {
        let known = self.known() & other.known();
        let ones = known & (self.ones ^ other.ones);
        Self {
            width: self.width,
            zeros: known & !ones,
            ones,
        }
    }

    /// The trailing run of bits known in both inputs, by ordinary carry
    /// propagation until the first unknown bit. Nothing above that run can be
    /// asserted, because the carry into it is unknown.
    pub(crate) fn add(self, other: Self) -> Self {
        self.carried(other, u64::wrapping_add)
    }

    pub(crate) fn sub(self, other: Self) -> Self {
        self.carried(other, u64::wrapping_sub)
    }

    fn carried(self, other: Self, combine: fn(u64, u64) -> u64) -> Self {
        let settled = low(run(self.known() & other.known(), self.width));
        let raw = combine(self.ones, other.ones) & settled;
        Self {
            width: self.width,
            zeros: settled & !raw,
            ones: raw,
        }
    }

    /// The trailing known-zero run is the sum of the inputs' trailing
    /// known-zero runs; nothing else.
    pub(crate) fn mul(self, other: Self) -> Self {
        let trailing = run(self.zeros, self.width)
            .saturating_add(run(other.zeros, other.width))
            .min(u32::from(self.width));
        Self {
            width: self.width,
            zeros: low(trailing),
            ones: 0,
        }
    }

    // -- the shift and width rows, added at revision 1.9.43 -----------------
    //
    // A shift by an amount that is not fully known, or is known and not below
    // the width, transfers to top: the out-of-range case belongs to the shift's
    // own definedness guard, and this analysis says nothing about a guarded
    // operation.

    /// How far a shift shifts, when the table admits the amount at all.
    fn distance(amount: Self, width: u16) -> Option<u32> {
        if !amount.is_fully_known() || amount.ones >= u64::from(width) {
            return None;
        }
        u32::try_from(amount.ones).ok()
    }

    /// Everything moves up; what moves in below is zero.
    pub(crate) fn shl(self, amount: Self) -> Self {
        let Some(distance) = Self::distance(amount, self.width) else {
            return Self::top(self.width);
        };
        Self {
            width: self.width,
            zeros: ((self.zeros << distance) | low(distance)) & span(self.width),
            ones: (self.ones << distance) & span(self.width),
        }
    }

    /// Everything moves down; what moves in above is zero.
    pub(crate) fn lshr(self, amount: Self) -> Self {
        let Some(distance) = Self::distance(amount, self.width) else {
            return Self::top(self.width);
        };
        Self {
            width: self.width,
            zeros: (self.zeros >> distance) | self.vacated(distance),
            ones: self.ones >> distance,
        }
    }

    /// Everything moves down; what moves in above is the sign, if the sign is
    /// known at all.
    pub(crate) fn ashr(self, amount: Self) -> Self {
        let Some(distance) = Self::distance(amount, self.width) else {
            return Self::top(self.width);
        };
        let vacated = self.vacated(distance);
        Self {
            width: self.width,
            zeros: (self.zeros >> distance) | (vacated & self.sign_zero()),
            ones: (self.ones >> distance) | (vacated & self.sign_one()),
        }
    }

    /// The bits a right shift by `distance` leaves behind at the top.
    fn vacated(self, distance: u32) -> u64 {
        span(self.width) & !low(u32::from(self.width) - distance)
    }

    fn sign_bit(self) -> u64 {
        1 << (self.width - 1)
    }

    /// All ones when the sign is known zero, nothing otherwise, so that either
    /// can be used as a mask over the bits an extension adds.
    fn sign_zero(self) -> u64 {
        if self.zeros & self.sign_bit() == 0 {
            0
        } else {
            u64::MAX
        }
    }

    fn sign_one(self) -> u64 {
        if self.ones & self.sign_bit() == 0 {
            0
        } else {
            u64::MAX
        }
    }

    /// The bits an extension to `width` adds above this fact's own.
    fn added(self, width: u16) -> u64 {
        span(width) & !span(self.width)
    }

    /// The added bits are zero.
    pub(crate) fn zext(self, width: u16) -> Self {
        Self {
            width,
            zeros: self.zeros | self.added(width),
            ones: self.ones,
        }
    }

    /// The added bits are the sign, if the sign is known.
    pub(crate) fn sext(self, width: u16) -> Self {
        let added = self.added(width);
        Self {
            width,
            zeros: self.zeros | (added & self.sign_zero()),
            ones: self.ones | (added & self.sign_one()),
        }
    }

    /// What is left of the low bits.
    pub(crate) fn trunc(self, width: u16) -> Self {
        Self {
            width,
            zeros: self.zeros & span(width),
            ones: self.ones & span(width),
        }
    }

    /// The smallest and largest values the unknown bits allow, read unsigned.
    fn bounds(self) -> (u64, u64) {
        (self.ones, self.ones | (span(self.width) & !self.known()))
    }

    fn signed(self, raw: u64) -> i128 {
        let sign = 1_u64 << (self.width - 1);
        if raw & sign == 0 {
            i128::from(raw)
        } else {
            i128::from(raw) - (1_i128 << self.width)
        }
    }

    fn sign_known(self) -> bool {
        self.known() & (1_u64 << (self.width - 1)) != 0
    }

    /// `icmp.*`, decided by the interval procedure of Section 8.10 and by
    /// nothing else. A procedure that decided more cases would be a stronger
    /// analysis than the table states.
    pub(crate) fn compare(self, other: Self, comparison: Comparison) -> Fact {
        if self.width != other.width {
            return Fact::top(1);
        }
        let decided = match comparison {
            Comparison::Eq | Comparison::Ne => {
                if self.ones & other.zeros != 0 || self.zeros & other.ones != 0 {
                    Some(false)
                } else if self.is_fully_known() && other.is_fully_known() {
                    Some(true)
                } else {
                    None
                }
            }
            Comparison::ULt | Comparison::ULe => {
                let (low, high) = self.bounds();
                let (other_low, other_high) = other.bounds();
                order(
                    i128::from(low),
                    i128::from(high),
                    i128::from(other_low),
                    i128::from(other_high),
                    comparison == Comparison::ULe,
                )
            }
            Comparison::SLt | Comparison::SLe => {
                if !self.sign_known() || !other.sign_known() {
                    None
                } else {
                    let (low, high) = self.bounds();
                    let (other_low, other_high) = other.bounds();
                    order(
                        self.signed(low),
                        self.signed(high),
                        other.signed(other_low),
                        other.signed(other_high),
                        comparison == Comparison::SLe,
                    )
                }
            }
        };
        match decided {
            None => Fact::top(1),
            Some(holds) => Fact::constant(
                1,
                u64::from(if comparison == Comparison::Ne {
                    !holds
                } else {
                    holds
                }),
            ),
        }
    }
}

fn order(
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

/// The comparison an `icmp` node names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Comparison {
    Eq,
    Ne,
    SLt,
    SLe,
    ULt,
    ULe,
}

impl Comparison {
    /// The operator table of Section 11.3 is exact, so an unknown spelling is
    /// not a comparison at all.
    pub(crate) fn from_operation(operation: &str) -> Option<Self> {
        Some(match operation {
            "icmp.eq" => Self::Eq,
            "icmp.ne" => Self::Ne,
            "icmp.slt" => Self::SLt,
            "icmp.sle" => Self::SLe,
            "icmp.ult" => Self::ULt,
            "icmp.ule" => Self::ULe,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Comparison, Fact};

    /// `zeros`/`ones` written as a bit string, most significant first, with `?`
    /// for an unknown bit.
    fn bits(pattern: &str) -> Fact {
        let width = u16::try_from(pattern.len()).expect("a pattern fits a width");
        let mut mask = 0_u64;
        let mut value = 0_u64;
        for (index, symbol) in pattern.chars().rev().enumerate() {
            match symbol {
                '?' => {}
                '0' => mask |= 1 << index,
                '1' => {
                    mask |= 1 << index;
                    value |= 1 << index;
                }
                other => panic!("unknown bit symbol {other}"),
            }
        }
        Fact::from_premise(width, mask, value).expect("a pattern is a fact")
    }

    #[test]
    fn the_shift_rows_move_the_bits_the_table_says() {
        assert_eq!(bits("1?01").shl(Fact::constant(8, 2)), bits("0100"));
        assert_eq!(bits("1?01").shl(Fact::constant(8, 0)), bits("1?01"));
        assert_eq!(bits("1?01").shl(bits("????????")), Fact::top(4));
        assert_eq!(bits("1?01").shl(Fact::constant(8, 4)), Fact::top(4));
        assert_eq!(bits("1?01").lshr(Fact::constant(8, 2)), bits("001?"));
        assert_eq!(bits("1?01").lshr(bits("????????")), Fact::top(4));
        assert_eq!(bits("1?01").ashr(Fact::constant(8, 2)), bits("111?"));
        assert_eq!(bits("??01").ashr(Fact::constant(8, 2)), Fact::top(4));
        assert_eq!(bits("0?01").ashr(Fact::constant(8, 2)), bits("000?"));
    }

    #[test]
    fn the_width_rows_extend_and_truncate_as_the_table_says() {
        assert_eq!(bits("1?0").zext(6), bits("0001?0"));
        assert_eq!(bits("???").zext(6), bits("000???"));
        assert_eq!(bits("1?0").sext(6), bits("1111?0"));
        assert_eq!(bits("0?0").sext(6), bits("0000?0"));
        assert_eq!(bits("??0").sext(6), bits("?????0"));
        assert_eq!(bits("1?01").trunc(2), bits("01"));
        assert_eq!(bits("1?01").trunc(3), bits("?01"));
    }

    /// The rows the checker still does not have, so that a premise about one of
    /// them cannot be confirmed by anything but top.
    #[test]
    fn division_and_remainder_have_no_row() {
        let surface = include_str!("facts.rs");
        for name in ["sdiv", "udiv", "srem", "urem"] {
            assert!(
                !surface.contains(&format!("fn {name}(")),
                "{name} is not a Section 8.10 transfer and must not be one here"
            );
        }
        // The comparison row is the one whose procedure the table spells out,
        // and it is still here.
        assert_eq!(
            Fact::constant(4, 3).compare(Fact::constant(4, 5), Comparison::ULt),
            Fact::constant(1, 1)
        );
    }
}
