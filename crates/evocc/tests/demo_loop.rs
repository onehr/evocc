//! The demo must finish with a retained, hash-bound Lean proof record.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn agent_loop_reaches_checked_final_assembly_in_under_thirty_seconds() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let fixture = repository.join("tools/demo/loop/buggy.c");
    let original = std::fs::read(&fixture).unwrap();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let work_path = std::env::temp_dir().join(format!(
        "evocc-demo-loop-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir(&work_path).unwrap();
    let work = Scratch(work_path);
    let checker = repository.join("lean/.lake/build/bin/evoir-sem");
    let started = Instant::now();
    let output = Command::new("sh")
        .arg(repository.join("tools/demo/agent-loop.sh"))
        .current_dir(&repository)
        .env("EVO", env!("CARGO_BIN_EXE_evo"))
        .env("EVOCC", env!("CARGO_BIN_EXE_evocc"))
        .env("EVOCC_PROOF_CHECKER", &checker)
        .env_remove("EVO_BUILD_TRACE")
        .env("TMPDIR", &work.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read(&fixture).unwrap(), original);

    // Follow the command-JSON tests' stdlib reader, without new dependencies.
    // A printed label alone cannot satisfy these retained-artifact checks.
    let mut oracle = Command::new("python3")
        .args([
            "-c",
            r#"import hashlib,json,pathlib,sys
lines=sys.stdin.read().splitlines()
assert len(lines)>=2,lines
proof=json.loads(lines[-2])
assert set(proof)=={'result','stages','record_path','proof_directory','exit_code','stderr'},proof
assert proof['result']=='CHECKED_TO_FINAL_ASSEMBLY' and proof['exit_code']==0,proof
assert lines[-1]==proof['result']+' record='+proof['record_path'],lines[-1]
work=pathlib.Path(sys.argv[1]).resolve()
checker=pathlib.Path(sys.argv[2]).resolve()
compiler=pathlib.Path(sys.argv[3]).resolve()
directory=pathlib.Path(proof['proof_directory']).resolve()
path=pathlib.Path(proof['record_path']).resolve()
assert directory.parent==work and directory.name.startswith('evo-proof-'),directory
assert path==directory/'checked/record.json',path
record=json.loads(path.read_text())
assert record['schema']=='evocc-proof-required-record-v1'
assert record['result']==proof['result'] and record['profile']=='core-v1'
assert record['all_stages_checked'] is True and record['output']['checked'] is True
assert record['checker_digest_pinned'] is True
assert pathlib.Path(record['checker']['path']).resolve()==checker
assert pathlib.Path(record['compiler']['path']).resolve()==compiler
assert {entry['entry'] for entry in record['entries']}=={'divide','main'}
assert proof['stages']==[{k:stage[k] for k in ('id','status')} for stage in record['stages']]
assert [stage['id'] for stage in record['stages']]==[
    'source-to-evoir','rewrite','lowering','allocation','frame-emission','pipeline']
assert [stage['status'] for stage in record['stages']]==[
    'CHECKED','NOT_CHECKED','CHECKED','CHECKED','CHECKED','CHECKED']
def digest(file): return hashlib.sha256(file.read_bytes()).hexdigest()
for name in ('checker','compiler','source'):
    artifact=record[name]
    assert digest(pathlib.Path(artifact['path']))==artifact['sha256'],name
source=pathlib.Path(record['source']['path']).resolve()
assert work in source.parents,source
for stage in record['stages']:
    if stage['status']=='NOT_CHECKED':
        assert stage['id']=='rewrite' and 'no rewrite ran' in stage['reason']
        continue
    assert stage['results'],stage
    for result in stage['results']:
        assert result['result']=='ACCEPTED',result
        for artifact in result['inputs']:
            assert digest(path.parent/artifact['path'])==artifact['sha256'],artifact
assembly=path.parent/record['output']['path']
assert assembly.stat().st_size>0 and digest(assembly)==record['output']['sha256']
assert all('PIPELINE_ACCEPTED entry=' in result['report']
           for result in record['stages'][-1]['results'])
"#,
        ])
        .arg(&work.0)
        .arg(&checker)
        .arg(env!("CARGO_BIN_EXE_evocc"))
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    oracle
        .stdin
        .take()
        .unwrap()
        .write_all(&output.stdout)
        .unwrap();
    assert!(
        oracle.wait().unwrap().success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "demo elapsed {:?}, expected less than 30 seconds",
        started.elapsed()
    );
}
