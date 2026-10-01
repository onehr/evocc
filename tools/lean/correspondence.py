#!/usr/bin/env python3
"""Segment C of the Lean receipt: the Rust implementations against the Lean model.

Runs the vector generator, the Lean evaluator and the three Rust runners over
the same inputs, compares every file pairwise with tools/lean/diff.py's rules
(an abstention is neither an agreement nor a difference) and writes
tests/contracts/lean-correspondence-receipt.json. This is test evidence about
assumption A7 of docs/lean-plan.md, never a proof: it says how many vectors
the producer's known-bits analysis, the checker's facts and the interpreter
answered exactly as the Lean definitions the theorems are about.

Selected runs replace only their own groups in the receipt. Totals cover all
retained groups; file_runs preserves each group's run, commit and toolchain.

Usage: python3 tools/lean/correspondence.py [--skip-build] [--set all|guards]
Requires a finished `lake build` under lean/ and an offline cargo workspace.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VECTORS = ROOT / "lean" / "vectors"
RECEIPT = ROOT / "tests" / "contracts" / "lean-correspondence-receipt.json"
FILES = ["transfers-small", "lattice-small", "values-width8", "random", "named", "guards"]
ABSTENTIONS = {"skipped", "unsupported"}
RUNNERS = {
    "producer": ("evocc-eqopt", "--example", "known_bits_vectors", "examples/known_bits_vectors"),
    "checker": ("evocc-proofcheck", "--bin", "evo-facts-vectors", "evo-facts-vectors"),
    "interpreter": ("evocc-evoir", "--example", "value_vectors", "examples/value_vectors"),
}


def run(cmd: list[str], cwd: Path = ROOT) -> None:
    subprocess.run(cmd, cwd=cwd, check=True, stdout=subprocess.DEVNULL)


def target_dir() -> Path:
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps", "--offline"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return Path(json.loads(out)["target_directory"])


def canonical(value):
    return json.dumps(value, sort_keys=True)


def abstained(value) -> bool:
    return isinstance(value, str) and value in ABSTENTIONS


def compare(left: Path, right: Path) -> dict:
    counts = {"vectors": 0, "compared": 0, "agreements": 0, "differences": 0, "abstentions": 0}
    with open(left) as a, open(right) as b:
        for la, lb in zip(a, b):
            counts["vectors"] += 1
            ra, rb = json.loads(la), json.loads(lb)
            if abstained(ra.get("result")) or abstained(rb.get("result")):
                counts["abstentions"] += 1
                continue
            counts["compared"] += 1
            if canonical(ra.get("result")) == canonical(rb.get("result")):
                counts["agreements"] += 1
            else:
                counts["differences"] += 1
    return counts


def merge_receipts(previous: dict, current: dict) -> dict:
    files = {**previous.get("files", {}), **current["files"]}
    file_runs = dict(previous.get("file_runs", {}))
    metadata = ("run", "commit", "leanToolchain")
    for name in previous.get("files", {}):
        file_runs.setdefault(name, {key: previous[key] for key in metadata})
    for name in current["files"]:
        file_runs[name] = {key: current[key] for key in metadata}
    total = {key: sum(counts[key] for sides in files.values() for counts in sides.values())
             for key in ("compared", "agreements", "differences")}
    return {**current, "files": files, "file_runs": file_runs, "total": total}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--set", choices=["all", "guards"], default="all")
    args = ap.parse_args()
    files = FILES if args.set == "all" else [args.set]
    run([sys.executable, "tools/lean/vectors.py", "--set", args.set])
    binaries = {}
    debug = target_dir() / "debug"
    for side, (package, kind, name, relative) in RUNNERS.items():
        if not args.skip_build:
            run(["cargo", "build", "-q", "--offline", "-p", package, kind, name])
        binaries[side] = debug / relative
    for name in files:
        run(["lake", "exe", "evoir-sem", f"vectors/{name}.jsonl", f"vectors/{name}.lean.jsonl"], cwd=ROOT / "lean")
        for side, binary in binaries.items():
            run([str(binary), str(VECTORS / f"{name}.jsonl"), str(VECTORS / f"{name}.{side}.jsonl")])
    results = {}
    for name in files:
        results[name] = {}
        for side in RUNNERS:
            counts = compare(VECTORS / f"{name}.lean.jsonl", VECTORS / f"{name}.{side}.jsonl")
            results[name][side] = counts
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()
    receipt = {
        "schemaVersion": 1,
        "segment": "C",
        "authority": "docs/lean-plan.md, sections 2 and 6; assumption A7 of docs/evoir-spec.md 11.7",
        "meaning": "differential test evidence between the Rust implementations and the Lean model; not a proof",
        "run": dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
        "commit": head,
        "leanToolchain": (ROOT / "lean" / "lean-toolchain").read_text().strip(),
        "sides": {side: f"{r[0]} {r[1]} {r[2]}" for side, r in RUNNERS.items()},
        "rule": "an abstention (skipped or unsupported) on either side is neither an agreement nor a difference",
        "files": results,
    }
    previous = json.loads(RECEIPT.read_text()) if RECEIPT.exists() else {}
    receipt = merge_receipts(previous, receipt)
    RECEIPT.write_text(json.dumps(receipt, indent=1) + "\n")
    total = receipt["total"]
    print(json.dumps(total))
    for name in files:
        for side, counts in results[name].items():
            print(f"{name:16} {side:12} compared={counts['compared']:7} agreements={counts['agreements']:7} differences={counts['differences']} abstentions={counts['abstentions']}")
    print("receipt:", RECEIPT.relative_to(ROOT))
    return 0 if total["differences"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
