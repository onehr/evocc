use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(options: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(options)
        .arg("/dev/stdin")
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
    child.wait_with_output().unwrap()
}

#[test]
fn effective_rounding_option_selects_executed_semantics() {
    let source = "double f(void) { return 1.0 + 0x1p-53; }";
    for optimize in ["-O0", "-O2"] {
        for (flags, strict) in [
            (vec!["-frounding-math"], true),
            (vec!["-frounding-math", "-fno-rounding-math"], false),
            (vec!["-fno-rounding-math", "-frounding-math"], true),
        ] {
            let mut options = vec!["--emit=evoir", optimize];
            options.extend(flags);
            let output = run(&options, source);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert_eq!(text.contains("fp.strict.add"), strict, "{text}");
        }
    }
}

#[test]
fn rounding_support_does_not_silently_enable_fenv_or_fast_math() {
    for (options, source, diagnostic) in [
        (
            vec!["-frounding-math", "--emit=evoir"],
            "#pragma STDC FENV_ACCESS ON\ndouble f(void){return 1.0;}",
            "E3065",
        ),
        (
            vec!["-frounding-math", "-ffast-math", "--emit=evoir"],
            "double f(void){return 1.0;}",
            "unsupported",
        ),
    ] {
        let output = run(&options, source);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
