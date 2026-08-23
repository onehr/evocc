#!/usr/bin/python3
"""Materialize and compare the frozen Day-7 defined-memory oracle lanes."""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import importlib.util
import json
import pathlib
import sys
from typing import Sequence


SCRIPT_DIR = pathlib.Path(__file__).resolve().parent
CORPUS_PATH = SCRIPT_DIR / "day7-memory-corpus.py"
DEFINED_COUNT = 128
MANIFEST_FIELDS = ("ordinal", "filename", "case_id", "source_sha256")


def _load_corpus():
    spec = importlib.util.spec_from_file_location("c30_day7_memory_corpus", CORPUS_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("cannot load Day-7 memory corpus")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CORPUS = _load_corpus()


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_cases():
    audit = CORPUS.audit_corpus()
    cases = CORPUS.build_defined_cases()
    if audit["defined"] != DEFINED_COUNT or len(cases) != DEFINED_COUNT:
        raise AssertionError("Day-7 defined denominator changed")
    return cases


def expected_manifest_rows() -> list[tuple[int, str, str, str]]:
    return [
        (
            ordinal,
            f"{ordinal:03}-{case.category}.c",
            case.case_id,
            _sha256(case.source.encode("utf-8")),
        )
        for ordinal, case in enumerate(canonical_cases())
    ]


def _parse_manifest(path: pathlib.Path) -> list[tuple[int, str, str, str]]:
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != len(MANIFEST_FIELDS):
            raise AssertionError(
                f"{path}:{line_number}: expected {len(MANIFEST_FIELDS)} non-golden fields"
            )
        rows.append((int(fields[0]), fields[1], fields[2], fields[3]))
    return rows


def materialize(destination_name: str) -> dict[str, object]:
    destination = pathlib.Path(destination_name)
    result = CORPUS.materialize_defined(destination)
    return {
        **result,
        "scope": "definedCasesOnly",
        "laneInputFields": list(MANIFEST_FIELDS),
        "directGoldenFields": 0,
        "invalidMaterialized": 0,
    }


def audit_lane_input(destination_name: str) -> dict[str, object]:
    destination = pathlib.Path(destination_name)
    if not destination.is_dir() or destination.is_symlink():
        raise AssertionError("lane input is missing, non-directory, or symbolic")
    manifest = destination / "manifest.tsv"
    expected_rows = expected_manifest_rows()
    if _parse_manifest(manifest) != expected_rows:
        raise AssertionError("lane input manifest differs from corpus authority")
    expected_files = {pathlib.Path("manifest.tsv")}
    expected_files.update(pathlib.Path(row[1]) for row in expected_rows)
    actual_files: set[pathlib.Path] = set()
    for path in destination.rglob("*"):
        if path.is_symlink():
            raise AssertionError(f"lane input contains symbolic path: {path}")
        if path.is_file():
            actual_files.add(path.relative_to(destination))
    if actual_files != expected_files:
        raise AssertionError("lane input file set differs from corpus authority")
    for row, case in zip(expected_rows, canonical_cases()):
        if (destination / row[1]).read_bytes() != case.source.encode("utf-8"):
            raise AssertionError(f"source bytes differ: {case.case_id}")
    return {
        "status": "PASS",
        "scope": "definedCasesOnly",
        "mountedInputFiles": len(actual_files),
        "unexpectedMountedFiles": 0,
        "laneInputFields": list(MANIFEST_FIELDS),
        "directGoldenFields": 0,
        "invalidMaterialized": 0,
        "manifestSha256": _sha256(manifest.read_bytes()),
    }


@dataclasses.dataclass(frozen=True)
class Observation:
    ordinal: int
    filename: str
    case_id: str
    source_sha256: str
    compile_rc: int
    run_rc: int | None
    stdout_bytes: int | None
    stderr_bytes: int | None

    def row(self) -> str:
        return "\t".join(
            str(value) if value is not None else "NA"
            for value in dataclasses.astuple(self)
        )


def _optional_int(value: str) -> int | None:
    return None if value == "NA" else int(value)


def read_observations(path_name: str) -> list[Observation]:
    path = pathlib.Path(path_name)
    if not path.is_file() or path.is_symlink():
        raise AssertionError(f"observation is missing or symbolic: {path}")
    rows = []
    for line_number, line in enumerate(path.read_text(encoding="ascii").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 8:
            raise AssertionError(f"{path}:{line_number}: expected 8 TSV fields")
        rows.append(
            Observation(
                ordinal=int(fields[0]),
                filename=fields[1],
                case_id=fields[2],
                source_sha256=fields[3],
                compile_rc=int(fields[4]),
                run_rc=_optional_int(fields[5]),
                stdout_bytes=_optional_int(fields[6]),
                stderr_bytes=_optional_int(fields[7]),
            )
        )
    return rows


def expected_observations() -> list[Observation]:
    return [
        Observation(*row, compile_rc=0, run_rc=case.value, stdout_bytes=0, stderr_bytes=0)
        for row, case in zip(expected_manifest_rows(), canonical_cases())
    ]


def mismatches(observed: Sequence[Observation]) -> list[str]:
    expected = expected_observations()
    failures = []
    if len(observed) != len(expected):
        failures.append(f"denominator expected={len(expected)} observed={len(observed)}")
    for ordinal, canonical in enumerate(expected):
        if ordinal >= len(observed):
            failures.append(f"{canonical.case_id}: missing")
        elif observed[ordinal] != canonical:
            fields = [
                field.name
                for field in dataclasses.fields(Observation)
                if getattr(observed[ordinal], field.name) != getattr(canonical, field.name)
            ]
            failures.append(f"{canonical.case_id}: {','.join(fields)}")
    failures.extend(f"unexpected:{row.case_id}" for row in observed[len(expected) :])
    return failures


def verify(gcc_name: str, clang_name: str, consensus_name: str) -> tuple[int, dict[str, object]]:
    gcc = read_observations(gcc_name)
    clang = read_observations(clang_name)
    expected = expected_observations()
    conflicts = [
        expected[index].case_id if index < len(expected) else f"ordinal-{index}"
        for index, (gcc_row, clang_row) in enumerate(zip(gcc, clang))
        if gcc_row != clang_row
    ]
    gcc_failures = mismatches(gcc)
    clang_failures = mismatches(clang)
    failures = [*(f"gcc:{value}" for value in gcc_failures), *(f"clang:{value}" for value in clang_failures)]
    failures.extend(f"oracle-conflict:{case_id}" for case_id in conflicts)
    if conflicts:
        status = "ORACLE_CONFLICT"
    elif failures:
        status = "FAIL"
    else:
        status = "ORACLE_CONSENSUS"
    summary: dict[str, object] = {
        "status": status,
        "required": DEFINED_COUNT,
        "gccAccepted": sum(a == b for a, b in zip(gcc, expected)),
        "clangAccepted": sum(a == b for a, b in zip(clang, expected)),
        "consensus": DEFINED_COUNT if status == "ORACLE_CONSENSUS" else 0,
        "oracleConflictCount": len(conflicts),
        "invalidNativeExecution": "PROHIBITED",
        "invalidExecuted": 0,
        "mismatchCount": len(failures),
        "mismatchSample": failures[:32],
    }
    if failures:
        return 1, summary
    consensus = pathlib.Path(consensus_name)
    if consensus.exists() or consensus.is_symlink():
        raise AssertionError("consensus output already exists")
    consensus.write_text(
        "".join(
            f"{row.ordinal}\t{row.case_id}\t{row.source_sha256}\t{row.run_rc}\n"
            for row in gcc
        ),
        encoding="ascii",
    )
    summary["consensusSha256"] = _sha256(consensus.read_bytes())
    return 0, summary


def self_test() -> dict[str, object]:
    baseline = expected_observations()
    if mismatches(baseline):
        raise AssertionError("comparator rejects canonical baseline")
    mutants = {
        "result": (0, {"run_rc": (baseline[0].run_rc or 0) + 1}),
        "compile": (1, {"compile_rc": 1, "run_rc": None}),
        "source": (2, {"source_sha256": "0" * 64}),
    }
    for name, (index, changes) in mutants.items():
        rows = list(baseline)
        rows[index] = dataclasses.replace(rows[index], **changes)
        if not mismatches(rows):
            raise AssertionError(f"comparator accepted {name} mutant")
    if not mismatches(baseline[:-1]):
        raise AssertionError("comparator accepted denominator mutant")
    return {
        "status": "PASS",
        "mutantsRejected": [*mutants, "denominator", "oracle-conflict"],
        "laneInputFields": list(MANIFEST_FIELDS),
        "directGoldenFields": 0,
        "invalidMaterializationPath": "ABSENT",
    }


def _parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("self-test")
    materialize = subparsers.add_parser("materialize-defined")
    materialize.add_argument("destination")
    audit = subparsers.add_parser("audit-lane-input")
    audit.add_argument("destination")
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("gcc")
    verify_parser.add_argument("clang")
    verify_parser.add_argument("consensus")
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = _parse_args(argv)
    if args.command == "self-test":
        result = self_test()
        rc = 0
    elif args.command == "materialize-defined":
        result = materialize(args.destination)
        rc = 0
    elif args.command == "audit-lane-input":
        result = audit_lane_input(args.destination)
        rc = 0
    elif args.command == "verify":
        rc, result = verify(args.gcc, args.clang, args.consensus)
    else:
        raise AssertionError(args.command)
    print(json.dumps(result, indent=2, sort_keys=True))
    return rc


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
