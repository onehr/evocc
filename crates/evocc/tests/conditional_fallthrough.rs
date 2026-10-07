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
        "../../../tests/fixtures/conditional-fallthrough.c"
    ));
    child.wait_with_output().unwrap()
}

#[test]
fn fallthrough_selection_preserves_values_effects_backedges_and_addressed_labels() {
    use evocc_mir::{MirLimits, MirValue, Target};
    for level in ["-O0", "-O2"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(level, mode);
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
        let output = run(level, "--emit=evoir");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let ir = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        evocc_evoir::validate_module(&ir).unwrap();
        let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
        evocc_mir::validate_module(&mir).unwrap();
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], MirLimits::new(100_000))
                .unwrap()
                .results,
            [MirValue::Int(0)]
        );
        for f in &mir.functions {
            evocc_mir::verify_allocation(
                &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(f)).unwrap(),
            )
            .unwrap();
        }
        let assembly = run(level, "--emit=asm");
        assert!(
            assembly.status.success(),
            "{}",
            String::from_utf8_lossy(&assembly.stderr)
        );
        let text = String::from_utf8(assembly.stdout).unwrap();
        let lines: Vec<_> = text.lines().collect();
        for window in lines.windows(3) {
            if window[0].trim().starts_with("b.") && window[1].trim().starts_with("b ") {
                let label = window[0].split_whitespace().last().unwrap();
                assert_ne!(
                    window[2],
                    format!("{label}:"),
                    "redundant adjacent taken edge"
                );
            }
        }
    }
}
