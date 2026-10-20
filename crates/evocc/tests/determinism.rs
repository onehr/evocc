//! Determinism is bounded to these fixed inputs, binaries and preprocessing context.
use std::path::Path;
use std::process::Command;

#[test]
fn fixed_inputs_have_byte_identical_agent_results() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("python3")
        .args([
            "-c",
            r#"
import hashlib, json, os, pathlib, subprocess, sys, tempfile

repo = pathlib.Path(sys.argv[1]).resolve()
evo, compiler = map(pathlib.Path, sys.argv[2:])
checker = repo / 'lean/.lake/build/bin/evoir-sem'
assert checker.is_file(), 'The real Lean checker must already be built.'
buggy = repo / 'tools/demo/loop/buggy.c'
positive = repo / 'tests/fixtures/verified-eqsat-chain.c'
proof_source = repo / 'tests/fixtures/verification/musl-abs.c'
originals = {p: p.read_bytes() for p in (buggy, positive, proof_source)}
digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
compiler_digest, checker_digest = digest(compiler), digest(checker)
environment = dict(os.environ, EVOCC=str(compiler), EVOCC_PROOF_CHECKER=str(checker))
environment.pop('EVO_BUILD_TRACE', None)
environment['SOURCE_DATE_EPOCH'] = '0'

def difference(a, b, field='$'):
    if type(a) != type(b): return field
    if isinstance(a, dict):
        if list(a) != list(b): return field + '.keys'
        for key in a:
            found = difference(a[key], b[key], field + '.' + key)
            if found: return found
    elif isinstance(a, list):
        if len(a) != len(b): return field + '.length'
        for i, (left, right) in enumerate(zip(a, b)):
            found = difference(left, right, field + '[' + str(i) + ']')
            if found: return found
    elif a != b: return field
    return None

def no_timestamps(value, field='$'):
    if isinstance(value, dict):
        for key, item in value.items():
            assert key.lower() not in {'timestamp', 'created_at', 'updated_at', 'generated_at'}, (field, key)
            no_timestamps(item, field + '.' + key)
    elif isinstance(value, list):
        for i, item in enumerate(value): no_timestamps(item, field + '[' + str(i) + ']')

cases = (
    ('buggy check', ['check', str(buggy)], 1),
    ('fix plan', ['fix', str(buggy), '--plan'], 0),
    ('explain', ['explain', 'E3004'], 0),
    ('positive check', ['check', str(positive)], 0),
    ('concise check', ['check', str(buggy), '--form=concise-1'], 1),
    ('concise positive check', ['check', str(positive), '--form=concise-1'], 0),
    ('run', ['run', str(positive)], 0),
    ('prove', ['prove', str(proof_source)], 0),
)
for name, arguments, expected in cases:
    runs = []
    for _ in range(2):
        with tempfile.TemporaryDirectory(prefix='evocc-determinism-') as temporary:
            root = pathlib.Path(temporary).resolve()
            env = dict(environment, TMPDIR=str(root))
            argv = [str(evo), *arguments, '--json']
            if name == 'prove': argv += ['-o', str(root / 'requested.s')]
            run = subprocess.run(argv, env=env, capture_output=True)
            assert run.returncode == expected and not run.stderr, (name, run)
            assert len(run.stdout.splitlines()) == 1, (name, run.stdout)
            result = json.loads(run.stdout)
            assert result['exit_code'] == run.returncode, (name, result)
            raw = run.stdout
            if name == 'buggy check':
                assert result['completion'] is None
                assert {'W0004', 'E3004'} <= {d['code'] for d in result['diagnostics']}
            elif name == 'fix plan':
                assert result['applied'] == 0 and result['file'] == str(buggy)
                assert {r['repair_id'] for r in result['repairs']} == {
                    'W0004.insert-int', 'W0001.remove-declaration'}
                assert all(r['kind'] == 'safe' for r in result['repairs'])
            elif name == 'explain':
                assert result['code'] == 'E3004' and result['title']
                assert result['text'].startswith('# E3004: ')
            elif name == 'positive check':
                assert result['diagnostics'] == []
                assert result['completion']['claim'] == 'executed-path-only'
                assert result['completion']['main_return'] == '0'
            elif name == 'concise check':
                assert list(result) == ['form', 'status', 'diagnostics', 'repairs', 'exit_code'], result
                assert result['form'] == 'concise-1' and result['status'] == 'stopped'
                assert [d['code'] for d in result['diagnostics']] == ['W0004', 'E3004']
                assert {r['id'] for r in result['repairs']} == {
                    'W0004.insert-int', 'W0001.remove-declaration'}
            elif name == 'concise positive check':
                assert list(result)[:4] == ['form', 'status', 'main_return', 'scope'], result
                assert result['status'] == 'completed' and result['main_return'] == '0'
                assert 'diagnostics' not in result and list(result)[-1] == 'exit_code'
            elif name == 'run':
                assert result == {'exit_code': 0, 'stdout': '', 'stderr': ''}
            else:
                assert result['result'] == 'CHECKED_TO_FINAL_ASSEMBLY', result
                path = pathlib.Path(result['record_path']).resolve()
                directory = pathlib.Path(result['proof_directory']).resolve()
                assert directory.parent == root and path == directory / 'checked/record.json'
                raw = path.read_bytes()
                record = json.loads(raw)
                no_timestamps(record)
                assert record['schema'] == 'evocc-proof-required-record-v1'
                assert record['result'] == result['result'] and record['profile'] == 'core-v1'
                assert record['all_stages_checked'] and record['checker_digest_pinned']
                assert record['source'] == {'path': str(proof_source), 'sha256': digest(proof_source)}
                assert record['compiler']['sha256'] == compiler_digest
                assert record['checker']['sha256'] == checker_digest
                assert [e['entry'] for e in record['entries']] == ['abs']
                assert [(s['id'], s['status']) for s in record['stages']] == [
                    ('source-to-evoir', 'CHECKED'), ('rewrite', 'NOT_CHECKED'),
                    ('lowering', 'CHECKED'), ('allocation', 'CHECKED'),
                    ('frame-emission', 'CHECKED'), ('pipeline', 'CHECKED')]
                assembly = path.parent / record['output']['path']
                assert record['output']['checked'] and digest(assembly) == record['output']['sha256']
                assert (root / 'requested.s').read_bytes() == assembly.read_bytes()
            runs.append(raw.replace(str(root).encode(), b'<dir>'))
    if runs[0] != runs[1]:
        field = difference(json.loads(runs[0]), json.loads(runs[1])) or 'raw JSON formatting'
        raise AssertionError(name + ': non-path difference at ' + field + '\n' + repr(runs))
    print(name + ': two byte-identical runs', flush=True)
assert all(p.read_bytes() == source for p, source in originals.items())
assert digest(compiler) == compiler_digest and digest(checker) == checker_digest
"#,
        ])
        .arg(repository)
        .arg(env!("CARGO_BIN_EXE_evo"))
        .arg(env!("CARGO_BIN_EXE_evocc"))
        .env_remove("EVO_BUILD_TRACE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}
