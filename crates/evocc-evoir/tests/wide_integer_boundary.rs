//! EvoIR 10.1.19 (revision 1.9.61): `bits<128>` executes in every scalar form
//! but division and remainder.
//!
//! "A `bits<128>` value lowers, is interpreted and is optimized exactly as any
//! other `bits<N>`: `load`/`store` at 16 bytes and alignment 16, `copy` of
//! records holding one, `zext`/`sext` from and `trunc` to a narrower width,
//! `add`, `sub`, `mul`, `and`, `or`, `xor`, `shl`, `lshr`, `ashr`, every
//! `icmp`, the UB predicates, block arguments, parameters, results and
//! anonymous arguments. Both interpreters extend their scalar value model to
//! 128 bits for these forms."
//!
//! Every expected value below is one the specification states and the sealed
//! oracles measured (`p35-run.c` in
//! `.evidence/builtin-probes/20260902-2c23684c/`, and the forms
//! `.evidence/builtin-probes/20260905-b6/b6-probes.out` adds).

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_types::EvoIRType;

/// One function over two `bits<64>` inputs returning one `bits<64>`.
fn wrap(body: &str) -> String {
    format!(
        "func @main(%arg0: bits<64>, %arg1: bits<64>, %arg2: mem<all>, %arg3: world) \
-> (bits<64>, mem<all>, world) cc = target_default\n    \
effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, \
may_trap=false, may_unwind=false }}\n{{\n\
^entry(%v0: bits<64>, %v1: bits<64>, %v2: mem<all>, %v3: world):\n{body}    \
ret %v99, %v2, %v3\n}}\n"
    )
}

/// Run one such function and answer its single `bits<64>` result.
fn run(body: &str, left: u64, right: u64) -> u64 {
    let text = wrap(body);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let outcome = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::bits(64, left),
            RuntimeValue::bits(64, right),
            RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(1000),
    )
    .expect("bits<128> executes at revision 1.9.61");
    outcome.return_values[0].raw_bits
}

/// `(u64)(((unsigned __int128)a * b) >> s)`: the kernel cohort's whole use of
/// the width, and the specification's first two reference values.
fn mul_shr(shift: u64) -> String {
    format!(
        "    %v4 = zext.bits<128> %v0\n\
         \x20   %v5 = zext.bits<128> %v1\n\
         \x20   %v6 = mul.bits<128> %v4, %v5\n\
         \x20   %v7 = const.bits<128> {shift}\n\
         \x20   %v8 = lshr.bits<128> %v6, %v7\n\
         \x20   %v99 = trunc.bits<64> %v8\n"
    )
}

#[test]
fn the_sixty_four_by_sixty_four_product_read_as_two_halves_executes() {
    // "((u128)0xffffffffffffffff * 0xfffffffffffffffe) >> 64 is
    // 0xfffffffffffffffd".
    assert_eq!(
        run(&mul_shr(64), 0xffff_ffff_ffff_ffff, 0xffff_ffff_ffff_fffe),
        0xffff_ffff_ffff_fffd
    );
    // "((u128)0x123456789abcdef0 * 0xfedcba9876543210) >> 37 is
    // 0x56bbeba1111b6c47".
    assert_eq!(
        run(&mul_shr(37), 0x1234_5678_9abc_def0, 0xfedc_ba98_7654_3210),
        0x56bb_eba1_111b_6c47
    );
    // A shift above the register width: "(u * 3) >> 100 is 3" below, and the
    // whole product shifted out here.
    assert_eq!(
        run(&mul_shr(100), 0xffff_ffff_ffff_ffff, 0xffff_ffff_ffff_ffff),
        ((u128::from(u64::MAX) * u128::from(u64::MAX)) >> 100) as u64
    );
}

#[test]
fn shift_and_add_carry_across_the_halves() {
    // "(1 << 100) has high half 0x1000000000".
    let high_of_one_shifted = "    %v4 = const.bits<128> 1\n\
                               \x20   %v5 = const.bits<128> 100\n\
                               \x20   %v6 = shl.bits<128> %v4, %v5\n\
                               \x20   %v7 = const.bits<128> 64\n\
                               \x20   %v8 = lshr.bits<128> %v6, %v7\n\
                               \x20   %v99 = trunc.bits<64> %v8\n";
    assert_eq!(run(high_of_one_shifted, 0, 0), 0x10_0000_0000);

    // "(((u128)1 << 64) | ~0ul) + 1 is 2:0": the carry leaves the low half.
    let carry = "    %v4 = const.bits<128> 1\n\
                 \x20   %v5 = const.bits<128> 64\n\
                 \x20   %v6 = shl.bits<128> %v4, %v5\n\
                 \x20   %v7 = const.bits<128> 18446744073709551615\n\
                 \x20   %v8 = or.bits<128> %v6, %v7\n\
                 \x20   %v9 = add.bits<128> %v8, %v4\n\
                 \x20   %v10 = lshr.bits<128> %v9, %v5\n\
                 \x20   %v99 = trunc.bits<64> %v10\n";
    assert_eq!(run(carry, 0, 0), 2, "the high half of the sum");

    let carry_low = "    %v4 = const.bits<128> 1\n\
                     \x20   %v5 = const.bits<128> 64\n\
                     \x20   %v6 = shl.bits<128> %v4, %v5\n\
                     \x20   %v7 = const.bits<128> 18446744073709551615\n\
                     \x20   %v8 = or.bits<128> %v6, %v7\n\
                     \x20   %v9 = add.bits<128> %v8, %v4\n\
                     \x20   %v99 = trunc.bits<64> %v9\n";
    assert_eq!(run(carry_low, 0, 0), 0, "the low half of the sum");

    // "((__uint128_t)1 << 100) + 7 shifted right by 64 is 68719476736, and
    // (u * 3) >> 100 is 3".
    let earlier = "    %v4 = const.bits<128> 1\n\
                   \x20   %v5 = const.bits<128> 100\n\
                   \x20   %v6 = shl.bits<128> %v4, %v5\n\
                   \x20   %v7 = const.bits<128> 7\n\
                   \x20   %v8 = add.bits<128> %v6, %v7\n\
                   \x20   %v9 = const.bits<128> 3\n\
                   \x20   %v10 = mul.bits<128> %v8, %v9\n\
                   \x20   %v11 = lshr.bits<128> %v10, %v5\n\
                   \x20   %v99 = trunc.bits<64> %v11\n";
    assert_eq!(run(earlier, 0, 0), 3);
}

#[test]
fn subtraction_borrows_and_the_signed_right_shift_fills_from_the_high_half() {
    // 0 - 1 at 128 bits is all ones in both halves.
    let borrow = "    %v4 = const.bits<128> 0\n\
                  \x20   %v5 = const.bits<128> 1\n\
                  \x20   %v6 = sub.bits<128> %v4, %v5\n\
                  \x20   %v7 = const.bits<128> 64\n\
                  \x20   %v8 = lshr.bits<128> %v6, %v7\n\
                  \x20   %v99 = trunc.bits<64> %v8\n";
    assert_eq!(run(borrow, 0, 0), u64::MAX);

    // `sext` to 128 fills the high half from the sign, and `ashr` by 100
    // keeps it: -1 stays -1.
    let sign = "    %v4 = sext.bits<128> %v0\n\
                \x20   %v5 = const.bits<128> 100\n\
                \x20   %v6 = ashr.bits<128> %v4, %v5\n\
                \x20   %v99 = trunc.bits<64> %v6\n";
    assert_eq!(run(sign, u64::MAX, 0), u64::MAX);
    // A positive value shifted past its own magnitude answers zero.
    assert_eq!(run(sign, 1, 0), 0);
}

#[test]
fn every_comparison_answers_at_the_full_width() {
    // "(1 << 64) < (~(u128)0 >> 64) is 0 and 1 < (1 << 64) is 1."
    let ult_high = "    %v4 = const.bits<128> 1\n\
                    \x20   %v5 = const.bits<128> 64\n\
                    \x20   %v6 = shl.bits<128> %v4, %v5\n\
                    \x20   %v7 = const.bits<128> 0\n\
                    \x20   %v8 = sub.bits<128> %v7, %v4\n\
                    \x20   %v9 = lshr.bits<128> %v8, %v5\n\
                    \x20   %v10 = icmp.ult %v6, %v9\n\
                    \x20   %v99 = zext.bits<64> %v10\n";
    assert_eq!(run(ult_high, 0, 0), 0);

    let ult_low = "    %v4 = const.bits<128> 1\n\
                   \x20   %v5 = const.bits<128> 64\n\
                   \x20   %v6 = shl.bits<128> %v4, %v5\n\
                   \x20   %v10 = icmp.ult %v4, %v6\n\
                   \x20   %v99 = zext.bits<64> %v10\n";
    assert_eq!(run(ult_low, 0, 0), 1);

    // eq and ne read every bit: two values agreeing in the low half only.
    let eq = "    %v4 = const.bits<128> 1\n\
              \x20   %v5 = const.bits<128> 64\n\
              \x20   %v6 = shl.bits<128> %v4, %v5\n\
              \x20   %v7 = or.bits<128> %v6, %v4\n\
              \x20   %v10 = icmp.eq %v7, %v4\n\
              \x20   %v99 = zext.bits<64> %v10\n";
    assert_eq!(run(eq, 0, 0), 0);

    // slt: -1 as a 128-bit signed value is below 1.
    let slt = "    %v4 = const.bits<128> 0\n\
               \x20   %v5 = const.bits<128> 1\n\
               \x20   %v6 = sub.bits<128> %v4, %v5\n\
               \x20   %v10 = icmp.slt %v6, %v5\n\
               \x20   %v99 = zext.bits<64> %v10\n";
    assert_eq!(run(slt, 0, 0), 1);

    // and the same pair is not unsigned-below, which is what makes the
    // signed comparison a distinct row.
    let ult_signed_pair = "    %v4 = const.bits<128> 0\n\
                           \x20   %v5 = const.bits<128> 1\n\
                           \x20   %v6 = sub.bits<128> %v4, %v5\n\
                           \x20   %v10 = icmp.ult %v6, %v5\n\
                           \x20   %v99 = zext.bits<64> %v10\n";
    assert_eq!(run(ult_signed_pair, 0, 0), 0);
}

#[test]
fn a_value_spanning_the_whole_width_is_reachable() {
    // The widest bit pattern is built the way the front end builds one, by
    // extending and shifting: `Inst::Const` carries its immediate in the same
    // 64-bit field the C lowering fills, so a literal above 2^64 - 1 has no
    // in-memory spelling yet. Section 3.2's range for `N = 128` is therefore
    // reachable through the arithmetic and not through the literal.
    let widest = "    %v4 = const.bits<128> 0\n\
                  \x20   %v5 = const.bits<128> 1\n\
                  \x20   %v6 = sub.bits<128> %v4, %v5\n\
                  \x20   %v7 = const.bits<128> 64\n\
                  \x20   %v8 = lshr.bits<128> %v6, %v7\n\
                  \x20   %v99 = trunc.bits<64> %v8\n";
    assert_eq!(run(widest, 0, 0), u64::MAX, "the high half is all ones");

    let low = "    %v4 = const.bits<128> 0\n\
               \x20   %v5 = const.bits<128> 1\n\
               \x20   %v6 = sub.bits<128> %v4, %v5\n\
               \x20   %v99 = trunc.bits<64> %v6\n";
    assert_eq!(run(low, 0, 0), u64::MAX, "and so is the low half");

    // A 64-bit literal keeps its exact value at the wider type.
    let literal = "    %v4 = const.bits<128> 18446744073709551615\n\
                   \x20   %v99 = trunc.bits<64> %v4\n";
    assert_eq!(run(literal, 0, 0), u64::MAX);
}

#[test]
fn division_and_remainder_stay_e3033_at_width_one_hundred_and_twenty_eight() {
    // "sdiv, udiv, srem and urem on bits<128> stay E3033
    // (not-executed: wide-integer) until a unit needs one."
    for operation in ["sdiv", "udiv", "srem", "urem"] {
        let body = format!(
            "    %v4 = zext.bits<128> %v0\n\
             \x20   %v5 = zext.bits<128> %v1\n\
             \x20   %v6 = {operation}.bits<128> %v4, %v5\n\
             \x20   %v99 = trunc.bits<64> %v6\n"
        );
        // Division may trap, so its function carries the Section 4.6 summary
        // that says so.
        let text = wrap(&body).replace("may_trap=false", "may_trap=true");
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let error = interpret_module(
            &module,
            "main",
            &[
                RuntimeValue::bits(64, 6),
                RuntimeValue::bits(64, 3),
                RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            InterpreterLimits::new(1000),
        )
        .expect_err("division at width 128 is not executed");
        assert_eq!(error.code(), "E3033", "{operation}: {error:?}");
        assert_eq!(error.message(), "not-executed: wide-integer", "{operation}");
    }
}

#[test]
fn a_width_between_sixty_five_and_one_hundred_and_twenty_seven_still_fails_closed() {
    // Section 1.1 admits every positive width; execution does not. The
    // narrowing of revision 1.9.61 opens 128 and nothing between.
    let text = wrap(
        "    %v4 = zext.bits<96> %v0\n\
         \x20   %v99 = trunc.bits<64> %v4\n",
    );
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let error = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::bits(64, 1),
            RuntimeValue::bits(64, 1),
            RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        InterpreterLimits::new(1000),
    )
    .expect_err("bits<96> has no value model");
    assert_eq!(error.code(), "E3033", "{error:?}");
}
