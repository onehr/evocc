#!/usr/bin/env python3
"""RED-first external acceptance test for three real Day-5 reduction pilots.

This test deliberately stays RED until the production reducer exposes the
``verify-pilots`` and ``replay-pilot-receipt`` commands.  It uses three exact
obligations from the immutable reconnaissance run; synthetic source failures,
oracle assertions supplied by the overlay, and state-string trust are not
accepted as substitutes for execution evidence.

The pilot result is intentionally narrower than the Day-5 verdict.  Exactly
three receipt-derived records may become ``VERIFIED`` while the global result
must remain ``INCOMPLETE`` with ``acceptanceValid=false``.
"""

from __future__ import annotations

import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tempfile
from typing import Any, Callable


ROOT = Path(__file__).resolve().parents[2]
POLICY = ROOT / "tests/contracts/day5-kernel-reduction-policy.json"
PILOT_CONTRACT = ROOT / "tests/contracts/day5-kernel-reduction-pilots.json"
VERIFIER = ROOT / "tests/acceptance/day5-kernel-reductions.py"
RUN_ID = "20260817T225959Z-57627"
EVIDENCE = ROOT / ".evidence/day5-kernel-reconnaissance" / RUN_ID
MANIFEST_SHA256 = "39af1ba905884d739789c996ede7b24ba08c07d3be39b77a643363334d9ec2a1"
FAILURE_PREFIX = "day5-kernel-reductions: FAIL: "

RAW_FAILURE_ID = "ccbede45994dd90ca75222cc689f1e1aad41ba42f5c009ca27710e10a860c796"
E1001_FAILURE_ID = "869b322a04168ffbbac452763bf0081b20c3b69bdea7ffd6fc27bb65c5ff4b5f"
E1002_FAILURE_ID = "4835fdc46c9b283c1ce2bc1f5070c92874e2c7c506699555e8797689a83faaff"
PILOT_FAILURE_IDS = sorted([RAW_FAILURE_ID, E1001_FAILURE_ID, E1002_FAILURE_ID])

PILOT_FACTS = {
    RAW_FAILURE_ID: {
        "code": "E0104",
        "diagnosticSourcePath": "include/linux/compiler_types.h",
        "emittedColumn": 1,
        "emittedLine": 189,
        "kind": "raw-closure",
        "lane": "evocpp-raw",
        "sourceColumn": 1,
        "sourceLine": 189,
        "sourcePath": "lib/bitrev.c",
        "tuId": "lib-bitrev",
    },
    E1001_FAILURE_ID: {
        "code": "E1001",
        "diagnosticSourcePath": "include/linux/compiler.h",
        "emittedColumn": 15,
        "emittedLine": 47,
        "kind": "preprocessed-parser",
        "lane": "gcc-preprocessed-parser",
        "sourceColumn": None,
        "sourceLine": 265,
        "sourcePath": "init/do_mounts.c",
        "tuId": "init-do-mounts",
    },
    E1002_FAILURE_ID: {
        "code": "E1002",
        "diagnosticSourcePath": "include/uapi/linux/types.h",
        "emittedColumn": 36,
        "emittedLine": 199,
        "kind": "preprocessed-parser",
        "lane": "clang-preprocessed-parser",
        "sourceColumn": None,
        "sourceLine": 17,
        "sourcePath": "kernel/power/console.c",
        "tuId": "kernel-power-console",
    },
}

PARSER_SEED_LINES = {
    E1001_FAILURE_ID: [47, 48, 50],
    E1002_FAILURE_ID: [199],
}
RAW_SEED_LINES = {
    "source/include/linux/compiler-version.h": [3, 5],
    "source/include/linux/compiler_types.h": [183, 185, 188, 189, 190],
    "source/include/linux/kconfig.h": [2, 75],
    "source/lib/bitrev.c": [2, 47],
}

RESULT_KEYS = {
    "acceptanceValid",
    "globalVerdict",
    "mode",
    "obligationManifestSha256",
    "pilotAcceptanceValid",
    "pilotContractSha256",
    "policySha256",
    "receiptSha256",
    "records",
    "remainingFailures",
    "sourceEvidenceRunId",
    "sourceFailures",
    "verifiedPilots",
    "verdict",
}
RESULT_RECORD_KEYS = {"failureId", "receiptSha256", "state"}
RECEIPT_KEYS = {
    "artifactClosure",
    "executionAuthority",
    "obligationManifestSha256",
    "overlayClosureSha256",
    "pilotContractSha256",
    "pilotFailureIds",
    "policyId",
    "policySha256",
    "records",
    "schemaVersion",
    "sourceEvidenceRunId",
}
RECEIPT_RECORD_KEYS = {
    "candidate",
    "causalAnchorSha256",
    "failureId",
    "minimality",
    "referenceAcceptance",
    "sourceRecordSha256",
    "state",
    "subjectReplays",
}
SUBJECT_RECEIPT_KEYS = {
    "argv",
    "argvSha256",
    "candidateDigest",
    "causalAnchorSha256",
    "environment",
    "environmentSha256",
    "inputClosureSha256",
    "normalizedObservation",
    "privateDirectory",
    "returnCode",
    "stderr",
    "stderrSha256",
    "stdout",
    "stdoutSha256",
    "subjectSha256",
    "timedOut",
}
ORACLE_RECEIPT_KEYS = {
    "candidateDigest",
    "command",
    "commandSha256",
    "compilerPath",
    "compilerSha256",
    "compilerVersion",
    "containerInputs",
    "environment",
    "imageId",
    "inputClosureSha256",
    "returnCode",
    "stderr",
    "stderrSha256",
    "stdout",
    "stdoutSha256",
    "timedOut",
}
ARTIFACT_KEYS = {"bytes", "path", "sha256"}
CONTAINER_INPUT_KEYS = {"bytes", "logicalPath", "sha256"}
PRIVATE_DIRECTORY_KEYS = {
    "entriesAfterReset",
    "home",
    "resetBeforeExecution",
    "tmpdir",
}
MINIMALITY_KEYS = {
    "atomOrderSha256",
    "atoms",
    "claim",
    "removableAtoms",
    "testedAtoms",
    "totalAtoms",
}
ATOM_RECEIPT_KEYS = {
    "atomId",
    "candidateDigest",
    "interesting",
    "referenceAcceptance",
    "subjectObservation",
}
ALLOWED_ENVIRONMENT_KEYS = {
    "HOME",
    "LANG",
    "LC_ALL",
    "SOURCE_DATE_EPOCH",
    "TMPDIR",
    "TZ",
}
STABLE_REPLAY_FIELDS = {
    "argvSha256",
    "candidateDigest",
    "environmentSha256",
    "returnCode",
    "stderrSha256",
    "stdoutSha256",
    "subjectSha256",
    "timedOut",
}
PILOT_CONTRACT_KEYS = {
    "contractId",
    "contractState",
    "executionIsolation",
    "measurementStatus",
    "minimalityAuthority",
    "parentAuthority",
    "pilots",
    "purpose",
    "receiptAuthority",
    "schemaVersion",
    "verdictSemantics",
}


class TestFailure(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise TestFailure(message)


def canonical(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def require_hash(value: Any, label: str) -> str:
    require(
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value),
        f"{label} is not a lowercase SHA-256",
    )
    return value


def exact_keys(value: Any, expected: set[str], label: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{label} is not an object")
    actual = set(value)
    require(
        actual == expected,
        f"{label} keys differ: missing={sorted(expected - actual)} "
        f"extra={sorted(actual - expected)}",
    )
    return value


def load(path: Path, label: str | None = None) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise TestFailure(f"cannot load {label or path}: {error}") from error
    require(isinstance(value, dict), f"{label or path} root is not an object")
    return value


def store(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))


def descriptor(path: Path, logical: str | None = None) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "bytes": len(data),
        "path": logical or path.name,
        "sha256": digest(data),
    }


def physical_lines(data: bytes) -> list[bytes]:
    return data.splitlines(keepends=True)


def select_lines(data: bytes, selected: list[int], label: str) -> bytes:
    lines = physical_lines(data)
    require(selected == sorted(set(selected)), f"{label} line selection is not sorted")
    require(bool(selected), f"{label} line selection is empty")
    require(selected[-1] <= len(lines), f"{label} line selection exceeds seed")
    return b"".join(lines[index - 1] for index in selected)


def lineage(data: bytes, selected: list[int]) -> list[dict[str, Any]]:
    lines = physical_lines(data)
    return [
        {
            "candidateLine": candidate_line,
            "seedLine": seed_line,
            "seedLineSha256": digest(lines[seed_line - 1]),
        }
        for candidate_line, seed_line in enumerate(selected, start=1)
    ]


def identity(record: dict[str, Any]) -> dict[str, Any]:
    diagnostic = record["diagnostic"]
    return {
        "classification": record["classification"],
        "diagnostic": {
            "code": diagnostic["code"],
            "message": diagnostic["message"],
            "sourcePath": diagnostic["sourcePath"],
            "sourceSpan": diagnostic["sourceSpan"],
        },
        "lane": record["lane"],
        "phase": record["phase"],
        "returnCode": record["returnCode"],
        "sourcePath": record["sourcePath"],
        "timedOut": record["timedOut"],
        "tuId": record["tuId"],
    }


def signature(record: dict[str, Any]) -> dict[str, Any]:
    value = identity(record)
    return {
        key: value[key]
        for key in ("classification", "diagnostic", "phase", "returnCode", "timedOut")
    }


def obligation(record: dict[str, Any]) -> dict[str, str]:
    return {
        "failureId": digest(canonical(identity(record))),
        "failureSignatureSha256": digest(canonical(signature(record))),
        "sourceRecordSha256": digest(canonical(record)),
    }


def normalized_observation(record: dict[str, Any]) -> dict[str, Any]:
    value = signature(record)
    return value


def parse_results(path: Path) -> list[dict[str, Any]]:
    raw = path.read_bytes()
    require(raw.endswith(b"\n") and bool(raw), "results.jsonl is empty or unterminated")
    records: list[dict[str, Any]] = []
    for number, line in enumerate(raw.splitlines(), start=1):
        try:
            value = json.loads(line)
        except json.JSONDecodeError as error:
            raise TestFailure(
                f"results.jsonl:{number} is invalid JSON: {error}"
            ) from error
        require(isinstance(value, dict), f"results.jsonl:{number} is not an object")
        require(
            canonical(value).rstrip(b"\n") == line,
            f"results.jsonl:{number} is noncanonical",
        )
        records.append(value)
    return records


def audit_authority() -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    policy = load(POLICY, "reduction policy")
    require(policy["schemaVersion"] == 1, "policy schemaVersion drift")
    require(policy["contractState"] == "locked", "reduction policy is not locked")
    source = policy["sourceEvidence"]
    require(source["runId"] == RUN_ID, "source run ID drift")
    require(
        Path(source["relativeRoot"]) == EVIDENCE.relative_to(ROOT),
        "source evidence root drift",
    )
    require(
        (
            source["expectedObservations"],
            source["expectedFailures"],
            source["expectedSuccesses"],
        )
        == (48, 46, 2),
        "source denominator drift",
    )
    for label, expected in source["authorityArtifacts"].items():
        path = EVIDENCE / expected["path"]
        require(path.is_file() and not path.is_symlink(), f"missing authority: {label}")
        require(
            descriptor(path, expected["path"]) == expected, f"authority drift: {label}"
        )

    records = parse_results(EVIDENCE / "results.jsonl")
    failures = [record for record in records if record.get("outcome") == "failure"]
    successes = [record for record in records if record.get("outcome") == "success"]
    require(
        (len(records), len(failures), len(successes)) == (48, 46, 2),
        "record denominator drift",
    )
    derived = sorted(
        (obligation(record) for record in failures), key=lambda item: item["failureId"]
    )
    require(len({item["failureId"] for item in derived}) == 46, "failure ID collision")
    require(
        derived == policy["obligationDerivation"]["obligations"],
        "policy obligation bijection drift",
    )
    require(
        digest(b"".join(canonical(item) for item in derived)) == MANIFEST_SHA256,
        "derived obligation manifest drift",
    )
    require(
        policy["obligationDerivation"]["obligationManifestSha256"] == MANIFEST_SHA256,
        "policy obligation manifest drift",
    )
    require(
        policy["replayPolicy"]["baselineReplayCount"]
        == policy["replayPolicy"]["requiredStableReplays"]
        == 3,
        "three-replay authority drift",
    )
    require(
        policy["referenceAcceptance"]["requiredCompilers"] == ["gcc", "clang"],
        "two-oracle authority drift",
    )
    require(
        policy["verdictPolicy"]["missingGateVerdict"] == "INCOMPLETE"
        and policy["verdictPolicy"]["missingGateAcceptanceValid"] is False,
        "fail-closed verdict policy drift",
    )

    by_id = {obligation(record)["failureId"]: record for record in failures}
    require(set(PILOT_FAILURE_IDS) <= set(by_id), "a pilot obligation is absent")
    for failure_id in PILOT_FAILURE_IDS:
        record = by_id[failure_id]
        facts = PILOT_FACTS[failure_id]
        diagnostic = record["diagnostic"]
        require(record["lane"] == facts["lane"], f"pilot lane drift: {failure_id}")
        require(record["tuId"] == facts["tuId"], f"pilot TU drift: {failure_id}")
        require(
            record["sourcePath"] == facts["sourcePath"],
            f"pilot source drift: {failure_id}",
        )
        require(diagnostic["code"] == facts["code"], f"pilot code drift: {failure_id}")
        require(
            diagnostic["sourcePath"] == facts["diagnosticSourcePath"]
            and diagnostic["sourceSpan"]
            == {"column": facts["sourceColumn"], "line": facts["sourceLine"]}
            and diagnostic["emittedSpan"]
            == {"column": facts["emittedColumn"], "line": facts["emittedLine"]},
            f"pilot causal coordinates drift: {failure_id}",
        )
    return policy, {failure_id: by_id[failure_id] for failure_id in PILOT_FAILURE_IDS}


def audit_pilot_contract(
    policy: dict[str, Any], records: dict[str, dict[str, Any]]
) -> dict[str, Any]:
    contract = exact_keys(
        load(PILOT_CONTRACT, "pilot contract"),
        PILOT_CONTRACT_KEYS,
        "pilot contract",
    )
    require(contract["schemaVersion"] == 1, "pilot contract schemaVersion drift")
    require(
        contract["contractId"] == "c30.day5.kernel-reduction-pilots.v1",
        "pilot contract ID drift",
    )
    require(contract["contractState"] == "locked", "pilot contract is not locked")
    require(
        contract["measurementStatus"] == "NOT_MEASURED",
        "pilot contract was result-shaped",
    )
    require(
        isinstance(contract["purpose"], str)
        and "no measured outcome" in contract["purpose"],
        "pilot contract purpose does not preserve pre-execution authority",
    )

    parent = exact_keys(
        contract["parentAuthority"],
        {
            "obligationManifestSha256",
            "policyBytes",
            "policyId",
            "policyPath",
            "policySha256",
            "sourceEvidenceRunId",
            "sourceFailures",
        },
        "pilot parent authority",
    )
    require(parent["policyId"] == policy["policyId"], "pilot parent policy ID drift")
    require(
        parent["policyPath"] == POLICY.relative_to(ROOT).as_posix(),
        "pilot parent policy path drift",
    )
    require(
        parent["policyBytes"] == POLICY.stat().st_size,
        "pilot parent policy bytes drift",
    )
    require(
        parent["policySha256"] == digest(POLICY.read_bytes()),
        "pilot parent policy SHA drift",
    )
    require(parent["sourceEvidenceRunId"] == RUN_ID, "pilot parent run ID drift")
    require(parent["sourceFailures"] == 46, "pilot parent failure denominator drift")
    require(
        parent["obligationManifestSha256"] == MANIFEST_SHA256,
        "pilot parent obligation manifest drift",
    )

    pilots = contract["pilots"]
    require(
        isinstance(pilots, list) and len(pilots) == 3,
        "pilot contract denominator drift",
    )
    ids = [item.get("failureId") for item in pilots if isinstance(item, dict)]
    require(ids == PILOT_FAILURE_IDS, "pilot contract IDs are not exact/sorted")
    for item in pilots:
        failure_id = item["failureId"]
        record = records[failure_id]
        derived = obligation(record)
        require(
            item["failureSignatureSha256"] == derived["failureSignatureSha256"]
            and item["sourceRecordSha256"] == derived["sourceRecordSha256"],
            f"pilot signature authority drift: {failure_id}",
        )
        require(
            (item["lane"], item["tuId"], item["sourcePath"])
            == (record["lane"], record["tuId"], record["sourcePath"]),
            f"pilot lane/TU/source drift: {failure_id}",
        )
        require(
            (item["phase"], item["classification"])
            == (record["phase"], record["classification"]),
            f"pilot phase/classification drift: {failure_id}",
        )
        diagnostic = record["diagnostic"]
        require(
            item["diagnostic"]
            == {
                "code": diagnostic["code"],
                "emittedSpan": diagnostic["emittedSpan"],
                "message": diagnostic["message"],
                "sourcePath": diagnostic["sourcePath"],
                "sourceSpan": diagnostic["sourceSpan"],
            },
            f"pilot diagnostic authority drift: {failure_id}",
        )
        selector = item["candidateSelector"]
        if failure_id in PARSER_SEED_LINES:
            require(
                selector
                == {
                    "kind": "preprocessed-parser",
                    "seedArtifact": "artifacts.parserInput",
                    "seedLines": PARSER_SEED_LINES[failure_id],
                },
                f"parser pilot selector drift: {failure_id}",
            )
            require(
                item["minimalityClaim"] == "preprocessed-line-1-minimal-v1"
                and item["atomCount"] == len(PARSER_SEED_LINES[failure_id]),
                f"parser pilot minimality authority drift: {failure_id}",
            )
        else:
            expected_files = [
                {"logicalPath": logical, "seedLines": selected}
                for logical, selected in RAW_SEED_LINES.items()
            ]
            require(
                selector
                == {
                    "kind": "raw-closure",
                    "outputFiles": [],
                    "seedFamilies": {
                        "clang": "clang",
                        "gcc": "gcc",
                        "subject": "gcc",
                    },
                    "sharedSourceFiles": expected_files,
                },
                "raw pilot selector drift",
            )
            require(
                item["minimalityClaim"] == "raw-closure-file-and-line-1-minimal-v1"
                and item["atomCount"] == 15,
                "raw pilot minimality authority drift",
            )

    isolation = contract["executionIsolation"]
    require(isolation["subjectReplays"] == 3, "pilot subject replay count drift")
    require(isolation["stablePrivatePaths"] is True, "pilot private paths are unstable")
    require(
        isolation["resetSameHomeAndTmpdirBeforeEveryReplay"] is True,
        "pilot replay reset authority drift",
    )
    require(
        isolation["requiredCompilers"] == ["gcc", "clang"], "pilot oracle set drift"
    )
    require(
        "Only the exact candidate or family closure bytes"
        in isolation["sealedContainerInputs"],
        "pilot container input isolation drift",
    )
    require(
        contract["verdictSemantics"]
        == {
            "acceptanceValid": False,
            "globalVerdict": "INCOMPLETE",
            "pilotAcceptanceValid": True,
            "remainingFailures": 43,
            "sourceFailures": 46,
            "verifiedPilotRecords": 3,
        },
        "pilot global verdict semantics drift",
    )
    require(
        contract["receiptAuthority"]
        == {
            "creation": "atomic-no-overwrite",
            "measuredReceiptIndex": "separate-post-run-authority",
            "replayRequiresCallerExpectedReceiptSha256": True,
            "resultAndReceiptBindPilotContractSha256": True,
            "schemaVersion": 1,
        },
        "pilot receipt authority drift",
    )

    forbidden_result_keys = {
        "measuredAt",
        "oracleResults",
        "outcome",
        "receiptSha256",
        "returnCode",
        "stderr",
        "stdout",
        "subjectResults",
        "timedOut",
    }

    def reject_measured_fields(value: Any, path: str) -> None:
        if isinstance(value, dict):
            overlap = set(value) & forbidden_result_keys
            require(
                not overlap,
                f"pilot contract contains measured fields at {path}: {sorted(overlap)}",
            )
            for name, child in value.items():
                reject_measured_fields(child, f"{path}.{name}")
        elif isinstance(value, list):
            for index, child in enumerate(value):
                reject_measured_fields(child, f"{path}[{index}]")

    reject_measured_fields(contract, "pilotContract")
    return contract


def audit_contract_atoms(
    contract: dict[str, Any], models: dict[str, dict[str, Any]]
) -> None:
    expected = [
        {"atomId": atom["atomId"], "failureId": failure_id}
        for failure_id in PILOT_FAILURE_IDS
        for atom in expected_atoms(models[failure_id])
    ]
    authority = exact_keys(
        contract["minimalityAuthority"],
        {"executionRule", "interestingness", "orderedAtoms", "totalAtoms"},
        "pilot minimality authority",
    )
    require(authority["orderedAtoms"] == expected, "pilot ordered atom authority drift")
    require(
        authority["totalAtoms"] == len(expected) == 19,
        "pilot total atom authority drift",
    )
    require(
        "including variants with an empty retained file or a missing file"
        in authority["executionRule"],
        "pilot minimality authority permits structural skips",
    )
    require(
        "both pinned compilers accept" in authority["interestingness"],
        "pilot interestingness omits two-oracle acceptance",
    )


def safe_artifact(evidence: Path, subdirectory: str, value: dict[str, Any]) -> Path:
    logical = PurePosixPath(value["path"])
    require(
        not logical.is_absolute()
        and ".." not in logical.parts
        and all(part not in {"", "."} for part in logical.parts),
        f"unsafe artifact path: {value['path']}",
    )
    path = evidence / subdirectory / Path(*logical.parts)
    require(path.is_file() and not path.is_symlink(), f"missing artifact: {path}")
    require(
        descriptor(path, value["path"]) == value, f"artifact descriptor drift: {path}"
    )
    return path


def parse_seed_closure(tu_id: str, family: str) -> dict[str, tuple[str, int]]:
    path = EVIDENCE / "inputs" / f"{tu_id}.{family}.inputs.tsv"
    require(path.is_file() and not path.is_symlink(), f"missing input closure: {path}")
    values: dict[str, tuple[str, int]] = {}
    for number, line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        parts = line.split("\t")
        require(len(parts) == 3, f"malformed input closure row: {path}:{number}")
        logical, sha256, byte_count_text = parts
        require(
            logical not in values, f"duplicate input logical path: {path}:{logical}"
        )
        require_hash(sha256, f"input closure SHA-256 {path}:{logical}")
        values[logical] = (sha256, int(byte_count_text))
    require(bool(values), f"empty input closure: {path}")
    return values


def raw_seed_path(policy: dict[str, Any], family: str, logical: str) -> Path:
    prefix, relative = logical.split("/", 1)
    require(prefix in {"source", "output"}, f"invalid raw logical path: {logical}")
    roots = policy["sourceEvidence"]["executionAuthority"]["rawSeedRoots"]
    root_value = roots["source" if prefix == "source" else f"{family}Output"]
    base = ROOT if root_value["base"] == "repository" else EVIDENCE
    root = (base / root_value["path"]).resolve(strict=True)
    path = (root / relative).resolve(strict=True)
    require(root in path.parents, f"raw seed escapes root: {logical}")
    require(path.is_file() and not path.is_symlink(), f"missing raw seed: {logical}")
    return path


def closure_sha(files: dict[str, bytes]) -> str:
    rows = [
        f"{logical}\t{digest(data)}\t{len(data)}\n".encode("utf-8")
        for logical, data in sorted(files.items())
    ]
    return digest(b"".join(rows))


def input_descriptors(files: dict[str, bytes]) -> list[dict[str, Any]]:
    return [
        {"bytes": len(data), "logicalPath": logical, "sha256": digest(data)}
        for logical, data in sorted(files.items())
    ]


def raw_bundle_digest(
    root_source_path: str,
    subject: dict[str, bytes],
    gcc: dict[str, bytes],
    clang: dict[str, bytes],
) -> str:
    root = f"source/{root_source_path}"
    bundle = {
        "referenceClosures": {
            "clang": {
                "closureSha256": closure_sha(clang),
                "root": root,
                "seedFamily": "clang",
            },
            "gcc": {
                "closureSha256": closure_sha(gcc),
                "root": root,
                "seedFamily": "gcc",
            },
        },
        "rootSourcePath": root_source_path,
        "subjectClosure": {
            "closureSha256": closure_sha(subject),
            "root": root,
            "seedFamily": "gcc",
        },
    }
    return digest(canonical(bundle))


def write_raw_closure(
    overlay: Path,
    failure_id: str,
    flavor: str,
    family: str,
    root_source_path: str,
    seeds: dict[str, tuple[bytes, list[int]]],
) -> dict[str, Any]:
    files: list[dict[str, Any]] = []
    materialized: dict[str, bytes] = {}
    for logical, (seed_data, selected) in sorted(seeds.items()):
        data = select_lines(seed_data, selected, f"raw {family}:{logical}")
        path = overlay / "candidates" / failure_id / flavor / Path(logical)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        materialized[logical] = data
        files.append(
            {
                "bytes": len(data),
                "lineage": lineage(seed_data, selected),
                "logicalPath": logical,
                "sha256": digest(data),
            }
        )
    return {
        "manifest": {
            "closureSha256": closure_sha(materialized),
            "files": files,
            "root": f"source/{root_source_path}",
            "seedFamily": family,
        },
        "materialized": materialized,
    }


def build_pilot_overlay(
    root: Path,
    policy: dict[str, Any],
    records: dict[str, dict[str, Any]],
) -> tuple[Path, dict[str, dict[str, Any]]]:
    overlay = root / "pilot-overlay"
    overlay.mkdir()
    models: dict[str, dict[str, Any]] = {}
    manifest_records: list[dict[str, Any]] = []

    for failure_id in PILOT_FAILURE_IDS:
        record = records[failure_id]
        facts = PILOT_FACTS[failure_id]
        if facts["kind"] == "preprocessed-parser":
            seed = safe_artifact(EVIDENCE, "parser", record["artifacts"]["parserInput"])
            seed_data = seed.read_bytes()
            selected = PARSER_SEED_LINES[failure_id]
            data = select_lines(seed_data, selected, f"parser pilot {failure_id}")
            relative = PurePosixPath("candidates") / failure_id / "parser.c"
            candidate_path = overlay / Path(*relative.parts)
            candidate_path.parent.mkdir(parents=True, exist_ok=True)
            candidate_path.write_bytes(data)
            candidate = {
                "bytes": len(data),
                "kind": "preprocessed-parser",
                "lineage": lineage(seed_data, selected),
                "path": relative.as_posix(),
                "sha256": digest(data),
            }
            models[failure_id] = {
                "candidateDigest": digest(data),
                "candidateManifest": candidate,
                "kind": "preprocessed-parser",
                "lineage": candidate["lineage"],
                "parserData": data,
                "record": record,
            }
        else:
            source_path = record["sourcePath"]
            closures = {
                family: parse_seed_closure(record["tuId"], family)
                for family in ("gcc", "clang")
            }
            seeds_by_family: dict[str, dict[str, tuple[bytes, list[int]]]] = {}
            for family in ("gcc", "clang"):
                seeds: dict[str, tuple[bytes, list[int]]] = {}
                for logical, selected in RAW_SEED_LINES.items():
                    require(
                        logical in closures[family],
                        f"raw pilot seed is absent from {family} closure: {logical}",
                    )
                    path = raw_seed_path(policy, family, logical)
                    data = path.read_bytes()
                    expected = closures[family][logical]
                    require(
                        (digest(data), len(data)) == expected,
                        f"raw pilot seed identity drift: {family}:{logical}",
                    )
                    seeds[logical] = (data, selected)
                seeds_by_family[family] = seeds
            for logical in RAW_SEED_LINES:
                require(
                    seeds_by_family["gcc"][logical][0]
                    == seeds_by_family["clang"][logical][0],
                    f"raw shared seed bytes differ: {logical}",
                )
            subject = write_raw_closure(
                overlay,
                failure_id,
                "subject",
                "gcc",
                source_path,
                seeds_by_family["gcc"],
            )
            reference_gcc = write_raw_closure(
                overlay,
                failure_id,
                "reference-gcc",
                "gcc",
                source_path,
                seeds_by_family["gcc"],
            )
            reference_clang = write_raw_closure(
                overlay,
                failure_id,
                "reference-clang",
                "clang",
                source_path,
                seeds_by_family["clang"],
            )
            candidate = {
                "bundleSha256": raw_bundle_digest(
                    source_path,
                    subject["materialized"],
                    reference_gcc["materialized"],
                    reference_clang["materialized"],
                ),
                "kind": "raw-closure",
                "referenceClosures": {
                    "clang": reference_clang["manifest"],
                    "gcc": reference_gcc["manifest"],
                },
                "rootSourcePath": source_path,
                "subjectClosure": subject["manifest"],
            }
            models[failure_id] = {
                "candidateDigest": candidate["bundleSha256"],
                "candidateManifest": candidate,
                "kind": "raw-closure",
                "rawFiles": {
                    "clang": reference_clang["materialized"],
                    "gcc": reference_gcc["materialized"],
                    "subject": subject["materialized"],
                },
                "rawLineage": {
                    entry["logicalPath"]: entry["lineage"]
                    for entry in subject["manifest"]["files"]
                },
                "record": record,
            }
        manifest_records.append(
            {"candidate": candidate, "failureId": failure_id, "state": "CANDIDATE"}
        )

    require(
        [item["failureId"] for item in manifest_records] == PILOT_FAILURE_IDS,
        "pilot records are not failure-ID sorted",
    )
    store(
        overlay / "overlay.json",
        {
            "policyId": policy["policyId"],
            "records": manifest_records,
            "schemaVersion": 1,
            "sourceEvidenceRunId": RUN_ID,
        },
    )
    return overlay, models


def regular_closure(root: Path, excluded: set[str] | None = None) -> dict[str, Any]:
    excluded = excluded or set()
    entries: list[dict[str, Any]] = []
    seen_inodes: set[tuple[int, int]] = set()
    for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
        relative = path.relative_to(root).as_posix()
        if relative in excluded:
            continue
        mode = path.lstat().st_mode
        require(not stat.S_ISLNK(mode), f"closure contains a symbolic link: {relative}")
        if path.is_dir():
            continue
        require(stat.S_ISREG(mode), f"closure contains a non-regular file: {relative}")
        info = path.stat()
        require(info.st_nlink == 1, f"closure contains a hard link: {relative}")
        inode = (info.st_dev, info.st_ino)
        require(inode not in seen_inodes, f"closure aliases an inode: {relative}")
        seen_inodes.add(inode)
        entries.append(descriptor(path, relative))
    rows = [
        f"{entry['path']}\t{entry['sha256']}\t{entry['bytes']}\n".encode("utf-8")
        for entry in entries
    ]
    return {
        "bytes": sum(entry["bytes"] for entry in entries),
        "files": entries,
        "sha256": digest(b"".join(rows)),
    }


def anchor_object(model: dict[str, Any]) -> dict[str, Any]:
    record = model["record"]
    failure_id = obligation(record)["failureId"]
    diagnostic = record["diagnostic"]
    emitted_seed_line = diagnostic["emittedSpan"]["line"]
    if model["kind"] == "preprocessed-parser":
        mappings = {
            item["seedLine"]: item for item in model["candidateManifest"]["lineage"]
        }
        require(
            emitted_seed_line in mappings,
            f"parser pilot deleted its causal line: {failure_id}",
        )
        mapping = mappings[emitted_seed_line]
        return {
            "candidateLine": mapping["candidateLine"],
            "failureId": failure_id,
            "kind": model["kind"],
            "seedLine": emitted_seed_line,
            "seedLineSha256": mapping["seedLineSha256"],
            "sourceColumn": diagnostic["sourceSpan"]["column"],
            "sourceLine": diagnostic["sourceSpan"]["line"],
            "sourcePath": diagnostic["sourcePath"],
            "sourceRecordSha256": obligation(record)["sourceRecordSha256"],
        }
    causal = "source/" + diagnostic["sourcePath"]
    mappings = {item["seedLine"]: item for item in model["rawLineage"][causal]}
    require(emitted_seed_line in mappings, "raw pilot deleted its causal line")
    mapping = mappings[emitted_seed_line]
    return {
        "candidateLine": mapping["candidateLine"],
        "failureId": failure_id,
        "kind": model["kind"],
        "logicalPath": causal,
        "seedLine": emitted_seed_line,
        "seedLineSha256": mapping["seedLineSha256"],
        "sourceColumn": diagnostic["sourceSpan"]["column"],
        "sourceLine": diagnostic["sourceSpan"]["line"],
        "sourcePath": diagnostic["sourcePath"],
        "sourceRecordSha256": obligation(record)["sourceRecordSha256"],
    }


def variant_from_parser(
    model: dict[str, Any], deleted_line: int | None
) -> dict[str, Any]:
    manifest = model["candidateManifest"]
    lines = physical_lines(model["parserData"])
    mappings = manifest["lineage"]
    kept = [
        (line, mapping)
        for index, (line, mapping) in enumerate(zip(lines, mappings), start=1)
        if index != deleted_line
    ]
    data = b"".join(line for line, _ in kept)
    files = {"candidate.c": data}
    return {
        "candidateDigest": digest(data),
        "inputs": {family: files for family in ("subject", "gcc", "clang")},
    }


def variant_from_raw(
    model: dict[str, Any],
    deleted_file: str | None,
    deleted_line: tuple[str, int] | None,
) -> dict[str, Any]:
    values: dict[str, dict[str, bytes]] = {}
    for family, original in model["rawFiles"].items():
        files = dict(original)
        if deleted_file is not None:
            files.pop(deleted_file)
        if deleted_line is not None:
            logical, candidate_line = deleted_line
            lines = physical_lines(files[logical])
            require(
                candidate_line <= len(lines),
                f"raw mutation line exceeds candidate: {logical}",
            )
            files[logical] = b"".join(
                line
                for index, line in enumerate(lines, start=1)
                if index != candidate_line
            )
        values[family] = files
    record = model["record"]
    return {
        "candidateDigest": raw_bundle_digest(
            record["sourcePath"],
            values["subject"],
            values["gcc"],
            values["clang"],
        ),
        "inputs": values,
    }


def expected_atoms(model: dict[str, Any]) -> list[dict[str, Any]]:
    atoms: list[dict[str, Any]] = []
    if model["kind"] == "preprocessed-parser":
        for item in model["candidateManifest"]["lineage"]:
            atom_id = (
                f"line:{item['candidateLine']:06d}:seed:{item['seedLine']:06d}:"
                f"{item['seedLineSha256']}"
            )
            atoms.append(
                {
                    "atomId": atom_id,
                    "variant": variant_from_parser(model, item["candidateLine"]),
                }
            )
    else:
        for logical in sorted(model["rawFiles"]["subject"]):
            atoms.append(
                {
                    "atomId": f"file:{logical}",
                    "variant": variant_from_raw(model, logical, None),
                }
            )
            for item in model["rawLineage"][logical]:
                atom_id = (
                    f"line:{logical}:candidate:{item['candidateLine']:06d}:"
                    f"seed:{item['seedLine']:06d}:{item['seedLineSha256']}"
                )
                atoms.append(
                    {
                        "atomId": atom_id,
                        "variant": variant_from_raw(
                            model, None, (logical, item["candidateLine"])
                        ),
                    }
                )
    atoms.sort(key=lambda item: item["atomId"])
    return atoms


def run_verifier(*args: str, timeout: int = 1800) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFIER), *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=timeout,
        check=False,
    )


def verify_command(overlay: Path, receipt: Path) -> subprocess.CompletedProcess[str]:
    return run_verifier(
        "verify-pilots",
        "--pilot-contract",
        str(PILOT_CONTRACT),
        "--policy",
        str(POLICY),
        "--evidence",
        str(EVIDENCE),
        "--overlay",
        str(overlay),
        "--receipt-dir",
        str(receipt),
    )


def replay_command(
    overlay: Path, receipt: Path, expected_sha256: str | None
) -> subprocess.CompletedProcess[str]:
    args = [
        "replay-pilot-receipt",
        "--pilot-contract",
        str(PILOT_CONTRACT),
        "--policy",
        str(POLICY),
        "--evidence",
        str(EVIDENCE),
        "--overlay",
        str(overlay),
        "--receipt-dir",
        str(receipt),
    ]
    if expected_sha256 is not None:
        args.extend(["--expected-receipt-sha256", expected_sha256])
    return run_verifier(*args)


def parse_success(
    result: subprocess.CompletedProcess[str], label: str
) -> dict[str, Any]:
    require(result.returncode == 0, f"{label} failed: {result.stderr.strip()}")
    require(result.stderr == "", f"{label} wrote stderr: {result.stderr.strip()}")
    require(result.stdout.endswith("\n"), f"{label} stdout is unterminated")
    require(
        result.stdout.count("\n") == 1, f"{label} stdout is not one canonical record"
    )
    try:
        value = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise TestFailure(f"{label} stdout is not JSON: {error}") from error
    require(
        canonical(value).decode("utf-8") == result.stdout,
        f"{label} JSON is noncanonical",
    )
    return exact_keys(value, RESULT_KEYS, f"{label} result")


def require_rejected(
    result: subprocess.CompletedProcess[str], label: str, receipt: Path | None = None
) -> None:
    require(result.returncode == 2, f"{label} was not rejected: rc={result.returncode}")
    require(
        "PILOT_VERIFIED" not in result.stdout, f"{label} emitted a false pilot verdict"
    )
    if not result.stderr.startswith(FAILURE_PREFIX):
        require(
            "usage:" in result.stderr,
            f"{label} lacks a contract or argparse failure: {result.stderr.strip()}",
        )
    if receipt is not None:
        require(not receipt.exists(), f"{label} left a partial receipt")


def validate_result(
    result: dict[str, Any],
    mode: str,
    policy: dict[str, Any],
    receipt: dict[str, Any],
    receipt_sha256: str,
) -> None:
    require(result["acceptanceValid"] is False, "pilot result forged global acceptance")
    require(
        result["globalVerdict"] == "INCOMPLETE", "pilot result forged global VERIFIED"
    )
    require(result["mode"] == mode, f"pilot result mode drift: {mode}")
    require(
        result["obligationManifestSha256"] == MANIFEST_SHA256, "result manifest drift"
    )
    require(result["pilotAcceptanceValid"] is True, "valid pilots were not accepted")
    require(
        result["pilotContractSha256"] == digest(PILOT_CONTRACT.read_bytes()),
        "result pilot contract SHA drift",
    )
    require(
        result["policySha256"] == digest(POLICY.read_bytes()), "result policy SHA drift"
    )
    require(result["receiptSha256"] == receipt_sha256, "result receipt SHA drift")
    require(result["remainingFailures"] == 43, "result remaining denominator drift")
    require(result["sourceEvidenceRunId"] == RUN_ID, "result source run drift")
    require(result["sourceFailures"] == 46, "result source failure denominator drift")
    require(result["verifiedPilots"] == 3, "result pilot denominator drift")
    require(result["verdict"] == "PILOT_VERIFIED", "pilot verdict drift")
    records = result["records"]
    require(
        isinstance(records, list) and len(records) == 3,
        "result records denominator drift",
    )
    expected_by_id = {item["failureId"]: item for item in receipt["records"]}
    ids: list[str] = []
    for index, item in enumerate(records):
        value = exact_keys(item, RESULT_RECORD_KEYS, f"result.records[{index}]")
        failure_id = value["failureId"]
        ids.append(failure_id)
        require(
            value["state"] == "VERIFIED", f"pilot record not VERIFIED: {failure_id}"
        )
        require(
            failure_id in expected_by_id,
            f"result includes an unknown pilot: {failure_id}",
        )
        require(
            value["receiptSha256"] == digest(canonical(expected_by_id[failure_id])),
            f"record receipt SHA drift: {failure_id}",
        )
    require(ids == PILOT_FAILURE_IDS, "result pilot records are not exact/sorted")
    require(
        policy["verdictPolicy"]["phase1AcceptanceValid"] is False, "policy false-green"
    )


def validate_environment(value: Any, label: str) -> dict[str, str]:
    environment = exact_keys(value, ALLOWED_ENVIRONMENT_KEYS, f"{label} environment")
    for name, item in environment.items():
        require(isinstance(item, str), f"{label} environment {name} is not text")
    require(environment["LANG"] == "C", f"{label} LANG drift")
    require(environment["LC_ALL"] == "C", f"{label} LC_ALL drift")
    require(environment["TZ"] == "UTC", f"{label} TZ drift")
    return environment


def check_artifact(
    receipt_root: Path,
    value: Any,
    label: str,
    seen: set[str],
) -> dict[str, Any]:
    artifact = exact_keys(value, ARTIFACT_KEYS, label)
    logical = PurePosixPath(artifact["path"])
    require(
        not logical.is_absolute()
        and ".." not in logical.parts
        and all(part not in {"", "."} for part in logical.parts),
        f"{label} path is unsafe",
    )
    relative = logical.as_posix()
    require(relative != "receipt.json", f"{label} aliases receipt.json")
    require(relative not in seen, f"artifact is reused across invocations: {relative}")
    seen.add(relative)
    path = receipt_root / Path(*logical.parts)
    require(path.is_file() and not path.is_symlink(), f"{label} is missing")
    info = path.stat()
    require(info.st_nlink == 1, f"{label} is hard-linked")
    require(descriptor(path, relative) == artifact, f"{label} descriptor drift")
    return artifact


def forbidden_container_texts(receipt_root: Path, overlay: Path) -> list[str]:
    return [
        str(POLICY),
        str(EVIDENCE),
        str(receipt_root),
        str(overlay),
        "expectedResult",
        "expected-result",
        "golden",
        "receipt.json",
        "sourceRecordSha256",
        "failureSignatureSha256",
    ]


def validate_subject_receipt(
    value: Any,
    label: str,
    policy: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    anchor_sha256: str,
    receipt_root: Path,
    seen_artifacts: set[str],
    baseline: bool,
) -> dict[str, Any]:
    receipt = exact_keys(value, SUBJECT_RECEIPT_KEYS, label)
    require(
        receipt["candidateDigest"] == variant["candidateDigest"],
        f"{label} candidate drift",
    )
    expected_inputs = variant["inputs"]["subject"]
    require(
        receipt["inputClosureSha256"] == closure_sha(expected_inputs),
        f"{label} input closure drift",
    )
    require(
        receipt["subjectSha256"]
        == policy["sourceEvidence"]["executionAuthority"]["subjectSha256"],
        f"{label} subject identity drift",
    )
    require(
        isinstance(receipt["argv"], list) and bool(receipt["argv"]),
        f"{label} argv invalid",
    )
    require(
        all(isinstance(item, str) for item in receipt["argv"]),
        f"{label} argv has non-text",
    )
    require(
        receipt["argvSha256"] == digest(canonical(receipt["argv"])),
        f"{label} argv hash drift",
    )
    environment = validate_environment(receipt["environment"], label)
    require(
        receipt["environmentSha256"] == digest(canonical(environment)),
        f"{label} environment hash drift",
    )
    private = exact_keys(
        receipt["privateDirectory"], PRIVATE_DIRECTORY_KEYS, f"{label} privateDirectory"
    )
    require(
        private["resetBeforeExecution"] is True, f"{label} did not reset private dirs"
    )
    require(
        private["entriesAfterReset"] == [],
        f"{label} private dirs were not empty after reset",
    )
    require(private["home"] == environment["HOME"], f"{label} HOME receipt drift")
    require(private["tmpdir"] == environment["TMPDIR"], f"{label} TMPDIR receipt drift")
    require(private["home"] != private["tmpdir"], f"{label} aliases HOME and TMPDIR")
    for path_text in (private["home"], private["tmpdir"]):
        require(
            path_text not in {str(ROOT), str(EVIDENCE), str(receipt_root)},
            f"{label} private path is not private",
        )
    stdout = check_artifact(
        receipt_root, receipt["stdout"], f"{label} stdout", seen_artifacts
    )
    stderr = check_artifact(
        receipt_root, receipt["stderr"], f"{label} stderr", seen_artifacts
    )
    require(receipt["stdoutSha256"] == stdout["sha256"], f"{label} stdout hash drift")
    require(receipt["stderrSha256"] == stderr["sha256"], f"{label} stderr hash drift")
    require(
        isinstance(receipt["returnCode"], int), f"{label} return code is not integer"
    )
    require(receipt["timedOut"] is False, f"{label} timed out")
    if baseline:
        require(
            receipt["normalizedObservation"] == normalized_observation(model["record"]),
            f"{label} does not reproduce the frozen normalized failure",
        )
        require(
            receipt["causalAnchorSha256"] == anchor_sha256,
            f"{label} causal anchor drift",
        )
    else:
        require(
            receipt["causalAnchorSha256"] in {None, anchor_sha256},
            f"{label} supplied an alien causal anchor",
        )
        require(
            receipt["normalizedObservation"] is None
            or isinstance(receipt["normalizedObservation"], dict),
            f"{label} normalized observation has invalid type",
        )
    return receipt


def validate_oracle_receipt(
    value: Any,
    family: str,
    label: str,
    policy: dict[str, Any],
    variant: dict[str, Any],
    receipt_root: Path,
    overlay: Path,
    seen_artifacts: set[str],
    require_acceptance: bool,
) -> dict[str, Any]:
    receipt = exact_keys(value, ORACLE_RECEIPT_KEYS, label)
    compiler = policy["sourceEvidence"]["executionAuthority"]["referenceCompilers"][
        family
    ]
    require(receipt["compilerPath"] == compiler["path"], f"{label} compiler path drift")
    require(
        receipt["compilerSha256"] == compiler["sha256"], f"{label} compiler SHA drift"
    )
    require(
        receipt["compilerVersion"] == compiler["version"],
        f"{label} compiler version drift",
    )
    require(
        receipt["imageId"]
        == policy["sourceEvidence"]["executionAuthority"]["sealedImageId"],
        f"{label} image identity drift",
    )
    require(
        receipt["candidateDigest"] == variant["candidateDigest"],
        f"{label} candidate drift",
    )
    expected_inputs = variant["inputs"][family]
    require(
        receipt["containerInputs"] == input_descriptors(expected_inputs),
        f"{label} received anything other than exact candidate inputs",
    )
    require(
        receipt["inputClosureSha256"] == closure_sha(expected_inputs),
        f"{label} input closure drift",
    )
    command = receipt["command"]
    require(isinstance(command, list) and bool(command), f"{label} command is invalid")
    require(
        all(isinstance(item, str) for item in command), f"{label} command has non-text"
    )
    require(command[0] == compiler["path"], f"{label} command uses another compiler")
    require(
        receipt["commandSha256"] == digest(canonical(command)),
        f"{label} command hash drift",
    )
    environment = validate_environment(receipt["environment"], label)
    serialized_authority = canonical(
        {"command": command, "environment": environment}
    ).decode("utf-8")
    for forbidden in forbidden_container_texts(receipt_root, overlay):
        require(
            forbidden not in serialized_authority,
            f"{label} leaked forbidden oracle input: {forbidden}",
        )
    stdout = check_artifact(
        receipt_root, receipt["stdout"], f"{label} stdout", seen_artifacts
    )
    stderr = check_artifact(
        receipt_root, receipt["stderr"], f"{label} stderr", seen_artifacts
    )
    require(receipt["stdoutSha256"] == stdout["sha256"], f"{label} stdout hash drift")
    require(receipt["stderrSha256"] == stderr["sha256"], f"{label} stderr hash drift")
    require(isinstance(receipt["returnCode"], int), f"{label} return code invalid")
    require(receipt["timedOut"] is False, f"{label} timed out")
    if require_acceptance:
        require(
            receipt["returnCode"] == 0, f"{label} did not accept exact candidate bytes"
        )
    return receipt


def validate_receipt(
    receipt_root: Path,
    overlay: Path,
    policy: dict[str, Any],
    models: dict[str, dict[str, Any]],
) -> tuple[dict[str, Any], str]:
    receipt_path = receipt_root / "receipt.json"
    require(
        receipt_path.is_file() and not receipt_path.is_symlink(),
        "receipt.json is absent",
    )
    receipt_bytes = receipt_path.read_bytes()
    receipt = load(receipt_path, "pilot receipt")
    require(canonical(receipt) == receipt_bytes, "receipt.json is not canonical")
    exact_keys(receipt, RECEIPT_KEYS, "pilot receipt")
    require(receipt["schemaVersion"] == 1, "receipt schemaVersion drift")
    require(receipt["policyId"] == policy["policyId"], "receipt policy ID drift")
    require(
        receipt["policySha256"] == digest(POLICY.read_bytes()),
        "receipt policy SHA drift",
    )
    require(receipt["sourceEvidenceRunId"] == RUN_ID, "receipt source run drift")
    require(
        receipt["obligationManifestSha256"] == MANIFEST_SHA256, "receipt manifest drift"
    )
    require(
        receipt["pilotContractSha256"] == digest(PILOT_CONTRACT.read_bytes()),
        "receipt pilot contract SHA drift",
    )
    require(receipt["pilotFailureIds"] == PILOT_FAILURE_IDS, "receipt pilot set drift")
    require(
        receipt["executionAuthority"] == policy["sourceEvidence"]["executionAuthority"],
        "receipt execution authority drift",
    )
    require(
        receipt["overlayClosureSha256"] == regular_closure(overlay)["sha256"],
        "receipt overlay closure drift",
    )

    artifact_closure = exact_keys(
        receipt["artifactClosure"], {"bytes", "files", "sha256"}, "artifact closure"
    )
    actual_artifacts = regular_closure(receipt_root, excluded={"receipt.json"})
    require(artifact_closure == actual_artifacts, "receipt artifact closure drift")

    seen_artifacts: set[str] = set()
    receipt_records = receipt["records"]
    require(
        isinstance(receipt_records, list) and len(receipt_records) == 3,
        "receipt record denominator drift",
    )
    ids: list[str] = []
    for record_index, record_value in enumerate(receipt_records):
        value = exact_keys(
            record_value, RECEIPT_RECORD_KEYS, f"receipt.records[{record_index}]"
        )
        failure_id = value["failureId"]
        ids.append(failure_id)
        require(failure_id in models, f"receipt includes unknown pilot: {failure_id}")
        model = models[failure_id]
        source_record = model["record"]
        require(
            value["state"] == "VERIFIED", f"receipt state is not VERIFIED: {failure_id}"
        )
        require(
            value["sourceRecordSha256"]
            == obligation(source_record)["sourceRecordSha256"],
            f"receipt source record drift: {failure_id}",
        )
        require(
            value["candidate"]
            == {"digest": model["candidateDigest"], "kind": model["kind"]},
            f"receipt candidate identity drift: {failure_id}",
        )
        expected_anchor = digest(canonical(anchor_object(model)))
        require(
            value["causalAnchorSha256"] == expected_anchor,
            f"receipt anchor drift: {failure_id}",
        )
        baseline_variant = (
            variant_from_parser(model, None)
            if model["kind"] == "preprocessed-parser"
            else variant_from_raw(model, None, None)
        )
        replays = value["subjectReplays"]
        require(
            isinstance(replays, list) and len(replays) == 3,
            f"subject replay denominator drift: {failure_id}",
        )
        validated_replays = [
            validate_subject_receipt(
                replay,
                f"{failure_id} subject replay {index}",
                policy,
                model,
                baseline_variant,
                expected_anchor,
                receipt_root,
                seen_artifacts,
                baseline=True,
            )
            for index, replay in enumerate(replays)
        ]
        stable = [
            {name: replay[name] for name in STABLE_REPLAY_FIELDS}
            for replay in validated_replays
        ]
        require(
            stable[0] == stable[1] == stable[2],
            f"subject replays are not byte-exact: {failure_id}",
        )
        require(
            validated_replays[0]["environment"]
            == validated_replays[1]["environment"]
            == validated_replays[2]["environment"],
            f"subject replay environments are not stable: {failure_id}",
        )
        require(
            validated_replays[0]["privateDirectory"]
            == validated_replays[1]["privateDirectory"]
            == validated_replays[2]["privateDirectory"],
            f"subject replay private paths are not stable/reset: {failure_id}",
        )

        references = exact_keys(
            value["referenceAcceptance"],
            {"clang", "gcc"},
            f"{failure_id} baseline references",
        )
        baseline_oracles = {
            family: validate_oracle_receipt(
                references[family],
                family,
                f"{failure_id} baseline {family}",
                policy,
                baseline_variant,
                receipt_root,
                overlay,
                seen_artifacts,
                require_acceptance=True,
            )
            for family in ("gcc", "clang")
        }
        require(
            baseline_oracles["gcc"]["compilerSha256"]
            != baseline_oracles["clang"]["compilerSha256"],
            f"baseline compilers alias: {failure_id}",
        )

        minimality = exact_keys(
            value["minimality"], MINIMALITY_KEYS, f"{failure_id} minimality"
        )
        expected = expected_atoms(model)
        expected_ids = [item["atomId"] for item in expected]
        expected_claim = (
            policy["overlayPolicy"]["parserCandidate"]["minimality"]
            if model["kind"] == "preprocessed-parser"
            else policy["overlayPolicy"]["rawCandidate"]["minimality"]
        )
        require(
            minimality["claim"] == expected_claim,
            f"minimality claim drift: {failure_id}",
        )
        require(
            minimality["totalAtoms"] == minimality["testedAtoms"] == len(expected),
            f"minimality denominator drift: {failure_id}",
        )
        require(
            minimality["removableAtoms"] == 0, f"candidate is not minimal: {failure_id}"
        )
        require(
            minimality["atomOrderSha256"] == digest(canonical(expected_ids)),
            f"minimality atom order digest drift: {failure_id}",
        )
        atoms = minimality["atoms"]
        require(
            isinstance(atoms, list) and len(atoms) == len(expected),
            f"minimality atom list drift: {failure_id}",
        )
        for atom_index, (atom_value, expected_atom) in enumerate(zip(atoms, expected)):
            atom = exact_keys(
                atom_value, ATOM_RECEIPT_KEYS, f"{failure_id} atom {atom_index}"
            )
            require(
                atom["atomId"] == expected_atom["atomId"],
                f"minimality atom ID drift: {failure_id}",
            )
            variant = expected_atom["variant"]
            require(
                atom["candidateDigest"] == variant["candidateDigest"],
                f"deleted candidate digest drift: {failure_id}",
            )
            subject = validate_subject_receipt(
                atom["subjectObservation"],
                f"{failure_id} atom {atom_index} subject",
                policy,
                model,
                variant,
                expected_anchor,
                receipt_root,
                seen_artifacts,
                baseline=False,
            )
            atom_references = exact_keys(
                atom["referenceAcceptance"],
                {"clang", "gcc"},
                f"{failure_id} atom {atom_index} references",
            )
            oracle_values = {
                family: validate_oracle_receipt(
                    atom_references[family],
                    family,
                    f"{failure_id} atom {atom_index} {family}",
                    policy,
                    variant,
                    receipt_root,
                    overlay,
                    seen_artifacts,
                    require_acceptance=False,
                )
                for family in ("gcc", "clang")
            }
            subject_matches = (
                subject["normalizedObservation"]
                == normalized_observation(source_record)
                and subject["causalAnchorSha256"] == expected_anchor
            )
            interesting = (
                subject_matches
                and oracle_values["gcc"]["returnCode"] == 0
                and not oracle_values["gcc"]["timedOut"]
                and oracle_values["clang"]["returnCode"] == 0
                and not oracle_values["clang"]["timedOut"]
            )
            require(
                atom["interesting"] is interesting,
                f"interestingness drift: {failure_id}:{atom_index}",
            )
            require(
                not interesting,
                f"removable atom was called minimal: {failure_id}:{atom_index}",
            )
    require(ids == PILOT_FAILURE_IDS, "receipt records are not exact/sorted")
    require(
        seen_artifacts == {item["path"] for item in artifact_closure["files"]},
        "receipt artifacts are unreferenced or multiply referenced",
    )
    require(
        sum(len(expected_atoms(models[item])) for item in PILOT_FAILURE_IDS) == 19,
        "pilot atom denominator is not 19",
    )
    return receipt, digest(receipt_bytes)


def snapshot(root: Path) -> list[tuple[str, str, int, str | None]]:
    values: list[tuple[str, str, int, str | None]] = []
    for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
        relative = path.relative_to(root).as_posix()
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            values.append((relative, "dir", stat.S_IMODE(mode), None))
        elif stat.S_ISLNK(mode):
            values.append((relative, "symlink", stat.S_IMODE(mode), os.readlink(path)))
        else:
            values.append(
                (relative, "file", stat.S_IMODE(mode), digest(path.read_bytes()))
            )
    return values


def mutate_receipt(
    source: Path,
    destination: Path,
    action: Callable[[dict[str, Any]], None],
) -> str:
    shutil.copytree(source, destination)
    value = load(destination / "receipt.json")
    action(value)
    store(destination / "receipt.json", value)
    return digest((destination / "receipt.json").read_bytes())


def first_receipt_record(receipt: dict[str, Any]) -> dict[str, Any]:
    records = receipt["records"]
    require(isinstance(records, list) and bool(records), "mutant receipt lacks records")
    return records[0]


def semantic_receipt_mutants(
    root: Path,
    overlay: Path,
    receipt_root: Path,
) -> int:
    controls = 0

    def expect_mutant(name: str, action: Callable[[dict[str, Any]], None]) -> None:
        nonlocal controls
        mutant = root / name
        mutant_sha = mutate_receipt(receipt_root, mutant, action)
        require_rejected(
            replay_command(overlay, mutant, mutant_sha),
            f"semantic receipt mutant {name}",
        )
        controls += 1

    expect_mutant(
        "receipt-two-replays",
        lambda value: first_receipt_record(value)["subjectReplays"].pop(),
    )

    def diverge_environment(value: dict[str, Any]) -> None:
        replay = first_receipt_record(value)["subjectReplays"][1]
        replay["environment"]["HOME"] += "-drift"
        replay["environmentSha256"] = digest(canonical(replay["environment"]))
        replay["privateDirectory"]["home"] = replay["environment"]["HOME"]

    expect_mutant("receipt-divergent-replay", diverge_environment)

    def remove_clang(value: dict[str, Any]) -> None:
        del first_receipt_record(value)["referenceAcceptance"]["clang"]

    expect_mutant("receipt-one-oracle", remove_clang)

    def alias_compilers(value: dict[str, Any]) -> None:
        references = first_receipt_record(value)["referenceAcceptance"]
        gcc = references["gcc"]
        clang = references["clang"]
        for name in ("compilerPath", "compilerSha256", "compilerVersion"):
            clang[name] = gcc[name]

    expect_mutant("receipt-aliased-oracles", alias_compilers)

    def omit_atom(value: dict[str, Any]) -> None:
        minimality = first_receipt_record(value)["minimality"]
        minimality["atoms"].pop()
        ids = [item["atomId"] for item in minimality["atoms"]]
        minimality["totalAtoms"] = len(ids)
        minimality["testedAtoms"] = len(ids)
        minimality["atomOrderSha256"] = digest(canonical(ids))

    expect_mutant("receipt-omitted-atom", omit_atom)

    def duplicate_atom(value: dict[str, Any]) -> None:
        minimality = first_receipt_record(value)["minimality"]
        minimality["atoms"][1] = copy.deepcopy(minimality["atoms"][0])
        ids = [item["atomId"] for item in minimality["atoms"]]
        minimality["atomOrderSha256"] = digest(canonical(ids))

    expect_mutant("receipt-duplicated-atom", duplicate_atom)

    def skip_oracle(value: dict[str, Any]) -> None:
        del first_receipt_record(value)["minimality"]["atoms"][0][
            "referenceAcceptance"
        ]["clang"]

    expect_mutant("receipt-skipped-deletion-oracle", skip_oracle)
    return controls


def overlay_input_mutants(
    root: Path,
    overlay: Path,
    receipt_root: Path,
    receipt_sha256: str,
) -> int:
    controls = 0
    forged = root / "forged-state-overlay"
    shutil.copytree(overlay, forged)
    forged_manifest = load(forged / "overlay.json")
    forged_manifest["records"][0]["state"] = "VERIFIED"
    store(forged / "overlay.json", forged_manifest)
    forged_receipt = root / "forged-state-receipt"
    require_rejected(
        verify_command(forged, forged_receipt), "forged input VERIFIED", forged_receipt
    )
    controls += 1

    substituted = root / "substituted-id-overlay"
    shutil.copytree(overlay, substituted)
    substituted_manifest = load(substituted / "overlay.json")
    substituted_manifest["records"][0]["failureId"] = "0" * 64
    store(substituted / "overlay.json", substituted_manifest)
    substituted_receipt = root / "substituted-id-receipt"
    require_rejected(
        verify_command(substituted, substituted_receipt),
        "substituted pilot ID",
        substituted_receipt,
    )
    controls += 1

    candidate_drift = root / "candidate-drift-overlay"
    shutil.copytree(overlay, candidate_drift)
    drift_manifest = load(candidate_drift / "overlay.json")
    parser_record = next(
        item
        for item in drift_manifest["records"]
        if item["failureId"] == E1002_FAILURE_ID
    )
    candidate_path = candidate_drift / parser_record["candidate"]["path"]
    candidate_path.write_bytes(candidate_path.read_bytes() + b"\n")
    parser_record["candidate"]["bytes"] += 1
    parser_record["candidate"]["sha256"] = digest(candidate_path.read_bytes())
    parser_record["candidate"]["lineage"].append(
        {
            "candidateLine": 2,
            "seedLine": 200,
            "seedLineSha256": "0" * 64,
        }
    )
    store(candidate_drift / "overlay.json", drift_manifest)
    require_rejected(
        replay_command(candidate_drift, receipt_root, receipt_sha256),
        "candidate-after-receipt drift",
    )
    controls += 1
    return controls


def nonminimal_overlay(root: Path, overlay: Path) -> Path:
    mutant = root / "nonminimal-overlay"
    shutil.copytree(overlay, mutant)
    manifest = load(mutant / "overlay.json")
    record = next(
        item for item in manifest["records"] if item["failureId"] == E1001_FAILURE_ID
    )
    source_record = next(
        value
        for value in parse_results(EVIDENCE / "results.jsonl")
        if value.get("outcome") == "failure"
        and obligation(value)["failureId"] == E1001_FAILURE_ID
    )
    seed = safe_artifact(EVIDENCE, "parser", source_record["artifacts"]["parserInput"])
    seed_data = seed.read_bytes()
    selected = [44, 47, 48, 50]
    data = select_lines(seed_data, selected, "nonminimal E1001 candidate")
    path = mutant / record["candidate"]["path"]
    path.write_bytes(data)
    record["candidate"].update(
        {
            "bytes": len(data),
            "lineage": lineage(seed_data, selected),
            "sha256": digest(data),
        }
    )
    store(mutant / "overlay.json", manifest)
    return mutant


def receipt_integrity_mutants(
    root: Path,
    overlay: Path,
    receipt_root: Path,
    receipt_sha256: str,
) -> int:
    controls = 0
    wrong = replay_command(overlay, receipt_root, "0" * 64)
    require_rejected(wrong, "wrong expected receipt SHA")
    controls += 1

    missing = replay_command(overlay, receipt_root, None)
    require_rejected(missing, "missing expected receipt SHA")
    controls += 1

    byte_mutant = root / "receipt-byte-mutant"
    shutil.copytree(receipt_root, byte_mutant)
    artifact = next(
        path
        for path in sorted(byte_mutant.rglob("*"), key=lambda item: item.as_posix())
        if path.is_file() and path.name != "receipt.json"
    )
    artifact.write_bytes(artifact.read_bytes() + b"drift")
    require_rejected(
        replay_command(overlay, byte_mutant, receipt_sha256), "receipt byte mutation"
    )
    controls += 1

    added = root / "receipt-added-file"
    shutil.copytree(receipt_root, added)
    (added / "unlisted").write_bytes(b"unlisted")
    require_rejected(
        replay_command(overlay, added, receipt_sha256), "receipt added file"
    )
    controls += 1

    deleted = root / "receipt-deleted-file"
    shutil.copytree(receipt_root, deleted)
    artifact = next(
        path
        for path in sorted(deleted.rglob("*"), key=lambda item: item.as_posix())
        if path.is_file() and path.name != "receipt.json"
    )
    artifact.unlink()
    require_rejected(
        replay_command(overlay, deleted, receipt_sha256), "receipt deleted file"
    )
    controls += 1

    symlink = root / "receipt-symlink"
    shutil.copytree(receipt_root, symlink)
    (symlink / "forbidden-link").symlink_to("receipt.json")
    require_rejected(
        replay_command(overlay, symlink, receipt_sha256), "receipt symlink"
    )
    controls += 1

    hardlink = root / "receipt-hardlink"
    shutil.copytree(receipt_root, hardlink)
    artifact = next(
        path
        for path in sorted(hardlink.rglob("*"), key=lambda item: item.as_posix())
        if path.is_file() and path.name != "receipt.json"
    )
    os.link(artifact, hardlink / "forbidden-hardlink")
    require_rejected(
        replay_command(overlay, hardlink, receipt_sha256), "receipt hardlink"
    )
    controls += 1
    return controls


def main() -> int:
    try:
        policy, records = audit_authority()
        pilot_contract = audit_pilot_contract(policy, records)
        require(VERIFIER.is_file(), f"production verifier absent: {VERIFIER}")
        with tempfile.TemporaryDirectory(prefix="c30-day5-phase2-pilots-") as directory:
            root = Path(directory)
            overlay, models = build_pilot_overlay(root, policy, records)
            audit_contract_atoms(pilot_contract, models)
            expected_denominators = {
                failure_id: len(expected_atoms(models[failure_id]))
                for failure_id in PILOT_FAILURE_IDS
            }
            require(
                expected_denominators
                == {RAW_FAILURE_ID: 15, E1001_FAILURE_ID: 3, E1002_FAILURE_ID: 1},
                f"pilot atom denominator drift: {expected_denominators}",
            )
            receipt_root = root / "receipt"
            executed = verify_command(overlay, receipt_root)
            if (
                executed.returncode == 2
                and "invalid choice: 'verify-pilots'" in executed.stderr
            ):
                require(not receipt_root.exists(), "absent command left a receipt")
                print(
                    "DAY5_KERNEL_REDUCTIONS_PHASE2_SELFTEST RED "
                    "command=verify-pilots absent pilots=3 deletion_atoms=19 "
                    f"obligation_manifest_sha256={MANIFEST_SHA256}"
                )
                return 1

            executed_result = parse_success(executed, "pilot execution")
            receipt, receipt_sha256 = validate_receipt(
                receipt_root, overlay, policy, models
            )
            validate_result(
                executed_result,
                "executed",
                policy,
                receipt,
                receipt_sha256,
            )

            before_replay = snapshot(receipt_root)
            replayed = parse_success(
                replay_command(overlay, receipt_root, receipt_sha256),
                "pilot receipt replay",
            )
            require(
                snapshot(receipt_root) == before_replay,
                "receipt replay modified the immutable receipt",
            )
            validate_result(
                replayed,
                "receipt-replay",
                policy,
                receipt,
                receipt_sha256,
            )

            no_overwrite_snapshot = snapshot(receipt_root)
            require_rejected(
                verify_command(overlay, receipt_root), "receipt no-overwrite control"
            )
            require(
                snapshot(receipt_root) == no_overwrite_snapshot,
                "no-overwrite rejection modified the existing receipt",
            )

            controls = 1
            controls += semantic_receipt_mutants(root, overlay, receipt_root)
            controls += overlay_input_mutants(
                root, overlay, receipt_root, receipt_sha256
            )
            controls += receipt_integrity_mutants(
                root, overlay, receipt_root, receipt_sha256
            )

            nonminimal = nonminimal_overlay(root, overlay)
            nonminimal_receipt = root / "nonminimal-receipt"
            require_rejected(
                verify_command(nonminimal, nonminimal_receipt),
                "nonminimal removable-line candidate",
                nonminimal_receipt,
            )
            controls += 1

        print(
            "DAY5_KERNEL_REDUCTIONS_PHASE2_SELFTEST "
            f"pass={controls + 2} fail=0 pilots=3 deletion_atoms=19 "
            "verified_pilots=3 source_failures=46 global_verdict=INCOMPLETE "
            f"obligation_manifest_sha256={MANIFEST_SHA256}"
        )
        return 0
    except (
        TestFailure,
        OSError,
        ValueError,
        json.JSONDecodeError,
        subprocess.SubprocessError,
    ) as error:
        print(f"day5-kernel-reductions-phase2-selftest: FAIL: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
