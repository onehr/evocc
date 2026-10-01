//! N04 public-path checks. Expected UI output is reviewed, never auto-blessed.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn run(args: &[&str], source: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .current_dir(root())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(source) = source {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
    }
    drop(child.stdin.take());
    child.wait_with_output().unwrap()
}

fn json_assert(bytes: &[u8], assertion: &str) {
    let program = format!(
        "import json,sys\nrows=[json.loads(x) for x in sys.stdin if x.strip()]\n{}\n{assertion}\n",
        r#"for r in rows:
    assert isinstance(r, dict)
    if r['schema'] == 'evocc.diagnostic.v1':
        assert set(r) == {'schema','kind','severity','code','message','phase','category','source','location','explanation'}
        assert r['kind'] == 'diagnostic' and r['severity'] == 'error'
        assert r['code'] is None or (isinstance(r['code'],str) and len(r['code'])==5 and r['code'][0]=='E' and r['code'][1:].isdigit())
        assert isinstance(r['message'],str)
        assert r['source'] is None or isinstance(r['source'],str)
        assert r['phase'] in {'input','lexing','preprocessing','parsing','lowering','codegen','execution','driver'}
        assert r['category'] in {'compilation','driver','undefined-behavior','resource-limit','unsupported-execution','execution-contract'}
        assert r['explanation'] in {'available','todo','unavailable'}
        if r['location'] is not None:
            assert set(r['location']) == {'line','column'}
            assert all(type(x) is int and x > 0 for x in r['location'].values())
    else:
        assert r['schema'] == 'evocc.check.v1'
        assert set(r) == {'schema','kind','status','main_return','claim'}
        assert r['kind']=='check' and r['status']=='completed' and r['claim']=='executed-path-only'
        assert isinstance(r['main_return'],str)
        int(r['main_return'])"#
    );
    let mut child = Command::new("python3")
        .args(["-c", &program])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(bytes).unwrap();
    assert!(
        child.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(bytes)
    );
}

#[test]
fn check_completes_a_defined_path_even_when_main_returns_nonzero() {
    for result in [0, 7, -1] {
        let source = format!("int main(void) {{ return {result}; }}");
        let out = run(&["--check", "/dev/stdin"], Some(&source));
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(out.stderr.is_empty());
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!("check completed: main returned {result}; no finding on the executed path\n")
        );
    }
}

#[test]
fn blessed_runtime_and_frontend_diagnostics() {
    for name in [
        "division",
        "uninitialized",
        "overflow",
        "shift",
        "bounds",
        "fuel",
        "parser",
        "external",
    ] {
        let path = format!("tests/ui/check/{name}.c");
        let out = run(&["--check", &path], None);
        assert_eq!(out.status.code(), Some(1), "{name}");
        assert!(out.stdout.is_empty(), "{name}");
        let expected = std::fs::read(root().join(format!("tests/ui/check/{name}.stderr"))).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&expected),
            "{name}"
        );
    }
}

#[test]
fn check_uses_the_existing_interpreter_findings_without_fabricated_spans() {
    for name in [
        "division",
        "uninitialized",
        "overflow",
        "shift",
        "bounds",
        "fuel",
    ] {
        let path = format!("tests/ui/check/{name}.c");
        let legacy = run(&["--interpret-evoir", &path], None);
        let check = run(&["--check", &path], None);
        assert_eq!(legacy.status.code(), check.status.code());
        let legacy = String::from_utf8(legacy.stderr).unwrap();
        let check = String::from_utf8(check.stderr).unwrap();
        assert_eq!(
            legacy.replace(&format!("{path}:1:1:"), &format!("{path}:")),
            check
        );
    }
    let out = run(
        &[
            "--check",
            "--error-format=json",
            "tests/ui/check/division.c",
        ],
        None,
    );
    json_assert(&out.stderr, "assert len(rows)==1\nr=rows[0]\nassert r['schema']=='evocc.diagnostic.v1'\nassert r['kind']=='diagnostic' and r['code']=='E3002'\nassert r['category']=='undefined-behavior' and r['phase']=='execution'\nassert r['location'] is None and r['source']=='tests/ui/check/division.c'\nassert r['explanation']=='available'");
}

#[test]
fn resource_and_unsupported_execution_are_not_ub_or_pass() {
    let fuel = run(
        &["--check", "--error-format=json", "tests/ui/check/fuel.c"],
        None,
    );
    assert_eq!(fuel.status.code(), Some(1));
    assert!(fuel.stdout.is_empty());
    json_assert(&fuel.stderr, "assert len(rows)==1 and rows[0]['code']=='E3008' and rows[0]['category']=='resource-limit'");
    let external = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("extern int unknown(void); int main(void) { return unknown(); }"),
    );
    assert_eq!(external.status.code(), Some(1));
    assert!(external.stdout.is_empty());
    json_assert(
        &external.stderr,
        "assert len(rows)==1 and rows[0]['category']=='unsupported-execution'",
    );
}

#[test]
fn json_completion_is_not_a_whole_program_proof() {
    let out = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("int main(void) { return 7; }"),
    );
    assert!(out.status.success());
    assert!(out.stderr.is_empty());
    json_assert(&out.stdout, "assert len(rows)==1\nr=rows[0]\nassert r['schema']=='evocc.check.v1' and r['kind']=='check'\nassert r['status']=='completed' and r['main_return']=='7' and r['claim']=='executed-path-only'");
}

#[test]
fn json_frontend_records_preserve_locations_and_survey_boundaries() {
    let out = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("\"unterminated"),
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(&out.stderr, "assert len(rows)==1\nr=rows[0]\nassert r['code']=='E0001' and r['phase']=='lexing'\nassert r['location']=={'line':1,'column':1}");
    let out = run(
        &[
            "--parse-only",
            "--survey",
            "--error-format=json",
            "/dev/stdin",
        ],
        Some("int a[-1];\nint b[-2];\n"),
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(&out.stderr, "assert len(rows)==2 and all(r['code']=='E2923' and r['phase']=='parsing' for r in rows)\nassert [r['location']['line'] for r in rows]==[1,2]");
    let out = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("#error \"quoted\"\\text\n"),
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(&out.stderr, "assert len(rows)==1 and rows[0]['code']=='E0104' and rows[0]['phase']=='preprocessing'\nassert '\"quoted\"' in rows[0]['message']");
}

#[test]
fn explain_uses_the_authoritative_table_and_records_missing_explanations() {
    let spec = std::fs::read_to_string(root().join("docs/evoir-spec.md")).unwrap();
    let table = spec
        .split("### 6.3 Diagnostic Authority")
        .nth(1)
        .unwrap()
        .split("### 6.4")
        .next()
        .unwrap();
    let mut count = 0;
    for line in table.lines().filter(|line| line.starts_with("| `E")) {
        let fields: Vec<_> = line.split('|').map(str::trim).collect();
        let code = fields[1].trim_matches('`');
        let out = run(&["--explain", code], None);
        assert!(
            out.status.success(),
            "{code}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let text = String::from_utf8(out.stdout).unwrap();
        assert!(text.contains(fields[2].trim_matches('`')), "{code}: {text}");
        assert!(text.contains(fields[3]), "{code}: {text}");
        count += 1;
    }
    let authority_rows = table
        .lines()
        .filter(|line| line.starts_with("| `E"))
        .count();
    assert!(authority_rows > 0);
    assert_eq!(count, authority_rows);
    let missing = run(&["--explain", "E3063"], None);
    assert!(missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stdout)
        .contains("TODO: explanation missing from EvoIR section 6.3"));
    let unknown = run(&["--explain", "E9999"], None);
    assert_eq!(unknown.status.code(), Some(1));
    assert!(unknown.stdout.is_empty());
}

#[test]
fn malformed_cli_fails_and_json_usage_errors_are_valid_records() {
    for args in [
        vec!["--check"],
        vec!["--check", "--interpret-evoir", "/dev/stdin"],
        vec!["--check", "--error-format=yaml", "/dev/stdin"],
        vec!["--check", "--check", "/dev/stdin"],
        vec!["--explain", "E3002", "ignored.c"],
    ] {
        let out = run(&args, Some("int main(void) { return 0; }"));
        assert_eq!(out.status.code(), Some(1), "{args:?}");
    }
    let out = run(
        &["--check", "--error-format=json", "--bad=\"quoted\""],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(&out.stderr, "assert len(rows)==1 and rows[0]['category']=='driver' and rows[0]['code'] is None\nassert '\"quoted\"' in rows[0]['message']");
}

#[test]
fn json_does_not_change_legacy_interpreter_stdout() {
    let out = run(
        &["--interpret-evoir", "--error-format=json", "/dev/stdin"],
        Some("int main(void) { return 7; }"),
    );
    assert!(out.status.success());
    assert_eq!(out.stdout, b"7\n");
    assert!(out.stderr.is_empty());
}

#[test]
fn input_failures_have_no_fabricated_source_position() {
    let out = run(
        &["--check", "--error-format=json", "tests/ui/check/missing.c"],
        None,
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(&out.stderr, "assert len(rows)==1\nr=rows[0]\nassert r['code']=='E0003' and r['phase']=='input' and r['location'] is None");
    let legacy = run(&["--parse-only", "tests/ui/check/missing.c"], None);
    assert!(String::from_utf8_lossy(&legacy.stderr)
        .starts_with("tests/ui/check/missing.c:1:1: error[E0003]:"));
    let escaped = run(
        &[
            "--check",
            "--error-format=json",
            "tests/ui/check/missing\"雪\n.c",
        ],
        None,
    );
    json_assert(&escaped.stderr, "assert len(rows)==1 and rows[0]['source']=='tests/ui/check/missing\"雪\\n.c' and rows[0]['location'] is None");
}
