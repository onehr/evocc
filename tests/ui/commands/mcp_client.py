"""Exercise newline JSON-RPC over real pipes. No protocol SDK is used."""
import json
import pathlib
import select
import shutil
import subprocess
import sys
import tempfile

executable, repository = sys.argv[1:]
root = pathlib.Path(repository)
server = subprocess.Popen([executable, "mcp"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def send(value):
    server.stdin.write(value if isinstance(value, bytes) else (json.dumps(value) + "\n").encode())
    server.stdin.flush()


def receive():
    assert select.select([server.stdout], [], [], 30)[0], "stdio response timed out"
    line = server.stdout.readline()
    assert line, (server.poll(), server.stderr.read())
    return json.loads(line)


def request(method, params=None, ident=1):
    value = {"jsonrpc": "2.0", "id": ident, "method": method}
    if params is not None:
        value["params"] = params
    send(value)
    result = receive()
    assert result["jsonrpc"] == "2.0" and result["id"] == ident, result
    return result


def remove_proof(record):
    directory = pathlib.Path(record["proof_directory"])
    assert directory.name.startswith("evo-proof-")
    assert directory.parent.resolve() == pathlib.Path(tempfile.gettempdir()).resolve()
    shutil.rmtree(directory)


try:
    assert request("tools/list")["error"]["code"] == -32002
    initial = request("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                     "clientInfo": {"name": "test", "version": "1"}}, "start")
    assert initial == json.loads((root / "tests/ui/commands/mcp.json").read_text())
    send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    # A notification must produce no response, including an unknown notification.
    send({"jsonrpc": "2.0", "method": "notifications/unused"})
    tools = request("tools/list", ident=2 ** 100)["result"]["tools"]
    assert [tool["name"] for tool in tools] == ["run", "check", "explain", "prove", "fix_plan"]
    for tool in tools:
        assert tool["inputSchema"]["type"] == "object"
        assert tool["inputSchema"]["additionalProperties"] is False
        assert tool["description"]

    with tempfile.TemporaryDirectory(prefix="evocc-stdio-client-") as temporary:
        work = pathlib.Path(temporary)
        source = work / 'input_\u03b1".c'
        contents = "int f(int unused) { return 0; } int main(void) { return 7; }\n"
        source.write_text(contents)
        output = work / "output.s"
        arguments = {"run": {"file": str(source), "args": ["a b", "--json", ""]},
                     "check": {"file": str(source)}, "explain": {"code": "E3002"},
                     "prove": {"file": str(source), "output": str(output)},
                     "fix_plan": {"file": str(source)}}
        for name, args in arguments.items():
            result = request("tools/call", {"name": name, "arguments": args}, name)["result"]
            record = result["structuredContent"]
            assert json.loads(result["content"][0]["text"]) == record
            assert result["isError"] == (record["exit_code"] != 0)
            if name == "run":
                assert record == {"exit_code": 7, "stdout": "", "stderr": ""}
            elif name == "check":
                assert record["completion"]["main_return"] == "7" and not record["diagnostics"]
            elif name == "explain":
                assert record["code"] == "E3002" and record["title"] == "integer-division-by-zero"
            elif name == "prove":
                proof = json.loads(pathlib.Path(record["record_path"]).read_text())
                assert record["result"] == proof["result"] == "CHECKED_TO_FINAL_ASSEMBLY"
                assert output.is_file()
                remove_proof(record)
            else:
                assert any(r["repair_id"] == "W0002.void-cast" for r in record["repairs"])
                assert record["applied"] == 0 and source.read_text() == contents
        for bad in [{"name": "fix", "arguments": {"file": str(source)}},
                    {"name": "fix_plan", "arguments": {"file": str(source), "review": True}},
                    {"name": "run", "arguments": {"file": str(source), "args": [1]}},
                    {"name": "check", "arguments": {"file": False}}]:
            assert request("tools/call", bad)["error"]["code"] == -32602
        assert source.read_text() == contents
        failed = request("tools/call", {"name": "check", "arguments": {"file": str(work / "absent.c")}})
        assert failed["result"]["isError"]

    # Use a supplementary Unicode letter, not a host-dependent byte conversion.
    assert request("tools/list", ident="\U00010400")["result"]["tools"] == tools
    for message in [b'{"jsonrpc":"2.0","id":1,"method":"tools/list",}\n',
                    b'{"jsonrpc":"2.0","jsonrpc":"2.0","id":1,"method":"tools/list"}\n',
                    b'{"jsonrpc":"2.0","id":"\\ud800","method":"tools/list"}\n',
                    b'{"jsonrpc":"2.0","id":01,"method":"tools/list"}\n',
                    b'"' + b'a' * 1048576 + b'"\n', b'\xff\n']:
        send(message)
        error = receive()
        assert error["id"] is None and error["error"]["code"] == -32700
    assert request("prompts/list")["error"]["code"] == -32601
    assert request("tools/list")["result"]["tools"] == tools
    assert request("tools/list", params=7)["error"]["code"] == -32602
    send({"jsonrpc": "1.0", "id": [], "method": "tools/list"})
    assert receive()["error"]["code"] == -32600
    server.stdin.close()
    assert server.wait(timeout=10) == 0
    assert not server.stderr.read()
    # Run the book's exact session, not a separately maintained approximation.
    blocks = (root / "book/src/agents.md").read_text().split("```json\n")[1:]
    transcript = subprocess.run([executable, "mcp"], input=blocks[0].split("```", 1)[0],
                                text=True, capture_output=True, timeout=30)
    assert transcript.returncode == 0 and not transcript.stderr
    responses = [json.loads(line) for line in transcript.stdout.splitlines()]
    assert len(responses) == 3
    assert responses[0] == json.loads(blocks[1].split("```", 1)[0])
    assert responses[1]["id"] == 2 and responses[1]["result"]["tools"] == tools
    assert responses[2]["id"] == 3
    assert responses[2]["result"]["structuredContent"]["code"] == "E3002"
finally:
    if server.poll() is None:
        server.kill()
        server.wait()
