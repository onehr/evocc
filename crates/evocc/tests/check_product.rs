//! Public execution checks and intentionally updated diagnostic snapshots.

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
        "{}\nimport sys\nrows=[json.loads(x) for x in sys.stdin if x.strip()]\n{}\n{assertion}\n",
        include_str!("../../../tests/ui/check/validate.py"),
        r#"for r in rows:
    assert isinstance(r, dict)
    if 'spans' in r:
        validate(r)
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
fn diagnostic_schema_rejects_missing_spans_and_malformed_fields() {
    let validator = include_str!("../../../tests/ui/check/validate.py");
    assert!(validator.lines().count() <= 80);
    let script = format!(
        "{validator}\n{}",
        r#"
import copy
from pathlib import Path
cases = sorted(Path('tests/ui/check').glob('*.json'))
assert len(cases) >= 40
records = [json.loads(line) for path in cases for line in path.read_text().splitlines()]
for record in records:
    validate(record)
assert {record['code'][:2] for record in records} >= {'E0','E1','E2','E3','E4'}
assert any(len(record['spans']) > 1 for record in records)
assert any(span['line_end'] > span['line_start'] for record in records for span in record['spans'])
assert any(record['notes'] and record['help'] for record in records)
assert any(len(path.read_text().splitlines()) > 1 for path in cases)
assert any('diagnostic-include.h' in span['file'] for record in records for span in record['spans'])
original = records[0]
mutations = []
for key in original:
    record = copy.deepcopy(original)
    del record[key]
    mutations.append(record)
for key, value in [('spans', []), ('level', 'success'), ('code', 'E１２３４'), ('notes', 'note'), ('help', [1])]:
    record = copy.deepcopy(original)
    record[key] = value
    mutations.append(record)
for key, value in [('primary', False), ('primary', 1), ('line_start', 0), ('column_end', True), ('label', ''), ('file', '')]:
    record = copy.deepcopy(original)
    record['spans'][0][key] = value
    mutations.append(record)
record = copy.deepcopy(original)
record['spans'].append(copy.deepcopy(record['spans'][0]))
mutations.append(record)
record = copy.deepcopy(original)
record['extra'] = None
mutations.append(record)
for record in mutations:
    try:
        validate(record)
    except (AssertionError, KeyError, TypeError):
        continue
    raise AssertionError(f'invalid record accepted: {record}')
"#
    );
    assert!(Command::new("python3")
        .current_dir(root())
        .args(["-c", &script])
        .status()
        .unwrap()
        .success());
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
    let mut cases: Vec<_> = std::fs::read_dir(root().join("tests/ui/check"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "c"))
        .collect();
    cases.sort();
    assert!(cases.len() >= 40, "only {} diagnostic cases", cases.len());
    for case in cases {
        let source = std::fs::read_to_string(&case).unwrap();
        let flags = source
            .lines()
            .find_map(|line| line.strip_prefix("// flags: "))
            .unwrap_or("--check");
        let expected_exit = source
            .lines()
            .find_map(|line| line.strip_prefix("// exit: "))
            .map_or(1, |value| value.parse::<i32>().unwrap());
        let path = format!(
            "tests/ui/check/{}",
            case.file_name().unwrap().to_str().unwrap()
        );
        for (format, extension) in [
            ("--error-format=text", "stderr"),
            ("--error-format=json", "json"),
        ] {
            let mut args: Vec<_> = flags.split_whitespace().collect();
            args.extend(["--color=never", format, &path]);
            let out = run(&args, None);
            assert_eq!(
                out.status.code(),
                Some(expected_exit),
                "{}: {}",
                case.display(),
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(out.stdout.is_empty(), "{}", case.display());
            if extension == "json" && !out.stderr.is_empty() {
                json_assert(&out.stderr, "assert rows");
            }
            // Included files have canonical paths. Only the checkout prefix varies.
            let actual = String::from_utf8(out.stderr)
                .unwrap()
                .replace(root().canonicalize().unwrap().to_str().unwrap(), "$ROOT");
            let expected = case.with_extension(extension);
            if std::env::var("UPDATE_EXPECT").as_deref() == Ok("1") {
                std::fs::write(&expected, &actual).unwrap();
            }
            assert_eq!(
                actual,
                std::fs::read_to_string(&expected).unwrap(),
                "{}",
                expected.display()
            );
        }
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
    json_assert(
        &out.stderr,
        "assert len(rows)==1\nr=rows[0]\nassert r['code']=='E3002' and r['level']=='error'\np=r['spans'][0]\nassert p['file']=='tests/ui/check/division.c' and p['line_start']==1\nassert p['column_end'] > p['column_start']",
    );
}

#[test]
fn resource_and_unsupported_execution_are_not_ub_or_pass() {
    let fuel = run(
        &["--check", "--error-format=json", "tests/ui/check/fuel.c"],
        None,
    );
    assert_eq!(fuel.status.code(), Some(1));
    assert!(fuel.stdout.is_empty());
    json_assert(
        &fuel.stderr,
        "assert len(rows)==1 and rows[0]['code']=='E3008'\nassert any('resource limit' in note for note in rows[0]['notes'])",
    );
    let external = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("extern int unknown(void); int main(void) { return unknown(); }"),
    );
    assert_eq!(external.status.code(), Some(1));
    assert!(external.stdout.is_empty());
    json_assert(
        &external.stderr,
        "assert len(rows)==1 and rows[0]['code']=='E3039'\nassert any('execution is not supported' in note for note in rows[0]['notes'])",
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
    json_assert(
        &out.stdout,
        "assert len(rows)==1\nr=rows[0]\nassert r['schema']=='evocc.check.v1' and r['kind']=='check'\nassert r['status']=='completed' and r['main_return']=='7' and r['claim']=='executed-path-only'",
    );
}

#[test]
fn json_frontend_records_preserve_locations_and_survey_boundaries() {
    let out = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("\"unterminated"),
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(
        &out.stderr,
        "assert len(rows)==1\nr=rows[0]\nassert r['code']=='E0001'\nassert r['spans'][0]['line_start']==1 and r['spans'][0]['column_start']==1",
    );
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
    json_assert(
        &out.stderr,
        "assert len(rows)==2 and all(r['code']=='E2923' for r in rows)\nassert [r['spans'][0]['line_start'] for r in rows]==[1,2]",
    );
    let out = run(
        &["--check", "--error-format=json", "/dev/stdin"],
        Some("#error \"quoted\"\\text\n"),
    );
    assert_eq!(out.status.code(), Some(1));
    json_assert(
        &out.stderr,
        "assert len(rows)==1 and rows[0]['code']=='E0104'\nassert '\"quoted\"' in rows[0]['message']",
    );
}

#[test]
fn explain_refuses_codes_without_authority_rows() {
    for code in ["E3063", "E9999", "E2207"] {
        let out = run(&["--explain", code], None);
        assert_eq!(out.status.code(), Some(1));
        assert!(out.stdout.is_empty());
        assert!(String::from_utf8_lossy(&out.stderr)
            .contains(&format!("no explanation for code {code}")));
    }
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
    json_assert(
        &out.stderr,
        "assert len(rows)==1 and rows[0]['code'] is None\nassert rows[0]['spans'][0]['file']=='<command-line>'\nassert '\"quoted\"' in rows[0]['message']",
    );
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
    json_assert(
        &out.stderr,
        "assert len(rows)==1\nr=rows[0]\nassert r['code']=='E0003' and r['spans'][0]['file']=='<command-line>'",
    );
    let legacy = run(&["--parse-only", "tests/ui/check/missing.c"], None);
    assert!(String::from_utf8_lossy(&legacy.stderr).starts_with("error[E0003]:"));
    let escaped = run(
        &[
            "--check",
            "--error-format=json",
            "tests/ui/check/missing\"雪\n.c",
        ],
        None,
    );
    json_assert(
        &escaped.stderr,
        "assert len(rows)==1 and rows[0]['spans'][0]['file']=='<command-line>'\nassert 'input file: tests/ui/check/missing\"雪\\n.c' in rows[0]['notes']",
    );
}
