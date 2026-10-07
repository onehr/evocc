//! Public diagnostic format checks. These tests do not change acceptance.
use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(options: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(options)
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn default_text_has_a_source_snippet_and_primary_label() {
    let output = run(
        &["--parse-only"],
        "int main(void) {\n    return missing;\n}\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.starts_with("error[E2001]:"), "{text}");
    assert!(text.contains("--> /dev/stdin:2:12"), "{text}");
    assert!(text.contains("2 |     return missing;"), "{text}");
    assert!(text.contains("^^^^^^^"), "{text}");
    assert!(
        !text.contains('\u{1b}'),
        "redirected auto output has no color"
    );
}

#[test]
fn short_keeps_the_single_line_channel() {
    let output = run(
        &["--parse-only", "--error-format=short"],
        "int main(void) { return missing; }",
    );
    let text = String::from_utf8(output.stderr).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(text.lines().count(), 1, "{text}");
    assert!(text.starts_with("/dev/stdin:1:25: error[E2001]:"), "{text}");
}

#[test]
fn empty_invocation_has_a_primary_command_span() {
    let output = Command::new(env!("CARGO_BIN_EXE_evocc")).output().unwrap();
    assert_eq!(output.status.code(), Some(69));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.contains("error: unrecognized invocation is NOT_IMPLEMENTED"),
        "{text}"
    );
    assert!(text.contains("--> <command-line>:1:1"), "{text}");
    assert!(text.contains('^'), "{text}");
}

#[test]
fn unsupported_capability_uses_the_diagnostic_renderer() {
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["codegen", "--probe"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(69));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.starts_with("error: capability `codegen` is NOT_IMPLEMENTED"),
        "{text}"
    );
    assert!(text.contains("--> <command-line>:1:1"), "{text}");
}

#[test]
fn malformed_probe_honors_the_json_channel() {
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["unknown", "--error-format=json", "--probe"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(69));
    let program = format!(
        "{}\nimport sys\nrows=[json.loads(line) for line in sys.stdin]\nassert len(rows)==1\nvalidate(rows[0])\nassert rows[0]['code'] is None\n",
        include_str!("../../../tests/ui/check/validate.py")
    );
    let mut validator = Command::new("python3")
        .args(["-c", &program])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    validator
        .stdin
        .take()
        .unwrap()
        .write_all(&output.stderr)
        .unwrap();
    assert!(
        validator.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn missing_input_marks_its_argument_not_the_complete_invocation() {
    for name in ["missing input.c", "missing\ninput.c", "missing-λ.c"] {
        let input = std::env::temp_dir().join(format!("evocc-{}-{name}", std::process::id()));
        assert!(!input.exists());
        let input = input.to_str().unwrap();
        let prefix = "--parse-only --error-format=json --color=never ";
        let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args([
                "--parse-only",
                "--error-format=json",
                "--color=never",
                input,
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        let program = format!(
            "{}\nimport sys\nr=json.load(sys.stdin)\nvalidate(r)\ns=r['spans'][0]\nassert r['code']=='E0003'\nassert s['file']=='<command-line>'\nassert s['column_start']==int(sys.argv[1]), s\nassert s['line_end']==int(sys.argv[2]), s\nassert s['column_end']==int(sys.argv[3]), s\n",
            include_str!("../../../tests/ui/check/validate.py")
        );
        let line_end = input.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column_end = match input.rsplit_once('\n') {
            Some((_, tail)) => tail.len() + 1,
            None => prefix.len() + input.len() + 1,
        };
        let mut validator = Command::new("python3")
            .args([
                "-c",
                &program,
                &(prefix.len() + 1).to_string(),
                &line_end.to_string(),
                &column_end.to_string(),
            ])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        validator
            .stdin
            .take()
            .unwrap()
            .write_all(&output.stderr)
            .unwrap();
        assert!(
            validator.wait().unwrap().success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn option_names_do_not_replace_the_header_sentence() {
    let args = [
        "--emit=asm",
        "--proof-required=unused",
        "-Oeq-scalar",
        "--core-entry=f",
        "-O0",
        "-fno-builtin",
        "missing.c",
    ];
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.starts_with("error: option -Oeq-scalar in proof-required mode requires"),
        "{text}"
    );
    let short = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .arg("--error-format=short")
        .output()
        .unwrap();
    assert_eq!(short.status.code(), Some(1));
    assert!(String::from_utf8(short.stderr)
        .unwrap()
        .starts_with("evocc: -Oeq-scalar in proof-required mode requires"));
}

#[test]
fn explicit_color_does_not_depend_on_redirection() {
    let source = "int main(void) { return missing; }";
    let always = run(&["--parse-only", "--color=always"], source);
    let never = run(&["--parse-only", "--color=never"], source);
    assert_eq!(always.status.code(), Some(1));
    assert_eq!(never.status.code(), Some(1));
    assert!(always.stderr.contains(&0x1b));
    assert!(!never.stderr.contains(&0x1b));
}

#[test]
fn runtime_json_has_an_actual_primary_span_and_stable_fields() {
    let output = run(
        &["--check", "--error-format=json"],
        "int main(void) {\n    int divisor = 0;\n    return 10 / divisor;\n}\n",
    );
    assert_eq!(output.status.code(), Some(1));
    let mut validator = Command::new("python3")
        .args([
            "-c",
            r#"import json, sys
rows = [json.loads(line) for line in sys.stdin]
assert len(rows) == 1
r = rows[0]
assert set(r) == {'code','level','message','spans','notes','help','rendered','repairs'}
assert r['repairs'] == []
assert r['code'] == 'E3002' and r['level'] == 'error'
p = [s for s in r['spans'] if s['primary']]
assert len(p) == 1
assert p[0]['file'] == '/dev/stdin' and p[0]['line_start'] == 3
assert p[0]['column_start'] > 0 and p[0]['column_end'] > p[0]['column_start']
assert r['notes'] and r['help'] and 'return 10 / divisor;' in r['rendered']
"#,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    validator
        .stdin
        .take()
        .unwrap()
        .write_all(&output.stderr)
        .unwrap();
    assert!(
        validator.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_runtime_site(options: &[&str], source: &str, line: u32, snippet: &str) {
    let output = run(options, source);
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains(&format!("--> /dev/stdin:{line}:")), "{text}");
    assert!(text.contains(snippet), "{text}");
    assert!(
        text.contains("this operation failed on the executed path"),
        "{text}"
    );
}

#[test]
fn runtime_span_follows_the_executed_branch_not_the_first_division() {
    assert_runtime_site(
        &["--check"],
        "int main(void) {\n int zero = 0;\n if (zero) return 1 / zero;\n return 2 / zero;\n}\n",
        4,
        "return 2 / zero;",
    );
}

#[test]
fn runtime_span_names_the_callee_not_the_call_site() {
    assert_runtime_site(
        &["--check"],
        "int fail(int n) {\n return 2 / n;\n}\nint main(void) {\n return fail(0);\n}\n",
        2,
        "return 2 / n;",
    );
}

#[test]
fn runtime_span_keeps_the_parent_operation_after_lowering_children() {
    assert_runtime_site(
        &["--check"],
        "int main(void) {\n int zero = 0;\n return 10 /\n (zero + 0);\n}\n",
        3,
        "return 10 /",
    );
}

#[test]
fn runtime_span_survives_optimized_block_and_value_numbering() {
    assert_runtime_site(
        &["--check", "-O2"],
        "int unused(void) { return 0; }\nint main(void) {\n int zero = 0;\n if (1) return 2 / zero;\n return 0;\n}\n",
        4,
        "if (1) return 2 / zero;",
    );
}

#[test]
fn runtime_span_points_into_the_included_source() {
    let dir = std::env::temp_dir().join(format!("evocc-diagnostic-include-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let header = dir.join("fail.h");
    std::fs::write(&header, "int fail(int n) {\n return 9 / n;\n}\n").unwrap();
    let header = std::fs::canonicalize(header).unwrap();
    let source = format!(
        "#include \"{}\"\nint main(void) {{ return fail(0); }}\n",
        header.display()
    );
    let output = run(&["--check"], &source);
    let text = String::from_utf8(output.stderr).unwrap();
    std::fs::remove_file(&header).unwrap();
    std::fs::remove_dir(&dir).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        text.contains(&format!("--> {}:2:", header.display())),
        "{text}"
    );
    assert!(text.contains("return 9 / n;"), "{text}");
}

#[test]
fn conflicting_declaration_labels_the_previous_declaration() {
    let output = run(&["--parse-only"], "int value;\ndouble value;\n");
    assert_eq!(output.status.code(), Some(1));
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("--> /dev/stdin:2:"), "{text}");
    assert!(text.contains("::: /dev/stdin:1:"), "{text}");
    assert!(text.contains("previous declaration is here"), "{text}");
}

#[test]
fn resource_limits_are_not_reported_as_success_or_undefined_behavior() {
    let output = run(&["--check"], "int main(void) { for (;;) {} }\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(text.contains("error[E3008]"), "{text}");
    assert!(
        text.contains("execution reached a resource limit"),
        "{text}"
    );
}

#[test]
fn unsupported_execution_does_not_establish_absence_of_undefined_behavior() {
    let output = run(
        &["--check"],
        "extern int missing(void); int main(void) { return missing(); }",
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let text = String::from_utf8(output.stderr).unwrap();
    assert!(
        text.contains("execution is not supported for this operation"),
        "{text}"
    );
}
