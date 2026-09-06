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
undefined, is recorded as an oracle split and is excluded from the count. Every
other outcome is recorded: a refusal by diagnostic code, an assembler or linker
failure, a timeout, or a checksum mismatch, which is a miscompile report.

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
    (work / "asm").mkdir(parents=True, exist_ok=True)
    inc = [f"-I{work / 'runtime'}", *inc]

    def one(seed: int) -> tuple[int, dict]:
        src = work / "src" / f"s{seed}.c"
        try:
            p = e0.sh(
                [str(evocc), f"--target={TARGET}", *RUNTIME_DEFINES, *inc, "--emit=asm", str(src)],
                timeout=120,
            )
        except subprocess.TimeoutExpired:
            return seed, {"emit": "timeout"}
        if p.returncode == 0 and p.stdout.strip():
            (work / "asm" / f"s{seed}.s").write_text(p.stdout)
            return seed, {"emit": "emitted"}
        code, message = diagnostic_code(p.stderr)
        return seed, {"emit": "refused", "code": code, "message": message}

    rows: dict[int, dict] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for seed, row in pool.map(one, seeds):
            rows[seed] = row
    return rows


RUNNER = r"""#!/bin/sh
# One program: assemble EvoCC's output, build both oracles at both levels,
# run everything, and print `<name> <config> <status> [checksum]` per line.
b="$1"
cd /work/run
D="-DCSMITH_MINIMAL -DUSE_MATH_MACROS_NOTMP -DINT_MAX=2147483647 -I/work/run/runtime"
if gcc -c asm/$b.s -o bin/$b.evocc.o 2>out/$b.evocc.err; then
  if gcc -static -o bin/$b.evocc bin/$b.evocc.o 2>>out/$b.evocc.err; then
    timeout 10 ./bin/$b.evocc > out/$b.evocc.txt 2>/dev/null
    if [ $? -eq 124 ]; then echo "$b evocc timeout"; else
      echo "$b evocc ran $(grep -o 'checksum = [0-9a-fA-F]*' out/$b.evocc.txt | tail -1 | awk '{print $3}')"
    fi
  else echo "$b evocc link-fail"; fi
else echo "$b evocc asm-fail"; fi
for cc in gcc clang-19; do
  for o in O0 O2; do
    if $cc -$o -static -w $D -o bin/$b.$cc.$o src/$b.c 2>out/$b.$cc.$o.err; then
      timeout 10 ./bin/$b.$cc.$o > out/$b.$cc.$o.txt 2>/dev/null
      if [ $? -eq 124 ]; then echo "$b $cc.$o timeout"; else
        echo "$b $cc.$o ran $(grep -o 'checksum = [0-9a-fA-F]*' out/$b.$cc.$o.txt | tail -1 | awk '{print $3}')"
      fi
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


def campaign(evocc: Path, binary: Path, name: str, count: int, inc: list[str], out_dir: Path, results) -> dict:
    work = BUILD / f"run-csmith-{name}"
    if work.exists():
        shutil.rmtree(work)
    (work / "runtime").mkdir(parents=True)
    for header in sorted((BUILD / "csmith" / "runtime").glob("*.h")):
        shutil.copy(header, work / "runtime" / header.name)
    seeds = generate(binary, name, range(1, count + 1), work)
    rows = emit(evocc, work, seeds, inc)
    emitted = [f"s{seed}" for seed in seeds if rows[seed]["emit"] == "emitted"]
    runs = run_in_container(work, emitted) if emitted else {}
    counts = collections.Counter()
    refusals = collections.Counter()
    mismatches = []
    for seed in seeds:
        program = f"s{seed}"
        row = {"campaign": name, "seed": seed, "program": program, **rows[seed]}
        if rows[seed]["emit"] != "emitted":
            outcome = rows[seed]["emit"]
        else:
            outcome, extra = verdict(runs.get(program, {}))
            row.update(extra)
            row["runs"] = runs.get(program, {})
        row["outcome"] = outcome
        counts[outcome] += 1
        if outcome == "refused":
            refusals[row["code"]] += 1
        if outcome == "mismatch":
            mismatches.append(program)
            shutil.copy(work / "src" / f"{program}.c", out_dir / "mismatch" / f"{name}-{program}.c")
        results.write(json.dumps(row) + "\n")
    return {
        "options": csmith_options(name),
        "defines": RUNTIME_DEFINES,
        "programs": len(seeds),
        "agreeing": counts["agree"],
        "outcomes": dict(counts.most_common()),
        "refusalsByCode": dict(refusals.most_common()),
        "mismatches": mismatches,
        "work": str(work.relative_to(ROOT)),
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--campaign", default="all", choices=["all", *CAMPAIGNS])
    ap.add_argument("--programs", type=int, default=300)
    ap.add_argument("--evocc", default=str(ROOT / "target" / "release" / "evocc"))
    args = ap.parse_args()
    evocc = Path(args.evocc)
    if not evocc.exists():
        print("build the release driver first: cargo build --release -p evocc", file=sys.stderr)
        return 1
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
    total = sum(c["programs"] for c in summary["campaigns"].values())
    agreeing = sum(c["agreeing"] for c in summary["campaigns"].values())
    refused = collections.Counter()
    for c in summary["campaigns"].values():
        refused.update(c["refusalsByCode"])
    summary["programs"] = total
    summary["agreeing"] = agreeing
    summary["refusalsByCode"] = dict(refused.most_common())
    summary["mismatches"] = sum(len(c["mismatches"]) for c in summary["campaigns"].values())
    json.dump(summary, open(out_dir / "summary.json", "w"), indent=1)
    receipt = {
        "schemaVersion": 1,
        "rung": "E0c",
        "authority": "docs/roadmap.md, milestone ladder",
        "latest": summary,
        "evidence": str(out_dir.relative_to(ROOT)),
    }
    json.dump(receipt, open(RECEIPT, "w"), indent=1)
    print(f"csmith: {agreeing} of {total} agree; refused {dict(refused.most_common())}; mismatches {summary['mismatches']}")
    print("receipt:", RECEIPT.relative_to(ROOT), "evidence:", out_dir.relative_to(ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
