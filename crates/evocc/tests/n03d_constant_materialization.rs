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

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn native_sequences_preserve_flags_and_clear_upper_bits_of_writes() {
    // Run exactly the driver's emitted constant instructions with a dirty
    // destination and each NZCV combination. The wrapper uses no callee saves
    // and returns value/flags in the standard two-GPR aggregate result.
    let mut subject = String::from(".text\n");
    let mut harness = String::from("typedef struct { unsigned long value, flags; } Pair;\n");
    let mut checks = String::new();
    let mut case = 0;
    for (ty, width) in [("unsigned", 32), ("unsigned long", 64)] {
        for value in [
            0,
            1,
            u64::MAX,
            0x1234_0000_0000_0000,
            0x1234_0000,
            0x00ff_00ff_00ff_00ff,
            0xffff_4321_ffff_1234,
            0x1234_5678_9abc_def0,
        ] {
            let value = if width == 32 {
                value & 0xffff_ffff
            } else {
                value
            };
            let source = format!("{ty} f(void) {{ return ({ty})0x{value:x}UL; }}");
            let emitted = assembly(&["--isel-opt=constants"], &source);
            subject.push_str(&format!(
                ".p2align 2\n.globl _constant_probe_{case}\n_constant_probe_{case}:\n    msr nzcv, x0\n    movn x0, #0\n"
            ));
            for line in constants(&emitted) {
                let destination = line.split_whitespace().nth(1).unwrap();
                assert!(destination == "x0," || destination == "w0,", "{emitted}");
                subject.push_str(&format!("    {line}\n"));
            }
            subject.push_str("    mrs x1, nzcv\n    ret\n");
            harness.push_str(&format!(
                "extern Pair constant_probe_{case}(unsigned long);\n"
            ));
            checks.push_str(&format!(
                "{{ Pair p = constant_probe_{case}(f << 28); if (p.value != 0x{value:x}UL || p.flags != (f << 28)) return {}; }}\n", case + 1
            ));
            case += 1;
        }
    }
    harness.push_str(&format!(
        "int main(void) {{ for (unsigned long f=0; f<16; ++f) {{ {checks} }} return 0; }}\n"
    ));
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        ".evidence/codegen/n03d/native-flags-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("subject.s"), subject).unwrap();
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("clang")
        .args(["-O2", "subject.s", "harness.c", "-o", "probe"])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "retained {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("probe")).output().unwrap();
    assert!(
        output.status.success(),
        "native failure {:?}; retained {}",
        output.status,
        dir.display()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
