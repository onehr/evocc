//! A shift amount is decoded at its own width (Section 3.4).
//!
//! "`%b` is the shift amount of independently promoted type `bits<M>` for
//! shifts", "`%b` is decoded at its own `M`-bit width", and "for an amount at
//! least `N`, `shl` and `lshr` produce zero; `ashr` produces all zeroes or all
//! ones according to the sign bit".
//!
//! The sentence has two halves that are easy to conflate, and the named vectors
//! of the correspondence plan separate them: the amount is *not* truncated to
//! `N` bits before the comparison, so `256 : bits<16>` is two hundred and
//! fifty-six and not zero, and the comparison is a natural-number one, so an
//! amount of `2^64` in a `bits<128>` operand is out of range for every `N` this
//! profile admits rather than wrapping to zero. An implementation that reduced
//! the amount modulo `N`, or that decoded it into 64 bits before comparing,
//! would pass a test written only over small amounts.

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_types::{EvoIRType, Region};

/// One shift instruction, alone in a module, over two entry arguments.
///
/// The amount is an entry argument rather than a `const` because `const` in the
/// textual form carries at most sixty-four bits, and the vectors this test
/// names reach `2^64`.
fn shift(operation: &str, width: u16, amount_width: u16, value: u128, amount: u128) -> u128 {
    let text = format!(
        "func @main(%arg0: bits<{width}>, %arg1: bits<{amount_width}>, %arg2: mem<all>, \
%arg3: world) -> (bits<{width}>, mem<all>, world) cc = target_default\n    \
effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, \
may_trap=false, may_unwind=false }}\n{{\n\
^entry(%v0: bits<{width}>, %v1: bits<{amount_width}>, %v2: mem<all>, %v3: world):\n    \
%v4 = {operation}.bits<{width}> %v0, %v1\n    \
ret %v4, %v2, %v3\n}}\n"
    );
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let outcome = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::wide(EvoIRType::Bits(width), value),
            RuntimeValue::wide(EvoIRType::Bits(amount_width), amount),
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(1_000),
    )
    .expect("one shift executes");
    outcome.return_values[0].value()
}

/// `shl(1 : bits<8>, 256 : bits<16>) = 0`.
///
/// The amount is decoded at sixteen bits, so it is two hundred and fifty-six
/// and not the zero its low eight bits spell.
#[test]
fn a_sixteen_bit_amount_is_not_truncated_to_the_shifted_width() {
    assert_eq!(shift("shl", 8, 16, 1, 256), 0);
}

/// `shl(1 : bits<8>, 0 : bits<128>) = 1` and `shl(1 : bits<8>, 1 : bits<128>) = 2`.
///
/// An amount wider than the value is still an ordinary amount when it is in
/// range; the width of the operand decides nothing but the decoding.
#[test]
fn an_in_range_amount_shifts_whatever_its_own_width() {
    assert_eq!(shift("shl", 8, 128, 1, 0), 1);
    assert_eq!(shift("shl", 8, 128, 1, 1), 2);
}

/// `shl(1 : bits<8>, 2^64 : bits<128>) = 0`.
///
/// The comparison against `N` is over natural numbers. An implementation that
/// decoded the amount into sixty-four bits first would read zero here and
/// answer one.
#[test]
fn an_amount_past_sixty_four_bits_is_out_of_range_rather_than_wrapped() {
    assert_eq!(shift("shl", 8, 128, 1, 1_u128 << 64), 0);
    assert_eq!(shift("lshr", 8, 128, 1, 1_u128 << 64), 0);
}

/// `ashr(128 : bits<8>, 2^64 : bits<128>) = 255`.
///
/// Out of range, `ashr` fills with the sign bit rather than with zero, and
/// `0x80` as a `bits<8>` is negative.
#[test]
fn an_out_of_range_arithmetic_shift_fills_with_the_sign_bit() {
    assert_eq!(shift("ashr", 8, 128, 0x80, 1_u128 << 64), 0xff);
    assert_eq!(shift("ashr", 8, 128, 0x7f, 1_u128 << 64), 0);
}

/// The boundary amounts the correspondence plan asks for: `N - 1`, `N`,
/// `N + 1` and `2N`, at a shift-amount width narrower than the value.
#[test]
fn the_boundary_amounts_land_on_the_width_and_not_beside_it() {
    assert_eq!(shift("shl", 8, 4, 1, 7), 0x80);
    assert_eq!(shift("shl", 8, 4, 1, 8), 0);
    assert_eq!(shift("shl", 8, 4, 1, 9), 0);
    assert_eq!(shift("lshr", 8, 4, 0x80, 7), 1);
    assert_eq!(shift("lshr", 8, 4, 0x80, 8), 0);
    assert_eq!(shift("ashr", 8, 4, 0x80, 7), 0xff);
    assert_eq!(shift("ashr", 8, 4, 0x80, 8), 0xff);
}

/// `M = 1`: a one-bit amount is zero or one and never out of range for a width
/// above one.
#[test]
fn a_one_bit_amount_is_still_an_amount() {
    assert_eq!(shift("shl", 8, 1, 1, 0), 1);
    assert_eq!(shift("shl", 8, 1, 1, 1), 2);
    assert_eq!(shift("shl", 1, 1, 1, 1), 0);
}
