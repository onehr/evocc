use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_evoir::{parse_section9_module, render_module};

const TARGET: &str = "--target=aarch64-unknown-linux-gnu";
static NEXT_SOURCE: AtomicU64 = AtomicU64::new(0);

struct SourceFile(PathBuf);

impl SourceFile {
    fn new(label: &str, source: &str) -> Self {
        let id = NEXT_SOURCE.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("evocc-day8-{label}-{}-{id}.c", std::process::id()));
        fs::write(&path, source).expect("write Day 8 source fixture");
        Self(path)
    }

    fn path(&self) -> &str {
        self.0.to_str().expect("fixture path is UTF-8")
    }
}

impl Drop for SourceFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn evocc(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .output()
        .expect("run evocc")
}

fn successful_stdout(args: &[&str]) -> String {
    let output = evocc(args);
    assert_eq!(
        output.status.code(),
        Some(0),
        "args={args:?}, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty(), "args={args:?}");
    String::from_utf8(output.stdout).expect("evocc stdout is UTF-8")
}

#[test]
fn day8_section9_contract_is_canonical_and_complete() {
    let source = SourceFile::new(
        "section9",
        r#"
int external_counter;
static int hidden_counter;
int identity(int source_name) { return source_name; }
int zero(void) { return 0; }
int main(void) { return identity(external_counter + hidden_counter) + zero(); }
"#,
    );
    let text = successful_stdout(&[TARGET, "--emit=evoir", source.path()]);
    let parsed = parse_section9_module(text.as_bytes()).expect("CLI output is canonical Section 9");
    assert_eq!(render_module(&parsed), text);
    let lines = text.lines().collect::<Vec<_>>();

    let globals = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("global "))
        .collect::<Vec<_>>();
    assert!(!globals.is_empty(), "{text}");
    assert!(
        globals
            .iter()
            .all(|line| line.contains(" kind=") && line.contains(" visibility=")),
        "{text}"
    );
    assert!(
        globals.iter().any(|line| {
            line.starts_with("global @external_counter ")
                && line.contains(" kind=file visibility=external ")
        }),
        "{text}"
    );
    assert!(
        globals.iter().any(|line| {
            line.starts_with("global @hidden_counter ")
                && line.contains(" kind=file visibility=internal ")
        }),
        "{text}"
    );

    for (index, line) in lines.iter().enumerate() {
        if line.starts_with("func ") {
            assert!(line.contains(" cc = target_default"), "{line}");
            assert!(
                lines
                    .get(index + 1)
                    .is_some_and(|next| next.starts_with("    effects { ")),
                "missing effects clause after {line:?}\n{text}"
            );
        }
    }
    assert!(text.contains("func @identity(%arg0: "), "{text}");
    assert!(!text.contains("%source_name:"), "{text}");
    assert!(text.ends_with('\n'));
    assert!(!text.ends_with("\n\n"));
}

#[test]
fn day8_storage_calls_effects_contract_is_semantically_equal() {
    let mut failures = Vec::new();
    let mut defined_cases = 0usize;
    let mut invalid_cases = 0usize;

    let static_source = SourceFile::new(
        "static-local",
        r#"
int bump(void) { static int value = 0; value += 1; return value; }
int main(void) { return bump() * 10 + bump(); }
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        defined_cases += 1;
        let output = evocc(&[TARGET, mode, static_source.path()]);
        if output.status.code() != Some(0) || output.stdout != b"12\n" || !output.stderr.is_empty()
        {
            failures.push(format!(
                "static local via {mode}: status={:?}, stdout={:?}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let recursive_static_source = SourceFile::new(
        "recursive-static-local",
        r#"
int encode_depth(int depth) {
    static int activations = 0;
    activations += 1;
    if (depth == 0) return activations;
    return encode_depth(depth - 1) * 10 + activations;
}
int main(void) { return encode_depth(2); }
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        defined_cases += 1;
        let output = evocc(&[TARGET, mode, recursive_static_source.path()]);
        if output.status.code() != Some(0) || output.stdout != b"333\n" || !output.stderr.is_empty()
        {
            failures.push(format!(
                "static local shared by recursive activations via {mode}: status={:?}, stdout={:?}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let fnptr_source = SourceFile::new(
        "fnptr-array",
        r#"
int add1(int x) { return x + 1; }
int add2(int x) { return x + 2; }
int main(void) {
    int (*table[2])(int) = { add1, add2 };
    return table[1](40);
}
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        defined_cases += 1;
        let output = evocc(&[TARGET, mode, fnptr_source.path()]);
        if output.status.code() != Some(0) || output.stdout != b"42\n" || !output.stderr.is_empty()
        {
            failures.push(format!(
                "function pointer via {mode}: status={:?}, stdout={:?}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let recursive_fnptr_source = SourceFile::new(
        "recursive-fnptr",
        r#"
int countdown(int value) {
    int (*next)(int) = countdown;
    if (value == 0) return 0;
    return next(value - 1) + 1;
}
int main(void) { return countdown(7); }
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        defined_cases += 1;
        let output = evocc(&[TARGET, mode, recursive_fnptr_source.path()]);
        if output.status.code() != Some(0) || output.stdout != b"7\n" || !output.stderr.is_empty() {
            failures.push(format!(
                "recursive function pointer via {mode}: status={:?}, stdout={:?}, stderr={}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    let null_fnptr_source = SourceFile::new(
        "null-fnptr",
        r#"
int main(void) {
    int (*callee)(int) = 0;
    return callee(41);
}
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        invalid_cases += 1;
        let output = evocc(&[TARGET, mode, null_fnptr_source.path()]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.code() == Some(0)
            || !output.stdout.is_empty()
            || !stderr.contains("E3015")
            || !stderr.contains("call-through-null-function-pointer")
        {
            failures.push(format!(
                "null function pointer via {mode}: status={:?}, stdout={:?}, stderr={stderr}",
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
            ));
        }
    }
    let emitted = successful_stdout(&[TARGET, "--emit=evoir", fnptr_source.path()]);
    let parsed = parse_section9_module(emitted.as_bytes())
        .expect("lowered function-pointer module is canonical Section 9");
    assert_eq!(render_module(&parsed), emitted);

    let effects_source = SourceFile::new(
        "effects",
        r#"
volatile int observed;
int leaf(void) { return observed; }
int middle(void) { return leaf(); }
int main(void) { return middle(); }
"#,
    );
    let output = evocc(&[TARGET, "--emit=evoir", effects_source.path()]);
    let text = String::from_utf8_lossy(&output.stdout);
    let transitive = "effects { reads={all}, writes={}, volatile=true, atomic=false, io=false, may_trap=true, may_unwind=false }";
    if output.status.code() != Some(0)
        || !output.stderr.is_empty()
        || text.matches(transitive).count() != 3
    {
        failures.push(format!(
            "transitive effect summary: status={:?}, matches={}, stderr={}, text={text}",
            output.status.code(),
            text.matches(transitive).count(),
            String::from_utf8_lossy(&output.stderr)
        ));
    } else {
        let parsed = parse_section9_module(&output.stdout)
            .expect("lowered transitive-effects module is canonical Section 9");
        assert_eq!(render_module(&parsed).as_bytes(), output.stdout);
    }

    eprintln!(
        "day8StorageCallsEffects definedInterpreterRuns={defined_cases}, invalidInterpreterRuns={invalid_cases}"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
