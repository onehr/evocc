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
    overflows = [d for d in diagnostics if d["code"] == "E3004"]
    if len(overflows) != 1:
        sys.exit("The fixed rule requires exactly one E3004 diagnostic.")
    span, = [s for s in overflows[0]["spans"] if s["primary"]]
    text = source.read_text()
    lines = text.splitlines(keepends=True)
    if pathlib.Path(span["file"]).resolve() != source or span["line_start"] != span["line_end"]:
        sys.exit("The fixed rule requires a single-line statement in this source.")
    index = span["line_start"] - 1
    line = lines[index]
    expression = line[span["column_start"]-1:span["column_end"]-1]
    offset = sum(map(len, lines[:index])) + span["column_start"] - 1
    functions = re.finditer(r"^int \w+\(int (\w+)\)\n\{\n([^{}]*)^\}", text, re.M)
    enclosing = [f for f in functions if f.start(2) <= offset < f.end(2)]
    if not text.isascii() or len(enclosing) != 1 or expression != "-" + enclosing[0][1]:
        sys.exit("The fixed rule requires unary minus of an int parameter in a flat function.")
    function = enclosing[0]
    guard = "    if (" + function[1] + " == -2147483647 - 1) return 2147483647;\n"
    source.write_text(text[:function.start(2)] + guard + text[function.start(2):])
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
