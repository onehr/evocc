//! The public C path must select real C4 shapes, not just accept the option.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const SOURCE: &str = include_str!("../../../tests/fixtures/codegen/n03d-control-selection.c");

fn run(flags: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(flags)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn assembly(flags: &[&str], source: &str) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let result = run(&args, source);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

fn without_selection_comments(text: &str) -> String {
    text.lines()
        .filter(|line| {
            !line.starts_with("// evocc-native-isel=") && !line.starts_with("// evocc-local-")
        })
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn late_branch_and_case_ranges_use_existing_direct_flag_branches() {
    let before = assembly(&[], SOURCE);
    assert_eq!(before.matches("    cset ").count(), 5, "{before}");
    let after = assembly(&["--isel-opt=controls"], SOURCE);
    assert_eq!(after.matches("    cset ").count(), 0, "{after}");
    assert_eq!(
        after
            .matches("// evocc-local-selected control_boolean_branch=1")
            .count(),
        3
    );
    assert_eq!(
        after
            .matches("// evocc-local-selected control_constant_interval=1")
            .count(),
        2
    );
    assert_eq!(before.matches("    bl inspect").count(), 1);
    assert_eq!(after.matches("    bl inspect").count(), 1);
}

#[test]
fn protected_profiles_suppress_every_control_transform() {
    for guard in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        assert_eq!(
            assembly(&[guard], SOURCE),
            without_selection_comments(&assembly(&[guard, "--isel-opt=controls"], SOURCE))
        );
    }
    let no_pattern = "unsigned long identity(unsigned long x){return x;}";
    assert_eq!(
        assembly(&[], no_pattern),
        without_selection_comments(&assembly(&["--isel-opt=controls"], no_pattern))
    );
}

#[test]
fn control_options_are_canonical_and_native_only() {
    assert_eq!(
        assembly(&["--isel-opt=controls,extensions,local,constants"], SOURCE),
        assembly(&["--isel-opt=constants,local,extensions,controls"], SOURCE)
    );
    for flags in [
        vec!["--emit=asm", "--isel-opt=controls,controls"],
        vec!["--emit=asm", "--isel-opt=controls,"],
        vec!["--emit=asm", "--isel-opt=controls,unknown"],
        vec!["--emit=asm", "--isel-opt=controls", "--isel-opt=constants"],
        vec!["--check", "--isel-opt=controls"],
    ] {
        assert!(!run(&flags, SOURCE).status.success());
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn selected_c_shapes_execute_against_gcc_and_clang_on_linux() {
    // Disposable test outputs do not belong in retained evidence directories.
    let dir = std::env::temp_dir().join(format!("evocc-native-control-{}", std::process::id()));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let mut harness = String::from("unsigned inspect(unsigned x){return x&1U;}\n");
    for prefix in ["off", "controls", "combined", "reference"] {
        let mut source = SOURCE.to_string();
        for name in ["late_branch", "interval_signed", "interval_unsigned"] {
            source = source.replace(&format!("{name}("), &format!("{prefix}_{name}("));
        }
        harness.push_str(&format!("extern unsigned {prefix}_late_branch(unsigned);\nextern unsigned {prefix}_interval_signed(int);\nextern unsigned {prefix}_interval_unsigned(unsigned long);\n"));
        if prefix == "reference" {
            std::fs::write(dir.join("reference.c"), source).unwrap();
            continue;
        }
        let flags: &[&str] = match prefix {
            "off" => &[],
            "controls" => &["--isel-opt=controls"],
            _ => &[
                "--isel-opt=constants,extensions,controls",
                "--frame-opt=paired-saves,shared-epilogue,leaf-frames",
                "--regalloc-opt=copy-hints,reload-reuse",
            ],
        };
        let asm = assembly(flags, &source);
        if prefix != "off" {
            assert!(asm.contains("// evocc-local-selected control_constant_interval=1"));
            assert!(!asm.contains("    cset "));
        }
        std::fs::write(dir.join(format!("{prefix}.s")), asm).unwrap();
    }
    harness.push_str("int main(void){unsigned long random=~0UL;for(unsigned i=0;i<65536;++i){unsigned long x=i<8192?i:random;int s=(int)x;\n");
    for prefix in ["off", "controls", "combined"] {
        harness.push_str(&format!("if({prefix}_late_branch((unsigned)x)!=reference_late_branch((unsigned)x))return 1;\nif({prefix}_interval_signed(s)!=reference_interval_signed(s))return 2;\nif({prefix}_interval_unsigned(x)!=reference_interval_unsigned(x))return 3;\n"));
    }
    harness.push_str("random=random*6364136223846793005UL+1UL;}return 0;}\n");
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("/usr/local/bin/container")
        .args(["run", "--rm", "--platform", "linux/arm64", "--network", "none", "--cpus", "2", "--memory", "3G", "--volume"])
        .arg(format!("{}:/work", dir.canonicalize().unwrap().display()))
        .args(["evo-nexlab:sealed", "sh", "-c", "set -eu; cd /work; for cc in gcc-14 clang-19; do $cc --version | head -n 1; $cc -O2 off.s controls.s combined.s reference.c harness.c -o probe; ./probe; done"])
        .output().unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
