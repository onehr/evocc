//! Machine-readable user commands retain the ordinary command's exit status.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "evocc-command-json-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("input.c"), "int main(void) { return 7; }\n").unwrap();
        Self(path)
    }
    fn file(&self) -> String {
        self.0.join("input.c").to_str().unwrap().into()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn evo() -> Command {
    let compiler = PathBuf::from(env!("CARGO_BIN_EXE_evocc"));
    let mut command = Command::new(compiler.with_file_name("evo"));
    command.env("EVOCC", compiler).env(
        "EVOCC_PROOF_CHECKER",
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lean/.lake/build/bin/evoir-sem"),
    );
    command
}

fn assert_json(output: &Output, assertions: &str, args: &[&str]) {
    assert!(output.stderr.is_empty(), "{:?}", output);
    let script = format!(
        "import json,sys,pathlib,subprocess,shutil,tempfile\nraw=sys.stdin.buffer.read()\nassert len(raw.splitlines())==1,raw\nr=json.loads(raw)\nassert isinstance(r,dict)\n{assertions}"
    );
    let mut check = Command::new("python3")
        .args(["-c", &script])
        .args(args)
        .env("TEST_EVOCOMPILER", env!("CARGO_BIN_EXE_evocc"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    check
        .stdin
        .take()
        .unwrap()
        .write_all(&output.stdout)
        .unwrap();
    assert!(check.wait().unwrap().success(), "{:?}", output);
}

fn snapshot(verb: &str, output: &Output, work: &Scratch) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui/commands");
    assert_json(
        output,
        r#"import os
verb,work,fixture=sys.argv[1:]
compiler=os.environ['TEST_EVOCOMPILER']
if verb in ('version','build'):
    expected=subprocess.check_output([compiler,'--version']).decode().strip()
    if verb=='build': expected=expected.splitlines()[0]
    assert r['version']==expected
    r['version']='<version>'
if verb=='build':
    assert r['target']==subprocess.check_output([compiler,'-dumpmachine']).decode().strip()
    receipt=json.loads(pathlib.Path(work,'evo-build-receipt.json').read_text())
    assert receipt['sources']==r['sources'] and receipt['exit_code']==r['exit_code']
    r['target']='<target>'
if verb=='explain':
    assert r['text']==subprocess.check_output([compiler,'--explain','E3002']).decode()
    r['text']='<page>'
if verb=='prove':
    path=pathlib.Path(r['record_path'])
    record=json.loads(path.read_text())
    assert r['result']==record['result']
    assert r['checker']==record['checker']['path']
    r['checker']='<checker>'
    assert r['stages']==[{k:s[k] for k in ('id','status')} for s in record['stages']]
    directory=pathlib.Path(r['proof_directory'])
    assert directory.name.startswith('evo-proof-')
    assert directory.parent.resolve()==pathlib.Path(tempfile.gettempdir()).resolve()
    assert path==directory/'checked/record.json'
    shutil.rmtree(directory)
    r['record_path']='<proof>/checked/record.json'
    r['proof_directory']='<proof>'
def normalize(x):
    if isinstance(x,str): return x.replace(str(pathlib.Path(work).resolve()),'<work>').replace(work,'<work>')
    if isinstance(x,list): return [normalize(y) for y in x]
    if isinstance(x,dict): return {k:normalize(v) for k,v in x.items()}
    return x
assert normalize(r)==json.loads(pathlib.Path(fixture).read_text()),r
"#,
        &[
            verb,
            work.0.to_str().unwrap(),
            root.join(format!("{verb}.json")).to_str().unwrap(),
        ],
    );
}

#[test]
fn run_and_check_have_distinct_json_statuses() {
    let work = Scratch::new();
    for verb in ["run", "check"] {
        for json_first in [true, false] {
            let file = work.file();
            let args = if json_first {
                [verb, "--json", &file]
            } else {
                [verb, &file, "--json"]
            };
            let output = evo().args(args).output().unwrap();
            assert_eq!(
                output.status.code(),
                Some(if verb == "run" { 7 } else { 0 })
            );
            snapshot(verb, &output, &work);
        }
    }
}

#[test]
fn explain_version_help_and_fix_have_json_snapshots() {
    let work = Scratch::new();
    for (verb, args) in [
        ("explain", vec!["explain", "E3002", "--json"]),
        ("explain", vec!["explain", "--json", "E3002"]),
        ("version", vec!["version", "--json"]),
        ("help", vec!["help", "--json"]),
        ("fix", vec!["fix", &work.file(), "--plan", "--json"]),
    ] {
        let output = evo().args(args).output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        snapshot(verb, &output, &work);
    }
}

#[test]
fn json_program_arguments_remain_byte_exact_after_the_separator() {
    use std::os::unix::ffi::OsStringExt;
    let work = Scratch::new();
    std::fs::write(work.file(), "int main(int n, char **v) { return n != 3 || v[1][0] != '-' || v[1][2] != 'j' || (unsigned char)v[2][0] != 255; }").unwrap();
    let output = evo()
        .args(["run", &work.file(), "--json", "--", "--json"])
        .arg(std::ffi::OsString::from_vec(vec![255]))
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert_json(
        &output,
        "assert r=={'exit_code':0,'stdout':'','stderr':''}",
        &[],
    );
}

#[test]
fn check_failure_contains_the_original_diagnostic_records() {
    let work = Scratch::new();
    std::fs::write(
        work.file(),
        "int main(void) { int zero = 0; return 1 / zero; }\n",
    )
    .unwrap();
    let output = evo()
        .args(["check", &work.file(), "--json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_json(&output, "assert r['exit_code']==1 and r['completion'] is None\nassert r['diagnostics'][0]['code']=='E3002'\nassert r['diagnostics'][0]['spans'][0]['primary']", &[]);
}

#[test]
fn usage_and_explanation_errors_are_json_too() {
    for args in [
        vec!["run", "--json"],
        vec!["check", "--json"],
        vec!["prove", "--json"],
        vec!["fix", "--json"],
        vec!["explain", "E9999", "--json"],
        vec!["unknown", "--json"],
    ] {
        let output = evo().args(args).output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert_json(
            &output,
            "assert r['exit_code']==1 and isinstance(r['error'],str) and r['error']",
            &[],
        );
    }
}

/// `concise-1` names a file below the working directory relatively. The file
/// argument keeps the uncanonical temporary path, as in the agent harness.
fn concise(work: &Scratch, source: &str, status: i32, expected: &str) {
    std::fs::write(work.file(), source).unwrap();
    let output = evo()
        .current_dir(&work.0)
        .args(["check", &work.file(), "--json", "--form=concise-1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(status), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
}

#[test]
fn concise_check_records_have_fixed_keys_and_relative_locations() {
    let work = Scratch::new();
    concise(
        &work,
        "int main(void) { int zero = 0; return 1 / zero; }\n",
        1,
        r#"{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3002","message":"integer-division-by-zero","at":"input.c:1:39","source":"1 / zero","help":["check the divisor before the division or remainder operation"],"why":"The executed integer division has a zero divisor.","fix":"Check the divisor and choose the required error behavior before dividing."}],"exit_code":1}
"#,
    );
    concise(
        &work,
        "int main(void) { return 7; }\n",
        0,
        r#"{"form":"concise-1","status":"completed","main_return":"7","scope":"No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct.","exit_code":0}
"#,
    );
    // A secondary span keeps its own location and label.
    concise(
        &work,
        "int f(int a);\nlong f(int a) { return a; }\nint main(void) { return 0; }\n",
        1,
        r#"{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E2002","message":"incompatible-redeclaration: f","at":"input.c:2:6","source":"f","also":["input.c:1:5 previous declaration is here"],"why":"Two declarations of one name do not describe compatible entities.","fix":"Correct the conflicting declaration instead of casting each use."}],"exit_code":1}
"#,
    );
    // A whole-function span shows its end and only the first line.
    concise(
        &work,
        "static int spin(void)\n{\n    for (;;) {\n    }\n    return 0;\n}\n\nint main(void)\n{\n    return spin();\n}\n",
        1,
        r#"{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3008","message":"interpreter-fuel-exhausted","at":"input.c:1:1","end":"6:2","source":"static int spin(void) ...","notes":["execution reached a resource limit; this is not a finding of undefined behavior","the executing function is shown; this instruction has no retained expression position"],"why":"The interpreter reached its execution step limit.","fix":"Inspect long-running loops and use a finite workload when testing termination."}],"exit_code":1}
"#,
    );
    // The plan adds the -Wall repairs. A warning keeps its own repairs.
    concise(
        &work,
        "static int f(int unused) { return 0; }\nmain(void) { return f(7); }\n",
        0,
        r#"{"form":"concise-1","status":"completed","main_return":"0","scope":"No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct.","diagnostics":[{"code":"W0004","message":"declaration has no type specifier","level":"warning","at":"input.c:2:1","source":"main","help":["write an explicit type specifier"],"why":"A declaration omits its type specifier.","fix":"Write an explicit type specifier, such as `int`.","repairs":[{"id":"W0004.insert-int","kind":"safe","title":"write an explicit type specifier","at":"input.c:2:1","end":"2:1","replacement":"int "}]}],"repairs":[{"id":"W0002.void-cast","kind":"safe","title":"use the parameter or mark it __attribute__((unused))","at":"input.c:1:27","end":"1:27","replacement":" (void)unused;"},{"id":"W0004.insert-int","kind":"safe","title":"write an explicit type specifier","at":"input.c:2:1","end":"2:1","replacement":"int "}],"exit_code":0}
"#,
    );
    // A review repair keeps its kind. Both copies escape a newline the same way.
    concise(
        &work,
        "int f(void) { return missing(1, 2.0); }\nint main(void) { return 0; }\n",
        1,
        r#"{"form":"concise-1","status":"stopped","diagnostics":[{"code":"W0003","message":"function has no declaration","at":"input.c:1:22","source":"missing","help":["declare the function before its first call"],"why":"A call names a function with no visible declaration.","fix":"Declare the function before its first call, normally by including its header.","repairs":[{"id":"W0003.insert-prototype","kind":"review","title":"declare the function before its first call","at":"input.c:1:1","end":"1:1","replacement":"int missing(int, double);\n"}]}],"repairs":[{"id":"W0003.insert-prototype","kind":"review","title":"declare the function before its first call","at":"input.c:1:1","end":"1:1","replacement":"int missing(int, double);\n"}],"exit_code":1}
"#,
    );
}

#[test]
fn concise_locations_outside_the_working_directory_keep_the_reported_path() {
    let work = Scratch::new();
    let elsewhere = Scratch::new();
    std::fs::write(
        work.file(),
        "int main(void) { int zero = 0; return 1 / zero; }\n",
    )
    .unwrap();
    let output = evo()
        .current_dir(&elsewhere.0)
        .args(["check", &work.file(), "--json", "--form=concise-1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains(&format!("\"at\":\"{}:1:39\"", work.file())),
        "{text}"
    );
    // A relative argument below the working directory stays relative.
    let output = evo()
        .current_dir(work.0.parent().unwrap())
        .args(["check", "--json", "--form=concise-1"])
        .arg(work.0.file_name().unwrap().to_str().unwrap().to_owned() + "/input.c")
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let name = work.0.file_name().unwrap().to_str().unwrap();
    assert!(
        text.contains(&format!("\"at\":\"{name}/input.c:1:39\"")),
        "{text}"
    );
}

#[test]
fn full_1_is_the_default_check_record() {
    let work = Scratch::new();
    for source in [
        "int main(void) { return 7; }\n",
        "int main(void) { int zero = 0; return 1 / zero; }\n",
        "main(void) { return 0; }\n",
    ] {
        std::fs::write(work.file(), source).unwrap();
        let default = evo()
            .args(["check", &work.file(), "--json"])
            .output()
            .unwrap();
        for args in [
            ["check", &work.file(), "--json", "--form=full-1"],
            ["check", "--form=full-1", "--json", &work.file()],
        ] {
            let explicit = evo().args(args).output().unwrap();
            assert_eq!(explicit.status, default.status, "{args:?}");
            assert_eq!(explicit.stdout, default.stdout, "{args:?}");
            assert_eq!(explicit.stderr, default.stderr, "{args:?}");
        }
    }
}

#[test]
fn form_selects_a_check_record_and_nothing_else() {
    let work = Scratch::new();
    let file = work.file();
    let output = evo()
        .args(["check", &file, "--form=concise-1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(
        error.starts_with("evo: ") && error.contains("--form"),
        "{error}"
    );
    for args in [
        vec!["check", &file, "--json", "--form=concise-2"],
        vec!["check", &file, "--json", "--form="],
        vec!["check", &file, "--json", "--form"],
        vec!["check", &file, "--json", "--form", "concise-1"],
        vec![
            "check",
            &file,
            "--json",
            "--form=concise-1",
            "--form=full-1",
        ],
        vec!["check", &file, "--json", "--form=full-1", "--form=full-1"],
        vec!["explain", "E3002", "--json", "--form=concise-1"],
        vec!["fix", &file, "--plan", "--json", "--form=concise-1"],
        vec!["run", &file, "--json", "--form=full-1"],
        vec!["version", "--json", "--form=full-1"],
    ] {
        let output = evo().args(&args).output().unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}");
        assert_json(
            &output,
            "assert r['exit_code']==1 and '--form' in r['error'],r",
            &[],
        );
    }
    // After the separator, the option is a program argument.
    std::fs::write(
        &file,
        "int main(int n, char **v) { return n == 2 && v[1][2] == 'f' ? 7 : 1; }\n",
    )
    .unwrap();
    let output = evo()
        .args(["run", &file, "--json", "--", "--form=concise-1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7), "{output:?}");
    assert_json(
        &output,
        "assert r=={'exit_code':7,'stdout':'','stderr':''}",
        &[],
    );
}

#[test]
fn build_json_is_the_written_receipt() {
    let work = Scratch::new();
    std::fs::write(
        work.0.join("Makefile"),
        "all:\n\t$(CC) -c input.c -o input.o\n",
    )
    .unwrap();
    for args in [
        ["build", work.0.to_str().unwrap(), "--json"],
        ["build", "--json", work.0.to_str().unwrap()],
    ] {
        let output = evo().args(args).output().unwrap();
        assert!(output.status.success(), "{:?}", output);
        let receipt = std::fs::read(work.0.join("evo-build-receipt.json")).unwrap();
        assert_eq!(output.stdout, receipt);
        snapshot("build", &output, &work);
    }
}

#[test]
fn prove_accepts_file_before_between_or_after_options() {
    let work = Scratch::new();
    let assembly = work.0.join("out.s");
    for output_first in [true, false] {
        for position in [0, 2, 4] {
            let output_option = ["-o".to_owned(), assembly.to_str().unwrap().to_owned()];
            let entry_option = ["--entry".to_owned(), "main".to_owned()];
            let mut args = if output_first {
                [output_option, entry_option].concat()
            } else {
                [entry_option, output_option].concat()
            };
            args.insert(position, work.file());
            let output = evo()
                .env("TMPDIR", &work.0)
                .args(["prove", "--json"])
                .args(&args)
                .output()
                .unwrap();
            assert!(output.status.success(), "{args:?}: {output:?}");
            assert_json(
                &output,
                "assert r['exit_code']==0 and r['result']=='CHECKED_TO_FINAL_ASSEMBLY'",
                &[],
            );
            assert!(std::fs::metadata(&assembly).unwrap().len() > 0);
            std::fs::remove_file(&assembly).unwrap();
        }
    }
}

#[test]
fn prove_rejects_invalid_arguments_without_writing_output() {
    let work = Scratch::new();
    let file = work.file();
    let assembly = work.0.join("out.s");
    let out = assembly.to_str().unwrap();
    for (args, expected) in [
        (
            vec![file.as_str(), "-o"],
            "missing output path or entry name",
        ),
        (vec!["-o"], "missing output path or entry name"),
        (
            vec![file.as_str(), "--entry"],
            "missing output path or entry name",
        ),
        (vec!["--entry"], "missing output path or entry name"),
        (
            vec![file.as_str(), "-o", out, "-o", out],
            "unknown prove option: -o",
        ),
        (
            vec!["-o", out, file.as_str(), "extra.c"],
            "prove requires exactly one C file",
        ),
        (
            vec![file.as_str(), "--unknown", "value"],
            "unknown prove option: --unknown",
        ),
        (
            vec!["--unknown", file.as_str(), "-o", out],
            "unknown prove option: --unknown",
        ),
        (vec!["--"], "unknown prove option: --"),
        (
            vec![file.as_str(), "-o", out, "--"],
            "unknown prove option: --",
        ),
    ] {
        let output = evo()
            .args(["prove", "--json"])
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        assert_json(
            &output,
            "assert r['exit_code']==1 and r['error']==sys.argv[1]",
            &[expected],
        );
        assert!(!assembly.exists());
    }
}

#[test]
fn prove_finds_the_checkout_checker_without_an_override() {
    let work = Scratch::new();
    let output = evo()
        .env_remove("EVOCC_PROOF_CHECKER")
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", &work.0)
        .current_dir(&work.0)
        .args(["prove", &work.file(), "--json", "-o"])
        .arg(work.0.join("out.s"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_json(&output, "assert r['checker'].endswith('/lean/.lake/build/bin/evoir-sem')\nassert r['checker']==json.loads(pathlib.Path(r['record_path']).read_text())['checker']['path']", &[]);
}

#[test]
fn prove_json_summarizes_the_retained_record() {
    let work = Scratch::new();
    let assembly = work.0.join("out.s");
    let output = evo()
        .args([
            "prove",
            &work.file(),
            "--json",
            "-o",
            assembly.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    assert!(assembly.is_file());
    snapshot("prove", &output, &work);
}

#[test]
fn prove_json_does_not_turn_an_output_copy_failure_into_success() {
    let work = Scratch::new();
    let assembly = work.0.join("absent/out.s");
    let output = evo()
        .args([
            "prove",
            &work.file(),
            "-o",
            assembly.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!assembly.exists());
    assert_json(
        &output,
        r#"
assert r['exit_code']==1 and r['stderr']
assert r['result']=='CHECKED_TO_FINAL_ASSEMBLY'
directory=pathlib.Path(r['proof_directory'])
assert directory.name.startswith('evo-proof-')
assert directory.parent.resolve()==pathlib.Path(tempfile.gettempdir()).resolve()
assert pathlib.Path(r['record_path']).is_file()
shutil.rmtree(directory)
"#,
        &[],
    );
}

#[test]
fn compiler_lookup_errors_stay_machine_readable() {
    let work = Scratch::new();
    let output = evo()
        .args(["version", "--json"])
        .env("EVOCC", work.0.join("absent"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_json(&output, "assert r['exit_code']==1 and r['error']", &[]);
}
