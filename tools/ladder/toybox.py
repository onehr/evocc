#!/usr/bin/env python3
"""Rung E6 of the evaluation ladder: toybox through EvoCC.

toybox is one binary made of about four hundred small translation units, one
per applet plus a shared library, which is exactly the shape the mixed link
wants: a refusal costs one applet, not the program. Its build script compiles
every unit separately into ``generated/obj`` and links them in one step, and
it skips a unit whose object is newer than its source, so an installed EvoCC
object survives the relink untouched.

The oracle is the original ``scripts/test.sh`` recipe, with its documented
``VERBOSE=all`` option so a failure does not truncate the suite. Baseline
failures and skips remain non-passes; a substituted run must also complete,
preserve every test occurrence and regress no baseline outcome. Installed
EvoCC objects are hashed before and after relinking/testing. Unsupported
source features are counted separately from test and environment failures.

Usage:
  python3 tools/ladder/toybox.py [--evocc PATH] [--jobs N] [--skip-reference]
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
from pathlib import Path
import re
import shlex
import shutil
import subprocess
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
cd {CONTAINER_ROOT} || exit 2
export HOSTCC=gcc-14
export CC={CONTAINER_ROOT}/evo-recordcc
export EVO_CMDLOG={CONTAINER_ROOT}/relink-commands.log
: > $EVO_CMDLOG
: > tests.log
# Replay the captured original link, preserving every argument. The upstream
# make target ignores changed .o files; forcing make.sh instead can discard all
# installed objects when its append-only generated/build.sh cache grows.
if ! @LINK_COMMAND@ > relink.log 2>&1 || [ ! -x generated/unstripped/toybox ]; then
  echo relink-failed; tail -30 relink.log; exit 2
fi
strip -s -R '.note*' -R .comment generated/unstripped/toybox -o toybox || exit 2
chmod 555 toybox || exit 2
binary_sha=$(sha256sum toybox | cut -d ' ' -f1) || exit 2
# Upstream creates PREFIX under its current tree. /work is a host-backed FUSE
# mount without FS_IOC_GETFLAGS; merely setting TESTDIR does not relocate it.
# Copy only the unchanged runtime recipe/data, not target objects or sources.
# LOOP_SET_STATUS64 has a 64-byte file-name field; the upstream losetup test
# uses the full runtime/generated/testdir/testdir/blah.img path.
runtime=$(mktemp -d /tmp/tb.XXXXXX) || exit 2
trap 'rm -rf -- "$runtime"' EXIT
mkdir -p "$runtime/generated/unstripped" "$runtime/lib" || exit 2
cp -a scripts tests configure .config toybox "$runtime/" || exit 2
cp lib/toyflags.h "$runtime/lib/" || exit 2
for header in generated/*.h; do
  [ ! -f "$header" ] || cp "$header" "$runtime/generated/" || exit 2
done
cmp -s toybox "$runtime/toybox" || exit 2
cd "$runtime" || exit 2
# Same upstream test recipe, without asking make to rebuild substituted code.
# Source it in a child shell: an early `exit` cannot print the completion marker.
# A normal failing suite returns false at the end, which must still be recorded.
unset TEST_HOST TEST_ALL DEBUG SHOWPASS SHOWFAIL SHOWSKIP
USER=$(id -un) || exit 2
export USER
ASAN=1 VERBOSE=all bash -c '
  . scripts/test.sh
  status=$?
  printf "\\nEVOCC_TOYBOX_TESTS_COMPLETE status=%s\\n" "$status"
  exit "$status"
' > {CONTAINER_ROOT}/tests.log 2>&1
status=$?
# A complete-looking log cannot authorize a replaced executable.
printf '%s  %s\\n' "$binary_sha" toybox "$binary_sha" generated/testdir/toybox |
  sha256sum -c - >/dev/null || exit 2
cmp -s toybox {CONTAINER_ROOT}/toybox || exit 2
exit "$status"
"""

VERDICT = re.compile(r"^(PASS|FAIL|SKIP):[ \t]*(.*)$", re.M)
COMPLETION = re.compile(r"^EVOCC_TOYBOX_TESTS_COMPLETE status=([0-9]+)$", re.M)
OutcomeId = tuple[str, int]


def outcomes(log: Path) -> dict[OutcomeId, str]:
    """Every ``PASS``/``FAIL``/``SKIP`` line the toybox test suite printed."""
    if not log.exists():
        return {}
    found: dict[OutcomeId, str] = {}
    occurrences: dict[str, int] = {}
    for verdict, name in VERDICT.findall(log.read_text(errors="replace")):
        name = name.strip()
        occurrences[name] = occurrences.get(name, 0) + 1
        found[name, occurrences[name]] = verdict
    return found


def outcome_name(key: OutcomeId) -> str:
    return f"{key[0]} [occurrence {key[1]}]"


def failures(log: Path) -> set[str]:
    return {outcome_name(key) for key, verdict in outcomes(log).items() if verdict == "FAIL"}


def regressions(baseline: dict[OutcomeId, str], found: dict[OutcomeId, str]) -> list[str]:
    """Fail closed on lost/added occurrences, new failures and lost execution."""
    differences = [f"missing {outcome_name(key)}" for key in baseline.keys() - found.keys()]
    differences += [f"unexpected {outcome_name(key)}" for key in found.keys() - baseline.keys()]
    for key in baseline.keys() & found.keys():
        before, after = baseline[key], found[key]
        if after != before and after != "PASS":
            differences.append(f"{outcome_name(key)}: {before} -> {after}")
    return sorted(differences)


def audit_objects(tree: Path, expected: dict[str, str]) -> list[str]:
    return mixed.audit_objects(tree, expected)


def test_summary(log: Path, verdict: str) -> dict:
    found = outcomes(log)
    return {
        "completed": verdict == "tests-done",
        "verdict": verdict,
        "tests": len(found),
        "outcomes": {status: list(found.values()).count(status) for status in ("PASS", "FAIL", "SKIP")},
        "failing": sorted(failures(log)),
        "logSha256": mixed.sha256(log) if log.is_file() else None,
        "binarySha256": mixed.sha256(log.parent / "toybox") if (log.parent / "toybox").is_file() else None,
        "runtimePolicy": {
            "workingDirectory": "ephemeral-container-local",
            "addedCapabilities": ["CAP_LINUX_IMMUTABLE", "CAP_SYS_ADMIN"],
            "user": "executing uid resolved by id -un",
            "temporaryPathPolicy": "/tmp/tb.XXXXXX; original loop backing path fits 64-byte field",
            "targetObjectsCopied": False,
        },
    }


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
    if done.returncode == 0 and "reference-build-ok" in done.stdout:
        return "ok"
    return (done.stdout + done.stderr)[-500:] or f"exit {done.returncode}"


def link_command(tree: Path) -> list[str]:
    """Replay only the frozen final object link, never a source compilation."""
    log = tree / "commands.log"
    links = []
    for record in log.read_bytes().split(b"\x01\n"):
        fields = [field.decode("utf-8") for field in record.strip(b"\n").split(b"\0") if field]
        if len(fields) < 3 or fields[0] != CONTAINER_ROOT:
            continue
        argv = fields[1:]
        if "-o" not in argv or argv[-2:] != ["-o", "generated/unstripped/toybox"]:
            continue
        if "-c" in argv or "-E" in argv or any(arg.endswith((".c", ".S")) for arg in argv):
            raise ValueError("toybox final link unexpectedly compiles source")
        links.append(argv)
    if not links or any(argv != links[0] for argv in links):
        raise ValueError("missing or inconsistent original toybox final link")
    units = {unit.name for unit in mixed.parse_commands(log, CONTAINER_ROOT)}
    objects = [arg for arg in links[0] if arg.endswith(".o")]
    if not units or set(objects) != units or len(objects) != len(units):
        raise ValueError("toybox link objects do not match recorded compilation population")
    # install_recorder's compiler is fixed to gcc-14 in the sealed image.
    return ["gcc-14", *links[0]]


def relink_and_test(
    tree: Path, expected_objects: dict[str, str] | None = None,
) -> tuple[str, dict[OutcomeId, str]]:
    expected_objects = expected_objects or {}
    if audit_objects(tree, expected_objects):
        return "object-mismatch-before-test", {}
    try:
        command = link_command(tree)
    except (OSError, UnicodeError, ValueError):
        return "invalid-link-record", {}
    try:
        done = mixed.container(
            {MOUNT: tree}, TEST_SCRIPT.replace("@LINK_COMMAND@", shlex.join(command)),
            timeout=5400, capabilities=("CAP_LINUX_IMMUTABLE", "CAP_SYS_ADMIN"),
        )
    except subprocess.TimeoutExpired:
        return "test-timeout", {}
    if "relink-failed" in done.stdout:
        return "relink-failed", {}
    log = tree / "tests.log"
    contents = log.read_text(errors="replace") if log.is_file() else ""
    completed = COMPLETION.findall(contents)
    found = outcomes(log)
    if (len(completed) != 1 or not found
            or not contents.rstrip().endswith(f"EVOCC_TOYBOX_TESTS_COMPLETE status={completed[0]}")):
        return "no-verdict", found
    status = int(completed[0])
    if (status not in (0, 1) or done.returncode != status
            or (status == 1) != ("FAIL" in found.values())
            or any(not key[0] for key in found)):
        return "inconsistent-verdict", found
    if audit_objects(tree, expected_objects):
        return "object-mismatch-after-test", found
    return "tests-done", found


def probe_candidate(
    subject: mixed.Tree, candidate: list[str], asm_dir: Path, baseline: dict[OutcomeId, str],
) -> tuple[bool, str, dict]:
    installed = mixed.substitute(subject, candidate, asm_dir)
    if set(installed) != set(candidate):
        return True, "missing-object", {"completed": False, "verdict": "missing-object"}
    expected = {
        unit: mixed.sha256(asm_dir / "obj" / (mixed.flat_name(unit) + ".o"))
        for unit in candidate
    }
    verdict, found = relink_and_test(subject.root, expected)
    summary = test_summary(subject.root / "tests.log", verdict)
    summary["objectFiles"] = expected
    changes = regressions(baseline, found) if verdict == "tests-done" else []
    summary["regressions"] = changes
    failed = verdict != "tests-done" or bool(changes)
    note = verdict if verdict != "tests-done" else ",".join(changes[:3]) or "no-regression"
    return failed, note, summary


def select_and_confirm(
    subject: mixed.Tree, candidates: list[str], asm_dir: Path,
    baseline: dict[OutcomeId, str], evidence: Path, *, budget: int, no_bisect: bool,
) -> tuple[list[str], list[str], str, dict, dict]:
    """The last bisection probe/cache entry is not the final installed build."""
    bisection: dict = {}
    failing: list[str] = []
    try:
        if no_bisect:
            selected = candidates
        else:
            def fails(candidate):
                failed, note, _ = probe_candidate(subject, candidate, asm_dir, baseline)
                return failed, note

            bisection = mixed.narrow(candidates, fails, evidence / "bisect.jsonl", budget)
            selected = bisection["passingSet"]
            failing = bisection["failingUnits"]
        if not selected:
            return [], failing, "no-validated-substitution", bisection, {}
        # Always execute, even if this candidate was cached by narrow().
        failed, note, summary = probe_candidate(subject, selected, asm_dir, baseline)
        if summary.get("logSha256"):
            shutil.copyfile(subject.root / "tests.log", evidence / "substituted-tests.log")
        result = f"final-replay-failed:{note}" if failed else "no-regression"
        if not failed and bisection.get("budgetExhausted"):
            result = "bisection-budget-exhausted"
        return [] if failed else selected, failing, result, bisection, summary
    finally:
        subject.restore_all()


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
    verdict, baseline_outcomes = relink_and_test(tree)
    baseline_failures = failures(tree / "tests.log")
    print(f"toybox: baseline {verdict}, {len(baseline_failures)} failing tests")
    if verdict != "tests-done":
        print("toybox: the reference suite did not complete consistently", file=sys.stderr)
        return 1
    baseline_test = test_summary(tree / "tests.log", verdict)
    shutil.copyfile(tree / "tests.log", evidence / "baseline-tests.log")

    units = mixed.parse_commands(tree / "commands.log", CONTAINER_ROOT)
    #: Preserve the frozen 203-unit population: generated applet-table main.o
    #: was excluded by the original make-based adapter and remains reference-built.
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
    substituted_test: dict = {}

    if candidates:
        subject = mixed.Tree(tree, build / "toybox-pristine")
        substituted, failing, test_result, bisection, substituted_test = select_and_confirm(
            subject, candidates, asm_dir, baseline_outcomes, evidence,
            budget=arguments.budget, no_bisect=arguments.no_bisect,
        )

    refusals = mixed.refusal_table(rows)
    receipt = {
        "schemaVersion": 2,
        "rung": PIN["rung"],
        "subject": PIN["subject"],
        "authority": "docs/archive/roadmap.md, milestone ladder",
        "run": run_id,
        "pin": PIN,
        "driverSha256": mixed.sha256(arguments.evocc),
        "target": mixed.TARGET,
        "baselineTest": baseline_test,
        "substitutedTest": substituted_test,
        "substitutedUnits": substituted,
        "testContract": {
            "recipe": "scripts/test.sh (original make tests recipe)",
            "verbose": "all",
            "ASAN": "1 (original recipe environment, not a sanitizer-instrumentation claim)",
            "relinkArgv": link_command(tree),
            "relink": "original recorded object link plus upstream strip/chmod; no source rebuild",
            "completion": "normal return from sourced upstream recipe; status/outcomes agree",
            "identity": "test name plus occurrence; identical reference/subject population",
            "objects": "installed EvoCC hashes checked before and after relink/test",
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
            "highBytesReEscaped": [],
            "sysroot": mixed.SYSROOT_ADJUSTMENTS,
            "defines": mixed.SYSROOT_DEFINES,
            "unitsNotSubstitutable": ["main.c: reference-built generated applet table, outside frozen population"],
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
