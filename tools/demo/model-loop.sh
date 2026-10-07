#!/bin/sh
# One real model session; authentication must already be available to Claude Code.
exec python3 - "$0" <<'PY'
import json, os, pathlib, shlex, shutil, subprocess, sys, tempfile

repo = pathlib.Path(sys.argv[1]).resolve().parents[2]
evo = os.environ.get("EVO") or shutil.which("evo")
client = shutil.which("claude")
if not evo or not client:
    sys.exit("Put evo and Claude Code on PATH before running this model session.")
work = pathlib.Path(tempfile.mkdtemp(prefix="evocc-model-loop-")).resolve()
shutil.copyfile(repo / "tools/demo/loop/buggy.c", work / "demo.c")
environment = {key: os.environ[key] for key in ("EVOCC", "EVOCC_PROOF_CHECKER") if key in os.environ}
environment["TMPDIR"] = str(work)
config = {"mcpServers": {"evo": {"command": str(pathlib.Path(evo).resolve()),
                               "args": ["mcp"], "env": environment}}}
prompt = """Work only on demo.c in the current directory. Use the evo MCP tools,
not shell commands. First check it, read E3004 with explain, and obtain fix_plan.
Use Read and Edit to apply its safe repairs only; review repairs need a human.
For E3004 on the unary minus in magnitude, the owner authorizes this one repair:
insert `if (a == -2147483647 - 1) return 2147483647;` as its first statement.
Keep main's input unchanged. Re-check after edits, at most five check rounds.
After a successful check, prove demo.c to output.s and run demo.c through MCP.
Do not call prove on a different source. Report the actual proof result and
record path. A successful check covers only the executed path. Do not claim the
rewrite stage is checked when it says NOT_CHECKED. Do not modify other files.
This session is recorded; report failures without hiding them."""
argv = [client, "--print", "--restricted", "--strict-mcp-config", "--mcp-config", json.dumps(config),
        "--tools", "Read,Edit", "--allowedTools", "Read", "Edit(./demo.c)",
        "mcp__evo__check", "mcp__evo__explain", "mcp__evo__fix_plan", "mcp__evo__prove", "mcp__evo__run",
        "--permission-mode", "dontAsk", "--permission-prompts", "none", "--max-turns", "16",
        "--max-budget-usd", "5", "--no-session-persistence", "--output-format", "stream-json", "--verbose"]
print("Model workspace: " + str(work), file=sys.stderr, flush=True)
print("$ " + shlex.join(argv), file=sys.stderr, flush=True)
sys.exit(subprocess.run([*argv, prompt], cwd=work).returncode)
PY
