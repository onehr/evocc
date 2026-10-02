//! C1 RED: exercise the real driver before changing constant emission.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(flags: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(flags)
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

fn assembly(flags: &[&str], source: &str) -> String {
    let mut args = vec!["--emit=asm", "-O2"];
    args.extend_from_slice(flags);
    let output = run(&args, source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn constants(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| {
            ["movz ", "movn ", "movk ", "orr "]
                .iter()
                .any(|p| line.starts_with(p))
        })
        .collect()
}

fn without_identity(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("// evocc-native-isel="))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn high_halfword_ones_and_logical_masks_use_one_instruction() {
    for (ty, value, family, width) in [
        ("unsigned long", "0x1234000000000000UL", "movz", "x"),
        ("unsigned long", "0xffffffffffffffffUL", "movn", "x"),
        ("unsigned long", "0x00ff00ff00ff00ffUL", "orr", "x"),
        ("unsigned", "0x12340000U", "movz", "w"),
        ("unsigned", "0xffffffffU", "movn", "w"),
        ("unsigned", "0x00ff00ffU", "orr", "w"),
    ] {
        let source = format!("{ty} f(void) {{ return {value}; }}");
        let before = assembly(&[], &source);
        assert!(
            constants(&before).len() > 1,
            "existing missed opportunity: {before}"
        );
        let after = assembly(&["--isel-opt=constants"], &source);
        assert!(after.contains("// evocc-native-isel=constants\n"));
        let sequence = constants(&after);
        assert_eq!(sequence.len(), 1, "{source}\n{after}");
        assert!(
            sequence[0].starts_with(&format!("{family} {width}")),
            "{after}"
        );
    }
}

#[test]
fn equal_cost_sequences_keep_the_exact_legacy_spelling() {
    for value in ["0UL", "1UL", "0xffffUL", "0x123456789abcdef0UL"] {
        let source = format!("unsigned long f(void) {{ return {value}; }}");
        let before = assembly(&[], &source);
        let after = assembly(&["--isel-opt=constants"], &source);
        assert_eq!(without_identity(&after), before);
    }
}

#[test]
fn protected_profiles_decline_constant_selection_without_changing_code() {
    let source = "unsigned long f(void) { return 0xffffffffffffffffUL; }";
    for guard in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        let before = assembly(&[guard], source);
        let after = assembly(&[guard, "--isel-opt=constants"], source);
        assert_eq!(without_identity(&after), before, "{guard}");
    }
}

#[test]
fn driver_refuses_malformed_duplicate_and_interpreter_only_requests() {
    for flags in [
        vec!["--emit=asm", "--isel-opt="],
        vec!["--emit=asm", "--isel-opt=unknown"],
        vec!["--emit=asm", "--isel-opt=constants,constants"],
        vec!["--emit=asm", "--isel-opt=constants", "--isel-opt=constants"],
        vec!["--check", "--isel-opt=constants"],
    ] {
        assert!(!run(&flags, "int main(void) { return 0; }").status.success());
    }
}
