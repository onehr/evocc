#!/usr/bin/env python3
"""Rung E7 of the evaluation ladder: musl plus libc-test through EvoCC.

musl is the one subject on this lane that needs no exported sysroot: it is
built ``-nostdinc`` against its own headers, so the include paths in the
recorded command are the whole story and the glibc headers -- the wall every
hosted subject on this lane runs into -- never enter the picture. What it
asks for instead is the C library's own vocabulary: variadic callees,
aggregate returns, thread-local storage, the syscall inline assembly of
``arch/aarch64``.

The oracle is libc-test, whose ``make`` compiles, links and runs every case
and concatenates what failed into ``src/REPORT``; an empty REPORT is a clean
libc. The rung's number is the one the ladder asks for even when the whole
library cannot be built by EvoCC: how many translation units substitute into
a libc that still passes.

``--disable-shared`` is passed to musl's configure so the library is one set
of ``.o`` files rather than ``.o`` and ``.lo`` built from the same sources,
and libc-test is linked ``-static`` for the same reason: what is measured
should be the objects that were substituted.

Usage:
  python3 tools/ladder/musl.py [--evocc PATH] [--jobs N] [--budget N]
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
    "subject": "musl",
    "rung": "E7",
    "roadmapRelease": "musl 1.2.6 (2026-03-20, musl.libc.org/releases.html)",
    "tarball": "musl-1.2.6.tar.gz",
    "origin": "https://musl.libc.org/releases/musl-1.2.6.tar.gz",
    "sha256": "d585fd3b613c66151fc3249e8ed44f77020cb5e6c1e635a616d3f9f82460512a",
    "libcTest": {
        "origin": "https://repo.or.cz/libc-test.git",
        "commit": "7b95dfa5f5d5ca4d949221e0228ccc290bacc14e",
        "originNote": (
            "git.musl-libc.org answered 502 Bad Gateway during this run, so the"
            " repo.or.cz mirror was cloned; there are no releases, and this is"
            " the HEAD this receipt pins"
        ),
    },
}

MOUNT = "musl"
TEST_MOUNT = "libc-test"
CONTAINER_ROOT = f"/work/{MOUNT}"

CONFIG_MAK = """CC = /work/musl/sysroot/bin/musl-gcc
CFLAGS += -pipe -std=c99 -D_POSIX_C_SOURCE=200809L -Wall -Wno-unused-function
CFLAGS += -Wno-missing-braces -Wno-unused -Wno-overflow -Wno-unknown-pragmas
CFLAGS += -fno-builtin -frounding-math -g
LDFLAGS += -g -static
"""

BUILD_SCRIPT = f"""
set -e
cd {CONTAINER_ROOT}
export EVO_CMDLOG={CONTAINER_ROOT}/commands.log
: > $EVO_CMDLOG
./configure --disable-shared --prefix={CONTAINER_ROOT}/sysroot \
  CC={CONTAINER_ROOT}/evo-recordcc > configure.log 2>&1
make -j4 > build.log 2>&1
make install > install.log 2>&1
echo musl-built
export EVO_CMDLOG=/dev/null
cd /work/{TEST_MOUNT}
cp {CONTAINER_ROOT}/libc-test-config.mak config.mak
make -j4 > build.log 2>&1 || true
echo reference-build-ok
"""

RELINK_SCRIPT = f"""
cd {CONTAINER_ROOT}
export EVO_CMDLOG=/dev/null
make -j4 > relink.log 2>&1
if [ ! -f lib/libc.a ]; then echo relink-failed; tail -30 relink.log; exit 0; fi
make install > install.log 2>&1
cd /work/{TEST_MOUNT}
rm -f src/*/*.exe src/*/*.err src/*/*.ld.err src/REPORT src/*/REPORT
make -j4 > relink.log 2>&1 || true
echo tests-done
"""


FAILED_CASE = re.compile(r"^(?:FAIL|BUILDERROR) (\S+)", re.M)


def report_failures(report: Path) -> set[str]:
    """The test binaries libc-test reported as failed or unbuildable.

    ``src/REPORT`` is the concatenation of every case's error output, so its
    line count moves with how talkative a failure is. What identifies a
    failure is the case, and comparing the set of cases is what tells a
    regression from a baseline that was already red: libc-test has no
    releases, its HEAD tests interfaces musl 1.2.6 does not have yet, and
    those cases fail for the reference build too.
    """
    if not report.exists():
        return set()
    return set(FAILED_CASE.findall(report.read_text(errors="replace")))


def prepare(build: Path, source: Path, test_source: Path) -> tuple[Path, Path, dict]:
    tree = build / "musl-work"
    tests = build / "libc-test-work"
    for path, origin, ignore in (
        (tree, source, None),
        (tests, test_source, shutil.ignore_patterns(".git")),
    ):
        if path.exists():
            shutil.rmtree(path)
        shutil.copytree(origin, path, ignore=ignore)
    pristine = build / "musl-pristine"
    if pristine.exists():
        shutil.rmtree(pristine)
    stripped = mixed.strip_page_breaks(tree)
    specifiers = mixed.strip_specifiers(tree)
    escaped = mixed.escape_high_bytes(tree)
    mixed.install_recorder(tree)
    (tree / "libc-test-config.mak").write_text(CONFIG_MAK, encoding="utf-8")
    return tree, tests, {
        "sourcePageBreaksStripped": stripped,
        "noreturnSpecifiersRemoved": specifiers,
        "highBytesReEscaped": escaped,
    }


def reference_build(tree: Path, tests: Path) -> tuple[str, set[str]]:
    done = mixed.container({MOUNT: tree, TEST_MOUNT: tests}, BUILD_SCRIPT, timeout=7200)
    if "reference-build-ok" not in done.stdout:
        return (done.stdout + done.stderr)[-600:], set()
    return "ok", report_failures(tests / "src" / "REPORT")


def relink_and_test(tree: Path, tests: Path) -> tuple[str, set[str]]:
    done = mixed.container({MOUNT: tree, TEST_MOUNT: tests}, RELINK_SCRIPT, timeout=7200)
    if "relink-failed" in done.stdout:
        return "relink-failed", set()
    if "tests-done" not in done.stdout:
        return "no-verdict", set()
    return "tests-done", report_failures(tests / "src" / "REPORT")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--evocc", type=Path, default=Path("/Users/onehrxn/dev/30days/c30/target/release/evocc")
    )
    parser.add_argument("--jobs", type=int, default=6)
    parser.add_argument("--budget", type=int, default=40, help="bisection probes allowed")
    parser.add_argument("--skip-reference", action="store_true")
    arguments = parser.parse_args()

    build = mixed.BUILD
    source, test_source = build / "musl", build / "libc-test"
    for path in (source, test_source):
        if not path.is_dir():
            print(f"musl: missing {path}", file=sys.stderr)
            return 2

    run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    evidence = mixed.EVIDENCE / "musl" / run_id
    evidence.mkdir(parents=True, exist_ok=True)

    tree, tests = build / "musl-work", build / "libc-test-work"
    source_adjustments: dict = {}
    if not arguments.skip_reference:
        tree, tests, source_adjustments = prepare(build, source, test_source)
        print("musl: reference build and libc-test baseline")
        status, baseline = reference_build(tree, tests)
        if status != "ok":
            print(f"musl: reference build failed: {status}", file=sys.stderr)
            return 1
    else:
        _, baseline = relink_and_test(tree, tests)
    print(f"musl: baseline has {len(baseline)} failing libc-test cases")
    if (tests / "src" / "REPORT").exists():
        shutil.copyfile(tests / "src" / "REPORT", evidence / "baseline-REPORT")

    units = mixed.parse_commands(tree / "commands.log", CONTAINER_ROOT)
    by_name = {unit.name: unit for unit in units}
    print(f"musl: {len(units)} units")

    asm_dir = build / "musl-asm"
    if asm_dir.exists():
        shutil.rmtree(asm_dir)
    #: musl is built ``-nostdinc`` against its own headers, so the recorded
    #: include paths are complete and no exported sysroot is added.
    rows = mixed.emit_all(units, arguments.evocc, MOUNT, tree, asm_dir, [], arguments.jobs)
    verdicts = mixed.assemble(rows, by_name, asm_dir)
    for row in rows:
        if row["outcome"] == "emitted":
            row["outcome"] = (
                "assembled" if verdicts.get(row["unit"]) == "ok" else "assembler-refused"
            )

    assembled = [row["unit"] for row in rows if row["outcome"] == "assembled"]
    parity = mixed.symbol_parity(rows, tree, asm_dir)
    candidates, excluded = mixed.compatible_units(parity)
    print(f"musl: {len(assembled)} units assembled, {len(candidates)} can link together")

    substituted: list[str] = []
    failing: list[str] = []
    test_result = "not-run"
    bisection: dict = {}
    if candidates:
        subject = mixed.Tree(tree, build / "musl-pristine")
        journal = evidence / "bisect.jsonl"

        def fails(candidate: list[str]) -> tuple[bool, str]:
            mixed.substitute(subject, candidate, asm_dir)
            what, failed = relink_and_test(subject.root, tests)
            if what != "tests-done":
                return True, what
            regressions = sorted(failed - baseline)
            return bool(regressions), ",".join(regressions[:3]) or "no-regression"

        bisection = mixed.narrow(candidates, fails, journal, arguments.budget)
        substituted = bisection["passingSet"]
        failing = bisection["failingUnits"]
        if bisection["budgetExhausted"]:
            test_result = "bisection-budget-exhausted"
        elif substituted == candidates:
            test_result = "libc-test-clean"
        elif substituted:
            test_result = "libc-test-clean-after-exclusions"
        else:
            test_result = "libc-test-regressed"
        if (tests / "src" / "REPORT").exists():
            shutil.copyfile(tests / "src" / "REPORT", evidence / "substituted-REPORT")
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
            "libcTestFailingCases": len(baseline),
            "cases": sorted(baseline),
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
            **source_adjustments,
            "sysroot": "none: musl is built -nostdinc against its own headers",
            "configure": "--disable-shared, so the library is one set of objects",
            "libcTestConfig": CONFIG_MAK,
        },
        "unconsumedGccOptions": mixed.omitted_table(rows),
        "evidence": str(evidence),
    }
    (evidence / "units.jsonl").write_text(
        "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows), encoding="utf-8"
    )
    (evidence / "summary.json").write_text(json.dumps(receipt, indent=1) + "\n", encoding="utf-8")
    if bisection:
        (evidence / "bisect.json").write_text(
            json.dumps(bisection, indent=1) + "\n", encoding="utf-8"
        )
    path = mixed.write_receipt("musl", receipt)
    print(mixed.summary_line("musl", len(substituted), len(units), refusals))
    print(
        f"musl: {len(assembled)} units compiled and assembled by EvoCC; "
        f"{len(excluded)} of them are set aside for symbol parity: a static"
        " function that reached the object file as a global symbol, or an"
        " alias the object never defined"
    )
    print(f"musl: receipt {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
