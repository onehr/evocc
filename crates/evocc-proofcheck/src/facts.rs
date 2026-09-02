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
