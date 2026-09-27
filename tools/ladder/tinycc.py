#!/usr/bin/env python3
"""Rung E5 of the evaluation ladder: TinyCC through EvoCC.

The subject is a compiler, so the oracle is unusually strong: ``make test``
builds tcc, then makes tcc compile and run about forty of its own test
programs, an assembler test, a bounds-checking test and a self-compilation
test. A substituted object that miscompiles one of tcc's tables shows up as a
wrong answer from a program tcc itself produced.

The pin is ``mob`` at ``0fb54300b565``, which the roadmap allows in place of
0.9.27 for the AArch64 fixes, and which this rung needs: 0.9.27 predates
tcc's AArch64 backend, so ``./configure`` on arm64 has no target to select.
The 0.9.27 tarball was fetched for its digest anyway; savannah answered 504
on the day of the run, and that is recorded rather than worked around.

Usage:
  python3 tools/ladder/tinycc.py [--evocc PATH] [--jobs N] [--skip-reference]
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mixed  # noqa: E402

PIN = {
    "subject": "tinycc",
    "rung": "E5",
    "roadmapRelease": "0.9.27 (2017-12-17), mob 0fb54300b565 allowed for AArch64",
    "used": "mob 0fb54300b565",
    "commit": "0fb54300b56512754221d80adda85ddb9815bceb",
    "origin": "https://repo.or.cz/tinycc.git",
    "originNote": (
        "download.savannah.gnu.org answered 504 Gateway Time-out for both the"
        " 0.9.27 tarball and the git repository during this run, so the"
        " repo.or.cz mirror of the same history was used; the commit is the"
        " one the roadmap pins"
    ),
}

MOUNT = "tinycc"
CONTAINER_ROOT = f"/work/{MOUNT}"

CONFIGURE = f"""
set -e
cd {CONTAINER_ROOT}
export EVO_CMDLOG={CONTAINER_ROOT}/commands.log
: > $EVO_CMDLOG
./configure --cc={CONTAINER_ROOT}/evo-recordcc > configure.log 2>&1
make -j4 > build.log 2>&1
echo reference-build-ok
"""

RELINK_AND_TEST = f"""
cd {CONTAINER_ROOT} || exit 1
rm -f tcc libtcc.a || exit 1
if ! make tcc libtcc.a > relink.log 2>&1; then
    echo relink-failed; tail -40 relink.log; exit 1
fi
if [ ! -x tcc ]; then echo relink-failed; exit 1; fi
if make test > test.log 2>&1 && grep -Fxq -- '------- ALL TESTS PASSED --------' test.log; then
    echo tests-passed
else
    echo tests-failed; exit 1
fi
"""


def prepare(build: Path, source: Path) -> tuple[Path, dict]:
    """A clean copy of the pinned tree, preserving supported source syntax."""
    tree = build / "tinycc"
    if tree.exists():
        shutil.rmtree(tree)
    shutil.copytree(source, tree, ignore=shutil.ignore_patterns(".git"))
    pristine = build / "tinycc-pristine"
    if pristine.exists():
        shutil.rmtree(pristine)
    mixed.install_recorder(tree)
    return tree, {
        "sourcePageBreaksStripped": [],
        "noreturnSpecifiersRemoved": [],
        "highBytesReEscaped": [],
    }


def reference_build(tree: Path) -> str:
    done = mixed.container({MOUNT: tree}, CONFIGURE, timeout=3600)
    return "ok" if "reference-build-ok" in done.stdout else (done.stdout + done.stderr)[-400:]


def run_tests(tree: Path, expected_objects: dict[str, str] | None = None) -> tuple[bool, str]:
    expected_objects = expected_objects or {}
    if mixed.audit_objects(tree, expected_objects):
        return False, "object-mismatch-before-test"
    done = mixed.container({MOUNT: tree}, RELINK_AND_TEST, timeout=5400)
    if mixed.audit_objects(tree, expected_objects):
        return False, "object-mismatch-after-test"
    lines = done.stdout.splitlines()
    for verdict in ("tests-failed", "relink-failed", "tests-passed"):
        if verdict in lines:
            if verdict == "tests-passed" and done.returncode != 0:
                return False, f"runner-exit-{done.returncode}"
            return verdict == "tests-passed", verdict
    return False, "no-verdict"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--evocc", type=Path, default=Path("/Users/onehrxn/dev/30days/c30/target/release/evocc"))
    parser.add_argument("--jobs", type=int, default=6)
    parser.add_argument("--skip-reference", action="store_true")
    arguments = parser.parse_args()

    build = mixed.BUILD
    source = build / "tinycc-git"
    if not source.is_dir():
        print(f"tinycc: the pinned checkout is missing at {source}", file=sys.stderr)
        return 2

    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = mixed.EVIDENCE / "tinycc" / run_id
    evidence.mkdir(parents=True, exist_ok=True)

    tree = build / "tinycc"
    source_adjustments: dict = {}
    if not arguments.skip_reference:
        tree, source_adjustments = prepare(build, source)
        print("tinycc: reference build")
        status = reference_build(tree)
        if status != "ok":
            print(f"tinycc: reference build failed: {status}", file=sys.stderr)
            return 1
        passed, note = run_tests(tree)
        for name in ('relink', 'test'):
            if (tree / f'{name}.log').is_file():
                shutil.copyfile(tree / f'{name}.log', evidence / f'{name}-baseline.log')
        print(f"tinycc: baseline {note}")
        baseline = note
        if not passed:
            print("tinycc: the reference build does not pass its own tests", file=sys.stderr)
            return 1
    else:
        baseline = "assumed-passing"

    units = mixed.parse_commands(tree / "commands.log", CONTAINER_ROOT)
    by_name = {unit.name: unit for unit in units}
    print(f"tinycc: {len(units)} units")

    sysroot, include = mixed.ladder_sysroot()
    extra = include + mixed.SYSROOT_DEFINES
    asm_dir = build / "tinycc-asm"
    if asm_dir.exists():
        shutil.rmtree(asm_dir)
    rows = mixed.emit_all(units, arguments.evocc, MOUNT, tree, asm_dir, extra, arguments.jobs)
    verdicts = mixed.assemble(rows, by_name, asm_dir)
    for row in rows:
        if row["outcome"] == "emitted":
            row["outcome"] = "assembled" if verdicts.get(row["unit"]) == "ok" else "assembler-refused"

    assembled = [row["unit"] for row in rows if row["outcome"] == "assembled"]
    parity = mixed.symbol_parity(rows, tree, asm_dir)
    candidates, excluded = mixed.compatible_units(parity)
    print(f"tinycc: {len(candidates)} of {len(assembled)} assembled units can link together")
    substituted: list[str] = []
    failing: list[str] = []
    test_result = "not-run"
    bisection: dict = {}
    trial_audits: list[dict] = []

    if candidates:
        subject_tree = mixed.Tree(tree, build / "tinycc-pristine")
        journal = evidence / "bisect.jsonl"

        def fails(candidate: list[str]) -> tuple[bool, str]:
            installed = mixed.substitute(subject_tree, candidate, asm_dir)
            expected = {unit: mixed.sha256(asm_dir / 'obj' / (mixed.flat_name(unit) + '.o'))
                        for unit in installed}
            if installed != candidate:
                ok, why = False, 'incomplete-install'
            else:
                ok, why = run_tests(subject_tree.root, expected)
            tag = str(len(trial_audits) + 1)
            logs = {}
            for name in ('relink', 'test'):
                source = tree / f'{name}.log'
                if source.is_file():
                    destination = evidence / f'{name}-{tag}.log'
                    shutil.copyfile(source, destination)
                    logs[name] = {'file': destination.name, 'sha256': mixed.sha256(destination)}
            trial_audits.append({
                'units': installed, 'expectedObjectHashes': expected,
                'changedObjects': mixed.audit_objects(tree, expected),
                'passed': ok, 'verdict': why, 'logs': logs,
                'binarySha256': mixed.sha256(tree / 'tcc') if (tree / 'tcc').is_file() else None,
            })
            return (not ok), why

        bisection = mixed.narrow(candidates, fails, journal)
        substituted = bisection["passingSet"]
        failing = bisection["failingUnits"]
        if substituted == candidates:
            test_result = "tests-passed"
        elif substituted:
            test_result = "tests-passed-after-exclusions"
        else:
            test_result = "tests-failed"
        subject_tree.restore_all()

    refusals = mixed.refusal_table(rows)
    receipt = {
        "schemaVersion": 1,
        "rung": PIN["rung"],
        "subject": PIN["subject"],
        "authority": "docs/roadmap.md, milestone ladder",
        "run": run_id,
        "pin": PIN,
        "tarballSha256": {
            "tcc-0.9.27.tar.bz2": "not-fetched: savannah answered 504 during this run",
        },
        "driverSha256": mixed.sha256(arguments.evocc),
        "target": mixed.TARGET,
        "baselineTest": baseline,
        "units": len(units),
        "unitsCompiledByEvocc": sum(1 for row in rows if row["outcome"] in ("assembled", "assembler-refused")),
        "unitsInPassingBuild": len(substituted),
        "refusalsByCode": refusals,
        "testResult": test_result,
        "trialAudits": trial_audits,
        "failingUnits": failing,
        "excludedForSymbolParity": excluded,
        "symbolParity": {
            unit: sides
            for unit, sides in parity.items()
            if sides["added"] or sides["missing"]
        },
        "adjustments": {
            **source_adjustments,
            "sysroot": mixed.SYSROOT_ADJUSTMENTS,
            "defines": mixed.SYSROOT_DEFINES,
        },
        "unconsumedGccOptions": mixed.omitted_table(rows),
        "evidence": str(evidence.relative_to(mixed.ROOT)) if str(evidence).startswith(str(mixed.ROOT)) else str(evidence),
    }
    (evidence / "units.jsonl").write_text(
        "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8"
    )
    (evidence / "summary.json").write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    if bisection:
        (evidence / "bisect.json").write_text(json.dumps(bisection, indent=1) + "\n", encoding="utf-8")
    path = mixed.write_receipt("tinycc", receipt)
    print(mixed.summary_line("tinycc", len(substituted), len(units), refusals))
    print(
        f"tinycc: {len(assembled)} units compiled and assembled by EvoCC; "
        f"{len(excluded)} of them are set aside for symbol parity: a static"
        " function that reached the object file as a global symbol, or an"
        " alias the object never defined"
    )
    print(f"tinycc: receipt {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
