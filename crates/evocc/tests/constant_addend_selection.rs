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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!(
            "../../../tests/fixtures/constant-addend-selection.c"
        ))
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn immediate_addends_keep_wraparound_and_operand_order() {
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
        let mir = evocc_mir::lower_module_for(evocc_mir::Target::Linux, &ir, &|_| false).unwrap();
        assert_eq!(
            evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000))
                .unwrap()
                .results,
            [evocc_mir::MirValue::Int(0)]
        );
        let output = run(level, "--emit=asm");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = String::from_utf8(output.stdout).unwrap();
        assert!(
            asm.lines()
                .any(|line| line.trim().starts_with("add ") && line.ends_with(", #4095")),
            "missing bounded add immediate"
        );
        assert!(
            asm.lines()
                .any(|line| line.trim().starts_with("sub ") && line.ends_with(", #19")),
            "missing bounded sub immediate"
        );
    }
}
