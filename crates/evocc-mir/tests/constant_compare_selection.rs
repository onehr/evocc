use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{MInst, MirLimits, Target};
use evocc_types::{EvoIRType, Region};

fn source(op: &str, bits: u16, constant: u64, left: bool) -> String {
    let operands = if left { "%v3, %v0" } else { "%v0, %v3" };
    format!("func @main(%arg0: bits<{bits}>, %arg1: mem<all>, %arg2: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<{bits}> {constant}
    %v4 = icmp.{op} {operands}
    ret %v4, %v1, %v2
}}
")
}

fn lower(text: &str) -> (evocc_evoir::Module, evocc_mir::MModule) {
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    assert_eq!(
        evocc_mir::parse_module(&evocc_mir::render_module(&mir)).unwrap(),
        mir
    );
    for f in &mir.functions {
        evocc_mir::verify_allocation(
            &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(f)).unwrap(),
        )
        .unwrap();
    }
    (ir, mir)
}

#[test]
fn immediate_comparisons_preserve_all_predicates_and_operand_order() {
    for op in ["eq", "ne", "slt", "sle", "ult", "ule"] {
        for bits in [32, 64] {
            for value in [0, 1, 19, 4095] {
                for left in [false, true] {
                    let text = source(op, bits, value, left);
                    let (ir, mir) = lower(&text);
                    assert!(
                        mir.functions[0].blocks[0].insts.iter().any(|i| matches!(i,
                        MInst::CmpImm { imm, .. } if *imm == value)),
                        "{text}"
                    );
                    for raw in [
                        0,
                        1,
                        18,
                        19,
                        20,
                        4094,
                        4095,
                        4096,
                        u64::MAX,
                        1_u64 << (bits - 1),
                        (1_u64 << (bits - 1)) - 1,
                    ] {
                        let args = [
                            RuntimeValue::bits(bits, raw),
                            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                            RuntimeValue::new(EvoIRType::World, 0),
                        ];
                        let expected = evocc_evoir::interpret_module(
                            &ir,
                            "main",
                            &args,
                            InterpreterLimits::new(100),
                        )
                        .unwrap();
                        let actual = evocc_mir::lane::observe_entry(
                            &mir,
                            "main",
                            &args,
                            &[EvoIRType::Bits(1)],
                            MirLimits::new(100),
                        )
                        .unwrap();
                        assert_eq!(
                            actual.returns,
                            expected.return_values[..1],
                            "{text}: {raw:x}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn nonmatching_widths_constants_and_dynamic_operands_keep_register_comparisons() {
    for (bits, value) in [(8, 19), (16, 19), (32, 4096), (64, u64::MAX)] {
        for op in ["eq", "slt", "ule"] {
            for left in [false, true] {
                let (_, mir) = lower(&source(op, bits, value, left));
                assert!(mir.functions[0].blocks[0]
                    .insts
                    .iter()
                    .any(|i| matches!(i, MInst::Cmp { .. })));
                assert!(!mir.functions[0].blocks[0]
                    .insts
                    .iter()
                    .any(|i| matches!(i, MInst::CmpImm { .. })));
            }
        }
    }
    let text = source("slt", 64, 1, false)
        .replace("    %v3 = const.bits<64> 1\n", "")
        .replace("%v0, %v3", "%v0, %v0");
    let (_, mir) = lower(&text);
    assert!(mir.functions[0].blocks[0]
        .insts
        .iter()
        .any(|i| matches!(i, MInst::Cmp { .. })));
}

#[test]
fn immediate_flags_keep_the_existing_single_use_branch_fusion() {
    let text = source("slt", 64, 19, true).replace("    ret %v4, %v1, %v2", "    cond_br %v4, ^b1(%v1, %v2), ^b2(%v1, %v2)\n\n^b1(%v5: mem<all>, %v6: world):\n    %v7 = const.bits<1> 1\n    ret %v7, %v5, %v6\n\n^b2(%v8: mem<all>, %v9: world):\n    %v10 = const.bits<1> 0\n    ret %v10, %v8, %v9");
    let (_, mir) = lower(&text);
    let entry = &mir.functions[0].blocks[0];
    assert!(matches!(
        entry.insts.last(),
        Some(MInst::CmpImm { imm: 19, .. })
    ));
    assert!(!entry.insts.iter().any(|i| matches!(i, MInst::Cset { .. })));
}

#[test]
fn malformed_compare_immediates_are_rejected_independently() {
    let (_, mir) = lower(&source("slt", 64, 19, false));
    // The current MIR form is unshifted imm12, not arbitrary assembler syntax.
    for invalid in [4096, 4097, u64::MAX] {
        let mut changed = mir.clone();
        for instruction in &mut changed.functions[0].blocks[0].insts {
            if let MInst::CmpImm { imm, .. } = instruction {
                *imm = invalid;
            }
        }
        assert!(evocc_mir::validate_module(&changed).is_err(), "{invalid}");
    }
}

#[test]
fn all_selected_predicates_assemble_on_both_targets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut text = String::new();
    for op in ["eq", "ne", "slt", "sle", "ult", "ule"] {
        for bits in [32, 64] {
            for value in [0, 1, 19, 4095] {
                for left in [false, true] {
                    text.push_str(
                        &source(op, bits, value, left)
                            .replace("@main(", &format!("@{op}_{bits}_{value}_{left}(")),
                    );
                    text.push('\n');
                }
            }
        }
    }
    text.pop();
    let (ir, _) = lower(&text);
    for (target, triple) in [
        (Target::Linux, "aarch64-linux-gnu"),
        (Target::Darwin, "arm64-apple-macosx11.0.0"),
    ] {
        let asm = evocc_mir::compile_module_for(target, &ir, &|_| true).unwrap();
        let mut child = Command::new("clang")
            .args([
                "-target",
                triple,
                "-x",
                "assembler",
                "-c",
                "-",
                "-o",
                "/dev/null",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(asm.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
