//! Experiment 4's feedback and byte counts stay frozen. Current records must
//! match except for scratch paths and the corrected execution-check scope.
//! This comparison does not establish byte-identical historical replay; such
//! a replay needs the compiler recorded by that experiment.
//!
//! `UPDATE_EXPECT=1` records the `concise-1` fixture. `EVOCC_RECORD_EVO` can
//! name the `evo` binary that records it; its sibling `evocc` checks.
use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT: &str = r#"
import hashlib, json, os, pathlib, re, shutil, subprocess, sys, tempfile

repo, evo, compiler, mode = sys.argv[1:]
repo = pathlib.Path(repo).resolve()
corpus = repo / 'tests/fixtures/agent-repair-4'
qualification_path = corpus / 'qualification.json'
qualification = json.loads(qualification_path.read_text())
fixture = corpus / 'concise-1-feedback.json'
marker = '=== evo diagnostics\n'
# The environment of tools/experiment/qualify_silent.py, which wrote the strings.
env = {'PATH': '/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin', 'TMPDIR': tempfile.gettempdir(),
       'EVOCC': compiler, 'EVOCC_SYSROOT': str(repo / 'sysroot/aarch64-unknown-linux-gnu')}
assert len(qualification['items']) == 24, len(qualification['items'])
digest = lambda path: hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()

def feedback(source, form):
    # The C3 commands of tools/experiment/agent_repair.py in a fresh scratch directory.
    with tempfile.TemporaryDirectory(prefix='silent-feedback-') as temporary:
        stage = pathlib.Path(temporary)
        stage.chmod(0o755)
        target = stage / 'input.c'
        shutil.copyfile(source, target)
        run = lambda *argv: subprocess.run([evo, *argv], cwd=stage, env=env, capture_output=True)
        if form == 'concise-1':
            records = [run('check', '--json', '--form=concise-1', str(target))]
        else:
            check = run('check', '--json', str(target))
            codes = sorted({d['code'] for line in check.stdout.decode().splitlines() if line.strip()
                            for d in json.loads(line).get('diagnostics', [])})
            records = [check] + [run('explain', code, '--json') for code in codes]
            records.append(run('fix', str(target), '--plan', '--json'))
        text = b''.join(r.stdout if r.stdout.strip() else r.stderr for r in records)
        return text.decode(), str(stage)

def frozen(item, version):
    recorded = item['feedback'][version]['c3']
    assert recorded.count(marker) == 1, (item['item'], version)
    part = recorded.split(marker, 1)[1]
    scratch = set(re.findall(r'(/[^"\s]*/silent-feedback-[^/"\s]+)/input\.c', part))
    assert len(scratch) == 1, (item['item'], version, scratch)
    return part, scratch.pop()

def current_scope(text):
    # Keep the historical fixture unchanged. Apply only the documented scope
    # correction to completed records; every other byte is still compared.
    old = 'No undefined behavior was found on the executed path. This does not check that the results are correct.'
    new = 'No finding from the implemented checks on the executed path. Pointer subtraction and ordering obligations are not checked. This does not check that the results are correct.'
    lines = []
    for line in text.splitlines(keepends=True):
        record = json.loads(line)
        completion = record.get('completion') or record
        if completion.get('status') == 'completed' and completion.get('scope') == old:
            field = '"scope":' + json.dumps(old)
            assert line.count(field) == 1
            line = line.replace(field, '"scope":' + json.dumps(new))
        lines.append(line)
    return ''.join(lines)

failures = []
if mode == 'full-1':
    for item in qualification['items']:
        for version in ('program.c', 'fixed.c'):
            part, scratch = frozen(item, version)
            fresh, stage = feedback(corpus / item['item'] / version, 'full-1')
            expected, actual = current_scope(part).replace(scratch, '<scratch>'), fresh.replace(stage, '<scratch>')
            if actual != expected:
                failures.append((item['item'], version, expected, actual))
    for name, version, expected, actual in failures:
        print('DIFFERENCE', name, version, '\nfrozen: ', repr(expected), '\ncurrent:', repr(actual))
    assert not failures, '%d of 48 C3 records differ beyond the corrected scope' % len(failures)
    print('48 of 48 C3 records match after the explicit completion-scope correction')
    sys.exit(0)

if mode == 'record':
    items = []
    for item in qualification['items']:
        versions = {}
        for version in ('program.c', 'fixed.c'):
            part, _ = frozen(item, version)
            text, stage = feedback(corpus / item['item'] / version, 'concise-1')
            versions[version] = {'scratch': stage, 'concise-1': text,
                                 'bytes': {'full-1': len(part.encode()), 'concise-1': len(text.encode())}}
        items.append({'item': item['item'], 'code': item['code'], 'feedback': versions})
    version_line = subprocess.run([evo, 'version'], env=env, capture_output=True, check=True).stdout.decode().splitlines()[0]
    place = lambda path: str(pathlib.Path(path).resolve().relative_to(repo)) if pathlib.Path(path).resolve().is_relative_to(repo) else str(path)
    totals = {form: sum(v['bytes'][form] for i in items for v in i['feedback'].values()) for form in ('full-1', 'concise-1')}
    record = {
        'schema': 'agent-repair.concise-feedback.v1',
        'form': 'concise-1',
        'qualification': {'path': 'tests/fixtures/agent-repair-4/qualification.json', 'sha256': digest(qualification_path)},
        'command': ['evo', 'check', '--json', '--form=concise-1', '<scratch>/input.c'],
        'version': version_line,
        'binaries': {'evo': {'path': place(evo), 'sha256': digest(evo)},
                     'evocc': {'path': place(compiler), 'sha256': digest(compiler)}},
        'bytes': totals,
        'items': items,
    }
    fixture.write_text(json.dumps(record, indent=2) + '\n')
    print('recorded', fixture, totals)

record = json.loads(fixture.read_text())
assert record['schema'] == 'agent-repair.concise-feedback.v1' and record['form'] == 'concise-1'
assert record['qualification']['sha256'] == digest(qualification_path), 'the qualification receipt changed'
assert record['version'].startswith('evocc '), record['version']
for binary in record['binaries'].values():
    assert re.fullmatch('[0-9a-f]{64}', binary['sha256']), binary
assert [(i['item'], i['code']) for i in record['items']] == [(i['item'], i['code']) for i in qualification['items']]
totals = {'full-1': 0, 'concise-1': 0}
for item, original in zip(record['items'], qualification['items']):
    assert list(item['feedback']) == ['program.c', 'fixed.c'], item['item']
    for version, entry in item['feedback'].items():
        part, _ = frozen(original, version)
        text = entry['concise-1']
        assert text.startswith('{"form":"concise-1"') and text.endswith('}\n') and text.count('\n') == 1, text
        assert entry['bytes'] == {'full-1': len(part.encode()), 'concise-1': len(text.encode())}, item['item']
        for form in totals:
            totals[form] += entry['bytes'][form]
        fresh, stage = feedback(corpus / item['item'] / version, 'concise-1')
        expected, actual = current_scope(text).replace(entry['scratch'], '<scratch>'), fresh.replace(stage, '<scratch>')
        if actual != expected:
            failures.append((item['item'], version, expected, actual))
assert record['bytes'] == totals, (record['bytes'], totals)
for name, version, expected, actual in failures:
    print('DIFFERENCE', name, version, '\nrecorded:', repr(expected), '\ncurrent: ', repr(actual))
assert not failures, '%d of 48 concise-1 records differ beyond the corrected scope' % len(failures)
print('48 of 48 concise-1 records match after the explicit completion-scope correction; historical bytes:', totals)
"#;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn script(mode: &str, evo: &Path) {
    let output = Command::new("python3")
        .args(["-c", SCRIPT])
        .arg(root())
        .arg(evo)
        .arg(evo.with_file_name("evocc"))
        .arg(mode)
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

#[test]
fn frozen_full_1_records_match_except_for_the_corrected_completion_scope() {
    script("full-1", Path::new(env!("CARGO_BIN_EXE_evo")));
}

#[test]
fn recorded_concise_1_records_match_except_for_the_corrected_completion_scope() {
    let update = std::env::var("UPDATE_EXPECT").as_deref() == Ok("1");
    let evo = std::env::var_os("EVOCC_RECORD_EVO")
        .filter(|_| update)
        .map_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_evo")), PathBuf::from);
    script(if update { "record" } else { "concise-1" }, &evo);
}
