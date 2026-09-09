#!/usr/bin/env python3
"""Rung E6 of the evaluation ladder: toybox through EvoCC.

toybox is one binary made of about four hundred small translation units, one
per applet plus a shared library, which is exactly the shape the mixed link
wants: a refusal costs one applet, not the program. Its build script compiles
every unit separately into ``generated/obj`` and links them in one step, and
it skips a unit whose object is newer than its source, so an installed EvoCC
object survives the relink untouched.

The oracle is ``make tests``, which runs the applets against recorded
expectations. Some of those tests need a filesystem, a user database or a
network that the sealed container does not have, so the baseline is measured
first and the rung passes when the substituted build fails no test the
reference build passed. Anything printf-shaped or floating point is expected
to be refused (E4001 and friends) and is counted, not chased.

Usage:
  python3 tools/ladder/toybox.py [--evocc PATH] [--jobs N] [--skip-reference]
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import re
import shutil
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))

import mixed  # noqa: E402

PIN = {
    "subject": "toybox",
    "rung": "E6",
    "roadmapRelease": "0.8.14 (2026-06-22, github.com/landley/toybox)",
    "tarball": "toybox-0.8.14.tar.gz",
    "origin": "https://github.com/landley/toybox/archive/refs/tags/0.8.14.tar.gz",
    "sha256": "082df8cfd76135ce3c4820c8cf4c0081d61491e0990aff2da48e21fc9bbb24d1",
}

MOUNT = "toybox"
CONTAINER_ROOT = f"/work/{MOUNT}"

BUILD_SCRIPT = f"""
set -e
cd {CONTAINER_ROOT}
export EVO_CMDLOG={CONTAINER_ROOT}/commands.log
: > $EVO_CMDLOG
export HOSTCC=gcc-14
export CC={CONTAINER_ROOT}/evo-recordcc
make defconfig > defconfig.log 2>&1
make -j4 > build.log 2>&1
echo reference-build-ok
"""

TEST_SCRIPT = f"""
cd {CONTAINER_ROOT}
export HOSTCC=gcc-14
export CC={CONTAINER_ROOT}/evo-recordcc
export EVO_CMDLOG={CONTAINER_ROOT}/relink-commands.log
: > $EVO_CMDLOG
make -j4 > relink.log 2>&1
if [ ! -x toybox ]; then echo relink-failed; tail -30 relink.log; exit 0; fi
make tests > tests.log 2>&1 || true
echo tests-done
"""

VERDICT = re.compile(r"^(PASS|FAIL|SKIP):\s*(.*)$", re.M)


def outcomes(log: Path) -> dict[str, str]:
    """Every ``PASS``/``FAIL``/``SKIP`` line the toybox test suite printed."""
    if not log.exists():
        return {}
    found: dict[str, str] = {}
    for verdict, name in VERDICT.findall(log.read_text(errors="replace")):
        key = name.strip()
        if found.get(key) != "FAIL":
            found[key] = verdict
    return found


def failures(log: Path) -> set[str]:
    return {name for name, verdict in outcomes(log).items() if verdict == "FAIL"}


def prepare(build: Path, source: Path) -> tuple[Path, list[str], list[str]]:
    tree = build / "toybox-work"
    if tree.exists():
        shutil.rmtree(tree)
    shutil.copytree(source, tree)
    pristine = build / "toybox-pristine"
    if pristine.exists():
        shutil.rmtree(pristine)
    mixed.install_recorder(tree)
    return tree, [], []


def reference_build(tree: Path) -> str:
    done = mixed.container({MOUNT: tree}, BUILD_SCRIPT, timeout=3600)
    return "ok" if "reference-build-ok" in done.stdout else (done.stdout + done.stderr)[-500:]


def relink_and_test(tree: Path) -> tuple[str, set[str]]:
    done = mixed.container({MOUNT: tree}, TEST_SCRIPT, timeout=5400)
    if "relink-failed" in done.stdout:
        return "relink-failed", set()
    if "tests-done" not in done.stdout:
        return "no-verdict", set()
    return "tests-done", failures(tree / "tests.log")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--evocc", type=Path, default=Path("/Users/onehrxn/dev/30days/c30/target/release/evocc")
    )
    parser.add_argument("--jobs", type=int, default=6)
    parser.add_argument("--skip-reference", action="store_true")
    parser.add_argument("--no-bisect", action="store_true")
    parser.add_argument("--budget", type=int, default=40, help="bisection probes allowed")
    arguments = parser.parse_args()

    build = mixed.BUILD
    source = build / "toybox"
    if not source.is_dir():
        print(f"toybox: the unpacked release is missing at {source}", file=sys.stderr)
        return 2

    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = mixed.EVIDENCE / "toybox" / run_id
    evidence.mkdir(parents=True, exist_ok=True)

    tree = build / "toybox-work"
    stripped: list[str] = []
    specifiers: list[str] = []
    if not arguments.skip_reference:
        tree, stripped, specifiers = prepare(build, source)
        print("toybox: reference build")
        status = reference_build(tree)
        if status != "ok":
            print(f"toybox: reference build failed: {status}", file=sys.stderr)
            return 1
    verdict, baseline_failures = relink_and_test(tree)
    print(f"toybox: baseline {verdict}, {len(baseline_failures)} failing tests")
    if verdict != "tests-done":
        print("toybox: the reference build does not relink", file=sys.stderr)
        return 1
    #: ``generated/flags.h`` exists only after the build, and it is the file
    #: that carries the option-marker bytes, so the re-spelling happens here
    #: rather than in ``prepare``.
    escaped = mixed.escape_high_bytes(tree)
    baseline_outcomes = outcomes(tree / "tests.log")
    shutil.copyfile(tree / "tests.log", evidence / "baseline-tests.log")

    units = mixed.parse_commands(tree / "commands.log", CONTAINER_ROOT)
    #: ``main.o`` carries the generated applet table and the build script
    #: deletes it on every run, so it can never stay substituted.
    units = [unit for unit in units if not unit.name.endswith("/main.o")]
    by_name = {unit.name: unit for unit in units}
    print(f"toybox: {len(units)} units")

    _, include = mixed.ladder_sysroot()
    extra = include + mixed.SYSROOT_DEFINES
    asm_dir = build / "toybox-asm"
    if asm_dir.exists():
        shutil.rmtree(asm_dir)
    rows = mixed.emit_all(units, arguments.evocc, MOUNT, tree, asm_dir, extra, arguments.jobs)
    verdicts = mixed.assemble(rows, by_name, asm_dir)
    for row in rows:
        if row["outcome"] == "emitted":
            row["outcome"] = (
                "assembled" if verdicts.get(row["unit"]) == "ok" else "assembler-refused"
            )

    assembled = [row["unit"] for row in rows if row["outcome"] == "assembled"]
    parity = mixed.symbol_parity(rows, tree, asm_dir)
    candidates, excluded = mixed.compatible_units(parity)
    print(f"toybox: {len(assembled)} units assembled, {len(candidates)} can link together")
    substituted: list[str] = []
    failing: list[str] = []
    test_result = "not-run"
    bisection: dict = {}

    if candidates:
        subject = mixed.Tree(tree, build / "toybox-pristine")
        journal = evidence / "bisect.jsonl"

        def fails(candidate: list[str]) -> tuple[bool, str]:
            mixed.substitute(subject, candidate, asm_dir)
            what, found = relink_and_test(subject.root)
            if what != "tests-done":
                return True, what
            extra_failures = sorted(found - baseline_failures)
            return bool(extra_failures), ",".join(extra_failures[:3]) or "no-regression"

        if arguments.no_bisect:
            mixed.substitute(subject, candidates, asm_dir)
            verdict, found = relink_and_test(subject.root)
            regressions = sorted(found - baseline_failures)
            substituted = candidates if verdict == "tests-done" and not regressions else []
            test_result = "no-regression" if substituted else f"{verdict}:{len(regressions)}-regressions"
        else:
            bisection = mixed.narrow(candidates, fails, journal, arguments.budget)
            substituted = bisection["passingSet"]
            failing = bisection["failingUnits"]
            if bisection["budgetExhausted"]:
                test_result = "bisection-budget-exhausted"
            elif substituted:
                test_result = "no-regression"
            else:
                test_result = "regressions-everywhere"
        if (tree / "tests.log").exists():
            shutil.copyfile(tree / "tests.log", evidence / "substituted-tests.log")
        subject.restore_all()

    refusals = mixed.refusal_table(rows)
    receipt = {
        "schemaVersion": 1,
        "rung": PIN["rung"],
        "subject": PIN["subject"],
        "authority": "docs/roadmap.md, milestone ladder",
        "run": run_id,
        "pin": PIN,
        "driverSha256": mixed.sha256(arguments.evocc),
        "target": mixed.TARGET,
        "baselineTest": {
            "tests": len(baseline_outcomes),
            "failing": sorted(baseline_failures),
        },
        "units": len(units),
        "unitsCompiledByEvocc": sum(
            1 for row in rows if row["outcome"] in ("assembled", "assembler-refused")
        ),
        "unitsInPassingBuild": len(substituted),
        "refusalsByCode": refusals,
        "testResult": test_result,
        "failingUnits": failing,
        "excludedForSymbolParity": excluded,
        "symbolParity": {
            unit: sides
            for unit, sides in parity.items()
            if sides["added"] or sides["missing"]
        },
        "adjustments": {
            "sourcePageBreaksStripped": stripped,
            "noreturnSpecifiersRemoved": specifiers,
            "highBytesReEscaped": escaped,
            "sysroot": mixed.SYSROOT_ADJUSTMENTS,
            "defines": mixed.SYSROOT_DEFINES,
            "unitsNotSubstitutable": ["the object built from main.c"],
        },
        "unconsumedGccOptions": mixed.omitted_table(rows),
        "evidence": str(evidence),
    }
    (evidence / "units.jsonl").write_text(
        "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8"
    )
    (evidence / "summary.json").write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    if bisection:
        (evidence / "bisect.json").write_text(json.dumps(bisection, indent=1) + "\n", encoding="utf-8")
    path = mixed.write_receipt("toybox", receipt)
    print(mixed.summary_line("toybox", len(substituted), len(units), refusals))
    print(
        f"toybox: {len(assembled)} units compiled and assembled by EvoCC; "
        f"{len(excluded)} of them are set aside for symbol parity: a static"
        " function that reached the object file as a global symbol, or an"
        " alias the object never defined"
    )
    print(f"toybox: receipt {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
