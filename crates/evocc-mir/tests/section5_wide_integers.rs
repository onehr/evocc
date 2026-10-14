//! EvoMIR Section 5, the row "bits<128> forms", and Section 6's "128-bit
//! arguments and results" and "Stack arguments" (revision 0.10).
//!
//! "A `bits<128>` EvoIR value has no class of its own: lowering represents it
//! as two `gpr64` virtual registers, the low half first, and every operation
//! on it becomes a sequence over the halves. The allocator, the verifier and
//! the interpreter see only 64-bit registers."
//!
//! Every sequence asserted below is one the sealed oracles emit for the same
//! C, recorded in `.evidence/builtin-probes/20260902-2c23684c/p34-p35.out`
//! (GCC-14) and `.evidence/builtin-probes/20260905-b6/b6-probes.out` (GCC-14
//! and Clang-19, with the ordered comparison and the arithmetic right shift
//! checked against the C operators over 20,480 and 8,192 items).

use evocc_evoir::parse_section9_module;
use evocc_mir::{
    allocate, compile_module_for, lower_module, lower_module_for, out_of_ssa, validate_module,
    verify_allocation, MInst, MTerm, RegClass, Target,
};

/// A function over `param_tys` returning `results`, with `body` between.
///
/// Section 9 numbers parameters positionally, so the effect tokens take the
/// two positions after the ordinary arguments.
fn func(param_tys: &[&str], entry: &str, results: &str, body: &str, ret: &str) -> String {
    let params: Vec<String> = param_tys
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("%arg{index}: {ty}"))
        .collect();
    let memory = param_tys.len();
    let world = memory + 1;
    let params = params.join(", ");
    let separator = if params.is_empty() { "" } else { ", " };
    format!(
        "func @main({params}{separator}%arg{memory}: mem<all>, %arg{world}: world) \
-> ({results}, mem<all>, world) cc = target_default\n    \
effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, \
may_trap=false, may_unwind=false }}\n{{\n\
^entry({entry}, %v90: mem<all>, %v91: world):\n{body}    ret {ret}, %v90, %v91\n}}\n"
    )
}

fn lower(text: &str) -> evocc_mir::MModule {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("Section 5 has a row for every form here");
    validate_module(&lowered).expect("the lowering produces verifiable EvoMIR");
    lowered
}

/// The same, for a named target. The pair rule of Section 6.5 differs by
/// ABI profile: Linux rounds a `bits<128>` argument to an even register,
/// and Darwin does not.
fn lower_for(target: Target, text: &str) -> evocc_mir::MModule {
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module_for(target, &module, &|_| false)
        .expect("Section 5 has a row for every form here");
    validate_module(&lowered).expect("the lowering produces verifiable EvoMIR");
    lowered
}

/// The mnemonics of every instruction of a lowered module, in order.
fn mnemonics(module: &evocc_mir::MModule) -> Vec<String> {
    let mut names = Vec::new();
    for function in &module.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                names.push(match inst {
                    MInst::Mov { .. } => "mov".to_owned(),
                    MInst::MovImm { .. } => "mov#".to_owned(),
                    MInst::Binary { op, .. } => op.mnemonic().to_owned(),
                    MInst::BinaryImm { op, .. } => format!("{}#", op.mnemonic()),
                    MInst::FlagBinary { op, .. } => op.mnemonic().to_owned(),
                    MInst::WideMultiply { op, .. } => op.mnemonic().to_owned(),
                    MInst::Msub { .. } => "msub".to_owned(),
                    MInst::Extend { op, .. } => op.mnemonic().to_owned(),
                    MInst::Bfx { signed, .. } => {
                        if *signed {
                            "sbfx".to_owned()
                        } else {
                            "ubfx".to_owned()
                        }
                    }
                    MInst::Cmp { .. } => "cmp".to_owned(),
                    MInst::CmpImm { .. } => "cmp#".to_owned(),
                    MInst::Cset { .. } => "cset".to_owned(),
                    MInst::Csel { .. } => "csel".to_owned(),
                    MInst::Load { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Store { kind, .. } => kind.mnemonic().to_owned(),
                    MInst::Ldp { .. } => "ldp".to_owned(),
                    MInst::Stp { .. } => "stp".to_owned(),
                    MInst::CarryBinary { op, .. } => op.mnemonic().to_owned(),
                    MInst::SubCarryFlags { .. } => "sbcs".to_owned(),
                    MInst::TstImm { .. } => "tst".to_owned(),
                    MInst::Madd { .. } => "madd".to_owned(),
                    other => format!("{other:?}")
                        .split_whitespace()
                        .next()
                        .unwrap_or("?")
                        .to_lowercase(),
                });
            }
        }
    }
    names
}

/// Section 2: after lowering, no virtual register is anything but a 64-bit
/// register, a 32-bit register or the flags.
fn every_register_is_narrow(module: &evocc_mir::MModule) {
    for function in &module.functions {
        for block in &function.blocks {
            for reg in block.args.iter().copied() {
                assert!(
                    matches!(
                        reg.class,
                        RegClass::Gpr32 | RegClass::Gpr64 | RegClass::Flags
                    ),
                    "a block argument escaped the pair representation: {reg}"
                );
            }
            for inst in &block.insts {
                for reg in evocc_mir::inst_defs(inst)
                    .into_iter()
                    .chain(evocc_mir::inst_uses(inst))
                {
                    assert!(
                        matches!(
                            reg.class,
                            RegClass::Gpr32 | RegClass::Gpr64 | RegClass::Flags
                        ),
                        "an operand escaped the pair representation: {reg}"
                    );
                }
            }
        }
    }
}

const ADD: &str = "    %v2 = add.bits<128> %v0, %v1\n";
const SUB: &str = "    %v2 = sub.bits<128> %v0, %v1\n";
const MUL: &str = "    %v2 = mul.bits<128> %v0, %v1\n";

fn wide_binary(body: &str) -> String {
    func(
        &["bits<128>", "bits<128>"],
        "%v0: bits<128>, %v1: bits<128>",
        "bits<128>",
        body,
        "%v2",
    )
}

#[test]
fn wide_pointer_indexes_keep_the_machine_address_bits() {
    let text = func(
        &["ptr<generic>", "bits<128>"],
        "%v0: ptr<generic>, %v1: bits<128>",
        "ptr<generic>",
        "    %v2 = ptr.offset.add.signed %v0, %v1 [scale=4]\n    %v3 = pointer_valid %v90, %v2\n",
        "%v2",
    )
    .replace("ret %v2, %v90, %v91", "ret %v2, %v3, %v91")
    .replace("reads={}", "reads={all}")
    .replace("may_trap=false", "may_trap=true");
    let module = lower_for(Target::Linux, &text);
    every_register_is_narrow(&module);
    let serialized = evocc_mir::render_module(&module);
    assert_eq!(evocc_mir::parse_module(&serialized).unwrap(), module);
    for (low, high, expected) in [(4, 0, 0x1010), (u64::MAX, u64::MAX, 0xffc)] {
        let output = evocc_mir::interpret_module(
            &module,
            "main",
            &[
                evocc_mir::MirValue::Int(0x1000),
                // Linux: the 128-bit argument starts at the next even
                // register pair.
                evocc_mir::MirValue::Int(0),
                evocc_mir::MirValue::Int(low),
                evocc_mir::MirValue::Int(high),
            ],
            evocc_mir::MirLimits::new(1000),
        )
        .unwrap();
        assert_eq!(output.results, [evocc_mir::MirValue::Int(expected)]);
        // Darwin: it takes the next two registers (Section 6.5).
        let darwin = lower_for(Target::Darwin, &text);
        let output = evocc_mir::interpret_module(
            &darwin,
            "main",
            &[
                evocc_mir::MirValue::Int(0x1000),
                evocc_mir::MirValue::Int(low),
                evocc_mir::MirValue::Int(high),
            ],
            evocc_mir::MirLimits::new(1000),
        )
        .unwrap();
        assert_eq!(output.results, [evocc_mir::MirValue::Int(expected)]);
    }
    compile_module_for(
        Target::Linux,
        &parse_section9_module(text.as_bytes()).unwrap(),
        &|_| true,
    )
    .unwrap();
}

#[test]
fn addition_and_subtraction_carry_between_the_halves() {
    // "add: `adds lo` then `adc hi`; sub: `subs lo` then `sbc hi`"
    // (GCC-14 `add128`: `adds x0, x0, x2` / `adc x1, x1, x3`;
    //  `sub128`: `subs x0, x0, x2` / `sbc x1, x1, x3`).
    let module = lower(&wide_binary(ADD));
    every_register_is_narrow(&module);
    assert_eq!(mnemonics(&module), vec!["adds", "adc"]);

    let module = lower(&wide_binary(SUB));
    every_register_is_narrow(&module);
    assert_eq!(mnemonics(&module), vec!["subs", "sbc"]);
}

#[test]
fn the_product_is_one_mul_one_umulh_and_two_madd() {
    // "mul: `mul lo, alo, blo`, `umulh hi, alo, blo`, `madd hi, alo, bhi, hi`,
    // `madd hi, ahi, blo, hi`" (GCC-14 `mul128`).
    let module = lower(&wide_binary(MUL));
    every_register_is_narrow(&module);
    let names = mnemonics(&module);
    assert_eq!(names.iter().filter(|name| *name == "mul").count(), 1);
    assert_eq!(names.iter().filter(|name| *name == "umulh").count(), 1);
    assert_eq!(names.iter().filter(|name| *name == "madd").count(), 2);
    assert_eq!(names.len(), 4, "{names:?}");
}

#[test]
fn the_bitwise_forms_are_halfwise() {
    // "and, or, xor: halfwise".
    for (operation, mnemonic) in [("and", "and"), ("or", "orr"), ("xor", "eor")] {
        let body = format!("    %v2 = {operation}.bits<128> %v0, %v1\n");
        let module = lower(&wide_binary(&body));
        every_register_is_narrow(&module);
        assert_eq!(mnemonics(&module), vec![mnemonic, mnemonic], "{operation}");
    }
}

#[test]
fn the_width_changes_are_a_high_half_of_zero_a_sign_fill_and_a_drop() {
    // "zext to 128: hi = 0; sext to 128: hi = asr lo, #63; trunc from 128:
    // the low half" (GCC-14 `zext128`: `mov x1, 0`; `sext128`: `asr x1, x0,
    // 63`; `trunc128`: nothing at all).
    let zext = func(
        &["bits<64>"],
        "%v0: bits<64>",
        "bits<128>",
        "    %v1 = zext.bits<128> %v0\n",
        "%v1",
    );
    let module = lower(&zext);
    every_register_is_narrow(&module);
    assert!(
        mnemonics(&module).contains(&"mov#".to_owned()),
        "the high half is materialized as zero: {:?}",
        mnemonics(&module)
    );

    let sext = func(
        &["bits<64>"],
        "%v0: bits<64>",
        "bits<128>",
        "    %v1 = sext.bits<128> %v0\n",
        "%v1",
    );
    let module = lower(&sext);
    every_register_is_narrow(&module);
    assert!(
        mnemonics(&module).contains(&"asr#".to_owned())
            || mnemonics(&module).contains(&"asr".to_owned()),
        "the high half is the sign fill: {:?}",
        mnemonics(&module)
    );

    let truncate = func(
        &["bits<128>"],
        "%v0: bits<128>",
        "bits<64>",
        "    %v1 = trunc.bits<64> %v0\n",
        "%v1",
    );
    let module = lower(&truncate);
    every_register_is_narrow(&module);
}

#[test]
fn the_variable_shifts_are_branch_free_with_two_selects() {
    // "shl, lshr, ashr by an amount s ...: the branch-free form the oracles
    // emit, that is the 64-bit shifts of both halves, the cross term lsl/lsr
    // of the other half by 63 - s after a one-bit pre-shift, combined by orr,
    // and two csel on tst s, #64" (GCC-14 `shl_var`, `lshr_var`; the
    // arithmetic form measured by hand in b6-cmp-sbcs.c over 8,192 items).
    for operation in ["shl", "lshr", "ashr"] {
        let body = format!("    %v2 = {operation}.bits<128> %v0, %v1\n");
        let module = lower(&wide_binary(&body));
        every_register_is_narrow(&module);
        let names = mnemonics(&module);
        assert_eq!(
            names.iter().filter(|name| *name == "csel").count(),
            2,
            "{operation}: two selects on the 64-or-more case: {names:?}"
        );
        assert!(
            names.iter().any(|name| name == "orr"),
            "{operation}: the cross term is combined by orr: {names:?}"
        );
    }
}

#[test]
fn equality_is_two_exclusive_ors_and_a_disjunction() {
    // "icmp.eq/ne: eor each half pair, orr, compare with zero" (GCC-14
    // `eq128`, `ne128`).
    for operation in ["eq", "ne"] {
        let body = format!("    %v2 = icmp.{operation} %v0, %v1\n");
        let text = func(
            &["bits<128>", "bits<128>"],
            "%v0: bits<128>, %v1: bits<128>",
            "bits<1>",
            &body,
            "%v2",
        );
        let module = lower(&text);
        every_register_is_narrow(&module);
        let names = mnemonics(&module);
        assert_eq!(
            names.iter().filter(|name| *name == "eor").count(),
            2,
            "{operation}: {names:?}"
        );
        assert!(names.iter().any(|name| name == "orr"), "{operation}");
    }
}

#[test]
fn an_ordered_comparison_subtracts_with_borrow_through_the_high_half() {
    // "ordered icmp: cmp alo, blo then sbcs xzr, ahi, bhi and cset with the
    // 64-bit form's condition (lo for ult, lt for slt; ule and sle compute
    // the swapped subtraction and take hs and ge)". GCC-14 emits a branching
    // form for these, so the sentence was measured directly against the C
    // operators: 20,480 items, 0 mismatches, both oracles
    // (`.evidence/builtin-probes/20260905-b6/b6-cmp-sbcs.c`).
    for operation in ["ult", "ule", "slt", "sle"] {
        let body = format!("    %v2 = icmp.{operation} %v0, %v1\n");
        let text = func(
            &["bits<128>", "bits<128>"],
            "%v0: bits<128>, %v1: bits<128>",
            "bits<1>",
            &body,
            "%v2",
        );
        let module = lower(&text);
        every_register_is_narrow(&module);
        let names = mnemonics(&module);
        assert!(
            names.iter().any(|name| name == "cmp"),
            "{operation}: {names:?}"
        );
        assert!(
            names.iter().any(|name| name == "sbcs"),
            "{operation}: the borrow crosses into the high half: {names:?}"
        );
        assert!(
            names.iter().any(|name| name == "cset"),
            "{operation}: {names:?}"
        );
    }
}

/// A 16-byte load and store of a `bits<128>` through a pointer parameter.
const WIDE_MEMORY: &str = "\
func @main(%arg0: ptr<generic>, %arg1: bits<128>, %arg2: mem<all>, %arg3: world) -> (bits<128>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: bits<128>, %v90: mem<all>, %v91: world):
    %v2 = access_in_bounds %v90, %v0 [size=16, align=16]
    (%v3, %v4) = store %v2, %v91, %v0, %v1
    %v5 = access_in_bounds %v3, %v0 [size=16, align=16]
    (%v6, %v7, %v8) = load %v5, %v4, %v0 : bits<128>
    ret %v6, %v7, %v8
}
";

#[test]
fn a_wide_load_and_store_are_a_register_pair() {
    // "load/store: ldp/stp of the halves at the address, which is not
    // single-copy atomic, exactly as both oracles emit for a volatile u128"
    // (GCC-14 `load_vol`: `ldp x0, x1, [x0]`; `store_vol`: `stp x4, x5,
    // [x0]`).
    let module = parse_section9_module(WIDE_MEMORY.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("a 16-byte access has a Section 5 row");
    validate_module(&lowered).expect("verifiable");
    every_register_is_narrow(&lowered);
    let names = mnemonics(&lowered);
    assert!(names.iter().any(|name| name == "ldp"), "{names:?}");
    assert!(names.iter().any(|name| name == "stp"), "{names:?}");
}

#[test]
fn wide_division_and_remainder_use_verified_runtime_calls() {
    for (operation, helper) in [
        ("sdiv", "__divti3"),
        ("udiv", "__udivti3"),
        ("srem", "__modti3"),
        ("urem", "__umodti3"),
    ] {
        // Division may trap, so its function carries the Section 4.6 summary
        // that says so.
        let text = [
            "func @main(%arg0: bits<128>, %arg1: bits<128>, %arg2: mem<all>, %arg3: world) -> (bits<128>, mem<all>, world) cc = target_default".to_owned(),
            "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }".to_owned(),
            "{".to_owned(),
            "^entry(%v0: bits<128>, %v1: bits<128>, %v90: mem<all>, %v91: world):".to_owned(),
            format!("    %v2 = {operation}.bits<128> %v0, %v1"),
            "    ret %v2, %v90, %v91".to_owned(),
            "}".to_owned(),
            String::new(),
        ]
        .join("\n");
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let lowered = lower_module(&module).expect("hosted runtime division lowers");
        validate_module(&lowered).unwrap();
        every_register_is_narrow(&lowered);
        let calls: Vec<_> = lowered
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .filter_map(|i| {
                if let MInst::Bl {
                    symbol,
                    args,
                    results,
                    ..
                } = i
                {
                    Some((symbol, args, results))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, helper);
        assert_eq!(calls[0].1.len(), 4);
        assert_eq!(calls[0].2.len(), 2);
        let rendered = evocc_mir::render_module(&lowered);
        assert_eq!(evocc_mir::parse_module(&rendered).unwrap(), lowered);
        let assembly = compile_module_for(Target::Linux, &module, &|_| true).unwrap();
        assert!(assembly.contains(&format!("bl {helper}")), "{assembly}");
        for (left, right) in [((1_u128 << 100) + 7, 3), (u128::MAX, 2), (1_u128 << 127, 1)] {
            let expected = match operation {
                "sdiv" => ((left as i128) / (right as i128)) as u128,
                "srem" => ((left as i128) % (right as i128)) as u128,
                "udiv" => left / right,
                _ => left % right,
            };
            let out = evocc_mir::interpret_module(
                &lowered,
                "main",
                &[
                    evocc_mir::MirValue::Int(left as u64),
                    evocc_mir::MirValue::Int((left >> 64) as u64),
                    evocc_mir::MirValue::Int(right as u64),
                    evocc_mir::MirValue::Int((right >> 64) as u64),
                ],
                evocc_mir::MirLimits::new(1000),
            )
            .unwrap();
            assert_eq!(
                out.results,
                [
                    evocc_mir::MirValue::Int(expected as u64),
                    evocc_mir::MirValue::Int((expected >> 64) as u64)
                ]
            );
        }
    }
}

#[test]
fn signed_predicates_have_full_width_lowering() {
    for predicate in [
        "signed_add_defined",
        "signed_sub_defined",
        "signed_mul_defined",
        "signed_div_rem_no_overflow",
    ] {
        let body = format!("    %v2 = {predicate} %v0, %v1\n");
        let text = func(
            &["bits<128>", "bits<128>"],
            "%v0: bits<128>, %v1: bits<128>",
            "bits<1>",
            &body,
            "%v2",
        );
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let lowered = lower_module(&module).expect("total predicates lower at width 128");
        validate_module(&lowered).expect("the independent machine validator accepts");
    }
}

#[test]
fn the_shift_count_predicate_compares_against_one_hundred_and_twenty_eight() {
    // "shift_count_defined compares the amount with #128".
    let text = func(
        &["bits<128>", "bits<128>"],
        "%v0: bits<128>, %v1: bits<128>",
        "bits<1>",
        "    %v2 = shift_count_defined.unsigned %v0, %v1\n",
        "%v2",
    );
    let module = lower(&text);
    every_register_is_narrow(&module);
    let mut bounds = Vec::new();
    for function in &module.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                if let MInst::CmpImm { imm, .. } = inst {
                    bounds.push(*imm);
                }
            }
        }
    }
    assert!(bounds.contains(&128), "the bound is the width: {bounds:?}");
}

#[test]
fn widths_between_sixty_five_and_one_hundred_and_twenty_seven_stay_e4003() {
    // Section 3: "Widths from 65 to 127 and above 128 are not lowered
    // (E4003)." The narrowing of revision 0.10 opens 128 and nothing else.
    for width in [65, 96, 127, 129, 256] {
        let text = func(
            &[format!("bits<{width}>").as_str()],
            &format!("%v0: bits<{width}>"),
            &format!("bits<{width}>"),
            &format!("    %v1 = add.bits<{width}> %v0, %v0\n"),
            "%v1",
        );
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let refusals = lower_module(&module).expect_err("only 1 to 64 and 128 are lowered");
        assert!(
            refusals.iter().any(|refusal| refusal.code == "E4003"),
            "bits<{width}>: {refusals:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Section 6: 128-bit arguments and results
// ---------------------------------------------------------------------------

/// The number of argument positions a lowered function's entry block holds.
///
/// Section 7 pins each by its position, so the count is what says which
/// register the convention assigned: a skipped register is a real position.
fn argument_positions(target: Target, text: &str) -> usize {
    let module = lower_for(target, text);
    module.functions[0].blocks[0].args.len()
}

/// One function summing a `bits<64>` argument with both halves of a
/// `bits<128>` one, so that every register the convention names is read.
fn sums_both_halves(before: &[&str]) -> String {
    let mut param_tys: Vec<&str> = before.to_vec();
    param_tys.push("bits<128>");
    let entry: Vec<String> = before
        .iter()
        .enumerate()
        .map(|(index, ty)| format!("%v{index}: {ty}"))
        .chain(std::iter::once(format!("%v{}: bits<128>", before.len())))
        .collect();
    let wide = before.len();
    let mut body = format!(
        "    %v20 = trunc.bits<64> %v{wide}\n\
         \x20   %v21 = const.bits<128> 64\n\
         \x20   %v22 = lshr.bits<128> %v{wide}, %v21\n\
         \x20   %v23 = trunc.bits<64> %v22\n\
         \x20   %v24 = add.bits<64> %v20, %v23\n"
    );
    let mut last = 24;
    for index in 0..before.len() {
        body.push_str(&format!(
            "    %v{} = add.bits<64> %v{last}, %v{index}\n",
            last + 1
        ));
        last += 1;
    }
    func(
        &param_tys,
        &entry.join(", "),
        "bits<64>",
        &body,
        &format!("%v{last}"),
    )
}

#[test]
fn a_wide_argument_takes_an_even_aligned_register_pair() {
    // "A bits<128> argument takes two consecutive registers x(2k), x(2k+1),
    // the low half in the even one: the next register number is rounded up to
    // even first, so after one 64-bit argument it is x2:x3 with x1 skipped,
    // after two it is x2:x3 with nothing skipped" (GCC-14 `take_after_one`,
    // `take_after_two`).
    assert_eq!(
        argument_positions(Target::Linux, &sums_both_halves(&[])),
        2,
        "a pair alone takes x0:x1"
    );
    assert_eq!(
        argument_positions(Target::Linux, &sums_both_halves(&["bits<64>"])),
        4,
        "after one 64-bit argument the pair is x2:x3, with x1 skipped"
    );
    assert_eq!(
        argument_positions(Target::Linux, &sums_both_halves(&["bits<64>", "bits<64>"])),
        4,
        "after two it is x2:x3 with nothing skipped"
    );
    assert_eq!(
        argument_positions(
            Target::Linux,
            &sums_both_halves(&["bits<64>", "bits<64>", "bits<64>"])
        ),
        6,
        "after three the pair is x4:x5, with x3 skipped"
    );
    // Section 6.5: the Darwin ABI profile does not round, as Apple clang does
    // not: after one 64-bit argument the pair is x1:x2 (D04).
    for (before, positions) in [
        (vec![], 2),
        (vec!["bits<64>"], 3),
        (vec!["bits<64>", "bits<64>"], 4),
        (vec!["bits<64>", "bits<64>", "bits<64>"], 5),
    ] {
        assert_eq!(
            argument_positions(Target::Darwin, &sums_both_halves(&before)),
            positions,
            "Darwin, after {} 64-bit arguments",
            before.len()
        );
    }

    // And every one of them allocates and verifies.
    for before in [
        vec![],
        vec!["bits<64>"],
        vec!["bits<64>", "bits<64>"],
        vec!["bits<64>", "bits<64>", "bits<64>"],
    ] {
        let text = sums_both_halves(&before);
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        compile_module_for(Target::Linux, &module, &|_| true)
            .expect("the even-aligned pair compiles");
    }
}

#[test]
fn a_wide_result_is_x0_and_x1_with_the_low_half_first() {
    // "A bits<128> result is x0:x1, low half in x0" (GCC-14 `ret128`, which
    // returns the constant in x0 and the shifted half in x1).
    let text = func(
        &["bits<64>"],
        "%v0: bits<64>",
        "bits<128>",
        "    %v1 = zext.bits<128> %v0\n",
        "%v1",
    );
    let module = lower(&text);
    let MTerm::Ret { values } = &module.functions[0].blocks[0].term else {
        panic!("the entry returns");
    };
    assert_eq!(values.len(), 2, "the pair occupies two result registers");

    // The low half is the zero-extended argument and the high half is the
    // materialized zero, so the two are distinguishable in the emitted text.
    let emitted = compile_module_for(Target::Linux, &module_of(&text), &|_| true)
        .expect("a wide result compiles");
    assert!(
        emitted.contains("x1, #0"),
        "the high half is zero in x1:\n{emitted}"
    );
}

fn module_of(text: &str) -> evocc_evoir::Module {
    parse_section9_module(text.as_bytes()).expect("canonical EvoIR")
}

// ---------------------------------------------------------------------------
// Section 6: stack arguments (Linux only)
// ---------------------------------------------------------------------------

fn many_arguments(count: usize) -> String {
    let param_tys: Vec<String> = (0..count)
        .map(|index| format!("bits<{}>", if index == 8 { 32 } else { 64 }))
        .collect();
    let borrowed: Vec<&str> = param_tys.iter().map(String::as_str).collect();
    let entry: Vec<String> = (0..count)
        .map(|index| format!("%v{index}: bits<{}>", if index == 8 { 32 } else { 64 }))
        .collect();
    let mut body = String::new();
    body.push_str("    %v100 = zext.bits<64> %v8\n");
    body.push_str("    %v101 = add.bits<64> %v0, %v100\n");
    let mut last = 101;
    for index in 1..count {
        if index == 8 {
            continue;
        }
        body.push_str(&format!(
            "    %v{} = add.bits<64> %v{last}, %v{index}\n",
            last + 1
        ));
        last += 1;
    }
    func(
        &borrowed,
        &entry.join(", "),
        "bits<64>",
        &body,
        &format!("%v{last}"),
    )
}

#[test]
fn the_ninth_and_tenth_arguments_reach_the_linux_stack() {
    // "On the Linux target an argument that finds no register goes to the
    // stack in an 8-byte slot, slots assigned in argument order at increasing
    // addresses from the caller's sp at the call ... the callee reads slot i
    // at x29 + 16 + 8 * i" (GCC-14 `nine`: `ldrb w1, [sp]`; `ten`:
    // `ldrb w2, [sp]` and `ldr x1, [sp, 8]`).
    for count in [9, 10] {
        let text = many_arguments(count);
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let emitted = compile_module_for(Target::Linux, &module, &|_| true)
            .unwrap_or_else(|stop| panic!("{count} arguments lower on Linux: {stop:?}"));
        assert!(emitted.contains("x29"), "{count}: {emitted}");
    }
}

#[test]
fn darwin_keeps_refusing_a_ninth_argument_with_e4010() {
    // "On Darwin the platform packs stack arguments by natural alignment
    // instead; that rule is still unmeasured, so more than eight integer
    // arguments stay E4010 there."
    for count in [9, 10] {
        let text = many_arguments(count);
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let refusals = lower_module(&module).expect_err("Darwin refuses more than eight arguments");
        assert!(
            refusals.iter().any(|refusal| refusal.code == "E4010"),
            "{count}: {refusals:?}"
        );
    }
}

#[test]
fn a_wide_argument_beyond_the_registers_takes_a_whole_sixteen_aligned_slot() {
    // "after seven it goes to the stack, whole, 16-aligned, with x7 unused"
    // (GCC-14 `take_after_seven`: `ldp x2, x1, [sp]`).
    let param_tys: Vec<String> = (0..7)
        .map(|_| "bits<64>".to_owned())
        .chain(std::iter::once("bits<128>".to_owned()))
        .collect();
    let borrowed: Vec<&str> = param_tys.iter().map(String::as_str).collect();
    let entry: Vec<String> = (0..7)
        .map(|index| format!("%v{index}: bits<64>"))
        .chain(std::iter::once("%v7: bits<128>".to_owned()))
        .collect();
    let text = func(
        &borrowed,
        &entry.join(", "),
        "bits<64>",
        "    %v8 = trunc.bits<64> %v7\n    %v9 = add.bits<64> %v0, %v8\n",
        "%v9",
    );
    let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
    let emitted = compile_module_for(Target::Linux, &module, &|_| true)
        .expect("a wide argument after seven goes to the stack");
    assert!(emitted.contains("ldp"), "{emitted}");
}

// ---------------------------------------------------------------------------
// The allocator and the verifier see only 64-bit registers
// ---------------------------------------------------------------------------

#[test]
fn the_allocator_and_the_verifier_see_only_sixty_four_bit_registers() {
    for body in [ADD, SUB, MUL] {
        let module = lower(&wide_binary(body));
        for function in &module.functions {
            let resolved = out_of_ssa(function);
            let allocated = allocate(&resolved).expect("the pair allocates as two registers");
            verify_allocation(&allocated).expect("the allocation verifies");
        }
    }
}

/// Section 5 fuses a comparison into the branch that consumes it: "when the
/// only consumer is a `cond_br` in the same block, `cmp` then `b.cond` with no
/// `cset`". The borrow-aware form leaves the flags in the second of two
/// instructions, so the fusion has to reach past the first.
#[test]
fn a_wide_comparison_fuses_into_the_branch_that_consumes_it() {
    for operation in ["eq", "ne", "ult", "ule", "slt", "sle"] {
        let text = [
            "func @main(%arg0: bits<128>, %arg1: bits<128>, %arg2: mem<all>, %arg3: world) -> (bits<64>, mem<all>, world) cc = target_default".to_owned(),
            "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }".to_owned(),
            "{".to_owned(),
            "^entry(%v0: bits<128>, %v1: bits<128>, %v90: mem<all>, %v91: world):".to_owned(),
            format!("    %v2 = icmp.{operation} %v0, %v1"),
            "    cond_br %v2, ^b1(%v90, %v91), ^b2(%v90, %v91)".to_owned(),
            String::new(),
            "^b1(%v5: mem<all>, %v6: world):".to_owned(),
            "    %v3 = const.bits<64> 1".to_owned(),
            "    ret %v3, %v5, %v6".to_owned(),
            String::new(),
            "^b2(%v7: mem<all>, %v8: world):".to_owned(),
            "    %v4 = const.bits<64> 0".to_owned(),
            "    ret %v4, %v7, %v8".to_owned(),
            "}".to_owned(),
            String::new(),
        ]
        .join("\n");
        let module = parse_section9_module(text.as_bytes()).expect("canonical EvoIR");
        let lowered = lower_module(&module).expect("a fused wide comparison lowers");
        validate_module(&lowered).expect("and keeps the flags adjacency rule");
        every_register_is_narrow(&lowered);
        let names = mnemonics(&lowered);
        assert!(
            !names.iter().any(|name| name == "cset"),
            "{operation}: the fused form writes no cset: {names:?}"
        );
        compile_module_for(Target::Linux, &module, &|_| true)
            .unwrap_or_else(|stop| panic!("{operation}: {stop:?}"));
    }
}
