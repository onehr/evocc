# Agent interfaces

Run a check before compilation. Read JSON, not the English diagnostic text.
Review repairs need a human. A completed check covers one executed path.

```sh
evo check demo.c --json
evo fix demo.c --plan --json
evo explain E3002 --json
```

## Command records

Place `--json` before the `--` separator. After `--`, it is a program or Make
argument. Each command writes one JSON object and a newline to standard output.
The process exit status is unchanged. JSON results do not use terminal colors.
The strings use UTF-8. Invalid UTF-8 output is an interface error, not replaced
data. Non-UTF-8 program arguments remain byte-exact at the CLI boundary.

| Verb | JSON fields |
|---|---|
| `help` | `usage`: string; `exit_code`: integer |
| `run` | `exit_code`: integer; `stdout`, `stderr`: captured command output strings |
| `check` | `exit_code`: integer; `completion`: the compiler completion object or null; `diagnostics`: array of diagnostic records |
| `explain` | `code`, `title`, `text`: strings; `exit_code`: integer |
| `prove` | `result`: record result string or null; `stages`: array of `{id,status}` strings; `record_path`: string or null; `proof_directory`: string; `exit_code`: integer; `stderr`: string |
| `build` | The same object as `evo-build-receipt.json`: `version`, `target`: strings; `sources`: string array; `warnings`: code-to-count object; `failing_command`: string array or null; `exit_code`: integer |
| `fix --plan` or `fix --apply` | `file`: string; `repairs`, `diagnostics`: arrays; `applied`, `exit_code`: integers |
| `version` | `version`: the full compiler version string; `exit_code`: integer |

Input and setup errors can return `{error: string, exit_code: integer}` instead.
The build receipt keeps the first version line. `version` returns the full text,
without outer whitespace. Diagnostic and repair fields are defined in
[the diagnostic specification](https://github.com/onehr/evocc/blob/main/spec/diagnostics.md).

`run` returns main's low eight bits. The interpreter does not implement arbitrary
external libc calls. A nonzero main return is not a check failure. `check` returns
0 when its path completes, even if main returns 7. Its completion object keeps
`schema`, `kind`, `status`, `main_return` and `claim` from the compiler.

`prove` reads the retained `record.json`. It does not infer success from a log.
Its stage summary preserves `NOT_CHECKED` rows. When no record exists, `result`
and `record_path` are null. A checked record with a nonzero exit can mean that
copying the output failed. Do not treat that case as a published output.
The full record retains the proof scope and trusted boundaries.

## Stdio tools

Start `evo mcp`. It uses the
[MCP 2025-11-25 initialize lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle)
and newline-delimited JSON-RPC 2.0 on standard input and output. It does not use
a network listener or a daemon. It accepts `initialize`, `tools/list` and
`tools/call`. Notifications receive no reply and cannot call a tool. Close input
to end the server. The server negotiates version `2025-11-25`; it does not claim
the later, per-request-metadata protocol.

| Tool | Arguments | Command |
|---|---|---|
| `run` | `file`: string; optional `args`: string array | `evo run FILE --json [-- ARGS]` |
| `check` | `file`: string | `evo check FILE --json` |
| `explain` | `code`: string | `evo explain CODE --json` |
| `prove` | `file`, `output`: strings; optional `entries`: string array | `evo prove FILE -o OUTPUT --json [--entry NAME ...]` |
| `fix_plan` | `file`: string | `evo fix FILE --plan --json` |

Tool arguments reject extra fields. File paths resolve from the server's working
directory. Arguments are passed directly, not through a shell. There is no
fix-apply tool. The server has the process's filesystem permissions; use a trusted
workspace. A prove call writes its requested output and retains proof files.

The tool result has `structuredContent` equal to the command record, one text
`content` item with the same JSON, and `isError=true` for a nonzero command exit.
Protocol errors use JSON-RPC error codes: -32700 for malformed JSON, -32600 for
invalid requests, -32601 for unknown methods, -32602 for bad tool arguments,
-32603 for a command-interface failure, and -32002 before initialization.
Frames, including their newline, are limited to 1 MiB and 128 nested values. Duplicate object keys and
unpaired Unicode surrogates are rejected. Numeric request IDs retain all digits.

## Example session

Send these lines to `evo mcp`. The test client also exercises all five tools.

```json
{"jsonrpc":"2.0","id":"start","method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"example","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"explain","arguments":{"code":"E3002"}}}
```

The first response is:

```json
{"jsonrpc":"2.0","id":"start","result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"evo","version":"0.1.0"}}}
```

The notification has no response. Request 2 lists the five tools above. Request 3
returns the E3002 explanation in `structuredContent` and in the text content.
Use [AGENTS.md](https://github.com/onehr/evocc/blob/main/AGENTS.md), also supplied as [llms.txt](https://github.com/onehr/evocc/blob/main/llms.txt), for
repository commands, reproduction rules and the human-review boundary.
