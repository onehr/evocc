use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn loop_transport_preserves_values_accesses_and_guards() {
    let source = include_str!("../../../tests/fixtures/loop-value-transport.c");
    for level in ["-O0", "-O2"] {
        for mode in [
            "--interpret-c-hir",
            "--interpret-evoir",
            "--emit=asm",
            "--emit=evoir",
        ] {
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
            child
                .stdin
                .take()
                .unwrap()
                .write_all(source.as_bytes())
                .unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(
                out.status.success(),
                "{level} {mode}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            if mode.starts_with("--interpret") {
                assert_eq!(out.stdout, b"0\n");
            }
            if mode == "--emit=evoir" {
                let ir = evocc_evoir::parse_section9_module(&out.stdout).unwrap();
                let mir =
                    evocc_mir::lower_module_for(evocc_mir::Target::Linux, &ir, &|_| false).unwrap();
                evocc_mir::validate_module(&mir).unwrap();
                assert_eq!(
                    evocc_mir::interpret_module(
                        &mir,
                        "main",
                        &[],
                        evocc_mir::MirLimits::new(100_000)
                    )
                    .unwrap()
                    .results,
                    [evocc_mir::MirValue::Int(0)]
                );
                for function in &mir.functions {
                    let after = evocc_mir::out_of_ssa(function);
                    let effects = |f: &evocc_mir::MFunction| {
                        f.blocks
                            .iter()
                            .flat_map(|b| &b.insts)
                            .filter(|i| {
                                matches!(
                                    i,
                                    evocc_mir::MInst::Load { .. } | evocc_mir::MInst::Store { .. }
                                )
                            })
                            .count()
                    };
                    assert_eq!(effects(function), effects(&after));
                    let traps = |f: &evocc_mir::MFunction| {
                        f.blocks
                            .iter()
                            .filter(|b| matches!(b.term, evocc_mir::MTerm::Brk { .. }))
                            .count()
                    };
                    assert_eq!(traps(function), traps(&after));
                    evocc_mir::verify_allocation(&evocc_mir::allocate(&after).unwrap()).unwrap();
                }
            }
        }
    }
}
