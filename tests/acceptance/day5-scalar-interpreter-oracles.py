#!/usr/bin/env python3
"""Materialize and verify the defined-only Day-5 native oracle cohort.

This helper deliberately has no command that materializes an EXPECTED_REJECT
program.  Those cases exercise C undefined behavior and are never native
compiler oracles.
"""

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
CORPUS_PATH = SCRIPT_DIR / "day5-scalar-interpreter.py"
CONTRACT_PATH = SCRIPT_DIR.parent / "contracts/day5-scalar-interpreter.json"
DEFINED_COUNT = 256
LANE_INPUT_FIELDS = ("ordinal", "filename", "case_id", "source_sha256")


def load_corpus():
    spec = importlib.util.spec_from_file_location(
        "c30_day5_scalar_interpreter_corpus", CORPUS_PATH
    )
    if spec is None or spec.loader is None:
        raise AssertionError("cannot load frozen scalar-interpreter corpus")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


CORPUS = load_corpus()


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def canonical_cases():
    CORPUS.audit_contract()
    cases = CORPUS.build_defined_cases()
    if len(cases) != DEFINED_COUNT:
        raise AssertionError(f"defined denominator is {len(cases)}, not {DEFINED_COUNT}")
    return cases


def assert_defined_only(cases) -> None:
    reject_hashes = {
        CORPUS.sha256_text(case.source) for case in CORPUS.build_reject_cases()
    }
    fuel_source, _ = CORPUS.fuel_control()
    prohibited_hashes = reject_hashes | {CORPUS.sha256_text(fuel_source)}
    actual_hashes = {CORPUS.sha256_text(case.source) for case in cases}
    overlap = sorted(actual_hashes & prohibited_hashes)
    if overlap:
        raise AssertionError(f"native oracle cohort contains prohibited source: {overlap}")


def lane_input_rows(cases=None) -> list[tuple[int, str, str, str]]:
    if cases is None:
        cases = canonical_cases()
    return [
        (
            case.ordinal,
            f"{case.ordinal:04d}.c",
            case.case_id,
            CORPUS.sha256_text(case.source),
        )
        for case in cases
    ]


def encode_lane_manifest(rows: Sequence[tuple[int, str, str, str]]) -> bytes:
    return (
        "\n".join("\t".join(str(value) for value in row) for row in rows) + "\n"
    ).encode("ascii")


def parse_lane_manifest_text(value: str, source_name: str) -> list[tuple[int, str, str, str]]:
    rows: list[tuple[int, str, str, str]] = []
    for line_number, line in enumerate(value.splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != len(LANE_INPUT_FIELDS):
            raise AssertionError(
                f"{source_name}:{line_number}: lane input must have exactly "
                f"{len(LANE_INPUT_FIELDS)} non-golden fields"
            )
        rows.append((int(fields[0]), fields[1], fields[2], fields[3]))
    return rows


def direct_goldens_from_mounted_metadata(value: str) -> list[int]:
    """Model the old answer-leak attack against manifest-only metadata."""

    fabricated: list[int] = []
    for line in value.splitlines():
        fields = line.split("\t")
        if len(fields) >= 5:
            try:
                fabricated.append(int(fields[4]))
            except ValueError:
                pass
    return fabricated


def materialize(destination_name: str) -> dict[str, object]:
    cases = canonical_cases()
    assert_defined_only(cases)
    destination = pathlib.Path(destination_name)
    destination.mkdir(mode=0o755, parents=False, exist_ok=False)
    cases_directory = destination / "defined-cases"
    cases_directory.mkdir(mode=0o755)
    manifest_path = destination / "manifest.tsv"

    for case in cases:
        filename = f"{case.ordinal:04d}.c"
        encoded = case.source.encode("ascii")
        digest = sha256_bytes(encoded)
        if digest != case.manifest()["sourceSha256"]:
            raise AssertionError(f"source hash drift: {case.case_id}")
        (cases_directory / filename).write_bytes(encoded)
    manifest_bytes = encode_lane_manifest(lane_input_rows(cases))
    manifest_path.write_bytes(manifest_bytes)
    return {
        "status": "PASS",
        "scope": "definedCasesOnly",
        "materialized": len(cases),
        "expectedRejectMaterialized": 0,
        "laneInputFields": list(LANE_INPUT_FIELDS),
        "directGoldenFields": 0,
        "fabricatedGoldenObservationsFromMountedMetadata": 0,
        "manifestSha256": sha256_bytes(manifest_bytes),
    }


def audit_lane_input(destination_name: str) -> dict[str, object]:
    cases = canonical_cases()
    assert_defined_only(cases)
    destination = pathlib.Path(destination_name)
    if not destination.is_dir() or destination.is_symlink():
        raise AssertionError(f"lane input is missing, non-directory, or symbolic: {destination}")
    expected_files = {pathlib.Path("manifest.tsv")}
    expected_files.update(
        pathlib.Path("defined-cases") / f"{case.ordinal:04d}.c" for case in cases
    )
    actual_files: set[pathlib.Path] = set()
    for path in destination.rglob("*"):
        if path.is_symlink():
            raise AssertionError(f"lane input contains symbolic path: {path}")
        if path.is_file():
            actual_files.add(path.relative_to(destination))
    if actual_files != expected_files:
        missing = sorted(str(path) for path in expected_files - actual_files)
        unexpected = sorted(str(path) for path in actual_files - expected_files)
        raise AssertionError(f"lane input file set differs: missing={missing}, unexpected={unexpected}")

    manifest_path = destination / "manifest.tsv"
    manifest_text = manifest_path.read_text(encoding="ascii")
    observed_rows = parse_lane_manifest_text(manifest_text, str(manifest_path))
    expected_rows = lane_input_rows(cases)
    if observed_rows != expected_rows:
        raise AssertionError("lane input manifest differs from the defined-only model")
    if direct_goldens_from_mounted_metadata(manifest_text):
        raise AssertionError("lane input manifest exposes directly reusable golden values")
    for case in cases:
        path = destination / "defined-cases" / f"{case.ordinal:04d}.c"
        if path.read_bytes() != case.source.encode("ascii"):
            raise AssertionError(f"mounted source differs from model: {case.case_id}")
    return {
        "status": "PASS",
        "scope": "definedCasesOnly",
        "mountedInputFiles": len(actual_files),
        "unexpectedMountedFiles": 0,
        "laneInputFields": list(LANE_INPUT_FIELDS),
        "directGoldenFields": 0,
        "fabricatedGoldenObservationsFromMountedMetadata": 0,
        "expectedRejectMaterialized": 0,
        "manifestSha256": sha256_bytes(manifest_text.encode("ascii")),
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
        values = (
            self.ordinal,
            self.filename,
            self.case_id,
            self.source_sha256,
            self.compile_rc,
            "NA" if self.run_rc is None else self.run_rc,
            "NA" if self.stdout_bytes is None else self.stdout_bytes,
            "NA" if self.stderr_bytes is None else self.stderr_bytes,
        )
        return "\t".join(str(value) for value in values)


def parse_optional_int(value: str) -> int | None:
    return None if value == "NA" else int(value)


def read_observations(path_name: str) -> list[Observation]:
    path = pathlib.Path(path_name)
    if not path.is_file() or path.is_symlink():
        raise AssertionError(f"observation is missing, non-regular, or symbolic: {path}")
    observations: list[Observation] = []
    for line_number, line in enumerate(path.read_text(encoding="ascii").splitlines(), 1):
        fields = line.split("\t")
        if len(fields) != 8:
            raise AssertionError(f"{path}:{line_number}: expected 8 TSV fields")
        observations.append(
            Observation(
                ordinal=int(fields[0]),
                filename=fields[1],
                case_id=fields[2],
                source_sha256=fields[3],
                compile_rc=int(fields[4]),
                run_rc=parse_optional_int(fields[5]),
                stdout_bytes=parse_optional_int(fields[6]),
                stderr_bytes=parse_optional_int(fields[7]),
            )
        )
    return observations


def expected_observations() -> list[Observation]:
    cases = canonical_cases()
    assert_defined_only(cases)
    return [
        Observation(
            ordinal=case.ordinal,
            filename=f"{case.ordinal:04d}.c",
            case_id=case.case_id,
            source_sha256=CORPUS.sha256_text(case.source),
            compile_rc=0,
            run_rc=case.value,
            stdout_bytes=0,
            stderr_bytes=0,
        )
        for case in cases
    ]


def mismatches(observed: Sequence[Observation]) -> list[str]:
    expected = expected_observations()
    failures: list[str] = []
    if len(observed) != len(expected):
        failures.append(f"denominator: expected {len(expected)}, observed {len(observed)}")
    for index, expected_case in enumerate(expected):
        if index >= len(observed):
            failures.append(f"{expected_case.case_id}: missing observation")
            continue
        actual = observed[index]
        if actual != expected_case:
            fields = [
                field.name
                for field in dataclasses.fields(Observation)
                if getattr(actual, field.name) != getattr(expected_case, field.name)
            ]
            failures.append(f"{expected_case.case_id}: differing fields {','.join(fields)}")
    for actual in observed[len(expected) :]:
        failures.append(f"unexpected observation: {actual.case_id}")
    return failures


def verify(gcc_name: str, clang_name: str, consensus_name: str) -> tuple[int, dict[str, object]]:
    gcc = read_observations(gcc_name)
    clang = read_observations(clang_name)
    expected = expected_observations()
    gcc_mismatches = mismatches(gcc)
    clang_mismatches = mismatches(clang)
    failures = [f"gcc: {value}" for value in gcc_mismatches]
    failures.extend(f"clang: {value}" for value in clang_mismatches)
    for index, (gcc_case, clang_case) in enumerate(zip(gcc, clang)):
        if gcc_case != clang_case:
            case_id = expected[index].case_id if index < len(expected) else gcc_case.case_id
            failures.append(f"oracle-conflict: {case_id}")

    gcc_accepted = sum(
        actual == canonical for actual, canonical in zip(gcc, expected)
    )
    clang_accepted = sum(
        actual == canonical for actual, canonical in zip(clang, expected)
    )

    summary: dict[str, object] = {
        "status": "ORACLE_CONSENSUS" if not failures else "FAIL",
        "scope": "definedCasesOnly",
        "required": DEFINED_COUNT,
        "gccAccepted": gcc_accepted,
        "clangAccepted": clang_accepted,
        "consensus": DEFINED_COUNT if not failures else 0,
        "expectedRejectExecuted": 0,
        "mismatchCount": len(failures),
        "mismatchSample": failures[:32],
    }
    if failures:
        return 1, summary

    consensus = pathlib.Path(consensus_name)
    if consensus.exists() or consensus.is_symlink():
        raise AssertionError(f"consensus output already exists: {consensus}")
    rows = [
        "\t".join(
            (
                str(value.ordinal),
                value.case_id,
                value.source_sha256,
                str(value.run_rc),
            )
        )
        for value in gcc
    ]
    consensus.write_text("\n".join(rows) + "\n", encoding="ascii")
    summary["consensusSha256"] = sha256_bytes(consensus.read_bytes())
    return 0, summary


def self_test() -> dict[str, object]:
    baseline = expected_observations()
    if mismatches(baseline):
        raise AssertionError("oracle comparator rejects its canonical baseline")
    mutants: list[tuple[str, list[Observation]]] = []

    def mutate(name: str, index: int, **changes: object) -> None:
        values = list(baseline)
        values[index] = dataclasses.replace(values[index], **changes)
        mutants.append((name, values))

    mutate("native-result", 0, run_rc=8)
    mutate("compile-failure", 1, compile_rc=1, run_rc=None)
    mutate("source-hash", 2, source_sha256="0" * 64)
    reject = CORPUS.build_reject_cases()[0]
    mutate(
        "expected-reject-substitution",
        3,
        case_id=reject.case_id,
        source_sha256=CORPUS.sha256_text(reject.source),
    )
    mutants.append(("denominator", list(baseline[:-1])))
    for name, values in mutants:
        if not mismatches(values):
            raise AssertionError(f"oracle comparator accepted {name} mutant")

    cases = canonical_cases()
    safe_manifest = encode_lane_manifest(lane_input_rows(cases)).decode("ascii")
    parsed_safe = parse_lane_manifest_text(safe_manifest, "safe-manifest")
    if parsed_safe != lane_input_rows(cases):
        raise AssertionError("lane input parser rejects its non-golden baseline")
    if direct_goldens_from_mounted_metadata(safe_manifest):
        raise AssertionError("non-golden lane input can fabricate golden observations")
    leaked_manifest = "\n".join(
        f"{line}\t{case.value}"
        for line, case in zip(safe_manifest.splitlines(), cases)
    ) + "\n"
    leaked_rejected = False
    try:
        parse_lane_manifest_text(leaked_manifest, "leaked-manifest")
    except AssertionError:
        leaked_rejected = True
    if not leaked_rejected:
        raise AssertionError("lane input parser accepted the old leaked-golden column")
    leaked_goldens = direct_goldens_from_mounted_metadata(leaked_manifest)
    if leaked_goldens != [case.value for case in cases]:
        raise AssertionError("answer-leak attack probe did not recover the injected goldens")
    return {
        "status": "PASS",
        "mutantsRejected": [name for name, _ in mutants] + ["leaked-golden-column"],
        "expectedRejectMaterializationPath": "ABSENT",
        "laneInputFields": list(LANE_INPUT_FIELDS),
        "directGoldenFields": 0,
        "fabricatedGoldenObservationsFromMountedMetadata": 0,
        "answerLeakAttackRecoveredFromMutant": len(leaked_goldens),
    }


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("self-test")
    materialize_parser = subparsers.add_parser("materialize-defined")
    materialize_parser.add_argument("destination")
    audit_input_parser = subparsers.add_parser("audit-lane-input")
    audit_input_parser.add_argument("destination")
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("gcc")
    verify_parser.add_argument("clang")
    verify_parser.add_argument("consensus")
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    if args.command == "self-test":
        print(json.dumps(self_test(), indent=2, sort_keys=True))
        return 0
    if args.command == "materialize-defined":
        print(json.dumps(materialize(args.destination), indent=2, sort_keys=True))
        return 0
    if args.command == "audit-lane-input":
        print(json.dumps(audit_lane_input(args.destination), indent=2, sort_keys=True))
        return 0
    if args.command == "verify":
        rc, summary = verify(args.gcc, args.clang, args.consensus)
        print(json.dumps(summary, indent=2, sort_keys=True))
        return rc
    raise AssertionError(args.command)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
