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
    child
        .stdin
        .take()
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();
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
    assert!(text.starts_with("/dev/stdin:1:24: error[E2001]:"), "{text}");
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
assert set(r) == {'code','level','message','spans','notes','help','rendered'}
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
