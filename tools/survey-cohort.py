#!/usr/bin/env python3
"""Survey front-end diagnostics over a cohort or a full Kbuild command inventory.

The small-cohort mode preserves the original interface. Full-configuration
mode reuses the frozen reconnaissance command decoder and argv projection. It
can stop after parsing or continue through EvoIR lowering, runs one bounded
release compiler process per real C compile command, and seals the exact
inputs and outputs used for the aggregate report.
"""

from __future__ import annotations

import argparse
import collections
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
import datetime as dt
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import time
from types import ModuleType
from typing import Any


PROJECT_ROOT = Path(__file__).resolve().parents[1]
RECONNAISSANCE_RUNNER = PROJECT_ROOT / "tests/acceptance/day5-kernel-reconnaissance.py"
DEFAULT_COMMAND_ROOT = PROJECT_ROOT / ".build/reference/gcc-lane-1"
DEFAULT_SOURCE_ROOT = PROJECT_ROOT / ".build/linux-src"
DEFAULT_EVIDENCE_ROOTS = {
    "parse": PROJECT_ROOT / ".evidence/full-config-parse-survey",
    "lower": PROJECT_ROOT / ".evidence/full-config-lowering-survey",
}
DEFAULT_CONTRACT = PROJECT_ROOT / "tests/contracts/day9-acceptance-contract.json"
DIAGNOSTIC = re.compile(
    r"^(?P<path>.+):(?P<line>[0-9]+):(?P<column>[0-9]+): "
    r"error\[(?P<code>[^]]+)\]: (?P<message>.*)$"
)
PANIC_MARKERS = ("panicked at", "panic in a function", "fatal runtime error")
TARGET_COMPILERS = frozenset({"gcc-14"})
HOST_COMPILERS = frozenset({"gcc"})
SOURCE_OPTION_ARGUMENTS = frozenset(
    {
        "-D",
        "-I",
        "-MF",
        "-MQ",
        "-MT",
        "-U",
        "-imacros",
        "-include",
        "-iquote",
        "-isystem",
        "-o",
        "-x",
    }
)
ASSEMBLY_SUFFIXES = frozenset({".s", ".S", ".asm"})
SYNTHETIC_RECORDS = frozenset({".vmlinux.o.cmd", "..vmlinux.export.o.cmd"})


class SurveyError(RuntimeError):
    """The frozen survey input or execution contract is invalid."""


@dataclass(frozen=True)
class CompileUnit:
    """One decoded C compilation selected from the Kbuild inventory."""

    unit_id: str
    record: Path
    record_relative: str
    command: str
    argv: tuple[str, ...]
    source_token: str
    source: Path
    source_logical: str
    source_origin: str
    projected_argv: tuple[str, ...]
    omitted_argv: tuple[str, ...]


def require(condition: bool, message: str) -> None:
    if not condition:
        raise SurveyError(message)


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def write_json(path: Path, value: Any) -> None:
    path.write_bytes(canonical_json(value))


def artifact(path: Path, root: Path) -> dict[str, Any]:
    require(path.is_file(), f"missing evidence artifact: {path}")
    return {
        "path": path.relative_to(root).as_posix(),
        "sha256": sha256_file(path),
        "bytes": path.stat().st_size,
    }


def load_reconnaissance_runner() -> ModuleType:
    spec = importlib.util.spec_from_file_location("kernel_reconnaissance", RECONNAISSANCE_RUNNER)
    require(spec is not None and spec.loader is not None, "cannot load reconnaissance runner")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def diagnostic_rows(stderr: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for raw_line in stderr.splitlines():
        match = DIAGNOSTIC.match(raw_line)
        if match:
            rows.append(
                {
                    "path": match.group("path"),
                    "line": int(match.group("line")),
                    "column": int(match.group("column")),
                    "code": match.group("code"),
                    "message": match.group("message"),
                }
            )
    return rows


def finding_key(diagnostic: dict[str, Any]) -> str:
    return f"{diagnostic['code']} {diagnostic['message'].split(':', 1)[0]}"


def diagnostic_code_family(diagnostic: dict[str, Any]) -> str:
    code = diagnostic["code"]
    if code.startswith("E30"):
        return "E30xx"
    if code.startswith("E29"):
        return "E29xx"
    return "other"


def panic_message(stderr: str) -> str | None:
    lines = stderr.splitlines()
    for index, line in enumerate(lines):
        lowered = line.lower()
        if any(marker in lowered for marker in PANIC_MARKERS):
            excerpt = lines[max(0, index - 1) : min(len(lines), index + 2)]
            return " | ".join(value.strip() for value in excerpt if value.strip())
    return None


def survey_unit(evocc: Path, unit: Path) -> list[tuple[int, str, str]] | None:
    """Retain the original preprocessed-cohort behavior and return shape."""
    out = subprocess.run(
        [str(evocc), "--parse-only", "--survey", str(unit)],
        capture_output=True,
        text=True,
        check=False,
    )
    if panic_message(out.stderr) is not None:
        return None
    return [
        (row["line"], row["code"], row["message"])
        for row in diagnostic_rows(out.stderr)
    ]


def run_cohort_mode(args: argparse.Namespace) -> int:
    units = sorted(args.units.glob(args.glob))
    if not units:
        print(f"no units matched {args.glob} under {args.units}", file=sys.stderr)
        return 2

    at_root: collections.Counter[str] = collections.Counter()
    anywhere: collections.Counter[str] = collections.Counter()
    units_with: collections.defaultdict[str, set[str]] = collections.defaultdict(set)
    roots: dict[str, Any] = {}
    unavailable: list[str] = []
    for unit in units:
        found = survey_unit(args.evocc, unit)
        if found is None:
            unavailable.append(unit.stem)
            roots[unit.stem] = {"unavailable": "recovery incomplete"}
            continue
        if not found:
            roots[unit.stem] = None
            continue
        root_line = found[0][0]
        roots[unit.stem] = {"line": root_line, "code": found[0][1], "message": found[0][2]}
        for line, code, message in found:
            key = f"{code} {message.split(':', 1)[0]}"
            anywhere[key] += 1
            units_with[key].add(unit.stem)
            if line == root_line:
                at_root[key] += 1

    print(f"{len(units)} units, {sum(anywhere.values())} diagnostics")
    print(f"{sum(1 for root in roots.values() if root is None)} units clean")
    if unavailable:
        print(f"{len(unavailable)} units unavailable (recovery incomplete): {', '.join(unavailable)}")
    print()
    print(f"{'at-root':>8} {'total':>8} {'units':>6}  finding")
    for key, total in anywhere.most_common():
        print(f"{at_root[key]:8} {total:8} {len(units_with[key]):6}  {key}")

    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "units": len(units),
                    "diagnostics": sum(anywhere.values()),
                    "unavailable": unavailable,
                    "roots": roots,
                    "findings": [
                        {
                            "finding": key,
                            "atRoot": at_root[key],
                            "total": total,
                            "units": sorted(units_with[key]),
                        }
                        for key, total in anywhere.most_common()
                    ],
                },
                indent=2,
            )
            + "\n"
        )
    return 0


def candidate_source_tokens(argv: list[str]) -> list[str]:
    """Return source-looking operands while excluding option arguments."""
    candidates: list[str] = []
    index = 1
    while index < len(argv):
        value = argv[index]
        if value in SOURCE_OPTION_ARGUMENTS:
            require(index + 1 < len(argv), f"missing command value after {value}")
            index += 2
            continue
        if not value.startswith("-") and Path(value).suffix in ({".c"} | ASSEMBLY_SUFFIXES):
            candidates.append(value)
        index += 1
    return candidates


def resolve_source(
    command_root: Path,
    source_root: Path,
    source_token: str,
) -> tuple[Path, str, str]:
    if source_token.startswith("/work/src/"):
        logical = source_token[len("/work/src/") :]
        return source_root / logical, logical, "kernel-source"
    require(not source_token.startswith("/"), f"unsupported absolute source path: {source_token}")
    logical = source_token.removeprefix("./")
    resolved = (command_root / logical).resolve()
    lane_root = command_root.resolve()
    require(
        resolved == lane_root or lane_root in resolved.parents,
        f"generated source escapes command root: {source_token}",
    )
    return resolved, logical, "gcc-output"


def front_end_projection(
    reconnaissance: ModuleType,
    evocc: Path,
    original: list[str],
    command_root: Path,
    source_token: str,
    source: Path,
    source_logical: str,
    action: str,
) -> tuple[list[str], list[str]]:
    lane = command_root.name
    raw, omitted = reconnaissance.raw_projection(evocc, original, lane, source_logical)
    if not source_token.startswith("/work/src/"):
        require(
            omitted.count(source_token) == 1,
            f"generated source token was not uniquely omitted: {source_token}",
        )
        omitted.remove(source_token)
        raw[-1] = str(source)
    require(raw[1:3] == ["-E", "-P"], "reconnaissance projection shape changed")
    mode = "--parse-only" if action == "parse" else "--emit=evoir"
    return [raw[0], mode, "--survey", *raw[3:]], omitted


def inventory(
    reconnaissance: ModuleType,
    command_root: Path,
    source_root: Path,
    evocc: Path,
    action: str = "parse",
) -> tuple[list[CompileUnit], list[dict[str, Any]]]:
    require(action in DEFAULT_EVIDENCE_ROOTS, f"unknown survey action: {action}")
    records = sorted(command_root.rglob("*.o.cmd"))
    require(bool(records), f"no .o.cmd records found under {command_root}")
    units: list[CompileUnit] = []
    rows: list[dict[str, Any]] = []

    for record in records:
        relative = record.relative_to(command_root).as_posix()
        command, argv = reconnaissance.first_saved_command(record)
        tool = Path(argv[0]).name
        declared = reconnaissance.source_declaration(record)
        candidates = candidate_source_tokens(argv)
        reason: str | None = None
        source_token: str | None = None

        if record.name in SYNTHETIC_RECORDS:
            reason = "vmlinux-synthetic-target"
        elif declared is not None:
            expected = f"/work/src/{declared}"
            require(argv.count(expected) == 1, f"declared source is absent or ambiguous: {relative}")
            source_token = expected
        elif len(candidates) == 1:
            source_token = candidates[0]
        elif not candidates:
            reason = "link-or-binary-transform"
        else:
            raise SurveyError(f"ambiguous compiler source operands in {relative}: {candidates}")

        if reason is None:
            require(source_token is not None, f"internal source classification failure: {relative}")
            suffix = Path(source_token).suffix
            if suffix in ASSEMBLY_SUFFIXES:
                reason = "assembly-source"
            elif suffix == ".c" and tool in HOST_COMPILERS and "-c" in argv:
                reason = "host-compiler-program"
            elif suffix != ".c" or tool not in TARGET_COMPILERS or "-c" not in argv:
                reason = "link-or-binary-transform"

        base: dict[str, Any] = {
            "record": relative,
            "recordSha256": sha256_file(record),
            "commandSha256": sha256_bytes(command.encode()),
            "argvSha256": reconnaissance.argv_sha256(argv),
            "argvCount": len(argv),
            "tool": tool,
        }
        if reason is not None:
            rows.append({**base, "included": False, "exclusionReason": reason})
            continue

        require(source_token is not None, f"missing included source token: {relative}")
        source, source_logical, source_origin = resolve_source(command_root, source_root, source_token)
        require(source.is_file(), f"selected source is absent: {relative}: {source}")
        projected, omitted = front_end_projection(
            reconnaissance,
            evocc,
            argv,
            command_root,
            source_token,
            source,
            source_logical,
            action,
        )
        unit_id = sha256_bytes(relative.encode())[:16]
        unit = CompileUnit(
            unit_id=unit_id,
            record=record,
            record_relative=relative,
            command=command,
            argv=tuple(argv),
            source_token=source_token,
            source=source,
            source_logical=source_logical,
            source_origin=source_origin,
            projected_argv=tuple(projected),
            omitted_argv=tuple(omitted),
        )
        units.append(unit)
        rows.append(
            {
                **base,
                "included": True,
                "unitId": unit_id,
                "sourceToken": source_token,
                "source": source_logical,
                "sourceOrigin": source_origin,
                "sourceSha256": sha256_file(source),
                "sourceBytes": source.stat().st_size,
                "projectedArgvSha256": reconnaissance.argv_sha256(projected),
                "projectedArgvCount": len(projected),
                "omittedArgvSha256": reconnaissance.argv_sha256(omitted),
                "omittedArgvCount": len(omitted),
            }
        )

    require(len({unit.unit_id for unit in units}) == len(units), "unit identifier collision")
    return units, rows


def run_one(
    unit: CompileUnit,
    capture: Path,
    timeout_seconds: int,
    action: str,
) -> dict[str, Any]:
    stdout_path = capture / "stdout" / f"{unit.unit_id}.stdout"
    stderr_path = capture / "stderr" / f"{unit.unit_id}.stderr"
    started = time.monotonic_ns()
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        try:
            completed = subprocess.run(
                list(unit.projected_argv),
                cwd=PROJECT_ROOT,
                stdout=stdout,
                stderr=stderr,
                timeout=timeout_seconds,
                check=False,
            )
            return_code = completed.returncode
            timed_out = False
        except subprocess.TimeoutExpired:
            return_code = 124
            timed_out = True
    elapsed_ms = (time.monotonic_ns() - started) / 1_000_000
    stderr_text = stderr_path.read_text(encoding="utf-8", errors="replace")
    diagnostics = diagnostic_rows(stderr_text)
    panic = panic_message(stderr_text)
    if timed_out:
        classification = "timeout"
    elif panic is not None:
        classification = "panic"
    elif diagnostics:
        classification = "diagnosed"
    elif return_code == 0:
        classification = "clean"
    else:
        classification = "unavailable"
    first = diagnostics[0] if diagnostics else None
    return {
        "unitId": unit.unit_id,
        "record": unit.record_relative,
        "source": unit.source_logical,
        "sourceOrigin": unit.source_origin,
        "action": action,
        "classification": classification,
        "returnCode": return_code,
        "timedOut": timed_out,
        "wallMilliseconds": round(elapsed_ms, 3),
        "diagnosticCount": len(diagnostics),
        "firstDiagnostic": first,
        "firstDiagnosticClass": finding_key(first) if first else None,
        "panicMessage": panic,
        "stdout": artifact(stdout_path, capture),
        "stderr": artifact(stderr_path, capture),
    }


def jsonl(path: Path, rows: list[dict[str, Any]]) -> None:
    with path.open("wb") as output:
        for row in rows:
            output.write(canonical_json(row))


def result_summary(
    rows: list[dict[str, Any]],
    inventory_rows: list[dict[str, Any]],
    jobs: int,
    timeout_seconds: int,
    action: str,
) -> dict[str, Any]:
    categories = collections.Counter(row["classification"] for row in rows)
    for category in ("clean", "diagnosed", "unavailable", "timeout", "panic"):
        categories.setdefault(category, 0)
    exclusions = collections.Counter(
        row["exclusionReason"] for row in inventory_rows if not row["included"]
    )
    tools = collections.Counter(row["tool"] for row in inventory_rows)
    roots: collections.defaultdict[str, list[dict[str, Any]]] = collections.defaultdict(list)
    for row in rows:
        if row["classification"] == "diagnosed":
            roots[row["firstDiagnosticClass"]].append(row)
    ranked = sorted(roots.items(), key=lambda item: (-len(item[1]), item[0]))
    diagnostic_families = collections.Counter(
        diagnostic_code_family(row["firstDiagnostic"])
        for row in rows
        if row["classification"] == "diagnosed"
    )
    top = [
        {
            "finding": finding,
            "units": len(found),
            "exampleUnit": min(row["source"] for row in found),
            "exampleRecord": min(found, key=lambda row: row["source"])["record"],
        }
        for finding, found in ranked[:15]
    ]
    panic_units = [
        {
            "source": row["source"],
            "record": row["record"],
            "panicMessage": row["panicMessage"],
        }
        for row in rows
        if row["classification"] == "panic"
    ]
    unavailable_units = [
        {"source": row["source"], "record": row["record"], "returnCode": row["returnCode"]}
        for row in rows
        if row["classification"] == "unavailable"
    ]
    times = [row["wallMilliseconds"] for row in rows]
    slowest = max(rows, key=lambda row: row["wallMilliseconds"])
    return {
        "schema": f"evocc.full-configuration-{action}-survey.v1",
        "action": action,
        "enumeration": {
            "commandRecords": len(inventory_rows),
            "includedCUnits": len(rows),
            "excluded": len(inventory_rows) - len(rows),
            "exclusionsByReason": dict(sorted(exclusions.items())),
            "recordsByTool": dict(sorted(tools.items())),
        },
        "classificationDefinition": {
            "timeout": "wall-clock limit expired, regardless of partial output",
            "panic": "a Rust/runtime panic marker was captured before normal completion",
            "diagnosed": "at least one typed error diagnostic was emitted",
            "clean": "exit zero with no typed error diagnostic",
            "unavailable": "nonzero exit with no typed diagnostic, timeout, or panic marker",
        },
        "classifications": {
            name: categories[name]
            for name in ("clean", "diagnosed", "unavailable", "timeout", "panic")
        },
        "diagnosedByCodeFamily": {
            name: diagnostic_families[name] for name in ("E30xx", "E29xx", "other")
        },
        "atRootTop15": top,
        "panicUnits": panic_units,
        "unavailableUnits": unavailable_units,
        "timing": {
            "parallelism": jobs,
            "perUnitTimeoutSeconds": timeout_seconds,
            "medianWallMilliseconds": round(statistics.median(times), 3),
            "maximumWallMilliseconds": slowest["wallMilliseconds"],
            "maximumUnit": slowest["source"],
            "note": "Per-process wall time was observed under concurrent host load; it is coarse telemetry, not a benchmark.",
        },
    }


def git_output(*args: str) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=PROJECT_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return completed.stdout.strip()


def validate_authority(contract: Path, action: str) -> None:
    value = json.loads(contract.read_text(encoding="utf-8"))
    scope = value.get("scope", [])
    if action == "parse":
        require(
            any(
                isinstance(item, str) and item.startswith("F3 full-configuration parse survey:")
                for item in scope
            ),
            "contract does not open the full-configuration parse survey",
        )
        ruling = value.get("rulings", {}).get("fullConfigSurveyIsMeasureOnly", "")
        require("measure-only" in ruling, "contract does not carry the measure-only ruling")
        return
    require(
        any(
            isinstance(item, str)
            and item.startswith("Z after closure:")
            and "lowering survey, measure-only" in item
            for item in scope
        ),
        "contract does not open the measure-only full-configuration lowering survey",
    )


def seal_capture(capture: Path) -> dict[str, Any]:
    manifest_path = capture / "capture-files.tsv"
    seal_path = capture / "seal.json"
    excluded = {manifest_path, seal_path}
    files = sorted(path for path in capture.rglob("*") if path.is_file() and path not in excluded)
    with manifest_path.open("w", encoding="utf-8", newline="\n") as output:
        for path in files:
            output.write(
                f"{path.relative_to(capture).as_posix()}\t{sha256_file(path)}\t{path.stat().st_size}\n"
            )
    seal = {
        "schema": "evocc.evidence-seal.v1",
        "files": len(files),
        "bytes": sum(path.stat().st_size for path in files),
        "manifest": artifact(manifest_path, capture),
        "summary": artifact(capture / "summary.json", capture),
        "results": artifact(capture / "results.jsonl", capture),
        "enumeration": artifact(capture / "enumeration.jsonl", capture),
        "argv": artifact(capture / "argv.jsonl", capture),
    }
    write_json(seal_path, seal)
    return {**seal, "seal": artifact(seal_path, capture)}


def run_full_configuration(args: argparse.Namespace) -> int:
    contract = args.contract.resolve()
    evocc = args.evocc.resolve()
    command_root = args.command_record_root.resolve()
    source_root = args.source_root.resolve()
    evidence_root = (args.evidence_root or DEFAULT_EVIDENCE_ROOTS[args.action]).resolve()
    validate_authority(contract, args.action)
    require(evocc.is_file(), f"release compiler is absent: {evocc}")
    require(args.timeout > 0, "timeout must be positive")
    require(args.jobs > 0, "parallelism must be positive")
    require(command_root.is_dir(), f"command root is absent: {command_root}")
    require(source_root.is_dir(), f"source root is absent: {source_root}")
    tracked_status = git_output("status", "--porcelain", "--untracked-files=no")
    require(not tracked_status, "tracked worktree must be clean before a sealed survey")
    subject = git_output("rev-parse", "HEAD")

    run_id = args.run_id or (
        f"{dt.datetime.now(dt.timezone.utc).strftime('%Y%m%dT%H%M%SZ')}-{os.getpid()}"
    )
    capture = evidence_root / run_id
    require(not capture.exists(), f"capture already exists: {capture}")
    capture.mkdir(parents=True)
    (capture / "stdout").mkdir()
    (capture / "stderr").mkdir()
    shutil.copy2(Path(__file__), capture / "survey-runner.py")
    shutil.copy2(RECONNAISSANCE_RUNNER, capture / "reconnaissance-runner.py")

    reconnaissance = load_reconnaissance_runner()
    units, inventory_rows = inventory(
        reconnaissance,
        command_root,
        source_root,
        evocc,
        args.action,
    )
    require(len(inventory_rows) == 2944, f"expected 2944 command records, found {len(inventory_rows)}")
    jsonl(capture / "enumeration.jsonl", inventory_rows)
    jsonl(
        capture / "argv.jsonl",
        [
            {
                "unitId": unit.unit_id,
                "record": unit.record_relative,
                "source": unit.source_logical,
                "originalArgv": list(unit.argv),
                "projectedArgv": list(unit.projected_argv),
                "omittedArgv": list(unit.omitted_argv),
            }
            for unit in units
        ],
    )
    config = {
        "schema": f"evocc.full-configuration-{args.action}-survey.config.v1",
        "runId": run_id,
        "action": args.action,
        "subjectCommit": subject,
        "contract": {
            "path": contract.relative_to(PROJECT_ROOT).as_posix(),
            "sha256": sha256_file(contract),
        },
        "releaseBinary": {
            "path": evocc.relative_to(PROJECT_ROOT).as_posix(),
            "sha256": sha256_file(evocc),
            "bytes": evocc.stat().st_size,
        },
        "commandRecordRoot": command_root.relative_to(PROJECT_ROOT).as_posix(),
        "sourceRoot": source_root.relative_to(PROJECT_ROOT).as_posix(),
        "commandRecords": len(inventory_rows),
        "includedCUnits": len(units),
        "parallelism": args.jobs,
        "perUnitTimeoutSeconds": args.timeout,
        "projectionAuthority": {
            "path": RECONNAISSANCE_RUNNER.relative_to(PROJECT_ROOT).as_posix(),
            "sha256": sha256_file(RECONNAISSANCE_RUNNER),
            "functions": [
                "first_saved_command",
                "source_declaration",
                "raw_projection",
                "argv_sha256",
            ],
        },
        "runnerSha256": sha256_file(Path(__file__)),
    }
    write_json(capture / "run-config.json", config)

    results: list[dict[str, Any]] = []
    completed_count = 0
    with ThreadPoolExecutor(max_workers=args.jobs) as executor:
        futures = {
            executor.submit(run_one, unit, capture, args.timeout, args.action): unit
            for unit in units
        }
        for future in as_completed(futures):
            unit = futures[future]
            try:
                results.append(future.result())
            except Exception as error:
                raise SurveyError(f"survey runner failed for {unit.record_relative}: {error}") from error
            completed_count += 1
            if completed_count % 100 == 0 or completed_count == len(units):
                print(f"surveyed {completed_count}/{len(units)} units", flush=True)
    results.sort(key=lambda row: row["record"])
    jsonl(capture / "results.jsonl", results)
    summary = result_summary(results, inventory_rows, args.jobs, args.timeout, args.action)
    summary["runId"] = run_id
    summary["subjectCommit"] = subject
    summary["capture"] = capture.relative_to(PROJECT_ROOT).as_posix()
    write_json(capture / "summary.json", summary)

    require(
        git_output("status", "--porcelain", "--untracked-files=no") == "",
        "tracked worktree changed during the survey",
    )
    require(git_output("rev-parse", "HEAD") == subject, "HEAD changed during the survey")
    require(sha256_file(evocc) == config["releaseBinary"]["sha256"], "release binary changed")
    require(sha256_file(contract) == config["contract"]["sha256"], "contract changed")
    require(sha256_file(Path(__file__)) == config["runnerSha256"], "survey runner changed")
    require(
        sha256_file(RECONNAISSANCE_RUNNER) == config["projectionAuthority"]["sha256"],
        "reconnaissance runner changed",
    )
    sealed = seal_capture(capture)
    print(json.dumps({**summary, "seal": sealed["seal"]}, indent=2))
    return 0


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evocc", type=Path, default=Path("target/release/evocc"))
    modes = parser.add_mutually_exclusive_group(required=True)
    modes.add_argument("--units", type=Path, help="directory of preprocessed cohort units")
    modes.add_argument(
        "--command-record-root",
        type=Path,
        help="Kbuild output root containing the full .o.cmd inventory",
    )
    parser.add_argument("--glob", default="*.gcc.c")
    parser.add_argument("--json", type=Path)
    parser.add_argument(
        "--action",
        choices=tuple(DEFAULT_EVIDENCE_ROOTS),
        default="parse",
        help="stop after parsing or continue through EvoIR lowering",
    )
    parser.add_argument("--source-root", type=Path, default=DEFAULT_SOURCE_ROOT)
    parser.add_argument("--evidence-root", type=Path)
    parser.add_argument("--contract", type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("--run-id")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.units is not None:
            require(args.action == "parse", "cohort mode supports only the parse action")
            return run_cohort_mode(args)
        return run_full_configuration(args)
    except (OSError, subprocess.CalledProcessError, SurveyError, json.JSONDecodeError) as error:
        print(f"survey error: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
