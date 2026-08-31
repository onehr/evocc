#!/usr/bin/env python3
"""Publish the kernel parse/preprocess dashboard from observer-sealed evidence.

This is the stage gate the roadmap names: the numbers are produced by this
script, from evidence the pinned observer sealed, with the command written
beside each number. It never predicts, never edits a previous receipt, and
refuses to write anything if any lane regressed against the previous published
run, because a dashboard that can go backwards silently is not a gate.

Flow:
  1. observer `capture` runs the reconnaissance harness as its child and seals
     stdout, stderr and exit status;
  2. the routing tool classifies every failed lane against the previous
     published run;
  3. the receipt is written only if the run is complete, sealed, on the frozen
     cohort, and regression-free.

`--self-test` exercises the refusal paths on synthetic inputs so a broken
guard shows up as a failing self-test rather than as a receipt nobody trusts.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OBSERVER = ROOT / "tests/acceptance/day5-kernel-reductions-observer.py"
RECON = ROOT / "tests/acceptance/day5-kernel-reconnaissance.py"
ROUTER = ROOT / "tools/route-kernel-failures.py"
COHORT = ROOT / "tests/contracts/day5-kernel-cohort.json"
RECEIPT = ROOT / "tests/contracts/kernel-dashboard-receipt.json"
EXPECTED_OBSERVATIONS = 48
RECON_ARGS = [
    "--timeout",
    "120",
    "--parser-timeout",
    "10",
    "--parser-jobs",
    "4",
    "--subject-profile",
    "release",
]


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Refusal(Exception):
    """A reason the dashboard must not be written. The message is the finding."""


def load_results(path: Path) -> list[dict]:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if len(rows) != EXPECTED_OBSERVATIONS:
        raise Refusal(f"incomplete run: {len(rows)}/{EXPECTED_OBSERVATIONS} observations")
    return rows


def check_sealed(capture_status: Path) -> dict:
    if not capture_status.exists():
        raise Refusal("unsealed evidence: no observer capture status")
    status = json.loads(capture_status.read_text())
    for key in ("argvSha256", "environmentSha256", "returnCode", "stdoutSha256", "stderrSha256", "timedOut"):
        if key not in status:
            raise Refusal(f"unsealed evidence: capture status lacks {key}")
    if status["timedOut"] or status["returnCode"] != 0:
        raise Refusal(f"reconnaissance did not complete: rc={status['returnCode']} timedOut={status['timedOut']}")
    return status


def check_cohort(run_dir: Path) -> None:
    frozen = sha256(COHORT)
    used = run_dir / "kernel-cohort.json"
    if not used.exists() or sha256(used) != frozen:
        raise Refusal("cohort mismatch: the run did not use the frozen cohort")


def measurement_configuration(run_dir: Path, root: Path = ROOT) -> dict:
    path = run_dir / "run-config.json"
    if not path.is_file():
        raise Refusal("measurement configuration is absent")
    config = json.loads(path.read_text())
    expected = {
        "subjectBuildProfile": "release",
        "parserTimeoutSecondsPerTu": 10,
        "parserJobs": 4,
    }
    actual = {name: config.get(name) for name in expected}
    if actual != expected:
        raise Refusal(f"measurement configuration mismatch: {actual!r}")
    return {
        **actual,
        "change": "subject compiler changed from debug to release; timeout remained 10 seconds",
        "runConfig": {"path": str(path.relative_to(root)), "sha256": sha256(path)},
    }


def regressions(current: list[dict], previous: list[dict]) -> list[str]:
    before = {(r["tuId"], r["lane"]): r["outcome"] for r in previous}
    return sorted(
        f"{r['tuId']}/{r['lane']}"
        for r in current
        if before.get((r["tuId"], r["lane"])) not in (None, "failure") and r["outcome"] == "failure"
    )


def summarize(rows: list[dict]) -> dict:
    lanes = {}
    for r in rows:
        lane = lanes.setdefault(r["lane"], {"clean": 0, "failed": 0})
        lane["clean" if r["outcome"] != "failure" else "failed"] += 1
    by_tu = {}
    for r in rows:
        if r["lane"].endswith("preprocessed-parser"):
            by_tu.setdefault(r["tuId"], []).append(r["outcome"] != "failure")
    dual = sum(1 for outcomes in by_tu.values() if len(outcomes) == 2 and all(outcomes))
    return {"lanes": lanes, "dualOracleCleanParseTus": f"{dual}/{len(by_tu)}"}


def capture_dir(evidence_dir: Path, subject_commit: str) -> Path:
    """One capture directory per subject commit; an existing one is never reused."""
    path = evidence_dir / subject_commit
    if path.exists():
        raise Refusal(f"capture directory already used: {path}")
    path.mkdir(parents=True)
    return path


def run_dir_from_capture(stdout_text: str) -> Path:
    """The run directory comes from this capture's own summary line, never from 'latest'."""
    match = re.search(r"^DAY5_KERNEL_RECON .*\bevidence=(\S+)", stdout_text, re.M)
    if not match:
        raise Refusal("capture stdout carries no reconnaissance evidence line")
    return Path(match.group(1))


def publish(previous_results: Path, evidence_dir: Path, subject_commit: str) -> dict:
    evidence_dir = capture_dir(evidence_dir, subject_commit)
    capture = {k: evidence_dir / f"reconnaissance.{k}" for k in ("stdout", "stderr", "status")}
    env = {k: os.environ[k] for k in ("PATH", "HOME") if k in os.environ}
    env.update({"LANG": "C", "LC_ALL": "C", "TZ": "UTC", "TMPDIR": tempfile.gettempdir()})
    command = [
        sys.executable, str(OBSERVER), "capture",
        "--stdout", str(capture["stdout"]), "--stderr", str(capture["stderr"]),
        "--status", str(capture["status"]), "--timeout", "3600",
        *[a for k, v in env.items() for a in ("--environment", f"{k}={v}")],
        "--", sys.executable, str(RECON), *RECON_ARGS,
    ]
    subprocess.run(command, check=False)
    status = check_sealed(capture["status"])
    run_dir = run_dir_from_capture(capture["stdout"].read_text())
    check_cohort(run_dir)
    measurement = measurement_configuration(run_dir)
    results = run_dir / "results.jsonl"
    current = load_results(results)
    previous = load_results(previous_results)
    regressed = regressions(current, previous)
    if regressed:
        raise Refusal("lane regression against the previous published run: " + ", ".join(regressed))
    routing_out = evidence_dir / "routing.json"
    subprocess.run([sys.executable, str(ROUTER), "--run", str(results), "--baseline", str(previous_results),
                    "--out", str(routing_out)], check=False)
    routing = json.loads(routing_out.read_text())
    receipt = {
        "receiptId": "c30.kernel-dashboard",
        "subjectCommit": subject_commit,
        "publishedFromRun": run_dir.name,
        "previousRun": previous_results.parent.name,
        "cohortSha256": sha256(COHORT),
        "observer": {"path": str(OBSERVER.relative_to(ROOT)), "sha256": sha256(OBSERVER)},
        "capture": status,
        "command": " ".join(RECON_ARGS and [str(RECON.relative_to(ROOT)), *RECON_ARGS]),
        "measurementConfiguration": measurement,
        "dashboard": summarize(current),
        "routing": routing["summary"]["byRoute"],
        "regressionsAgainstPrevious": [],
        "artifacts": {name: {"path": str(path.relative_to(ROOT)), "sha256": sha256(path)}
                      for name, path in (("results", results), ("routing", routing_out), ("captureStatus", capture["status"]))},
    }
    RECEIPT.write_text(json.dumps(receipt, indent=2) + "\n")
    return receipt


def self_test() -> int:
    """Each refusal path must refuse. A guard that does not fire is the defect."""
    failures = []
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        good = [{"tuId": f"t{i}", "lane": "gcc-preprocessed-parser" if i % 2 else "clang-preprocessed-parser",
                 "outcome": "failure"} for i in range(EXPECTED_OBSERVATIONS)]
        results = tmp / "results.jsonl"
        results.write_text("\n".join(json.dumps(r) for r in good) + "\n")
        try:
            load_results(results)
        except Refusal as e:
            failures.append(f"complete run refused: {e}")
        short = tmp / "short.jsonl"
        short.write_text("\n".join(json.dumps(r) for r in good[:-1]) + "\n")
        try:
            load_results(short)
            failures.append("incomplete run accepted")
        except Refusal:
            pass
        try:
            check_sealed(tmp / "missing.status")
            failures.append("unsealed evidence accepted")
        except Refusal:
            pass
        bad_status = tmp / "bad.status"
        bad_status.write_text(json.dumps({"argvSha256": "x", "environmentSha256": "x", "returnCode": 1,
                                          "stdoutSha256": "x", "stderrSha256": "x", "timedOut": False}))
        try:
            check_sealed(bad_status)
            failures.append("failed reconnaissance accepted")
        except Refusal:
            pass
        run_dir = tmp / "run"
        run_dir.mkdir()
        (run_dir / "kernel-cohort.json").write_text("{}")
        try:
            check_cohort(run_dir)
            failures.append("cohort mismatch accepted")
        except Refusal:
            pass
        previous = [dict(r, outcome="success") if r["tuId"] == "t3" else r for r in good]
        if regressions(good, previous) != ["t3/gcc-preprocessed-parser"]:
            failures.append("lane regression not detected")
        if regressions(good, good):
            failures.append("false regression reported")
        used = tmp / "used-capture"
        used.mkdir()
        try:
            capture_dir(tmp, "used-capture")
            failures.append("existing capture directory reused")
        except Refusal:
            pass
        if capture_dir(tmp, "fresh").name != "fresh":
            failures.append("fresh capture directory not created")
        try:
            run_dir_from_capture("no evidence line here\n")
            failures.append("capture without an evidence line accepted")
        except Refusal:
            pass
        if run_dir_from_capture("DAY5_KERNEL_RECON observed=48/48 evidence=/x/run-1 summary_sha256=0\n") != Path("/x/run-1"):
            failures.append("evidence line not parsed")
        config_run = tmp / "configured-run"
        config_run.mkdir()
        (config_run / "run-config.json").write_text(
            json.dumps(
                {
                    "subjectBuildProfile": "release",
                    "parserTimeoutSecondsPerTu": 10,
                    "parserJobs": 4,
                }
            )
        )
        try:
            measurement_configuration(config_run, tmp)
        except Refusal as error:
            failures.append(f"release measurement configuration refused: {error}")
        (config_run / "run-config.json").write_text(
            json.dumps(
                {
                    "subjectBuildProfile": "debug",
                    "parserTimeoutSecondsPerTu": 10,
                    "parserJobs": 4,
                }
            )
        )
        try:
            measurement_configuration(config_run, tmp)
            failures.append("debug subject measurement configuration accepted")
        except Refusal:
            pass
    for f in failures:
        print("kernel-dashboard: FAIL:", f, file=sys.stderr)
    print("kernel-dashboard self-test:", "PASS" if not failures else "FAIL")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--previous", type=Path, help="results.jsonl of the previously published run")
    parser.add_argument("--evidence", type=Path, default=ROOT / ".evidence/kernel-dashboard")
    parser.add_argument("--commit", default=subprocess.run(["git", "rev-parse", "--short", "HEAD"],
                                                           capture_output=True, text=True, cwd=ROOT).stdout.strip())
    args = parser.parse_args()
    if args.self_test:
        return self_test()
    if not args.previous:
        parser.error("--previous is required unless --self-test")
    try:
        receipt = publish(args.previous, args.evidence, args.commit)
    except Refusal as e:
        print("kernel-dashboard: REFUSED:", e, file=sys.stderr)
        return 3
    print(json.dumps(receipt["dashboard"]))
    return 0


if __name__ == "__main__":
    sys.exit(main())
