//! C2 RED through the actual driver; machine-level boundary tests complement it.
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

fn instructions(text: &str) -> usize {
    text.lines().filter(|line| line.starts_with("    ")).count()
}

fn without_identity(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("// evocc-native-isel="))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn real_driver_selects_adjacent_idioms() {
    for (source, fused, removed) in [
        (
            "unsigned long f(unsigned long x,unsigned long y){return x+(y<<3);}",
            ", lsl #3",
            1,
        ),
        (
            "unsigned f(unsigned x,unsigned y){return x^(y>>7);}",
            ", lsr #7",
            1,
        ),
        (
            "unsigned long f(unsigned long x,unsigned long y,unsigned long z){return x+y*z;}",
            "    madd ",
            1,
        ),
        (
            "unsigned long f(unsigned long x,unsigned long y,unsigned long z){return x-y*z;}",
            "    msub ",
            1,
        ),
        ("long f(long x,signed char y){return x+y;}", ", sxtb", 1),
        (
            "int f(unsigned long x,unsigned long y){return x<(y>>3);}",
            ", lsr #3",
            1,
        ),
        (
            "unsigned long f(const unsigned long *p,unsigned long i){return p[i];}",
            ", lsl #3]",
            2,
        ),
        (
            "void f(unsigned long *p,unsigned long i,unsigned long v){p[i]=v;}",
            ", lsl #3]",
            2,
        ),
    ] {
        let before = assembly(&[], source);
        let after = assembly(&["--isel-opt=local"], source);
        assert!(after.contains(fused), "{source}\n{after}");
        assert_eq!(
            instructions(&before) - instructions(&after),
            removed,
            "{source}\n{before}\n{after}"
        );
    }
}

#[test]
fn live_temporary_and_volatile_memory_are_not_erased() {
    let source =
        "unsigned long f(unsigned long x,unsigned long y){unsigned long s=y<<3;return (x+s)^s;}";
    assert_eq!(
        assembly(&[], source),
        without_identity(&assembly(&["--isel-opt=local"], source))
    );
    let source = "unsigned long f(const volatile unsigned long *p,unsigned long i){return p[i];}";
    let after = assembly(&["--isel-opt=local"], source);
    assert!(!after.contains(", lsl #3]"), "{after}");
    assert!(after.contains("    ldr "), "{after}");
}

#[test]
fn protected_and_malformed_requests_remain_closed() {
    let source = "unsigned long f(unsigned long x,unsigned long y){return x+(y<<3);}";
    for guard in [
        "-fno-omit-frame-pointer",
        "-mno-omit-leaf-frame-pointer",
        "-mgeneral-regs-only",
        "-fpatchable-function-entry=2",
    ] {
        assert_eq!(
            assembly(&[guard], source),
            without_identity(&assembly(&[guard, "--isel-opt=constants,local"], source))
        );
    }
    for flags in [
        vec!["--emit=asm", "--isel-opt=local,local"],
        vec!["--emit=asm", "--isel-opt=local,"],
        vec!["--emit=asm", "--isel-opt=local", "--isel-opt=constants"],
        vec!["--check", "--isel-opt=local"],
    ] {
        assert!(!run(&flags, source).status.success());
    }
}
