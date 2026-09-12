mod support;

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TARGET: &str = "--target=aarch64-unknown-linux-gnu";
const WATCHDOG: Duration = Duration::from_secs(2);

const LOOP_AND_CALL_SOURCE: &str = "int add1(int x) { return x + 1; } int main(void) { int total = 0; for (int i = 0; i < 3; i++) { total += add1(i); } return total; }\n";

const LOOP_AND_CALL_EVOIR: &str = r#"func @add1(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = signed_add_defined %v0, %v3
    ub.assume.signed_overflow %v4
    %v5 = add.bits<32> %v0, %v3
    ret %v5, %v1, %v2
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 0
    %v3 = const.bits<32> 0
    br ^b1(%v3, %v2, %v0, %v1)

^b1(%v4: bits<32>, %v5: bits<32>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<32> 3
    %v9 = icmp.slt %v4, %v8
    cond_br %v9, ^b3(%v4, %v5, %v6, %v7), ^b4(%v5, %v6, %v7)

^b2(%v10: bits<32>, %v11: bits<32>, %v12: mem<all>, %v13: world):
    %v14 = const.bits<32> 1
    %v15 = signed_add_defined %v10, %v14
    ub.assume.signed_overflow %v15
    %v16 = add.bits<32> %v10, %v14
    br ^b1(%v16, %v11, %v12, %v13)

^b3(%v17: bits<32>, %v18: bits<32>, %v19: mem<all>, %v20: world):
    (%v21, %v22, %v23) = call @add1(%v17, %v19, %v20)
    %v24 = signed_add_defined %v18, %v21
    ub.assume.signed_overflow %v24
    %v25 = add.bits<32> %v18, %v21
    br ^b2(%v17, %v25, %v22, %v23)

^b4(%v26: bits<32>, %v27: mem<all>, %v28: world):
    ret %v26, %v27, %v28
}
"#;

struct TempSource {
    path: PathBuf,
}

impl TempSource {
    fn new(label: &str, source: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock is after the Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "evocc-day6-{label}-{}-{nonce}.c",
            std::process::id()
        ));
        fs::write(&path, source).expect("write temporary C source");
        Self { path }
    }

    fn as_str(&self) -> &str {
        self.path.to_str().expect("temporary path is UTF-8")
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn run(args: &[&str]) -> Output {
    support::warm_evocc();
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn evocc");
    let deadline = Instant::now() + WATCHDOG;
    loop {
        match child.try_wait().expect("poll evocc") {
            Some(_) => return child.wait_with_output().expect("collect evocc output"),
            None if Instant::now() >= deadline => {
                child.kill().expect("kill timed-out evocc");
                let output = child.wait_with_output().expect("collect timed-out output");
                panic!(
                    "evocc exceeded the two-second watchdog; args={args:?} stdout={:?} stderr={:?}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }
}

fn assert_no_panic(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("panicked at"), "panic leaked: {stderr}");
    assert!(
        !stderr.contains("stack backtrace"),
        "backtrace leaked: {stderr}"
    );
}

#[test]
fn emit_evoir_prints_exact_section_9_text_for_loop_and_call() {
    let source = TempSource::new("emit", LOOP_AND_CALL_SOURCE);
    let output = run(&[TARGET, "--emit=evoir", source.as_str()]);

    assert_no_panic(&output);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, LOOP_AND_CALL_EVOIR.as_bytes());
    assert_eq!(output.stderr, b"");
}

#[test]
fn interpret_evoir_executes_the_lowered_loop_and_call() {
    let source = TempSource::new("interpret", LOOP_AND_CALL_SOURCE);
    let chir = run(&[TARGET, "--interpret-c-hir", source.as_str()]);
    let evoir = run(&[TARGET, "--interpret-evoir", source.as_str()]);

    for output in [&chir, &evoir] {
        assert_no_panic(output);
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, b"6\n");
        assert_eq!(output.stderr, b"");
    }
    assert_eq!(chir.stdout, evoir.stdout);
}

#[test]
fn exact_evoir_capability_probes_are_green_in_named_and_flag_forms() {
    for args in [
        &["evoir-emit", "--probe"][..],
        &["evoir-interpret", "--probe"][..],
        &["--emit=evoir", "--probe"][..],
        &["--interpret-evoir", "--probe"][..],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(0), "args={args:?}");
        assert_eq!(output.stdout, b"", "args={args:?}");
        assert_eq!(output.stderr, b"", "args={args:?}");
    }
}

#[test]
fn near_miss_combined_probes_and_conflicting_modes_fail_closed() {
    for args in [
        &["--emit=evoirXYZ", "--probe"][..],
        &["--interpret-evoirXYZ", "--probe"][..],
        &["evoir-emit", "evoir-interpret", "--probe"][..],
        &["--emit=evoir", "--interpret-evoir", "--probe"][..],
    ] {
        let output = run(args);
        assert_no_panic(&output);
        assert_eq!(output.status.code(), Some(69), "args={args:?}");
        assert_eq!(output.stdout, b"", "args={args:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("NOT_IMPLEMENTED"),
            "args={args:?}, stderr={:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let source = TempSource::new("conflicting-modes", LOOP_AND_CALL_SOURCE);
    let args = &[TARGET, "--emit=evoir", "--interpret-evoir", source.as_str()];
    let output = run(args);
    assert_no_panic(&output);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("exactly one"),
        "args={args:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn broad_evoir_and_unrelated_capability_probes_remain_not_implemented() {
    for capability in [
        "parse",
        "evoir",
        "eqsat",
        "codegen",
        "kernel-build",
        "preprocess",
        "parse-only",
        "interpret-c-hir",
        "c-hir-emit",
        "eqsat-optimize",
    ] {
        let args = &[capability, "--probe"];
        let output = run(args);
        assert_eq!(output.status.code(), Some(69), "args={args:?}");
        assert_eq!(output.stdout, b"", "args={args:?}");
        assert_eq!(
            String::from_utf8(output.stderr).expect("probe stderr is UTF-8"),
            format!("evocc: capability `{capability}` is NOT_IMPLEMENTED\n"),
            "args={args:?}"
        );
    }
}

#[test]
fn computed_goto_is_emitted_and_executed_through_the_public_driver() {
    let source = TempSource::new("computed", "int main(void){goto *&&end;end:return 3;}\n");
    let emitted = run(&[TARGET, "--emit=evoir", source.as_str()]);
    assert_no_panic(&emitted);
    assert_eq!(emitted.status.code(), Some(0));
    assert!(emitted.stderr.is_empty());
    assert!(String::from_utf8(emitted.stdout)
        .unwrap()
        .contains("br.indirect"));
    let executed = run(&[TARGET, "--interpret-evoir", source.as_str()]);
    assert_no_panic(&executed);
    assert_eq!(executed.status.code(), Some(0));
    assert!(executed.stderr.is_empty());
    assert_eq!(executed.stdout, b"3\n");
}

#[test]
fn malformed_lowering_and_runtime_failures_are_diagnostics_not_panics() {
    let malformed = TempSource::new("malformed", "int +;\n");
    let lowering = TempSource::new(
        "lowering",
        "struct S { double x; }; int f(int, ...); int pass(struct S s) { return f(1, s); }\n",
    );
    let runtime = TempSource::new(
        "runtime",
        "int main(void) { int left = 7; int zero = 0; return left / zero; }\n",
    );
    let noncanonical_main =
        TempSource::new("noncanonical-main", "int main(int argc) { return argc; }\n");
    let void_main = TempSource::new("void-main", "void main(void) { return; }\n");
    let fuel = TempSource::new("fuel", "int main(void) { while (1) {} }\n");

    for (kind, output) in [
        (
            "malformed",
            run(&[TARGET, "--emit=evoir", malformed.as_str()]),
        ),
        (
            "lowering",
            run(&[TARGET, "--emit=evoir", lowering.as_str()]),
        ),
        (
            "runtime",
            run(&[TARGET, "--interpret-evoir", runtime.as_str()]),
        ),
        (
            "noncanonical-main",
            run(&[TARGET, "--interpret-evoir", noncanonical_main.as_str()]),
        ),
        (
            "void-main",
            run(&[TARGET, "--interpret-evoir", void_main.as_str()]),
        ),
        ("fuel", run(&[TARGET, "--interpret-evoir", fuel.as_str()])),
    ] {
        assert_no_panic(&output);
        assert_eq!(output.status.code(), Some(1), "{kind}");
        assert_eq!(output.stdout, b"", "{kind}");
        let stderr = String::from_utf8(output.stderr).expect("diagnostic is UTF-8");
        assert!(stderr.contains("error[E"), "{kind}: {stderr}");
        match kind {
            "malformed" => assert!(
                stderr.ends_with(":1:5: error[E1001]: expected-declaration\n"),
                "{stderr}"
            ),
            "lowering" => assert!(
                stderr.ends_with("error[E3030]: evoir-lowering-failed: not-lowered: varargs\n"),
                "{stderr}"
            ),
            "runtime" => assert!(
                // EvoIR retains typed block/value provenance, not a C source
                // span. The driver therefore uses its stable input-level
                // fallback while preserving the exact typed diagnostic.
                stderr.ends_with(":1:1: error[E3002]: integer-division-by-zero\n"),
                "{stderr}"
            ),
            "noncanonical-main" => assert!(
                stderr.ends_with(":1:1: error[E3007]: evoir-main-parameters-are-not-canonical\n"),
                "{stderr}"
            ),
            "void-main" => assert!(
                stderr.ends_with(":1:1: error[E3007]: evoir-main-results-are-not-canonical\n"),
                "{stderr}"
            ),
            "fuel" => assert!(
                stderr.ends_with(":1:1: error[E3008]: interpreter-fuel-exhausted\n"),
                "{stderr}"
            ),
            _ => unreachable!("fixed diagnostic table"),
        }
    }
}
