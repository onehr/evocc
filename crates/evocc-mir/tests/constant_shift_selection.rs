use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{BinOp, MInst, MirLimits, Target};
use evocc_types::{EvoIRType, Region};

fn source(op: &str, bits: u16, count_bits: u16, count: u64) -> String {
    format!("func @main(%arg0: bits<{bits}>, %arg1: mem<all>, %arg2: world) -> (bits<{bits}>, mem<all>, world) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<{bits}>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<{count_bits}> {count}
    %v4 = {op}.bits<{bits}> %v0, %v3
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
    for function in &mir.functions {
        evocc_mir::verify_allocation(
            &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(function)).unwrap(),
        )
        .unwrap();
    }
    (ir, mir)
}

#[test]
fn defined_constant_counts_use_existing_machine_immediates() {
    for (name, op) in [
        ("shl", BinOp::Lsl),
        ("lshr", BinOp::Lsr),
        ("ashr", BinOp::Asr),
    ] {
        for bits in [1, 8, 13, 32, 64] {
            for count_bits in [8, 32, 64] {
                for count in [0, bits - 1] {
                    let text = source(name, bits, count_bits, u64::from(count));
                    let (ir, mir) = lower(&text);
                    assert!(mir.functions[0].blocks[0].insts.iter().any(|inst| matches!(inst,
                        MInst::BinaryImm { op: actual, imm, .. } if *actual == op && *imm == u64::from(count))), "{text}");
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
fn out_of_range_and_dynamic_counts_keep_the_existing_register_path() {
    for name in ["shl", "lshr", "ashr"] {
        for bits in [8, 32, 64] {
            for count in [u64::from(bits), 128, u64::MAX] {
                let (_, mir) = lower(&source(name, bits, 64, count));
                assert!(mir.functions[0].blocks[0].insts.iter().any(|inst| matches!(
                    inst,
                    MInst::Binary {
                        op: BinOp::Lsl | BinOp::Lsr | BinOp::Asr,
                        ..
                    }
                )));
            }
            let text = source(name, bits, bits, 1)
                .replace(&format!("    %v3 = const.bits<{bits}> 1\n"), "")
                .replace("%v0, %v3", "%v0, %v0");
            let (_, mir) = lower(&text);
            assert!(mir.functions[0].blocks[0].insts.iter().any(|inst| matches!(
                inst,
                MInst::Binary {
                    op: BinOp::Lsl | BinOp::Lsr | BinOp::Asr,
                    ..
                }
            )));
        }
    }
}

#[test]
fn an_invalid_source_count_still_reaches_its_original_trap() {
    for (bits, count_bits, count, signedness) in [
        (32, 64, 32, "unsigned"),
        (64, 64, u64::MAX, "signed"),
        (32, 1, 1, "signed"),
        (32, 64, 0x1_0000_0000, "unsigned"),
    ] {
        let text = source("shl", bits, count_bits, count)
            .replace("may_trap=false", "may_trap=true")
            .replace("    %v4 =", &format!("    %v5 = shift_count_defined.{signedness} %v0, %v3\n    ub.assume.invalid_shift_count %v5\n    %v4 ="));
        let (_, mir) = lower(&text);
        assert_eq!(
            evocc_mir::interpret_module(
                &mir,
                "main",
                &[evocc_mir::MirValue::Int(1)],
                MirLimits::new(100)
            )
            .unwrap_err(),
            evocc_mir::MirError::Trap { imm: 3005 }
        );
    }
}

#[test]
fn constant_shift_encodings_assemble_for_both_platforms() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("SKIP assembler check: clang is absent");
        return;
    }
    let mut text = String::new();
    for op in ["shl", "lshr", "ashr"] {
        for bits in [1, 8, 13, 32, 64] {
            for count in [0, bits - 1].into_iter().enumerate() {
                text.push_str(
                    &source(op, bits, 64, u64::from(count.1))
                        .replace("@main(", &format!("@{op}_{bits}_{}(", count.0)),
                );
                text.push('\n');
            }
        }
    }
    text.pop(); // Canonical modules have one final newline, not a blank line.
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
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
