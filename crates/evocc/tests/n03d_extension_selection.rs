//! Repeated native extension selection through the public C driver.
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
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn assembly(flags: &[&str], source: &str) -> String {
    let mut args = vec!["--emit=asm", "-O2", "--ub-checks=unchecked"];
    args.extend_from_slice(flags);
    let output = run(&args, source);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn repeated_narrow_arguments_share_the_existing_extension() {
    for (source, mnemonic, before_count) in [
        (
            "extern int consume(signed char,signed char,signed char); int f(signed char x){return consume(x,x,x);}",
            "    uxtb ",
            3,
        ),
        (
            "extern unsigned consume(unsigned short,unsigned short); unsigned f(unsigned short x){return consume(x,x);}",
            "    uxth ",
            2,
        ),
    ] {
        let before = assembly(&[], source);
        assert_eq!(before.matches(mnemonic).count(), before_count, "{before}");
        let after = assembly(&["--isel-opt=extensions"], source);
        assert_eq!(after.matches(mnemonic).count(), 1, "{after}");
        assert_eq!(before.matches("    bl consume").count(), 1);
        assert_eq!(after.matches("    bl consume").count(), 1);
    }
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
fn already_legal_counts_and_protected_profiles_do_not_change() {
    let source = include_str!("../../../tests/fixtures/codegen/n03d-extension-reuse.c");
    let controls = source.split("extern int consume_signed").next().unwrap();
    assert_eq!(
        assembly(&[], controls),
        without_selection_comments(&assembly(&["--isel-opt=extensions"], controls))
    );
    for guard in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        assert_eq!(
            assembly(&[guard], source),
            without_selection_comments(&assembly(&[guard, "--isel-opt=extensions"], source))
        );
    }
}

#[test]
fn option_identity_is_canonical_and_invalid_combinations_are_rejected() {
    let source = "int f(signed char x){return x;}";
    assert_eq!(
        assembly(&["--isel-opt=extensions,constants,local"], source),
        assembly(&["--isel-opt=constants,local,extensions"], source)
    );
    for flags in [
        vec!["--emit=asm", "--isel-opt=extensions,extensions"],
        vec!["--emit=asm", "--isel-opt=extensions,"],
        vec!["--emit=asm", "--isel-opt=extensions,unknown"],
        vec![
            "--emit=asm",
            "--isel-opt=extensions",
            "--isel-opt=constants",
        ],
        vec!["--check", "--isel-opt=extensions"],
    ] {
        assert!(!run(&flags, source).status.success());
    }
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
#[test]
fn repeated_narrow_arguments_execute_with_independent_linux_callees() {
    // The C driver targets Linux. Retargeting its signed narrow signatures to
    // Darwin would violate EvoMIR section 6's explicit sign-extension contract.
    // MIR-only Darwin extension execution is covered independently.
    let fixture = include_str!("../../../tests/fixtures/codegen/n03d-extension-reuse.c");
    // Disposable test outputs do not belong in retained evidence directories.
    let dir = std::env::temp_dir().join(format!("evocc-native-extension-{}", std::process::id()));
    std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
    std::fs::create_dir(&dir).unwrap();
    let names = [
        "repeated_count_three",
        "repeated_count",
        "distinct_counts",
        "repeated_signed_abi",
        "repeated_unsigned_abi",
    ];
    let mut harness = String::from("int consume_signed(signed char a,signed char b,signed char c){return 3*a+5*b+7*c;}\nunsigned consume_unsigned(unsigned short a,unsigned short b){return 3U*a+5U*b;}\n");
    for prefix in ["off", "extensions", "combined", "reference"] {
        let mut source = fixture.to_string();
        for name in names {
            source = source.replace(&format!("{name}("), &format!("{prefix}_{name}("));
        }
        harness.push_str(&format!("extern int {prefix}_repeated_signed_abi(signed char);\nextern unsigned {prefix}_repeated_unsigned_abi(unsigned short);\n"));
        if prefix == "reference" {
            std::fs::write(dir.join("reference.c"), source).unwrap();
            continue;
        }
        let flags: &[&str] = match prefix {
            "off" => &[],
            "extensions" => &["--isel-opt=extensions"],
            _ => &[
                "--isel-opt=constants,extensions",
                "--frame-opt=paired-saves,shared-epilogue,leaf-frames",
                "--regalloc-opt=copy-hints,reload-reuse",
            ],
        };
        let asm = assembly(flags, &source);
        if prefix != "off" {
            assert!(asm.contains("// evocc-local-selected"));
        }
        std::fs::write(dir.join(format!("{prefix}.s")), asm).unwrap();
    }
    harness.push_str("int main(void){for(unsigned i=0;i<65536;++i){signed char s=(signed char)i;unsigned short u=(unsigned short)i;\n");
    for prefix in ["off", "extensions", "combined"] {
        harness.push_str(&format!("if({prefix}_repeated_signed_abi(s)!=reference_repeated_signed_abi(s))return 1;\nif({prefix}_repeated_unsigned_abi(u)!=reference_repeated_unsigned_abi(u))return 2;\n"));
    }
    harness.push_str("}return 0;}\n");
    std::fs::write(dir.join("harness.c"), harness).unwrap();
    let output = Command::new("/usr/local/bin/container")
        .args(["run", "--rm", "--platform", "linux/arm64", "--network", "none", "--cpus", "2", "--memory", "3G", "--volume"])
        .arg(format!("{}:/work",dir.canonicalize().unwrap().display()))
        .args(["evo-nexlab:sealed", "sh", "-c", "set -eu; cd /work; for cc in gcc-14 clang-19; do $cc --version | head -n 1; $cc -O2 off.s extensions.s combined.s reference.c harness.c -o probe; ./probe; done"])
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
