//! Editor snapshots keep physical source identity and the JSON byte contract.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-editor-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::create_dir(path.join("src")).unwrap();
        std::fs::write(path.join("src/local.h"), "#define LOCAL_VALUE 9\n").unwrap();
        Self(path)
    }
    fn run(&self, args: &[&str], source: &str) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .current_dir(&self.0)
            .args(["--error-format=json", "--color=never", "-nostdinc"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Err(error) = child.stdin.take().unwrap().write_all(source.as_bytes()) {
            assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        }
        child.wait_with_output().unwrap()
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
fn succeeded(output: &Output) -> String {
    let stderr = String::from_utf8(output.stderr.clone()).unwrap();
    assert!(output.status.success(), "{:?}: {stderr}", output.status);
    assert!(output.stdout.is_empty());
    stderr
}

#[test]
fn stdin_snapshot_keeps_filename_local_headers_and_original_bytes() {
    let work = Work::new();
    let original = b"#error the editor must not read or overwrite this disk source\n";
    std::fs::write(work.0.join("src/editor.c"), original).unwrap();
    let output = work.run(
        &["--source-from-stdin", "-fsyntax-only", "-Wall", "src/editor.c"],
        "#include \"local.h\"\n_Static_assert(sizeof(__FILE__) == sizeof(\"src/editor.c\"), \"source identity\");\nint f(int unused) { return LOCAL_VALUE; }\n",
    );
    let stderr = succeeded(&output);
    assert!(stderr.contains("\"code\":\"W0002\""), "{stderr}");
    assert!(stderr.contains("\"file\":\"src/editor.c\""), "{stderr}");
    assert!(stderr.contains("\"repair_id\":\"W0002.void-cast\""));
    assert_eq!(std::fs::read(work.0.join("src/editor.c")).unwrap(), original);
    assert!(!work.0.join("editor.o").exists());
    assert!(!work.0.join("editor.d").exists());
}

#[test]
fn snapshot_can_name_a_source_that_has_never_been_saved() {
    let work = Work::new();
    let stderr = succeeded(&work.run(
        &["--source-from-stdin", "-fsyntax-only", "-Wall", "src/new.c"],
        "#include \"local.h\"\nint f(int unused) { return LOCAL_VALUE; }\n",
    ));
    assert!(stderr.contains("\"file\":\"src/new.c\""), "{stderr}");
    assert!(!work.0.join("src/new.c").exists());
}

#[test]
fn snapshot_input_refuses_output_modes_multiple_inputs_and_repair_application() {
    let work = Work::new();
    let original = b"int f(int unused) { return 0; }\n";
    std::fs::write(work.0.join("src/editor.c"), original).unwrap();
    std::fs::write(work.0.join("src/other.c"), original).unwrap();
    for args in [
        vec!["-c", "src/editor.c", "-o", "forbidden.o"],
        vec!["-S", "src/editor.c", "-o", "forbidden.s"],
        vec!["-E", "src/editor.c", "-o", "forbidden.i"],
        vec!["--parse-only", "src/editor.c"],
        vec!["-fsyntax-only", "src/editor.c", "src/other.c"],
        vec!["-fsyntax-only", "-Wall", "--repair=safe", "src/editor.c"],
        vec!["-fsyntax-only", "-Wall", "--repair=review", "src/editor.c"],
        vec!["-fsyntax-only", "-MMD", "-MF", "forbidden.d", "src/editor.c"],
    ] {
        let mut command = vec!["--source-from-stdin"];
        command.extend(args);
        let output = work.run(&command, "int f(int unused) { return 1; }\n");
        assert_eq!(output.status.code(), Some(1), "{command:?}");
        assert_eq!(std::fs::read(work.0.join("src/editor.c")).unwrap(), original);
    }
    for name in ["forbidden.o", "forbidden.s", "forbidden.i", "forbidden.d"] {
        assert!(!work.0.join(name).exists(), "{name} was written");
    }
}

#[test]
fn unicode_diagnostic_and_repair_columns_are_both_utf8_bytes() {
    let work = Work::new();
    std::fs::write(
        work.0.join("src/editor.c"),
        "/* 😀 */ int f(int unused) { return 0; }\n",
    )
    .unwrap();
    let stderr = succeeded(&work.run(&["-fsyntax-only", "-Wall", "src/editor.c"], ""));
    assert!(stderr.contains("\"code\":\"W0002\""), "{stderr}");
    assert!(
        stderr.contains("\"line_start\":1,\"column_start\":22,\"line_end\":1,\"column_end\":28"),
        "{stderr}"
    );
    assert!(
        stderr.contains("\"line_start\":1,\"column_start\":31,\"line_end\":1,\"column_end\":31"),
        "{stderr}"
    );
}
