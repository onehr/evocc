//! The Section 8.10 lattice, one test per sentence.
//!
//! The transfers are deliberately weak, so these tests pin both directions:
//! what each row must know, and what it must *not* claim to know. A transfer
//! that learned more than its row would make the producer state a premise a
//! checker recomputing the same table could not confirm.

use evocc_eqopt::known_bits::{Comparison, KnownBits};

/// `mask`/`value` written as bit strings, most significant first, with `?` for
/// an unknown bit. Reading a fact off the page is what makes these tests about
/// the table rather than about arithmetic.
fn bits(pattern: &str) -> KnownBits {
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
    KnownBits::new(width, mask, value)
}

#[test]
fn a_constant_is_the_bottom_of_the_lattice() {
    let five = KnownBits::constant(8, 5);
    assert!(five.is_constant());
    assert_eq!(five.ones(), 5);
    assert_eq!(five.zeros(), 0xfa);
    assert!(five.is_at_or_below(KnownBits::top(8)));
    assert!(!KnownBits::top(8).is_at_or_below(five));
}

#[test]
fn order_is_by_what_is_known() {
    let known = bits("1010");
    let partial = bits("10??");
    let contradicting = bits("11??");
    assert!(known.is_at_or_below(partial), "a constant refines a prefix");
    assert!(!partial.is_at_or_below(known));
    assert!(
        !known.is_at_or_below(contradicting),
        "a differing known bit"
    );
}

#[test]
fn a_union_that_disagrees_on_a_known_bit_is_invalid() {
    assert_eq!(bits("1?").meet(bits("?0")), Some(bits("10")));
    assert_eq!(
        bits("10").meet(bits("11")),
        None,
        "two values with a differing known bit cannot be equal"
    );
    assert_eq!(
        bits("????").meet(bits("1010")),
        Some(bits("1010")),
        "meeting with top learns everything the other side knows"
    );
}

#[test]
fn and_is_known_where_either_is_zero_or_both_are_one() {
    assert_eq!(bits("1?").transfer_and(bits("?0")), bits("?0"));
    assert_eq!(bits("11").transfer_and(bits("11")), bits("11"));
    assert_eq!(bits("0?").transfer_and(bits("??")), bits("0?"));
    assert_eq!(
        bits("1?").transfer_and(bits("1?")),
        bits("1?"),
        "both known one gives a known one"
    );
    assert_eq!(
        bits("??").transfer_and(bits("??")),
        KnownBits::top(2),
        "nothing known on either side learns nothing"
    );
}

#[test]
fn or_is_known_where_either_is_one_or_both_are_zero() {
    assert_eq!(bits("1?").transfer_or(bits("??")), bits("1?"));
    assert_eq!(bits("0?").transfer_or(bits("0?")), bits("0?"));
    assert_eq!(
        bits("0?").transfer_or(bits("??")),
        KnownBits::top(2),
        "one known zero is not enough"
    );
}

#[test]
fn xor_is_known_only_where_both_are_known() {
    assert_eq!(bits("10").transfer_xor(bits("11")), bits("01"));
    assert_eq!(
        bits("1?").transfer_xor(bits("11")),
        bits("0?"),
        "an unknown bit on either side stays unknown"
    );
    assert_eq!(bits("??").transfer_xor(bits("11")), KnownBits::top(2));
}

#[test]
fn add_and_sub_keep_the_trailing_run_known_in_both() {
    // 0b0011 + 0b0001 = 0b0100, and the high bits are unknown in one input, so
    // only the trailing run both inputs know survives.
    assert_eq!(bits("??11").transfer_add(bits("0001")), bits("??00"));
    assert_eq!(bits("0011").transfer_add(bits("0001")), bits("0100"));
    assert_eq!(
        bits("???1").transfer_add(bits("???1")),
        bits("???0"),
        "one known trailing bit each gives one known trailing bit"
    );
    assert_eq!(bits("0011").transfer_sub(bits("0001")), bits("0010"));
    assert_eq!(bits("??11").transfer_sub(bits("0001")), bits("??10"));
    assert_eq!(
        bits("???1").transfer_sub(bits("??1?")),
        KnownBits::top(4),
        "an unknown bit zero on one side stops the run at once"
    );
}

#[test]
fn mul_knows_only_the_trailing_zero_run() {
    // Two trailing known zeros times five gives seven: the runs add.
    assert_eq!(
        bits("??????00").transfer_mul(bits("???00000")),
        bits("?0000000")
    );
    assert_eq!(
        bits("00000011").transfer_mul(bits("00000011")),
        bits("????????"),
        "the table gives nothing above the trailing zero run, even for constants"
    );
    assert_eq!(
        bits("????????").transfer_mul(bits("????????")),
        KnownBits::top(8)
    );
}

#[test]
fn icmp_is_known_only_when_the_bits_decide_it() {
    assert_eq!(
        bits("1?").transfer_icmp(bits("0?"), Comparison::Eq),
        KnownBits::constant(1, 0),
        "a differing known bit decides inequality"
    );
    assert_eq!(
        bits("1?").transfer_icmp(bits("0?"), Comparison::Ne),
        KnownBits::constant(1, 1)
    );
    assert_eq!(
        bits("1010").transfer_icmp(bits("1010"), Comparison::Eq),
        KnownBits::constant(1, 1),
        "two constants decide equality"
    );
    assert_eq!(
        bits("1?").transfer_icmp(bits("1?"), Comparison::Eq),
        KnownBits::top(1),
        "agreeing on the known bits is not agreeing"
    );
    assert_eq!(
        bits("00??").transfer_icmp(bits("01??"), Comparison::ULt),
        KnownBits::constant(1, 1),
        "the intervals are disjoint"
    );
    assert_eq!(
        bits("0???").transfer_icmp(bits("0???"), Comparison::ULt),
        KnownBits::top(1),
        "overlapping intervals decide nothing"
    );
    assert_eq!(
        bits("1???").transfer_icmp(bits("0???"), Comparison::SLt),
        KnownBits::constant(1, 1),
        "a known sign bit orders the two"
    );
    assert_eq!(
        bits("????").transfer_icmp(bits("0000"), Comparison::SLt),
        KnownBits::top(1),
        "an unknown sign bit is not an interval in signed order"
    );
}

#[test]
fn a_transfer_that_is_not_listed_is_top() {
    // Section 8.10: "anything not listed transfers to top". The lattice offers
    // no shift, division, extension or truncation transfer, so a caller that
    // wanted one has nothing to call and the fact stays top by construction.
    // This test records that as a property of the module's surface, which is
    // what keeps a later contributor from quietly adding a stronger row.
    let listed = [
        "constant",
        "transfer_and",
        "transfer_or",
        "transfer_xor",
        "transfer_add",
        "transfer_sub",
        "transfer_mul",
        "transfer_icmp",
        "meet",
        "top",
    ];
    let surface = include_str!("../src/known_bits.rs");
    for name in [
        "shl", "lshr", "ashr", "sdiv", "udiv", "srem", "urem", "sext", "zext", "trunc",
    ] {
        assert!(
            !surface.contains(&format!("pub fn transfer_{name}(")),
            "{name} is not a Section 8.10 transfer and must not be one here"
        );
    }
    for name in listed {
        assert!(
            surface.contains(name),
            "the module should still implement the listed transfer {name}"
        );
    }
}
