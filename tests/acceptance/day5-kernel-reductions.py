#!/usr/bin/env python3
"""Audit Day-5 kernel reduction authority without shaping reductions.

Phase 1 implements the immutable source-authority audit and structural
validation of the exact candidate-overlay bijection, bytes, closures, and
deletion-only lineage.  It always returns a machine-readable, fail-closed
``INCOMPLETE`` verdict because subject replay, sealed GCC/Clang acceptance,
minimality sweeps, and their receipts are not implemented yet.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shlex
import shutil
import stat
import subprocess
import sys
import tempfile
import time
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_POLICY = ROOT / "tests/contracts/day5-kernel-reduction-policy.json"
FAILURE_PREFIX = "day5-kernel-reductions: FAIL:"
HEX64 = re.compile(r"^[0-9a-f]{64}$")

ROOT_KEYS = {
    "schemaVersion",
    "policyId",
    "contractState",
    "purpose",
    "sourceEvidence",
    "obligationDerivation",
    "overlayPolicy",
    "replayPolicy",
    "referenceAcceptance",
    "verdictPolicy",
    "integrationPolicy",
    "cli",
}
SOURCE_KEYS = {
    "runId",
    "relativeRoot",
    "expectedObservations",
    "expectedFailures",
    "expectedSuccesses",
    "authorityArtifacts",
    "authorityEqualities",
    "executionAuthority",
}
AUTHORITY_ARTIFACT_KEYS = {
    "summary",
    "results",
    "kernelCohortContract",
    "buildEnvironmentContract",
    "hostBuildAuthority",
    "reconnaissanceRunner",
    "runConfig",
    "kernelInputClosureStart",
    "kernelInputClosureEnd",
    "subjectBuildInputClosureStart",
    "subjectBuildInputClosureEnd",
    "kernelCohortDigestRecord",
    "subjectDigestRecord",
}
EXECUTION_KEYS = {
    "subjectEvidenceRelativePath",
    "subjectSha256",
    "containerCliSha256",
    "cargoSha256",
    "rustcSha256",
    "sealedImageId",
    "rawSeedRoots",
    "referenceCompilers",
}
DERIVATION_KEYS = {
    "canonicalJson",
    "identityObject",
    "failureId",
    "failureSignatureObjectFields",
    "failureSignatureSha256",
    "sourceRecordSha256",
    "obligationManifestSha256",
    "obligationManifestEncoding",
    "obligations",
}
OVERLAY_POLICY_KEYS = {
    "manifestPath",
    "rootExactKeys",
    "recordExactKeys",
    "inputState",
    "verifierOnlyState",
    "oneRecordPerFailureId",
    "candidatePathsUniqueAndConfined",
    "symbolicLinks",
    "hardLinks",
    "overlaySuppliedOracleReplayArgvEnvironmentTimeoutOrReceipt",
    "parserCandidate",
    "rawCandidate",
}
PARSER_CANDIDATE_KEYS = {
    "kind",
    "exactKeys",
    "lineageEntryExactKeys",
    "lineage",
    "minimality",
}
RAW_CANDIDATE_KEYS = {
    "kind",
    "exactKeys",
    "closureExactKeys",
    "fileExactKeys",
    "lineageEntryExactKeys",
    "closureEncoding",
    "bundleEncoding",
    "subjectClosure",
    "referenceClosures",
    "sharedSourceInvariant",
    "rootInvariant",
    "storageLayout",
    "lineage",
    "minimality",
}
REPLAY_KEYS = {
    "baselineReplayCount",
    "requiredStableReplays",
    "parserCommand",
    "rawCommand",
    "timeouts",
    "environment",
    "normalizedObservationFields",
    "originalAnchorFields",
    "stableReplayExactFields",
    "stableReplayRule",
    "sourceFailureReproduction",
    "causalAnchor",
    "minimality",
}
REFERENCE_KEYS = {
    "authority",
    "imageIdentity",
    "compilerIdentity",
    "network",
    "exactCandidateBytes",
    "parserCommands",
    "rawCommands",
    "acceptedReturnCode",
    "timeout",
    "requiredCompilers",
    "receiptFields",
}
VERDICT_KEYS = {
    "phase1AuditSourceMayPass",
    "phase1VerifyOverlayVerdict",
    "phase1AcceptanceValid",
    "verifiedRequiresAll",
    "missingGateVerdict",
    "missingGateAcceptanceValid",
    "verifierMayEmitVerifiedOnlyAfterAllGates",
    "sourceSignatureAloneIsOracle",
}
CLI_KEYS = {
    "auditSource",
    "verifyOverlay",
    "selfTest",
    "contractFailureExitCode",
    "contractFailurePrefix",
}
INTEGRATION_KEYS = {
    "historicalResultsImmutable",
    "verifiedResultsOverlayRequired",
    "reconnaissanceSummaryMayTrustStateString",
    "pendingCountDerivation",
    "forgedVerifiedStateMustRemainPending",
}
RUN_CONFIG_KEYS = {
    "buildEnvironmentContract",
    "generalTimeoutSeconds",
    "kernelCohortContract",
    "parserJobs",
    "parserTimeoutSecondsPerTu",
    "requiredObservationLanes",
    "runner",
    "schemaVersion",
}
REQUIRED_OBSERVATION_LANES = [
    "evocpp-raw",
    "gcc-preprocessed-parser",
    "clang-preprocessed-parser",
]
PILOT_CONTRACT_KEYS = {
    "contractId",
    "contractState",
    "executionIsolation",
    "measurementStatus",
    "minimalityAuthority",
    "observerAuthority",
    "oracleEvidenceAuthority",
    "parentAuthority",
    "pilots",
    "purpose",
    "receiptAuthority",
    "schemaVersion",
    "verdictSemantics",
}
PHASE2_PILOT_FAILURE_IDS = [
    "4835fdc46c9b283c1ce2bc1f5070c92874e2c7c506699555e8797689a83faaff",
    "869b322a04168ffbbac452763bf0081b20c3b69bdea7ffd6fc27bb65c5ff4b5f",
    "ccbede45994dd90ca75222cc689f1e1aad41ba42f5c009ca27710e10a860c796",
]
OBSERVER_AUTHORITY_KEYS = {
    "actorTopology",
    "absenceBehavior",
    "attestationTrust",
    "authorityState",
    "checkpoints",
    "executable",
    "liveMeasurementPermitted",
    "measurementStatus",
    "oracleInputRunCount",
    "oracleRunCount",
    "protocolDeltaFromV1",
    "protocolId",
    "provisional",
    "requiredExecutionBoundary",
    "returnCodeAuthority",
    "schemaVersion",
    "subjectRunCount",
}
PILOT_PARENT_KEYS = {
    "obligationManifestSha256",
    "policyBytes",
    "policyId",
    "policyPath",
    "policySha256",
    "sourceEvidenceRunId",
    "sourceFailures",
}
PILOT_KEYS = {
    "atomCount",
    "candidateSelector",
    "classification",
    "diagnostic",
    "failureId",
    "failureSignatureSha256",
    "lane",
    "minimalityClaim",
    "phase",
    "sourcePath",
    "sourceRecordSha256",
    "tuId",
}
PILOT_RESULT_KEYS = {
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
PILOT_RECEIPT_KEYS = {
    "artifactClosure",
    "executionEvidence",
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
PILOT_RECEIPT_RECORD_KEYS = {
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
    "executionRunId",
    "imageId",
    "inputClosureSha256",
    "returnCode",
    "stderr",
    "stderrSha256",
    "stdout",
    "stdoutSha256",
    "timedOut",
}
EXECUTION_EVIDENCE_KEYS = {
    "aliasCleanup",
    "compilerIdentities",
    "containerCli",
    "counts",
    "preflight",
    "runOrderSha256",
    "runs",
    "schemaVersion",
}
EXECUTION_COUNTS = {
    "baselineAcceptance": 6,
    "compilerIdentity": 2,
    "minimalityDeletion": 38,
    "receiptOracles": 44,
    "total": 46,
}
RUN_EVIDENCE_KEYS = {
    "cleanup",
    "containerAfter",
    "containerBefore",
    "containerName",
    "family",
    "imageAfter",
    "imageBefore",
    "inputClosureSha256",
    "purpose",
    "runId",
}
CLEANUP_EVIDENCE_KEYS = {
    "deleteReturnCode",
    "deleteStderr",
    "deleteStdout",
    "postDeleteInspectReturnCode",
    "postDeleteInspectStderr",
    "postDeleteInspectStdout",
}
MINIMALITY_RECEIPT_KEYS = {
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
EXECUTION_ENVIRONMENT_KEYS = {
    "HOME",
    "LANG",
    "LC_ALL",
    "SOURCE_DATE_EPOCH",
    "TMPDIR",
    "TZ",
}
DIAGNOSTIC_RE = re.compile(
    r"^(?P<path>.+):(?P<line>[0-9]+):(?P<column>[0-9]+): "
    r"error\[(?P<code>[^]]+)\]: (?P<message>.*)$"
)


class ContractError(RuntimeError):
    """An immutable policy, source artifact, or requested verdict is invalid."""


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def exact_keys(value: Any, expected: Iterable[str], label: str) -> dict[str, Any]:
    require(isinstance(value, dict), f"{label} is not an object")
    observed = set(value)
    expected_set = set(expected)
    require(
        observed == expected_set,
        f"{label} keys differ: missing={sorted(expected_set - observed)} extra={sorted(observed - expected_set)}",
    )
    return value


def is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def require_int(value: Any, label: str, *, minimum: int = 0) -> int:
    require(
        is_int(value) and value >= minimum, f"{label} is not an integer >= {minimum}"
    )
    return value


def require_bool(value: Any, label: str) -> bool:
    require(type(value) is bool, f"{label} is not a boolean")
    return value


def require_string(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(value), f"{label} is not a nonempty string")
    return value


def require_hash(value: Any, label: str) -> str:
    require(
        isinstance(value, str) and HEX64.fullmatch(value) is not None,
        f"{label} is not lowercase SHA-256",
    )
    return value


def canonical(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def reject_duplicate_pairs(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    value: dict[str, Any] = {}
    for key, item in pairs:
        if key in value:
            raise ContractError(f"duplicate JSON object key: {key}")
        value[key] = item
    return value


def decode_json(data: bytes, label: str) -> Any:
    try:
        text = data.decode("utf-8", errors="strict")
        return json.loads(text, object_pairs_hook=reject_duplicate_pairs)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot decode JSON {label}: {error}") from error


def load_json(path: Path, label: str | None = None) -> dict[str, Any]:
    try:
        value = decode_json(path.read_bytes(), label or str(path))
    except OSError as error:
        raise ContractError(f"cannot read JSON {label or path}: {error}") from error
    require(isinstance(value, dict), f"JSON root is not an object: {label or path}")
    return value


def safe_relative(value: Any, label: str) -> str:
    text = require_string(value, label)
    require(
        "\\" not in text and "\x00" not in text,
        f"{label} is not a portable relative path",
    )
    pure = PurePosixPath(text)
    require(not pure.is_absolute(), f"{label} is absolute")
    require(
        all(part not in {"", ".", ".."} for part in pure.parts),
        f"{label} contains unsafe components",
    )
    require(pure.as_posix() == text, f"{label} is not canonical POSIX syntax")
    return text


def require_plain_root(path: Path, label: str) -> Path:
    try:
        mode = path.lstat().st_mode
    except OSError as error:
        raise ContractError(f"cannot stat {label}: {error}") from error
    require(
        stat.S_ISDIR(mode) and not stat.S_ISLNK(mode),
        f"{label} is not a plain directory: {path}",
    )
    return path.resolve(strict=True)


def confined_path(
    root: Path, relative: Any, label: str, *, regular_file: bool = True
) -> Path:
    text = safe_relative(relative, label)
    current = root
    for part in PurePosixPath(text).parts:
        current = current / part
        try:
            mode = current.lstat().st_mode
        except OSError as error:
            raise ContractError(f"missing {label}: {current}: {error}") from error
        require(
            not stat.S_ISLNK(mode), f"symbolic link prohibited for {label}: {current}"
        )
    resolved = current.resolve(strict=True)
    require(
        resolved != root and root in resolved.parents,
        f"{label} escapes its authority root",
    )
    if regular_file:
        require(
            stat.S_ISREG(current.lstat().st_mode),
            f"{label} is not a regular file: {current}",
        )
    else:
        require(
            stat.S_ISDIR(current.lstat().st_mode),
            f"{label} is not a directory: {current}",
        )
    return current


def descriptor_schema(value: Any, label: str) -> dict[str, Any]:
    item = exact_keys(value, {"path", "bytes", "sha256"}, label)
    safe_relative(item["path"], f"{label}.path")
    require_int(item["bytes"], f"{label}.bytes")
    require_hash(item["sha256"], f"{label}.sha256")
    return item


def descriptor(path: Path, logical: str) -> dict[str, Any]:
    data = path.read_bytes()
    return {"path": logical, "bytes": len(data), "sha256": digest(data)}


def audit_descriptor(root: Path, expected: Any, label: str) -> Path:
    item = descriptor_schema(expected, label)
    path = confined_path(root, item["path"], label)
    try:
        data = path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read {label}: {error}") from error
    require(len(data) == item["bytes"], f"{label} byte count drift")
    require(digest(data) == item["sha256"], f"{label} SHA-256 drift")
    return path


def require_string_list(value: Any, expected: list[str], label: str) -> None:
    require(
        isinstance(value, list) and all(isinstance(item, str) for item in value),
        f"{label} is not a string list",
    )
    require(value == expected, f"{label} protocol drift")


def validate_policy(policy: dict[str, Any]) -> None:
    exact_keys(policy, ROOT_KEYS, "policy")
    require(policy["schemaVersion"] == 1, "unsupported policy schemaVersion")
    require(
        policy["policyId"] == "c30.day5.kernel-reduction-policy.v1", "policyId drift"
    )
    require(policy["contractState"] == "locked", "policy is not locked")
    require_string(policy["purpose"], "policy.purpose")

    source = exact_keys(policy["sourceEvidence"], SOURCE_KEYS, "policy.sourceEvidence")
    run_id = safe_relative(source["runId"], "sourceEvidence.runId")
    require(
        len(PurePosixPath(run_id).parts) == 1,
        "sourceEvidence.runId is not a safe single component",
    )
    relative_root = safe_relative(source["relativeRoot"], "sourceEvidence.relativeRoot")
    require(
        PurePosixPath(relative_root).name == run_id,
        "sourceEvidence.runId does not match relativeRoot basename",
    )
    observations = require_int(
        source["expectedObservations"], "sourceEvidence.expectedObservations", minimum=1
    )
    failures = require_int(
        source["expectedFailures"], "sourceEvidence.expectedFailures", minimum=1
    )
    successes = require_int(
        source["expectedSuccesses"], "sourceEvidence.expectedSuccesses"
    )
    require(
        observations == failures + successes, "sourceEvidence denominators disagree"
    )
    artifacts = exact_keys(
        source["authorityArtifacts"],
        AUTHORITY_ARTIFACT_KEYS,
        "sourceEvidence.authorityArtifacts",
    )
    paths: list[str] = []
    for name in sorted(AUTHORITY_ARTIFACT_KEYS):
        item = descriptor_schema(
            artifacts[name], f"sourceEvidence.authorityArtifacts.{name}"
        )
        paths.append(item["path"])
    require(len(paths) == len(set(paths)), "authority artifact paths alias")
    equalities = source["authorityEqualities"]
    require(
        equalities
        == [
            ["kernelInputClosureStart", "kernelInputClosureEnd"],
            ["subjectBuildInputClosureStart", "subjectBuildInputClosureEnd"],
        ],
        "authority equality protocol drift",
    )

    execution = exact_keys(
        source["executionAuthority"],
        EXECUTION_KEYS,
        "sourceEvidence.executionAuthority",
    )
    safe_relative(
        execution["subjectEvidenceRelativePath"],
        "executionAuthority.subjectEvidenceRelativePath",
    )
    for name in ("subjectSha256", "containerCliSha256", "cargoSha256", "rustcSha256"):
        require_hash(execution[name], f"executionAuthority.{name}")
    image_id = require_string(
        execution["sealedImageId"], "executionAuthority.sealedImageId"
    )
    require(
        image_id.startswith("sha256:") and HEX64.fullmatch(image_id[7:]) is not None,
        "sealedImageId is malformed",
    )
    seed_roots = exact_keys(
        execution["rawSeedRoots"],
        {"source", "gccOutput", "clangOutput"},
        "executionAuthority.rawSeedRoots",
    )
    for name in ("source", "gccOutput", "clangOutput"):
        seed_root = exact_keys(
            seed_roots[name], {"base", "path"}, f"rawSeedRoots.{name}"
        )
        require(
            seed_root["base"] in {"repository", "evidence"},
            f"rawSeedRoots.{name}.base invalid",
        )
        safe_relative(seed_root["path"], f"rawSeedRoots.{name}.path")
    compilers = exact_keys(
        execution["referenceCompilers"],
        {"gcc", "clang"},
        "executionAuthority.referenceCompilers",
    )
    compiler_hashes: list[str] = []
    for family in ("gcc", "clang"):
        compiler = exact_keys(
            compilers[family],
            {"path", "version", "sha256"},
            f"referenceCompilers.{family}",
        )
        require_string(compiler["path"], f"referenceCompilers.{family}.path")
        require(
            str(compiler["path"]).startswith("/"),
            f"referenceCompilers.{family}.path is not absolute",
        )
        require_string(compiler["version"], f"referenceCompilers.{family}.version")
        compiler_hashes.append(
            require_hash(compiler["sha256"], f"referenceCompilers.{family}.sha256")
        )
    require(len(set(compiler_hashes)) == 2, "reference compiler identities alias")

    derivation = exact_keys(
        policy["obligationDerivation"], DERIVATION_KEYS, "policy.obligationDerivation"
    )
    require(
        derivation["canonicalJson"]
        == "utf8-json-sort-keys-compact-separators-trailing-lf-v1",
        "canonical JSON protocol drift",
    )
    identity_object = exact_keys(
        derivation["identityObject"],
        {"orderedByCanonicalJson", "fields"},
        "identityObject",
    )
    require(
        identity_object["orderedByCanonicalJson"] is True,
        "identity canonical ordering disabled",
    )
    require_string_list(
        identity_object["fields"],
        [
            "classification",
            "diagnostic.code",
            "diagnostic.message",
            "diagnostic.sourcePath",
            "diagnostic.sourceSpan.line",
            "diagnostic.sourceSpan.column",
            "lane",
            "phase",
            "returnCode",
            "sourcePath",
            "timedOut",
            "tuId",
        ],
        "identityObject.fields",
    )
    require_string_list(
        derivation["failureSignatureObjectFields"],
        [
            "classification",
            "diagnostic.code",
            "diagnostic.message",
            "diagnostic.sourcePath",
            "diagnostic.sourceSpan.line",
            "diagnostic.sourceSpan.column",
            "phase",
            "returnCode",
            "timedOut",
        ],
        "failureSignatureObjectFields",
    )
    for name in (
        "failureId",
        "failureSignatureSha256",
        "sourceRecordSha256",
        "obligationManifestEncoding",
    ):
        require_string(derivation[name], f"obligationDerivation.{name}")
    require_hash(derivation["obligationManifestSha256"], "obligationManifestSha256")
    obligations_value = derivation["obligations"]
    require(isinstance(obligations_value, list), "obligations is not a list")
    require(len(obligations_value) == failures, "policy obligation denominator drift")
    obligation_ids: list[str] = []
    for index, value in enumerate(obligations_value):
        obligation_value = exact_keys(
            value,
            {"failureId", "failureSignatureSha256", "sourceRecordSha256"},
            f"obligations[{index}]",
        )
        for name in ("failureId", "failureSignatureSha256", "sourceRecordSha256"):
            require_hash(obligation_value[name], f"obligations[{index}].{name}")
        obligation_ids.append(obligation_value["failureId"])
    require(
        obligation_ids == sorted(obligation_ids),
        "policy obligations are not failureId-sorted",
    )
    require(
        len(obligation_ids) == len(set(obligation_ids)), "policy failure IDs collide"
    )

    overlay = exact_keys(
        policy["overlayPolicy"], OVERLAY_POLICY_KEYS, "policy.overlayPolicy"
    )
    safe_relative(overlay["manifestPath"], "overlayPolicy.manifestPath")
    require_string_list(
        overlay["rootExactKeys"],
        ["policyId", "records", "schemaVersion", "sourceEvidenceRunId"],
        "overlayPolicy.rootExactKeys",
    )
    require_string_list(
        overlay["recordExactKeys"],
        ["candidate", "failureId", "state"],
        "overlayPolicy.recordExactKeys",
    )
    require(overlay["inputState"] == "CANDIDATE", "overlay input state drift")
    require(overlay["verifierOnlyState"] == "VERIFIED", "overlay verifier state drift")
    for name in ("oneRecordPerFailureId", "candidatePathsUniqueAndConfined"):
        require_bool(overlay[name], f"overlayPolicy.{name}")
        require(overlay[name] is True, f"overlayPolicy.{name} disabled")
    require(overlay["symbolicLinks"] == "PROHIBITED", "symbolic-link policy drift")
    require(overlay["hardLinks"] == "PROHIBITED", "hard-link policy drift")
    require(
        overlay["overlaySuppliedOracleReplayArgvEnvironmentTimeoutOrReceipt"]
        == "PROHIBITED",
        "overlay authority injection policy drift",
    )
    parser_candidate = exact_keys(
        overlay["parserCandidate"],
        PARSER_CANDIDATE_KEYS,
        "overlayPolicy.parserCandidate",
    )
    require(
        parser_candidate["kind"] == "preprocessed-parser", "parser candidate kind drift"
    )
    require_string_list(
        parser_candidate["exactKeys"],
        ["bytes", "kind", "lineage", "path", "sha256"],
        "parserCandidate.exactKeys",
    )
    require_string_list(
        parser_candidate["lineageEntryExactKeys"],
        ["candidateLine", "seedLine", "seedLineSha256"],
        "parserCandidate.lineageEntryExactKeys",
    )
    require_string(parser_candidate["lineage"], "parserCandidate.lineage")
    require(
        parser_candidate["minimality"] == "preprocessed-line-1-minimal-v1",
        "parser minimality drift",
    )
    raw_candidate = exact_keys(
        overlay["rawCandidate"], RAW_CANDIDATE_KEYS, "overlayPolicy.rawCandidate"
    )
    require(raw_candidate["kind"] == "raw-closure", "raw candidate kind drift")
    require_string_list(
        raw_candidate["exactKeys"],
        [
            "bundleSha256",
            "kind",
            "referenceClosures",
            "rootSourcePath",
            "subjectClosure",
        ],
        "rawCandidate.exactKeys",
    )
    require_string_list(
        raw_candidate["closureExactKeys"],
        ["closureSha256", "files", "root", "seedFamily"],
        "rawCandidate.closureExactKeys",
    )
    require_string_list(
        raw_candidate["fileExactKeys"],
        ["bytes", "lineage", "logicalPath", "sha256"],
        "rawCandidate.fileExactKeys",
    )
    require_string_list(
        raw_candidate["lineageEntryExactKeys"],
        ["candidateLine", "seedLine", "seedLineSha256"],
        "rawCandidate.lineageEntryExactKeys",
    )
    for name in (
        "closureEncoding",
        "bundleEncoding",
        "subjectClosure",
        "sharedSourceInvariant",
        "rootInvariant",
        "storageLayout",
        "lineage",
    ):
        require_string(raw_candidate[name], f"rawCandidate.{name}")
    reference_closures = exact_keys(
        raw_candidate["referenceClosures"],
        {"gcc", "clang"},
        "rawCandidate.referenceClosures",
    )
    require_string(reference_closures["gcc"], "rawCandidate.referenceClosures.gcc")
    require_string(reference_closures["clang"], "rawCandidate.referenceClosures.clang")
    require(
        "exactly once" in raw_candidate["rootInvariant"],
        "raw root uniqueness is unbound",
    )
    storage_layout = raw_candidate["storageLayout"]
    require(
        all(
            value in storage_layout
            for value in (
                "candidates/F/subject/<logicalPath>",
                "candidates/F/reference-gcc/<logicalPath>",
                "candidates/F/reference-clang/<logicalPath>",
            )
        ),
        "raw candidate storage layout drift",
    )
    require(
        raw_candidate["minimality"] == "raw-closure-file-and-line-1-minimal-v1",
        "raw minimality drift",
    )

    replay = exact_keys(policy["replayPolicy"], REPLAY_KEYS, "policy.replayPolicy")
    require(
        replay["baselineReplayCount"] == 3 and replay["requiredStableReplays"] == 3,
        "stable replay denominator drift",
    )
    require_string_list(
        replay["parserCommand"],
        [
            "<verified-subject>",
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            "<private-candidate>",
        ],
        "replayPolicy.parserCommand",
    )
    require_string(replay["rawCommand"], "replayPolicy.rawCommand")
    exact_keys(replay["timeouts"], {"parser", "raw"}, "replayPolicy.timeouts")
    environment = exact_keys(
        replay["environment"],
        {"LANG", "LC_ALL", "TZ", "SOURCE_DATE_EPOCH", "HOME", "TMPDIR"},
        "replayPolicy.environment",
    )
    require(
        environment["LANG"] == environment["LC_ALL"] == "C"
        and environment["TZ"] == "UTC",
        "replay locale/timezone drift",
    )
    require_string_list(
        replay["normalizedObservationFields"],
        [
            "classification",
            "diagnostic.code",
            "diagnostic.message",
            "diagnostic.sourcePath",
            "diagnostic.sourceSpan.line",
            "diagnostic.sourceSpan.column",
            "phase",
            "returnCode",
            "timedOut",
        ],
        "replayPolicy.normalizedObservationFields",
    )
    require_string_list(
        replay["originalAnchorFields"],
        ["lane", "sourcePath", "tuId"],
        "replayPolicy.originalAnchorFields",
    )
    require_string_list(
        replay["stableReplayExactFields"],
        [
            "argvSha256",
            "candidateDigest",
            "environmentSha256",
            "returnCode",
            "stderrSha256",
            "stdoutSha256",
            "subjectSha256",
            "timedOut",
        ],
        "replayPolicy.stableReplayExactFields",
    )
    stable_rule = require_string(
        replay["stableReplayRule"], "replayPolicy.stableReplayRule"
    )
    require(
        "byte-identical" in stable_rule
        and "normalized equality never substitutes" in stable_rule,
        "stable replay byte authority drift",
    )
    require_string(
        replay["sourceFailureReproduction"], "replayPolicy.sourceFailureReproduction"
    )
    causal = exact_keys(
        replay["causalAnchor"],
        {
            "parser",
            "raw",
            "overlayMaySupplyAnchor",
            "sourceRecordSha256BindsOriginalEmittedFacts",
        },
        "replayPolicy.causalAnchor",
    )
    require(
        "emitted line/column"
        in require_string(causal["parser"], "causalAnchor.parser"),
        "parser causal coordinate is unbound",
    )
    require(
        "seed-closure logical path"
        in require_string(causal["raw"], "causalAnchor.raw"),
        "raw causal coordinate is unbound",
    )
    require(
        causal["overlayMaySupplyAnchor"] is False, "overlay may supply a causal anchor"
    )
    require(
        causal["sourceRecordSha256BindsOriginalEmittedFacts"] is True,
        "source record does not bind emitted facts",
    )
    minimality = require_string(replay["minimality"], "replayPolicy.minimality")
    require(
        "both pinned compilers accept" in minimality,
        "minimality omits the two-oracle predicate",
    )
    require(
        "subject failure but breaks either oracle is not removable" in minimality,
        "minimality misclassifies oracle-invalid deletions",
    )

    reference = exact_keys(
        policy["referenceAcceptance"], REFERENCE_KEYS, "policy.referenceAcceptance"
    )
    for name in (
        "authority",
        "imageIdentity",
        "compilerIdentity",
        "network",
        "timeout",
    ):
        require_string(reference[name], f"referenceAcceptance.{name}")
    require(reference["network"] == "none", "reference network policy drift")
    require(
        reference["exactCandidateBytes"] is True,
        "exact candidate byte binding disabled",
    )
    require(
        reference["acceptedReturnCode"] == 0, "reference accepted return code drift"
    )
    commands = exact_keys(
        reference["parserCommands"],
        {"gcc", "clang"},
        "referenceAcceptance.parserCommands",
    )
    for family in ("gcc", "clang"):
        require(
            isinstance(commands[family], list)
            and all(isinstance(item, str) for item in commands[family]),
            f"parserCommands.{family} malformed",
        )
    raw_commands = exact_keys(
        reference["rawCommands"],
        {"gcc", "clang", "crossFamilyArgvReuse", "acceptedCandidate"},
        "referenceAcceptance.rawCommands",
    )
    require(
        "gcc-lane-1" in require_string(raw_commands["gcc"], "rawCommands.gcc"),
        "raw GCC projection authority drift",
    )
    require(
        "clang-lane-1" in require_string(raw_commands["clang"], "rawCommands.clang"),
        "raw Clang projection authority drift",
    )
    require(
        raw_commands["crossFamilyArgvReuse"] == "PROHIBITED",
        "cross-family raw argv reuse allowed",
    )
    require_string(raw_commands["acceptedCandidate"], "rawCommands.acceptedCandidate")
    require_string_list(
        reference["requiredCompilers"],
        ["gcc", "clang"],
        "referenceAcceptance.requiredCompilers",
    )
    require_string_list(
        reference["receiptFields"],
        [
            "candidateDigest",
            "commandSha256",
            "compilerPath",
            "compilerSha256",
            "compilerVersion",
            "imageId",
            "returnCode",
            "stderrSha256",
            "stdoutSha256",
            "timedOut",
        ],
        "referenceAcceptance.receiptFields",
    )

    verdict = exact_keys(policy["verdictPolicy"], VERDICT_KEYS, "policy.verdictPolicy")
    require(
        verdict["phase1AuditSourceMayPass"] is True, "phase-1 source audit disabled"
    )
    require(
        verdict["phase1VerifyOverlayVerdict"] == "INCOMPLETE",
        "phase-1 overlay verdict drift",
    )
    require(verdict["phase1AcceptanceValid"] is False, "phase-1 acceptance false-green")
    require(
        isinstance(verdict["verifiedRequiresAll"], list)
        and len(verdict["verifiedRequiresAll"]) == 7,
        "verified gate list malformed",
    )
    require(verdict["missingGateVerdict"] == "INCOMPLETE", "missing-gate verdict drift")
    require(
        verdict["missingGateAcceptanceValid"] is False,
        "missing-gate acceptance false-green",
    )
    require(
        verdict["verifierMayEmitVerifiedOnlyAfterAllGates"] is True,
        "all-gates verdict policy disabled",
    )
    require(
        verdict["sourceSignatureAloneIsOracle"] is False,
        "source signature promoted to oracle",
    )

    integration = exact_keys(
        policy["integrationPolicy"], INTEGRATION_KEYS, "policy.integrationPolicy"
    )
    require(
        integration["historicalResultsImmutable"] is True,
        "historical results may be mutated",
    )
    require(
        integration["verifiedResultsOverlayRequired"] is True,
        "verified overlay is not required",
    )
    require(
        integration["reconnaissanceSummaryMayTrustStateString"] is False,
        "summary may trust a forged state string",
    )
    require_string(
        integration["pendingCountDerivation"],
        "integrationPolicy.pendingCountDerivation",
    )
    require(
        integration["forgedVerifiedStateMustRemainPending"] is True,
        "forged verified state may escape pending",
    )

    cli = exact_keys(policy["cli"], CLI_KEYS, "policy.cli")
    for name in ("auditSource", "verifyOverlay", "selfTest"):
        require_string(cli[name], f"cli.{name}")
    require(cli["contractFailureExitCode"] == 2, "contract failure exit code drift")
    require(
        cli["contractFailurePrefix"] == FAILURE_PREFIX, "contract failure prefix drift"
    )


def identity(record: dict[str, Any]) -> dict[str, Any]:
    diagnostic = record.get("diagnostic")
    require(isinstance(diagnostic, dict), "failure record diagnostic is not an object")
    span = exact_keys(
        diagnostic.get("sourceSpan"),
        {"line", "column"},
        "failure diagnostic.sourceSpan",
    )
    require_int(span["line"], "failure diagnostic.sourceSpan.line", minimum=1)
    require(
        span["column"] is None or (is_int(span["column"]) and span["column"] >= 1),
        "failure diagnostic.sourceSpan.column malformed",
    )
    return {
        "classification": require_string(
            record.get("classification"), "failure classification"
        ),
        "diagnostic": {
            "code": require_string(diagnostic.get("code"), "failure diagnostic.code"),
            "message": require_string(
                diagnostic.get("message"), "failure diagnostic.message"
            ),
            "sourcePath": require_string(
                diagnostic.get("sourcePath"), "failure diagnostic.sourcePath"
            ),
            "sourceSpan": {"line": span["line"], "column": span["column"]},
        },
        "lane": require_string(record.get("lane"), "failure lane"),
        "phase": require_string(record.get("phase"), "failure phase"),
        "returnCode": require_int(
            record.get("returnCode"), "failure returnCode", minimum=0
        ),
        "sourcePath": require_string(record.get("sourcePath"), "failure sourcePath"),
        "timedOut": require_bool(record.get("timedOut"), "failure timedOut"),
        "tuId": require_string(record.get("tuId"), "failure tuId"),
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


def derive_obligations(records: list[dict[str, Any]]) -> list[dict[str, str]]:
    values = [
        obligation(record) for record in records if record.get("outcome") == "failure"
    ]
    return sorted(values, key=lambda value: value["failureId"])


def obligation_manifest(values: list[dict[str, str]]) -> str:
    return digest(b"".join(canonical(value) for value in values))


def bound_evidence_root(
    policy_path: Path, policy: dict[str, Any], requested: Path
) -> Path:
    root = require_plain_root(requested, "source evidence root")
    run_id = policy["sourceEvidence"]["runId"]
    require(
        root.name == run_id,
        "requested source evidence basename does not match sourceEvidence.runId",
    )
    relative = policy["sourceEvidence"]["relativeRoot"]
    candidates: list[Path] = []
    for base in (ROOT, policy_path.resolve(strict=True).parent):
        candidate = base / relative
        try:
            candidates.append(candidate.resolve(strict=True))
        except OSError:
            pass
    require(
        root in candidates,
        "requested source evidence does not match policy.relativeRoot",
    )
    return root


def load_results(path: Path) -> list[dict[str, Any]]:
    try:
        raw = path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read results.jsonl: {error}") from error
    require(
        bool(raw) and raw.endswith(b"\n"), "results.jsonl is empty or lacks trailing LF"
    )
    records: list[dict[str, Any]] = []
    for number, line in enumerate(raw.splitlines(keepends=True), start=1):
        require(
            line.endswith(b"\n") and line != b"\n",
            f"results.jsonl line {number} is blank or unterminated",
        )
        value = decode_json(line, f"results.jsonl:{number}")
        require(isinstance(value, dict), f"results.jsonl:{number} is not an object")
        require(
            line == canonical(value), f"results.jsonl:{number} is not canonical JSON"
        )
        records.append(value)
    return records


def audit_record_shape(record: dict[str, Any], number: int) -> None:
    label = f"results[{number}]"
    required = {
        "artifacts",
        "classification",
        "diagnostic",
        "lane",
        "outcome",
        "phase",
        "returnCode",
        "sourcePath",
        "timedOut",
        "tuId",
    }
    require(
        required <= set(record),
        f"{label} lacks required keys: {sorted(required - set(record))}",
    )
    require(
        isinstance(record["artifacts"], dict), f"{label}.artifacts is not an object"
    )
    require(
        record["lane"]
        in {"evocpp-raw", "gcc-preprocessed-parser", "clang-preprocessed-parser"},
        f"{label}.lane invalid",
    )
    require_string(record["phase"], f"{label}.phase")
    require_string(record["sourcePath"], f"{label}.sourcePath")
    require_string(record["tuId"], f"{label}.tuId")
    require_int(record["returnCode"], f"{label}.returnCode")
    require_bool(record["timedOut"], f"{label}.timedOut")
    require(record["outcome"] in {"success", "failure"}, f"{label}.outcome invalid")
    expected_phase = (
        "raw-source-preprocess"
        if record["lane"] == "evocpp-raw"
        else "oracle-preprocessed-parse"
    )
    require(
        record["phase"] == expected_phase,
        f"{label} lane/phase authority drift",
    )
    if record["outcome"] == "failure":
        require(
            record["returnCode"] > 0 and record["timedOut"] is False,
            f"{label} reducible failure must be non-timeout with positive returnCode",
        )
        identity(record)
        diagnostic = record["diagnostic"]
        require_string(diagnostic.get("emittedPath"), f"{label}.diagnostic.emittedPath")
        emitted_span = exact_keys(
            diagnostic.get("emittedSpan"),
            {"line", "column"},
            f"{label}.diagnostic.emittedSpan",
        )
        require_int(
            emitted_span["line"], f"{label}.diagnostic.emittedSpan.line", minimum=1
        )
        require_int(
            emitted_span["column"], f"{label}.diagnostic.emittedSpan.column", minimum=1
        )
    else:
        require(
            record["classification"] is None and record["diagnostic"] is None,
            f"{label} success carries failure data",
        )
        require(
            record["returnCode"] == 0 and record["timedOut"] is False,
            f"{label} success must be non-timeout with zero returnCode",
        )
    expected_artifacts = (
        {"argv", "stdout", "stderr"}
        if record["lane"] == "evocpp-raw"
        else {
            "compilerStderr",
            "compilerStdout",
            "lineMap",
            "oraclePreprocessed",
            "parserInput",
            "stderr",
            "stdout",
        }
    )
    require(
        set(record["artifacts"]) == expected_artifacts,
        f"{label}.artifacts keys differ: missing={sorted(expected_artifacts - set(record['artifacts']))} "
        f"extra={sorted(set(record['artifacts']) - expected_artifacts)}",
    )
    for name, artifact in record["artifacts"].items():
        require_string(name, f"{label}.artifact name")
        descriptor_schema(artifact, f"{label}.artifacts.{name}")


def record_artifact_root(evidence: Path, record: dict[str, Any], name: str) -> Path:
    lane = record["lane"]
    if lane == "evocpp-raw":
        require(
            name in {"argv", "stdout", "stderr"},
            f"unexpected raw record artifact: {name}",
        )
        return evidence / "evocpp-raw"
    require(
        name
        in {
            "compilerStderr",
            "compilerStdout",
            "lineMap",
            "oraclePreprocessed",
            "parserInput",
            "stderr",
            "stdout",
        },
        f"unexpected parser record artifact: {name}",
    )
    if name in {"compilerStderr", "compilerStdout", "oraclePreprocessed"}:
        family = lane.removesuffix("-preprocessed-parser")
        return evidence / f"oracle-{family}"
    return evidence / "parser"


def audit_record_artifacts(evidence: Path, records: list[dict[str, Any]]) -> None:
    for number, record in enumerate(records):
        for name, expected in record["artifacts"].items():
            root = record_artifact_root(evidence, record, name)
            require_plain_root(
                root, f"record artifact root for {record['tuId']}:{name}"
            )
            audit_descriptor(
                root.resolve(strict=True),
                expected,
                f"results[{number}].artifacts.{name}",
            )


def audit_summary(
    summary: dict[str, Any], source: dict[str, Any], artifacts: dict[str, Any]
) -> None:
    mappings = {
        "requiredRecords": source["expectedObservations"],
        "observedRecords": source["expectedObservations"],
        "failures": source["expectedFailures"],
        "successes": source["expectedSuccesses"],
        "pendingReductions": source["expectedFailures"],
    }
    for name, expected in mappings.items():
        require(summary.get(name) == expected, f"summary.{name} denominator drift")
    require(
        summary.get("contractSha256") == artifacts["kernelCohortContract"]["sha256"],
        "summary contract digest drift",
    )
    require(
        summary.get("resultsJsonl") == artifacts["results"],
        "summary results descriptor drift",
    )


def parse_closure_rows(
    data: bytes, label: str, *, input_paths: bool = False
) -> list[tuple[str, str, int]]:
    require(
        bool(data) and data.endswith(b"\n"), f"{label} is empty or lacks trailing LF"
    )
    rows: list[tuple[str, str, int]] = []
    for number, raw_line in enumerate(data.splitlines(), start=1):
        try:
            line = raw_line.decode("utf-8", errors="strict")
        except UnicodeDecodeError as error:
            raise ContractError(f"{label}:{number} is not UTF-8: {error}") from error
        fields = line.split("\t")
        require(len(fields) == 3, f"{label}:{number} does not have three TSV fields")
        if input_paths:
            logical = require_string(fields[0], f"{label}:{number} logical path")
            require(
                "\\" not in logical and "\x00" not in logical,
                f"{label}:{number} logical path is not portable",
            )
            parts = logical.split("/")
            require(
                len(parts) >= 2 and parts[0] in {"source", "output"},
                f"{label}:{number} logical path has no source/output root",
            )
            depth = 0
            for part in parts[1:]:
                require(
                    part not in {"", "."},
                    f"{label}:{number} logical path has an empty/dot component",
                )
                if part == "..":
                    require(
                        depth > 0,
                        f"{label}:{number} logical path escapes its source/output root",
                    )
                    depth -= 1
                else:
                    depth += 1
        else:
            logical = safe_relative(fields[0], f"{label}:{number} logical path")
        sha256 = require_hash(fields[1], f"{label}:{number} SHA-256")
        require(
            fields[2].isdigit() and (fields[2] == "0" or not fields[2].startswith("0")),
            f"{label}:{number} byte count is not canonical",
        )
        byte_count = int(fields[2])
        rows.append((logical, sha256, byte_count))
    logicals = [row[0] for row in rows]
    require(logicals == sorted(logicals), f"{label} rows are not logical-path sorted")
    require(len(logicals) == len(set(logicals)), f"{label} has duplicate logical paths")
    return rows


def audit_closure_directory(
    root: Path, rows: list[tuple[str, str, int]], label: str
) -> None:
    resolved = require_plain_root(root, label)
    expected = {logical: (sha256, byte_count) for logical, sha256, byte_count in rows}
    observed: dict[str, Path] = {}
    for entry in root.iterdir():
        mode = entry.lstat().st_mode
        require(not stat.S_ISLNK(mode), f"symbolic link prohibited in {label}: {entry}")
        require(
            stat.S_ISREG(mode),
            f"nested or non-regular entry prohibited in {label}: {entry}",
        )
        observed[entry.name] = entry
    require(
        set(observed) == set(expected),
        f"{label} file set differs from its closure manifest",
    )
    for logical, (expected_sha, expected_bytes) in expected.items():
        path = confined_path(resolved, logical, f"{label}/{logical}")
        data = path.read_bytes()
        require(len(data) == expected_bytes, f"{label}/{logical} byte count drift")
        require(digest(data) == expected_sha, f"{label}/{logical} SHA-256 drift")


def audit_kernel_input_closures(
    evidence: Path,
    start_path: Path,
    end_path: Path,
    records: list[dict[str, Any]],
) -> None:
    start_rows = parse_closure_rows(
        start_path.read_bytes(), "kernel-input-closure.start.tsv"
    )
    end_rows = parse_closure_rows(end_path.read_bytes(), "kernel-input-closure.end.tsv")
    require(
        start_rows == end_rows,
        "kernel input directory closure changed during source run",
    )
    audit_closure_directory(evidence / "inputs", start_rows, "source inputs directory")
    audit_closure_directory(
        evidence / "inputs-end", end_rows, "source inputs-end directory"
    )

    source_by_tu: dict[str, str] = {}
    for record in records:
        prior = source_by_tu.setdefault(record["tuId"], record["sourcePath"])
        require(
            prior == record["sourcePath"],
            f"TU sourcePath drift across lanes: {record['tuId']}",
        )
    expected_names = {
        f"{tu_id}.{family}.inputs.tsv"
        for tu_id in source_by_tu
        for family in ("gcc", "clang")
    }
    require(
        {row[0] for row in start_rows} == expected_names,
        "per-TU family input closure set is incomplete",
    )
    inputs_root = (evidence / "inputs").resolve(strict=True)
    end_root = (evidence / "inputs-end").resolve(strict=True)
    for tu_id, source_path in sorted(source_by_tu.items()):
        family_rows: dict[str, dict[str, tuple[str, int]]] = {}
        for family in ("gcc", "clang"):
            name = f"{tu_id}.{family}.inputs.tsv"
            start_file = confined_path(inputs_root, name, f"inputs/{name}")
            end_file = confined_path(end_root, name, f"inputs-end/{name}")
            require(
                start_file.read_bytes() == end_file.read_bytes(),
                f"per-TU input closure changed: {name}",
            )
            rows = parse_closure_rows(
                start_file.read_bytes(), f"inputs/{name}", input_paths=True
            )
            row_map = {
                logical: (sha256, byte_count) for logical, sha256, byte_count in rows
            }
            root_logical = f"source/{source_path}"
            require(
                root_logical in row_map,
                f"per-TU input closure lacks root source: {name}",
            )
            family_rows[family] = row_map
        common_source = {
            logical
            for logical in family_rows["gcc"]
            if logical.startswith("source/") and logical in family_rows["clang"]
        }
        for logical in common_source:
            require(
                family_rows["gcc"][logical] == family_rows["clang"][logical],
                f"cross-family source seed bytes disagree: {tu_id}:{logical}",
            )


def audit_cohort_bindings(
    cohort_path: Path,
    evidence: Path,
    records: list[dict[str, Any]],
    expected_observations: int,
) -> None:
    cohort = load_json(cohort_path, "kernel-cohort.json")
    require(cohort.get("schemaVersion") == 1, "kernel cohort schemaVersion drift")
    require(cohort.get("contractState") == "locked", "kernel cohort is not locked")
    units_value = cohort.get("translationUnits")
    require(
        isinstance(units_value, list) and bool(units_value),
        "kernel cohort translationUnits is empty or absent",
    )
    require(
        expected_observations == len(units_value) * 3,
        "kernel cohort/result denominator drift",
    )
    units: dict[str, dict[str, Any]] = {}
    for index, value in enumerate(units_value):
        require(
            isinstance(value, dict),
            f"kernel cohort translationUnits[{index}] is not an object",
        )
        tu_id = require_string(value.get("id"), f"translationUnits[{index}].id")
        require(tu_id not in units, f"kernel cohort duplicates TU id: {tu_id}")
        require_string(value.get("sourcePath"), f"translationUnits[{index}].sourcePath")
        require_hash(
            value.get("sourceSha256"), f"translationUnits[{index}].sourceSha256"
        )
        require_int(
            value.get("sourceBytes"),
            f"translationUnits[{index}].sourceBytes",
            minimum=1,
        )
        commands = exact_keys(
            value.get("commands"),
            {"gcc", "clang"},
            f"translationUnits[{index}].commands",
        )
        for family in ("gcc", "clang"):
            require(
                isinstance(commands[family], dict),
                f"translationUnits[{index}].commands.{family} is not an object",
            )
            closure = exact_keys(
                commands[family].get("inputClosure"),
                {"files", "bytes", "sha256"},
                f"translationUnits[{index}].commands.{family}.inputClosure",
            )
            require_int(
                closure["files"],
                f"translationUnits[{index}].commands.{family}.inputClosure.files",
                minimum=1,
            )
            require_int(
                closure["bytes"],
                f"translationUnits[{index}].commands.{family}.inputClosure.bytes",
                minimum=1,
            )
            require_hash(
                closure["sha256"],
                f"translationUnits[{index}].commands.{family}.inputClosure.sha256",
            )
        units[tu_id] = value

    required_lanes = {
        "evocpp-raw",
        "gcc-preprocessed-parser",
        "clang-preprocessed-parser",
    }
    observed_anchors = {(record["tuId"], record["lane"]) for record in records}
    expected_anchors = {(tu_id, lane) for tu_id in units for lane in required_lanes}
    require(
        observed_anchors == expected_anchors,
        "results do not form the exact cohort TU-by-lane matrix",
    )
    for record in records:
        require(
            record["sourcePath"] == units[record["tuId"]]["sourcePath"],
            f"results sourcePath disagrees with cohort: {record['tuId']}:{record['lane']}",
        )

    inputs_root = require_plain_root(evidence / "inputs", "source inputs directory")
    for tu_id, unit in sorted(units.items()):
        root_logical = f"source/{unit['sourcePath']}"
        for family in ("gcc", "clang"):
            name = f"{tu_id}.{family}.inputs.tsv"
            path = confined_path(inputs_root, name, f"inputs/{name}")
            data = path.read_bytes()
            rows = parse_closure_rows(data, f"inputs/{name}", input_paths=True)
            authority = unit["commands"][family]["inputClosure"]
            require(
                digest(data) == authority["sha256"],
                f"cohort input closure SHA-256 drift: {tu_id}:{family}",
            )
            require(
                len(rows) == authority["files"],
                f"cohort input closure file count drift: {tu_id}:{family}",
            )
            require(
                sum(row[2] for row in rows) == authority["bytes"],
                f"cohort input closure byte count drift: {tu_id}:{family}",
            )
            row_map = {
                logical: (sha256, byte_count) for logical, sha256, byte_count in rows
            }
            require(
                root_logical in row_map,
                f"cohort root source absent from input closure: {tu_id}:{family}",
            )
            require(
                row_map[root_logical] == (unit["sourceSha256"], unit["sourceBytes"]),
                f"cohort root source identity drift: {tu_id}:{family}",
            )


def audit_execution_authority(
    policy: dict[str, Any], build_environment_path: Path, host_authority_path: Path
) -> None:
    execution = policy["sourceEvidence"]["executionAuthority"]
    build_environment = load_json(build_environment_path, "build-environment.json")
    host_authority = load_json(host_authority_path, "host-build-authority.json")

    require(
        build_environment.get("schemaVersion") == 1,
        "build-environment schemaVersion drift",
    )
    require(
        build_environment.get("contractState") == "locked",
        "build-environment is not locked",
    )
    require(
        host_authority.get("schemaVersion") == 1,
        "host-build-authority schemaVersion drift",
    )
    build_image = build_environment.get("buildImage")
    require(isinstance(build_image, dict), "build-environment buildImage is absent")
    for field in ("sealedImageId", "observedImageId", "executionReference"):
        require(
            build_image.get(field) == execution["sealedImageId"],
            f"sealed image authority drift: buildImage.{field}",
        )

    toolchain = build_environment.get("toolchain")
    require(isinstance(toolchain, dict), "build-environment toolchain is absent")
    for family in ("gcc", "clang"):
        observed = toolchain.get(family)
        require(
            isinstance(observed, dict),
            f"{family.upper()} reference compiler authority is absent",
        )
        expected = execution["referenceCompilers"][family]
        for policy_field, build_field in (
            ("path", "binaryPathInImage"),
            ("version", "resolvedVersion"),
            ("sha256", "binarySha256"),
        ):
            require(
                observed.get(build_field) == expected[policy_field],
                f"{family.upper()} reference compiler authority drift: {build_field}",
            )

    host_tools = host_authority.get("tools")
    require(isinstance(host_tools, dict), "host build tools authority is absent")
    container_runtime = build_environment.get("containerRuntime")
    require(
        isinstance(container_runtime, dict),
        "build-environment containerRuntime is absent",
    )
    container_tool = host_tools.get("container")
    require(isinstance(container_tool, dict), "host container tool authority is absent")
    for observed, label in (
        (container_runtime.get("cliSha256"), "containerRuntime.cliSha256"),
        (container_tool.get("sha256"), "host tools.container.sha256"),
    ):
        require(
            observed == execution["containerCliSha256"],
            f"container CLI authority drift: {label}",
        )

    host_replay = toolchain.get("hostReplay")
    require(isinstance(host_replay, dict), "build-environment hostReplay is absent")
    rust_toolchain = toolchain.get("rust")
    require(
        isinstance(rust_toolchain, dict), "build-environment Rust toolchain is absent"
    )
    replay_hashes = {
        "cargo": execution["cargoSha256"],
        "rustc": execution["rustcSha256"],
    }
    for tool, expected_hash in replay_hashes.items():
        replay_tool = host_replay.get(tool)
        host_tool = host_tools.get(tool)
        require(
            isinstance(replay_tool, dict),
            f"build-environment hostReplay.{tool} is absent",
        )
        require(isinstance(host_tool, dict), f"host tools.{tool} is absent")
        require(
            replay_tool.get("sha256") == expected_hash,
            f"{tool} replay authority drift: toolchain.hostReplay.{tool}.sha256",
        )
        require(
            host_tool.get("sha256") == expected_hash,
            f"{tool} replay authority drift: host tools.{tool}.sha256",
        )
    require(
        rust_toolchain.get("rustcSha256") == execution["rustcSha256"],
        "rustc replay authority drift: toolchain.rust.rustcSha256",
    )


def audit_run_config(
    run_config_path: Path, expected_artifacts: dict[str, dict[str, Any]]
) -> None:
    run_config = exact_keys(
        load_json(run_config_path, "run-config.json"),
        RUN_CONFIG_KEYS,
        "run-config",
    )
    require(run_config["schemaVersion"] == 1, "run-config schemaVersion drift")
    require_int(
        run_config["generalTimeoutSeconds"], "run-config general timeout", minimum=1
    )
    require_int(
        run_config["parserTimeoutSecondsPerTu"],
        "run-config parser timeout",
        minimum=1,
    )
    require_int(run_config["parserJobs"], "run-config parserJobs", minimum=1)
    require(
        run_config["requiredObservationLanes"] == REQUIRED_OBSERVATION_LANES,
        "run-config requiredObservationLanes drift",
    )
    for run_name, artifact_name in (
        ("kernelCohortContract", "kernelCohortContract"),
        ("buildEnvironmentContract", "buildEnvironmentContract"),
        ("runner", "reconnaissanceRunner"),
    ):
        require(
            run_config[run_name] == expected_artifacts[artifact_name],
            f"run-config {run_name} descriptor drift",
        )


def audit_source(
    policy_path: Path, requested_evidence: Path, *, audit_record_files: bool = True
) -> dict[str, Any]:
    try:
        policy_bytes = policy_path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read JSON policy: {error}") from error
    policy = decode_json(policy_bytes, "policy")
    require(isinstance(policy, dict), "JSON root is not an object: policy")
    policy_sha256 = digest(policy_bytes)
    validate_policy(policy)
    source = policy["sourceEvidence"]
    evidence = bound_evidence_root(policy_path, policy, requested_evidence)
    expected_artifacts = source["authorityArtifacts"]
    artifact_paths: dict[str, Path] = {}
    for name in sorted(AUTHORITY_ARTIFACT_KEYS):
        artifact_paths[name] = audit_descriptor(
            evidence, expected_artifacts[name], f"authorityArtifacts.{name}"
        )

    for left, right in source["authorityEqualities"]:
        require(
            artifact_paths[left].read_bytes() == artifact_paths[right].read_bytes(),
            f"authority equality drift: {left}!={right}",
        )
        require(
            expected_artifacts[left]["sha256"] == expected_artifacts[right]["sha256"],
            f"authority equality descriptor drift: {left}!={right}",
        )

    cohort_digest = artifact_paths["kernelCohortDigestRecord"].read_bytes()
    require(
        cohort_digest
        == (expected_artifacts["kernelCohortContract"]["sha256"] + "\n").encode(
            "ascii"
        ),
        "kernel cohort digest record drift",
    )
    execution = source["executionAuthority"]
    subject_digest = artifact_paths["subjectDigestRecord"].read_bytes()
    require(
        subject_digest == (execution["subjectSha256"] + "\n").encode("ascii"),
        "subject digest record drift",
    )

    subject_relative = execution["subjectEvidenceRelativePath"]
    subject_path = confined_path(evidence, subject_relative, "verified subject")
    require(os.access(subject_path, os.X_OK), "verified subject is not executable")
    require(
        digest(subject_path.read_bytes()) == execution["subjectSha256"],
        "verified subject SHA-256 drift",
    )
    audit_execution_authority(
        policy,
        artifact_paths["buildEnvironmentContract"],
        artifact_paths["hostBuildAuthority"],
    )

    summary = load_json(artifact_paths["summary"], "summary.json")
    audit_summary(summary, source, expected_artifacts)
    audit_run_config(artifact_paths["runConfig"], expected_artifacts)

    records = load_results(artifact_paths["results"])
    for number, record in enumerate(records):
        audit_record_shape(record, number)
    keys = [(record["tuId"], record["lane"]) for record in records]
    require(len(keys) == len(set(keys)), "duplicate source observation anchor")
    failures = [record for record in records if record["outcome"] == "failure"]
    successes = [record for record in records if record["outcome"] == "success"]
    require(
        len(records) == source["expectedObservations"],
        "source observation denominator drift",
    )
    require(
        len(failures) == source["expectedFailures"], "source failure denominator drift"
    )
    require(
        len(successes) == source["expectedSuccesses"],
        "source success denominator drift",
    )
    audit_kernel_input_closures(
        evidence,
        artifact_paths["kernelInputClosureStart"],
        artifact_paths["kernelInputClosureEnd"],
        records,
    )
    audit_cohort_bindings(
        artifact_paths["kernelCohortContract"],
        evidence,
        records,
        source["expectedObservations"],
    )
    audit_raw_seed_authority(policy, evidence, records)
    if audit_record_files:
        audit_record_artifacts(evidence, records)

    derived = derive_obligations(records)
    require(
        len({value["failureId"] for value in derived}) == len(derived),
        "derived failure IDs collide",
    )
    authority = policy["obligationDerivation"]
    require(derived == authority["obligations"], "source failure obligations drift")
    manifest = obligation_manifest(derived)
    require(
        manifest == authority["obligationManifestSha256"],
        "source obligation manifest drift",
    )
    return {
        "policy": policy,
        "policyBytes": len(policy_bytes),
        "policyPath": policy_path.resolve(strict=True),
        "policySha256": policy_sha256,
        "evidence": evidence,
        "records": records,
        "failures": failures,
        "manifestSha256": manifest,
        "sourceEvidenceRunId": source["runId"],
    }


def load_bound_json(path: Path, label: str) -> tuple[dict[str, Any], bytes, str]:
    try:
        data = path.read_bytes()
    except OSError as error:
        raise ContractError(f"cannot read JSON {label}: {error}") from error
    value = decode_json(data, label)
    require(isinstance(value, dict), f"JSON root is not an object: {label}")
    return value, data, digest(data)


def audit_pilot_contract(
    audited: dict[str, Any], pilot_contract_path: Path
) -> dict[str, Any]:
    contract_value, contract_bytes, contract_sha256 = load_bound_json(
        pilot_contract_path, "pilot contract"
    )
    contract = exact_keys(contract_value, PILOT_CONTRACT_KEYS, "pilot contract")
    require(contract["schemaVersion"] == 4, "pilot contract schemaVersion drift")
    require(
        contract["contractId"] == "c30.day5.kernel-reduction-pilots.v4",
        "pilot contractId drift",
    )
    require(contract["contractState"] == "locked", "pilot contract is not locked")
    require(
        contract["measurementStatus"] == "NOT_MEASURED",
        "pilot contract contains measured status",
    )
    purpose = require_string(contract["purpose"], "pilot contract purpose")
    require(
        "no measured outcome" in purpose,
        "pilot contract purpose does not preserve pre-execution authority",
    )

    parent = exact_keys(
        contract["parentAuthority"], PILOT_PARENT_KEYS, "pilot parent authority"
    )
    policy = audited["policy"]
    require(parent["policyId"] == policy["policyId"], "pilot parent policyId drift")
    parent_policy_path = confined_path(
        ROOT,
        parent["policyPath"],
        "pilot parent policyPath",
    ).resolve(strict=True)
    require(
        parent_policy_path == audited["policyPath"],
        "pilot parent policyPath does not identify the audited policy",
    )
    require(
        parent["policyBytes"] == audited["policyBytes"],
        "pilot parent policy byte count drift",
    )
    require(
        parent["policySha256"] == audited["policySha256"],
        "pilot parent policy SHA-256 drift",
    )
    require(
        parent["sourceEvidenceRunId"] == audited["sourceEvidenceRunId"],
        "pilot parent source run drift",
    )
    require(
        parent["sourceFailures"] == len(audited["failures"]),
        "pilot parent failure denominator drift",
    )
    require(
        parent["obligationManifestSha256"] == audited["manifestSha256"],
        "pilot parent obligation manifest drift",
    )

    by_failure_id = record_by_failure_id(audited)
    pilots_value = contract["pilots"]
    require(
        isinstance(pilots_value, list) and len(pilots_value) == 3,
        "pilot contract pilot denominator drift",
    )
    pilots: list[dict[str, Any]] = []
    pilot_ids: list[str] = []
    atom_counts: dict[str, int] = {}
    for index, pilot_value in enumerate(pilots_value):
        pilot = exact_keys(pilot_value, PILOT_KEYS, f"pilots[{index}]")
        failure_id = require_hash(pilot["failureId"], f"pilots[{index}].failureId")
        require(
            failure_id in by_failure_id,
            f"pilot failureId is not an obligation: {failure_id}",
        )
        record = by_failure_id[failure_id]
        derived = obligation(record)
        require(
            pilot["failureSignatureSha256"] == derived["failureSignatureSha256"],
            f"pilot failure signature drift: {failure_id}",
        )
        require(
            pilot["sourceRecordSha256"] == derived["sourceRecordSha256"],
            f"pilot source record drift: {failure_id}",
        )
        for name in ("lane", "tuId", "sourcePath", "phase", "classification"):
            require(
                pilot[name] == record[name],
                f"pilot {name} disagrees with source record: {failure_id}",
            )
        diagnostic = exact_keys(
            pilot["diagnostic"],
            {"code", "emittedSpan", "message", "sourcePath", "sourceSpan"},
            f"pilots[{index}].diagnostic",
        )
        source_diagnostic = record["diagnostic"]
        require(
            diagnostic
            == {
                "code": source_diagnostic["code"],
                "emittedSpan": source_diagnostic["emittedSpan"],
                "message": source_diagnostic["message"],
                "sourcePath": source_diagnostic["sourcePath"],
                "sourceSpan": source_diagnostic["sourceSpan"],
            },
            f"pilot diagnostic drift: {failure_id}",
        )
        selector = pilot["candidateSelector"]
        if record["lane"] == "evocpp-raw":
            raw_selector = exact_keys(
                selector,
                {"kind", "outputFiles", "seedFamilies", "sharedSourceFiles"},
                f"pilots[{index}].candidateSelector",
            )
            require(raw_selector["kind"] == "raw-closure", "raw pilot kind drift")
            require(
                raw_selector["seedFamilies"]
                == {"subject": "gcc", "gcc": "gcc", "clang": "clang"},
                "raw pilot seed families drift",
            )
            require(
                isinstance(raw_selector["outputFiles"], list),
                "raw outputFiles is not a list",
            )
            files = raw_selector["sharedSourceFiles"]
            require(
                isinstance(files, list) and bool(files),
                "raw sharedSourceFiles is empty",
            )
            logicals: list[str] = []
            line_atoms = 0
            for file_index, file_value in enumerate(files):
                item = exact_keys(
                    file_value,
                    {"logicalPath", "seedLines"},
                    f"raw selector file[{file_index}]",
                )
                logical = normalize_closure_logical(
                    item["logicalPath"],
                    f"raw selector file[{file_index}].logicalPath",
                    require_normalized=True,
                )
                require(
                    logical.startswith("source/"),
                    "raw shared selector includes output file",
                )
                lines = item["seedLines"]
                require(
                    isinstance(lines, list)
                    and bool(lines)
                    and all(is_int(line) and line >= 1 for line in lines)
                    and lines == sorted(set(lines)),
                    f"raw selector seedLines invalid: {logical}",
                )
                logicals.append(logical)
                line_atoms += len(lines)
            require(
                logicals == sorted(set(logicals)),
                "raw selector files are not unique/sorted",
            )
            output_files = raw_selector["outputFiles"]
            require(
                all(isinstance(item, dict) for item in output_files),
                "raw outputFiles contains a non-object",
            )
            atom_count = len(files) + len(output_files) + line_atoms
        else:
            parser_selector = exact_keys(
                selector,
                {"kind", "seedArtifact", "seedLines"},
                f"pilots[{index}].candidateSelector",
            )
            require(
                parser_selector["kind"] == "preprocessed-parser"
                and parser_selector["seedArtifact"] == "artifacts.parserInput",
                f"parser pilot selector drift: {failure_id}",
            )
            lines = parser_selector["seedLines"]
            require(
                isinstance(lines, list)
                and bool(lines)
                and all(is_int(line) and line >= 1 for line in lines)
                and lines == sorted(set(lines)),
                f"parser pilot seedLines invalid: {failure_id}",
            )
            atom_count = len(lines)
        require_int(pilot["atomCount"], f"pilots[{index}].atomCount", minimum=1)
        require(
            pilot["atomCount"] == atom_count,
            f"pilot atom denominator drift: {failure_id}",
        )
        require_string(pilot["minimalityClaim"], f"pilots[{index}].minimalityClaim")
        pilots.append(pilot)
        pilot_ids.append(failure_id)
        atom_counts[failure_id] = atom_count
    require(
        pilot_ids == sorted(set(pilot_ids)), "pilot failureIds are not unique/sorted"
    )
    require(
        pilot_ids == PHASE2_PILOT_FAILURE_IDS,
        "pilot failureId set/order drift",
    )

    minimality = exact_keys(
        contract["minimalityAuthority"],
        {"executionRule", "interestingness", "orderedAtoms", "totalAtoms"},
        "pilot minimalityAuthority",
    )
    ordered_atoms = minimality["orderedAtoms"]
    require(isinstance(ordered_atoms, list), "pilot orderedAtoms is not a list")
    observed_counts = {failure_id: 0 for failure_id in pilot_ids}
    for index, value in enumerate(ordered_atoms):
        atom = exact_keys(value, {"atomId", "failureId"}, f"orderedAtoms[{index}]")
        failure_id = require_hash(atom["failureId"], f"orderedAtoms[{index}].failureId")
        require(
            failure_id in observed_counts,
            f"ordered atom has unknown pilot: {failure_id}",
        )
        require_string(atom["atomId"], f"orderedAtoms[{index}].atomId")
        observed_counts[failure_id] += 1
    total_atoms = require_int(
        minimality["totalAtoms"], "minimalityAuthority.totalAtoms", minimum=1
    )
    require(
        total_atoms == len(ordered_atoms) == 19,
        "pilot ordered atom denominator drift",
    )
    require(
        observed_counts == atom_counts, "pilot per-obligation atom denominator drift"
    )
    execution_rule = require_string(
        minimality["executionRule"], "minimalityAuthority.executionRule"
    )
    require(
        "including variants with an empty retained file or a missing file"
        in execution_rule,
        "pilot minimality authority permits structural skips",
    )
    interestingness = require_string(
        minimality["interestingness"], "minimalityAuthority.interestingness"
    )
    require(
        "both pinned compilers accept" in interestingness,
        "pilot interestingness omits two-oracle acceptance",
    )

    isolation = exact_keys(
        contract["executionIsolation"],
        {
            "requiredCompilers",
            "resetSameHomeAndTmpdirBeforeEveryReplay",
            "sealedContainerInputs",
            "stablePrivatePaths",
            "subjectReplays",
        },
        "pilot executionIsolation",
    )
    require(isolation["subjectReplays"] == 3, "pilot subject replay denominator drift")
    require(
        isolation["stablePrivatePaths"] is True, "pilot private paths are not stable"
    )
    require(
        isolation["resetSameHomeAndTmpdirBeforeEveryReplay"] is True,
        "pilot private directory reset disabled",
    )
    require(
        isolation["requiredCompilers"] == ["gcc", "clang"], "pilot compiler set drift"
    )
    require_string(
        isolation["sealedContainerInputs"], "executionIsolation.sealedContainerInputs"
    )
    oracle_authority = exact_keys(
        contract["oracleEvidenceAuthority"],
        {
            "cleanup",
            "containerDescriptor",
            "containerRuns",
            "perRun",
            "preflight",
            "runOrder",
            "schemaVersion",
        },
        "pilot oracleEvidenceAuthority",
    )
    require(
        oracle_authority["schemaVersion"] == 2,
        "pilot oracle evidence schemaVersion drift",
    )
    require(
        exact_keys(
            oracle_authority["containerRuns"],
            set(EXECUTION_COUNTS),
            "pilot oracle containerRuns",
        )
        == EXECUTION_COUNTS,
        "pilot oracle execution denominator drift",
    )
    descriptor_authority = exact_keys(
        oracle_authority["containerDescriptor"],
        {
            "afterExitStatus",
            "afterState",
            "beforeState",
            "capAdd",
            "capDrop",
            "image",
            "initProcess",
            "mounts",
            "exactEnvironment",
            "exactInputMountSource",
            "networks",
            "platform",
            "publishedPorts",
            "publishedSockets",
            "readOnlyRoot",
            "rosetta",
            "sameIdAndConfigurationBeforeAfter",
            "ssh",
            "virtualization",
        },
        "pilot oracle containerDescriptor",
    )
    require(
        descriptor_authority["sameIdAndConfigurationBeforeAfter"] is True
        and descriptor_authority["beforeState"] == "stopped-without-startedDate"
        and descriptor_authority["afterState"] == "stopped-with-startedDate"
        and descriptor_authority["platform"] == "linux/arm64"
        and descriptor_authority["readOnlyRoot"] is True
        and descriptor_authority["networks"] == []
        and descriptor_authority["capAdd"] == []
        and descriptor_authority["capDrop"] == ["ALL"]
        and descriptor_authority["publishedPorts"] == []
        and descriptor_authority["publishedSockets"] == []
        and descriptor_authority["rosetta"] is False
        and descriptor_authority["ssh"] is False
        and descriptor_authority["virtualization"] is False
        and descriptor_authority["exactEnvironment"] is True
        and descriptor_authority["exactInputMountSource"] is True,
        "pilot oracle container descriptor authority drift",
    )
    for name in ("image", "initProcess", "mounts", "afterExitStatus"):
        require_string(descriptor_authority[name], f"oracleEvidenceAuthority.{name}")
    require(
        "after.status.exitCode" in descriptor_authority["afterExitStatus"]
        and "no unlisted variables" in descriptor_authority["initProcess"]
        and "descriptor source equals" in descriptor_authority["mounts"],
        "pilot oracle descriptor semantic binding drift",
    )
    require_string(oracle_authority["runOrder"], "oracleEvidenceAuthority.runOrder")
    require_string(oracle_authority["preflight"], "oracleEvidenceAuthority.preflight")
    require_string(oracle_authority["perRun"], "oracleEvidenceAuthority.perRun")
    require(
        exact_keys(
            oracle_authority["cleanup"],
            {
                "aliasDeleteRequired",
                "containerDeleteRequired",
                "exceptionTotalCleanup",
                "externalPostCleanupAttestationRequired",
                "inspectFailureIsAbsence",
                "partialEvidenceForbidden",
                "plannedResourceLedgerRequired",
                "postAliasDeleteSuccessfulInventoryRequired",
                "postDeleteSuccessfulInventoryRequired",
                "registerBeforeSideEffect",
            },
            "pilot oracle cleanup",
        )
        == {
            "aliasDeleteRequired": True,
            "containerDeleteRequired": True,
            "exceptionTotalCleanup": True,
            "externalPostCleanupAttestationRequired": True,
            "inspectFailureIsAbsence": False,
            "partialEvidenceForbidden": True,
            "plannedResourceLedgerRequired": True,
            "postAliasDeleteSuccessfulInventoryRequired": True,
            "postDeleteSuccessfulInventoryRequired": True,
            "registerBeforeSideEffect": True,
        },
        "pilot oracle cleanup authority drift",
    )
    observer = exact_keys(
        contract["observerAuthority"],
        OBSERVER_AUTHORITY_KEYS,
        "pilot observer authority",
    )
    require(observer["schemaVersion"] == 2, "pilot observer schemaVersion drift")
    require(
        observer["measurementStatus"] == "NOT_MEASURED",
        "pilot observer authority was result-shaped",
    )
    require(
        observer["protocolId"] == "c30.day5.phase2-external-observer.v2"
        and observer["attestationTrust"] == "externally-pinned-sha256"
        and observer["checkpoints"] == ["plan", "post-execution-seal"]
        and observer["requiredExecutionBoundary"]
        == [
            "subject-runner",
            "oracle-input-closure",
            "oracle-before",
            "oracle-after",
            "cleanup",
            "receipt-replay",
        ]
        and observer["oracleRunCount"] == 46
        and observer["oracleInputRunCount"] == 44
        and observer["subjectRunCount"] == 28,
        "pilot observer protocol/denominator authority drift",
    )
    require(
        observer["actorTopology"]
        == {
            "corpusExecutionCount": 1,
            "observerRole": "parent-reviewer-and-sealer",
            "userEntryPoint": "observer",
            "verifierRole": "single-execution-child",
        }
        and observer["protocolDeltaFromV1"]
        == {
            "dropped": [
                "ready-ack-file-handshake",
                "observer-duplicated-cleanup",
            ],
            "retained": [
                "pre-execution-plan",
                "raw-stdout-stderr-and-status",
                "oracle-input-closure",
                "oracle-before-and-after-descriptors",
                "cleanup-evidence",
                "externally-pinned-observer-identity",
                "independent-replay",
            ],
        }
        and observer["returnCodeAuthority"]
        == {
            "oracle": "container-after.status.exitCode",
            "subject": "observer-runner OS process status",
        },
        "pilot observer v2 topology/delta/return-code authority drift",
    )
    executable = exact_keys(
        observer["executable"],
        {"bytes", "path", "sha256"},
        "pilot observer executable authority",
    )
    if observer["authorityState"] == "ABSENT":
        require(
            observer["provisional"] is True
            and observer["liveMeasurementPermitted"] is False
            and executable == {"bytes": 0, "path": None, "sha256": digest(b"")}
            and "Reject before subject or container execution"
            in require_string(observer["absenceBehavior"], "observer absenceBehavior"),
            "pilot observer executable ABSENT sentinel drift",
        )
    else:
        path_value = require_string(executable["path"], "observer executable path")
        observer_path = confined_path(ROOT, path_value, "observer executable")
        require(
            observer["authorityState"] == "PINNED"
            and observer["provisional"] is False
            and observer["liveMeasurementPermitted"] is True
            and executable["bytes"] == observer_path.stat().st_size
            and executable["sha256"] == digest(observer_path.read_bytes())
            and executable["bytes"] > 0
            and executable["sha256"] != digest(b""),
            "pilot live observer executable authority drift",
        )
    verdict = exact_keys(
        contract["verdictSemantics"],
        {
            "acceptanceValid",
            "globalVerdict",
            "pilotAcceptanceValid",
            "remainingFailures",
            "sourceFailures",
            "verifiedPilotRecords",
        },
        "pilot verdictSemantics",
    )
    require(
        verdict
        == {
            "acceptanceValid": False,
            "globalVerdict": "INCOMPLETE",
            "pilotAcceptanceValid": True,
            "remainingFailures": len(audited["failures"]) - len(pilots),
            "sourceFailures": len(audited["failures"]),
            "verifiedPilotRecords": len(pilots),
        },
        "pilot verdict semantics drift",
    )
    receipt_authority = exact_keys(
        contract["receiptAuthority"],
        {
            "cleanupEvidenceRequired",
            "cleanupLedgerRequired",
            "creation",
            "externalReceiptIndexRequired",
            "executionEvidenceRequired",
            "executionRunCount",
            "measuredReceiptIndex",
            "noReplacePrimitiveRequired",
            "observerEvidenceRequired",
            "observerOracleInputRunCount",
            "observerOracleRunCount",
            "observerSubjectRunCount",
            "producerReceiptRole",
            "rawDescriptorArtifactsInClosure",
            "replayRequiresTrustedReceiptIndexSha256",
            "resultAndReceiptBindPilotContractSha256",
            "schemaVersion",
            "sealedReceiptRole",
            "trustedIndexBindings",
        },
        "pilot receiptAuthority",
    )
    require(
        receipt_authority
        == {
            "cleanupEvidenceRequired": True,
            "cleanupLedgerRequired": True,
            "creation": "atomic-no-overwrite-with-no-replace-primitive",
            "externalReceiptIndexRequired": True,
            "executionEvidenceRequired": True,
            "executionRunCount": 46,
            "measuredReceiptIndex": "separate-externally-pinned-post-run-authority",
            "noReplacePrimitiveRequired": True,
            "observerEvidenceRequired": True,
            "observerOracleInputRunCount": 44,
            "observerOracleRunCount": 46,
            "observerSubjectRunCount": 28,
            "producerReceiptRole": "untrusted single-execution input to observer sealing",
            "rawDescriptorArtifactsInClosure": True,
            "replayRequiresTrustedReceiptIndexSha256": True,
            "resultAndReceiptBindPilotContractSha256": True,
            "schemaVersion": 4,
            "sealedReceiptRole": "observer-evidence-bearing receipt bound by the trusted external index",
            "trustedIndexBindings": [
                "receiptSha256",
                "observerExecutableSha256",
                "observerAttestationSha256",
                "pilotContractSha256",
                "policySha256",
                "overlayClosureSha256",
                "oracleRunOrderSha256",
                "subjectRunOrderSha256",
            ],
        },
        "pilot receipt authority drift",
    )

    forbidden = {
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

    def reject_measured(value: Any, label: str) -> None:
        if isinstance(value, dict):
            overlap = set(value) & forbidden
            require(
                not overlap,
                f"pilot contract contains measured fields at {label}: {sorted(overlap)}",
            )
            for name, child in value.items():
                reject_measured(child, f"{label}.{name}")
        elif isinstance(value, list):
            for index, child in enumerate(value):
                reject_measured(child, f"{label}[{index}]")

    reject_measured(contract, "pilotContract")
    return {
        "bytes": len(contract_bytes),
        "contract": contract,
        "observerAuthority": observer,
        "orderedAtoms": ordered_atoms,
        "pilotIds": pilot_ids,
        "pilots": pilots,
        "sha256": contract_sha256,
    }


def reject_overlay_symlinks(root: Path) -> None:
    for directory, dirnames, filenames in os.walk(
        root, topdown=True, followlinks=False
    ):
        base = Path(directory)
        for name in [*dirnames, *filenames]:
            path = base / name
            mode = path.lstat().st_mode
            require(
                not stat.S_ISLNK(mode),
                f"overlay symbolic link prohibited: {path.relative_to(root)}",
            )
            require(
                stat.S_ISDIR(mode) or stat.S_ISREG(mode),
                f"overlay special filesystem entry prohibited: {path.relative_to(root)}",
            )
            if stat.S_ISREG(mode):
                require(
                    path.stat().st_nlink == 1,
                    f"overlay hard-link isolation violation: {path.relative_to(root)}",
                )


def require_live_pilot_observer_authority(
    pilot_authority: dict[str, Any],
) -> None:
    observer = pilot_authority["observerAuthority"]
    require(
        observer["authorityState"] != "ABSENT"
        and observer["liveMeasurementPermitted"] is True,
        "pilot observer authority is ABSENT; live measurement is not permitted",
    )


def physical_lines(data: bytes) -> list[bytes]:
    return data.splitlines(keepends=True)


def normalize_closure_logical(
    value: Any, label: str, *, require_normalized: bool
) -> str:
    text = require_string(value, label)
    require("\\" not in text and "\x00" not in text, f"{label} is not portable")
    parts = text.split("/")
    require(
        len(parts) >= 2 and parts[0] in {"source", "output"},
        f"{label} lacks source/output root",
    )
    normalized: list[str] = [parts[0]]
    for part in parts[1:]:
        require(part not in {"", "."}, f"{label} has an empty/dot component")
        if part == "..":
            require(len(normalized) > 1, f"{label} escapes its source/output root")
            normalized.pop()
        else:
            normalized.append(part)
    result = "/".join(normalized)
    if require_normalized:
        require(text == result, f"{label} is not normalized")
    return result


def validate_lineage(
    candidate_data: bytes,
    seed_data: bytes,
    lineage_value: Any,
    exact_lineage_keys: list[str],
    label: str,
) -> list[tuple[int, int]]:
    require(
        isinstance(lineage_value, list) and bool(lineage_value),
        f"{label} is empty or not a list",
    )
    candidate_lines = physical_lines(candidate_data)
    seed_lines = physical_lines(seed_data)
    require(bool(candidate_lines), f"{label} candidate has no physical lines")
    require(
        len(lineage_value) == len(candidate_lines),
        f"{label} is not bijective with candidate lines",
    )
    expected_keys = set(exact_lineage_keys)
    mappings: list[tuple[int, int]] = []
    previous_seed = 0
    for index, value in enumerate(lineage_value, start=1):
        entry = exact_keys(value, expected_keys, f"{label}[{index - 1}]")
        candidate_line = require_int(
            entry["candidateLine"], f"{label}[{index - 1}].candidateLine", minimum=1
        )
        seed_line = require_int(
            entry["seedLine"], f"{label}[{index - 1}].seedLine", minimum=1
        )
        require(
            candidate_line == index,
            f"{label} candidateLine sequence is not one-based and contiguous",
        )
        require(
            seed_line > previous_seed,
            f"{label} seedLine sequence is not strictly increasing",
        )
        require(seed_line <= len(seed_lines), f"{label} seedLine is out of range")
        seed_atom = seed_lines[seed_line - 1]
        require_hash(entry["seedLineSha256"], f"{label}[{index - 1}].seedLineSha256")
        require(
            digest(seed_atom) == entry["seedLineSha256"],
            f"{label} seed-line SHA-256 drift",
        )
        require(
            candidate_lines[index - 1] == seed_atom,
            f"{label} candidate line is not the claimed seed atom",
        )
        mappings.append((candidate_line, seed_line))
        previous_seed = seed_line
    return mappings


def record_by_failure_id(audited: dict[str, Any]) -> dict[str, dict[str, Any]]:
    values: dict[str, dict[str, Any]] = {}
    for record in audited["failures"]:
        failure_id = obligation(record)["failureId"]
        require(failure_id not in values, "failure ID collision during overlay binding")
        values[failure_id] = record
    return values


def register_candidate_path(
    path: Path,
    label: str,
    seen_paths: set[Path],
    seen_inodes: set[tuple[int, int]],
) -> None:
    resolved = path.resolve(strict=True)
    require(
        resolved not in seen_paths,
        f"candidate path aliases another obligation: {label}",
    )
    identity_value = (path.stat().st_dev, path.stat().st_ino)
    require(
        identity_value not in seen_inodes,
        f"candidate hard-link aliases another obligation: {label}",
    )
    seen_paths.add(resolved)
    seen_inodes.add(identity_value)


def validate_parser_candidate(
    audited: dict[str, Any],
    overlay_root: Path,
    failure_id: str,
    record: dict[str, Any],
    candidate_value: Any,
    seen_paths: set[Path],
    seen_inodes: set[tuple[int, int]],
) -> dict[str, Any]:
    policy = audited["policy"]["overlayPolicy"]["parserCandidate"]
    candidate = exact_keys(
        candidate_value,
        set(policy["exactKeys"]),
        f"overlay parser candidate {failure_id}",
    )
    require(
        candidate["kind"] == policy["kind"],
        f"parser candidate kind drift: {failure_id}",
    )
    candidate_path = confined_path(
        overlay_root,
        candidate["path"],
        f"overlay parser candidate path {failure_id}",
    )
    register_candidate_path(candidate_path, failure_id, seen_paths, seen_inodes)
    candidate_data = candidate_path.read_bytes()
    require_int(candidate["bytes"], f"parser candidate bytes {failure_id}", minimum=1)
    require(
        candidate["bytes"] == len(candidate_data),
        f"parser candidate byte count drift: {failure_id}",
    )
    require_hash(candidate["sha256"], f"parser candidate SHA-256 {failure_id}")
    require(
        candidate["sha256"] == digest(candidate_data),
        f"parser candidate SHA-256 drift: {failure_id}",
    )

    seed_descriptor = record["artifacts"].get("parserInput")
    require(
        seed_descriptor is not None,
        f"parser obligation has no parserInput seed: {failure_id}",
    )
    seed_root = require_plain_root(audited["evidence"] / "parser", "parser seed root")
    seed_path = audit_descriptor(
        seed_root, seed_descriptor, f"parserInput seed {failure_id}"
    )
    mappings = validate_lineage(
        candidate_data,
        seed_path.read_bytes(),
        candidate["lineage"],
        policy["lineageEntryExactKeys"],
        f"parser lineage {failure_id}",
    )
    diagnostic = record["diagnostic"]
    emitted_span = diagnostic["emittedSpan"]
    emitted_line = emitted_span["line"]
    emitted_column = emitted_span["column"]
    mapped_candidate_lines = {
        seed_line: candidate_line for candidate_line, seed_line in mappings
    }
    seed_lines = physical_lines(seed_path.read_bytes())
    require(
        emitted_line <= len(seed_lines),
        f"parser frozen causal line is outside its seed file: {failure_id}",
    )
    require(
        emitted_line in mapped_candidate_lines,
        f"parser candidate deletes the frozen causal seed line: {failure_id}",
    )
    causal_atom = seed_lines[emitted_line - 1].rstrip(b"\r\n")
    require(
        emitted_column <= len(causal_atom) + 1,
        f"parser frozen causal column is outside its seed line: {failure_id}",
    )
    line_map_descriptor = record["artifacts"].get("lineMap")
    require(
        line_map_descriptor is not None,
        f"parser obligation has no lineMap authority: {failure_id}",
    )
    line_map_path = audit_descriptor(
        seed_root, line_map_descriptor, f"parser lineMap seed {failure_id}"
    )
    line_map_rows: dict[int, dict[str, Any]] = {}
    for number, line in enumerate(
        line_map_path.read_bytes().splitlines(keepends=True), start=1
    ):
        require(
            line.endswith(b"\n") and line != b"\n",
            f"parser lineMap row malformed: {failure_id}:{number}",
        )
        value = decode_json(line, f"parser lineMap {failure_id}:{number}")
        row = exact_keys(
            value,
            {"originLine", "originPath", "parserLine"},
            f"parser lineMap {failure_id}:{number}",
        )
        parser_line = require_int(
            row["parserLine"],
            f"parser lineMap {failure_id}:{number}.parserLine",
            minimum=1,
        )
        require(
            parser_line not in line_map_rows,
            f"parser lineMap duplicates parserLine: {failure_id}:{parser_line}",
        )
        require_int(
            row["originLine"],
            f"parser lineMap {failure_id}:{number}.originLine",
            minimum=1,
        )
        require_string(
            row["originPath"], f"parser lineMap {failure_id}:{number}.originPath"
        )
        line_map_rows[parser_line] = row
    require(
        emitted_line in line_map_rows,
        f"parser causal seed line lacks lineMap row: {failure_id}",
    )
    causal_row = line_map_rows[emitted_line]
    require(
        causal_row["originPath"] == diagnostic["sourcePath"]
        and causal_row["originLine"] == diagnostic["sourceSpan"]["line"],
        f"parser causal lineMap row disagrees with frozen source diagnostic: {failure_id}",
    )
    return {
        "failureId": failure_id,
        "kind": candidate["kind"],
        "candidatePath": candidate_path,
        "candidateSha256": candidate["sha256"],
        "lineage": mappings,
        "causalCandidateLine": mapped_candidate_lines[emitted_line],
    }


def seed_closure_map(
    evidence: Path,
    tu_id: str,
    family: str,
) -> dict[str, tuple[str, int]]:
    inputs_root = require_plain_root(evidence / "inputs", "source inputs directory")
    name = f"{tu_id}.{family}.inputs.tsv"
    path = confined_path(inputs_root, name, f"inputs/{name}")
    rows = parse_closure_rows(path.read_bytes(), f"inputs/{name}", input_paths=True)
    result: dict[str, tuple[str, int]] = {}
    for raw_logical, sha256, byte_count in rows:
        logical = normalize_closure_logical(
            raw_logical,
            f"inputs/{name} logical path",
            require_normalized=False,
        )
        descriptor_value = (sha256, byte_count)
        if logical in result:
            require(
                result[logical] == descriptor_value,
                f"normalized seed logical path aliases conflicting bytes: {tu_id}:{family}:{logical}",
            )
        else:
            result[logical] = descriptor_value
    return result


def audit_raw_seed_authority(
    policy: dict[str, Any],
    evidence: Path,
    records: list[dict[str, Any]],
) -> None:
    audited: dict[str, Any] = {"policy": policy, "evidence": evidence}
    raw_seed_roots(audited)
    record_by_tu: dict[str, dict[str, Any]] = {}
    for record in records:
        record_by_tu.setdefault(record["tuId"], record)
    for tu_id, record in sorted(record_by_tu.items()):
        for family in ("gcc", "clang"):
            for logical, expected in seed_closure_map(evidence, tu_id, family).items():
                resolve_seed_witness(audited, record, logical, family, expected)


def raw_seed_roots(audited: dict[str, Any]) -> dict[str, Path]:
    cached = audited.get("_rawSeedRoots")
    if isinstance(cached, dict):
        return cached
    authority = audited["policy"]["sourceEvidence"]["executionAuthority"][
        "rawSeedRoots"
    ]
    roots: dict[str, Path] = {}
    for name in ("source", "gccOutput", "clangOutput"):
        value = authority[name]
        base = ROOT if value["base"] == "repository" else audited["evidence"]
        base_root = require_plain_root(base, f"raw seed {name} base")
        roots[name] = confined_path(
            base_root,
            value["path"],
            f"raw seed root {name}",
            regular_file=False,
        ).resolve(strict=True)
    audited["_rawSeedRoots"] = roots
    return roots


def resolve_seed_witness(
    audited: dict[str, Any],
    record: dict[str, Any],
    logical: str,
    family: str,
    expected: tuple[str, int],
) -> Path:
    cache = audited.setdefault("_seedWitnessCache", {})
    cache_key = (
        "source" if logical.startswith("source/") else f"{family}Output",
        logical,
        expected,
    )
    if cache_key in cache:
        return cache[cache_key]
    relative = PurePosixPath(logical).parts[1:]
    roots = raw_seed_roots(audited)
    root = (
        roots["source"] if logical.startswith("source/") else roots[f"{family}Output"]
    )
    path = root.joinpath(*relative)
    try:
        resolved = path.resolve(strict=True)
        mode = resolved.stat().st_mode
    except OSError as error:
        raise ContractError(
            f"missing raw seed byte witness: {record['tuId']}:{family}:{logical}: {error}"
        ) from error
    require(
        stat.S_ISREG(mode),
        f"raw seed witness is not a regular file: {record['tuId']}:{family}:{logical}",
    )
    require(
        root in resolved.parents,
        f"raw seed witness escapes frozen root: {record['tuId']}:{family}:{logical}",
    )
    data = resolved.read_bytes()
    require(
        (digest(data), len(data)) == expected,
        f"raw seed witness identity drift: {record['tuId']}:{family}:{logical}",
    )
    cache[cache_key] = resolved
    return resolved


def validate_raw_closure(
    audited: dict[str, Any],
    overlay_root: Path,
    failure_id: str,
    record: dict[str, Any],
    closure_value: Any,
    flavor: str,
    expected_seed_family: str,
    root_logical: str,
    seen_paths: set[Path],
    seen_inodes: set[tuple[int, int]],
) -> dict[str, Any]:
    policy = audited["policy"]["overlayPolicy"]["rawCandidate"]
    closure = exact_keys(
        closure_value,
        set(policy["closureExactKeys"]),
        f"raw {flavor} closure {failure_id}",
    )
    require(
        closure["seedFamily"] == expected_seed_family,
        f"raw {flavor} seedFamily drift: {failure_id}",
    )
    require(
        closure["root"] == root_logical,
        f"raw {flavor} logical root drift: {failure_id}",
    )
    files_value = closure["files"]
    require(
        isinstance(files_value, list) and bool(files_value),
        f"raw {flavor} files is empty or not a list: {failure_id}",
    )
    seed_map = seed_closure_map(
        audited["evidence"], record["tuId"], expected_seed_family
    )
    base_relative = f"candidates/{failure_id}/{flavor}"
    base = confined_path(
        overlay_root,
        base_relative,
        f"raw {flavor} storage directory {failure_id}",
        regular_file=False,
    )
    file_keys = set(policy["fileExactKeys"])
    logicals: list[str] = []
    file_results: dict[str, dict[str, Any]] = {}
    expected_physical: set[Path] = set()
    closure_rows: list[bytes] = []
    for index, value in enumerate(files_value):
        file_value = exact_keys(
            value, file_keys, f"raw {flavor} files[{index}] {failure_id}"
        )
        logical = normalize_closure_logical(
            file_value["logicalPath"],
            f"raw {flavor} files[{index}].logicalPath {failure_id}",
            require_normalized=True,
        )
        require(
            logical not in file_results,
            f"raw {flavor} duplicates logicalPath: {failure_id}:{logical}",
        )
        require(
            logical in seed_map,
            f"raw {flavor} logicalPath is outside its seed closure: {failure_id}:{logical}",
        )
        physical = confined_path(
            overlay_root,
            f"{base_relative}/{logical}",
            f"raw {flavor} file {failure_id}:{logical}",
        )
        require(
            base.resolve(strict=True) in physical.resolve(strict=True).parents,
            f"raw {flavor} file escapes derived storage directory",
        )
        register_candidate_path(
            physical, f"{failure_id}:{flavor}:{logical}", seen_paths, seen_inodes
        )
        data = physical.read_bytes()
        byte_count = require_int(
            file_value["bytes"], f"raw {flavor} bytes {failure_id}:{logical}", minimum=1
        )
        require(
            byte_count == len(data),
            f"raw {flavor} byte count drift: {failure_id}:{logical}",
        )
        sha256 = require_hash(
            file_value["sha256"], f"raw {flavor} SHA-256 {failure_id}:{logical}"
        )
        require(
            sha256 == digest(data),
            f"raw {flavor} SHA-256 drift: {failure_id}:{logical}",
        )
        seed_path = resolve_seed_witness(
            audited,
            record,
            logical,
            expected_seed_family,
            seed_map[logical],
        )
        mappings = validate_lineage(
            data,
            seed_path.read_bytes(),
            file_value["lineage"],
            policy["lineageEntryExactKeys"],
            f"raw {flavor} lineage {failure_id}:{logical}",
        )
        logicals.append(logical)
        expected_physical.add(physical.resolve(strict=True))
        closure_rows.append(f"{logical}\t{sha256}\t{byte_count}\n".encode("utf-8"))
        file_results[logical] = {
            "bytes": byte_count,
            "dataSha256": sha256,
            "lineage": tuple(seed_line for _, seed_line in mappings),
            "seedDescriptor": seed_map[logical],
            "seedPath": seed_path,
        }
    require(
        logicals == sorted(logicals),
        f"raw {flavor} files are not logicalPath-sorted: {failure_id}",
    )
    require(
        root_logical in file_results,
        f"raw {flavor} omits its root source file: {failure_id}",
    )
    actual_physical = {
        path.resolve(strict=True) for path in base.rglob("*") if path.is_file()
    }
    require(
        actual_physical == expected_physical,
        f"raw {flavor} storage contains unlisted or missing files: {failure_id}",
    )
    expected_closure_sha = digest(b"".join(closure_rows))
    require_hash(closure["closureSha256"], f"raw {flavor} closureSha256 {failure_id}")
    require(
        closure["closureSha256"] == expected_closure_sha,
        f"raw {flavor} closure SHA-256 drift: {failure_id}",
    )
    return {
        "closureSha256": expected_closure_sha,
        "files": file_results,
        "root": root_logical,
        "seedFamily": expected_seed_family,
    }


def validate_raw_candidate(
    audited: dict[str, Any],
    overlay_root: Path,
    failure_id: str,
    record: dict[str, Any],
    candidate_value: Any,
    seen_paths: set[Path],
    seen_inodes: set[tuple[int, int]],
) -> dict[str, Any]:
    policy = audited["policy"]["overlayPolicy"]["rawCandidate"]
    candidate = exact_keys(
        candidate_value, set(policy["exactKeys"]), f"overlay raw candidate {failure_id}"
    )
    require(
        candidate["kind"] == policy["kind"], f"raw candidate kind drift: {failure_id}"
    )
    root_source_path = safe_relative(
        candidate["rootSourcePath"], f"raw rootSourcePath {failure_id}"
    )
    require(
        root_source_path == record["sourcePath"],
        f"raw rootSourcePath disagrees with obligation: {failure_id}",
    )
    root_logical = f"source/{root_source_path}"
    reference_values = exact_keys(
        candidate["referenceClosures"],
        {"gcc", "clang"},
        f"raw referenceClosures {failure_id}",
    )
    subject = validate_raw_closure(
        audited,
        overlay_root,
        failure_id,
        record,
        candidate["subjectClosure"],
        "subject",
        "gcc",
        root_logical,
        seen_paths,
        seen_inodes,
    )
    reference_gcc = validate_raw_closure(
        audited,
        overlay_root,
        failure_id,
        record,
        reference_values["gcc"],
        "reference-gcc",
        "gcc",
        root_logical,
        seen_paths,
        seen_inodes,
    )
    reference_clang = validate_raw_closure(
        audited,
        overlay_root,
        failure_id,
        record,
        reference_values["clang"],
        "reference-clang",
        "clang",
        root_logical,
        seen_paths,
        seen_inodes,
    )
    subject_logicals = set(subject["files"])
    reference_gcc_logicals = set(reference_gcc["files"])
    require(
        subject_logicals == reference_gcc_logicals,
        f"raw subject/reference-gcc logical file sets differ: {failure_id}",
    )
    for logical in sorted(subject_logicals):
        subject_identity = tuple(
            subject["files"][logical][name]
            for name in ("bytes", "dataSha256", "lineage", "seedDescriptor")
        )
        reference_identity = tuple(
            reference_gcc["files"][logical][name]
            for name in ("bytes", "dataSha256", "lineage", "seedDescriptor")
        )
        require(
            subject_identity == reference_identity,
            f"raw subject/reference-gcc bytes/lineage differ: {failure_id}:{logical}",
        )
    require(
        subject["closureSha256"] == reference_gcc["closureSha256"],
        f"raw subject/reference-gcc closure SHA-256 differs: {failure_id}",
    )

    closures = [subject, reference_gcc, reference_clang]
    source_sets = [
        {logical for logical in closure["files"] if logical.startswith("source/")}
        for closure in closures
    ]
    require(
        source_sets[0] == source_sets[1] == source_sets[2],
        f"raw shared-source file sets differ: {failure_id}",
    )
    for logical in sorted(source_sets[0]):
        identities = {
            (
                closure["files"][logical]["bytes"],
                closure["files"][logical]["dataSha256"],
                closure["files"][logical]["lineage"],
                closure["files"][logical]["seedDescriptor"],
            )
            for closure in closures
        }
        require(
            len(identities) == 1,
            f"raw shared-source bytes/lineage differ: {failure_id}:{logical}",
        )

    diagnostic = record["diagnostic"]
    diagnostic_source = diagnostic["sourcePath"]
    causal_logical = (
        "output/" + diagnostic_source.removeprefix("@gcc-output/")
        if diagnostic_source.startswith("@gcc-output/")
        else "source/" + diagnostic_source
    )
    causal_logical = normalize_closure_logical(
        causal_logical,
        f"raw frozen causal logical path {failure_id}",
        require_normalized=True,
    )
    require(
        causal_logical in subject["files"],
        f"raw candidate deletes the frozen causal file: {failure_id}",
    )
    emitted_line = record["diagnostic"]["emittedSpan"]["line"]
    require(
        emitted_line in subject["files"][causal_logical]["lineage"],
        f"raw candidate deletes the frozen causal seed line: {failure_id}",
    )
    emitted_column = record["diagnostic"]["emittedSpan"]["column"]
    causal_seed_lines = physical_lines(
        subject["files"][causal_logical]["seedPath"].read_bytes()
    )
    require(
        emitted_line <= len(causal_seed_lines),
        f"raw frozen causal line is outside its seed file: {failure_id}",
    )
    causal_atom = causal_seed_lines[emitted_line - 1].rstrip(b"\r\n")
    require(
        emitted_column <= len(causal_atom) + 1,
        f"raw frozen causal column is outside its retained seed atom: {failure_id}",
    )

    bundle_object = {
        "referenceClosures": {
            "clang": {
                "closureSha256": reference_clang["closureSha256"],
                "root": reference_clang["root"],
                "seedFamily": reference_clang["seedFamily"],
            },
            "gcc": {
                "closureSha256": reference_gcc["closureSha256"],
                "root": reference_gcc["root"],
                "seedFamily": reference_gcc["seedFamily"],
            },
        },
        "rootSourcePath": root_source_path,
        "subjectClosure": {
            "closureSha256": subject["closureSha256"],
            "root": subject["root"],
            "seedFamily": subject["seedFamily"],
        },
    }
    bundle_sha = digest(canonical(bundle_object))
    require_hash(candidate["bundleSha256"], f"raw bundleSha256 {failure_id}")
    require(
        candidate["bundleSha256"] == bundle_sha,
        f"raw bundle SHA-256 drift: {failure_id}",
    )
    return {
        "failureId": failure_id,
        "kind": candidate["kind"],
        "bundleSha256": bundle_sha,
    }


def validate_overlay_manifest(
    audited: dict[str, Any], overlay_root: Path, manifest_path: Path
) -> dict[str, Any]:
    policy = audited["policy"]
    overlay_policy = policy["overlayPolicy"]
    manifest = load_json(manifest_path, "candidate overlay manifest")
    exact_keys(manifest, set(overlay_policy["rootExactKeys"]), "overlay manifest")
    require(manifest["schemaVersion"] == 1, "overlay schemaVersion drift")
    require(manifest["policyId"] == policy["policyId"], "overlay policyId drift")
    require(
        manifest["sourceEvidenceRunId"] == policy["sourceEvidence"]["runId"],
        "overlay sourceEvidenceRunId drift",
    )
    records_value = manifest["records"]
    require(isinstance(records_value, list), "overlay records is not a list")
    expected_by_id = record_by_failure_id(audited)
    require(
        len(records_value) == len(expected_by_id),
        "overlay obligation denominator drift",
    )
    seen_ids: set[str] = set()
    seen_paths: set[Path] = {manifest_path.resolve(strict=True)}
    manifest_stat = manifest_path.stat()
    seen_inodes: set[tuple[int, int]] = {(manifest_stat.st_dev, manifest_stat.st_ino)}
    validated: list[dict[str, Any]] = []
    for index, value in enumerate(records_value):
        entry = exact_keys(
            value, set(overlay_policy["recordExactKeys"]), f"overlay.records[{index}]"
        )
        failure_id = require_hash(
            entry["failureId"], f"overlay.records[{index}].failureId"
        )
        require(
            failure_id in expected_by_id,
            f"overlay contains unknown failureId: {failure_id}",
        )
        require(
            failure_id not in seen_ids, f"overlay duplicates failureId: {failure_id}"
        )
        seen_ids.add(failure_id)
        require(
            entry["state"] == overlay_policy["inputState"],
            f"overlay state is not CANDIDATE: {failure_id}",
        )
        record = expected_by_id[failure_id]
        candidate = entry["candidate"]
        require(
            isinstance(candidate, dict),
            f"overlay candidate is not an object: {failure_id}",
        )
        if record["lane"] == "evocpp-raw":
            validated.append(
                validate_raw_candidate(
                    audited,
                    overlay_root,
                    failure_id,
                    record,
                    candidate,
                    seen_paths,
                    seen_inodes,
                )
            )
        else:
            validated.append(
                validate_parser_candidate(
                    audited,
                    overlay_root,
                    failure_id,
                    record,
                    candidate,
                    seen_paths,
                    seen_inodes,
                )
            )
    require(seen_ids == set(expected_by_id), "overlay failureId bijection drift")
    actual_files = {
        path.resolve(strict=True) for path in overlay_root.rglob("*") if path.is_file()
    }
    require(
        actual_files == seen_paths,
        "overlay contains unreferenced or missing regular files",
    )
    return {"records": validated}


def file_closure(root: Path, *, excluded: set[str] | None = None) -> dict[str, Any]:
    excluded = excluded or set()
    entries: list[dict[str, Any]] = []
    seen_inodes: set[tuple[int, int]] = set()
    for path in sorted(root.rglob("*"), key=lambda value: value.as_posix()):
        relative = path.relative_to(root).as_posix()
        if relative in excluded:
            continue
        mode = path.lstat().st_mode
        require(
            not stat.S_ISLNK(mode), f"file closure contains symbolic link: {relative}"
        )
        if stat.S_ISDIR(mode):
            continue
        require(stat.S_ISREG(mode), f"file closure contains special entry: {relative}")
        info = path.stat()
        require(info.st_nlink == 1, f"file closure contains hard link: {relative}")
        inode = (info.st_dev, info.st_ino)
        require(inode not in seen_inodes, f"file closure aliases inode: {relative}")
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


def closure_sha256(files: dict[str, bytes]) -> str:
    return digest(
        b"".join(
            f"{logical}\t{digest(data)}\t{len(data)}\n".encode("utf-8")
            for logical, data in sorted(files.items())
        )
    )


def raw_bundle_sha256(
    root_source_path: str,
    subject: dict[str, bytes],
    gcc: dict[str, bytes],
    clang: dict[str, bytes],
) -> str:
    root = f"source/{root_source_path}"
    return digest(
        canonical(
            {
                "referenceClosures": {
                    "clang": {
                        "closureSha256": closure_sha256(clang),
                        "root": root,
                        "seedFamily": "clang",
                    },
                    "gcc": {
                        "closureSha256": closure_sha256(gcc),
                        "root": root,
                        "seedFamily": "gcc",
                    },
                },
                "rootSourcePath": root_source_path,
                "subjectClosure": {
                    "closureSha256": closure_sha256(subject),
                    "root": root,
                    "seedFamily": "gcc",
                },
            }
        )
    )


def pilot_anchor(model: dict[str, Any]) -> dict[str, Any]:
    record = model["record"]
    failure_id = model["failureId"]
    diagnostic = record["diagnostic"]
    seed_line = diagnostic["emittedSpan"]["line"]
    if model["kind"] == "preprocessed-parser":
        mapping = next(
            (item for item in model["lineage"] if item["seedLine"] == seed_line),
            None,
        )
        require(mapping is not None, f"pilot deletes parser causal line: {failure_id}")
        return {
            "candidateLine": mapping["candidateLine"],
            "failureId": failure_id,
            "kind": model["kind"],
            "seedLine": seed_line,
            "seedLineSha256": mapping["seedLineSha256"],
            "sourceColumn": diagnostic["sourceSpan"]["column"],
            "sourceLine": diagnostic["sourceSpan"]["line"],
            "sourcePath": diagnostic["sourcePath"],
            "sourceRecordSha256": obligation(record)["sourceRecordSha256"],
        }
    logical = f"source/{diagnostic['sourcePath']}"
    mapping = next(
        (
            item
            for item in model["rawLineage"][logical]
            if item["seedLine"] == seed_line
        ),
        None,
    )
    require(mapping is not None, f"pilot deletes raw causal line: {failure_id}")
    return {
        "candidateLine": mapping["candidateLine"],
        "failureId": failure_id,
        "kind": model["kind"],
        "logicalPath": logical,
        "seedLine": seed_line,
        "seedLineSha256": mapping["seedLineSha256"],
        "sourceColumn": diagnostic["sourceSpan"]["column"],
        "sourceLine": diagnostic["sourceSpan"]["line"],
        "sourcePath": diagnostic["sourcePath"],
        "sourceRecordSha256": obligation(record)["sourceRecordSha256"],
    }


def parser_variant(model: dict[str, Any], deleted_line: int | None) -> dict[str, Any]:
    lines = physical_lines(model["parserData"])
    kept = [
        (line, mapping)
        for index, (line, mapping) in enumerate(zip(lines, model["lineage"]), start=1)
        if index != deleted_line
    ]
    data = b"".join(line for line, _ in kept)
    lineage = [
        {**mapping, "candidateLine": index}
        for index, (_, mapping) in enumerate(kept, start=1)
    ]
    files = {"candidate.c": data}
    return {
        "candidateDigest": digest(data),
        "inputs": {family: dict(files) for family in ("subject", "gcc", "clang")},
        "lineage": lineage,
    }


def raw_variant(
    model: dict[str, Any],
    deleted_file: str | None,
    deleted_line: tuple[str, int] | None,
) -> dict[str, Any]:
    inputs: dict[str, dict[str, bytes]] = {}
    lineages: dict[str, list[dict[str, Any]]] = {
        logical: copy.deepcopy(value) for logical, value in model["rawLineage"].items()
    }
    for family, original in model["rawFiles"].items():
        files = dict(original)
        if deleted_file is not None:
            files.pop(deleted_file, None)
        if deleted_line is not None:
            logical, candidate_line = deleted_line
            lines = physical_lines(files[logical])
            files[logical] = b"".join(
                line
                for index, line in enumerate(lines, start=1)
                if index != candidate_line
            )
        inputs[family] = files
    if deleted_file is not None:
        lineages.pop(deleted_file, None)
    if deleted_line is not None:
        logical, candidate_line = deleted_line
        retained = [
            item
            for index, item in enumerate(lineages[logical], start=1)
            if index != candidate_line
        ]
        lineages[logical] = [
            {**item, "candidateLine": index}
            for index, item in enumerate(retained, start=1)
        ]
    return {
        "candidateDigest": raw_bundle_sha256(
            model["record"]["sourcePath"],
            inputs["subject"],
            inputs["gcc"],
            inputs["clang"],
        ),
        "inputs": inputs,
        "rawLineage": lineages,
    }


def pilot_atoms(model: dict[str, Any]) -> list[dict[str, Any]]:
    atoms: list[dict[str, Any]] = []
    if model["kind"] == "preprocessed-parser":
        for item in model["lineage"]:
            atoms.append(
                {
                    "atomId": (
                        f"line:{item['candidateLine']:06d}:seed:{item['seedLine']:06d}:"
                        f"{item['seedLineSha256']}"
                    ),
                    "variant": parser_variant(model, item["candidateLine"]),
                }
            )
    else:
        for logical in sorted(model["rawFiles"]["subject"]):
            atoms.append(
                {
                    "atomId": f"file:{logical}",
                    "variant": raw_variant(model, logical, None),
                }
            )
            for item in model["rawLineage"][logical]:
                atoms.append(
                    {
                        "atomId": (
                            f"line:{logical}:candidate:{item['candidateLine']:06d}:"
                            f"seed:{item['seedLine']:06d}:{item['seedLineSha256']}"
                        ),
                        "variant": raw_variant(
                            model, None, (logical, item["candidateLine"])
                        ),
                    }
                )
    return sorted(atoms, key=lambda value: value["atomId"])


def validate_pilot_overlay(
    audited: dict[str, Any], pilot_authority: dict[str, Any], overlay: Path
) -> dict[str, Any]:
    overlay_root = require_plain_root(overlay, "pilot overlay root")
    reject_overlay_symlinks(overlay_root)
    manifest_path = confined_path(
        overlay_root,
        audited["policy"]["overlayPolicy"]["manifestPath"],
        "pilot overlay manifest",
    )
    manifest = load_json(manifest_path, "pilot overlay manifest")
    overlay_policy = audited["policy"]["overlayPolicy"]
    exact_keys(manifest, set(overlay_policy["rootExactKeys"]), "pilot overlay manifest")
    require(manifest["schemaVersion"] == 1, "pilot overlay schemaVersion drift")
    require(
        manifest["policyId"] == audited["policy"]["policyId"],
        "pilot overlay policyId drift",
    )
    require(
        manifest["sourceEvidenceRunId"] == audited["sourceEvidenceRunId"],
        "pilot overlay source run drift",
    )
    records = manifest["records"]
    require(isinstance(records, list), "pilot overlay records is not a list")
    require(
        [value.get("failureId") for value in records if isinstance(value, dict)]
        == pilot_authority["pilotIds"],
        "pilot overlay failureId set/order drift",
    )
    by_source_id = record_by_failure_id(audited)
    pilots_by_id = {value["failureId"]: value for value in pilot_authority["pilots"]}
    seen_paths: set[Path] = {manifest_path.resolve(strict=True)}
    manifest_info = manifest_path.stat()
    seen_inodes: set[tuple[int, int]] = {(manifest_info.st_dev, manifest_info.st_ino)}
    models: dict[str, dict[str, Any]] = {}
    for index, record_value in enumerate(records):
        entry = exact_keys(
            record_value,
            set(overlay_policy["recordExactKeys"]),
            f"pilot overlay.records[{index}]",
        )
        failure_id = entry["failureId"]
        require(
            entry["state"] == "CANDIDATE",
            f"pilot overlay state is not CANDIDATE: {failure_id}",
        )
        source_record = by_source_id[failure_id]
        candidate = entry["candidate"]
        pilot = pilots_by_id[failure_id]
        selector = pilot["candidateSelector"]
        if source_record["lane"] == "evocpp-raw":
            validate_raw_candidate(
                audited,
                overlay_root,
                failure_id,
                source_record,
                candidate,
                seen_paths,
                seen_inodes,
            )
            require(
                candidate["kind"] == selector["kind"],
                f"pilot raw candidate kind drift: {failure_id}",
            )
            raw_files: dict[str, dict[str, bytes]] = {}
            for family, flavor, closure in (
                ("subject", "subject", candidate["subjectClosure"]),
                ("gcc", "reference-gcc", candidate["referenceClosures"]["gcc"]),
                ("clang", "reference-clang", candidate["referenceClosures"]["clang"]),
            ):
                raw_files[family] = {
                    item["logicalPath"]: (
                        overlay_root
                        / "candidates"
                        / failure_id
                        / flavor
                        / item["logicalPath"]
                    ).read_bytes()
                    for item in closure["files"]
                }
            shared_selector = selector["sharedSourceFiles"]
            expected_logicals = [item["logicalPath"] for item in shared_selector]
            require(
                sorted(raw_files["subject"]) == sorted(expected_logicals),
                f"pilot raw selector file set drift: {failure_id}",
            )
            raw_lineage = {
                item["logicalPath"]: item["lineage"]
                for item in candidate["subjectClosure"]["files"]
            }
            for expected in shared_selector:
                require(
                    [item["seedLine"] for item in raw_lineage[expected["logicalPath"]]]
                    == expected["seedLines"],
                    f"pilot raw selector lineage drift: {failure_id}:{expected['logicalPath']}",
                )
            model = {
                "candidateDigest": candidate["bundleSha256"],
                "candidateManifest": candidate,
                "failureId": failure_id,
                "kind": "raw-closure",
                "rawFiles": raw_files,
                "rawLineage": raw_lineage,
                "record": source_record,
            }
        else:
            validated = validate_parser_candidate(
                audited,
                overlay_root,
                failure_id,
                source_record,
                candidate,
                seen_paths,
                seen_inodes,
            )
            require(
                candidate["kind"] == selector["kind"],
                f"pilot parser candidate kind drift: {failure_id}",
            )
            require(
                [item["seedLine"] for item in candidate["lineage"]]
                == selector["seedLines"],
                f"pilot parser selector lineage drift: {failure_id}",
            )
            model = {
                "candidateDigest": candidate["sha256"],
                "candidateManifest": candidate,
                "failureId": failure_id,
                "kind": "preprocessed-parser",
                "lineage": copy.deepcopy(candidate["lineage"]),
                "parserData": validated["candidatePath"].read_bytes(),
                "record": source_record,
            }
        atoms = pilot_atoms(model)
        require(
            len(atoms) == pilot["atomCount"],
            f"pilot overlay atom denominator drift: {failure_id}",
        )
        model["atoms"] = atoms
        model["anchor"] = pilot_anchor(model)
        model["anchorSha256"] = digest(canonical(model["anchor"]))
        models[failure_id] = model
    expected_atoms = [
        {"atomId": atom["atomId"], "failureId": failure_id}
        for failure_id in pilot_authority["pilotIds"]
        for atom in models[failure_id]["atoms"]
    ]
    require(
        pilot_authority["orderedAtoms"] == expected_atoms,
        "pilot contract ordered atoms disagree with overlay",
    )
    actual_files = {
        path.resolve(strict=True) for path in overlay_root.rglob("*") if path.is_file()
    }
    require(
        actual_files == seen_paths,
        "pilot overlay contains unreferenced or missing files",
    )
    return {
        "closureSha256": file_closure(overlay_root)["sha256"],
        "manifest": manifest,
        "models": models,
        "root": overlay_root,
    }


def verify_overlay_phase1(
    policy_path: Path, evidence: Path, overlay: Path
) -> dict[str, Any]:
    audited = audit_source(policy_path, evidence)
    overlay_root = require_plain_root(overlay, "candidate overlay root")
    reject_overlay_symlinks(overlay_root)
    manifest_path = confined_path(
        overlay_root,
        audited["policy"]["overlayPolicy"]["manifestPath"],
        "candidate overlay manifest",
    )
    validated = validate_overlay_manifest(audited, overlay_root, manifest_path)
    return {
        "acceptanceValid": False,
        "obligationManifestSha256": audited["manifestSha256"],
        "policySha256": audited["policySha256"],
        "reason": "phase1-reference-acceptance-not-implemented",
        "sourceEvidenceRunId": audited["sourceEvidenceRunId"],
        "sourceFailures": len(audited["failures"]),
        "structurallyValidCandidates": len(validated["records"]),
        "verdict": "INCOMPLETE",
    }


def prepare_pilot_scope(
    policy_path: Path,
    pilot_contract_path: Path,
    evidence: Path,
    overlay: Path,
) -> tuple[dict[str, Any], dict[str, Any], dict[str, Any]]:
    audited = audit_source(policy_path, evidence)
    pilot_authority = audit_pilot_contract(audited, pilot_contract_path)
    require_live_pilot_observer_authority(pilot_authority)
    pilot_overlay = validate_pilot_overlay(audited, pilot_authority, overlay)
    return audited, pilot_authority, pilot_overlay


def reset_directory(path: Path) -> None:
    if path.exists():
        require(
            path.is_dir() and not path.is_symlink(),
            f"private reset target is unsafe: {path}",
        )
        shutil.rmtree(path)
    path.mkdir(parents=True)


def materialize_inputs(root: Path, files: dict[str, bytes]) -> None:
    reset_directory(root)
    for logical, data in sorted(files.items()):
        normalized = (
            normalize_closure_logical(
                logical,
                f"private input {logical}",
                require_normalized=True,
            )
            if "/" in logical
            else safe_relative(logical, f"private input {logical}")
        )
        path = root / normalized
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)


def captured_process(
    argv: list[str],
    environment: dict[str, str] | None,
    stdout_path: Path,
    stderr_path: Path,
    timeout: int,
) -> tuple[int, bool]:
    stdout_path.parent.mkdir(parents=True, exist_ok=True)
    stderr_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
            completed = subprocess.run(
                argv,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                timeout=timeout,
                check=False,
            )
        return completed.returncode, False
    except subprocess.TimeoutExpired:
        return 124, True


def receipt_artifact(receipt_root: Path, path: Path) -> dict[str, Any]:
    relative = path.relative_to(receipt_root).as_posix()
    return descriptor(path, relative)


def failure_classification(code: str, phase: str) -> str:
    if phase == "raw-source-preprocess":
        return "preprocess" if code.startswith("E01") else "driver"
    if code.startswith("E00"):
        return "lex"
    if code.startswith("E01"):
        return "preprocess"
    if code.startswith("E1"):
        return "parse"
    if code.startswith("E2"):
        return "sema"
    if code.startswith("E3"):
        return "layout"
    return "driver"


def parser_line_map(
    audited: dict[str, Any], record: dict[str, Any]
) -> dict[int, dict[str, Any]]:
    root = require_plain_root(audited["evidence"] / "parser", "parser line-map root")
    path = audit_descriptor(
        root, record["artifacts"]["lineMap"], "pilot parser lineMap"
    )
    rows: dict[int, dict[str, Any]] = {}
    for number, line in enumerate(path.read_bytes().splitlines(keepends=True), start=1):
        require(
            line.endswith(b"\n") and line != b"\n",
            f"pilot lineMap row malformed: {number}",
        )
        value = exact_keys(
            decode_json(line, f"pilot lineMap:{number}"),
            {"originLine", "originPath", "parserLine"},
            f"pilot lineMap:{number}",
        )
        parser_line = require_int(
            value["parserLine"], f"pilot lineMap:{number}.parserLine", minimum=1
        )
        require(
            parser_line not in rows,
            f"pilot lineMap duplicates parserLine: {parser_line}",
        )
        rows[parser_line] = value
    return rows


def normalized_subject_observation(
    audited: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    private_inputs: Path,
    return_code: int,
    timed_out: bool,
    stderr_data: bytes,
) -> tuple[dict[str, Any] | None, str | None]:
    if timed_out or return_code == 0:
        return None, None
    first = next(
        (
            line
            for line in stderr_data.decode("utf-8", errors="replace").splitlines()
            if line.strip()
        ),
        "",
    )
    match = DIAGNOSTIC_RE.match(first)
    if match is None:
        return None, None
    code = match.group("code")
    message = match.group("message")
    live_line = int(match.group("line"))
    live_column = int(match.group("column"))
    record = model["record"]
    frozen = record["diagnostic"]
    source_path: str | None = None
    source_span: dict[str, Any] | None = None
    causal = False
    if model["kind"] == "preprocessed-parser":
        mapping = next(
            (item for item in variant["lineage"] if item["candidateLine"] == live_line),
            None,
        )
        if mapping is not None:
            seed_line = mapping["seedLine"]
            origin = parser_line_map(audited, record).get(seed_line)
            if origin is not None:
                source_path = origin["originPath"]
                source_span = {"column": None, "line": origin["originLine"]}
                causal = (
                    seed_line == frozen["emittedSpan"]["line"]
                    and live_column == frozen["emittedSpan"]["column"]
                    and source_path == frozen["sourcePath"]
                    and source_span == frozen["sourceSpan"]
                )
    else:
        emitted = Path(match.group("path")).resolve(strict=False)
        source_root = (private_inputs / "source").resolve(strict=True)
        if emitted != source_root and source_root in emitted.parents:
            logical = "source/" + emitted.relative_to(source_root).as_posix()
            mapping = next(
                (
                    item
                    for item in variant["rawLineage"].get(logical, [])
                    if item["candidateLine"] == live_line
                ),
                None,
            )
            if mapping is not None:
                source_path = logical.removeprefix("source/")
                source_span = {"column": live_column, "line": mapping["seedLine"]}
                causal = (
                    logical == f"source/{frozen['sourcePath']}"
                    and mapping["seedLine"] == frozen["emittedSpan"]["line"]
                    and live_column == frozen["emittedSpan"]["column"]
                    and source_span == frozen["sourceSpan"]
                )
    if source_path is None or source_span is None:
        return None, None
    observation = {
        "classification": failure_classification(code, record["phase"]),
        "diagnostic": {
            "code": code,
            "message": message,
            "sourcePath": source_path,
            "sourceSpan": source_span,
        },
        "phase": record["phase"],
        "returnCode": return_code,
        "timedOut": timed_out,
    }
    anchor_sha256 = model["anchorSha256"] if causal else None
    return observation, anchor_sha256


def raw_subject_argv(
    audited: dict[str, Any],
    model: dict[str, Any],
    private_inputs: Path,
    subject_path: Path,
) -> list[str]:
    record = model["record"]
    raw_root = require_plain_root(audited["evidence"] / "evocpp-raw", "raw argv root")
    argv_path = audit_descriptor(
        raw_root, record["artifacts"]["argv"], "pilot raw argv"
    )
    argv_record = load_json(argv_path, "pilot raw argv")
    argv_value = argv_record.get("argv")
    require(
        isinstance(argv_value, list)
        and bool(argv_value)
        and all(isinstance(item, str) for item in argv_value),
        "pilot raw argv is invalid",
    )
    roots = raw_seed_roots(audited)
    source_root = str(roots["source"])
    output_root = str(roots["gccOutput"])
    private_source = str(private_inputs / "source")
    private_output = str(private_inputs / "output")
    rewritten = [str(subject_path)]
    for value in argv_value[1:]:
        rewritten.append(
            value.replace(source_root, private_source).replace(
                output_root, private_output
            )
        )
    return rewritten


def run_subject_receipt(
    audited: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    receipt_root: Path,
    execution_root: Path,
    artifact_prefix: str,
    stable_private_key: str,
    timeout: int,
    source_date_epoch: str,
) -> dict[str, Any]:
    private_root = execution_root / stable_private_key
    inputs = private_root / "inputs"
    home = private_root / "home"
    temporary = private_root / "tmp"
    materialize_inputs(inputs, variant["inputs"]["subject"])
    reset_directory(home)
    reset_directory(temporary)
    entries_after_reset = sorted(
        [path.name for path in [*home.iterdir(), *temporary.iterdir()]]
    )
    require(entries_after_reset == [], "private HOME/TMPDIR reset is not empty")
    environment = {
        "HOME": str(home),
        "LANG": "C",
        "LC_ALL": "C",
        "SOURCE_DATE_EPOCH": source_date_epoch,
        "TMPDIR": str(temporary),
        "TZ": "UTC",
    }
    subject_path = confined_path(
        audited["evidence"],
        audited["policy"]["sourceEvidence"]["executionAuthority"][
            "subjectEvidenceRelativePath"
        ],
        "pilot subject executable",
    )
    if model["kind"] == "preprocessed-parser":
        argv = [
            str(subject_path),
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            str(inputs / "candidate.c"),
        ]
    else:
        argv = raw_subject_argv(audited, model, inputs, subject_path)
    stdout_path = receipt_root / f"{artifact_prefix}.stdout"
    stderr_path = receipt_root / f"{artifact_prefix}.stderr"
    return_code, timed_out = captured_process(
        argv, environment, stdout_path, stderr_path, timeout
    )
    normalized, anchor_sha256 = normalized_subject_observation(
        audited,
        model,
        variant,
        inputs,
        return_code,
        timed_out,
        stderr_path.read_bytes(),
    )
    stdout = receipt_artifact(receipt_root, stdout_path)
    stderr = receipt_artifact(receipt_root, stderr_path)
    return {
        "argv": argv,
        "argvSha256": digest(canonical(argv)),
        "candidateDigest": variant["candidateDigest"],
        "causalAnchorSha256": anchor_sha256,
        "environment": environment,
        "environmentSha256": digest(canonical(environment)),
        "inputClosureSha256": closure_sha256(variant["inputs"]["subject"]),
        "normalizedObservation": normalized,
        "privateDirectory": {
            "entriesAfterReset": entries_after_reset,
            "home": str(home),
            "resetBeforeExecution": True,
            "tmpdir": str(temporary),
        },
        "returnCode": return_code,
        "stderr": stderr,
        "stderrSha256": stderr["sha256"],
        "stdout": stdout,
        "stdoutSha256": stdout["sha256"],
        "subjectSha256": audited["policy"]["sourceEvidence"]["executionAuthority"][
            "subjectSha256"
        ],
        "timedOut": timed_out,
    }


def execute_subject_checkpoint(
    audited: dict[str, Any],
    pilot_overlay: dict[str, Any],
    receipt_root: Path,
    execution_root: Path,
) -> dict[str, Any]:
    source_artifacts = audited["policy"]["sourceEvidence"]["authorityArtifacts"]
    build_environment = load_json(
        confined_path(
            audited["evidence"],
            source_artifacts["buildEnvironmentContract"]["path"],
            "pilot build-environment contract",
        ),
        "pilot build-environment contract",
    )
    run_config = load_json(
        confined_path(
            audited["evidence"],
            source_artifacts["runConfig"]["path"],
            "pilot run-config",
        ),
        "pilot run-config",
    )
    source_date_epoch = require_string(
        build_environment.get("reproducibility", {}).get("SOURCE_DATE_EPOCH"),
        "pilot SOURCE_DATE_EPOCH",
    )
    values: dict[str, Any] = {}
    stable_fields = {
        "argvSha256",
        "candidateDigest",
        "environmentSha256",
        "returnCode",
        "stderrSha256",
        "stdoutSha256",
        "subjectSha256",
        "timedOut",
    }
    for failure_id, model in sorted(pilot_overlay["models"].items()):
        timeout = require_int(
            run_config[
                "parserTimeoutSecondsPerTu"
                if model["kind"] == "preprocessed-parser"
                else "generalTimeoutSeconds"
            ],
            f"pilot subject timeout {failure_id}",
            minimum=1,
        )
        baseline = (
            parser_variant(model, None)
            if model["kind"] == "preprocessed-parser"
            else raw_variant(model, None, None)
        )
        replays = [
            run_subject_receipt(
                audited,
                model,
                baseline,
                receipt_root,
                execution_root,
                f"artifacts/{failure_id}/baseline/subject-{index}",
                f"{failure_id}/baseline-private",
                timeout,
                source_date_epoch,
            )
            for index in range(3)
        ]
        expected_observation = signature(model["record"])
        for index, replay in enumerate(replays):
            require(
                not replay["timedOut"],
                f"pilot baseline subject timed out: {failure_id}:{index}",
            )
            require(
                replay["normalizedObservation"] == expected_observation,
                f"pilot baseline does not reproduce source failure: {failure_id}:{index}",
            )
            require(
                replay["causalAnchorSha256"] == model["anchorSha256"],
                f"pilot baseline causal anchor drift: {failure_id}:{index}",
            )
        stable = [{name: replay[name] for name in stable_fields} for replay in replays]
        require(
            stable[0] == stable[1] == stable[2],
            f"pilot subject replays are unstable: {failure_id}",
        )
        require(
            replays[0]["environment"]
            == replays[1]["environment"]
            == replays[2]["environment"],
            f"pilot subject environments are unstable: {failure_id}",
        )
        require(
            replays[0]["privateDirectory"]
            == replays[1]["privateDirectory"]
            == replays[2]["privateDirectory"],
            f"pilot private reset paths are unstable: {failure_id}",
        )
        atoms: list[dict[str, Any]] = []
        for index, atom in enumerate(model["atoms"]):
            observation = run_subject_receipt(
                audited,
                model,
                atom["variant"],
                receipt_root,
                execution_root,
                f"artifacts/{failure_id}/atoms/{index:03d}/subject",
                f"{failure_id}/atom-{index:03d}",
                timeout,
                source_date_epoch,
            )
            require(
                not observation["timedOut"],
                f"pilot deletion subject timed out: {failure_id}:{index}",
            )
            atoms.append(
                {
                    "atomId": atom["atomId"],
                    "subjectObservation": observation,
                    "variant": atom["variant"],
                }
            )
        values[failure_id] = {
            "atoms": atoms,
            "baseline": baseline,
            "subjectReplays": replays,
        }
    return values


def control_process(
    argv: list[str], timeout: int = 60
) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise ContractError(f"control process timed out: {argv[:3]}") from error


def decoded_json_output(
    completed: subprocess.CompletedProcess[bytes], label: str
) -> dict[str, Any]:
    require(
        completed.returncode == 0,
        f"{label} failed: {completed.stderr.decode('utf-8', errors='replace').strip()}",
    )
    value = decode_json(completed.stdout, label)
    if isinstance(value, list):
        require(len(value) == 1, f"{label} returned multiple records")
        value = value[0]
    require(isinstance(value, dict), f"{label} did not return an object")
    return value


def require_image_identity(container_bin: str, reference: str, expected: str) -> None:
    inspected = decoded_json_output(
        control_process([container_bin, "image", "inspect", reference]),
        f"image inspect {reference}",
    )
    observed = "sha256:" + str(inspected.get("id", "")).removeprefix("sha256:")
    require(observed == expected, f"sealed image identity drift: {reference}")


def require_container_identity(
    container_bin: str, name: str, expected_image: str, alias: str
) -> None:
    inspected = decoded_json_output(
        control_process([container_bin, "inspect", name]),
        f"container inspect {name}",
    )
    configuration = inspected.get("configuration")
    require(isinstance(configuration, dict), "container inspection lacks configuration")
    image = configuration.get("image")
    require(isinstance(image, dict), "container inspection lacks image")
    descriptor_value = image.get("descriptor")
    require(
        isinstance(descriptor_value, dict),
        "container inspection lacks image descriptor",
    )
    require(
        descriptor_value.get("digest") == expected_image,
        "container image descriptor drift",
    )
    canonical_alias = (
        alias if "/" in alias.split(":", 1)[0] else f"docker.io/library/{alias}"
    )
    require(image.get("reference") == canonical_alias, "container image alias drift")
    platform = configuration.get("platform")
    require(
        isinstance(platform, dict)
        and platform.get("os") == "linux"
        and platform.get("architecture") == "arm64",
        "container platform drift",
    )
    require(configuration.get("readOnly") is True, "container root is not read-only")
    require(configuration.get("networks") == [], "container has a network")
    require(configuration.get("capDrop") == ["ALL"], "container capability drop drift")


def bound_container_process(
    container_bin: str,
    alias: str,
    expected_image: str,
    name: str,
    command: list[str],
    environment: dict[str, str],
    stdout_path: Path,
    stderr_path: Path,
    timeout: int,
    input_root: Path | None,
) -> tuple[int, bool]:
    require_image_identity(container_bin, alias, expected_image)
    create = [
        container_bin,
        "create",
        "--name",
        name,
        "--platform",
        "linux/arm64",
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--tmpfs",
        "/tmp",
        "--workdir",
        "/inputs" if input_root is not None else "/tmp",
    ]
    for key, value in sorted(environment.items()):
        create.extend(("--env", f"{key}={value}"))
    if input_root is not None:
        require(
            input_root.is_dir() and not input_root.is_symlink(),
            "oracle input root is unsafe",
        )
        create.extend(("--volume", f"{input_root}:/inputs:ro"))
    create.extend((alias, *command))
    created = False
    try:
        completed = control_process(create)
        require(
            completed.returncode == 0,
            "cannot create sealed oracle container: "
            + completed.stderr.decode("utf-8", errors="replace").strip(),
        )
        created = True
        require_container_identity(container_bin, name, expected_image, alias)
        return_code, timed_out = captured_process(
            [container_bin, "start", "--attach", name],
            None,
            stdout_path,
            stderr_path,
            timeout,
        )
        if timed_out:
            control_process([container_bin, "stop", name])
        require_container_identity(container_bin, name, expected_image, alias)
        require_image_identity(container_bin, alias, expected_image)
        return return_code, timed_out
    finally:
        if created:
            deleted = control_process([container_bin, "delete", name])
            require(
                deleted.returncode == 0,
                f"cannot delete sealed oracle container: {name}",
            )


def write_artifact(receipt_root: Path, relative: str, data: bytes) -> dict[str, Any]:
    path = receipt_root / safe_relative(relative, f"receipt artifact {relative}")
    path.parent.mkdir(parents=True, exist_ok=True)
    require(not path.exists(), f"receipt artifact path reused: {relative}")
    path.write_bytes(data)
    require(path.stat().st_nlink == 1, f"receipt artifact is hard-linked: {relative}")
    return descriptor(path, relative)


def control_json_artifact(
    argv: list[str], receipt_root: Path, relative: str, label: str
) -> tuple[dict[str, Any], dict[str, Any]]:
    completed = control_process(argv)
    require(
        completed.returncode == 0,
        f"{label} failed: {completed.stderr.decode('utf-8', errors='replace').strip()}",
    )
    artifact = write_artifact(receipt_root, relative, completed.stdout)
    return artifact, decoded_json_output(completed, label)


def image_inspection_artifact(
    container_bin: str,
    reference: str,
    expected_image: str,
    receipt_root: Path,
    relative: str,
    label: str,
) -> dict[str, Any]:
    artifact, inspected = control_json_artifact(
        [container_bin, "image", "inspect", reference], receipt_root, relative, label
    )
    observed = "sha256:" + str(inspected.get("id", "")).removeprefix("sha256:")
    require(observed == expected_image, f"sealed image identity drift: {reference}")
    return artifact


def environment_items(value: Any, label: str) -> dict[str, str]:
    require(isinstance(value, list), f"{label} is not a list")
    environment: dict[str, str] = {}
    for index, item in enumerate(value):
        require(
            isinstance(item, str) and "=" in item,
            f"{label}[{index}] is not NAME=value",
        )
        name, content = item.split("=", 1)
        require(name not in environment, f"{label} duplicates {name}")
        environment[name] = content
    return environment


def mount_type(value: Any) -> str | None:
    if isinstance(value, str):
        return value
    if isinstance(value, dict) and len(value) == 1:
        return next(iter(value))
    return None


def validate_container_pair(
    before: dict[str, Any],
    after: dict[str, Any],
    name: str,
    alias: str,
    expected_image: str,
    command: list[str],
    environment: dict[str, str],
    has_inputs: bool,
) -> None:
    require(before.get("id") == name, f"container before ID drift: {name}")
    require(after.get("id") == name, f"container after ID drift: {name}")
    before_configuration = before.get("configuration")
    after_configuration = after.get("configuration")
    require(
        isinstance(before_configuration, dict)
        and before_configuration == after_configuration,
        f"container configuration changed during execution: {name}",
    )
    configuration = before_configuration
    image = configuration.get("image")
    require(isinstance(image, dict), f"container image descriptor absent: {name}")
    image_descriptor = image.get("descriptor")
    require(
        isinstance(image_descriptor, dict)
        and image_descriptor.get("digest") == expected_image,
        f"container sealed image drift: {name}",
    )
    canonical_alias = (
        alias if "/" in alias.split(":", 1)[0] else f"docker.io/library/{alias}"
    )
    require(image.get("reference") == canonical_alias, f"container alias drift: {name}")
    platform = configuration.get("platform")
    require(
        isinstance(platform, dict)
        and platform.get("os") == "linux"
        and platform.get("architecture") == "arm64",
        f"container platform drift: {name}",
    )
    require(configuration.get("readOnly") is True, f"container root writable: {name}")
    require(configuration.get("networks") == [], f"container network present: {name}")
    require(configuration.get("capAdd") == [], f"container capabilities added: {name}")
    require(configuration.get("capDrop") == ["ALL"], f"container capDrop drift: {name}")
    require(
        configuration.get("publishedPorts") == [], f"container publishes ports: {name}"
    )
    require(
        configuration.get("publishedSockets") == [],
        f"container publishes sockets: {name}",
    )
    for key in ("rosetta", "ssh", "virtualization"):
        require(configuration.get(key) is False, f"container enables {key}: {name}")
    process = configuration.get("initProcess")
    require(isinstance(process, dict), f"container init process absent: {name}")
    require(
        process.get("executable") == command[0], f"container executable drift: {name}"
    )
    require(
        process.get("arguments") == command[1:], f"container arguments drift: {name}"
    )
    require(
        process.get("workingDirectory") == ("/inputs" if has_inputs else "/tmp"),
        f"container working directory drift: {name}",
    )
    effective_environment = environment_items(
        process.get("environment"), f"container environment {name}"
    )
    for key, value in environment.items():
        require(
            effective_environment.get(key) == value,
            f"container environment drift: {name}:{key}",
        )
    mounts = configuration.get("mounts")
    require(isinstance(mounts, list), f"container mounts absent: {name}")
    by_destination: dict[str, dict[str, Any]] = {}
    for value in mounts:
        require(isinstance(value, dict), f"container mount invalid: {name}")
        destination = value.get("destination")
        require(
            isinstance(destination, str) and destination not in by_destination,
            f"container mount destination invalid: {name}",
        )
        by_destination[destination] = value
    expected_destinations = {"/tmp", "/inputs"} if has_inputs else {"/tmp"}
    require(
        set(by_destination) == expected_destinations,
        f"container mount set drift: {name}",
    )
    require(
        mount_type(by_destination["/tmp"].get("type")) == "tmpfs",
        f"container /tmp is not tmpfs: {name}",
    )
    if has_inputs:
        inputs = by_destination["/inputs"]
        require(
            mount_type(inputs.get("type")) == "virtiofs"
            and inputs.get("options") == ["ro"],
            f"container /inputs mount is not readonly virtiofs: {name}",
        )
        source = inputs.get("source")
        require(
            isinstance(source, str) and Path(source).is_absolute(),
            f"container /inputs source is not absolute: {name}",
        )
    before_status = before.get("status")
    after_status = after.get("status")
    require(isinstance(before_status, dict), f"container before status absent: {name}")
    require(isinstance(after_status, dict), f"container after status absent: {name}")
    require(
        before_status.get("state") == "stopped" and "startedDate" not in before_status,
        f"container before state is not pristine stopped: {name}",
    )
    require(
        after_status.get("state") == "stopped"
        and isinstance(after_status.get("startedDate"), str)
        and bool(after_status["startedDate"]),
        f"container after state does not prove execution: {name}",
    )


def evidenced_container_process(
    authority: dict[str, Any],
    run_id: str,
    purpose: str,
    family: str,
    command: list[str],
    environment: dict[str, str],
    stdout_path: Path,
    stderr_path: Path,
    timeout: int,
    input_root: Path | None,
    input_closure_sha256: str | None,
    receipt_root: Path,
) -> tuple[int, bool]:
    container_bin = authority["containerBin"]
    alias = authority["alias"]
    expected_image = authority["expectedImage"]
    authority["invocation"] += 1
    ordinal = authority["invocation"]
    name = f"c30-pilot-{ordinal:03d}-{authority['token']}"
    prefix = f"artifacts/execution/runs/{ordinal:03d}"
    image_before = image_inspection_artifact(
        container_bin,
        alias,
        expected_image,
        receipt_root,
        f"{prefix}/image-before.json",
        f"{run_id} image before",
    )
    create = [
        container_bin,
        "create",
        "--name",
        name,
        "--platform",
        "linux/arm64",
        "--network",
        "none",
        "--read-only",
        "--cap-drop",
        "ALL",
        "--tmpfs",
        "/tmp",
        "--workdir",
        "/inputs" if input_root is not None else "/tmp",
    ]
    for key, value in sorted(environment.items()):
        create.extend(("--env", f"{key}={value}"))
    if input_root is not None:
        require(
            input_root.is_dir() and not input_root.is_symlink(),
            "oracle input root is unsafe",
        )
        create.extend(("--volume", f"{input_root}:/inputs:ro"))
    create.extend((alias, *command))
    completed = control_process(create)
    require(
        completed.returncode == 0,
        "cannot create sealed oracle container: "
        + completed.stderr.decode("utf-8", errors="replace").strip(),
    )
    authority["retainedNames"].append(name)
    container_before, before = control_json_artifact(
        [container_bin, "inspect", name],
        receipt_root,
        f"{prefix}/container-before.json",
        f"{run_id} container before",
    )
    return_code, timed_out = captured_process(
        [container_bin, "start", "--attach", name],
        None,
        stdout_path,
        stderr_path,
        timeout,
    )
    if timed_out:
        stopped = control_process([container_bin, "stop", name])
        if stopped.returncode != 0:
            stopped = control_process([container_bin, "kill", name])
        require(stopped.returncode == 0, f"cannot stop timed-out container: {name}")
    container_after, after = control_json_artifact(
        [container_bin, "inspect", name],
        receipt_root,
        f"{prefix}/container-after.json",
        f"{run_id} container after",
    )
    image_after = image_inspection_artifact(
        container_bin,
        alias,
        expected_image,
        receipt_root,
        f"{prefix}/image-after.json",
        f"{run_id} image after",
    )
    validate_container_pair(
        before,
        after,
        name,
        alias,
        expected_image,
        command,
        environment,
        input_root is not None,
    )
    authority["runs"].append(
        {
            "cleanup": None,
            "containerAfter": container_after,
            "containerBefore": container_before,
            "containerName": name,
            "family": family,
            "imageAfter": image_after,
            "imageBefore": image_before,
            "inputClosureSha256": input_closure_sha256,
            "purpose": purpose,
            "runId": run_id,
        }
    )
    return return_code, timed_out


def prepare_oracle_authority(
    audited: dict[str, Any], execution_root: Path, receipt_root: Path
) -> dict[str, Any]:
    artifacts = audited["policy"]["sourceEvidence"]["authorityArtifacts"]
    build_environment = load_json(
        confined_path(
            audited["evidence"],
            artifacts["buildEnvironmentContract"]["path"],
            "oracle build-environment contract",
        ),
        "oracle build-environment contract",
    )
    host_authority = load_json(
        confined_path(
            audited["evidence"],
            artifacts["hostBuildAuthority"]["path"],
            "oracle host-build authority",
        ),
        "oracle host-build authority",
    )
    container_value = host_authority.get("tools", {}).get("container", {})
    container_path = Path(
        require_string(container_value.get("path"), "container CLI path")
    )
    require(container_path.is_absolute(), "container CLI path is not absolute")
    require(
        container_path.is_file() and not container_path.is_symlink(),
        "container CLI is absent or symbolic",
    )
    require(os.access(container_path, os.X_OK), "container CLI is not executable")
    execution = audited["policy"]["sourceEvidence"]["executionAuthority"]
    require(
        len(container_path.read_bytes()) == container_value.get("bytes")
        and digest(container_path.read_bytes()) == execution["containerCliSha256"],
        "container CLI SHA-256 drift before pilot execution",
    )
    container_bin = str(container_path)
    expected_version = require_string(
        build_environment.get("containerRuntime", {}).get("version"),
        "container runtime version",
    )
    version = control_process([container_bin, "--version"])
    require(
        version.returncode == 0
        and expected_version in version.stdout.decode("utf-8", errors="replace"),
        "container runtime version drift before pilot execution",
    )
    build_image = build_environment.get("buildImage", {})
    discovery = require_string(
        build_image.get("sealedImageTag"), "sealed image discovery tag"
    )
    expected_image = execution["sealedImageId"]
    token = digest(str(execution_root).encode("utf-8"))[:12]
    alias = f"c30-day5-pilot-{os.getpid()}-{token}:bound"
    discovery_image = image_inspection_artifact(
        container_bin,
        discovery,
        expected_image,
        receipt_root,
        "artifacts/execution/preflight/discovery-image.json",
        "preflight discovery image",
    )
    tagged = control_process([container_bin, "image", "tag", discovery, alias])
    require(
        tagged.returncode == 0,
        "cannot create pilot image alias: "
        + tagged.stderr.decode("utf-8", errors="replace").strip(),
    )
    try:
        alias_image = image_inspection_artifact(
            container_bin,
            alias,
            expected_image,
            receipt_root,
            "artifacts/execution/preflight/alias-image.json",
            "preflight alias image",
        )
    except Exception:
        control_process([container_bin, "image", "delete", alias])
        raise
    return {
        "alias": alias,
        "aliasLive": True,
        "buildEnvironment": build_environment,
        "containerBin": container_bin,
        "containerCli": {
            "bytes": container_value["bytes"],
            "path": container_value["path"],
            "sha256": execution["containerCliSha256"],
            "version": expected_version,
        },
        "expectedImage": expected_image,
        "invocation": 0,
        "preflight": {
            "alias": alias,
            "aliasImage": alias_image,
            "discoveryImage": discovery_image,
            "discoveryReference": discovery,
            "expectedImage": expected_image,
        },
        "retainedNames": [],
        "runs": [],
        "token": token,
    }


def delete_oracle_authority(authority: dict[str, Any]) -> None:
    deleted = control_process(
        [authority["containerBin"], "image", "delete", authority["alias"]]
    )
    require(deleted.returncode == 0, "cannot delete pilot image alias")


def captured_control_evidence(
    argv: list[str], receipt_root: Path, prefix: str
) -> tuple[int, dict[str, Any], dict[str, Any]]:
    stdout_path = receipt_root / f"{prefix}.stdout"
    stderr_path = receipt_root / f"{prefix}.stderr"
    return_code, timed_out = captured_process(argv, None, stdout_path, stderr_path, 60)
    require(not timed_out, f"control command timed out: {argv[:3]}")
    return (
        return_code,
        receipt_artifact(receipt_root, stdout_path),
        receipt_artifact(receipt_root, stderr_path),
    )


def cleanup_run_evidence(
    authority: dict[str, Any], run: dict[str, Any], receipt_root: Path, ordinal: int
) -> dict[str, Any]:
    container_bin = authority["containerBin"]
    name = run["containerName"]
    prefix = f"artifacts/execution/runs/{ordinal:03d}/cleanup"
    delete_code, delete_stdout, delete_stderr = captured_control_evidence(
        [container_bin, "delete", name], receipt_root, f"{prefix}/delete"
    )
    require(delete_code == 0, f"cannot delete retained pilot container: {name}")
    inspect_code, inspect_stdout, inspect_stderr = captured_control_evidence(
        [container_bin, "inspect", name], receipt_root, f"{prefix}/post-delete-inspect"
    )
    require(
        inspect_code != 0,
        f"deleted pilot container remains inspectable: {name}",
    )
    authority["retainedNames"].remove(name)
    return {
        "deleteReturnCode": delete_code,
        "deleteStderr": delete_stderr,
        "deleteStdout": delete_stdout,
        "postDeleteInspectReturnCode": inspect_code,
        "postDeleteInspectStderr": inspect_stderr,
        "postDeleteInspectStdout": inspect_stdout,
    }


def cleanup_alias_evidence(
    authority: dict[str, Any], receipt_root: Path
) -> dict[str, Any]:
    container_bin = authority["containerBin"]
    alias = authority["alias"]
    prefix = "artifacts/execution/alias-cleanup"
    delete_code, delete_stdout, delete_stderr = captured_control_evidence(
        [container_bin, "image", "delete", alias], receipt_root, f"{prefix}/delete"
    )
    require(delete_code == 0, "cannot delete pilot image alias")
    inspect_code, inspect_stdout, inspect_stderr = captured_control_evidence(
        [container_bin, "image", "inspect", alias],
        receipt_root,
        f"{prefix}/post-delete-inspect",
    )
    require(inspect_code != 0, "deleted pilot image alias remains inspectable")
    authority["aliasLive"] = False
    return {
        "deleteReturnCode": delete_code,
        "deleteStderr": delete_stderr,
        "deleteStdout": delete_stdout,
        "postDeleteInspectReturnCode": inspect_code,
        "postDeleteInspectStderr": inspect_stderr,
        "postDeleteInspectStdout": inspect_stdout,
    }


def best_effort_oracle_cleanup(authority: dict[str, Any]) -> None:
    container_bin = authority["containerBin"]
    for name in list(authority["retainedNames"]):
        control_process([container_bin, "stop", name])
        control_process([container_bin, "delete", "--force", name])
        authority["retainedNames"].remove(name)
    if authority.get("aliasLive"):
        control_process([container_bin, "image", "delete", authority["alias"]])
        authority["aliasLive"] = False


def observer_checkpoint(
    observer_ready_file: Path | None,
    observer_ack_file: Path | None,
    audited: dict[str, Any],
    pilot_authority: dict[str, Any],
    pilot_overlay: dict[str, Any],
    authority: dict[str, Any],
) -> None:
    require(
        (observer_ready_file is None) == (observer_ack_file is None),
        "observer ready/ack files must be supplied together",
    )
    if observer_ready_file is None or observer_ack_file is None:
        return
    require(not observer_ready_file.exists(), "observer ready file already exists")
    require(not observer_ack_file.exists(), "observer ack file already exists")
    observer_ready_file.parent.mkdir(parents=True, exist_ok=True)
    ready = {
        "alias": authority["alias"],
        "overlayClosureSha256": pilot_overlay["closureSha256"],
        "pilotContractSha256": pilot_authority["sha256"],
        "policySha256": audited["policySha256"],
        "runs": [
            {"containerName": run["containerName"], "runId": run["runId"]}
            for run in authority["runs"]
        ],
        "schemaVersion": 1,
    }
    ready_bytes = canonical(ready)
    ready_sha256 = digest(ready_bytes)
    try:
        with observer_ready_file.open("xb") as output:
            output.write(ready_bytes)
    except OSError as error:
        raise ContractError(f"cannot publish observer ready file: {error}") from error
    deadline = time.monotonic() + 1800
    while not observer_ack_file.exists():
        require(time.monotonic() < deadline, "observer acknowledgement timed out")
        time.sleep(0.25)
    require(
        observer_ack_file.is_file() and not observer_ack_file.is_symlink(),
        "observer acknowledgement is not a plain file",
    )
    acknowledgement = exact_keys(
        decode_json(observer_ack_file.read_bytes(), "observer acknowledgement"),
        {"readySha256"},
        "observer acknowledgement",
    )
    require(
        acknowledgement["readySha256"] == ready_sha256,
        "observer acknowledgement does not bind the ready inventory",
    )


def oracle_environment(source_date_epoch: str) -> dict[str, str]:
    return {
        "HOME": "/tmp/c30-pilot-home",
        "LANG": "C",
        "LC_ALL": "C",
        "SOURCE_DATE_EPOCH": source_date_epoch,
        "TMPDIR": "/tmp/c30-pilot-tmp",
        "TZ": "UTC",
    }


def verify_compiler_identities(
    audited: dict[str, Any], authority: dict[str, Any], receipt_root: Path
) -> dict[str, Any]:
    environment = oracle_environment(
        require_string(
            authority["buildEnvironment"]
            .get("reproducibility", {})
            .get("SOURCE_DATE_EPOCH"),
            "oracle SOURCE_DATE_EPOCH",
        )
    )
    compilers = audited["policy"]["sourceEvidence"]["executionAuthority"][
        "referenceCompilers"
    ]
    identities: dict[str, Any] = {}
    for family in ("gcc", "clang"):
        compiler = compilers[family]
        stdout_path = receipt_root / f"artifacts/execution/identities/{family}.stdout"
        stderr_path = receipt_root / f"artifacts/execution/identities/{family}.stderr"
        command = [
            "/bin/sh",
            "-c",
            '/usr/bin/sha256sum "$1"; "$1" --version',
            "c30-compiler-identity",
            compiler["path"],
        ]
        return_code, timed_out = evidenced_container_process(
            authority,
            f"identity:{family}",
            "compiler-identity",
            family,
            command,
            environment,
            stdout_path,
            stderr_path,
            60,
            None,
            None,
            receipt_root,
        )
        require(
            not timed_out and return_code == 0,
            f"cannot measure {family} compiler identity",
        )
        lines = stdout_path.read_text(encoding="utf-8", errors="strict").splitlines()
        require(bool(lines), f"{family} compiler identity output is empty")
        require(
            lines[0].split()[0] == compiler["sha256"],
            f"{family} compiler SHA-256 drift",
        )
        require(
            len(lines) >= 2 and lines[1] == compiler["version"],
            f"{family} compiler version drift",
        )
        stdout = receipt_artifact(receipt_root, stdout_path)
        stderr = receipt_artifact(receipt_root, stderr_path)
        identities[family] = {
            "command": command,
            "commandSha256": digest(canonical(command)),
            "compilerPath": compiler["path"],
            "compilerSha256": compiler["sha256"],
            "compilerVersion": compiler["version"],
            "environment": environment,
            "executionRunId": f"identity:{family}",
            "stderr": stderr,
            "stderrSha256": stderr["sha256"],
            "stdout": stdout,
            "stdoutSha256": stdout["sha256"],
        }
    return identities


def first_saved_argv(path: Path) -> list[str]:
    try:
        first = path.read_bytes().splitlines()[0].decode("utf-8", errors="strict")
    except (OSError, IndexError, UnicodeDecodeError) as error:
        raise ContractError(f"cannot decode command record {path}: {error}") from error
    require(" := " in first, f"command record lacks saved-command delimiter: {path}")
    try:
        argv = shlex.split(first.split(" := ", 1)[1], posix=True)
    except ValueError as error:
        raise ContractError(f"cannot parse command record {path}: {error}") from error
    require(bool(argv), f"command record argv is empty: {path}")
    return argv


def raw_oracle_command(
    audited: dict[str, Any], model: dict[str, Any], family: str
) -> list[str]:
    artifacts = audited["policy"]["sourceEvidence"]["authorityArtifacts"]
    cohort = load_json(
        confined_path(
            audited["evidence"],
            artifacts["kernelCohortContract"]["path"],
            "pilot kernel cohort",
        ),
        "pilot kernel cohort",
    )
    unit = next(
        (
            value
            for value in cohort["translationUnits"]
            if value.get("id") == model["record"]["tuId"]
        ),
        None,
    )
    require(isinstance(unit, dict), "pilot TU is absent from kernel cohort")
    roots = raw_seed_roots(audited)
    command_path = roots[f"{family}Output"] / unit["commandRecordPath"]
    expected_sha256 = unit["commands"][family]["recordSha256ByLane"][f"{family}-lane-1"]
    require(
        command_path.is_file() and not command_path.is_symlink(),
        f"missing {family} command record",
    )
    require(
        digest(command_path.read_bytes()) == expected_sha256,
        f"{family} command record SHA-256 drift",
    )
    original = first_saved_argv(command_path)
    compiler = audited["policy"]["sourceEvidence"]["executionAuthority"][
        "referenceCompilers"
    ][family]["path"]
    projected = [compiler, "-E", "-P"]

    def translate_path(value: str) -> str:
        if value.startswith("/work/src/"):
            return "/inputs/source/" + value.removeprefix("/work/src/")
        if value.startswith("/work/out/"):
            return "/inputs/output/" + value.removeprefix("/work/out/")
        relative = value.removeprefix("./")
        return "/inputs/output/" + relative

    separate_paths = {"-I", "-include", "-imacros", "-isystem", "-iquote"}
    index = 1
    while index < len(original):
        value = original[index]
        if value.startswith(("-Wp,-MMD,", "-Wp,-MD,")) or value == "-c":
            index += 1
            continue
        if value == "-o":
            require(
                index + 1 < len(original), f"{family} command record has dangling -o"
            )
            index += 2
            continue
        if value in separate_paths:
            require(
                index + 1 < len(original),
                f"{family} command record has dangling {value}",
            )
            projected.extend((value, translate_path(original[index + 1])))
            index += 2
            continue
        if value.startswith("-I") and len(value) > 2:
            projected.append("-I" + translate_path(value[2:]))
        elif value.startswith("-fmacro-prefix-map=/work/src/"):
            projected.append(value.replace("/work/src/", "/inputs/source/"))
        elif value.startswith("/work/src/"):
            projected.append(translate_path(value))
        else:
            projected.append(value)
        index += 1
    return projected


def input_descriptors(files: dict[str, bytes]) -> list[dict[str, Any]]:
    return [
        {"bytes": len(data), "logicalPath": logical, "sha256": digest(data)}
        for logical, data in sorted(files.items())
    ]


def run_oracle_receipt(
    audited: dict[str, Any],
    authority: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    family: str,
    receipt_root: Path,
    execution_root: Path,
    artifact_prefix: str,
    timeout: int,
    run_id: str,
    purpose: str,
) -> dict[str, Any]:
    inputs = execution_root / "oracle-inputs" / artifact_prefix / family
    materialize_inputs(inputs, variant["inputs"][family])
    compiler = audited["policy"]["sourceEvidence"]["executionAuthority"][
        "referenceCompilers"
    ][family]
    if model["kind"] == "preprocessed-parser":
        command = [
            value.replace("<exact-private-candidate>", "/inputs/candidate.c")
            for value in audited["policy"]["referenceAcceptance"]["parserCommands"][
                family
            ]
        ]
    else:
        command = raw_oracle_command(audited, model, family)
    environment = oracle_environment(
        require_string(
            authority["buildEnvironment"]
            .get("reproducibility", {})
            .get("SOURCE_DATE_EPOCH"),
            "oracle SOURCE_DATE_EPOCH",
        )
    )
    stdout_path = receipt_root / f"{artifact_prefix}/{family}.stdout"
    stderr_path = receipt_root / f"{artifact_prefix}/{family}.stderr"
    input_closure = closure_sha256(variant["inputs"][family])
    return_code, timed_out = evidenced_container_process(
        authority,
        run_id,
        purpose,
        family,
        command,
        environment,
        stdout_path,
        stderr_path,
        timeout,
        inputs,
        input_closure,
        receipt_root,
    )
    stdout = receipt_artifact(receipt_root, stdout_path)
    stderr = receipt_artifact(receipt_root, stderr_path)
    return {
        "candidateDigest": variant["candidateDigest"],
        "command": command,
        "commandSha256": digest(canonical(command)),
        "compilerPath": compiler["path"],
        "compilerSha256": compiler["sha256"],
        "compilerVersion": compiler["version"],
        "containerInputs": input_descriptors(variant["inputs"][family]),
        "environment": environment,
        "executionRunId": run_id,
        "imageId": authority["expectedImage"],
        "inputClosureSha256": input_closure,
        "returnCode": return_code,
        "stderr": stderr,
        "stderrSha256": stderr["sha256"],
        "stdout": stdout,
        "stdoutSha256": stdout["sha256"],
        "timedOut": timed_out,
    }


def execute_oracle_checkpoint(
    audited: dict[str, Any],
    pilot_authority: dict[str, Any],
    pilot_overlay: dict[str, Any],
    subject_values: dict[str, Any],
    receipt_root: Path,
    execution_root: Path,
    observer_ready_file: Path | None,
    observer_ack_file: Path | None,
) -> dict[str, Any]:
    artifacts = audited["policy"]["sourceEvidence"]["authorityArtifacts"]
    run_config = load_json(
        confined_path(
            audited["evidence"],
            artifacts["runConfig"]["path"],
            "pilot oracle run-config",
        ),
        "pilot oracle run-config",
    )
    timeout = require_int(
        run_config["generalTimeoutSeconds"], "pilot oracle timeout", minimum=1
    )
    authority: dict[str, Any] | None = None
    try:
        authority = prepare_oracle_authority(audited, execution_root, receipt_root)
        identities = verify_compiler_identities(audited, authority, receipt_root)
        for failure_id, model in sorted(pilot_overlay["models"].items()):
            baseline = subject_values[failure_id]["baseline"]
            baseline_references = {
                family: run_oracle_receipt(
                    audited,
                    authority,
                    model,
                    baseline,
                    family,
                    receipt_root,
                    execution_root,
                    f"artifacts/{failure_id}/baseline/oracle",
                    timeout,
                    f"baseline:{failure_id}:{family}",
                    "baseline-acceptance",
                )
                for family in ("gcc", "clang")
            }
            for family, receipt in baseline_references.items():
                require(
                    not receipt["timedOut"],
                    f"pilot baseline {family} timed out: {failure_id}",
                )
                require(
                    receipt["returnCode"] == 0,
                    f"pilot baseline {family} rejected candidate: {failure_id}",
                )
            subject_values[failure_id]["referenceAcceptance"] = baseline_references
            for index, atom in enumerate(subject_values[failure_id]["atoms"]):
                atom["referenceAcceptance"] = {
                    family: run_oracle_receipt(
                        audited,
                        authority,
                        model,
                        atom["variant"],
                        family,
                        receipt_root,
                        execution_root,
                        f"artifacts/{failure_id}/atoms/{index:03d}/oracle",
                        timeout,
                        f"atom:{failure_id}:{index:03d}:{family}",
                        "minimality-deletion",
                    )
                    for family in ("gcc", "clang")
                }
                for family, receipt in atom["referenceAcceptance"].items():
                    require(
                        not receipt["timedOut"],
                        f"pilot deletion {family} timed out: {failure_id}:{index}",
                    )
        expected_run_ids = ["identity:gcc", "identity:clang"]
        for failure_id in pilot_authority["pilotIds"]:
            expected_run_ids.extend(
                f"baseline:{failure_id}:{family}" for family in ("gcc", "clang")
            )
            for index, _ in enumerate(pilot_overlay["models"][failure_id]["atoms"]):
                expected_run_ids.extend(
                    f"atom:{failure_id}:{index:03d}:{family}"
                    for family in ("gcc", "clang")
                )
        require(
            len(expected_run_ids) == 46
            and authority["invocation"] == 46
            and [run["runId"] for run in authority["runs"]] == expected_run_ids,
            "pilot oracle execution run denominator/order drift",
        )
        observer_checkpoint(
            observer_ready_file,
            observer_ack_file,
            audited,
            pilot_authority,
            pilot_overlay,
            authority,
        )
        for ordinal, run in enumerate(authority["runs"], start=1):
            run["cleanup"] = cleanup_run_evidence(authority, run, receipt_root, ordinal)
        alias_cleanup = cleanup_alias_evidence(authority, receipt_root)
        require(
            not authority["retainedNames"] and not authority["aliasLive"],
            "pilot oracle cleanup is incomplete",
        )
        execution_evidence = {
            "aliasCleanup": alias_cleanup,
            "compilerIdentities": identities,
            "containerCli": authority["containerCli"],
            "counts": copy.deepcopy(EXECUTION_COUNTS),
            "preflight": authority["preflight"],
            "runOrderSha256": digest(canonical(expected_run_ids)),
            "runs": authority["runs"],
            "schemaVersion": 1,
        }
        exact_keys(
            execution_evidence,
            EXECUTION_EVIDENCE_KEYS,
            "measured executionEvidence",
        )
        return {
            "executionEvidence": execution_evidence,
            "observations": subject_values,
        }
    except Exception:
        if authority is not None:
            best_effort_oracle_cleanup(authority)
        raise


def assemble_pilot_receipt(
    audited: dict[str, Any],
    pilot_authority: dict[str, Any],
    pilot_overlay: dict[str, Any],
    observations: dict[str, Any],
    execution_evidence: dict[str, Any],
    receipt_root: Path,
) -> dict[str, Any]:
    records: list[dict[str, Any]] = []
    pilots = {value["failureId"]: value for value in pilot_authority["pilots"]}
    for failure_id, model in sorted(pilot_overlay["models"].items()):
        measured = observations[failure_id]
        atoms: list[dict[str, Any]] = []
        removable = 0
        for atom in measured["atoms"]:
            subject = atom["subjectObservation"]
            references = atom["referenceAcceptance"]
            subject_matches = (
                subject["normalizedObservation"] == signature(model["record"])
                and subject["causalAnchorSha256"] == model["anchorSha256"]
            )
            interesting = (
                subject_matches
                and references["gcc"]["returnCode"] == 0
                and not references["gcc"]["timedOut"]
                and references["clang"]["returnCode"] == 0
                and not references["clang"]["timedOut"]
            )
            removable += int(interesting)
            atoms.append(
                {
                    "atomId": atom["atomId"],
                    "candidateDigest": atom["variant"]["candidateDigest"],
                    "interesting": interesting,
                    "referenceAcceptance": references,
                    "subjectObservation": subject,
                }
            )
        require(
            removable == 0,
            f"pilot candidate has removable deletion atom: {failure_id}",
        )
        atom_ids = [value["atomId"] for value in atoms]
        records.append(
            {
                "candidate": {
                    "digest": model["candidateDigest"],
                    "kind": model["kind"],
                },
                "causalAnchorSha256": model["anchorSha256"],
                "failureId": failure_id,
                "minimality": {
                    "atomOrderSha256": digest(canonical(atom_ids)),
                    "atoms": atoms,
                    "claim": pilots[failure_id]["minimalityClaim"],
                    "removableAtoms": removable,
                    "testedAtoms": len(atoms),
                    "totalAtoms": len(atoms),
                },
                "referenceAcceptance": measured["referenceAcceptance"],
                "sourceRecordSha256": obligation(model["record"])["sourceRecordSha256"],
                "state": "VERIFIED",
                "subjectReplays": measured["subjectReplays"],
            }
        )
    require(
        [value["failureId"] for value in records] == pilot_authority["pilotIds"],
        "pilot receipt record order drift",
    )
    return {
        "artifactClosure": file_closure(receipt_root),
        "executionEvidence": execution_evidence,
        "executionAuthority": copy.deepcopy(
            audited["policy"]["sourceEvidence"]["executionAuthority"]
        ),
        "obligationManifestSha256": audited["manifestSha256"],
        "overlayClosureSha256": pilot_overlay["closureSha256"],
        "pilotContractSha256": pilot_authority["sha256"],
        "pilotFailureIds": pilot_authority["pilotIds"],
        "policyId": audited["policy"]["policyId"],
        "policySha256": audited["policySha256"],
        "records": records,
        "schemaVersion": 2,
        "sourceEvidenceRunId": audited["sourceEvidenceRunId"],
    }


def pilot_result(
    audited: dict[str, Any],
    pilot_authority: dict[str, Any],
    receipt: dict[str, Any],
    receipt_sha256: str,
    mode: str,
) -> dict[str, Any]:
    verdict = pilot_authority["contract"]["verdictSemantics"]
    records = receipt["records"]
    result = {
        "acceptanceValid": verdict["acceptanceValid"],
        "globalVerdict": verdict["globalVerdict"],
        "mode": mode,
        "obligationManifestSha256": audited["manifestSha256"],
        "pilotAcceptanceValid": verdict["pilotAcceptanceValid"],
        "pilotContractSha256": pilot_authority["sha256"],
        "policySha256": audited["policySha256"],
        "receiptSha256": receipt_sha256,
        "records": [
            {
                "failureId": value["failureId"],
                "receiptSha256": digest(canonical(value)),
                "state": value["state"],
            }
            for value in records
        ],
        "remainingFailures": verdict["remainingFailures"],
        "sourceEvidenceRunId": audited["sourceEvidenceRunId"],
        "sourceFailures": verdict["sourceFailures"],
        "verifiedPilots": verdict["verifiedPilotRecords"],
        "verdict": "PILOT_VERIFIED",
    }
    exact_keys(result, PILOT_RESULT_KEYS, "pilot result")
    return result


def verify_pilots_phase2(
    policy_path: Path,
    pilot_contract_path: Path,
    evidence: Path,
    overlay: Path,
    receipt_dir: Path,
    observer_ready_file: Path | None = None,
    observer_ack_file: Path | None = None,
) -> dict[str, Any]:
    require(not receipt_dir.exists(), "pilot receipt directory already exists")
    require(
        (observer_ready_file is None) == (observer_ack_file is None),
        "observer ready/ack files must be supplied together",
    )
    audited, pilot_authority, pilot_overlay = prepare_pilot_scope(
        policy_path, pilot_contract_path, evidence, overlay
    )
    receipt_dir.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix=".day5-pilot-receipt-staging-", dir=receipt_dir.parent
    ) as receipt_staging, tempfile.TemporaryDirectory(
        prefix="c30-day5-pilot-execution-"
    ) as execution_staging:
        observations = execute_subject_checkpoint(
            audited,
            pilot_overlay,
            Path(receipt_staging),
            Path(execution_staging),
        )
        oracle_values = execute_oracle_checkpoint(
            audited,
            pilot_authority,
            pilot_overlay,
            observations,
            Path(receipt_staging),
            Path(execution_staging),
            observer_ready_file,
            observer_ack_file,
        )
        receipt = assemble_pilot_receipt(
            audited,
            pilot_authority,
            pilot_overlay,
            oracle_values["observations"],
            oracle_values["executionEvidence"],
            Path(receipt_staging),
        )
        receipt_bytes = canonical(receipt)
        receipt_sha256 = digest(receipt_bytes)
        receipt_path = Path(receipt_staging) / "receipt.json"
        receipt_path.write_bytes(receipt_bytes)
        require(receipt_path.stat().st_nlink == 1, "pilot receipt is hard-linked")
        require(
            not receipt_dir.exists(),
            "pilot receipt directory appeared during execution",
        )
        os.rename(receipt_staging, receipt_dir)
    return pilot_result(audited, pilot_authority, receipt, receipt_sha256, "executed")


def expected_execution_run_ids(pilot_overlay: dict[str, Any]) -> list[str]:
    values = ["identity:gcc", "identity:clang"]
    for failure_id, model in sorted(pilot_overlay["models"].items()):
        values.extend(f"baseline:{failure_id}:{family}" for family in ("gcc", "clang"))
        for index, _ in enumerate(model["atoms"]):
            values.extend(
                f"atom:{failure_id}:{index:03d}:{family}" for family in ("gcc", "clang")
            )
    require(len(values) == 46, "receipt execution denominator drift")
    return values


def validate_receipt_artifact(
    receipt_root: Path,
    value: Any,
    label: str,
    seen: set[str],
) -> dict[str, Any]:
    artifact = descriptor_schema(value, label)
    relative = safe_relative(artifact["path"], f"{label}.path")
    require(relative != "receipt.json", f"{label} aliases receipt.json")
    require(relative not in seen, f"receipt artifact is referenced twice: {relative}")
    path = confined_path(receipt_root, relative, label)
    info = path.stat()
    require(info.st_nlink == 1, f"receipt artifact is hard-linked: {relative}")
    require(
        descriptor(path, relative) == artifact, f"receipt artifact drift: {relative}"
    )
    seen.add(relative)
    return artifact


def receipt_artifact_json(
    receipt_root: Path, artifact: dict[str, Any], label: str
) -> dict[str, Any]:
    path = confined_path(receipt_root, artifact["path"], label)
    value = decode_json(path.read_bytes(), label)
    if isinstance(value, list):
        require(len(value) == 1, f"{label} contains multiple descriptors")
        value = value[0]
    require(isinstance(value, dict), f"{label} is not a descriptor object")
    return value


def validate_receipt_image_artifact(
    receipt_root: Path,
    value: Any,
    label: str,
    seen: set[str],
    expected_image: str,
) -> None:
    artifact = validate_receipt_artifact(receipt_root, value, label, seen)
    inspected = receipt_artifact_json(receipt_root, artifact, label)
    observed = "sha256:" + str(inspected.get("id", "")).removeprefix("sha256:")
    require(observed == expected_image, f"{label} image identity drift")


def validate_receipt_environment(value: Any, label: str) -> dict[str, str]:
    environment = exact_keys(value, EXECUTION_ENVIRONMENT_KEYS, f"{label} environment")
    for name, item in environment.items():
        require(isinstance(item, str), f"{label} environment {name} is not text")
    require(
        environment["LANG"] == "C"
        and environment["LC_ALL"] == "C"
        and environment["TZ"] == "UTC",
        f"{label} locale/timezone environment drift",
    )
    return environment


def validate_receipt_cleanup(
    receipt_root: Path,
    value: Any,
    label: str,
    seen: set[str],
) -> None:
    cleanup = exact_keys(value, CLEANUP_EVIDENCE_KEYS, label)
    require(cleanup["deleteReturnCode"] == 0, f"{label} delete failed")
    require(
        is_int(cleanup["postDeleteInspectReturnCode"])
        and cleanup["postDeleteInspectReturnCode"] != 0,
        f"{label} post-delete inspection did not prove absence",
    )
    for name in (
        "deleteStdout",
        "deleteStderr",
        "postDeleteInspectStdout",
        "postDeleteInspectStderr",
    ):
        validate_receipt_artifact(receipt_root, cleanup[name], f"{label}.{name}", seen)


def validate_execution_evidence_receipt(
    audited: dict[str, Any],
    pilot_overlay: dict[str, Any],
    receipt_root: Path,
    value: Any,
    seen: set[str],
) -> dict[str, Any]:
    evidence = exact_keys(value, EXECUTION_EVIDENCE_KEYS, "receipt executionEvidence")
    require(evidence["schemaVersion"] == 1, "executionEvidence schemaVersion drift")
    require(
        exact_keys(evidence["counts"], set(EXECUTION_COUNTS), "execution counts")
        == EXECUTION_COUNTS,
        "execution evidence counts drift",
    )
    policy = audited["policy"]
    source_artifacts = policy["sourceEvidence"]["authorityArtifacts"]
    host = load_json(
        confined_path(
            audited["evidence"],
            source_artifacts["hostBuildAuthority"]["path"],
            "receipt host build authority",
        ),
        "receipt host build authority",
    )
    build = load_json(
        confined_path(
            audited["evidence"],
            source_artifacts["buildEnvironmentContract"]["path"],
            "receipt build environment",
        ),
        "receipt build environment",
    )
    execution = policy["sourceEvidence"]["executionAuthority"]
    container_tool = host["tools"]["container"]
    require(
        exact_keys(
            evidence["containerCli"],
            {"bytes", "path", "sha256", "version"},
            "receipt container CLI",
        )
        == {
            "bytes": container_tool["bytes"],
            "path": container_tool["path"],
            "sha256": execution["containerCliSha256"],
            "version": build["containerRuntime"]["version"],
        },
        "receipt container CLI authority drift",
    )
    preflight = exact_keys(
        evidence["preflight"],
        {
            "alias",
            "aliasImage",
            "discoveryImage",
            "discoveryReference",
            "expectedImage",
        },
        "receipt execution preflight",
    )
    require(
        preflight["expectedImage"] == execution["sealedImageId"],
        "receipt expected image drift",
    )
    require(
        preflight["discoveryReference"] == build["buildImage"]["sealedImageTag"],
        "receipt discovery image reference drift",
    )
    alias = preflight["alias"]
    require(
        isinstance(alias, str)
        and alias.startswith("c30-day5-pilot-")
        and alias.endswith(":bound"),
        "receipt image alias is invalid",
    )
    for name in ("discoveryImage", "aliasImage"):
        validate_receipt_image_artifact(
            receipt_root,
            preflight[name],
            f"receipt preflight {name}",
            seen,
            execution["sealedImageId"],
        )
    expected_ids = expected_execution_run_ids(pilot_overlay)
    runs = evidence["runs"]
    require(isinstance(runs, list) and len(runs) == 46, "receipt run count drift")
    require(
        [item.get("runId") for item in runs if isinstance(item, dict)] == expected_ids,
        "receipt execution run set/order drift",
    )
    require(
        evidence["runOrderSha256"] == digest(canonical(expected_ids)),
        "receipt execution run order hash drift",
    )
    run_map: dict[str, dict[str, Any]] = {}
    container_names: set[str] = set()
    for index, run_value in enumerate(runs):
        run = exact_keys(run_value, RUN_EVIDENCE_KEYS, f"execution runs[{index}]")
        run_id = run["runId"]
        parts = run_id.split(":")
        if parts[0] == "identity" and len(parts) == 2:
            purpose, family = "compiler-identity", parts[1]
        elif parts[0] == "baseline" and len(parts) == 3:
            purpose, family = "baseline-acceptance", parts[2]
        elif parts[0] == "atom" and len(parts) == 4:
            purpose, family = "minimality-deletion", parts[3]
        else:
            raise ContractError(f"invalid receipt execution runId: {run_id}")
        require(family in {"gcc", "clang"}, f"receipt run family drift: {run_id}")
        require(
            run["purpose"] == purpose and run["family"] == family,
            f"receipt run purpose/family drift: {run_id}",
        )
        name = require_string(run["containerName"], f"{run_id}.containerName")
        require(
            name.startswith("c30-pilot-") and name not in container_names,
            f"receipt container name invalid/reused: {run_id}",
        )
        container_names.add(name)
        if purpose == "compiler-identity":
            require(
                run["inputClosureSha256"] is None,
                f"identity run has candidate inputs: {run_id}",
            )
        else:
            require_hash(run["inputClosureSha256"], f"{run_id} input closure")
        for artifact_name in ("imageBefore", "imageAfter"):
            validate_receipt_image_artifact(
                receipt_root,
                run[artifact_name],
                f"{run_id}.{artifact_name}",
                seen,
                execution["sealedImageId"],
            )
        before_artifact = validate_receipt_artifact(
            receipt_root, run["containerBefore"], f"{run_id}.containerBefore", seen
        )
        after_artifact = validate_receipt_artifact(
            receipt_root, run["containerAfter"], f"{run_id}.containerAfter", seen
        )
        validate_receipt_cleanup(
            receipt_root, run["cleanup"], f"{run_id}.cleanup", seen
        )
        run_map[run_id] = {
            "after": receipt_artifact_json(
                receipt_root, after_artifact, f"{run_id}.containerAfter"
            ),
            "before": receipt_artifact_json(
                receipt_root, before_artifact, f"{run_id}.containerBefore"
            ),
            "value": run,
        }
    identities = exact_keys(
        evidence["compilerIdentities"],
        {"gcc", "clang"},
        "receipt compiler identities",
    )
    source_date_epoch = require_string(
        build.get("reproducibility", {}).get("SOURCE_DATE_EPOCH"),
        "receipt SOURCE_DATE_EPOCH",
    )
    expected_environment = oracle_environment(source_date_epoch)
    for family in ("gcc", "clang"):
        identity = exact_keys(
            identities[family],
            {
                "command",
                "commandSha256",
                "compilerPath",
                "compilerSha256",
                "compilerVersion",
                "environment",
                "executionRunId",
                "stderr",
                "stderrSha256",
                "stdout",
                "stdoutSha256",
            },
            f"receipt {family} compiler identity",
        )
        compiler = execution["referenceCompilers"][family]
        command = [
            "/bin/sh",
            "-c",
            '/usr/bin/sha256sum "$1"; "$1" --version',
            "c30-compiler-identity",
            compiler["path"],
        ]
        require(
            identity["executionRunId"] == f"identity:{family}"
            and identity["command"] == command
            and identity["commandSha256"] == digest(canonical(command))
            and identity["compilerPath"] == compiler["path"]
            and identity["compilerSha256"] == compiler["sha256"]
            and identity["compilerVersion"] == compiler["version"]
            and identity["environment"] == expected_environment,
            f"receipt {family} compiler identity authority drift",
        )
        stdout = validate_receipt_artifact(
            receipt_root, identity["stdout"], f"identity {family} stdout", seen
        )
        stderr = validate_receipt_artifact(
            receipt_root, identity["stderr"], f"identity {family} stderr", seen
        )
        require(
            identity["stdoutSha256"] == stdout["sha256"]
            and identity["stderrSha256"] == stderr["sha256"],
            f"receipt {family} identity stream hash drift",
        )
        lines = (
            confined_path(receipt_root, stdout["path"], f"identity {family} stdout")
            .read_text(encoding="utf-8", errors="strict")
            .splitlines()
        )
        require(
            bool(lines)
            and lines[0].split()[0] == compiler["sha256"]
            and len(lines) >= 2
            and lines[1] == compiler["version"],
            f"receipt {family} measured identity drift",
        )
        run = run_map[f"identity:{family}"]
        validate_container_pair(
            run["before"],
            run["after"],
            run["value"]["containerName"],
            alias,
            execution["sealedImageId"],
            command,
            expected_environment,
            False,
        )
    validate_receipt_cleanup(
        receipt_root, evidence["aliasCleanup"], "receipt alias cleanup", seen
    )
    return {"alias": alias, "runs": run_map}


def validate_subject_receipt_value(
    audited: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    receipt_root: Path,
    value: Any,
    label: str,
    seen: set[str],
    *,
    baseline: bool,
) -> dict[str, Any]:
    receipt = exact_keys(value, SUBJECT_RECEIPT_KEYS, label)
    require(
        receipt["candidateDigest"] == variant["candidateDigest"],
        f"{label} candidate digest drift",
    )
    require(
        receipt["inputClosureSha256"] == closure_sha256(variant["inputs"]["subject"]),
        f"{label} input closure drift",
    )
    execution = audited["policy"]["sourceEvidence"]["executionAuthority"]
    require(
        receipt["subjectSha256"] == execution["subjectSha256"],
        f"{label} subject identity drift",
    )
    environment = validate_receipt_environment(receipt["environment"], label)
    require(
        receipt["environmentSha256"] == digest(canonical(environment)),
        f"{label} environment hash drift",
    )
    private = exact_keys(
        receipt["privateDirectory"],
        {"entriesAfterReset", "home", "resetBeforeExecution", "tmpdir"},
        f"{label}.privateDirectory",
    )
    require(
        private["resetBeforeExecution"] is True and private["entriesAfterReset"] == [],
        f"{label} private directory was not reset",
    )
    home = Path(require_string(private["home"], f"{label} private HOME"))
    temporary = Path(require_string(private["tmpdir"], f"{label} private TMPDIR"))
    require(
        home.is_absolute()
        and temporary.is_absolute()
        and home != temporary
        and home.parent == temporary.parent
        and environment["HOME"] == str(home)
        and environment["TMPDIR"] == str(temporary),
        f"{label} private path/environment drift",
    )
    subject_path = confined_path(
        audited["evidence"],
        execution["subjectEvidenceRelativePath"],
        f"{label} subject executable",
    )
    inputs = home.parent / "inputs"
    expected_argv = (
        [
            str(subject_path),
            "--parse-only",
            "--target=aarch64-unknown-linux-gnu",
            str(inputs / "candidate.c"),
        ]
        if model["kind"] == "preprocessed-parser"
        else raw_subject_argv(audited, model, inputs, subject_path)
    )
    require(
        receipt["argv"] == expected_argv
        and receipt["argvSha256"] == digest(canonical(expected_argv)),
        f"{label} subject argv drift",
    )
    stdout = validate_receipt_artifact(
        receipt_root, receipt["stdout"], f"{label}.stdout", seen
    )
    stderr = validate_receipt_artifact(
        receipt_root, receipt["stderr"], f"{label}.stderr", seen
    )
    require(
        receipt["stdoutSha256"] == stdout["sha256"]
        and receipt["stderrSha256"] == stderr["sha256"],
        f"{label} stream hash drift",
    )
    require(is_int(receipt["returnCode"]), f"{label} returnCode is not an integer")
    require(receipt["timedOut"] is False, f"{label} timed out")
    if baseline:
        require(
            receipt["normalizedObservation"] == signature(model["record"])
            and receipt["causalAnchorSha256"] == model["anchorSha256"],
            f"{label} does not reproduce the frozen causal failure",
        )
    else:
        require(
            receipt["causalAnchorSha256"] in {None, model["anchorSha256"]},
            f"{label} has an alien causal anchor",
        )
        require(
            receipt["normalizedObservation"] is None
            or isinstance(receipt["normalizedObservation"], dict),
            f"{label} normalized observation has invalid type",
        )
    return receipt


def validate_oracle_receipt_value(
    audited: dict[str, Any],
    model: dict[str, Any],
    variant: dict[str, Any],
    family: str,
    expected_run_id: str,
    execution_evidence: dict[str, Any],
    receipt_root: Path,
    overlay_root: Path,
    value: Any,
    label: str,
    seen: set[str],
    *,
    require_acceptance: bool,
) -> dict[str, Any]:
    receipt = exact_keys(value, ORACLE_RECEIPT_KEYS, label)
    execution = audited["policy"]["sourceEvidence"]["executionAuthority"]
    compiler = execution["referenceCompilers"][family]
    require(
        receipt["compilerPath"] == compiler["path"]
        and receipt["compilerSha256"] == compiler["sha256"]
        and receipt["compilerVersion"] == compiler["version"]
        and receipt["imageId"] == execution["sealedImageId"],
        f"{label} compiler/image authority drift",
    )
    require(
        receipt["candidateDigest"] == variant["candidateDigest"]
        and receipt["executionRunId"] == expected_run_id,
        f"{label} candidate/run binding drift",
    )
    inputs = variant["inputs"][family]
    input_closure = closure_sha256(inputs)
    require(
        receipt["containerInputs"] == input_descriptors(inputs)
        and receipt["inputClosureSha256"] == input_closure,
        f"{label} container input closure drift",
    )
    expected_command = (
        [
            item.replace("<exact-private-candidate>", "/inputs/candidate.c")
            for item in audited["policy"]["referenceAcceptance"]["parserCommands"][
                family
            ]
        ]
        if model["kind"] == "preprocessed-parser"
        else raw_oracle_command(audited, model, family)
    )
    require(
        receipt["command"] == expected_command
        and receipt["commandSha256"] == digest(canonical(expected_command)),
        f"{label} compiler command drift",
    )
    source_artifacts = audited["policy"]["sourceEvidence"]["authorityArtifacts"]
    build = load_json(
        confined_path(
            audited["evidence"],
            source_artifacts["buildEnvironmentContract"]["path"],
            f"{label} build environment",
        ),
        f"{label} build environment",
    )
    expected_environment = oracle_environment(
        require_string(
            build.get("reproducibility", {}).get("SOURCE_DATE_EPOCH"),
            f"{label} SOURCE_DATE_EPOCH",
        )
    )
    require(
        receipt["environment"] == expected_environment,
        f"{label} compiler environment drift",
    )
    serialized = canonical(
        {"command": receipt["command"], "environment": receipt["environment"]}
    ).decode("utf-8")
    for forbidden in (
        str(audited["policyPath"]),
        str(audited["evidence"]),
        str(receipt_root),
        str(overlay_root),
        "expectedResult",
        "expected-result",
        "golden",
        "receipt.json",
        "sourceRecordSha256",
        "failureSignatureSha256",
    ):
        require(
            forbidden not in serialized, f"{label} leaks oracle authority: {forbidden}"
        )
    stdout = validate_receipt_artifact(
        receipt_root, receipt["stdout"], f"{label}.stdout", seen
    )
    stderr = validate_receipt_artifact(
        receipt_root, receipt["stderr"], f"{label}.stderr", seen
    )
    require(
        receipt["stdoutSha256"] == stdout["sha256"]
        and receipt["stderrSha256"] == stderr["sha256"],
        f"{label} stream hash drift",
    )
    require(is_int(receipt["returnCode"]), f"{label} returnCode is not integer")
    require(receipt["timedOut"] is False, f"{label} timed out")
    if require_acceptance:
        require(receipt["returnCode"] == 0, f"{label} rejected the exact candidate")
    run = execution_evidence["runs"].get(expected_run_id)
    require(isinstance(run, dict), f"{label} execution descriptor is absent")
    require(
        run["value"]["inputClosureSha256"] == input_closure,
        f"{label} execution descriptor input closure drift",
    )
    validate_container_pair(
        run["before"],
        run["after"],
        run["value"]["containerName"],
        execution_evidence["alias"],
        execution["sealedImageId"],
        expected_command,
        expected_environment,
        True,
    )
    return receipt


def validate_pilot_receipt(
    audited: dict[str, Any],
    pilot_authority: dict[str, Any],
    pilot_overlay: dict[str, Any],
    receipt_dir: Path,
    expected_receipt_sha256: str,
) -> tuple[dict[str, Any], str]:
    receipt_root = require_plain_root(receipt_dir, "pilot receipt root")
    file_closure(receipt_root)
    receipt_path = confined_path(receipt_root, "receipt.json", "pilot receipt")
    require(receipt_path.stat().st_nlink == 1, "pilot receipt is hard-linked")
    receipt_bytes = receipt_path.read_bytes()
    receipt_sha256 = digest(receipt_bytes)
    require(
        receipt_sha256 == expected_receipt_sha256,
        "pilot receipt does not match caller expected SHA-256",
    )
    receipt_value = decode_json(receipt_bytes, "pilot receipt")
    require(isinstance(receipt_value, dict), "pilot receipt root is not an object")
    receipt = exact_keys(receipt_value, PILOT_RECEIPT_KEYS, "pilot receipt")
    require(canonical(receipt) == receipt_bytes, "pilot receipt is not canonical JSON")
    require(receipt["schemaVersion"] == 2, "pilot receipt schemaVersion drift")
    require(
        receipt["policyId"] == audited["policy"]["policyId"]
        and receipt["policySha256"] == audited["policySha256"]
        and receipt["sourceEvidenceRunId"] == audited["sourceEvidenceRunId"]
        and receipt["obligationManifestSha256"] == audited["manifestSha256"]
        and receipt["pilotContractSha256"] == pilot_authority["sha256"]
        and receipt["pilotFailureIds"] == pilot_authority["pilotIds"]
        and receipt["executionAuthority"]
        == audited["policy"]["sourceEvidence"]["executionAuthority"]
        and receipt["overlayClosureSha256"] == pilot_overlay["closureSha256"],
        "pilot receipt root authority drift",
    )
    artifact_closure = exact_keys(
        receipt["artifactClosure"],
        {"bytes", "files", "sha256"},
        "pilot receipt artifactClosure",
    )
    require(
        artifact_closure == file_closure(receipt_root, excluded={"receipt.json"}),
        "pilot receipt artifact closure drift",
    )
    seen: set[str] = set()
    execution_evidence = validate_execution_evidence_receipt(
        audited,
        pilot_overlay,
        receipt_root,
        receipt["executionEvidence"],
        seen,
    )
    records = receipt["records"]
    require(
        isinstance(records, list) and len(records) == len(pilot_authority["pilotIds"]),
        "pilot receipt record denominator drift",
    )
    require(
        [item.get("failureId") for item in records if isinstance(item, dict)]
        == pilot_authority["pilotIds"],
        "pilot receipt records are not exact/sorted",
    )
    stable_fields = {
        "argvSha256",
        "candidateDigest",
        "environmentSha256",
        "returnCode",
        "stderrSha256",
        "stdoutSha256",
        "subjectSha256",
        "timedOut",
    }
    for record_index, value in enumerate(records):
        record = exact_keys(
            value,
            PILOT_RECEIPT_RECORD_KEYS,
            f"pilot receipt.records[{record_index}]",
        )
        failure_id = record["failureId"]
        model = pilot_overlay["models"][failure_id]
        source_record = model["record"]
        require(record["state"] == "VERIFIED", f"pilot state drift: {failure_id}")
        require(
            record["sourceRecordSha256"]
            == obligation(source_record)["sourceRecordSha256"]
            and record["candidate"]
            == {"digest": model["candidateDigest"], "kind": model["kind"]}
            and record["causalAnchorSha256"] == model["anchorSha256"],
            f"pilot receipt identity drift: {failure_id}",
        )
        baseline = (
            parser_variant(model, None)
            if model["kind"] == "preprocessed-parser"
            else raw_variant(model, None, None)
        )
        replay_values = record["subjectReplays"]
        require(
            isinstance(replay_values, list) and len(replay_values) == 3,
            f"pilot subject replay denominator drift: {failure_id}",
        )
        replays = [
            validate_subject_receipt_value(
                audited,
                model,
                baseline,
                receipt_root,
                replay,
                f"{failure_id} subject replay {index}",
                seen,
                baseline=True,
            )
            for index, replay in enumerate(replay_values)
        ]
        stable = [{name: replay[name] for name in stable_fields} for replay in replays]
        require(
            stable[0] == stable[1] == stable[2]
            and replays[0]["environment"]
            == replays[1]["environment"]
            == replays[2]["environment"]
            and replays[0]["privateDirectory"]
            == replays[1]["privateDirectory"]
            == replays[2]["privateDirectory"],
            f"pilot subject replay instability: {failure_id}",
        )
        references = exact_keys(
            record["referenceAcceptance"],
            {"gcc", "clang"},
            f"{failure_id} baseline references",
        )
        baseline_oracles = {
            family: validate_oracle_receipt_value(
                audited,
                model,
                baseline,
                family,
                f"baseline:{failure_id}:{family}",
                execution_evidence,
                receipt_root,
                pilot_overlay["root"],
                references[family],
                f"{failure_id} baseline {family}",
                seen,
                require_acceptance=True,
            )
            for family in ("gcc", "clang")
        }
        require(
            baseline_oracles["gcc"]["compilerSha256"]
            != baseline_oracles["clang"]["compilerSha256"],
            f"pilot baseline compiler identities alias: {failure_id}",
        )
        minimality = exact_keys(
            record["minimality"],
            MINIMALITY_RECEIPT_KEYS,
            f"{failure_id} minimality",
        )
        expected_atoms = model["atoms"]
        expected_atom_ids = [atom["atomId"] for atom in expected_atoms]
        expected_claim = (
            audited["policy"]["overlayPolicy"]["parserCandidate"]["minimality"]
            if model["kind"] == "preprocessed-parser"
            else audited["policy"]["overlayPolicy"]["rawCandidate"]["minimality"]
        )
        require(
            minimality["claim"] == expected_claim
            and minimality["totalAtoms"]
            == minimality["testedAtoms"]
            == len(expected_atoms)
            and minimality["removableAtoms"] == 0
            and minimality["atomOrderSha256"] == digest(canonical(expected_atom_ids)),
            f"pilot minimality authority drift: {failure_id}",
        )
        atoms = minimality["atoms"]
        require(
            isinstance(atoms, list) and len(atoms) == len(expected_atoms),
            f"pilot minimality atom denominator drift: {failure_id}",
        )
        for atom_index, (atom_value, expected_atom) in enumerate(
            zip(atoms, expected_atoms)
        ):
            atom = exact_keys(
                atom_value,
                ATOM_RECEIPT_KEYS,
                f"{failure_id} minimality atom {atom_index}",
            )
            variant = expected_atom["variant"]
            require(
                atom["atomId"] == expected_atom["atomId"]
                and atom["candidateDigest"] == variant["candidateDigest"],
                f"pilot minimality atom identity drift: {failure_id}:{atom_index}",
            )
            subject = validate_subject_receipt_value(
                audited,
                model,
                variant,
                receipt_root,
                atom["subjectObservation"],
                f"{failure_id} atom {atom_index} subject",
                seen,
                baseline=False,
            )
            atom_references = exact_keys(
                atom["referenceAcceptance"],
                {"gcc", "clang"},
                f"{failure_id} atom {atom_index} references",
            )
            oracle_values = {
                family: validate_oracle_receipt_value(
                    audited,
                    model,
                    variant,
                    family,
                    f"atom:{failure_id}:{atom_index:03d}:{family}",
                    execution_evidence,
                    receipt_root,
                    pilot_overlay["root"],
                    atom_references[family],
                    f"{failure_id} atom {atom_index} {family}",
                    seen,
                    require_acceptance=False,
                )
                for family in ("gcc", "clang")
            }
            subject_matches = (
                subject["normalizedObservation"] == signature(source_record)
                and subject["causalAnchorSha256"] == model["anchorSha256"]
            )
            interesting = (
                subject_matches
                and oracle_values["gcc"]["returnCode"] == 0
                and not oracle_values["gcc"]["timedOut"]
                and oracle_values["clang"]["returnCode"] == 0
                and not oracle_values["clang"]["timedOut"]
            )
            require(
                atom["interesting"] is interesting and not interesting,
                f"pilot minimality interestingness drift: {failure_id}:{atom_index}",
            )
    require(
        seen == {item["path"] for item in artifact_closure["files"]},
        "pilot receipt has unreferenced or multiply referenced artifacts",
    )
    require(
        sum(len(model["atoms"]) for model in pilot_overlay["models"].values()) == 19,
        "pilot receipt deletion atom denominator drift",
    )
    return receipt, receipt_sha256


def replay_pilot_receipt_phase2(
    policy_path: Path,
    pilot_contract_path: Path,
    evidence: Path,
    overlay: Path,
    receipt_dir: Path,
    expected_receipt_sha256: str,
) -> dict[str, Any]:
    require_hash(expected_receipt_sha256, "expected pilot receipt SHA-256")
    audited, pilot_authority, pilot_overlay = prepare_pilot_scope(
        policy_path, pilot_contract_path, evidence, overlay
    )
    receipt, receipt_sha256 = validate_pilot_receipt(
        audited,
        pilot_authority,
        pilot_overlay,
        receipt_dir,
        expected_receipt_sha256,
    )
    return pilot_result(
        audited, pilot_authority, receipt, receipt_sha256, "receipt-replay"
    )


def store(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))


def synthetic_failure() -> dict[str, Any]:
    return {
        "artifacts": {},
        "classification": "parse",
        "diagnostic": {
            "code": "E1002",
            "emittedPath": "/synthetic/parser/synthetic.gcc.c",
            "emittedSpan": {"column": 1, "line": 2},
            "message": "synthetic-stable-failure",
            "sourcePath": "include/synthetic.h",
            "sourceSpan": {"column": None, "line": 7},
        },
        "lane": "gcc-preprocessed-parser",
        "outcome": "failure",
        "phase": "oracle-preprocessed-parse",
        "returnCode": 1,
        "sourcePath": "src/synthetic.c",
        "timedOut": False,
        "tuId": "synthetic",
    }


def build_internal_fixture(root: Path) -> tuple[Path, Path, Path]:
    base = load_json(DEFAULT_POLICY, "checked-in policy")
    run_id = "synthetic-internal"
    evidence = root / run_id
    evidence.mkdir()
    record = synthetic_failure()
    parser_root = evidence / "parser"
    oracle_root = evidence / "oracle-gcc"
    parser_root.mkdir()
    oracle_root.mkdir()
    parser_files = {
        "parserInput": (
            parser_root / "synthetic.gcc.c",
            b"int synthetic;\nint broken(\n",
        ),
        "lineMap": (
            parser_root / "synthetic.gcc.linemap.jsonl",
            canonical(
                {"originLine": 6, "originPath": "include/synthetic.h", "parserLine": 1}
            )
            + canonical(
                {"originLine": 7, "originPath": "include/synthetic.h", "parserLine": 2}
            ),
        ),
        "stdout": (parser_root / "synthetic.gcc.stdout", b""),
        "stderr": (
            parser_root / "synthetic.gcc.stderr",
            b"/synthetic/parser/synthetic.gcc.c:2:1: error[E1002]: synthetic-stable-failure\n",
        ),
    }
    oracle_files = {
        "compilerStdout": (oracle_root / "synthetic.compiler.stdout", b""),
        "compilerStderr": (oracle_root / "synthetic.compiler.stderr", b""),
        "oraclePreprocessed": (
            oracle_root / "synthetic.oracle.i",
            b"int synthetic;\nint broken(\n",
        ),
    }
    for name, (path, data) in {**parser_files, **oracle_files}.items():
        path.write_bytes(data)
        record["artifacts"][name] = descriptor(path, path.name)
    raw_record: dict[str, Any] = {
        "artifacts": {},
        "classification": "preprocess",
        "diagnostic": {
            "code": "E0104",
            "emittedPath": "/synthetic/source/src/synthetic.c",
            "emittedSpan": {"column": 1, "line": 1},
            "message": "synthetic-raw-failure",
            "sourcePath": "src/synthetic.c",
            "sourceSpan": {"column": 1, "line": 1},
        },
        "lane": "evocpp-raw",
        "outcome": "failure",
        "phase": "raw-source-preprocess",
        "returnCode": 1,
        "sourcePath": "src/synthetic.c",
        "timedOut": False,
        "tuId": "synthetic",
    }
    raw_root = evidence / "evocpp-raw"
    raw_root.mkdir()
    raw_files = {
        "argv": (
            raw_root / "synthetic.argv.json",
            canonical(["evocc", "-E", "src/synthetic.c"]),
        ),
        "stdout": (raw_root / "synthetic.stdout", b""),
        "stderr": (
            raw_root / "synthetic.stderr",
            (
                f"{raw_record['diagnostic']['emittedPath']}:1:1: "
                "error[E0104]: synthetic-raw-failure\n"
            ).encode("utf-8"),
        ),
    }
    for name, (path, data) in raw_files.items():
        path.write_bytes(data)
        raw_record["artifacts"][name] = descriptor(path, path.name)

    clang_record: dict[str, Any] = {
        "artifacts": {},
        "classification": None,
        "diagnostic": None,
        "lane": "clang-preprocessed-parser",
        "outcome": "success",
        "phase": "oracle-preprocessed-parse",
        "returnCode": 0,
        "sourcePath": "src/synthetic.c",
        "timedOut": False,
        "tuId": "synthetic",
    }
    clang_root = evidence / "oracle-clang"
    clang_root.mkdir()
    clang_files = {
        "parserInput": (parser_root / "synthetic.clang.c", b"int synthetic;\n"),
        "lineMap": (
            parser_root / "synthetic.clang.linemap.jsonl",
            canonical(
                {"originLine": 1, "originPath": "src/synthetic.c", "parserLine": 1}
            ),
        ),
        "stdout": (parser_root / "synthetic.clang.stdout", b""),
        "stderr": (parser_root / "synthetic.clang.stderr", b""),
        "compilerStdout": (clang_root / "synthetic.clang.compiler.stdout", b""),
        "compilerStderr": (clang_root / "synthetic.clang.compiler.stderr", b""),
        "oraclePreprocessed": (
            clang_root / "synthetic.clang.oracle.i",
            b"int synthetic;\n",
        ),
    }
    for name, (path, data) in clang_files.items():
        path.write_bytes(data)
        clang_record["artifacts"][name] = descriptor(path, path.name)
    records = [raw_record, record, clang_record]
    (evidence / "results.jsonl").write_bytes(
        b"".join(canonical(value) for value in records)
    )
    seed = b"int synthetic;\n"
    gcc_output_seed = b"#define SYNTHETIC_GCC 1\n"
    clang_output_seed = b"#define SYNTHETIC_CLANG 1\n"
    output_logical = "output/generated/synthetic.h"
    source_logical = "source/src/synthetic.c"
    source_row = f"{source_logical}\t{digest(seed)}\t{len(seed)}\n".encode("ascii")
    gcc_per_tu_closure = (
        f"{output_logical}\t{digest(gcc_output_seed)}\t{len(gcc_output_seed)}\n".encode(
            "ascii"
        )
        + source_row
    )
    clang_per_tu_closure = (
        f"{output_logical}\t{digest(clang_output_seed)}\t{len(clang_output_seed)}\n".encode(
            "ascii"
        )
        + source_row
    )
    input_authorities = {
        "gcc": {
            "bytes": len(seed) + len(gcc_output_seed),
            "files": 2,
            "sha256": digest(gcc_per_tu_closure),
        },
        "clang": {
            "bytes": len(seed) + len(clang_output_seed),
            "files": 2,
            "sha256": digest(clang_per_tu_closure),
        },
    }
    execution = base["sourceEvidence"]["executionAuthority"]
    reference_compilers = execution["referenceCompilers"]
    build_environment = {
        "buildImage": {
            "executionReference": execution["sealedImageId"],
            "observedImageId": execution["sealedImageId"],
            "sealedImageId": execution["sealedImageId"],
        },
        "containerRuntime": {"cliSha256": execution["containerCliSha256"]},
        "contractState": "locked",
        "reproducibility": {"SOURCE_DATE_EPOCH": "1"},
        "schemaVersion": 1,
        "toolchain": {
            "clang": {
                "binaryPathInImage": reference_compilers["clang"]["path"],
                "binarySha256": reference_compilers["clang"]["sha256"],
                "resolvedVersion": reference_compilers["clang"]["version"],
            },
            "gcc": {
                "binaryPathInImage": reference_compilers["gcc"]["path"],
                "binarySha256": reference_compilers["gcc"]["sha256"],
                "resolvedVersion": reference_compilers["gcc"]["version"],
            },
            "hostReplay": {
                "cargo": {"sha256": execution["cargoSha256"]},
                "rustc": {"sha256": execution["rustcSha256"]},
            },
            "rust": {"rustcSha256": execution["rustcSha256"]},
        },
    }
    host_build_authority = {
        "schemaVersion": 1,
        "tools": {
            "cargo": {"sha256": execution["cargoSha256"]},
            "container": {"sha256": execution["containerCliSha256"]},
            "rustc": {"sha256": execution["rustcSha256"]},
        },
    }
    values: dict[str, Any] = {
        "kernel-cohort.json": {
            "contractState": "locked",
            "schemaVersion": 1,
            "translationUnits": [
                {
                    "commands": {
                        "clang": {"inputClosure": input_authorities["clang"]},
                        "gcc": {"inputClosure": input_authorities["gcc"]},
                    },
                    "id": "synthetic",
                    "sourceBytes": len(seed),
                    "sourcePath": "src/synthetic.c",
                    "sourceSha256": digest(seed),
                }
            ],
        },
        "build-environment.json": build_environment,
        "host-build-authority.json": host_build_authority,
    }
    for name, value in values.items():
        store(evidence / name, value)
    (evidence / "runner.py").write_text("# synthetic authority\n", encoding="utf-8")
    store(
        evidence / "run-config.json",
        {
            "buildEnvironmentContract": descriptor(
                evidence / "build-environment.json", "build-environment.json"
            ),
            "generalTimeoutSeconds": 1,
            "kernelCohortContract": descriptor(
                evidence / "kernel-cohort.json", "kernel-cohort.json"
            ),
            "parserJobs": 1,
            "parserTimeoutSecondsPerTu": 1,
            "requiredObservationLanes": REQUIRED_OBSERVATION_LANES,
            "runner": descriptor(evidence / "runner.py", "runner.py"),
            "schemaVersion": 1,
        },
    )
    inputs = evidence / "inputs"
    inputs_end = evidence / "inputs-end"
    inputs.mkdir()
    inputs_end.mkdir()
    seed_source = evidence / "seed/source/src"
    seed_source.mkdir(parents=True)
    (seed_source / "synthetic.c").write_bytes(seed)
    gcc_output_path = evidence / "seed/gcc-output/generated/synthetic.h"
    clang_output_path = evidence / "seed/clang-output/generated/synthetic.h"
    gcc_output_path.parent.mkdir(parents=True)
    clang_output_path.parent.mkdir(parents=True)
    gcc_output_path.write_bytes(gcc_output_seed)
    clang_output_path.write_bytes(clang_output_seed)
    family_closures = {
        "gcc": gcc_per_tu_closure,
        "clang": clang_per_tu_closure,
    }
    for family, per_tu_closure in family_closures.items():
        name = f"synthetic.{family}.inputs.tsv"
        (inputs / name).write_bytes(per_tu_closure)
        (inputs_end / name).write_bytes(per_tu_closure)
    directory_closure = b"".join(
        f"{path.name}\t{digest(path.read_bytes())}\t{path.stat().st_size}\n".encode(
            "ascii"
        )
        for path in sorted(inputs.iterdir())
    )
    (evidence / "kernel-input-closure.start.tsv").write_bytes(directory_closure)
    (evidence / "kernel-input-closure.end.tsv").write_bytes(directory_closure)
    build_closure = b"synthetic\t" + digest(b"synthetic").encode("ascii") + b"\t9\n"
    (evidence / "evocc-build-inputs.tsv").write_bytes(build_closure)
    (evidence / "evocc-build-inputs.end.tsv").write_bytes(build_closure)
    cohort_sha = digest((evidence / "kernel-cohort.json").read_bytes())
    subject_bytes = b"#!/bin/sh\nexit 1\n"
    subject_path = (
        evidence
        / base["sourceEvidence"]["executionAuthority"]["subjectEvidenceRelativePath"]
    )
    subject_path.parent.mkdir(parents=True)
    subject_path.write_bytes(subject_bytes)
    subject_path.chmod(0o700)
    subject_sha = digest(subject_bytes)
    (evidence / "contract.sha256").write_text(cohort_sha + "\n", encoding="ascii")
    (evidence / "evocc.sha256").write_text(subject_sha + "\n", encoding="ascii")
    names = {
        "results": "results.jsonl",
        "kernelCohortContract": "kernel-cohort.json",
        "buildEnvironmentContract": "build-environment.json",
        "hostBuildAuthority": "host-build-authority.json",
        "reconnaissanceRunner": "runner.py",
        "runConfig": "run-config.json",
        "kernelInputClosureStart": "kernel-input-closure.start.tsv",
        "kernelInputClosureEnd": "kernel-input-closure.end.tsv",
        "subjectBuildInputClosureStart": "evocc-build-inputs.tsv",
        "subjectBuildInputClosureEnd": "evocc-build-inputs.end.tsv",
        "kernelCohortDigestRecord": "contract.sha256",
        "subjectDigestRecord": "evocc.sha256",
    }
    artifacts = {
        name: descriptor(evidence / filename, filename)
        for name, filename in names.items()
    }
    summary = {
        "contractSha256": cohort_sha,
        "failures": 2,
        "observedRecords": 3,
        "pendingReductions": 2,
        "requiredRecords": 3,
        "resultsJsonl": artifacts["results"],
        "schemaVersion": 1,
        "successes": 1,
    }
    store(evidence / "summary.json", summary)
    artifacts["summary"] = descriptor(evidence / "summary.json", "summary.json")

    policy = copy.deepcopy(base)
    source = policy["sourceEvidence"]
    source.update(
        {
            "authorityArtifacts": artifacts,
            "expectedFailures": 2,
            "expectedObservations": 3,
            "expectedSuccesses": 1,
            "relativeRoot": run_id,
            "runId": run_id,
        }
    )
    source["executionAuthority"]["subjectSha256"] = subject_sha
    source["executionAuthority"]["rawSeedRoots"] = {
        "source": {"base": "evidence", "path": "seed/source"},
        "gccOutput": {"base": "evidence", "path": "seed/gcc-output"},
        "clangOutput": {"base": "evidence", "path": "seed/clang-output"},
    }
    derived = derive_obligations(records)
    policy["obligationDerivation"]["obligations"] = derived
    policy["obligationDerivation"]["obligationManifestSha256"] = obligation_manifest(
        derived
    )
    policy_path = root / "policy.json"
    store(policy_path, policy)
    overlay = root / "overlay"
    overlay.mkdir()
    candidate_data = (evidence / "parser/synthetic.gcc.c").read_bytes()
    candidate_path = overlay / "synthetic-candidate.c"
    candidate_path.write_bytes(candidate_data)
    seed_lines = physical_lines(candidate_data)
    parser_candidate = {
        "bytes": len(candidate_data),
        "kind": "preprocessed-parser",
        "lineage": [
            {
                "candidateLine": index,
                "seedLine": index,
                "seedLineSha256": digest(line),
            }
            for index, line in enumerate(seed_lines, start=1)
        ],
        "path": candidate_path.name,
        "sha256": digest(candidate_data),
    }
    raw_failure_id = obligation(raw_record)["failureId"]
    parser_failure_id = obligation(record)["failureId"]
    raw_logical = source_logical

    def internal_raw_closure(
        flavor: str, family: str, seeds: list[tuple[str, bytes]]
    ) -> dict[str, Any]:
        files: list[dict[str, Any]] = []
        rows: list[bytes] = []
        for logical, data in sorted(seeds):
            path = overlay / f"candidates/{raw_failure_id}/{flavor}/{logical}"
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
            files.append(
                {
                    "bytes": len(data),
                    "lineage": [
                        {
                            "candidateLine": index,
                            "seedLine": index,
                            "seedLineSha256": digest(line),
                        }
                        for index, line in enumerate(physical_lines(data), start=1)
                    ],
                    "logicalPath": logical,
                    "sha256": digest(data),
                }
            )
            rows.append(f"{logical}\t{digest(data)}\t{len(data)}\n".encode("ascii"))
        return {
            "closureSha256": digest(b"".join(rows)),
            "files": files,
            "root": raw_logical,
            "seedFamily": family,
        }

    gcc_seeds = [(source_logical, seed), (output_logical, gcc_output_seed)]
    clang_seeds = [(source_logical, seed), (output_logical, clang_output_seed)]
    subject_closure = internal_raw_closure("subject", "gcc", gcc_seeds)
    reference_gcc = internal_raw_closure("reference-gcc", "gcc", gcc_seeds)
    reference_clang = internal_raw_closure("reference-clang", "clang", clang_seeds)
    bundle_object = {
        "referenceClosures": {
            "clang": {
                "closureSha256": reference_clang["closureSha256"],
                "root": reference_clang["root"],
                "seedFamily": reference_clang["seedFamily"],
            },
            "gcc": {
                "closureSha256": reference_gcc["closureSha256"],
                "root": reference_gcc["root"],
                "seedFamily": reference_gcc["seedFamily"],
            },
        },
        "rootSourcePath": "src/synthetic.c",
        "subjectClosure": {
            "closureSha256": subject_closure["closureSha256"],
            "root": subject_closure["root"],
            "seedFamily": subject_closure["seedFamily"],
        },
    }
    raw_candidate = {
        "bundleSha256": digest(canonical(bundle_object)),
        "kind": "raw-closure",
        "referenceClosures": {"clang": reference_clang, "gcc": reference_gcc},
        "rootSourcePath": "src/synthetic.c",
        "subjectClosure": subject_closure,
    }
    overlay_records = sorted(
        [
            {
                "candidate": raw_candidate,
                "failureId": raw_failure_id,
                "state": "CANDIDATE",
            },
            {
                "candidate": parser_candidate,
                "failureId": parser_failure_id,
                "state": "CANDIDATE",
            },
        ],
        key=lambda value: value["failureId"],
    )
    store(
        overlay / "overlay.json",
        {
            "policyId": policy["policyId"],
            "records": overlay_records,
            "schemaVersion": 1,
            "sourceEvidenceRunId": source["runId"],
        },
    )
    return policy_path, evidence, overlay


def expect_failure(action: Any, needle: str, label: str) -> None:
    try:
        action()
    except ContractError as error:
        require(needle in str(error), f"{label} failed for the wrong reason: {error}")
        return
    raise ContractError(f"negative control did not fail: {label}")


def self_test() -> dict[str, Any]:
    validate_policy(load_json(DEFAULT_POLICY, "checked-in policy"))
    with tempfile.TemporaryDirectory(
        prefix="c30-day5-reduction-selftest-"
    ) as directory:
        root = Path(directory)
        policy_path, evidence, overlay = build_internal_fixture(root)
        audited = audit_source(policy_path, evidence)
        require(
            audited["policySha256"] == digest(policy_path.read_bytes()),
            "audit receipt policy SHA-256 drift",
        )
        require(
            audited["sourceEvidenceRunId"] == "synthetic-internal",
            "audit receipt source run ID drift",
        )
        incomplete = verify_overlay_phase1(policy_path, evidence, overlay)
        require(incomplete["verdict"] == "INCOMPLETE", "phase-1 overlay false-green")
        require(
            incomplete["acceptanceValid"] is False, "phase-1 acceptance false-green"
        )
        require(
            incomplete["policySha256"] == audited["policySha256"]
            and incomplete["sourceEvidenceRunId"] == audited["sourceEvidenceRunId"],
            "overlay receipt authority drift",
        )

        success_record = next(
            record for record in audited["records"] if record["outcome"] == "success"
        )
        success_mutant = copy.deepcopy(success_record)
        success_mutant["returnCode"] = 1
        expect_failure(
            lambda: audit_record_shape(success_mutant, 0),
            "success must be non-timeout with zero returnCode",
            "source success-semantics control",
        )

        failure_record = next(
            record for record in audited["records"] if record["outcome"] == "failure"
        )
        failure_mutant = copy.deepcopy(failure_record)
        failure_mutant["timedOut"] = True
        expect_failure(
            lambda: audit_record_shape(failure_mutant, 0),
            "reducible failure must be non-timeout with positive returnCode",
            "source failure-semantics control",
        )

        phase_mutant = copy.deepcopy(failure_record)
        phase_mutant["phase"] = (
            "oracle-preprocessed-parse"
            if failure_record["lane"] == "evocpp-raw"
            else "raw-source-preprocess"
        )
        expect_failure(
            lambda: audit_record_shape(phase_mutant, 0),
            "lane/phase authority drift",
            "source lane-phase control",
        )

        overlay_manifest_path = overlay / "overlay.json"
        overlay_manifest = load_json(overlay_manifest_path)
        extra_overlay = copy.deepcopy(overlay_manifest)
        extra_overlay["unexpected"] = True
        store(overlay_manifest_path, extra_overlay)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "overlay manifest keys differ",
            "overlay exact-schema control",
        )
        store(overlay_manifest_path, overlay_manifest)

        state_overlay = copy.deepcopy(overlay_manifest)
        state_overlay["records"][0]["state"] = "VERIFIED"
        store(overlay_manifest_path, state_overlay)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "overlay state is not CANDIDATE",
            "overlay forged-state control",
        )
        store(overlay_manifest_path, overlay_manifest)

        parser_index = next(
            index
            for index, value in enumerate(overlay_manifest["records"])
            if value["candidate"]["kind"] == "preprocessed-parser"
        )
        lineage_overlay = copy.deepcopy(overlay_manifest)
        lineage_overlay["records"][parser_index]["candidate"]["lineage"][0][
            "seedLineSha256"
        ] = "0" * 64
        store(overlay_manifest_path, lineage_overlay)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "seed-line SHA-256 drift",
            "overlay lineage control",
        )
        store(overlay_manifest_path, overlay_manifest)

        raw_index = next(
            index
            for index, value in enumerate(overlay_manifest["records"])
            if value["candidate"]["kind"] == "raw-closure"
        )
        gcc_fanout_overlay = copy.deepcopy(overlay_manifest)
        gcc_fanout_record = gcc_fanout_overlay["records"][raw_index]
        gcc_fanout_candidate = gcc_fanout_record["candidate"]
        gcc_fanout_subject = gcc_fanout_candidate["subjectClosure"]
        removed_output = next(
            value
            for value in gcc_fanout_subject["files"]
            if value["logicalPath"].startswith("output/")
        )
        gcc_fanout_subject["files"] = [
            value
            for value in gcc_fanout_subject["files"]
            if value is not removed_output
        ]
        gcc_fanout_subject["closureSha256"] = digest(
            b"".join(
                (
                    f"{value['logicalPath']}\t{value['sha256']}\t{value['bytes']}\n"
                ).encode("ascii")
                for value in gcc_fanout_subject["files"]
            )
        )
        gcc_fanout_candidate["bundleSha256"] = digest(
            canonical(
                {
                    "referenceClosures": {
                        family: {
                            "closureSha256": closure["closureSha256"],
                            "root": closure["root"],
                            "seedFamily": closure["seedFamily"],
                        }
                        for family, closure in gcc_fanout_candidate[
                            "referenceClosures"
                        ].items()
                    },
                    "rootSourcePath": gcc_fanout_candidate["rootSourcePath"],
                    "subjectClosure": {
                        "closureSha256": gcc_fanout_subject["closureSha256"],
                        "root": gcc_fanout_subject["root"],
                        "seedFamily": gcc_fanout_subject["seedFamily"],
                    },
                }
            )
        )
        removed_output_path = (
            overlay
            / "candidates"
            / gcc_fanout_record["failureId"]
            / "subject"
            / removed_output["logicalPath"]
        )
        removed_output_bytes = removed_output_path.read_bytes()
        removed_output_path.unlink()
        store(overlay_manifest_path, gcc_fanout_overlay)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "raw subject/reference-gcc logical file sets differ",
            "raw subject/reference-gcc full-closure fanout control",
        )
        removed_output_path.write_bytes(removed_output_bytes)
        store(overlay_manifest_path, overlay_manifest)

        bundle_overlay = copy.deepcopy(overlay_manifest)
        bundle_overlay["records"][raw_index]["candidate"]["bundleSha256"] = "0" * 64
        store(overlay_manifest_path, bundle_overlay)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "raw bundle SHA-256 drift",
            "overlay raw-bundle control",
        )
        store(overlay_manifest_path, overlay_manifest)

        parser_candidate_path = (
            overlay / overlay_manifest["records"][parser_index]["candidate"]["path"]
        )
        parser_candidate_bytes = parser_candidate_path.read_bytes()
        parser_candidate_path.write_bytes(parser_candidate_bytes + b"x")
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "parser candidate byte count drift",
            "overlay candidate-digest control",
        )
        parser_candidate_path.write_bytes(parser_candidate_bytes)

        symlink_path = overlay / "forbidden-link"
        symlink_path.symlink_to("synthetic-candidate.c")
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "overlay symbolic link prohibited",
            "overlay symlink control",
        )
        symlink_path.unlink()

        hardlink_path = overlay / "forbidden-hardlink"
        os.link(parser_candidate_path, hardlink_path)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "overlay hard-link isolation violation",
            "overlay hard-link control",
        )
        hardlink_path.unlink()

        parser_candidate_path.unlink()
        os.link(evidence / "parser/synthetic.gcc.c", parser_candidate_path)
        expect_failure(
            lambda: verify_overlay_phase1(policy_path, evidence, overlay),
            "overlay hard-link isolation violation",
            "overlay seed-hard-link control",
        )
        parser_candidate_path.unlink()
        parser_candidate_path.write_bytes(parser_candidate_bytes)

        policy_extra = load_json(policy_path)
        policy_extra["unexpected"] = True
        extra_path = root / "policy-extra.json"
        store(extra_path, policy_extra)
        expect_failure(
            lambda: validate_policy(load_json(extra_path)),
            "policy keys differ",
            "policy extra-key control",
        )

        run_id_policy = load_json(policy_path)
        run_id_policy["sourceEvidence"]["runId"] = "forged-run-id"
        run_id_policy_path = root / "policy-run-id.json"
        store(run_id_policy_path, run_id_policy)
        expect_failure(
            lambda: audit_source(run_id_policy_path, evidence),
            "runId does not match relativeRoot basename",
            "source run ID binding control",
        )

        authority_policy = load_json(policy_path)
        authority_policy["sourceEvidence"]["executionAuthority"]["referenceCompilers"][
            "gcc"
        ]["sha256"] = "0" * 64
        authority_policy_path = root / "policy-execution-authority.json"
        store(authority_policy_path, authority_policy)
        expect_failure(
            lambda: audit_source(authority_policy_path, evidence),
            "GCC reference compiler authority drift",
            "execution-authority cross-link control",
        )

        run_config_path = evidence / "run-config.json"
        original_run_config = run_config_path.read_bytes()
        run_config_mutant = load_json(run_config_path)
        run_config_mutant["requiredObservationLanes"] = ["evocpp-raw"]
        store(run_config_path, run_config_mutant)
        run_config_policy = load_json(policy_path)
        run_config_policy["sourceEvidence"]["authorityArtifacts"]["runConfig"] = (
            descriptor(run_config_path, "run-config.json")
        )
        run_config_policy_path = root / "policy-run-config.json"
        store(run_config_policy_path, run_config_policy)
        expect_failure(
            lambda: audit_source(run_config_policy_path, evidence),
            "run-config requiredObservationLanes drift",
            "run-config lane authority control",
        )
        run_config_path.write_bytes(original_run_config)

        results = evidence / "results.jsonl"
        original_results = results.read_bytes()
        results.write_bytes(original_results + b"\n")
        expect_failure(
            lambda: audit_source(policy_path, evidence),
            "results byte count drift",
            "authority tamper control",
        )
        results.write_bytes(original_results)

        subject = evidence / "cargo-target/debug/evocc"
        original_subject = subject.read_bytes()
        subject.write_bytes(original_subject.replace(b"exit 1", b"exit 0"))
        expect_failure(
            lambda: audit_source(policy_path, evidence),
            "verified subject SHA-256 drift",
            "subject binary control",
        )
        subject.write_bytes(original_subject)

        parser_input = evidence / "parser/synthetic.gcc.c"
        original_parser_input = parser_input.read_bytes()
        parser_input.write_bytes(
            original_parser_input.replace(b"int synthetic", b"ont synthetic")
        )
        expect_failure(
            lambda: audit_source(policy_path, evidence),
            "parserInput SHA-256 drift",
            "record artifact control",
        )
        parser_input.write_bytes(original_parser_input)

        per_tu = evidence / "inputs/synthetic.gcc.inputs.tsv"
        original_per_tu = per_tu.read_bytes()
        per_tu.write_bytes(original_per_tu + b"x")
        expect_failure(
            lambda: audit_source(policy_path, evidence),
            "byte count drift",
            "per-TU closure control",
        )
        per_tu.write_bytes(original_per_tu)

        equality = evidence / "kernel-input-closure.end.tsv"
        original_equality = equality.read_bytes()
        equality.write_bytes(
            original_equality + b"drift\t" + digest(b"drift").encode("ascii") + b"\t5\n"
        )
        equality_policy = load_json(policy_path)
        equality_policy["sourceEvidence"]["authorityArtifacts"][
            "kernelInputClosureEnd"
        ] = descriptor(equality, "kernel-input-closure.end.tsv")
        equality_policy_path = root / "policy-equality.json"
        store(equality_policy_path, equality_policy)
        expect_failure(
            lambda: audit_source(equality_policy_path, evidence),
            "authority equality drift",
            "closure equality control",
        )
        equality.write_bytes(original_equality)

        manifest_policy = load_json(policy_path)
        manifest_policy["obligationDerivation"]["obligationManifestSha256"] = "0" * 64
        manifest_path = root / "policy-manifest.json"
        store(manifest_path, manifest_policy)
        expect_failure(
            lambda: audit_source(manifest_path, evidence),
            "source obligation manifest drift",
            "manifest control",
        )
        return {
            "controls": 22,
            "manifestSha256": audited["manifestSha256"],
            "sourceFailures": len(audited["failures"]),
        }


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    value.add_argument(
        "--self-test",
        action="store_true",
        help="run internal positive and negative controls",
    )
    subparsers = value.add_subparsers(dest="command")
    audit = subparsers.add_parser(
        "audit-source", help="audit immutable source evidence"
    )
    audit.add_argument("--policy", type=Path, required=True)
    audit.add_argument("--evidence", type=Path, required=True)
    verify = subparsers.add_parser(
        "verify-overlay", help="return the fail-closed Phase-1 overlay verdict"
    )
    verify.add_argument("--policy", type=Path, required=True)
    verify.add_argument("--evidence", type=Path, required=True)
    verify.add_argument("--overlay", type=Path, required=True)
    pilots = subparsers.add_parser(
        "verify-pilots", help="execute the exact locked Phase-2 pilot contract"
    )
    pilots.add_argument("--pilot-contract", type=Path, required=True)
    pilots.add_argument("--policy", type=Path, required=True)
    pilots.add_argument("--evidence", type=Path, required=True)
    pilots.add_argument("--overlay", type=Path, required=True)
    pilots.add_argument("--receipt-dir", type=Path, required=True)
    pilots.add_argument("--observer-ready-file", type=Path)
    pilots.add_argument("--observer-ack-file", type=Path)
    replay = subparsers.add_parser(
        "replay-pilot-receipt", help="independently audit an immutable pilot receipt"
    )
    replay.add_argument("--pilot-contract", type=Path, required=True)
    replay.add_argument("--policy", type=Path, required=True)
    replay.add_argument("--evidence", type=Path, required=True)
    replay.add_argument("--overlay", type=Path, required=True)
    replay.add_argument("--receipt-dir", type=Path, required=True)
    replay.add_argument("--expected-receipt-sha256", required=True)
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        require(
            args.self_test ^ (args.command is not None),
            "choose exactly one command or --self-test",
        )
        if args.self_test:
            result = self_test()
            print(
                "DAY5_KERNEL_REDUCTIONS_SELFTEST "
                f"pass=1 fail=0 controls={result['controls']} source_failures={result['sourceFailures']} "
                f"obligation_manifest_sha256={result['manifestSha256']} phase1_verify=INCOMPLETE"
            )
            return 0
        if args.command == "audit-source":
            result = audit_source(args.policy, args.evidence)
            print(
                "DAY5_KERNEL_REDUCTIONS_AUDIT "
                f"pass=1 fail=0 source_failures={len(result['failures'])} "
                f"obligation_manifest_sha256={result['manifestSha256']} "
                f"policy_sha256={result['policySha256']} "
                f"source_run_id={result['sourceEvidenceRunId']}"
            )
            return 0
        if args.command == "verify-overlay":
            result = verify_overlay_phase1(args.policy, args.evidence, args.overlay)
            print(
                json.dumps(
                    result, ensure_ascii=False, sort_keys=True, separators=(",", ":")
                )
            )
            return 0
        if args.command == "verify-pilots":
            observer_ready = args.observer_ready_file
            observer_ack = args.observer_ack_file
            if observer_ready is None and os.environ.get("C30_DAY5_OBSERVER_READY"):
                observer_ready = Path(os.environ["C30_DAY5_OBSERVER_READY"])
            if observer_ack is None and os.environ.get("C30_DAY5_OBSERVER_ACK"):
                observer_ack = Path(os.environ["C30_DAY5_OBSERVER_ACK"])
            result = verify_pilots_phase2(
                args.policy,
                args.pilot_contract,
                args.evidence,
                args.overlay,
                args.receipt_dir,
                observer_ready,
                observer_ack,
            )
            print(
                json.dumps(
                    result, ensure_ascii=False, sort_keys=True, separators=(",", ":")
                )
            )
            return 0
        if args.command == "replay-pilot-receipt":
            result = replay_pilot_receipt_phase2(
                args.policy,
                args.pilot_contract,
                args.evidence,
                args.overlay,
                args.receipt_dir,
                args.expected_receipt_sha256,
            )
            print(
                json.dumps(
                    result, ensure_ascii=False, sort_keys=True, separators=(",", ":")
                )
            )
            return 0
        raise ContractError("unknown command")
    except (ContractError, OSError, ValueError) as error:
        print(f"{FAILURE_PREFIX} {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
