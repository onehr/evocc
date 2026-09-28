use evocc_evoir::{parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{MInst, MirLimits, Target, Width};
use evocc_types::{EvoIRType, Region};

const FUNCTION_POINTER: &str = "fnptr<(mem<all>, world)->(bits<1>, mem<all>, world)>";
const PURE: &str = "effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }";

fn source(kind: &str, op: &str, left: bool) -> String {
    let ty = if kind.starts_with("function") {
        FUNCTION_POINTER
    } else {
        "ptr<generic>"
    };
    let definition = match kind {
        "null" | "function-null" => format!("%v2 = ptr.null : {ty}"),
        "object" => "(%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=4, align=4]".into(),
        "exposed" => "%v3 = const.bits<64> 37\n    %v2 = ptr.from_int.ptr<generic> %v3".into(),
        "function" => format!("%v2 = func_addr @helper : {ty}"),
        _ => unreachable!(),
    };
    let effects = if kind == "object" {
        PURE.replace("reads={}", "reads={all}")
            .replace("writes={}", "writes={all}")
            .replace("may_trap=false", "may_trap=true")
    } else {
        PURE.into()
    };
    let tokens = if kind == "object" {
        "%v3, %v4"
    } else {
        "%v0, %v1"
    };
    let operands = if left { "%v10, %v2" } else { "%v2, %v10" };
    format!("func @main(%arg0: mem<all>, %arg1: world) -> (bits<1>, mem<all>, world) cc = target_default
    {effects}
{{
^entry(%v0: mem<all>, %v1: world):
    {definition}
    %v10 = ptr.null : {ty}
    %v11 = icmp.{op} {operands}
    ret %v11, {tokens}
}}

func @helper(%arg0: mem<all>, %arg1: world) -> (bits<1>, mem<all>, world) cc = target_default
    {PURE}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<1> 0
    ret %v2, %v0, %v1
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
fn direct_null_selects_immediate_without_losing_pointer_identity() {
    for kind in ["null", "object", "exposed", "function", "function-null"] {
        for op in ["eq", "ne"] {
            for left in [false, true] {
                let text = source(kind, op, left);
                let (ir, mir) = lower(&text);
                let args = [
                    RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                    RuntimeValue::new(EvoIRType::World, 0),
                ];
                let expected =
                    evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100))
                        .unwrap();
                let actual = evocc_mir::lane::observe_entry(
                    &mir,
                    "main",
                    &args,
                    &[EvoIRType::Bits(1)],
                    MirLimits::new(100),
                )
                .unwrap();
                assert_eq!(actual.returns, expected.return_values[..1], "{text}");
                let is_null = matches!(kind, "null" | "function-null");
                assert_eq!(
                    actual.returns[0].raw_bits,
                    u64::from(is_null == (op == "eq")),
                    "{text}"
                );
                assert!(
                    mir.functions[0].blocks[0].insts.iter().any(|i| matches!(
                        i,
                        MInst::CmpImm {
                            imm: 0,
                            width: Width::X,
                            ..
                        }
                    )),
                    "{text}"
                );
            }
        }
    }
}

#[test]
fn zero_integer_reconstruction_does_not_supply_a_null_definition_fact() {
    // No inference from address bits, symbolic addresses or casts is needed
    // for this selection. The existing reconstruction/provenance path stays.
    let text = source("exposed", "eq", false).replace(
        "    %v10 = ptr.null : ptr<generic>",
        "    %v9 = const.bits<64> 0\n    %v10 = ptr.from_int.ptr<generic> %v9",
    );
    let (_, mir) = lower(&text);
    assert!(mir.functions[0].blocks[0]
        .insts
        .iter()
        .any(|i| matches!(i, MInst::Cmp { .. })));
    assert!(!mir.functions[0].blocks[0]
        .insts
        .iter()
        .any(|i| matches!(i, MInst::CmpImm { .. })));
}

#[test]
fn null_comparison_keeps_the_existing_branch_flags_consumer() {
    let text = source("object", "ne", true).replace(
        "    ret %v11, %v3, %v4",
        "    cond_br %v11, ^b1(%v3, %v4), ^b2(%v3, %v4)\n\n^b1(%v12: mem<all>, %v13: world):\n    %v14 = const.bits<1> 1\n    ret %v14, %v12, %v13\n\n^b2(%v15: mem<all>, %v16: world):\n    %v17 = const.bits<1> 0\n    ret %v17, %v15, %v16",
    );
    let (_, mir) = lower(&text);
    let entry = &mir.functions[0].blocks[0];
    assert!(matches!(
        entry.insts.last(),
        Some(MInst::CmpImm {
            imm: 0,
            width: Width::X,
            ..
        })
    ));
    assert!(!entry.insts.iter().any(|i| matches!(i, MInst::Cset { .. })));
}

#[test]
fn null_comparison_shapes_assemble_for_both_targets() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut text = String::new();
    let mut origin = 0;
    for kind in ["null", "object", "exposed", "function", "function-null"] {
        for op in ["eq", "ne"] {
            for left in [false, true] {
                let suffix = format!("{}_{}_{}", kind.replace('-', "_"), op, left);
                text.push_str(
                    &source(kind, op, left)
                        .replace("@main", &format!("@main_{suffix}"))
                        .replace("@helper", &format!("@helper_{suffix}"))
                        .replace("[origin=0,", &format!("[origin={origin},")),
                );
                origin += 1;
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
