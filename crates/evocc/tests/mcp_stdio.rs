//! Drive the installed stdio server with an independent JSON implementation.
use std::path::Path;
use std::process::Command;

#[test]
fn stdio_client_checks_all_five_tools_and_protocol_errors() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let compiler = Path::new(env!("CARGO_BIN_EXE_evocc"));
    let result = Command::new("python3")
        .arg(root.join("tests/ui/commands/mcp_client.py"))
        .arg(compiler.with_file_name("evo"))
        .arg(&root)
        .env("EVOCC", compiler)
        .env(
            "EVOCC_PROOF_CHECKER",
            root.join("lean/.lake/build/bin/evoir-sem"),
        )
        .status()
        .unwrap();
    assert!(result.success());
}

#[test]
fn check_form_selects_the_record_and_omission_keeps_full_1_bytes() {
    let compiler = Path::new(env!("CARGO_BIN_EXE_evocc"));
    let script = r#"
import json, pathlib, subprocess, sys, tempfile
evo, compiler = sys.argv[1:]
env = {'EVOCC': compiler, 'PATH': '/usr/bin:/bin'}
with tempfile.TemporaryDirectory(prefix='evocc-mcp-form-') as temporary:
    work = pathlib.Path(temporary)
    source = work / 'input.c'
    source.write_text('int main(void) { int zero = 0; return 1 / zero; }\n')
    cli = lambda *a: subprocess.run([evo, 'check', str(source), '--json', *a], cwd=work, env=env,
                                    capture_output=True, check=False).stdout.decode().strip()
    calls = [{'file': str(source)}, {'file': str(source), 'form': 'full-1'},
             {'file': str(source), 'form': 'concise-1'}, {'file': str(source), 'form': 'concise-2'},
             {'file': str(source), 'form': 1}, {'file': str(source), 'form': None},
             {'file': str(source), 'form': ['concise-1']}]
    lines = [{'jsonrpc': '2.0', 'id': 0, 'method': 'initialize', 'params': {'protocolVersion': '2025-11-25',
              'capabilities': {}, 'clientInfo': {'name': 'form', 'version': '1'}}},
             {'jsonrpc': '2.0', 'id': 1, 'method': 'tools/list'}]
    lines += [{'jsonrpc': '2.0', 'id': 2 + i, 'method': 'tools/call', 'params': {'name': 'check', 'arguments': a}}
              for i, a in enumerate(calls)]
    lines.append({'jsonrpc': '2.0', 'id': 99, 'method': 'tools/call',
                  'params': {'name': 'explain', 'arguments': {'code': 'E3002', 'form': 'concise-1'}}})
    server = subprocess.run([evo, 'mcp'], input=''.join(json.dumps(l) + '\n' for l in lines), cwd=work,
                            env=env, capture_output=True, text=True, timeout=120)
    assert server.returncode == 0 and not server.stderr, server
    responses = {r['id']: r for r in map(json.loads, server.stdout.splitlines())}
    check = next(t for t in responses[1]['result']['tools'] if t['name'] == 'check')
    assert check['inputSchema'] == {'type': 'object', 'properties': {'file': {'type': 'string'},
        'form': {'type': 'string', 'enum': ['full-1', 'concise-1']}}, 'required': ['file'],
        'additionalProperties': False}, check
    full, concise = cli(), cli('--form=concise-1')
    assert full.startswith('{"exit_code":1,"completion":null,') and concise.startswith('{"form":"concise-1",')
    for ident, text in ((2, full), (3, full), (4, concise)):
        result = responses[ident]['result']
        assert result['content'] == [{'type': 'text', 'text': text}], (ident, result)
        assert result['structuredContent'] == json.loads(text) and result['isError'] is True
    for ident in (5, 6, 7, 8, 99):
        assert responses[ident]['error']['code'] == -32602, responses[ident]
    print('form: default and full-1 keep the CLI bytes; concise-1 is selected')
"#;
    let output = Command::new("python3")
        .args(["-c", script])
        .arg(compiler.with_file_name("evo"))
        .arg(compiler)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn agent_instructions_are_identical_and_name_the_safety_boundary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let agents = std::fs::read_to_string(root.join("AGENTS.md")).unwrap();
    assert!(agents.lines().count() <= 120);
    assert_eq!(
        agents,
        std::fs::read_to_string(root.join("llms.txt")).unwrap()
    );
    for rule in [
        "run evo check before evocc -c",
        "a review repair needs a human",
    ] {
        assert!(agents.to_ascii_lowercase().contains(rule));
    }
}
