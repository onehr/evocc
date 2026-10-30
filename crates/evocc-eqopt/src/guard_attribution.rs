//! Producer-only size attribution. Nothing here produces a certificate,
//! edits EvoIR, extends P0 eligibility, or authorizes installation.
use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{inst_results, inst_uses, Function, Inst};
use evocc_types::EvoIRType;

use crate::eligibility::Admission;
use crate::known_bits::KnownBits;
use crate::scalar_eq::{known_of, record_known, KnownEnv};
use crate::seed_facts::SeedFacts;

#[derive(Debug, Default)]
pub struct GuardSizeAttribution {
    pub true_predicates: BTreeSet<u32>,
    pub total_predicates: usize,
    pub total_assumptions: usize,
    pub removable_assumptions: usize,
    pub predicates_by_kind: BTreeMap<&'static str, usize>,
    pub true_by_kind: BTreeMap<&'static str, usize>,
}

/// Derive only the existing value-intrinsic transfers and all-edge seeds.
/// Predicate truth is an unverified diagnostic claim, not a new seed fact.
#[must_use]
pub fn guard_size_attribution(function: &Function) -> GuardSizeAttribution {
    let admission = Admission::of(function);
    let seeds = SeedFacts::of(function, &admission);
    let mut report = GuardSizeAttribution::default();
    for block in &function.blocks {
        let mut known = seeds.arguments_of(block);
        for inst in &block.insts {
            if let Some(kind) = predicate_kind(inst) {
                report.total_predicates += 1;
                *report.predicates_by_kind.entry(kind).or_default() += 1;
                if !inst_uses(inst).iter().any(|v| admission.is_tainted(v.id))
                    && true_for_facts(inst, &known)
                {
                    report.true_predicates.insert(inst_results(inst)[0].id);
                    *report.true_by_kind.entry(kind).or_default() += 1;
                }
            }
            record_known(inst, &mut known);
        }
    }
    for inst in function.blocks.iter().flat_map(|b| &b.insts) {
        if let Inst::UbAssume(value, _) = inst {
            report.total_assumptions += 1;
            report.removable_assumptions += usize::from(report.true_predicates.contains(&value.id));
        }
    }
    report
}

fn predicate_kind(inst: &Inst) -> Option<&'static str> {
    Some(match inst {
        Inst::DivisorNonZero(..) => "divisor_nonzero",
        Inst::SignedDivRemNoOverflow(..) => "signed_div_rem_no_overflow",
        Inst::SignedAddDefined(..) => "signed_add_defined",
        Inst::SignedSubDefined(..) => "signed_sub_defined",
        Inst::SignedMulDefined(..) => "signed_mul_defined",
        Inst::ShiftCountDefined { .. } => "shift_count_defined",
        Inst::SignedShlDefined { .. } => "legacy_signed_shl_defined",
        _ => return None,
    })
}

fn width_mask(width: u16) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

fn excludes(fact: KnownBits, value: u64) -> bool {
    (fact.value() ^ value) & fact.mask() != 0
}

fn signed_range(fact: KnownBits) -> (i128, i128) {
    let width = fact.width();
    let sign = 1_u64 << (width - 1);
    let unknown = width_mask(width) & !fact.mask();
    let decode = |raw: u64| i128::from(raw) - if raw & sign == 0 { 0 } else { 1_i128 << width };
    if unknown & sign != 0 {
        (
            decode(fact.value() | sign),
            decode(fact.value() | (unknown & !sign)),
        )
    } else {
        (decode(fact.value()), decode(fact.value() | unknown))
    }
}

fn true_for_facts(inst: &Inst, known: &KnownEnv) -> bool {
    match inst {
        Inst::DivisorNonZero(_, divisor) => known_of(divisor, known).is_some_and(|f| f.ones() != 0),
        Inst::SignedDivRemNoOverflow(_, lhs, rhs) => {
            let (Some(a), Some(b)) = (known_of(lhs, known), known_of(rhs, known)) else {
                return false;
            };
            a.width() == b.width()
                && (excludes(a, 1_u64 << (a.width() - 1)) || excludes(b, width_mask(b.width())))
        }
        Inst::ShiftCountDefined {
            shifted,
            amount,
            amount_signed,
            ..
        } => {
            let EvoIRType::Bits(width @ 1..=64) = shifted.ty else {
                return false;
            };
            let Some(k) = known_of(amount, known) else {
                return false;
            };
            let maximum = k.value() | (width_mask(k.width()) & !k.mask());
            maximum < u64::from(width)
                && (!amount_signed || k.zeros() & (1_u64 << (k.width() - 1)) != 0)
        }
        Inst::SignedAddDefined(_, lhs, rhs)
        | Inst::SignedSubDefined(_, lhs, rhs)
        | Inst::SignedMulDefined(_, lhs, rhs) => {
            let (Some(a), Some(b)) = (known_of(lhs, known), known_of(rhs, known)) else {
                return false;
            };
            if a.width() != b.width() {
                return false;
            }
            let ((lo_a, hi_a), (lo_b, hi_b)) = (signed_range(a), signed_range(b));
            let (lo, hi) = match inst {
                Inst::SignedAddDefined(..) => (lo_a + lo_b, hi_a + hi_b),
                Inst::SignedSubDefined(..) => (lo_a - hi_b, hi_a - lo_b),
                _ => {
                    let corners = [lo_a * lo_b, lo_a * hi_b, hi_a * lo_b, hi_a * hi_b];
                    (
                        *corners.iter().min().unwrap(),
                        *corners.iter().max().unwrap(),
                    )
                }
            };
            let extent = 1_i128 << (a.width() - 1);
            lo >= -extent && hi < extent
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_evoir::Value;

    fn value(id: u32, width: u16) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(width),
        }
    }

    #[test]
    fn independent_shift_width_and_signedness_do_not_use_machine_masking() {
        let shifted = value(0, 64);
        let amount = value(1, 2);
        let predicate = |signed| Inst::ShiftCountDefined {
            result: value(2, 1),
            shifted: shifted.clone(),
            amount: amount.clone(),
            amount_signed: signed,
        };
        let env = KnownEnv::new();
        assert!(true_for_facts(&predicate(false), &env));
        assert!(!true_for_facts(&predicate(true), &env));
        let env = KnownEnv::from([(1, KnownBits::constant(2, 3))]);
        assert!(!true_for_facts(&predicate(true), &env));
        let too_large = Inst::ShiftCountDefined {
            result: value(2, 1),
            shifted: value(0, 32),
            amount: value(1, 64),
            amount_signed: false,
        };
        let env = KnownEnv::from([(1, KnownBits::constant(64, 64))]);
        assert!(!true_for_facts(&too_large, &env));
    }

    #[test]
    fn every_small_fact_cube_claim_is_true_for_all_of_its_concrete_values() {
        for width in 1..=4 {
            let extent = 1_i128 << (width - 1);
            let limit = 1_u64 << width;
            let signed = |x: u64| {
                i128::from(x)
                    - if x & (limit / 2) == 0 {
                        0
                    } else {
                        1_i128 << width
                    }
            };
            let facts: Vec<_> = (0..limit)
                .flat_map(|mask| {
                    (0..limit)
                        .filter(move |v| v & !mask == 0)
                        .map(move |v| KnownBits::new(width, mask, v))
                })
                .collect();
            for a in &facts {
                for b in &facts {
                    let (r, x, y) = (value(2, 1), value(0, width), value(1, width));
                    let predicates = [
                        Inst::SignedAddDefined(r.clone(), x.clone(), y.clone()),
                        Inst::SignedSubDefined(r.clone(), x.clone(), y.clone()),
                        Inst::SignedMulDefined(r.clone(), x.clone(), y.clone()),
                        Inst::SignedDivRemNoOverflow(r.clone(), x, y.clone()),
                        Inst::DivisorNonZero(r, y),
                    ];
                    let known = KnownEnv::from([(0, *a), (1, *b)]);
                    for (kind, p) in predicates.iter().enumerate() {
                        if !true_for_facts(p, &known) {
                            continue;
                        }
                        for x in (0..limit).filter(|v| v & a.mask() == a.value()) {
                            for y in (0..limit).filter(|v| v & b.mask() == b.value()) {
                                let (sx, sy) = (signed(x), signed(y));
                                let truth = match kind {
                                    0 => (-extent..extent).contains(&(sx + sy)),
                                    1 => (-extent..extent).contains(&(sx - sy)),
                                    2 => (-extent..extent).contains(&(sx * sy)),
                                    3 => !(sx == -extent && sy == -1),
                                    _ => y != 0,
                                };
                                assert!(
                                    truth,
                                    "width={width} a={a:?} b={b:?} kind={kind} x={x} y={y}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
