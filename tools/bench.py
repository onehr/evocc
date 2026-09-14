#!/usr/bin/env python3
"""One entry point for roadmap measurements; the existing lanes own execution.

Examples: python3 tools/bench.py csmith cjson
          python3 tools/bench.py all --jobs 4 --budget 20
Exit 0: selected gates pass or have evidenced PROFILE_CLEAN dispositions.
PROFILE_CLEAN is not an all-program PASS. Exit 1: measured gaps/failures.
Exit 2: usage, build, runner or evidence error. No failures are skipped.
"""

from __future__ import annotations

import argparse
from collections import Counter, deque
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
RECEIPT = Path("tests/contracts/bench-receipt.json")
# name: (roadmap rung, existing adapter, measurement limitation)
CATALOG = {
    "ctest": ("E0b", "e0", "self-checking programs and expected stdout"),
    "torture": ("E0a", "e0", "mixed integer/FP suite; selected options consumed, evidenced inapplicability retained, unknown requirements remain unresolved"),
    "csmith": ("E0c", "csmith", "original seeds; unavailable oracles are not passes"),
    "zlib": ("E1", "subject", "mixed-link program tests"),
    "zstd": ("E2", "subject", "mixed-link program tests; see recorded source/build adjustments"),
    "cjson": ("E3", "subject", "installed objects judged by public API tests; full private-helper suite and FP deferred"),
    "tinycc": ("E5", "mixed", "mixed-link compiler tests"),
    "toybox": ("E6", "mixed", "mixed-link tests relative to baseline failures"),
    "musl": ("E7", "mixed", "libc-test relative to baseline failures"),
    "kernel": ("K1/K5-K7 ramp", "kernel", "mixed-link boot plus substitution proof; not full EvoCC kernel/kselftest"),
    "mbedtls": ("E4", None, "no benchmark adapter yet"),
    "polybench": ("E8", None, "no adapter; floating-point rung deferred"),
    "llvm-test-suite": ("E9", None, "no adapter; mixed integer/FP suite"),
    "jq": ("E10", None, "no adapter; floating-point rung deferred"),
    "lua": ("E11", None, "no adapter; floating-point rung deferred"),
    "sqlite": ("E12", None, "no adapter; floating-point rung deferred"),
    "kernel-full": ("K6", "kernel-full", "all target C objects, generated-input fixed point, full link and real boot; selftests separate"),
    "kernel-selftest": ("K7", "kernel-selftest", "selected upstream kselftests in the existing complete kernel; identical reference-built payload on both kernels, KUnit separate"),
    "kernel-selftest-all": ("K7", "kernel-selftest-all", "all upstream collections attempted; build gaps, unavailable hardware/configuration, skips and timeouts remain separate"),
}


def parser():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("benchmarks", nargs="*", help="names (space/comma separated), or all")
    p.add_argument("--list", action="store_true", help="list adapters and roadmap gaps")
    p.add_argument("--dry-run", action="store_true", help="print commands without building or measuring")
    p.add_argument("--evocc", type=Path, help="measure this existing binary; do not build another one")
    p.add_argument("--no-build", action="store_true", help="reuse the default release binary")
    p.add_argument("--jobs", type=positive, default=4, help="Cargo / emission / Csmith workers; reference build settings stay unchanged")
    p.add_argument("--programs", type=positive, default=500, help="Csmith programs per campaign")
    p.add_argument("--campaign", choices=["all", "base", "bitfields"], default="all")
    p.add_argument("--budget", type=positive, default=40, help="toybox/musl bisection probes")
    p.add_argument("--allow-qemu-drift", action="store_true", help="acknowledge and record the installed QEMU digest")
    p.add_argument("--reuse-kernel-survey", type=Path, help="replay mixed linking from a completed survey of this exact binary")
    p.add_argument("--survey-only", action="store_true", help="kernel compilation/assembly only; no installation, link, bisection or boot")
    p.add_argument("--reuse-selftest-build", action="store_true", help="reuse the previously built full kselftest payload; still execute both kernels")
    p.add_argument('--expanded-selftest-kernel', action='store_true', help='use the audited expanded KUnit kernel pair for full selftests')
    p.add_argument('--selftest-runtime', type=Path, help='per-kernel runtime root with matching module payloads')
    p.add_argument('--refresh-selftest-runtime', action='store_true', help='repair full-suite runtime dependencies without rebuilding test programs')
    p.add_argument('--selftest-select', action='append', default=[], help='replay one collection or collection:program; other full-suite programs remain NOT_RUN')
    p.add_argument("--reuse-selftest-reference", type=Path,
                   help="reuse a completed unchanged GCC reference summary and payload; still execute every program on the current EvoCC kernel")
    return p


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def select(names):
    chosen = []
    for name in names:
        for part in name.split(","):
            expanded = list(CATALOG) if part == "all" else [part]
            for item in expanded:
                if item not in CATALOG:
                    raise ValueError(f"unknown benchmark: {item}; use --list")
                if item not in chosen:
                    chosen.append(item)
    return chosen


def command(name, args, binary):
    env = os.environ.copy()
    env["PYTHONUNBUFFERED"] = "1"
    if name in ("kernel-selftest", "kernel-selftest-all"):
        if args.reuse_kernel_survey:
            raise ValueError("kernel-selftest consumes the booted Image, not a compilation survey")
        if args.allow_qemu_drift:
            env["K1_ALLOW_QEMU_DRIFT"] = "1"
        if name == 'kernel-selftest-all':
            reuse = '--reuse-build' if not args.refresh_selftest_runtime and (args.reuse_selftest_build or args.reuse_selftest_reference) else '--install-deps'
            cmd = [sys.executable, '-u', str(ROOT / 'tools/k1/selftest_full.py'), '--jobs', str(args.jobs), reuse]
            if args.expanded_selftest_kernel:
                cmd += ['--expanded-kernel']
            if args.selftest_runtime:
                cmd += ['--runtime-root', str(args.selftest_runtime)]
            if args.refresh_selftest_runtime:
                cmd += ['--refresh-runtime']
            if args.reuse_selftest_reference:
                cmd += ['--reference-run', str(args.reuse_selftest_reference)]
            for selector in args.selftest_select:
                cmd += ['--select', selector]
            return cmd, env
        return [sys.executable, '-u', str(ROOT / 'tools/k1/selftest.py')], env
    if name == "kernel-full":
        if args.reuse_kernel_survey:
            raise ValueError("kernel-full reuses verified object caches, not a mixed-link allowlist")
        if args.allow_qemu_drift:
            env["K1_ALLOW_QEMU_DRIFT"] = "1"
        return [sys.executable, str(ROOT / "tools/k1/substitute.py"), "--all", "--require-all", "--boot",
                "--evocc", str(binary), "--jobs", str(args.jobs)], env
    if name == "kernel":
        env["K1_EVOCC"] = str(binary)
        if args.survey_only:
            cmd = [sys.executable, str(ROOT / "tools/k1/substitute.py"), "--all", "--no-install",
                   "--evocc", str(binary), "--jobs", str(args.jobs)]
            if run := getattr(args, "survey_run", None):
                cmd += ["--run-id", run]
            return cmd, env
        if args.allow_qemu_drift:
            env["K1_ALLOW_QEMU_DRIFT"] = "1"
        cmd = [str(ROOT / "tools/k1/measure.sh"), "--jobs", str(args.jobs)]
        if args.reuse_kernel_survey:
            survey = args.reuse_kernel_survey.resolve()
            metadata = json.loads((survey / "substitution.json").read_text())
            if metadata.get("evoccSha256") != sha256(binary):
                raise ValueError("the reusable kernel survey belongs to another compiler")
            cmd += ["--reuse-survey", str(survey)]
        return cmd, env
    script = "e0" if name in ("ctest", "torture") else name
    cmd = [sys.executable, "-u", str(ROOT / f"tools/ladder/{script}.py"), "--evocc", str(binary)]
    if script == "e0":
        cmd += ["--suite", name]
    if name == "csmith":
        cmd += ["--campaign", args.campaign, "--programs", str(args.programs), "--jobs", str(args.jobs)]
    if name in ("toybox", "musl", "tinycc"):
        cmd += ["--jobs", str(args.jobs)]
    if name in ("toybox", "musl"):
        cmd += ["--budget", str(args.budget)]
    return cmd, env


def lane_receipt(name):
    if name == "kernel-selftest-all":
        return ROOT / "tests/contracts/kernel-selftest-full-receipt.json"
    if name == "kernel-selftest":
        return ROOT / "tests/contracts/kernel-selftest-receipt.json"
    if name == "kernel-full":
        return ROOT / "tests/contracts/kernel-full-receipt.json"
    stem = "e0" if name in ("ctest", "torture") else name
    return ROOT / ("tests/contracts/k1-mixed-link-receipt.json" if name == "kernel"
                   else f"tests/contracts/ladder-{stem}-receipt.json")


def metrics(name, data):
    """Normalize claim ceilings, without rewriting a lane's raw outcomes."""
    row = {"rung": CATALOG[name][0], "note": CATALOG[name][2], "outcomes": {},
           "refusals": {}, "baselineFailures": 0, "failingUnits": data.get("failingUnits", [])}
    if name == "kernel-selftest-all":
        measured = data.get('execution', {}).get('subject', {}).get('summary', {})
        row.update(total=measured.get('programs', 0), passed=measured.get('outcomes', {}).get('PASS', 0),
                   kind='programs', status=data['status'], outcomes=measured.get('outcomes', {}),
                   collectionCount=measured.get('collections', len(data['build']['targets'])),
                   buildFailures=measured.get('buildFailures', []),
                   noRunnableTests=measured.get('noRunnableTests', []),
                   reference=data.get('execution', {}).get('reference', {}).get('summary', {}),
                   kernelImageSha256=data['kernelImageSha256'])
        if data.get('selection'):
            row['selection'] = data['selection']
            row['note'] += '; targeted replay, unselected programs NOT_RUN'
        if data.get('blocker'):
            row['error'] = data['blocker']
    elif name == "kernel-selftest":
        if 'subject' not in data or 'reference' not in data:
            return {**row, 'total': data['programs'], 'passed': 0, 'kind': 'programs',
                    'status': data['status'], 'outcomes': {'NOT_RUN': data['programs']},
                    'error': data.get('error', 'selftest flights incomplete'),
                    'run': data['run'], 'evidence': data['evidence']}
        outcomes = data['subject']['outcomes']
        booted = all(data[lane]['boot']['verdict'] == 'PASS' for lane in ('reference', 'subject'))
        same_image = data['subject']['boot']['imageSha256'] == data['kernelImageSha256']
        row.update(total=data['programs'], passed=outcomes.get('PASS', 0), kind='programs',
                   outcomes=outcomes, status=data['status'] if booted and same_image else 'INCOMPLETE',
                   baselineFailures=len(data['baselineFailures']), failingUnits=data['regressions'],
                   subtests=data['subject']['subtests'], referenceSubtests=data['reference']['subtests'],
                   kernelImageSha256=data['kernelImageSha256'], compilerComposition=data['compilerComposition'])
    elif name == "kernel-full":
        total = data["requestedUnits"]
        counts = data["counts"]
        complete = data.get("completeComposition") is True and counts == {"substituted": total}
        boot = data.get("boot", {})
        passed = complete and boot.get("verdict") == "PASS" and boot.get("imageSha256") == data.get("imageSha256")
        row.update(total=total, passed=total if passed else 0, compiled=sum(counts.get(key, 0) for key in ("assembled", "substituted")),
                   kind="units", outcomes=counts, status="PASS" if passed else "INCOMPLETE",
                   testVerdict=boot.get("verdict", "NOT_RUN"), evidence=data.get("evidence"),
                   refusals=data.get("refusalsByFirstCode", {}))
    elif name in ("ctest", "torture"):
        values = data["suites"][name]
        row.update(total=values["programs"], passed=values["outcomes"].get("pass", 0),
                   outcomes=values["outcomes"], kind="programs",
                   refusals=values.get("refusalsByCode", {}),
                   runtimeDiagnostics=values.get("failuresByInterpreterVerdict", {}))
        if dispositions := values.get('dispositionCounts'):
            labels = {'PASS', 'compiler-failure', 'FP-deferred', 'not-applicable', 'oracle-unusable'}
            if (set(dispositions) != labels
                    or any(not isinstance(n, int) or n < 0 for n in dispositions.values())
                    or sum(dispositions.values()) != row['total']
                    or dispositions['PASS'] > row['passed']):
                raise ValueError('program dispositions disagree with the original denominator or raw passes')
            row.update(passed=dispositions['PASS'], dispositions=dispositions)
            if not dispositions['compiler-failure'] and dispositions['PASS'] != row['total']:
                row['status'] = 'PROFILE_CLEAN'
    elif name == "csmith":
        outcomes = Counter()
        for campaign in data["campaigns"].values():
            outcomes.update(campaign["outcomes"])
        row.update(total=data["programs"], passed=data["agreeing"], outcomes=dict(outcomes),
                   kind="programs", refusals=data.get("refusalsByCode", {}))
    elif name == "kernel":
        if data.get("mode") == "survey-only":
            total = data["requestedUnits"]
            counts = data["counts"]
            if not isinstance(total, int) or total <= 0 or any(n < 0 for n in counts.values()) or sum(counts.values()) != total:
                raise ValueError("kernel survey counts disagree with denominator")
            row.update(total=total, passed=0, compiled=counts.get("assembled", 0),
                       kind="units", outcomes=counts, status="INCOMPLETE", mode="survey-only",
                       testVerdict="NOT_RUN", refusals=data.get("refusalsByFirstCode", {}),
                       run=data.get("runId"), evidence=data.get("evidence"),
                       cacheHits=data.get("cacheHits", 0), assemblyCacheHits=data.get("assemblyCacheHits", 0),
                       timing=data.get("seconds", {}),
                       note="full compilation survey; linking, boot and selftests NOT_RUN")
            return row
        row.update(total=data["survey"]["requestedUnits"],
                   passed=data["unitsCompiledByEvoccInABootingKernel"], kind="units",
                   compiled=data["survey"]["outcomeCounts"].get("assembled", 0),
                   refusals=data["survey"].get("refusalsByFirstCode", {}),
                   testVerdict=data["mixedLink"]["boot"]["verdict"],
                   proof=data["proofOfSubstitution"]["verdictCounts"])
        row["status"] = "INCOMPLETE"  # this adapter does not run a full-kernel kselftest gate
        if row["testVerdict"] != "PASS" or any(
            count for verdict, count in row["proof"].items()
            if verdict not in ("proven", "proven-modulo-relocations", "no-symbols", "not-linked")
        ):
            row["status"] = "FAIL"
    else:
        total = data.get("unitsTotal", data.get("units"))
        row.update(total=total, passed=data["unitsInPassingBuild"], kind="units",
                   compiled=data.get("unitsEvoccCompiled", data.get("unitsCompiledByEvocc", 0)),
                   refusals=data.get("refusalsByCode", {}),
                   excluded=len(data.get("excludedForSymbolParity", {})),
                   testVerdict=data.get("testResult", data.get("test", {})),
                   adjustments=data.get("adjustments", {}))
        baseline = data.get("baselineTest", {})
        if isinstance(baseline, dict):
            row["baselineFailures"] = baseline.get("libcTestFailingCases", len(baseline.get("failing", [])))
        if name == "cjson" and (
            not isinstance(row["testVerdict"], dict)
            or not row["testVerdict"].get("objectFiles")
            or not set(data.get("substitutedUnits", [])).issubset(
                row["testVerdict"]["objectFiles"]
            )
        ):
            row.update(status="VALIDATION_GAP", passed=0)
        elif isinstance(row["testVerdict"], dict):
            if not row["testVerdict"].get("passed") or row["testVerdict"].get("relinkStatus") != 0:
                row["status"] = "FAIL"
        elif row["testVerdict"] not in (
            "tests-passed", "tests-passed-after-exclusions", "no-regression",
            "libc-test-clean", "libc-test-clean-after-exclusions",
        ):
            row["status"] = "INCOMPLETE" if row["testVerdict"] == "bisection-budget-exhausted" else "FAIL"
    if not isinstance(row["total"], int) or row["total"] <= 0 or not 0 <= row["passed"] <= row["total"]:
        raise ValueError("missing, empty or inconsistent denominator")
    if row["outcomes"] and sum(row["outcomes"].values()) != row["total"]:
        raise ValueError("outcome counts disagree with denominator")
    row.setdefault("status", "PASS" if row["passed"] == row["total"] and not row["baselineFailures"] else "INCOMPLETE")
    row["run"] = data.get("run", data.get("generatedAt"))
    row["evidence"] = data.get("evidence", (
        data.get("survey", {}).get("runDirectory") if name == "kernel"
        else f".evidence/ladder/{'e0' if name in ('ctest', 'torture') else name}/{row['run']}"
    ))
    return row


def read_result(name, before, expected, survey_run=None):
    if survey_run:
        directory = ROOT / ".evidence/k1" / survey_run
        data = json.loads((directory / "substitution.json").read_text())
        if data["evoccSha256"] != expected or data["unitsSha256"] != sha256(directory / "units.jsonl"):
            raise ValueError("kernel survey compiler or per-unit digest differs")
        data.update(mode="survey-only", evidence=str(directory.relative_to(ROOT)))
        return metrics(name, data)
    path = lane_receipt(name)
    after = path.read_bytes()
    if before == after:
        raise ValueError("receipt unchanged; previous evidence is not a new measurement")
    data = json.loads(after)
    data = data.get("latest", data)
    digest = data.get("driverSha256")
    if name == "kernel":
        survey = ROOT / data["survey"]["runDirectory"] / "substitution.json"
        digest = json.loads(survey.read_text())["evoccSha256"]
    if name in ('kernel-selftest', 'kernel-selftest-all'):
        receipt_name = data.get('kernelReceiptPath', 'tests/contracts/kernel-full-receipt.json')
        if receipt_name not in ('tests/contracts/kernel-full-receipt.json', 'tests/contracts/kernel-kunit-receipt.json'):
            raise ValueError('unrecognized kernel acceptance receipt')
        kernel = ROOT / receipt_name
        if data['kernelReceiptSha256'] != sha256(kernel):
            raise ValueError('kernel selftests name a different complete kernel receipt')
    elif digest != expected:
        raise ValueError("receipt names a different compiler binary")
    row = metrics(name, data)
    row["receipt"] = str(path.relative_to(ROOT))
    return row


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def execute(cmd, env):
    """Stream the existing lane; keep only a small error tail, not another log."""
    print("+ " + shlex.join(cmd), flush=True)
    tail = deque(maxlen=20)
    process = subprocess.Popen(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE,
                               stderr=subprocess.STDOUT, text=True, errors="replace", start_new_session=True)
    try:
        for line in process.stdout:
            print(line, end="", flush=True)
            tail.append(line[-1000:].rstrip())
        return process.wait(), list(tail)
    except BaseException:
        os.killpg(process.pid, signal.SIGTERM)
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait()
        raise


def save(summary, directory):
    text = json.dumps(summary, indent=2) + "\n"
    (directory / "summary.json").write_text(text)
    (ROOT / RECEIPT).write_text(text)


def main(argv=None):
    args = parser().parse_args(argv)
    if args.list:
        for name, (rung, adapter, note) in CATALOG.items():
            print(f"{name:18} {rung:16} {'available' if adapter else 'NOT_IMPLEMENTED':16} {note}")
        return 0
    try:
        selected = select(args.benchmarks)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2
    if not selected:
        parser().print_help()
        return 2
    if args.survey_only and (selected != ["kernel"] or args.reuse_kernel_survey):
        print("--survey-only requires kernel alone and cannot reuse a prior survey", file=sys.stderr)
        return 2
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target/frontend")).resolve()
    binary = (args.evocc or target / "release/evocc").resolve()
    needs_compiler = any(name not in ('kernel-selftest', 'kernel-selftest-all') and CATALOG[name][1] for name in selected)
    if args.dry_run:
        for name in selected:
            print(name + ": " + (shlex.join(command(name, args, binary)[0]) if CATALOG[name][1] else CATALOG[name][2]))
        return 0
    (ROOT / ".build").mkdir(exist_ok=True)
    with (ROOT / ".build/bench.lock").open("a") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            print("another benchmark invocation owns the shared work directories", file=sys.stderr)
            return 2
        if needs_compiler and not args.no_build and not args.evocc:
            env = {**os.environ, "CARGO_TARGET_DIR": str(target)}
            code, _ = execute(["cargo", "build", "--release", "-p", "evocc", "--locked", "--offline", "-j", str(args.jobs)], env)
            if code:
                return 2
        if needs_compiler and not binary.is_file():
            print(f"compiler missing: {binary}", file=sys.stderr)
            return 2
        driver = sha256(binary) if needs_compiler else None
        run = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%S.%fZ")
        args.survey_run = f"survey-{run}" if args.survey_only else None
        directory = ROOT / ".evidence/bench" / run
        directory.mkdir(parents=True)
        prior = json.loads((ROOT / RECEIPT).read_text()) if (ROOT / RECEIPT).exists() else {}
        known = prior.get("benchmarks", {})
        for row in known.values():
            row.setdefault("measurement", {key: prior.get(key) for key in ("run", "subject", "driverSha256")})
        known.update({name: {"rung": CATALOG[name][0], "status": "NOT_RUN", "note": CATALOG[name][2]}
                      for name in selected})
        summary = {"schemaVersion": 1, "run": run,
                   "subject": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                   "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()),
                   "driver": str(binary) if needs_compiler else None, "driverSha256": driver,
                   "invocation": [sys.executable, "tools/bench.py", *(sys.argv[1:] if argv is None else argv)],
                   "execution": "sequential lanes; original per-program watchdogs unchanged",
                   "selected": selected, "status": "RUNNING", "benchmarks": known}
        save(summary, directory)
        overall = 0
        try:
            for name in selected:
                if not CATALOG[name][1]:
                    summary["benchmarks"][name] = {"rung": CATALOG[name][0], "status": "NOT_IMPLEMENTED", "note": CATALOG[name][2]}
                    overall = max(overall, 1)
                    save(summary, directory)
                    continue
                path = lane_receipt(name)
                before = path.read_bytes() if path.exists() else None
                previous = None
                if before:
                    try:
                        old = json.loads(before)
                        previous = metrics(name, old.get("latest", old))
                    except (KeyError, TypeError, ValueError):
                        pass
                started = time.monotonic()
                cmd = []
                code, tail = None, []
                try:
                    cmd, env = command(name, args, binary)
                    summary["benchmarks"][name].update(status="RUNNING", command=cmd)
                    save(summary, directory)
                    code, tail = execute(cmd, env)
                    if needs_compiler and sha256(binary) != driver:
                        raise ValueError("compiler changed during evaluation")
                    row = read_result(name, before, driver, args.survey_run)
                    if code:
                        row.update(status="ERROR", error=f"runner exit {code}", errorTail=tail)
                except (OSError, KeyError, TypeError, ValueError) as error:
                    row = {"rung": CATALOG[name][0], "status": "ERROR", "error": str(error), "errorTail": tail}
                row.update(command=cmd, exitCode=code, seconds=round(time.monotonic() - started, 1), previous=previous)
                row["measurement"] = {key: summary[key] for key in ("run", "subject", "driverSha256")}
                summary["benchmarks"][name] = row
                overall = max(overall, 2 if row["status"] == "ERROR" else int(row["status"] not in ('PASS', 'PROFILE_CLEAN')))
                print(f"bench {name}: {row['status']} {row.get('passed', '?')}/{row.get('total', '?')}", flush=True)
                save(summary, directory)
                if needs_compiler and (not binary.is_file() or sha256(binary) != driver):
                    break  # do not mix compiler identities in later lanes
        except KeyboardInterrupt:
            summary["status"] = "INTERRUPTED"
            save(summary, directory)
            return 130
        profile_only = any(summary['benchmarks'][name]['status'] == 'PROFILE_CLEAN' for name in selected)
        summary["status"] = ("PROFILE_CLEAN" if profile_only else "PASS") if overall == 0 else ("ERROR" if overall == 2 else "INCOMPLETE")
        save(summary, directory)
        render = subprocess.run([sys.executable, str(ROOT / "tools/render-status.py")], cwd=ROOT)
        print(f"bench receipt: {RECEIPT}; capture: {directory.relative_to(ROOT)}", flush=True)
        return max(overall, 2 if render.returncode else 0)


if __name__ == "__main__":
    raise SystemExit(main())
