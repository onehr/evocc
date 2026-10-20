# Agent interfaces

Run a check before compilation. Read JSON, not the English diagnostic text.
Review repairs need a human. A completed check covers one executed path.

```sh
evo check demo.c --json
evo fix demo.c --plan --json
evo explain E3002 --json
```

## Connect an agent

Put `evo` on the client's `PATH`. For Claude Code, run:

```sh
claude mcp add evo -- evo mcp
```

For Codex, add this to `~/.codex/config.toml`:

```toml
[mcp_servers.evo]
command = "evo"
args = ["mcp"]
```

Restart the client after a configuration change. Approve the local server when
the client asks. Use it only in a workspace where source and output access is allowed.
The [EvoCC skill](https://github.com/onehr/evocc/blob/main/skills/evocc/SKILL.md)
gives the check, repair and proof rules. A review repair still needs a human.

Both clients were installed on October 7, 2026. Claude Code 2.1.292 connected
and used all five tools in the [recorded model session](agent-loop.md#real-model-session).
The separate configuration test used `--scope project`; a fresh project correctly
reported pending approval. That state alone does not establish a connection.
Run `sh tools/demo/connect-agent.sh` from the repository to test temporary
client configuration and discovery without a model turn. Its Codex client used
the same command and arguments through process-local overrides, with other
servers disabled. This is the actual client-side discovery output:

```text
codex-cli 0.160.1
Codex: only evo enabled in temporary command-line configuration.
Codex tools: ["check", "explain", "fix_plan", "prove", "run"]
No model turn or persistent client configuration was created.
```

## Command records

See [An agent repair loop](agent-loop.md) for the scripted run and a real model session.

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
| `check --form=concise-1` | In this order: `form`, `status`: strings; `main_return`, `scope`: strings when main returned; `diagnostics`, `repairs`: arrays of concise records when nonempty; `exit_code`: integer. `--form=full-1` is the default `check` record |
| `explain` | `code`, `title`, `text`: strings; `exit_code`: integer |
| `prove` | `result`: record result string or null; `stages`: array of `{id,status}` strings; `record_path`: string or null; `proof_directory`: string; `checker`: chosen absolute path or null; `exit_code`: integer; `stderr`: string |
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
Checker lookup uses `EVOCC_PROOF_CHECKER`, then a sibling executable, then
`lean/.lake/build/bin/evoir-sem` under the nearest executable ancestor with
`Cargo.toml` and `lean/`, then PATH. `checker` is null if no checker was selected.

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
| `check` | `file`: string; optional `form`: `full-1` or `concise-1` | `evo check FILE --json [--form=FORM]` |
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
