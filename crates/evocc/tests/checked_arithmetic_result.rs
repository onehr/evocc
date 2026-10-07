use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, level: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            level,
            mode,
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn checked_arithmetic_preserves_edges_values_and_volatile_reads() {
    let source = include_str!("../../../tests/fixtures/checked-arithmetic-result.c");
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, level, mode);
            assert!(
                out.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
        let out = run(source, level, "--emit=evoir");
        assert!(out.status.success());
        let ir = evocc_evoir::parse_section9_module(&out.stdout).unwrap();
        let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &ir, &|_| false).unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        for name in ["add32", "sub64", "twice", "read_once"] {
            let function = mir.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.insts)
                    .all(|inst| !matches!(
                        inst,
                        evocc_mir::MInst::Binary {
                            op: evocc_mir::BinOp::Add | evocc_mir::BinOp::Sub,
                            ..
                        }
                    )),
                "{level} {name}"
            );
        }
        let reads = mir
            .functions
            .iter()
            .find(|f| f.name == "read_once")
            .unwrap()
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .filter(|inst| matches!(inst, evocc_mir::MInst::Load { volatile: true, .. }))
            .count();
        assert_eq!(reads, 2, "both volatile reads must remain");
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000))
                .unwrap()
                .results,
            [evocc_mir::MirValue::Int(0)]
        );
    }
}

#[test]
fn checked_arithmetic_retains_overflow_and_uninitialized_errors() {
    for (source, code) in [
        (
            "int main(void){volatile int a=2147483647;return a+1;}",
            "E3004",
        ),
        (
            "int main(void){volatile long a=-9223372036854775807L-1;return a-1;}",
            "E3004",
        ),
        ("int main(void){volatile int a;return a+1;}", "E3001"),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = run(source, "-O2", mode);
            assert_eq!(out.status.code(), Some(1));
            assert!(
                String::from_utf8_lossy(&out.stderr).contains(code),
                "{mode}: {:?}",
                out.stderr
            );
        }
    }
}

#[test]
fn direct_flag_guards_preserve_small_c_execution_and_volatile_accesses() {
    let source = include_str!("../../../tests/fixtures/checked-flag-branch.c");
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
            let out = run(source, level, mode);
            assert!(
                out.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
        }
        let out = run(source, level, "--emit=evoir");
        assert!(out.status.success());
        let ir = evocc_evoir::parse_section9_module(&out.stdout).unwrap();
        let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &ir, &|_| false).unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        for name in ["sum", "difference", "product", "shift", "observed"] {
            let function = mir.functions.iter().find(|f| f.name == name).unwrap();
            assert!(
                function
                    .blocks
                    .iter()
                    .any(|b| matches!(b.term, evocc_mir::MTerm::BCond { .. })),
                "{name}"
            );
            assert!(
                function
                    .blocks
                    .iter()
                    .flat_map(|b| &b.insts)
                    .all(|i| !matches!(i, evocc_mir::MInst::Cset { .. })),
                "{name}"
            );
        }
        let reads = mir
            .functions
            .iter()
            .find(|f| f.name == "observed")
            .unwrap()
            .blocks
            .iter()
            .flat_map(|b| &b.insts)
            .filter(|i| matches!(i, evocc_mir::MInst::Load { volatile: true, .. }))
            .count();
        assert_eq!(reads, 2);
        for function in &mir.functions {
            let allocated = evocc_mir::allocate(&evocc_mir::out_of_ssa(function)).unwrap();
            evocc_mir::verify_allocation(&allocated).unwrap();
        }
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000))
                .unwrap()
                .results,
            [evocc_mir::MirValue::Int(0)]
        );
    }
}

#[test]
fn direct_flag_guards_keep_all_original_error_kinds() {
    let source = include_str!("../../../tests/fixtures/checked-flag-branch.c");
    for (define, code) in [
        ("TRAP_ADD", "E3004"),
        ("TRAP_SUB", "E3004"),
        ("TRAP_MUL", "E3004"),
        ("TRAP_SHIFT", "E3005"),
        ("TRAP_DIV", "E3004"),
    ] {
        for level in ["-O0", "-O2"] {
            for mode in ["--interpret-c-hir", "--interpret-evoir"] {
                let out = run(&format!("#define {define}\n{source}"), level, mode);
                assert_eq!(out.status.code(), Some(1), "{define} {mode}");
                assert!(
                    String::from_utf8_lossy(&out.stderr).contains(code),
                    "{define} {mode}: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    }
}
