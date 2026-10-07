#!/bin/sh
# Keep the fixture intact. The transcript names the retained temporary output.
exec python3 - "$0" <<'PY'
import difflib, json, os, pathlib, re, shlex, shutil, subprocess, sys, tempfile

print("Scripted agent (no model).", flush=True)
repo = pathlib.Path(sys.argv[1]).resolve().parents[2]
evo = os.environ.get("EVO") or shutil.which("evo")
if not evo:
    sys.exit("Put evo on PATH or set EVO to its executable path.")
fixture = repo / "tools/demo/loop/buggy.c"
original = fixture.read_text()
work = pathlib.Path(tempfile.mkdtemp(prefix="evocc-agent-loop-")).resolve()
source = work / "fixed.c"
source.write_text(original)

def command(*args):
    argv = [evo, *map(str, args), "--json"]
    print("$ " + shlex.join(argv), flush=True)
    run = subprocess.run(argv, capture_output=True, text=True)
    print(run.stdout, end="", flush=True)
    print(run.stderr, end="", file=sys.stderr, flush=True)
    records = [json.loads(line) for line in run.stdout.splitlines()]
    if len(records) != 1 or records[0].get("exit_code") != run.returncode:
        sys.exit("Expected one command record with the actual exit code.")
    return run.returncode, records[0]

for attempt in range(5):
    status, checked = command("check", source)
    if status == 0:
        print("Check: completed with exit 0.", flush=True)
        break
    diagnostics = checked.get("diagnostics", [])
    print("Evidence: " + ", ".join(d["code"] for d in diagnostics), flush=True)
    status, plan = command("fix", source, "--plan")
    if status:
        sys.exit(status)
    if any(r["kind"] == "safe" for r in plan["repairs"]):
        status, applied = command("fix", source, "--apply")
        if status:
            sys.exit(status)
        print("Safe repairs applied: " + str(applied["applied"]), flush=True)
        continue  # Repair edits can move the diagnostic span. Obtain a fresh one.
    divisions = [d for d in diagnostics if d["code"] == "E3002"]
    if len(divisions) != 1:
        sys.exit("The fixed rule requires exactly one E3002 diagnostic.")
    span, = [s for s in divisions[0]["spans"] if s["primary"]]
    lines = source.read_text().splitlines(keepends=True)
    if pathlib.Path(span["file"]).resolve() != source or span["line_start"] != span["line_end"]:
        sys.exit("The fixed rule requires a single-line statement in this source.")
    index = span["line_start"] - 1
    line = lines[index]
    expression = line[span["column_start"]-1:span["column_end"]-1]
    match = re.fullmatch(r"\s*\d+\s*/\s*([A-Za-z_]\w*)\s*", expression)
    statement = re.fullmatch(r"(\s*)return\s+" + re.escape(expression) + r";\s*", line)
    if not match or not statement:
        sys.exit("The fixed rule supports only return INTEGER / IDENTIFIER;")
    guard = statement[1] + "if (" + match[1] + " == 0) return 0;\n"
    lines.insert(index, guard)
    source.write_text("".join(lines))
    print("Guard inserted: " + guard.strip(), flush=True)
else:
    sys.exit("No successful check within five rounds.")
print("".join(difflib.unified_diff(original.splitlines(True), source.read_text().splitlines(True),
                                  fromfile=str(fixture), tofile=str(source))), end="", flush=True)
status, proof = command("prove", source, "-o", work / "output.s")
if status:
    sys.exit(status)
print(proof["result"] + " record=" + proof["record_path"], flush=True)
PY
