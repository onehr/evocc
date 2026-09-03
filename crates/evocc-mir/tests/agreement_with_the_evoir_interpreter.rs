//! Section 9.1, instrument one: "an interpreter executes an EvoMIR function on
//! the same inputs as the EvoIR interpreter and the two must agree on results
//! and on the volatile trace."
//!
//! These fixtures are the hand-written half of that instrument. The randomized
//! half is the third lane of the semantic gate.

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{lane, lower_module, MirLimits};
use evocc_types::EvoIRType;

fn agree(text: &str, arguments: &[RuntimeValue]) {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let entry = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("an entry function");
    let result_types: Vec<EvoIRType> = entry
        .ret_tys
        .iter()
        .filter(|ty| !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World))
        .cloned()
        .collect();

    let evoir = interpret_module(&module, "main", arguments, InterpreterLimits::new(100_000))
        .expect("the EvoIR execution is defined");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    let mir = lane::observe_entry(
        &lowered,
        "main",
        arguments,
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
    assert_eq!(mir.returns, expected, "results agree");
    assert_eq!(
        mir.world.volatile_trace(),
        evoir.world.volatile_trace(),
        "volatile traces agree"
    );
}

fn bits(width: u16, raw: u64) -> RuntimeValue {
    RuntimeValue::bits(width, raw)
}

fn tokens() -> Vec<RuntimeValue> {
    vec![
        RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

fn with_tokens(mut values: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    values.extend(tokens());
    values
}

const ARITHMETIC: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = add.bits<32> %v0, %v1
    %v5 = mul.bits<32> %v4, %v0
    %v6 = sub.bits<32> %v5, %v1
    %v7 = xor.bits<32> %v6, %v0
    ret %v7, %v2, %v3
}
";

#[test]
fn arithmetic_agrees() {
    for (left, right) in [(0_u64, 0_u64), (7, 9), (0xffff_ffff, 3), (0x8000_0000, 1)] {
        agree(
            ARITHMETIC,
            &with_tokens(vec![bits(32, left), bits(32, right)]),
        );
    }
}

const NARROW: &str = "\
func @main(%arg0: bits<13>, %arg1: bits<13>, %arg2: mem<all>, %arg3: world) -> (bits<13>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<13>, %v1: bits<13>, %v2: mem<all>, %v3: world):
    %v8 = const.bits<13> 7
    %v9 = and.bits<13> %v0, %v8
    %v10 = and.bits<13> %v1, %v8
    %v4 = add.bits<13> %v0, %v1
    %v5 = lshr.bits<13> %v4, %v10
    %v6 = ashr.bits<13> %v5, %v9
    %v7 = mul.bits<13> %v6, %v1
    ret %v7, %v2, %v3
}
";

// Every shift amount is masked below the shifted width first, which is what
// EvoIR Section 3.4 requires of "a valid C lowering": the raw shift is total,
// so an unguarded amount at or above the width means zero there and a
// wrap-around count on the machine.
#[test]
fn narrow_widths_agree() {
    for (left, right) in [(0_u64, 0_u64), (1, 2), (0x1fff, 3), (0x1000, 12), (5, 7)] {
        agree(NARROW, &with_tokens(vec![bits(13, left), bits(13, right)]));
    }
}

const DIVISION: &str = "\
func @main(%arg0: bits<17>, %arg1: bits<17>, %arg2: mem<all>, %arg3: world) -> (bits<17>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<17>, %v1: bits<17>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<17> 3
    %v5 = or.bits<17> %v1, %v4
    %v6 = sdiv.bits<17> %v0, %v5
    %v7 = urem.bits<17> %v0, %v5
    %v8 = add.bits<17> %v6, %v7
    ret %v8, %v2, %v3
}
";

#[test]
fn division_and_remainder_agree() {
    for (left, right) in [(0_u64, 0_u64), (100, 7), (0x1ffff, 5), (0x10000, 9)] {
        agree(
            DIVISION,
            &with_tokens(vec![bits(17, left), bits(17, right)]),
        );
    }
}

const COMPARISONS: &str = "\
func @main(%arg0: bits<7>, %arg1: bits<7>, %arg2: mem<all>, %arg3: world) -> (bits<7>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<7>, %v1: bits<7>, %v2: mem<all>, %v3: world):
    %v4 = icmp.eq %v0, %v1
    %v5 = icmp.slt %v0, %v1
    %v6 = icmp.ult %v0, %v1
    %v7 = zext.bits<7> %v4
    %v8 = zext.bits<7> %v5
    %v9 = zext.bits<7> %v6
    %v10 = add.bits<7> %v7, %v8
    %v11 = add.bits<7> %v10, %v9
    ret %v11, %v2, %v3
}
";

#[test]
fn comparisons_at_a_narrow_width_agree() {
    for left in 0..8_u64 {
        for right in 0..8_u64 {
            agree(
                COMPARISONS,
                &with_tokens(vec![bits(7, left * 9), bits(7, right * 11)]),
            );
        }
    }
}

const TRUNCATED_EQUALITY: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = trunc.bits<3> %v0
    %v4 = const.bits<3> 7
    %v5 = icmp.eq %v3, %v4
    ret %v5, %v1, %v2
}
";

#[test]
fn equality_on_a_truncated_value_agrees() {
    // Section 3's table says icmp.eq and icmp.ne need no inserted extension
    // because "the low N bits of the result depend only on the low N bits of
    // the operands". That sentence does not hold of a comparison: `trunc`
    // inserts nothing, so the bits above N are the source's, and a 32-bit `cmp`
    // reads them. This input distinguishes the two readings.
    for raw in [0xffff_ffff_u64, 7, 0x0000_00f7, 0x8000_0007] {
        agree(TRUNCATED_EQUALITY, &with_tokens(vec![bits(32, raw)]));
    }
}

const MEMORY: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=8, align=4]
    %v6 = access_in_bounds %v4, %v3 [size=4, align=4]
    (%v7, %v8) = store %v6, %v5, %v3, %v0
    %v9 = const.bits<64> 1
    %v10 = ptr.offset.add.unsigned %v3, %v9 [scale=4]
    %v11 = pointer_valid %v7, %v10
    %v12 = access_in_bounds %v11, %v10 [size=4, align=4]
    (%v13, %v14) = store %v12, %v8, %v10, %v0
    %v15 = access_in_bounds %v13, %v10 [size=4, align=4]
    (%v16, %v17, %v18) = load %v15, %v14, %v10 : bits<32>
    %v19 = add.bits<32> %v16, %v0
    ret %v19, %v17, %v18
}
";

#[test]
fn allocation_offsetting_stores_and_loads_agree() {
    for raw in [0_u64, 5, 0xdead_beef] {
        agree(MEMORY, &with_tokens(vec![bits(32, raw)]));
    }
}

const VOLATILE: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=4, align=4]
    %v6 = access_in_bounds %v4, %v3 [size=4, align=4]
    (%v7, %v8) = store volatile %v6, %v5, %v3, %v0
    %v9 = access_in_bounds %v7, %v3 [size=4, align=4]
    (%v10, %v11, %v12) = load volatile %v9, %v8, %v3 : bits<32>
    ret %v10, %v11, %v12
}
";

#[test]
fn the_volatile_trace_agrees_including_the_object_origin() {
    agree(VOLATILE, &with_tokens(vec![bits(32, 0x1234_5678)]));
}

const LOOP: &str = "\
func @main(%arg0: bits<64>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: mem<all>, %v2: world):
    %v20 = const.bits<64> 0
    br ^b1(%v0, %v20, %v1, %v2)

^b1(%v4: bits<64>, %v5: bits<64>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<64> 0
    %v9 = icmp.ult %v8, %v4
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v5, %v6, %v7)

^b2(%v10: bits<64>, %v11: bits<64>, %v12: mem<all>, %v13: world):
    %v14 = const.bits<64> 1
    %v15 = sub.bits<64> %v10, %v14
    %v16 = add.bits<64> %v11, %v10
    br ^b1(%v15, %v16, %v12, %v13)

^b3(%v17: bits<64>, %v18: mem<all>, %v19: world):
    ret %v17, %v18, %v19
}
";

#[test]
fn a_loop_agrees() {
    for trips in [0_u64, 1, 5, 17] {
        agree(LOOP, &with_tokens(vec![bits(64, trips)]));
    }
}

const CALLS: &str = "\
func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<32> %v0, %v0
    ret %v3, %v1, %v2
}

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @helper(%v0, %v1, %v2)
    (%v6, %v7, %v8) = call @helper(%v3, %v4, %v5)
    ret %v6, %v7, %v8
}
";

#[test]
fn calls_agree() {
    for raw in [0_u64, 1, 0x4000_0000] {
        agree(CALLS, &with_tokens(vec![bits(32, raw)]));
    }
}

const GLOBALS: &str = "\
global @counter origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = global_addr @counter : ptr<generic>
    %v4 = access_in_bounds %v1, %v3 [size=4, align=4]
    (%v5, %v6, %v7) = load %v4, %v2, %v3 : bits<32>
    %v8 = add.bits<32> %v5, %v0
    %v9 = access_in_bounds %v6, %v3 [size=4, align=4]
    (%v10, %v11) = store %v9, %v7, %v3, %v8
    ret %v8, %v10, %v11
}
";

#[test]
fn a_global_read_and_written_agrees() {
    agree(GLOBALS, &with_tokens(vec![bits(32, 35)]));
}

const OBLIGATION: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 0
    %v4 = icmp.ne %v0, %v3
    ub.assume.division_by_zero %v4
    %v5 = udiv.bits<32> %v0, %v0
    ret %v5, %v1, %v2
}
";

#[test]
fn a_satisfied_obligation_falls_through_to_the_continuation() {
    agree(OBLIGATION, &with_tokens(vec![bits(32, 9)]));
}

#[test]
fn a_violated_obligation_traps_in_both_interpreters() {
    let module = parse_section9_module(OBLIGATION.as_bytes()).expect("canonical EvoIR");
    let arguments = with_tokens(vec![bits(32, 0)]);
    let evoir = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect_err("the obligation is violated");
    assert_eq!(evoir.code(), "E3002");

    let lowered = lower_module(&module).expect("lowerable");
    let error = lane::observe_entry(
        &lowered,
        "main",
        &arguments,
        &[EvoIRType::Bits(32)],
        MirLimits::new(1_000_000),
    )
    .expect_err("the checked profile traps");
    assert_eq!(error, evocc_mir::MirError::Trap { imm: 3002 });
}

const INDIRECT: &str = "\
func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 3
    %v4 = mul.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = func_addr @helper : fnptr<(bits<32>, mem<all>, world)->(bits<32>, mem<all>, world)>
    (%v4, %v5, %v6) = call %v3(%v0, %v1, %v2)
    ret %v4, %v5, %v6
}
";

#[test]
fn a_function_address_and_an_indirect_call_agree() {
    for raw in [0_u64, 7, 0xffff_ffff] {
        agree(INDIRECT, &with_tokens(vec![bits(32, raw)]));
    }
}

const NULL_COMPARISON: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=4, align=4]
    %v6 = ptr.null : ptr<generic>
    %v7 = icmp.eq %v3, %v6
    %v8 = icmp.ne %v3, %v6
    %v9 = xor.bits<1> %v7, %v8
    ret %v9, %v4, %v5
}
";

#[test]
fn a_null_pointer_comparison_agrees() {
    agree(NULL_COMPARISON, &with_tokens(vec![bits(32, 1)]));
}

const OBJECT_COPY: &str = "\
func @main(%arg0: bits<64>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=16, align=8]
    (%v6, %v7, %v8) = alloca %v4, %v5 [origin=1, size=16, align=8]
    %v9 = access_in_bounds %v7, %v3 [size=8, align=8]
    (%v10, %v11) = store %v9, %v8, %v3, %v0
    %v12 = const.bits<64> 1
    %v13 = ptr.offset.add.unsigned %v3, %v12 [scale=8]
    %v14 = pointer_valid %v10, %v13
    %v15 = access_in_bounds %v14, %v13 [size=8, align=8]
    (%v16, %v17) = store %v15, %v11, %v13, %v0
    (%v18, %v19) = copy %v16, %v17, %v6, %v3 [size=16, align=8]
    %v20 = access_in_bounds %v18, %v6 [size=8, align=8]
    (%v21, %v22, %v23) = load %v20, %v19, %v6 : bits<64>
    ret %v21, %v22, %v23
}
";

#[test]
fn an_object_copy_agrees() {
    for raw in [0_u64, 0x0123_4567_89ab_cdef] {
        agree(OBJECT_COPY, &with_tokens(vec![bits(64, raw)]));
    }
}

const LIFETIME: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=4, align=4]
    (%v6, %v7) = lifetime.end %v4, %v5, %v3
    ret %v0, %v6, %v7
}
";

#[test]
fn an_explicit_lifetime_end_agrees() {
    agree(LIFETIME, &with_tokens(vec![bits(32, 5)]));
}

const OBLIGATION_IN_A_LOOP: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 0
    br ^b1(%v0, %v3, %v1, %v2)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 0
    %v9 = icmp.ne %v4, %v8
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v5, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    %v14 = const.bits<32> 0
    %v15 = icmp.ne %v10, %v14
    ub.assume.division_by_zero %v15
    %v16 = udiv.bits<32> %v11, %v10
    %v17 = const.bits<32> 1
    %v18 = sub.bits<32> %v10, %v17
    %v19 = add.bits<32> %v16, %v10
    br ^b1(%v18, %v19, %v12, %v13)

^b3(%v20: bits<32>, %v21: mem<all>, %v22: world):
    ret %v20, %v21, %v22
}
";

#[test]
fn an_obligation_inside_a_loop_body_agrees() {
    // The split at the obligation carries every value the rest of the block
    // still reads into the continuation block's arguments, so the loop's
    // back edge still passes what the header expects.
    for trips in [0_u64, 1, 4, 9] {
        agree(OBLIGATION_IN_A_LOOP, &with_tokens(vec![bits(32, trips)]));
    }
}

const FELL_OFF_THE_END: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 0
    %v4 = icmp.eq %v0, %v3
    cond_br %v4, ^b1(%v1, %v2), ^b2(%v0, %v1, %v2)

^b1(%v8: mem<all>, %v9: world):
    trap.unreachable_reached

^b2(%v5: bits<32>, %v6: mem<all>, %v7: world):
    ret %v5, %v6, %v7
}
";

#[test]
fn a_trap_reached_stops_both_interpreters_with_the_same_reason() {
    agree(FELL_OFF_THE_END, &with_tokens(vec![bits(32, 3)]));

    let module = parse_section9_module(FELL_OFF_THE_END.as_bytes()).expect("canonical EvoIR");
    let arguments = with_tokens(vec![bits(32, 0)]);
    let evoir = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect_err("the trap is reached");
    assert_eq!(evoir.code(), "E3024");

    let lowered = lower_module(&module).expect("lowerable");
    let error = lane::observe_entry(
        &lowered,
        "main",
        &arguments,
        &[EvoIRType::Bits(32)],
        MirLimits::new(1_000_000),
    )
    .expect_err("the machine reaches its brk");
    assert_eq!(error.code(), "E3024");
}

const WIDE_SCALAR: &str = "\
func @main(%arg0: bits<47>, %arg1: bits<47>, %arg2: mem<all>, %arg3: world) -> (bits<47>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<47>, %v1: bits<47>, %v2: mem<all>, %v3: world):
    %v4 = const.bits<47> 5
    %v5 = or.bits<47> %v1, %v4
    %v6 = sdiv.bits<47> %v0, %v5
    %v7 = srem.bits<47> %v0, %v5
    %v8 = add.bits<47> %v6, %v7
    %v9 = icmp.sle %v8, %v0
    %v10 = zext.bits<47> %v9
    %v11 = xor.bits<47> %v8, %v10
    %v12 = trunc.bits<9> %v11
    %v13 = sext.bits<47> %v12
    ret %v13, %v2, %v3
}
";

#[test]
fn a_gpr64_class_scalar_agrees() {
    for (left, right) in [
        (0_u64, 0_u64),
        (1, 2),
        (0x7fff_ffff_ffff, 0x11),
        (0x4000_0000_0000, 3),
    ] {
        agree(
            WIDE_SCALAR,
            &with_tokens(vec![bits(47, left), bits(47, right)]),
        );
    }
}
