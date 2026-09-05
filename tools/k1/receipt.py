#!/usr/bin/env python3
"""Assemble the mixed-link receipt from the evidence of one measurement.

The receipt is a join over runs that already exist on disk: the reference
boot, the survey of every unit, the substitution that was installed, the boot
of the resulting kernel, the proof that the kernel carries that code, and the
bisection if a boot failed. Nothing here re-runs or re-judges anything; it
reads the receipts each step wrote and states the number they add up to.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import sys

REPO_ROOT = Path(__file__).resolve().parents[2]


def load(path: Path | None) -> dict | None:
    if path is None:
        return None
    if not path.is_file():
        print(f"k1: missing evidence file: {path}", file=sys.stderr)
        raise SystemExit(2)
    return json.loads(path.read_text(encoding="utf-8"))


def relative(path: str | None) -> str | None:
    if path is None:
        return None
    try:
        return str(Path(path).resolve().relative_to(REPO_ROOT))
    except ValueError:
        return path


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--reference-boot", type=Path, required=True)
    parser.add_argument("--survey", type=Path, required=True)
    parser.add_argument("--substitution", type=Path, required=True)
    parser.add_argument("--boot", type=Path, required=True)
    parser.add_argument("--proof", type=Path, required=True)
    parser.add_argument("--bisection", type=Path, default=None)
    parser.add_argument("--failing-units", type=Path, default=None)
    parser.add_argument(
        "--output", type=Path, default=REPO_ROOT / "tests/contracts/k1-mixed-link-receipt.json"
    )
    args = parser.parse_args()

    reference = load(args.reference_boot)
    survey = load(args.survey)
    substitution = load(args.substitution)
    boot = load(args.boot)
    proof = load(args.proof)
    bisection = load(args.bisection) if args.bisection else None
    failing = (
        [line.strip() for line in args.failing_units.read_text(encoding="utf-8").splitlines()
         if line.strip()]
        if args.failing_units
        else []
    )

    booted = boot["verdict"] == "PASS"
    substituted = substitution["substitutedUnits"]
    proven = sorted(
        row["unit"] for row in proof["units"] if row["verdict"] == "proven"
    )
    contributing_no_symbols = sorted(
        row["unit"] for row in proof["units"] if row["verdict"] == "no-symbols"
    )

    receipt = {
        "schemaVersion": 1,
        "measurement": "translation units compiled by EvoCC in a kernel that boots",
        "unitsCompiledByEvoccInABootingKernel": len(substituted) if booted else 0,
        "unitsProvenPresentInVmlinux": len(proven) if booted else 0,
        "generatedAt": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "reference": {
            "verdict": reference["verdict"],
            "image": relative(reference["image"]),
            "imageSha256": reference["imageSha256"],
            "referenceImage": relative(reference["referenceImage"]),
            "referenceImageSha256": reference["referenceImageSha256"],
            "initramfsSha256": reference["initramfsSha256"],
            "serialLog": relative(reference["serialLog"]),
            "serialLogSha256": reference["serialLogSha256"],
            "nonce": reference["nonce"],
            "runDirectory": relative(str(Path(reference["serialLog"]).parent)),
        },
        "emulator": reference.get("qemu"),
        "survey": {
            "requestedUnits": survey["requestedUnits"],
            "outcomeCounts": survey["counts"],
            "refusalCodeCounts": survey["refusalCodeCounts"],
            "runDirectory": relative(str(args.survey.parent)),
        },
        "mixedLink": {
            "requestedUnits": substitution["requestedUnits"],
            "outcomeCounts": substitution["counts"],
            "substitutedUnits": substituted,
            "allowSymbolDrift": substitution["allowSymbolDrift"],
            "relinkExitCode": substitution["relinkExitCode"],
            "imageSha256": substitution["imageSha256"],
            "vmlinuxSha256": substitution["vmlinuxSha256"],
            "runDirectory": relative(str(args.substitution.parent)),
            "boot": {
                "verdict": boot["verdict"],
                "nonce": boot["nonce"],
                "imageSha256": boot["imageSha256"],
                "serialLog": relative(boot["serialLog"]),
                "serialLogSha256": boot["serialLogSha256"],
                "runDirectory": relative(str(Path(boot["serialLog"]).parent)),
            },
        },
        "proofOfSubstitution": {
            "producerNoteInMixedVmlinux": proof["producerNote"]["identInMixedVmlinux"],
            "producerNoteInReferenceVmlinux": proof["producerNote"]["identInReferenceVmlinux"],
            "commentStrings": proof["producerNote"]["mixedCommentStrings"],
            "verdictCounts": proof["counts"],
            "unitsProvenByteIdenticalInVmlinux": proven,
            "unitsDefiningNoSymbol": contributing_no_symbols,
        },
        "failingUnits": failing,
        "bisection": (
            {
                "result": bisection["result"],
                "allowlistSize": bisection.get("allowlistSize"),
                "minimalFailingSet": bisection.get("minimalFailingSet"),
                "rounds": bisection.get("rounds"),
                "runDirectory": relative(str(args.bisection.parent)),
            }
            if bisection
            else None
        ),
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(receipt, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"receipt: {args.output}")
    print(json.dumps(
        {
            "unitsCompiledByEvoccInABootingKernel":
                receipt["unitsCompiledByEvoccInABootingKernel"],
            "unitsProvenPresentInVmlinux": receipt["unitsProvenPresentInVmlinux"],
            "surveyOutcomeCounts": receipt["survey"]["outcomeCounts"],
        },
        indent=2,
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
