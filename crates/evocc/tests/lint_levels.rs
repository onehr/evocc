//! Warning control must change diagnostics, not the compiled program.
use std::io::Write;
use std::process::{Command, Output, Stdio};

const CASES: [(&str, &str, &str); 5] = [
    (
        "unused-variable",
        "W0001",
        "int f(void) { int unused = 2; return 0; }",
    ),
    (
        "unused-parameter",
        "W0002",
        "int f(int unused) { return 0; }",
    ),
    (
        "implicit-function-declaration",
        "W0003",
        "int f(void) { return missing(); }",
    ),
    ("implicit-int", "W0004", "f(void) { return 0; }"),
    ("return-type", "W0005", "int f(int x) { if (x) return 1; }"),
];

fn run(flags: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--parse-only", "--error-format=json", "--color=never"])
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

fn expect(out: Output, code: &str, level: &str) {
    let text = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        out.status.code(),
        Some(if level == "error" { 1 } else { 0 }),
        "{text}"
    );
    assert!(out.stdout.is_empty());
    if level == "allow" {
        assert!(text.is_empty(), "{text}");
    } else {
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(
            text.starts_with(&format!("{{\"code\":\"{code}\",\"level\":\"{level}\",")),
            "{text}"
        );
        assert!(text.contains("\"primary\":true"), "{text}");
        assert!(text.contains("\"file\":\"/dev/stdin\""), "{text}");
    }
}

#[test]
fn every_lint_supports_allow_warn_and_deny() {
    for (name, code, source) in CASES {
        for (prefix, level) in [("-Wno-", "allow"), ("-W", "warning"), ("-Werror=", "error")] {
            expect(run(&[&format!("{prefix}{name}")], source), code, level);
        }
    }
}

#[test]
fn lint_defaults_follow_the_language_mode() {
    for (_, code, source) in &CASES[..2] {
        expect(run(&[], source), code, "allow");
    }
    expect(run(&["-std=c99"], CASES[2].2), "W0003", "error");
    expect(run(&["-std=gnu89"], CASES[2].2), "W0003", "warning");
    expect(run(&[], CASES[3].2), "W0004", "warning");
    expect(run(&[], CASES[4].2), "W0005", "warning");
}

#[test]
fn specific_controls_override_groups_and_caps_limit_severity() {
    let source = CASES[0].2;
    for flags in [
        vec!["-Wall"],
        vec!["-Wextra"],
        vec!["-Werror=unused-variable", "--cap-lints=warn"],
    ] {
        expect(run(&flags, source), "W0001", "warning");
    }
    for flags in [
        vec!["-Wall", "-Werror", "-Wno-unused-variable"],
        vec!["-Wno-unused-variable", "-Wall", "-Werror"],
        vec!["-Werror=unused-variable", "-w"],
        vec!["-w", "-Werror=unused-variable"],
        vec!["-Werror=unused-variable", "--cap-lints=allow"],
    ] {
        expect(run(&flags, source), "W0001", "allow");
    }
    expect(run(&["-Wall", "-Werror"], source), "W0001", "error");
    expect(
        run(&["-Wall", "--cap-lints=deny"], source),
        "W0001",
        "warning",
    );
    expect(run(&["--cap-lints=deny"], source), "W0001", "allow");
}

#[test]
fn uses_shadowing_and_unused_attributes_are_not_false_positives() {
    let source = r#"
int f(int x, int y __attribute__((unused))) {
    int z = x;
    { int x = 4; z += x; }
    int quiet __attribute__((unused)) = 2;
    (void)sizeof(z);
    return z;
}
int main(void) {}
"#;
    expect(run(&["-Wall"], source), "W0001", "allow");
    expect(run(&["-Wall"], "int f(int x);"), "W0002", "allow");
    expect(
        run(&["-Wunused-variable"], "int visible; extern int imported;"),
        "W0001",
        "allow",
    );
    expect(
        run(&["-Wunused-variable"], "static int private_value;"),
        "W0001",
        "warning",
    );
}

#[test]
fn explicit_use_and_nonreturning_calls_do_not_warn() {
    for source in [
        "static int retained __attribute__((used));",
        "extern void die(void) __attribute__((noreturn)); int f(void) { die(); }",
        "_Noreturn void die(void); int f(void) { die(); }",
    ] {
        expect(run(&["-Wall", "-Werror"], source), "W0005", "allow");
    }
}

#[test]
fn symbol_attributes_after_a_pointer_star_mark_the_declared_variable() {
    // Linux writes `static void * __used __section(...) name = &sym;`. As in
    // GCC 14, `used` there marks the declaration, and `unused` on the pointer
    // next to the name marks the variable.
    for source in [
        "extern int sym; static void * __attribute__((__used__)) kept = &sym;",
        "static int * __attribute__((unused)) quiet;",
        "static int (* __attribute__((unused)) rows)[2];",
        "int f(void) { int * __attribute__((unused)) local; return 0; }",
    ] {
        expect(run(&["-Wall", "-Werror"], source), "W0001", "allow");
    }
    // GCC 14 still warns when `unused` marks a pointer farther from the name,
    // an array element type or a parameter.
    for source in [
        "static int * __attribute__((unused)) * outer;",
        "static int * __attribute__((unused)) elements[2];",
        "int f(void) { int * __attribute__((unused)) * local; return 0; }",
    ] {
        expect(run(&["-Wall"], source), "W0001", "warning");
    }
    expect(
        run(
            &["-Wall"],
            "int f(int * __attribute__((unused)) p) { return 0; }",
        ),
        "W0002",
        "warning",
    );
}

#[test]
fn return_analysis_checks_paths_without_warning_on_complete_returns() {
    for source in [
        "int f(int x) { if (x) return 1; else return 2; }",
        "int f(void) { for (;;) {} }",
        "void f(void) {}",
        "int main(void) {}",
        "int f(int x) { switch(x) { case 1: return 2; default: return 3; } }",
        "int f(void) { goto done; done: return 2; }",
        "int f(void) { while ((unsigned __int128)-1) {} }",
        "int f(void) { do { continue; } while (1); }",
    ] {
        expect(run(&["-Wreturn-type"], source), "W0005", "allow");
    }
    for source in [
        "int f(int x) { switch(x) { case 1: return 2; } }",
        "int f(void) { for (;;) { break; } }",
        "int f(void) { goto end; return 1; end:; }",
    ] {
        expect(run(&["-Wreturn-type"], source), "W0005", "warning");
    }
}

#[test]
fn indirect_jumps_check_their_label_exits_not_a_synthetic_function_exit() {
    for source in [
        "int f(unsigned n) { static void *const table[] = {&&a, &&b}; goto *table[n & 1]; a: return 7; b: return 9; }",
        "int f(void) { void *p = &&again; again: goto *p; }",
        "int f(void) { goto *&&done; done: return 2; }",
    ] {
        expect(run(&["-Werror=return-type"], source), "W0005", "allow");
    }
    for source in [
        "int f(unsigned n) { static void *const table[] = {&&a, &&b}; goto *table[n & 1]; a: return 7; b:; }",
        "int f(void *p) { goto *p; }",
        "int f(void) { goto *({ goto end; &&ok; }); ok: return 1; end:; }",
    ] {
        expect(run(&["-Werror=return-type"], source), "W0005", "error");
    }
}

#[test]
fn denied_lint_publishes_no_object_and_warned_build_succeeds() {
    let directory = std::env::temp_dir().join(format!("evocc-lint-output-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let source = directory.join("input.c");
    let object = directory.join("output.o");
    std::fs::write(&source, CASES[0].2).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["-c", "-Werror=unused-variable", "--color=never"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!object.exists());
    let out = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["-c", "-Wunused-variable", "--color=never"])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(object.exists());
    assert!(String::from_utf8_lossy(&out.stderr).contains("warning[W0001]"));
    std::fs::remove_dir_all(directory).unwrap();
}
