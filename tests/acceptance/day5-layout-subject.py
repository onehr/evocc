#!/usr/bin/env python3
"""Feed the frozen Day-5 sources and generic observation requests to EvoCC."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class SubjectCase:
    case_id: str
    type_name: str
    source: str
    offset_paths: tuple[str, ...]
    assignments: tuple[str, ...]


def encode(value: str) -> str:
    return value.encode("utf-8").hex()


def encode_list(values: Sequence[str]) -> str:
    return ",".join(encode(value) for value in values) or "-"


def load_cases(path: Path) -> list[SubjectCase]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    cases = manifest.get("cases")
    if not isinstance(cases, list) or manifest.get("caseCount") != len(cases):
        raise ValueError("manifest caseCount does not match cases")
    required = {
        "id",
        "typeName",
        "source",
        "sourceSha256",
        "offsets",
        "storageAssignments",
    }
    subject_cases = []
    for ordinal, case in enumerate(cases, 1):
        if not isinstance(case, dict) or not required.issubset(case):
            raise ValueError(f"case {ordinal} lacks subject input fields")
        source = case["source"]
        if not isinstance(source, str):
            raise ValueError(f"case {ordinal}: source is not a string")
        if hashlib.sha256(source.encode("utf-8")).hexdigest() != case["sourceSha256"]:
            raise ValueError(f"{case['id']}: source hash mismatch")
        if case.get("ordinal") != ordinal:
            raise ValueError(f"{case['id']}: non-sequential ordinal")
        if not all(
            isinstance(offset, dict) and isinstance(offset.get("label"), str)
            for offset in case["offsets"]
        ):
            raise ValueError(f"{case['id']}: malformed offset request")
        if not all(isinstance(item, str) for item in case["storageAssignments"]):
            raise ValueError(f"{case['id']}: malformed storage assignment")
        if not isinstance(case["id"], str) or not isinstance(case["typeName"], str):
            raise ValueError(f"case {ordinal}: ID or typedef name is not a string")
        subject_cases.append(
            SubjectCase(
                case_id=case["id"],
                type_name=case["typeName"],
                source=source,
                offset_paths=tuple(offset["label"] for offset in case["offsets"]),
                assignments=tuple(case["storageAssignments"]),
            )
        )
    return subject_cases


def protocol(cases: list[SubjectCase]) -> bytes:
    records = []
    for case in cases:
        records.append(
            "\t".join(
                [
                    case.case_id,
                    encode(case.source),
                    encode(case.type_name),
                    encode_list(case.offset_paths),
                    encode_list(case.assignments),
                ]
            )
        )
    return ("\n".join(records) + "\n").encode("ascii")


def observe(probe: Path, cases: list[SubjectCase]) -> bytes:
    completed = subprocess.run(
        [str(probe)],
        input=protocol(cases),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=60,
    )
    stderr = completed.stderr.decode("utf-8", errors="replace")
    if completed.returncode != 0:
        raise ValueError(f"probe exited {completed.returncode}: {stderr.strip()}")
    if b"\r" in completed.stdout:
        raise ValueError("probe emitted forbidden CR bytes")
    try:
        lines = completed.stdout.decode("ascii").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("probe output is not ASCII") from error
    if len(lines) != len(cases):
        raise ValueError(f"probe emitted {len(lines)} lines for {len(cases)} cases")
    errors = []
    for case, line in zip(cases, lines):
        fields = line.split("\t")
        if fields[:2] == ["ERR", case.case_id] and len(fields) == 3:
            try:
                message = bytes.fromhex(fields[2]).decode("utf-8")
            except (ValueError, UnicodeDecodeError):
                message = "malformed probe error"
            errors.append(f"{case.case_id}: {message}")
            continue
        if len(fields) != 5 or fields[0] != case.case_id:
            errors.append(f"{case.case_id}: malformed or out-of-order observation")
    if errors:
        preview = "\n".join(errors[:16])
        suffix = "" if len(errors) <= 16 else f"\n... and {len(errors) - 16} more"
        raise ValueError(f"subject observation failed:\n{preview}{suffix}")
    if stderr:
        raise ValueError(f"successful probe wrote stderr: {stderr.strip()}")
    return completed.stdout


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest", type=Path)
    parser.add_argument("probe", type=Path)
    args = parser.parse_args()
    try:
        if not args.manifest.is_file() or args.manifest.is_symlink():
            raise ValueError("manifest must be a regular non-symbolic file")
        if not args.probe.is_file() or args.probe.is_symlink():
            raise ValueError("probe must be a regular non-symbolic file")
        sys.stdout.buffer.write(observe(args.probe, load_cases(args.manifest)))
    except (OSError, ValueError, json.JSONDecodeError, subprocess.TimeoutExpired) as error:
        print(f"day5-layout-subject: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
