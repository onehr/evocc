use std::io::Write;
use std::process::{Command, Stdio};

fn run(level: &str, mode: &str) -> std::process::Output {
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
    let _ = child.stdin.take().unwrap().write_all(include_bytes!(
        "../../../tests/fixtures/asm-value-transport.c"
    ));
    child.wait_with_output().unwrap()
}

#[test]
fn assembly_boundaries_survive_direct_edge_identity_reduction() {
    for level in ["-O0", "-O2"] {
        let out = run(level, "--emit=evoir");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let ir = evocc_evoir::parse_section9_module(&out.stdout).unwrap();
        let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &ir, &|_| false).unwrap();
        for function in &mir.functions {
            let after = evocc_mir::out_of_ssa(function);
            let observations = |f: &evocc_mir::MFunction| {
                f.blocks
                    .iter()
                    .flat_map(|b| &b.insts)
                    .filter_map(|i| {
                        if let evocc_mir::MInst::Asm(body) = i {
                            Some((
                                body.template.clone(),
                                body.volatile,
                                body.numbered()
                                    .iter()
                                    .map(|op| (op.constraint, op.fixed_register))
                                    .collect::<Vec<_>>(),
                                body.clobbers.clone(),
                            ))
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(observations(function), observations(&after));
            evocc_mir::verify_allocation(
                &evocc_mir::allocate_for(evocc_mir::Target::Linux, &after).unwrap(),
            )
            .unwrap();
        }
        let asm = run(level, "--emit=asm");
        assert!(
            asm.status.success(),
            "{}",
            String::from_utf8_lossy(&asm.stderr)
        );
        // Transport optimization does not license either interpreter to skip asm.
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let out = run(level, mode);
            assert!(!out.status.success());
            assert!(
                String::from_utf8_lossy(&out.stderr).contains("E3021"),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }
}
