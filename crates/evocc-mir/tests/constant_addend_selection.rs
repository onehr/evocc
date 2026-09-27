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
fn small_known_addends_select_existing_immediates_with_exact_wraparound() {
    for (name, op, left) in [
        ("add", BinOp::Add, false),
        ("add", BinOp::Add, true),
        ("sub", BinOp::Sub, false),
    ] {
        for bits in [32, 64] {
            for immediate in [0, 1, 19, 4095] {
                let text = source(name, bits, immediate, left);
                let (ir, mir) = lower(&text);
                assert!(mir.functions[0].blocks[0].insts.iter().any(|i| matches!(i,
                    MInst::BinaryImm { op: actual, imm, .. } if *actual == op && *imm == immediate)), "{text}");
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

#[test]
fn nonmatching_operands_do_not_commute_subtraction_or_widen_the_scope() {
    for (op, bits, value, left) in [
        ("sub", 64, 19, true),
        ("add", 32, 4096, false),
        ("sub", 64, u64::MAX, false),
        ("add", 8, 19, false),
        ("add", 128, 19, false),
        ("mul", 64, 19, false),
        ("and", 64, 19, false),
    ] {
        let (_, mir) = lower(&source(op, bits, value, left));
        assert!(!mir.functions[0].blocks[0]
            .insts
            .iter()
            .any(|i| matches!(i, MInst::BinaryImm { .. })));
    }
    let dynamic = source("add", 64, 19, false)
        .replace("    %v3 = const.bits<64> 19\n", "")
        .replace("%v0, %v3", "%v0, %v0");
    let (_, mir) = lower(&dynamic);
    assert!(mir.functions[0].blocks[0]
        .insts
        .iter()
        .any(|i| matches!(i, MInst::Binary { op: BinOp::Add, .. })));
}

#[test]
fn original_signed_overflow_obligations_still_trap() {
    for bits in [32, 64] {
        let text = source("add", bits, 1, false).replace("may_trap=false", "may_trap=true")
            .replace("    %v4 =", "    %v5 = signed_add_defined %v0, %v3\n    ub.assume.signed_overflow %v5\n    %v4 =");
        let (_, mir) = lower(&text);
        let result = evocc_mir::interpret_module(
            &mir,
            "main",
            &[evocc_mir::MirValue::Int((1_u64 << (bits - 1)) - 1)],
            MirLimits::new(100),
        );
        assert_eq!(result.unwrap_err(), evocc_mir::MirError::Trap { imm: 3004 });
    }
}

#[test]
fn immediate_forms_assemble_on_both_targets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut text = String::new();
    for (name, left) in [("add", false), ("add", true), ("sub", false)] {
        for bits in [32, 64] {
            for value in [0, 1, 19, 4095] {
                text.push_str(
                    &source(name, bits, value, left)
                        .replace("@main(", &format!("@{name}_{left}_{bits}_{value}(")),
                );
                text.push('\n');
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
