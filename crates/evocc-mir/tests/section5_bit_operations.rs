//! EvoMIR Section 5's bit-counting and byte-swap row (revision 0.14).
//!
//! "`ctlz`: zero-extend per Section 3, `clz` at the register width, then
//! subtract `32 - N` or `64 - N` for a narrower `N`; `cttz`: `rbit` then
//! `clz`, and for a narrower `N` the result capped at `N` with `cmp`/`csel`
//! ...; `ctpop`: the general-register sequence both oracles use without SIMD
//! ...; `bswap`: `rev16` (16, upper bits unspecified per Section 3), `rev` at
//! 32 or 64."
//!
//! The reference values are EvoIR 3.4's, measured under both sealed oracles in
//! `.evidence/builtin-probes/20260902-2c23684c/p39-bits.c` and `p39.out`.

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{compile_module_for, lane, lower_module, MirLimits, Target};
use evocc_types::EvoIRType;

/// One function of one bit operation at one width.
fn unit(op: &str, width: u16) -> String {
    format!(
        "\
func @main(%arg0: bits<{width}>, %arg1: mem<all>, %arg2: world) -> (bits<{width}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<{width}>, %v1: mem<all>, %v2: world):
    %v3 = {op}.bits<{width}> %v0
    ret %v3, %v1, %v2
}}
"
    )
}

fn tokens() -> Vec<RuntimeValue> {
    vec![
        RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

/// The value the EvoMIR interpreter answers, checked against the EvoIR one.
fn both(text: &str, width: u16, argument: u64) -> u64 {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let mut arguments = vec![RuntimeValue::bits(width, argument)];
    arguments.extend(tokens());
    let result_types = vec![EvoIRType::Bits(width)];

    let evoir = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect("the EvoIR execution is defined");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    let mir = lane::observe_entry(
        &lowered,
        "main",
        &arguments,
        &result_types,
        MirLimits::new(1_000_000),
    )
    .expect("the EvoMIR execution is defined");

    let expected: Vec<RuntimeValue> = evoir
        .return_values
        .iter()
        .filter(|value| !matches!(value.ty, EvoIRType::Mem(_) | EvoIRType::World))
        .cloned()
        .collect();
    assert_eq!(mir.returns, expected, "the two interpreters agree");
    mir.returns[0].raw_bits
}

#[test]
fn the_measured_reference_values_are_reproduced() {
    // EvoIR 3.4, both oracles: clz(1u) = 31, ctz(8u) = 3, clzll(1ull) = 63,
    // popcountll(~0ull) = 64, bswap32(0x11223344) = 0x44332211,
    // bswap16(0x1234) = 0x3412.
    assert_eq!(both(&unit("ctlz", 32), 32, 1), 31);
    assert_eq!(both(&unit("cttz", 32), 32, 8), 3);
    assert_eq!(both(&unit("ctlz", 64), 64, 1), 63);
    assert_eq!(both(&unit("ctpop", 64), 64, u64::MAX), 64);
    assert_eq!(both(&unit("bswap", 32), 32, 0x1122_3344), 0x4433_2211);
    assert_eq!(both(&unit("bswap", 16), 16, 0x1234), 0x3412);
    assert_eq!(
        both(&unit("bswap", 64), 64, 0x0102_0304_0506_0708),
        0x0807_0605_0403_0201
    );
}

#[test]
fn each_operation_is_total_at_zero() {
    // "each is total, answers `N` (or `0` for `ctpop`) for `%a = 0`".
    assert_eq!(both(&unit("ctlz", 32), 32, 0), 32);
    assert_eq!(both(&unit("cttz", 32), 32, 0), 32);
    assert_eq!(both(&unit("ctlz", 64), 64, 0), 64);
    assert_eq!(both(&unit("cttz", 64), 64, 0), 64);
    assert_eq!(both(&unit("ctpop", 32), 32, 0), 0);
}

#[test]
fn a_narrow_width_is_capped_and_corrected() {
    // Section 5 subtracts `32 - N` from a narrow `ctlz` and caps a narrow
    // `cttz` at `N`.
    assert_eq!(both(&unit("ctlz", 8), 8, 1), 7);
    assert_eq!(both(&unit("ctlz", 8), 8, 0), 8);
    assert_eq!(both(&unit("cttz", 8), 8, 0), 8);
    assert_eq!(both(&unit("cttz", 8), 8, 0x80), 7);
    assert_eq!(both(&unit("ctpop", 8), 8, 0xff), 8);
}

fn emitted(op: &str, width: u16) -> String {
    let text = unit(op, width);
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    compile_module_for(Target::Linux, &module, &|_| true).expect("the unit compiles")
}

#[test]
fn the_emitted_sequences_are_the_ones_the_oracles_use() {
    assert!(emitted("ctlz", 32).contains("clz w"));
    assert!(emitted("ctlz", 64).contains("clz x"));

    let trailing = emitted("cttz", 64);
    assert!(trailing.contains("rbit x"), "{trailing}");
    assert!(trailing.contains("clz x"), "{trailing}");

    // "the general-register sequence both oracles use without SIMD (`p39.out`,
    // Clang-19; GCC-14 calls `__popcountdi2`, which the kernel does not
    // provide)".
    let population = emitted("ctpop", 64);
    for absent in ["cnt ", "fmov", "addv", "    bl "] {
        assert!(
            !population.contains(absent),
            "the population count uses general registers only: {population}"
        );
    }
    assert!(population.contains("mul x"), "{population}");

    assert!(emitted("bswap", 16).contains("rev16 w"));
    assert!(emitted("bswap", 32).contains("rev w"));
    assert!(emitted("bswap", 64).contains("rev x"));
}
