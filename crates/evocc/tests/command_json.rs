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
    assert r['stages']==[{k:s[k] for k in ('id','status')} for s in record['stages']]
    directory=pathlib.Path(r['proof_directory'])
    assert directory.name.startswith('evo-proof-')
    assert directory.parent.resolve()==pathlib.Path(tempfile.gettempdir()).resolve()
    assert path==directory/'checked/record.json'
    shutil.rmtree(directory)
    r['record_path']='<proof>/checked/record.json'
    r['proof_directory']='<proof>'
def normalize(x):
    if isinstance(x,str): return x.replace(work,'<work>')
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
    let run = evo()
        .args(["run", &work.file(), "--json"])
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(7));
    snapshot("run", &run, &work);
    let check = evo()
        .args(["check", "--json", &work.file()])
        .output()
        .unwrap();
    assert!(check.status.success());
    snapshot("check", &check, &work);
}

#[test]
fn explain_version_help_and_fix_have_json_snapshots() {
    let work = Scratch::new();
    for (verb, args) in [
        ("explain", vec!["explain", "E3002", "--json"]),
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

#[test]
fn build_json_is_the_written_receipt() {
    let work = Scratch::new();
    std::fs::write(
        work.0.join("Makefile"),
        "all:\n\t$(CC) -c input.c -o input.o\n",
    )
    .unwrap();
    let output = evo()
        .args(["build", work.0.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{:?}", output);
    let receipt = std::fs::read(work.0.join("evo-build-receipt.json")).unwrap();
    assert_eq!(output.stdout, receipt);
    snapshot("build", &output, &work);
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
