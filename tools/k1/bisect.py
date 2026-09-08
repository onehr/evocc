#!/usr/bin/env python3
"""Find the smallest set of substituted units that stops the kernel booting.

The search is delta debugging over the allowlist. It starts by halving, which
is what a single guilty unit needs, and only refines the partition when
neither half fails on its own, which is what an interaction between units
needs. It ends on a 1-minimal failing set: removing any one of its units makes
the kernel boot again.

Every round re-substitutes from the pristine copy, so a round never inherits
an object an earlier round installed, and every boot is recorded.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
import subprocess
import sys
from typing import Callable, Iterable, Sequence

REPO_ROOT = Path(__file__).resolve().parents[2]


def default_evocc() -> Path:
    """The release driver, honoring a shared CARGO_TARGET_DIR."""
    if os.environ.get("K1_EVOCC"):
        return Path(os.environ["K1_EVOCC"])
    target = os.environ.get("CARGO_TARGET_DIR")
    if target:
        return Path(target) / "release/evocc"
    return REPO_ROOT / "target/release/evocc"
K1_DIR = Path(__file__).resolve().parent

PASS = "PASS"
FAIL = "FAIL"


def ddmin(units: Sequence[str], fails: Callable[[tuple[str, ...]], bool]) -> list[str]:
    """The smallest subset of ``units`` that still ``fails``.

    ``fails`` answers whether a candidate allowlist reproduces the failure.
    The result is 1-minimal: no single unit can be removed from it and leave
    the failure in place. The caller is responsible for having established
    that the whole set fails; a set of one is returned unexamined.
    """
    candidate = list(units)
    if len(candidate) <= 1:
        return candidate
    granularity = 2
    while len(candidate) >= 2:
        chunks = _partition(candidate, min(granularity, len(candidate)))
        reduced = False
        for chunk in chunks:
            if chunk and fails(tuple(chunk)):
                candidate = chunk
                granularity = 2
                reduced = True
                break
        if reduced:
            continue
        for chunk in chunks:
            complement = [unit for unit in candidate if unit not in set(chunk)]
            if complement and len(complement) < len(candidate) and fails(tuple(complement)):
                candidate = complement
                granularity = max(granularity - 1, 2)
                reduced = True
                break
        if reduced:
            continue
        if granularity >= len(candidate):
            break
        granularity = min(granularity * 2, len(candidate))
    return candidate


def _partition(units: Sequence[str], parts: int) -> list[list[str]]:
    """Split into ``parts`` chunks whose sizes differ by at most one."""
    parts = max(1, min(parts, len(units)))
    size, remainder = divmod(len(units), parts)
    chunks: list[list[str]] = []
    start = 0
    for index in range(parts):
        end = start + size + (1 if index < remainder else 0)
        chunks.append(list(units[start:end]))
        start = end
    return chunks


class Trial:
    """Substitute one candidate allowlist, relink, boot, and cache the verdict."""

    def __init__(self, run_dir: Path, args: argparse.Namespace) -> None:
        self.run_dir = run_dir
        self.args = args
        self.cache: dict[tuple[str, ...], str] = {}
        self.rounds: list[dict] = []
        self.journal = run_dir / "bisect.jsonl"

    def verdict(self, units: Iterable[str]) -> str:
        key = tuple(sorted(units))
        if key in self.cache:
            return self.cache[key]
        index = len(self.rounds) + 1
        round_dir = self.run_dir / f"round-{index:03d}"
        round_dir.mkdir(parents=True, exist_ok=True)
        allowlist = round_dir / "allowlist.txt"
        allowlist.write_text("".join(f"{unit}\n" for unit in key), encoding="utf-8")

        substitution = subprocess.run(
            [
                sys.executable, str(K1_DIR / "substitute.py"),
                "--allowlist", str(allowlist),
                "--k1-root", str(self.args.k1_root),
                "--src-root", str(self.args.src_root),
                "--evocc", str(self.args.evocc),
                "--evidence-root", str(round_dir),
                "--run-id", "substitution",
                "--jobs", str(self.args.jobs),
            ]
            + (["--allow-symbol-drift"] if self.args.allow_symbol_drift else []),
            capture_output=True,
            text=True,
            check=False,
        )
        (round_dir / "substitute.log").write_text(
            substitution.stdout + "\n--- stderr ---\n" + substitution.stderr, encoding="utf-8"
        )
        if substitution.returncode != 0:
            verdict = FAIL
            reason = "relink-failed"
        else:
            boot = subprocess.run(
                [str(K1_DIR / "boot.sh"),
                 str(self.args.k1_root / "out/arch/arm64/boot/Image"),
                 str(round_dir / "boot")],
                capture_output=True,
                text=True,
                check=False,
            )
            (round_dir / "boot-console.log").write_text(
                boot.stdout + "\n--- stderr ---\n" + boot.stderr, encoding="utf-8"
            )
            verdict = PASS if boot.returncode == 0 else FAIL
            reason = boot.stdout.strip().splitlines()[-1] if boot.stdout.strip() else "unknown"

        record = {
            "round": index,
            "units": list(key),
            "unitCount": len(key),
            "verdict": verdict,
            "reason": reason,
            "roundDirectory": str(round_dir),
            "at": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        }
        self.rounds.append(record)
        with self.journal.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=True) + "\n")
        print(f"k1: round {index}: {len(key)} units -> {verdict} ({reason})", flush=True)
        self.cache[key] = verdict
        return verdict

    def fails(self, units: Iterable[str]) -> bool:
        return self.verdict(units) == FAIL


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("allowlist", type=Path, help="the failing allowlist")
    parser.add_argument("--k1-root", type=Path, default=REPO_ROOT / ".build/k1")
    parser.add_argument("--src-root", type=Path, default=REPO_ROOT / ".build/linux-src")
    parser.add_argument("--evocc", type=Path, default=default_evocc())
    parser.add_argument("--evidence-root", type=Path, default=REPO_ROOT / ".evidence/k1")
    parser.add_argument("--run-id", default=None)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--allow-symbol-drift", action="store_true")
    parser.add_argument(
        "--assume-failing",
        action="store_true",
        help="skip the confirming run of the whole allowlist",
    )
    args = parser.parse_args()

    units = [
        line.split("#", 1)[0].strip()
        for line in args.allowlist.read_text(encoding="utf-8").splitlines()
    ]
    units = [unit for unit in units if unit]
    if not units:
        print("k1: the allowlist is empty", file=sys.stderr)
        return 2

    run_id = args.run_id or ("bisect-" + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ"))
    run_dir = args.evidence_root / run_id
    run_dir.mkdir(parents=True, exist_ok=True)
    trial = Trial(run_dir, args)

    if not args.assume_failing and not trial.fails(units):
        print("k1: the whole allowlist boots; there is nothing to bisect", file=sys.stderr)
        (run_dir / "bisect.json").write_text(
            json.dumps({"runId": run_id, "result": "no-failure", "units": units}, indent=2) + "\n",
            encoding="utf-8",
        )
        return 0

    minimal = ddmin(units, trial.fails)
    summary = {
        "runId": run_id,
        "result": "minimal-failing-set",
        "allowlist": units,
        "allowlistSize": len(units),
        "minimalFailingSet": minimal,
        "minimalFailingSetSize": len(minimal),
        "rounds": len(trial.rounds),
        "journal": str(trial.journal),
    }
    (run_dir / "bisect.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
