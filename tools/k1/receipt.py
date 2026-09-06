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

sys.path.insert(0, str(Path(__file__).resolve().parent))

import plan  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]


def load(path: Path | None) -> dict | None:
    if path is None:
        return None
    if not path.is_file():
        print(f"k1: missing evidence file: {path}", file=sys.stderr)
        raise SystemExit(2)
    return json.loads(path.read_text(encoding="utf-8"))


#: Repository-relative roots the evidence lives under. The build and evidence
#: trees may be reached through a worktree symlink, so a resolved path is
#: rewritten from the last of these markers rather than from the checkout.
EVIDENCE_ROOTS = ("/.evidence/", "/.build/", "/tests/", "/tools/")


def relative(path: str | None) -> str | None:
    """A repository-relative path, so the receipt is not host-specific."""
    if path is None:
        return None
    resolved = Path(path).resolve()
    try:
        return str(resolved.relative_to(REPO_ROOT.resolve()))
    except ValueError:
        pass
    text = str(resolved)
    for marker in EVIDENCE_ROOTS:
        index = text.rfind(marker)
        if index >= 0:
            return text[index + 1 :]
    return text


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--reference-boot", type=Path, required=True)
    parser.add_argument("--survey", type=Path, required=True)
    parser.add_argument("--substitution", type=Path, required=True)
    parser.add_argument("--boot", type=Path, required=True)
    parser.add_argument("--proof", type=Path, required=True)
    parser.add_argument("--bisection", type=Path, default=None)
    parser.add_argument("--survey-units", type=Path, default=None,
                        help="the survey's units.jsonl, for the per-code refusal table")
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
    survey_rows = (
        plan.load_rows(args.survey_units)
        if args.survey_units and args.survey_units.is_file()
        else []
    )
    failing = (
        [line.strip() for line in args.failing_units.read_text(encoding="utf-8").splitlines()
         if line.strip()]
        if args.failing_units
        else []
    )

    # The evidence must describe one kernel. A proof of a different vmlinux,
    # or a boot of a different Image, would join runs that never coexisted.
    if proof.get("vmlinuxSha256") != substitution["vmlinuxSha256"]:
        print(
            "k1: the proof and the substitution describe different vmlinux images",
            file=sys.stderr,
        )
        return 2
    if boot["imageSha256"] != substitution["imageSha256"]:
        print("k1: the boot and the substitution describe different Images", file=sys.stderr)
        return 2

    booted = boot["verdict"] == "PASS"
    substituted = substitution["substitutedUnits"]
    proven = sorted(
        row["unit"]
        for row in proof["units"]
        if row["verdict"] in ("proven", "proven-modulo-relocations")
    )
    contributing_no_symbols = sorted(
        row["unit"] for row in proof["units"] if row["verdict"] == "no-symbols"
    )
    not_linked = sorted(row["unit"] for row in proof["units"] if row["verdict"] == "not-linked")
    # The measurement counts units of the kernel. An object Kbuild builds but
    # does not link into vmlinux, such as a compiler feature probe, is not one.
    in_kernel = sorted(proof["unitsLinkedIntoVmlinux"])

    receipt = {
        "schemaVersion": 1,
        "measurement": "translation units compiled by EvoCC in a kernel that boots",
        "unitsCompiledByEvoccInABootingKernel": len(in_kernel) if booted else 0,
        "unitsSubstituted": len(substituted),
        # Units that assembled but were kept out of the link because another
        # candidate defines a symbol they define. They are not refusals and
        # not boot failures; they are the linker's objection, counted apart.
        "unitsExcludedForMultipleDefinition": len(
            plan.link_collisions(survey_rows)["units"]
        ),
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
            "assemblingUnits": sum(
                survey["counts"].get(outcome, 0) for outcome in plan.ASSEMBLING_OUTCOMES
            ),
            "outcomeCounts": survey["counts"],
            "refusalCodeCounts": survey["refusalCodeCounts"],
            # Which stage each refusal belongs to. A machine-side code is the
            # backend's next piece of work, not the front end's.
            "refusalsByCode": plan.refusal_table(survey_rows),
            "refusalsByFirstCode": plan.first_code_table(survey_rows),
            "refusalsByOwner": plan.owner_totals(survey_rows),
            # Objects the linker cannot take together, read off the survey's
            # symbol tables. These bound the measurement without refusing a
            # single unit, so they are counted apart from the refusals.
            "linkCollisions": plan.link_collisions(survey_rows),
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
            "provenVmlinuxSha256": proof["vmlinuxSha256"],
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
            "unitsLinkedIntoVmlinux": in_kernel,
            "unitsProvenByteIdenticalInVmlinux": proven,
            "unitsDefiningNoSymbol": contributing_no_symbols,
            "unitsNotLinkedIntoVmlinux": not_linked,
        },
        "failingUnits": failing,
        "failingUnitsByReason": plan.failures_by_reason(failing, survey_rows),
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
    print(plan.summary_line(
        receipt["unitsCompiledByEvoccInABootingKernel"],
        receipt["survey"]["assemblingUnits"],
        receipt["survey"]["requestedUnits"],
    ))
    print(json.dumps(
        {
            "unitsCompiledByEvoccInABootingKernel":
                receipt["unitsCompiledByEvoccInABootingKernel"],
            "unitsSubstituted": receipt["unitsSubstituted"],
            "unitsProvenPresentInVmlinux": receipt["unitsProvenPresentInVmlinux"],
            "surveyOutcomeCounts": receipt["survey"]["outcomeCounts"],
        },
        indent=2,
    ))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
