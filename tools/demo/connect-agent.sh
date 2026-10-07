#!/bin/sh
# Exercise client configuration and discovery without starting a model turn.
exec python3 - <<'PY'
import json, os, pathlib, re, select, shutil, subprocess, tempfile, time

evo = pathlib.Path(os.environ.get('EVO') or shutil.which('evo') or '').resolve()
assert evo.is_file(), 'Put evo on PATH or set EVO.'
expected = {'check', 'explain', 'fix_plan', 'prove', 'run'}
with tempfile.TemporaryDirectory(prefix='evocc-connect-') as scratch:
    root = pathlib.Path(scratch).resolve()
    env = dict(os.environ, PATH=str(evo.parent) + os.pathsep + os.environ['PATH'])
    env.pop('EVO_BUILD_TRACE', None)
    def run(args, environment=env):
        result = subprocess.run(args, cwd=root, env=environment, capture_output=True, text=True, timeout=30)
        assert result.returncode == 0, (args, result.stdout, result.stderr)
        return result.stdout
    claude = shutil.which('claude')
    if claude:
        isolated = dict(env, CLAUDE_CONFIG_DIR=str(root / 'claude-config'))
        print(run([claude, '--version'], isolated).strip(), flush=True)
        print('$ claude mcp add --scope project evo -- evo mcp', flush=True)
        print(run([claude, 'mcp', 'add', '--scope', 'project', 'evo', '--', 'evo', 'mcp'], isolated), end='', flush=True)
        print('$ claude mcp get evo', flush=True)
        print(run([claude, 'mcp', 'get', 'evo'], isolated), end='', flush=True)
        configured = json.loads((root / '.mcp.json').read_text())['mcpServers']['evo']
        assert configured['command'] == 'evo' and configured['args'] == ['mcp']
    else:
        print('Claude Code: not installed; configuration only.', flush=True)
    codex = shutil.which('codex')
    if not codex:
        print('Codex: not installed; configuration only.', flush=True)
    else:
        print(run([codex, '--version']).strip(), flush=True)
        base = [codex, '-c', 'features.plugins=false', '-c', 'features.apps=false']
        for server in json.loads(run([*base, 'mcp', 'list', '--json'])):
            name = server['name']
            assert re.fullmatch(r'[A-Za-z0-9_-]+', name), 'unsupported config key in this demo'
            base += ['-c', 'mcp_servers.' + name + '.enabled=false']
        base += ['-c', 'mcp_servers.evo.enabled=true', '-c', 'mcp_servers.evo.command=' + json.dumps(str(evo)),
                 '-c', 'mcp_servers.evo.args=["mcp"]']
        configured = json.loads(run([*base, 'mcp', 'list', '--json']))
        assert {s['name'] for s in configured if s['enabled']} == {'evo'}
        print('Codex: only evo enabled in temporary command-line configuration.', flush=True)
        with tempfile.TemporaryFile() as errors:
            server = subprocess.Popen([*base, 'app-server', '--listen', 'stdio://'], cwd=root, env=env,
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, bufsize=0)
            def send(message):
                server.stdin.write(json.dumps(message).encode() + b'\n')
                server.stdin.flush()
            def receive(identifier):
                deadline = time.monotonic() + 30
                while time.monotonic() < deadline:
                    if not select.select([server.stdout], [], [], max(0, deadline-time.monotonic()))[0]:
                        break
                    line = server.stdout.readline()
                    assert line, 'client closed before its response'
                    message = json.loads(line)
                    if message.get('id') == identifier:
                        assert 'error' not in message, message
                        return message['result']
                raise AssertionError('client discovery timed out')
            try:
                send({'id': 0, 'method': 'initialize', 'params': {'clientInfo': {'name': 'evocc_demo', 'version': '1'}}})
                receive(0)
                send({'method': 'initialized', 'params': {}})
                send({'id': 1, 'method': 'mcpServerStatus/list', 'params': {'detail': 'toolsAndAuthOnly'}})
                catalog = receive(1)
                servers = [s for s in catalog['data'] if s['name'] == 'evo']
                assert len(servers) == 1 and set(servers[0]['tools']) == expected, catalog
                print('Codex tools: ' + json.dumps(sorted(servers[0]['tools'])), flush=True)
            finally:
                server.stdin.close()
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill(); server.wait()
                    raise
                errors.seek(0)
                assert server.returncode == 0, errors.read().decode(errors='replace')
print('No model turn or persistent client configuration was created.', flush=True)
PY
