use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

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

    let static_source = SourceFile::new(
        "static-local",
        r#"
int bump(void) { static int value = 0; value += 1; return value; }
int main(void) { return bump() * 10 + bump(); }
"#,
    );
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
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
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
