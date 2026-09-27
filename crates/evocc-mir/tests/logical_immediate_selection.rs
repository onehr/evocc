use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{BinOp, MInst, MirLimits, Target};
use evocc_types::{EvoIRType, Region};

fn source(op: &str, bits: u16, constant: u64, left: bool) -> String {
    let operands = if left { "%v3, %v0" } else { "%v0, %v3" };
    format!("func @main(%arg0: bits<{bits}>, %arg1: mem<all>, %arg2: world) -> (bits<{bits}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<{bits}> {constant}
    %v4 = {op}.bits<{bits}> {operands}
    ret %v4, %v1, %v2
}}
")
}

fn masks(bits: u16) -> Vec<u64> {
    if bits == 32 {
        vec![
            1,
            0x8000_0000,
            0xffff_fffe,
            0x8000_0001,
            0x5555_5555,
            0x3333_3333,
            0x0f0f_0f0f,
            0x00ff_00ff,
            0x07f0_07f0,
            0xffff_0000,
        ]
    } else {
        vec![
            1,
            1 << 63,
            u64::MAX - 1,
            0x8000_0000_0000_0001,
            0x5555_5555_5555_5555,
            0x3333_3333_3333_3333,
            0x0f0f_0f0f_0f0f_0f0f,
            0x00ff_00ff_00ff_00ff,
            0x07f0_07f0_07f0_07f0,
            0xffff_ffff_ffff_f000,
        ]
    }
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
    for function in &mir.functions {
        evocc_mir::verify_allocation(
            &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(function)).unwrap(),
        )
        .unwrap();
    }
    (ir, mir)
}

#[test]
fn encodable_masks_select_existing_logical_immediates_and_keep_exact_values() {
    for (name, op) in [("and", BinOp::And), ("or", BinOp::Orr), ("xor", BinOp::Eor)] {
        for bits in [32, 64] {
            for mask in masks(bits) {
                for left in [false, true] {
                    let text = source(name, bits, mask, left);
                    let (ir, mir) = lower(&text);
                    assert!(
                        mir.functions[0].blocks[0].insts.iter().any(|i| matches!(i,
                        MInst::BinaryImm { op: actual, imm, .. } if *actual == op && *imm == mask)),
                        "{text}"
                    );
                    for raw in [0, 1, u64::MAX, 1_u64 << (bits - 1), 0x1234_5678_9abc_def0] {
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
                            &[EvoIRType::Bits(bits)],
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
fn unencodable_narrow_pair_and_dynamic_operands_keep_the_register_path() {
    for name in ["and", "or", "xor"] {
        for (bits, value) in [
            (32, 0),
            (64, 0),
            (32, u64::from(u32::MAX)),
            (64, u64::MAX),
            (32, 0x12345678),
            (64, 0x123456789abcdef0),
            (8, 0x55),
            (16, 0x5555),
            (128, 0x55555555),
        ] {
            let (_, mir) = lower(&source(name, bits, value, false));
            assert!(!mir.functions[0].blocks[0].insts.iter().any(|i| matches!(
                i,
                MInst::BinaryImm {
                    op: BinOp::And | BinOp::Orr | BinOp::Eor,
                    ..
                }
            )));
        }
        let text = source(name, 64, 1, false)
            .replace("    %v3 = const.bits<64> 1\n", "")
            .replace("%v0, %v3", "%v0, %v0");
        let (_, mir) = lower(&text);
        assert!(mir.functions[0].blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, MInst::Binary { .. })));
    }
}

#[test]
fn constant_complements_and_bitwise_chains_remain_exact_facts() {
    for bits in [32, 64] {
        let full = if bits == 32 {
            u64::from(u32::MAX)
        } else {
            u64::MAX
        };
        let text = source("and", bits, full ^ 4095, false).replace(
            &format!("    %v3 = const.bits<{bits}> {}\n", full ^ 4095),
            &format!("    %v5 = const.bits<{bits}> 4095\n    %v6 = const.bits<{bits}> {full}\n    %v7 = xor.bits<{bits}> %v5, %v6\n    %v8 = const.bits<{bits}> 1\n    %v9 = or.bits<{bits}> %v7, %v8\n    %v3 = and.bits<{bits}> %v9, %v7\n"),
        );
        let (ir, mir) = lower(&text);
        assert!(mir.functions[0].blocks[0].insts.iter().any(|i| matches!(i,
            MInst::BinaryImm { op: BinOp::And, imm, .. } if *imm == full ^ 4095)));
        let args = [
            RuntimeValue::bits(bits, 0x1234_5678_9abc_def0),
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ];
        let expected =
            evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100)).unwrap();
        let actual = evocc_mir::lane::observe_entry(
            &mir,
            "main",
            &args,
            &[EvoIRType::Bits(bits)],
            MirLimits::new(100),
        )
        .unwrap();
        assert_eq!(actual.returns, expected.return_values[..1]);
    }
}

#[test]
fn malformed_logical_immediates_are_rejected_by_the_validator() {
    let (_, mir) = lower(&source("and", 64, 1, false));
    for invalid in [0, u64::MAX, 0x123456789abcdef0] {
        let mut changed = mir.clone();
        let instruction = changed.functions[0].blocks[0].insts.last_mut().unwrap();
        let (dst, lhs, width) = match instruction {
            MInst::Binary {
                dst, lhs, width, ..
            }
            | MInst::BinaryImm {
                dst, lhs, width, ..
            } => (*dst, *lhs, *width),
            _ => panic!("expected logical operation"),
        };
        *instruction = MInst::BinaryImm {
            op: BinOp::And,
            dst,
            lhs,
            width,
            imm: invalid,
        };
        assert!(
            evocc_mir::validate_module(&changed).is_err(),
            "accepted {invalid:x}"
        );
    }
    let (_, mut narrow) = lower(&source("and", 32, 1, false));
    let instruction = narrow.functions[0].blocks[0].insts.last_mut().unwrap();
    if let MInst::BinaryImm { imm, .. } = instruction {
        *imm = 0xffff_ffff_0000_0001;
    } else {
        panic!("expected logical immediate");
    }
    assert!(evocc_mir::validate_module(&narrow).is_err());
}

#[test]
fn every_selected_shape_assembles_for_both_targets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut text = String::new();
    for name in ["and", "or", "xor"] {
        for bits in [32, 64] {
            for mask in masks(bits) {
                for left in [false, true] {
                    text.push_str(
                        &source(name, bits, mask, left)
                            .replace("@main(", &format!("@{name}_{bits}_{mask}_{left}(")),
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
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
