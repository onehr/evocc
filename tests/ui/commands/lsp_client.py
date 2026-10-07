"""Exercise LSP framing over real pipes without a protocol SDK or editor extension."""
import json
import os
import pathlib
import select
import subprocess
import sys
import tempfile
import time

executable, repository = sys.argv[1:]
assert pathlib.Path(repository, "spec/diagnostics.md").is_file()


class Client:
    def __init__(self, workspace, *flags):
        self.process = subprocess.Popen(
            [executable, "lsp", *flags], cwd=workspace, stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.buffer = b""
        self.pending = []
        self.ident = 0

    def send(self, method, params=None, ident=None):
        message = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            message["params"] = params
        if ident is not None:
            message["id"] = ident
        body = json.dumps(message, ensure_ascii=False).encode("utf-8")
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.process.stdin.flush()

    def receive(self, timeout=20):
        deadline = time.monotonic() + timeout
        while True:
            if b"\r\n\r\n" in self.buffer:
                header, body = self.buffer.split(b"\r\n\r\n", 1)
                fields = dict(line.split(b":", 1) for line in header.split(b"\r\n"))
                length = int(fields[b"Content-Length"].strip())
                assert 0 < length < 1048576, length
                if len(body) >= length:
                    self.buffer = body[length:]
                    result = json.loads(body[:length])
                    assert result["jsonrpc"] == "2.0", result
                    return result
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([self.process.stdout], [], [], remaining)[0]:
                raise TimeoutError("LSP response timed out")
            chunk = os.read(self.process.stdout.fileno(), 65536)
            assert chunk, (self.process.poll(), self.process.stderr.read().decode())
            self.buffer += chunk

    def request(self, method, params=None):
        self.ident += 1
        self.send(method, params, self.ident)
        while True:
            message = self.receive()
            if message.get("id") == self.ident:
                return message
            assert message.get("method") == "textDocument/publishDiagnostics", message
            self.pending.append(message)

    def diagnostics(self, uri):
        message = self.pending.pop(0) if self.pending else self.receive()
        assert message["method"] == "textDocument/publishDiagnostics", message
        params = message["params"]
        assert params["uri"] == uri, params
        assert isinstance(params["diagnostics"], list), params
        for diagnostic in params["diagnostics"]:
            assert diagnostic["message"] and diagnostic["code"], diagnostic
            assert diagnostic["severity"] in (1, 2, 3, 4), diagnostic
            for end in ("start", "end"):
                assert diagnostic["range"][end]["line"] >= 0, diagnostic
                assert diagnostic["range"][end]["character"] >= 0, diagnostic
        return params["diagnostics"]

    def quiet(self, duration):
        assert not self.pending, self.pending
        try:
            message = self.receive(duration)
        except TimeoutError:
            return
        raise AssertionError(f"unexpected publication before debounce elapsed: {message}")

    def initialize(self, workspace):
        result = self.request("initialize", {
            "processId": None, "rootUri": workspace.as_uri(),
            "workspaceFolders": [{"uri": workspace.as_uri(), "name": "test"}],
            "capabilities": {"general": {"positionEncodings": ["utf-16"]},
                             "textDocument": {"codeAction": {
                                 "codeActionLiteralSupport": {"codeActionKind": {
                                     "valueSet": ["quickfix"]}}}}}})["result"]
        caps = result["capabilities"]
        assert caps.get("positionEncoding", "utf-16") == "utf-16", caps
        sync = caps["textDocumentSync"]
        assert sync == 1 or (sync["change"] == 1 and sync["openClose"]), caps
        assert caps["codeActionProvider"], caps
        assert not caps.get("executeCommandProvider"), caps
        self.send("initialized", {})

    def open(self, path, text, version=1):
        self.send("textDocument/didOpen", {"textDocument": {
            "uri": path.as_uri(), "languageId": "c", "version": version, "text": text}})

    def change(self, path, text, version):
        self.send("textDocument/didChange", {
            "textDocument": {"uri": path.as_uri(), "version": version},
            "contentChanges": [{"text": text}]})

    def actions(self, path, diagnostic):
        return self.request("textDocument/codeAction", {
            "textDocument": {"uri": path.as_uri()}, "range": diagnostic["range"],
            "context": {"diagnostics": [diagnostic], "only": ["quickfix"]}})["result"]

    def close(self, path):
        self.send("textDocument/didClose", {"textDocument": {"uri": path.as_uri()}})
        assert self.diagnostics(path.as_uri()) == []

    def stop(self):
        response = self.request("shutdown")
        assert response["result"] is None, response
        self.send("exit")
        assert self.process.wait(timeout=10) == 0
        assert not self.buffer and not self.pending
        assert not self.process.stdout.read()
        assert not self.process.stderr.read()

    def cleanup(self):
        if self.process.poll() is None:
            self.process.kill()
            self.process.wait()


def utf16_offset(text, position):
    lines = text.splitlines(keepends=True)
    line, character = position["line"], position["character"]
    prefix = "".join(lines[:line])
    if line == len(lines):
        assert character == 0
        return len(text)
    encoded = lines[line].encode("utf-16-le")
    return len(prefix) + len(encoded[:character * 2].decode("utf-16-le"))


def apply_workspace_edit(text, uri, edit):
    if "changes" in edit:
        assert set(edit["changes"]) == {uri}, edit
        changes = edit["changes"][uri]
    else:
        documents = edit["documentChanges"]
        assert len(documents) == 1 and documents[0]["textDocument"]["uri"] == uri, edit
        changes = documents[0]["edits"]
    spans = [(utf16_offset(text, e["range"]["start"]),
              utf16_offset(text, e["range"]["end"]), e["newText"]) for e in changes]
    for start, end, replacement in sorted(spans, reverse=True):
        text = text[:start] + replacement + text[end:]
    return text


with tempfile.TemporaryDirectory(prefix="evocc-lsp-client-") as temporary:
    work = pathlib.Path(temporary).resolve() / "project space \u03b1"
    work.mkdir()
    source = work / "input space \u03b1.c"
    disk_text = "int main(void) { return 0; }\n"
    source.write_text(disk_text)
    bad = "enum { result = 1 / 0 };\nint main(void) { return 0; }\n"
    good = "enum { result = 1 / 1 };\nint main(void) { return 0; }\n"
    client = Client(work)
    try:
        client.initialize(work)
        client.open(source, bad)
        diagnostic = client.diagnostics(source.as_uri())
        assert len(diagnostic) == 1 and diagnostic[0]["code"] == "E1303", diagnostic
        assert diagnostic[0]["severity"] == 1
        assert diagnostic[0]["range"] == {
            "start": {"line": 0, "character": 16}, "end": {"line": 0, "character": 21}}
        assert source.read_text() == disk_text

        client.change(source, good, 2)
        assert client.diagnostics(source.as_uri()) == []
        # Full replacements coalesce: an intermediate invalid buffer must not publish.
        client.change(source, bad, 3)
        client.quiet(0.12)
        client.change(source, good, 4)
        client.quiet(0.20)
        assert client.diagnostics(source.as_uri()) == []
        client.send("textDocument/didSave", {"textDocument": {"uri": source.as_uri()}})
        assert client.diagnostics(source.as_uri()) == []

        # Syntax checking must not run unsaved C or claim runtime UB detection.
        client.change(source, "int main(void) { return 1 / 0; }\n", 5)
        assert client.diagnostics(source.as_uri()) == []

        warning = "/* \U0001f600 */ int f(int unused) { return 0; }\n"
        client.change(source, warning, 6)
        diagnostics = client.diagnostics(source.as_uri())
        unused = next(d for d in diagnostics if d["code"] == "W0002")
        assert unused["severity"] == 2
        assert unused["range"] == {
            "start": {"line": 0, "character": 19}, "end": {"line": 0, "character": 25}}
        actions = client.actions(source, unused)
        assert len(actions) == 1, actions
        action = actions[0]
        assert action["title"] and action["kind"] == "quickfix" and "command" not in action
        edited = apply_workspace_edit(warning, source.as_uri(), action["edit"])
        assert edited == "/* \U0001f600 */ int f(int unused) { (void)unused; return 0; }\n", edited
        assert source.read_text() == disk_text
        client.change(source, edited, 7)
        assert client.diagnostics(source.as_uri()) == []

        # A P07 repair marked review is not an automatic editor action.
        client.change(source, "int f(void) { }\n", 8)
        review = next(d for d in client.diagnostics(source.as_uri()) if d["code"] == "W0005")
        assert client.actions(source, review) == []
        assert source.read_text() == disk_text
        client.close(source)
        assert client.request("unsupported/request")["error"]["code"] == -32601
        client.stop()
        assert set(work.iterdir()) == {source}, list(work.iterdir())
    finally:
        client.cleanup()

    # --json must preserve the framed stream, as it does for the MCP stream.
    # Match a relative compile-database file and resolve -I relative to directory.
    include = work / "include space"
    include.mkdir()
    header = include / "config.h"
    header.write_text("#define HEADER_VALUE 7\n")
    other = work / "other.c"
    other.write_text(disk_text)
    database = work / "compile_commands.json"
    database.write_text(json.dumps([
        {"directory": str(work), "file": other.name,
         "arguments": ["evocc", "-DP09_VALUE=0", "-c", other.name, "-o", "other.o"]},
        {"directory": str(work), "file": source.name,
         "arguments": ["evocc", "-I", str(include.relative_to(work)), "-DP09_VALUE=1",
                       "-c", source.name, "-o", "input.o"]}]))
    snapshot = {path: path.read_bytes() for path in work.rglob("*") if path.is_file()}
    client = Client(work, "--json")
    try:
        client.initialize(work)
        client.open(source, '#include "config.h"\n'
                    'enum { value = HEADER_VALUE / P09_VALUE };\n'
                    'int main(void) { return value; }\n')
        assert client.diagnostics(source.as_uri()) == []
        client.close(source)
        client.stop()
        assert {path: path.read_bytes() for path in work.rglob("*") if path.is_file()} == snapshot
    finally:
        client.cleanup()
