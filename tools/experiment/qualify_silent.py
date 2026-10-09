#!/usr/bin/env python3
"""Qualify the silent-UB corpus and score repairs on three sealed builds."""
import argparse, datetime, difflib, hashlib, json, os, re, shutil, subprocess, sys, tempfile, uuid
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import repair

BUILDS = (("gcc-14", "-O0"), ("gcc-14", "-O2"), ("clang-19", "-O2"))
GUEST = r'''
import json, os, pathlib, resource, shutil, subprocess, sys, tempfile
resource.setrlimit(resource.RLIMIT_FSIZE, (67108864, 67108864))  # Sanitizer runtimes exceed 1 MiB.
resource.setrlimit(resource.RLIMIT_CPU, (90, 90))
os.chdir(tempfile.mkdtemp(prefix="silent-qualification-"))
p = json.load(sys.stdin)
def run(argv, env=None):
    try:
        r = subprocess.run(argv, capture_output=True, timeout=15, env=env)
        return dict(argv=argv, exit_code=r.returncode, stdout=r.stdout[:16384].decode("utf-8", "replace"),
                    stderr=r.stderr[:32768].decode("utf-8", "replace"), truncated=len(r.stdout)>16384 or len(r.stderr)>32768)
    except subprocess.TimeoutExpired:
        return dict(argv=argv, exit_code=124, stdout="", stderr="timeout", timeout=True)
rows = {}
for name in p["sources"]:
    shutil.copyfile("/in/" + name, "input.c")
    builds = {}
    for compiler, opt in p["builds"]:
        row = dict(compile=run([compiler, opt, "-Dmain=program_main", "-c", "input.c", "-o", "input.o"]), runs=[])
        if row["compile"]["exit_code"] == 0:
            pathlib.Path("reference_main.c").write_text(p["reference"])
            row["link"] = run([compiler, opt, "input.o", "reference_main.c", "-o", "score"])
            pathlib.Path("reference_main.c").unlink()
            if row["link"]["exit_code"] == 0:
                row["runs"] = [run(["./score"]) for _ in range(p["repeats"])]
        builds[compiler + opt] = row
    rows[name] = builds
matrix = {}
if p["matrix"]:
    shutil.copyfile("/in/program.c", "input.c")
    cases = [("gcc-warnings-O0", "gcc-14", ["-O0", "-Wall", "-Wextra"]),
             ("gcc-warnings-O2", "gcc-14", ["-O2", "-Wall", "-Wextra"]),
             ("clang-warnings", "clang-19", ["-O0", "-Weverything"]),
             ("gcc-ubsan", "gcc-14", ["-O0", "-g", "-fsanitize=undefined"]),
             ("clang-ubsan", "clang-19", ["-O0", "-g", "-fsanitize=undefined"]),
             ("clang-msan", "clang-19", ["-O0", "-g", "-fsanitize=memory"]),
             ("clang-asan", "clang-19", ["-O0", "-g", "-fsanitize=address", "-fsanitize-address-use-after-scope"]),
             ("gcc-asan", "gcc-14", ["-O0", "-g", "-fsanitize=address"])]
    for label, compiler, flags in cases:
        row = dict(compile=run([compiler, *flags, "input.c", "-o", "detect"]))
        env = dict(os.environ, ASAN_OPTIONS="detect_stack_use_after_return=1") if label == "clang-asan" else None
        if row["compile"]["exit_code"] == 0: row["run"] = run(["./detect"], env)
        matrix[label] = row
print(json.dumps(dict(scores=rows, matrix=matrix)))
'''


def changed_lines(before, after):
    return [[i + 1, max(i + 1, j)] for tag, i, j, _, _ in
            difflib.SequenceMatcher(a=before.splitlines(), b=after.splitlines(), autojunk=False).get_opcodes() if tag != "equal"]


def finding(text, spans):
    sanitizer = "ERROR: AddressSanitizer" in text or "WARNING: MemorySanitizer" in text
    for line in text.splitlines():
        for match in re.finditer(r"(?:input|program)\.c:(\d+)(?::\d+)?", line):
            if any(lo <= int(match[1]) <= hi for lo, hi in spans) and (sanitizer or re.search(r"warning:|error:|runtime error:", line)):
                return True
    return False


def main_result(text):
    match = re.search(r"=== run exit (-?\d+)\n--- stdout\n(.*?)--- stderr\n", text, re.S)
    return (int(match[1]), match[2]) if match else None


def passes(row, expected, repeats):
    records = [row.get("compile", {}), row.get("link", {})] + row.get("runs", [])
    return (len(row.get("runs", [])) == repeats and all(r.get("exit_code") == 0 and not r.get("truncated") for r in records)
            and all(r["stdout"] == expected for r in row["runs"]) and len({r["stderr"] for r in row["runs"]}) == 1)


def guest(sources, reference, repeats=1, matrix=False):
    if shutil.disk_usage(repair.ROOT).free < 25 * 1024**3: raise repair.Failure("disk_below_25_GiB")
    name = "silent-qualification-" + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="silent-qualification-") as tmp:
        stage = Path(tmp); stage.chmod(0o755)
        for key, source in sources.items(): (stage / key).write_text(source)
        payload = dict(sources=list(sources), reference=reference, repeats=repeats, builds=BUILDS, matrix=matrix)
        try:
            r = repair.run([repair.CONTAINER, "run", "--name", name, "--platform", "linux/arm64", "--network", "none",
                            "--cpus", "1", "--memory", "512M", "--user", "65534:65534", "-i", "-v", str(stage) + ":/in:ro",
                            repair.IMAGE, "python3", "-c", GUEST], data=json.dumps(payload).encode())
        finally:
            cleaned = subprocess.run([repair.CONTAINER, "delete", "--force", name], env=repair.ENV,
                                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30)
        if cleaned.returncode: raise repair.Failure("container_cleanup_failed")
        if r["exit_code"]: raise repair.Failure("qualification_guest_failed", r)
        return json.loads(r["stdout"])


def score_builds(source, item):
    rows = guest({"program.c": source}, (item / "reference_main.c").read_text())["scores"]["program.c"]
    expected = (item / "expected.txt").read_text()
    return {key: dict(passed=passes(row, expected, 1), **row) for key, row in rows.items()}


def qualify(item):
    import agent_repair as agent
    sources = {name: (item / name).read_text() for name in ("program.c", "fixed.c")}
    spans = json.loads((item / "fix-spans.json").read_text())["spans"]
    code = (item / "class").read_text().strip()
    source = sources["program.c"]
    checks = dict(profile=len(source.encode()) <= 65536 and 80 <= len(source.splitlines()) <= 200
                  and not any(t in source for t in ("#", "%:", "??=")), spans=spans == changed_lines(source, sources["fixed.c"]))
    result = guest(sources, (item / "reference_main.c").read_text(), 5, True)
    expected = (item / "expected.txt").read_text()
    checks["fixed"] = all(passes(row, expected, 5) for row in result["scores"]["fixed.c"].values())
    checks["original_fails"] = any(len(row["runs"]) == 5 and all(r["exit_code"] != 0 or r["stdout"] != expected for r in row["runs"])
                                    for row in result["scores"]["program.c"].values())
    feedback = {}
    repair.ENV = dict(repair.ENV, EVOCC_SYSROOT=str(agent.SYSROOT))
    for version in sources:
        feedback[version] = {}
        for condition in ("c1", "c3"):
            if shutil.disk_usage(repair.ROOT).free < 25 * 1024**3: raise repair.Failure("disk_below_25_GiB")
            with tempfile.TemporaryDirectory(prefix="silent-feedback-") as tmp:
                stage = Path(tmp); stage.chmod(0o755)
                feedback[version][condition] = agent.feedback(condition, item / version, stage)
    c1, c3 = feedback["program.c"]["c1"], feedback["program.c"]["c3"]
    silent = code in ("E3001", "E3010", "E3011")
    checks["c1_finding"] = finding(c1, spans) == (not silent)
    checks["silent_main"] = not silent or (main_result(c1) is not None and main_result(c1) == main_result(feedback["fixed.c"]["c1"]))
    diagnostics = [json.loads(line) for line in c3.splitlines() if line.startswith('{"exit_code":')]
    ds = diagnostics[0].get("diagnostics", []) if diagnostics else []
    checks["evo_finding"] = any(d["code"] == code and any(s.get("primary") and any(lo <= s["line_start"] <= hi for lo, hi in spans)
                                for s in d.get("spans", [])) for d in ds)
    checks["evo_completes"] = bool(diagnostics) and not any(d["code"] == "E3008" for d in ds)
    fixed_ds = [json.loads(line) for line in feedback["fixed.c"]["c3"].splitlines() if line.startswith('{"exit_code":')]
    checks["fixed_evo"] = bool(fixed_ds) and fixed_ds[0]["exit_code"] == 0
    for label, row in result["matrix"].items():
        text = "\n".join(v.get("stderr", "") for v in row.values())
        row["reported"] = finding(text, spans)
        row["available"] = row["compile"]["exit_code"] == 0 and not any(v.get("truncated") or v.get("timeout") for v in row.values() if isinstance(v, dict))
    result["matrix"]["evo-check"] = dict(reported=checks["evo_finding"], available=checks["evo_completes"])
    return dict(item=item.name, code=code, checks=checks, qualified=all(checks.values()), feedback=feedback,
                hashes={p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(item.iterdir()) if p.is_file()}, **result)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--items", type=Path, default=repair.ROOT / "tests/fixtures/agent-repair-4")
    parser.add_argument("--item", action="append")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    selected = sorted(p for p in args.items.iterdir() if p.is_dir() and (not args.item or p.name in args.item))
    receipt = dict(schema="agent-repair.qualification.v4", date=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                   image=repair.IMAGE, builds=BUILDS, repeats=5, items=[])
    if args.item and args.out.exists(): receipt = json.loads(args.out.read_text())
    for old in list(receipt["items"]):
        if not (args.items / old["item"]).is_dir():
            receipt.setdefault("replaced", []).append(old); receipt["items"].remove(old)
    for item in selected:
        row = qualify(item)
        for old in list(receipt["items"]):
            if old["item"] == item.name:
                receipt.setdefault("replaced", []).append(old); receipt["items"].remove(old)
        receipt["items"].append(row); receipt["items"].sort(key=lambda r: r["item"])
        args.out.write_text(json.dumps(receipt, indent=2) + "\n")
        print(json.dumps(dict(item=item.name, qualified=row["qualified"], failed=[k for k, v in row["checks"].items() if not v])), flush=True)
    return 0 if selected and all(r["qualified"] for r in receipt["items"]) else 1


if __name__ == "__main__":
    raise SystemExit(main())
