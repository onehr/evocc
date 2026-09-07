#!/usr/bin/env python3
"""Rung E0c of the evaluation ladder: Csmith differential testing.

Csmith is pinned in docs/roadmap.md at git master 0cdc710315cf. Every program
is generated from a fixed seed so a campaign reproduces exactly. EvoCC emits
AArch64 assembly on the host; inside the sealed container GCC-14 assembles and
links that assembly, GCC-14 and Clang-19 compile the same source at -O0 and
-O2, and all five binaries run with a ten second timeout and print Csmith's
checksum.

A program counts as agreeing when EvoCC's checksum equals the checksum every
oracle prints. When the two oracles disagree with each other the program is
recorded as an oracle split, not as a pass or proof of undefined behavior. Every
other outcome is recorded: a refusal by diagnostic code, an assembler or linker
failure, a timeout, a run that trapped, or a checksum mismatch, which is the
miscompile report.

Agreeing oracles are not a proof that a program is defined. The pinned runtime
has a measured undefined signed shift in its 32/64-bit subtraction guards.
The shared copy replaces only those two masks with ~INT_MAX, as the upstream
function-based runtime already does. All compilers receive identical repaired
headers; their before/after digests are recorded with the campaign. Old runs
remain unchanged. A trap elsewhere still needs its own diagnosis.

Csmith's runtime header needs a hosted libc that EvoCC does not yet parse
(glibc's <math.h> declares vector types EvoCC has no model for), so the
campaign uses Csmith's own minimal runtime with the ISO C safe-math macros,
which `--math-notmp` makes Csmith call. The minimal runtime leaves INT_MAX
undefined, so the campaign defines it on the command line of all three
compilers alike.

Usage:
  python3 tools/ladder/csmith.py [--campaign base|bitfields|all] [--programs N]
Writes .evidence/ladder/csmith/<run>/{results.jsonl,summary.json,mismatch/}
and updates tests/contracts/ladder-csmith-receipt.json.
"""

from __future__ import annotations

import argparse
import collections
import concurrent.futures
import datetime as dt
import importlib.util
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

_spec = importlib.util.spec_from_file_location("ladder_e0", Path(__file__).parent / "e0.py")
e0 = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(e0)

BUILD = ROOT / ".build" / "ladder"
EVIDENCE = ROOT / ".evidence" / "ladder" / "csmith"
RECEIPT = ROOT / "tests" / "contracts" / "ladder-csmith-receipt.json"
REDUCTIONS = ROOT / "tests" / "fixtures" / "csmith-reductions"
IMAGE = "evo-flight-lab:sealed"
TARGET = "aarch64-unknown-linux-gnu"

PIN = {
    "repo": "https://github.com/csmith-project/csmith.git",
    "commit": "0cdc710315cfee9035e22ef4363ca479270d1934",
    "shortCommit": "0cdc710315cf",
}

# Options every campaign shares. `--no-float` is the rung's headline
# restriction; the rest keep the program inside the profile EvoCC advertises
# today, and `--math-notmp` selects the safe-math macros that are plain ISO C
# comma expressions instead of GNU statement expressions.
PROFILE_OPTIONS = [
    "--no-float",
    "--no-packed-struct",
    "--no-bitfields",
    "--no-volatiles",
    "--no-volatile-pointers",
    "--max-funcs",
    "10",
    "--math-notmp",
    "--max-expr-complexity",
    "3",
    "--max-block-size",
    "3",
    "--max-array-dim",
    "2",
    "--max-pointer-depth",
    "1",
]

# The second campaign turns bitfields back on and changes nothing else.
CAMPAIGNS = {
    "base": PROFILE_OPTIONS,
    "bitfields": [o for o in PROFILE_OPTIONS if o != "--no-bitfields"] + ["--bitfields"],
}

RUNTIME_DEFINES = [
    "-DCSMITH_MINIMAL",
    "-DUSE_MATH_MACROS_NOTMP",
    "-DINT_MAX=2147483647",
]

ORACLES = [("gcc", "O0"), ("gcc", "O2"), ("clang-19", "O0"), ("clang-19", "O2")]

CHECKSUM = re.compile(r"checksum\s*=\s*([0-9a-fA-F]+)")


def csmith_options(campaign: str) -> list[str]:
    """The generator options for a campaign, in a fixed order."""
    if campaign not in CAMPAIGNS:
        raise KeyError(campaign)
    return list(CAMPAIGNS[campaign])


def csmith_command(binary: Path, campaign: str, seed: int, output: Path) -> list[str]:
    """The exact generator invocation for one seed, so a program reproduces."""
    return [
        str(binary),
        *csmith_options(campaign),
        "--seed",
        str(seed),
        "--output",
        str(output),
    ]


def parse_checksum(text: str) -> str | None:
    """Csmith's minimal runtime prints `checksum = <hex>` once, at the end.

    The value is normalised to lower case without leading zeros so that two
    compilers whose printf pads differently still compare equal.
    """
    found = CHECKSUM.findall(text or "")
    if not found:
        return None
    return found[-1].lower().lstrip("0") or "0"


def diagnostic_code(stderr: str) -> tuple[str, str]:
    """The first diagnostic EvoCC reported, as (code, message)."""
    m = re.search(r"error\[(E\d+)\]: ([^\n]*)", stderr)
    if m:
        return m.group(1), m.group(2)[:160]
    if "panicked" in stderr:
        return "panic", stderr.strip()[-160:]
    return "other", stderr.strip()[-160:]


def build_csmith() -> Path:
    """Clone the pin and build Csmith on the host; return the binary."""
    src = BUILD / "csmith"
    binary = src / "src" / "csmith"
    if binary.exists():
        return binary
    src.parent.mkdir(parents=True, exist_ok=True)
    if not (src / ".git").is_dir():
        e0.sh(["git", "clone", "-q", PIN["repo"], str(src)], check=True)
    e0.sh(["git", "-C", str(src), "checkout", "-q", PIN["commit"]], check=True)
    e0.sh(["cmake", "-DCMAKE_BUILD_TYPE=Release", "."], cwd=src, check=True)
    e0.sh(["make", "-j4"], cwd=src, check=True)
    if not binary.exists():
        raise SystemExit("csmith did not build")
    return binary


def generate(binary: Path, campaign: str, seeds: range, work: Path) -> list[int]:
    """Write one program per seed; a seed the generator rejects is dropped."""
    (work / "src").mkdir(parents=True, exist_ok=True)
    kept = []
    for seed in seeds:
        out = work / "src" / f"s{seed}.c"
        p = e0.sh(csmith_command(binary, campaign, seed, out))
        if p.returncode == 0 and out.exists() and out.stat().st_size:
            kept.append(seed)
    return kept


def emit(evocc: Path, work: Path, seeds: list[int], inc: list[str]) -> dict[int, dict]:
    """EvoCC emits AArch64 assembly on the host, four programs at a time."""
    rows = emit_by_stem(evocc, work, [f"s{seed}" for seed in seeds], inc)
    return {seed: rows[f"s{seed}"] for seed in seeds}


def emit_by_stem(evocc: Path, work: Path, names: list[str], inc: list[str]) -> dict[str, dict]:
    """The same emission, addressed by file stem, which reduction also uses."""
    (work / "asm").mkdir(parents=True, exist_ok=True)
    flags = [f"-I{work / 'runtime'}", *inc]

    def one(name: str) -> tuple[str, dict]:
        src = work / "src" / f"{name}.c"
        try:
            p = e0.sh(
                [str(evocc), f"--target={TARGET}", *RUNTIME_DEFINES, *flags, "--emit=asm", str(src)],
                timeout=120,
            )
        except subprocess.TimeoutExpired:
            return name, {"emit": "timeout"}
        if p.returncode == 0 and p.stdout.strip():
            (work / "asm" / f"{name}.s").write_text(p.stdout)
            return name, {"emit": "emitted"}
        code, message = diagnostic_code(p.stderr)
        return name, {"emit": "refused", "code": code, "message": message}

    rows: dict[str, dict] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for name, row in pool.map(one, names):
            rows[name] = row
    return rows


RUNNER = r"""#!/bin/sh
# One program: assemble EvoCC's output, build both oracles at both levels,
# run everything, and print `<name> <config> <status> [value]` per line, where
# the value is the checksum of a run that printed one and the exit status of a
# run that did not.
b="$1"
cd /work/run
D="-DCSMITH_MINIMAL -DUSE_MATH_MACROS_NOTMP -DINT_MAX=2147483647 -I/work/run/runtime"
report() { # report <config> <exit status> <output file>
  if [ "$2" -eq 124 ]; then echo "$b $1 timeout $2"; return; fi
  if [ "$2" -ne 0 ]; then echo "$b $1 run-fail $2"; return; fi
  s=$(grep -o 'checksum = [0-9a-fA-F]*' "$3" | tail -1 | awk '{print $3}')
  if [ -n "$s" ]; then echo "$b $1 ran $s"; else echo "$b $1 no-checksum $2"; fi
}
if gcc -c asm/$b.s -o bin/$b.evocc.o 2>out/$b.evocc.err; then
  if gcc -static -o bin/$b.evocc bin/$b.evocc.o 2>>out/$b.evocc.err; then
    timeout 10 ./bin/$b.evocc > out/$b.evocc.txt 2>/dev/null
    report evocc $? out/$b.evocc.txt
  else echo "$b evocc link-fail"; fi
else echo "$b evocc asm-fail"; fi
for cc in gcc clang-19; do
  for o in O0 O2; do
    if $cc -$o -static -w $D -o bin/$b.$cc.$o src/$b.c 2>out/$b.$cc.$o.err; then
      timeout 10 ./bin/$b.$cc.$o > out/$b.$cc.$o.txt 2>/dev/null
      report $cc.$o $? out/$b.$cc.$o.txt
    else echo "$b $cc.$o compile-fail"; fi
  done
done
"""

DRIVER = r"""#!/bin/sh
cd /work/run
mkdir -p bin out
xargs -a names.txt -n 1 -P 4 sh /work/run/one.sh
"""


def run_in_container(work: Path, names: list[str], timeout: int = 7200) -> dict[str, dict]:
    """One container invocation assembles, compiles and runs the whole batch."""
    (work / "one.sh").write_text(RUNNER)
    (work / "drive.sh").write_text(DRIVER)
    (work / "names.txt").write_text("".join(n + "\n" for n in names))
    p = e0.sh(
        [
            "container", "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-v", f"{work}:/work/run", IMAGE, "sh", "/work/run/drive.sh",
        ],
        timeout=timeout,
    )
    out: dict[str, dict] = collections.defaultdict(dict)
    for line in p.stdout.splitlines():
        parts = line.split()
        if len(parts) < 3 or parts[0].startswith("["):
            continue
        name, config, status = parts[0], parts[1], parts[2]
        row = {"status": status}
        if status == "ran":
            row["checksum"] = parse_checksum("checksum = " + parts[3]) if len(parts) > 3 else None
            if row["checksum"] is None:
                row["status"] = "no-checksum"
        elif status in ("no-checksum", "run-fail", "timeout") and len(parts) > 3:
            row["exit"] = int(parts[3])
        out[name][config] = row
    return dict(out)


def verdict(row: dict) -> tuple[str, dict]:
    """Classify one program from its five runs."""
    oracle_sums = {}
    for cc, opt in ORACLES:
        r = row.get(f"{cc}.{opt}")
        if r is None or r["status"] != "ran":
            return "oracle-unusable", {}
        oracle_sums[f"{cc}.{opt}"] = r["checksum"]
    if len(set(oracle_sums.values())) != 1:
        return "oracle-split", {"oracleChecksums": oracle_sums}
    reference = next(iter(oracle_sums.values()))
    mine = row.get("evocc")
    if mine is None:
        return "not-run", {"reference": reference}
    if mine["status"] != "ran":
        return mine["status"], {"reference": reference}
    if mine["checksum"] == reference:
        return "agree", {"reference": reference}
    return "mismatch", {"reference": reference, "evocc": mine["checksum"]}


# --- reduction -------------------------------------------------------------
#
# C-Reduce is not installed on this host, so a mismatch is reduced by deleting
# lines. One pass over a granularity deletes every chunk of that size in turn;
# all the candidates of a pass are emitted on the host and then compiled and
# run inside a single container invocation, so a pass costs one container run
# rather than one per candidate. A candidate is kept when EvoCC still emits
# assembly for it, both oracles still agree with each other at both levels,
# and EvoCC's checksum still differs from theirs.


def interesting(runs: dict, target: str) -> bool:
    """The reduced program must still show the same disagreement."""
    return verdict(runs)[0] == target


def reduction_pass(
    evocc: Path, work: Path, lines: list[str], chunks: int, inc: list[str], target: str
) -> list[str] | None:
    """Delete one chunk in turn, `chunks` chunks over the whole program, and
    return the shortest deletion that keeps the disagreement, or None when no
    deletion at this granularity keeps it. One pass costs one container run."""
    for name in ("src", "asm"):
        shutil.rmtree(work / name, ignore_errors=True)
        (work / name).mkdir(parents=True)
    size = max(1, -(-len(lines) // chunks))
    candidates = {}
    for index in range(0, len(lines), size):
        kept = lines[:index] + lines[index + size :]
        if not kept:
            continue
        name = f"c{index}"
        (work / "src" / f"{name}.c").write_text("".join(kept))
        candidates[name] = kept
    if not candidates:
        return None
    rows = emit_by_stem(evocc, work, sorted(candidates), inc)
    emitted = [n for n in sorted(candidates) if rows[n]["emit"] == "emitted"]
    if not emitted:
        return None
    runs = run_in_container(work, emitted, timeout=3600)
    keepers = [n for n in emitted if interesting(runs.get(n, {}), target)]
    if not keepers:
        return None
    return min((candidates[n] for n in keepers), key=len)


def reduce_program(evocc: Path, source: Path, inc: list[str], budget: float) -> tuple[Path, dict]:
    """Shrink one disagreeing program within `budget` seconds of wall time.

    C-Reduce is not on this host, so the reduction deletes lines: a pass over a
    granularity tries every single-chunk deletion, and the granularity doubles
    until a pass finds nothing or the budget runs out. Candidates that stop
    compiling, that stop being emitted, or whose oracles stop agreeing are not
    interesting, so the program stays well defined all the way down.
    """
    work = BUILD / "run-csmith-reduce"
    shutil.rmtree(work, ignore_errors=True)
    runtime = prepare_runtime(work)
    lines = source.read_text().splitlines(keepends=True)
    started = dt.datetime.now(dt.timezone.utc)

    def spent() -> float:
        return (dt.datetime.now(dt.timezone.utc) - started).total_seconds()

    def measure(body: list[str]) -> tuple[str, dict]:
        for name in ("src", "asm"):
            shutil.rmtree(work / name, ignore_errors=True)
            (work / name).mkdir(parents=True)
        (work / "src" / "probe.c").write_text("".join(body))
        rows = emit_by_stem(evocc, work, ["probe"], inc)
        names = ["probe"] if rows["probe"]["emit"] == "emitted" else []
        return verdict(run_in_container(work, names, timeout=900).get("probe", {}))

    target, detail = measure(lines)
    if target not in DISAGREEMENT:
        return source, {"source": str(source), "outcome": target, **detail}
    chunks = 2
    while chunks <= 128 and spent() < budget:
        shorter = reduction_pass(evocc, work, lines, chunks, inc, target)
        if shorter is not None and len(shorter) < len(lines):
            lines = shorter
            chunks = 2
            continue
        chunks *= 2
    reduced = source.with_name(source.stem + "-reduced.c")
    reduced.write_text("".join(lines))
    outcome, detail = measure(lines)
    return reduced, {
        "source": str(source),
        "runtime": runtime,
        "reduced": str(reduced),
        "lines": len(lines),
        "bytes": reduced.stat().st_size,
        "seconds": round(spent(), 1),
        "outcome": outcome,
        **detail,
    }

# A miscompile report is a program whose binaries all ran and whose checksums
# disagree. A trap is tracked separately: the check itself or its lowering
# can be wrong, so the signal alone establishes neither UB nor correctness.
MISCOMPILE = ("mismatch",)
TRAP_EXIT = 133  # 128 + SIGTRAP
# What reduction preserves: EvoCC's binary did not produce the checksum both
# oracles agreed on, whether it printed a different one or none at all.
DISAGREEMENT = ("mismatch", "no-checksum", "run-fail")


def summarise(name: str, rows: list[dict]) -> dict:
    """The campaign's reading, derived only from the recorded per-program rows."""
    counts = collections.Counter(row["outcome"] for row in rows)
    refusals = collections.Counter(
        row["code"] for row in rows if row["outcome"] == "refused"
    )
    messages = collections.Counter(
        f"{row['code']} {row['message']}" for row in rows if row["outcome"] == "refused"
    )
    miscompiles = [
        {
            "program": row["program"],
            "reference": row.get("reference"),
            "evocc": row.get("evocc"),
        }
        for row in rows
        if row["outcome"] in MISCOMPILE
    ]
    trapped = [
        row["program"]
        for row in rows
        if row["outcome"] in ("no-checksum", "run-fail")
        and row.get("runs", {}).get("evocc", {}).get("exit") == TRAP_EXIT
    ]
    return {
        "options": csmith_options(name),
        "defines": RUNTIME_DEFINES,
        "programs": len(rows),
        "agreeing": counts["agree"],
        "outcomes": dict(counts.most_common()),
        "refusalsByCode": dict(refusals.most_common()),
        "refusalsByMessage": dict(messages.most_common()),
        "miscompiles": miscompiles,
        "trapExit133Programs": trapped,
    }


def repair_runtime_header(source: str) -> str:
    """Replace the two measured UB masks, leaving the guard algorithm intact."""
    for width in (32, 64):
        old = f"(((int{width}_t)1) << (sizeof(int{width}_t)*CHAR_BIT-1))"
        if source.count(old) != 1:
            raise ValueError(f"unexpected Csmith int{width}_t subtraction guard")
        source = source.replace(old, f"(~INT{width}_MAX)")
    return source


def prepare_runtime(work: Path) -> dict:
    destination = work / "runtime"
    destination.mkdir(parents=True)
    for header in sorted((BUILD / "csmith" / "runtime").glob("*.h")):
        shutil.copy(header, destination / header.name)
    header = destination / "safe_math_macros_notmp.h"
    before = e0.sha256(header)
    header.write_text(repair_runtime_header(header.read_text()))
    return {
        "change": "subtraction-sign-masks",
        "header": header.name,
        "originalSha256": before,
        "repairedSha256": e0.sha256(header),
        "sharedBy": ["evocc", "gcc", "clang-19"],
    }


def campaign(evocc: Path, binary: Path, name: str, count: int, inc: list[str], out_dir: Path, results) -> dict:
    work = BUILD / f"run-csmith-{name}"
    if work.exists():
        shutil.rmtree(work)
    runtime = prepare_runtime(work)
    seeds = generate(binary, name, range(1, count + 1), work)
    emitted_rows = emit(evocc, work, seeds, inc)
    emitted = [f"s{seed}" for seed in seeds if emitted_rows[seed]["emit"] == "emitted"]
    runs = run_in_container(work, emitted) if emitted else {}
    rows = []
    for seed in seeds:
        program = f"s{seed}"
        row = {"campaign": name, "seed": seed, "program": program, **emitted_rows[seed]}
        if emitted_rows[seed]["emit"] != "emitted":
            row["outcome"] = emitted_rows[seed]["emit"]
        else:
            outcome, extra = verdict(runs.get(program, {}))
            row.update(extra)
            row["runs"] = runs.get(program, {})
            row["outcome"] = outcome
        if row["outcome"] in MISCOMPILE:
            shutil.copy(work / "src" / f"{program}.c", out_dir / "mismatch" / f"{name}-{program}.c")
        rows.append(row)
        results.write(json.dumps(row) + "\n")
    return {**summarise(name, rows), "work": str(work.relative_to(ROOT)), "runtime": runtime}


def seal(summary: dict, out_dir: Path) -> dict:
    """Roll the campaigns up, write summary.json and refresh the receipt."""
    campaigns = summary["campaigns"].values()
    refused = collections.Counter()
    for one in campaigns:
        refused.update(one["refusalsByCode"])
    summary["programs"] = sum(one["programs"] for one in campaigns)
    summary["agreeing"] = sum(one["agreeing"] for one in campaigns)
    summary["refusalsByCode"] = dict(refused.most_common())
    summary["miscompiles"] = sum(len(one["miscompiles"]) for one in campaigns)
    summary["trapExit133Programs"] = sum(
        len(one["trapExit133Programs"]) for one in campaigns
    )
    summary["trapClassification"] = "Exit 133 alone does not establish undefined behavior; each cause needs a reduced diagnostic or sanitizer reproduction."
    summary["reductions"] = sorted(
        str(path.relative_to(ROOT)) for path in REDUCTIONS.glob("*.c")
    )
    json.dump(summary, open(out_dir / "summary.json", "w"), indent=1)
    receipt = {
        "schemaVersion": 1,
        "rung": "E0c",
        "authority": "docs/roadmap.md, milestone ladder",
        "latest": summary,
        "evidence": str(out_dir.relative_to(ROOT)),
    }
    json.dump(receipt, open(RECEIPT, "w"), indent=1)
    return summary


def reseal(out_dir: Path) -> dict:
    """Recompute a finished run's reading from its recorded rows.

    Classification is a pure function of what each program did, so a run is
    re-sealed without re-measuring when the classification changes.
    """
    out_dir = out_dir.resolve()
    summary = json.load(open(out_dir / "summary.json"))
    rows = [json.loads(line) for line in open(out_dir / "results.jsonl")]
    grouped = collections.defaultdict(list)
    for row in rows:
        grouped[row["campaign"]].append(row)
    for name, group in grouped.items():
        work = summary["campaigns"].get(name, {}).get("work")
        runtime = summary["campaigns"].get(name, {}).get("runtime")
        summary["campaigns"][name] = summarise(name, group)
        if work:
            summary["campaigns"][name]["work"] = work
        if runtime:
            summary["campaigns"][name]["runtime"] = runtime
    return seal(summary, out_dir)


def report_line(summary: dict) -> str:
    return (
        f"csmith: {summary['agreeing']} of {summary['programs']} agree; "
        f"refused {summary['refusalsByCode']}; mismatches {summary['miscompiles']}"
    )


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--campaign", default="all", choices=["all", *CAMPAIGNS])
    ap.add_argument("--programs", type=int, default=300)
    ap.add_argument("--evocc", default=str(ROOT / "target" / "release" / "evocc"))
    ap.add_argument("--reduce", help="shrink one disagreeing program instead of running a campaign")
    ap.add_argument("--reduce-seconds", type=float, default=1800.0)
    ap.add_argument("--reseal", help="re-derive an existing run's summary and receipt from its rows")
    args = ap.parse_args()
    evocc = Path(args.evocc)
    if args.reseal:
        print(report_line(reseal(Path(args.reseal))))
        return 0
    if not evocc.exists():
        print("build the release driver first: cargo build --release -p evocc", file=sys.stderr)
        return 1
    if args.reduce:
        inc = e0.include_flags(e0.sysroot())
        reduced, report = reduce_program(evocc, Path(args.reduce), inc, args.reduce_seconds)
        print(json.dumps(report, indent=1))
        return 0 if report["outcome"] in DISAGREEMENT else 1
    binary = build_csmith()
    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_dir = EVIDENCE / run_id
    (out_dir / "mismatch").mkdir(parents=True, exist_ok=True)
    inc = e0.include_flags(e0.sysroot())
    head = e0.sh(["git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"]).stdout.strip()
    summary = {
        "run": run_id,
        "subject": head,
        "driverSha256": e0.sha256(evocc),
        "csmith": {**PIN, "sha256": e0.sha256(binary), "builtOn": "host"},
        "target": TARGET,
        "campaigns": {},
    }
    names = list(CAMPAIGNS) if args.campaign == "all" else [args.campaign]
    with open(out_dir / "results.jsonl", "w") as results:
        for name in names:
            summary["campaigns"][name] = campaign(
                evocc, binary, name, args.programs, inc, out_dir, results
            )
            print(f"{name}: {summary['campaigns'][name]['outcomes']}")
    seal(summary, out_dir)
    print(report_line(summary))
    print("receipt:", RECEIPT.relative_to(ROOT), "evidence:", out_dir.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
