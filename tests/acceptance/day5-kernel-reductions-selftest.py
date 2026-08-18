#!/usr/bin/env python3
"""RED-first contract test for the Day-5 kernel reduction verifier.

The first run is deliberately RED only while the production verifier is
absent.  The checked-in 46-way authority is audited before that RED result.
After the verifier exists, its own ``--self-test`` and ``audit-source`` are run
against a tiny synthetic evidence tree rather than the real kernel inputs.
"""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
POLICY = ROOT / "tests/contracts/day5-kernel-reduction-policy.json"
VERIFIER = ROOT / "tests/acceptance/day5-kernel-reductions.py"
RUN_ID = "20260817T225959Z-57627"
MANIFEST_SHA256 = "39af1ba905884d739789c996ede7b24ba08c07d3be39b77a643363334d9ec2a1"


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


def load(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(value, dict), f"JSON root is not an object: {path}")
    return value


def store(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value))


def descriptor(path: Path, logical: str | None = None) -> dict[str, Any]:
    data = path.read_bytes()
    return {"path": logical or path.name, "bytes": len(data), "sha256": digest(data)}


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


def obligations(records: list[dict[str, Any]]) -> list[dict[str, str]]:
    values = [
        obligation(record) for record in records if record.get("outcome") == "failure"
    ]
    return sorted(values, key=lambda value: value["failureId"])


def manifest(values: list[dict[str, str]]) -> str:
    return digest(b"".join(canonical(value) for value in values))


def audit_checked_in_authority() -> dict[str, Any]:
    policy = load(POLICY)
    source = policy["sourceEvidence"]
    require(policy["contractState"] == "locked", "reduction policy is not locked")
    require(source["runId"] == RUN_ID, "source run drift")
    require(
        (
            source["expectedObservations"],
            source["expectedFailures"],
            source["expectedSuccesses"],
        )
        == (48, 46, 2),
        "frozen source denominator drift",
    )
    evidence = ROOT / source["relativeRoot"]
    for label, expected in source["authorityArtifacts"].items():
        path = evidence / expected["path"]
        require(path.is_file() and not path.is_symlink(), f"missing authority: {label}")
        actual = descriptor(path, expected["path"])
        require(actual == expected, f"authority drift: {label}")
    records = [
        json.loads(line)
        for line in (evidence / "results.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    ]
    derived = obligations(records)
    require(len(records) == 48 and len(derived) == 46, "derived denominator drift")
    require(len({value["failureId"] for value in derived}) == 46, "failure IDs collide")
    require(
        derived == policy["obligationDerivation"]["obligations"],
        "46-way bijection drift",
    )
    require(manifest(derived) == MANIFEST_SHA256, "obligation manifest drift")
    require(
        policy["obligationDerivation"]["obligationManifestSha256"] == MANIFEST_SHA256,
        "policy manifest authority drift",
    )
    verdict = policy["verdictPolicy"]
    require(
        verdict["phase1VerifyOverlayVerdict"] == "INCOMPLETE", "phase-1 verdict drift"
    )
    require(verdict["phase1AcceptanceValid"] is False, "phase-1 false-green")
    require(
        verdict["sourceSignatureAloneIsOracle"] is False,
        "source signature promoted to oracle",
    )
    anchor = policy["replayPolicy"]["causalAnchor"]
    require(
        anchor["overlayMaySupplyAnchor"] is False, "overlay may forge causal anchor"
    )
    require(
        "emitted line/column" in anchor["parser"],
        "parser emitted coordinate is unbound",
    )
    require(
        "seed-closure logical path" in anchor["raw"], "raw seed coordinate is unbound"
    )
    compilers = source["executionAuthority"]["referenceCompilers"]
    require(
        compilers["gcc"]["sha256"] != compilers["clang"]["sha256"],
        "oracle identities alias",
    )
    raw_seed_roots = source["executionAuthority"]["rawSeedRoots"]
    require(
        raw_seed_roots
        == {
            "source": {"base": "repository", "path": ".build/linux-src"},
            "gccOutput": {
                "base": "repository",
                "path": ".build/reference/gcc-lane-1",
            },
            "clangOutput": {
                "base": "repository",
                "path": ".build/reference/clang-lane-1",
            },
        },
        "raw seed roots are not independently pinned",
    )
    require(
        policy["referenceAcceptance"]["requiredCompilers"] == ["gcc", "clang"],
        "two-oracle gate drift",
    )
    raw_commands = policy["referenceAcceptance"]["rawCommands"]
    require("gcc-lane-1" in raw_commands["gcc"], "raw GCC projection authority drift")
    require(
        "clang-lane-1" in raw_commands["clang"], "raw Clang projection authority drift"
    )
    require(
        raw_commands["crossFamilyArgvReuse"] == "PROHIBITED",
        "cross-family argv reuse allowed",
    )
    minimality = policy["replayPolicy"]["minimality"]
    require(
        "both pinned compilers accept" in minimality,
        "minimality ignores oracle acceptance",
    )
    require(
        "subject failure but breaks either oracle is not removable" in minimality,
        "minimality misclassifies oracle-invalid deletions",
    )
    stable_fields = policy["replayPolicy"]["stableReplayExactFields"]
    require(
        "stdoutSha256" in stable_fields and "stderrSha256" in stable_fields,
        "replays are not byte-exact",
    )
    require(
        "argvSha256" in stable_fields and "environmentSha256" in stable_fields,
        "replay authority is unbound",
    )
    raw_candidate = policy["overlayPolicy"]["rawCandidate"]
    require(
        set(raw_candidate["referenceClosures"]) == {"gcc", "clang"},
        "raw family closures are incomplete",
    )
    require(
        "exactly once" in raw_candidate["rootInvariant"],
        "raw root uniqueness is unbound",
    )
    require(
        "candidates/F/subject/<logicalPath>" in raw_candidate["storageLayout"]
        and "candidates/F/reference-gcc/<logicalPath>" in raw_candidate["storageLayout"]
        and "candidates/F/reference-clang/<logicalPath>"
        in raw_candidate["storageLayout"],
        "raw candidate byte storage is not deterministic",
    )
    integration = policy["integrationPolicy"]
    require(
        integration["reconnaissanceSummaryMayTrustStateString"] is False,
        "summary may trust forged state",
    )
    require(
        policy["overlayPolicy"]["hardLinks"] == "PROHIBITED",
        "overlay hard links are allowed",
    )
    return policy


def synthetic_record(index: int) -> dict[str, Any]:
    lane = ("evocpp-raw", "gcc-preprocessed-parser", "clang-preprocessed-parser")[
        index // 16
    ]
    tu_index = index % 16
    raw = lane == "evocpp-raw"
    success = not raw and tu_index == 15
    if success:
        return {
            "artifacts": {},
            "classification": None,
            "diagnostic": None,
            "lane": lane,
            "outcome": "success",
            "phase": "oracle-preprocessed-parse",
            "returnCode": 0,
            "sourcePath": f"src/tu-{tu_index:02d}.c",
            "timedOut": False,
            "tuId": f"tu-{tu_index:02d}",
        }
    return {
        "artifacts": {},
        "classification": "preprocess" if raw else "parse",
        "diagnostic": {
            "code": "E0104" if raw else ("E1001" if index % 2 else "E1002"),
            "emittedPath": (
                f"/synthetic/source/include/origin-{tu_index % 3}.h"
                if raw
                else f"/synthetic/parser/tu-{tu_index:02d}.{lane.split('-')[0]}.c"
            ),
            "emittedSpan": {"line": 2, "column": 1},
            "message": "synthetic-stable-failure",
            "sourcePath": f"include/origin-{tu_index % 3}.h",
            "sourceSpan": {"line": 10 + tu_index, "column": 1},
        },
        "lane": lane,
        "outcome": "failure",
        "phase": "raw-source-preprocess" if raw else "oracle-preprocessed-parse",
        "returnCode": 1,
        "sourcePath": f"src/tu-{tu_index:02d}.c",
        "timedOut": False,
        "tuId": f"tu-{tu_index:02d}",
    }


def write_synthetic_record_artifacts(
    evidence: Path, record: dict[str, Any], index: int
) -> None:
    tu_id = record["tuId"]
    if record["lane"] == "evocpp-raw":
        artifact_root = evidence / "evocpp-raw"
        artifact_root.mkdir(exist_ok=True)
        files = {
            "argv": (
                artifact_root / f"{tu_id}.argv.json",
                canonical(
                    [
                        "evocc",
                        "-E",
                        "--target=aarch64-unknown-linux-gnu",
                        record["sourcePath"],
                    ]
                ),
            ),
            "stdout": (artifact_root / f"{tu_id}.stdout", b""),
            "stderr": (
                artifact_root / f"{tu_id}.stderr",
                (
                    f"{record['diagnostic']['emittedPath']}:2:1: "
                    f"error[{record['diagnostic']['code']}]: synthetic-stable-failure\n"
                ).encode("utf-8"),
            ),
        }
    else:
        family = record["lane"].removesuffix("-preprocessed-parser")
        parser_root = evidence / "parser"
        oracle_root = evidence / f"oracle-{family}"
        parser_root.mkdir(exist_ok=True)
        oracle_root.mkdir(exist_ok=True)
        stem = f"{tu_id}.{family}"
        source = (
            f"/* synthetic {index} */\nint good_{index};\n"
            if record["outcome"] == "success"
            else f"/* synthetic {index} */\nint broken_{index}(\n"
        ).encode("utf-8")
        mapped_origin_path = (
            record["sourcePath"]
            if record["outcome"] == "success"
            else record["diagnostic"]["sourcePath"]
        )
        mapped_origin_line = (
            10 + (index % 16)
            if record["outcome"] == "success"
            else record["diagnostic"]["sourceSpan"]["line"]
        )
        line_map = canonical(
            {
                "originLine": 9 + (index % 16),
                "originPath": record["sourcePath"],
                "parserLine": 1,
            }
        ) + canonical(
            {
                "originLine": mapped_origin_line,
                "originPath": mapped_origin_path,
                "parserLine": 2,
            }
        )
        stderr = b""
        if record["outcome"] == "failure":
            stderr = (
                f"{record['diagnostic']['emittedPath']}:2:1: "
                f"error[{record['diagnostic']['code']}]: synthetic-stable-failure\n"
            ).encode("utf-8")
        files = {
            "parserInput": (parser_root / f"{stem}.c", source),
            "lineMap": (parser_root / f"{stem}.linemap.jsonl", line_map),
            "stdout": (parser_root / f"{stem}.stdout", b""),
            "stderr": (parser_root / f"{stem}.stderr", stderr),
            "compilerStdout": (oracle_root / f"{stem}.compiler.stdout", b""),
            "compilerStderr": (oracle_root / f"{stem}.compiler.stderr", b""),
            "oraclePreprocessed": (oracle_root / f"{stem}.oracle.i", source),
        }
    for name, (path, data) in files.items():
        path.write_bytes(data)
        record["artifacts"][name] = descriptor(path, path.name)


def write_synthetic_input_closures(evidence: Path) -> list[dict[str, Any]]:
    inputs = evidence / "inputs"
    inputs_end = evidence / "inputs-end"
    seed_source = evidence / "seed/source"
    seed_gcc = evidence / "seed/gcc-output"
    seed_clang = evidence / "seed/clang-output"
    inputs.mkdir()
    inputs_end.mkdir()
    seed_source.mkdir(parents=True)
    seed_gcc.mkdir(parents=True)
    seed_clang.mkdir(parents=True)
    units: list[dict[str, Any]] = []
    for tu_index in range(16):
        source = f"int synthetic_tu_{tu_index};\n".encode("ascii")
        source_relative = Path(f"src/tu-{tu_index:02d}.c")
        source_path = seed_source / source_relative
        source_path.parent.mkdir(parents=True, exist_ok=True)
        source_path.write_bytes(source)
        origin_index = tu_index % 3
        origin = (
            f"/* synthetic origin {origin_index} */\n#error synthetic-stable-failure\n"
        ).encode("ascii")
        origin_relative = Path(f"include/origin-{origin_index}.h")
        origin_path = seed_source / origin_relative
        origin_path.parent.mkdir(parents=True, exist_ok=True)
        if origin_path.exists():
            require(origin_path.read_bytes() == origin, "synthetic origin seed drift")
        else:
            origin_path.write_bytes(origin)
        commands: dict[str, Any] = {}
        for family in ("clang", "gcc"):
            output = f"#define SYNTHETIC_{family.upper()}_{tu_index} 1\n".encode(
                "ascii"
            )
            output_relative = Path(f"generated/tu-{tu_index:02d}.h")
            output_root = seed_clang if family == "clang" else seed_gcc
            output_path = output_root / output_relative
            output_path.parent.mkdir(parents=True, exist_ok=True)
            output_path.write_bytes(output)
            rows = [
                f"output/{output_relative.as_posix()}\t{digest(output)}\t{len(output)}\n",
                f"source/{origin_relative.as_posix()}\t{digest(origin)}\t{len(origin)}\n",
                f"source/{source_relative.as_posix()}\t{digest(source)}\t{len(source)}\n",
            ]
            closure = "".join(sorted(rows)).encode("ascii")
            name = f"tu-{tu_index:02d}.{family}.inputs.tsv"
            (inputs / name).write_bytes(closure)
            (inputs_end / name).write_bytes(closure)
            commands[family] = {
                "inputClosure": {
                    "files": 3,
                    "bytes": len(source) + len(origin) + len(output),
                    "sha256": digest(closure),
                }
            }
        units.append(
            {
                "id": f"tu-{tu_index:02d}",
                "sourcePath": f"src/tu-{tu_index:02d}.c",
                "sourceSha256": digest(source),
                "sourceBytes": len(source),
                "commands": commands,
            }
        )

    def closure_manifest(directory: Path) -> bytes:
        rows = []
        for path in sorted(directory.iterdir(), key=lambda item: item.name):
            data = path.read_bytes()
            rows.append(f"{path.name}\t{digest(data)}\t{len(data)}\n")
        return "".join(rows).encode("ascii")

    start = closure_manifest(inputs)
    end = closure_manifest(inputs_end)
    require(start == end, "synthetic input closure endpoints differ")
    (evidence / "kernel-input-closure.start.tsv").write_bytes(start)
    (evidence / "kernel-input-closure.end.tsv").write_bytes(end)
    return units


def build_small_source(root: Path, checked_policy: dict[str, Any]) -> tuple[Path, Path]:
    evidence = root / "synthetic-46"
    evidence.mkdir(parents=True)
    records = [synthetic_record(index) for index in range(48)]
    for index, record in enumerate(records):
        write_synthetic_record_artifacts(evidence, record, index)
    (evidence / "results.jsonl").write_bytes(
        b"".join(canonical(record) for record in records)
    )
    units = write_synthetic_input_closures(evidence)
    execution = checked_policy["sourceEvidence"]["executionAuthority"]
    reference_compilers = execution["referenceCompilers"]
    build_environment = {
        "schemaVersion": 1,
        "contractState": "locked",
        "buildImage": {
            "sealedImageId": execution["sealedImageId"],
            "observedImageId": execution["sealedImageId"],
            "executionReference": execution["sealedImageId"],
        },
        "containerRuntime": {"cliSha256": execution["containerCliSha256"]},
        "reproducibility": {"SOURCE_DATE_EPOCH": "1"},
        "toolchain": {
            "gcc": {
                "binaryPathInImage": reference_compilers["gcc"]["path"],
                "resolvedVersion": reference_compilers["gcc"]["version"],
                "binarySha256": reference_compilers["gcc"]["sha256"],
            },
            "clang": {
                "binaryPathInImage": reference_compilers["clang"]["path"],
                "resolvedVersion": reference_compilers["clang"]["version"],
                "binarySha256": reference_compilers["clang"]["sha256"],
            },
            "rust": {"rustcSha256": execution["rustcSha256"]},
            "hostReplay": {
                "cargo": {"sha256": execution["cargoSha256"]},
                "rustc": {"sha256": execution["rustcSha256"]},
            },
        },
    }
    host_build_authority = {
        "schemaVersion": 1,
        "tools": {
            "cargo": {"sha256": execution["cargoSha256"]},
            "rustc": {"sha256": execution["rustcSha256"]},
            "container": {"sha256": execution["containerCliSha256"]},
        },
    }
    placeholders = {
        "kernel-cohort.json": {
            "schemaVersion": 1,
            "contractState": "locked",
            "translationUnits": units,
        },
        "build-environment.json": build_environment,
        "host-build-authority.json": host_build_authority,
    }
    for name, value in placeholders.items():
        store(evidence / name, value)
    host_build_authority["buildEnvironmentContract"] = descriptor(
        evidence / "build-environment.json", "build-environment.json"
    )
    store(evidence / "host-build-authority.json", host_build_authority)
    (evidence / "runner.py").write_text("# synthetic\n", encoding="utf-8")
    store(
        evidence / "run-config.json",
        {
            "schemaVersion": 1,
            "generalTimeoutSeconds": 1,
            "parserTimeoutSecondsPerTu": 1,
            "parserJobs": 1,
            "requiredObservationLanes": [
                "evocpp-raw",
                "gcc-preprocessed-parser",
                "clang-preprocessed-parser",
            ],
            "kernelCohortContract": descriptor(
                evidence / "kernel-cohort.json", "kernel-cohort.json"
            ),
            "buildEnvironmentContract": descriptor(
                evidence / "build-environment.json", "build-environment.json"
            ),
            "runner": descriptor(evidence / "runner.py", "runner.py"),
        },
    )
    for name in ("evocc-build-inputs.tsv", "evocc-build-inputs.end.tsv"):
        (evidence / name).write_text(
            "synthetic\t" + digest(b"synthetic") + "\t9\n", encoding="ascii"
        )
    cohort_sha = digest((evidence / "kernel-cohort.json").read_bytes())
    subject_bytes = b"#!/bin/sh\nexit 1\n"
    subject_path = (
        evidence
        / checked_policy["sourceEvidence"]["executionAuthority"][
            "subjectEvidenceRelativePath"
        ]
    )
    subject_path.parent.mkdir(parents=True)
    subject_path.write_bytes(subject_bytes)
    subject_path.chmod(0o700)
    subject_sha = digest(subject_bytes)
    (evidence / "contract.sha256").write_text(cohort_sha + "\n", encoding="ascii")
    (evidence / "evocc.sha256").write_text(subject_sha + "\n", encoding="ascii")
    store(
        evidence / "summary.json",
        {
            "schemaVersion": 1,
            "requiredRecords": 48,
            "observedRecords": 48,
            "successes": 2,
            "failures": 46,
            "pendingReductions": 46,
            "contractSha256": cohort_sha,
            "resultsJsonl": descriptor(evidence / "results.jsonl"),
        },
    )
    names = {
        "summary": "summary.json",
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
    policy = copy.deepcopy(checked_policy)
    source = policy["sourceEvidence"]
    source.update({"runId": "synthetic-46", "relativeRoot": "synthetic-46"})
    source["authorityArtifacts"] = {
        label: descriptor(evidence / name, name) for label, name in names.items()
    }
    source["executionAuthority"]["subjectSha256"] = subject_sha
    source["executionAuthority"]["rawSeedRoots"] = {
        "source": {"base": "evidence", "path": "seed/source"},
        "gccOutput": {"base": "evidence", "path": "seed/gcc-output"},
        "clangOutput": {"base": "evidence", "path": "seed/clang-output"},
    }
    derived = obligations(records)
    policy["obligationDerivation"]["obligations"] = derived
    policy["obligationDerivation"]["obligationManifestSha256"] = manifest(derived)
    policy_path = root / "policy.json"
    store(policy_path, policy)
    return policy_path, evidence


def physical_lines(data: bytes) -> list[bytes]:
    return data.splitlines(keepends=True)


def full_lineage(data: bytes) -> list[dict[str, Any]]:
    return [
        {
            "candidateLine": index,
            "seedLine": index,
            "seedLineSha256": digest(line),
        }
        for index, line in enumerate(physical_lines(data), start=1)
    ]


def parse_synthetic_closure(path: Path) -> list[tuple[str, str, int]]:
    rows: list[tuple[str, str, int]] = []
    for line in path.read_text(encoding="ascii").splitlines():
        logical, sha256, byte_count = line.split("\t")
        rows.append((logical, sha256, int(byte_count)))
    require(bool(rows), f"synthetic closure is empty: {path}")
    return rows


def build_valid_overlay(root: Path, policy_path: Path, evidence: Path) -> Path:
    policy = load(policy_path)
    records = [
        json.loads(line)
        for line in (evidence / "results.jsonl")
        .read_text(encoding="utf-8")
        .splitlines()
    ]
    overlay = root / "valid-overlay"
    overlay.mkdir()
    candidates: list[dict[str, Any]] = []
    seed_roots = policy["sourceEvidence"]["executionAuthority"]["rawSeedRoots"]
    resolved_seed_roots = {
        name: evidence / value["path"] for name, value in seed_roots.items()
    }
    for record in records:
        if record["outcome"] != "failure":
            continue
        failure_id = obligation(record)["failureId"]
        if record["lane"] != "evocpp-raw":
            parser_seed = (
                evidence / "parser" / record["artifacts"]["parserInput"]["path"]
            )
            data = parser_seed.read_bytes()
            relative = Path(f"candidates/{failure_id}/parser.c")
            candidate_path = overlay / relative
            candidate_path.parent.mkdir(parents=True)
            candidate_path.write_bytes(data)
            candidate = {
                "bytes": len(data),
                "kind": "preprocessed-parser",
                "lineage": full_lineage(data),
                "path": relative.as_posix(),
                "sha256": digest(data),
            }
        else:
            tu_id = record["tuId"]
            root_logical = f"source/{record['sourcePath']}"

            def make_closure(flavor: str, family: str) -> dict[str, Any]:
                rows = parse_synthetic_closure(
                    evidence / "inputs" / f"{tu_id}.{family}.inputs.tsv"
                )
                files: list[dict[str, Any]] = []
                closure_rows: list[bytes] = []
                for logical, expected_sha, expected_bytes in rows:
                    prefix, relative_text = logical.split("/", 1)
                    seed_root = (
                        resolved_seed_roots["source"]
                        if prefix == "source"
                        else resolved_seed_roots[
                            "gccOutput" if family == "gcc" else "clangOutput"
                        ]
                    )
                    data = (seed_root / relative_text).read_bytes()
                    require(
                        (digest(data), len(data)) == (expected_sha, expected_bytes),
                        f"synthetic raw seed identity drift: {tu_id}:{family}:{logical}",
                    )
                    candidate_path = (
                        overlay / f"candidates/{failure_id}/{flavor}/{logical}"
                    )
                    candidate_path.parent.mkdir(parents=True, exist_ok=True)
                    candidate_path.write_bytes(data)
                    files.append(
                        {
                            "bytes": len(data),
                            "lineage": full_lineage(data),
                            "logicalPath": logical,
                            "sha256": digest(data),
                        }
                    )
                    closure_rows.append(
                        f"{logical}\t{digest(data)}\t{len(data)}\n".encode("ascii")
                    )
                return {
                    "closureSha256": digest(b"".join(closure_rows)),
                    "files": files,
                    "root": root_logical,
                    "seedFamily": family,
                }

            subject = make_closure("subject", "gcc")
            reference_gcc = make_closure("reference-gcc", "gcc")
            reference_clang = make_closure("reference-clang", "clang")
            bundle = {
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
                "rootSourcePath": record["sourcePath"],
                "subjectClosure": {
                    "closureSha256": subject["closureSha256"],
                    "root": subject["root"],
                    "seedFamily": subject["seedFamily"],
                },
            }
            candidate = {
                "bundleSha256": digest(canonical(bundle)),
                "kind": "raw-closure",
                "referenceClosures": {
                    "clang": reference_clang,
                    "gcc": reference_gcc,
                },
                "rootSourcePath": record["sourcePath"],
                "subjectClosure": subject,
            }
        candidates.append(
            {
                "candidate": candidate,
                "failureId": failure_id,
                "state": "CANDIDATE",
            }
        )
    candidates.sort(key=lambda value: value["failureId"])
    require(len(candidates) == 46, "synthetic overlay denominator drift")
    store(
        overlay / "overlay.json",
        {
            "policyId": policy["policyId"],
            "records": candidates,
            "schemaVersion": 1,
            "sourceEvidenceRunId": policy["sourceEvidence"]["runId"],
        },
    )
    return overlay


def run(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, str(VERIFIER), *args],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=30,
        check=False,
    )


def require_rejected(
    result: subprocess.CompletedProcess[str], needle: str, label: str
) -> None:
    require(result.returncode == 2, f"{label} was not rejected: rc={result.returncode}")
    require(
        result.stdout == "",
        f"{label} wrote false-success stdout: {result.stdout.strip()}",
    )
    require(
        result.stderr.startswith("day5-kernel-reductions: FAIL: "),
        f"{label} lacks the contract-failure prefix: {result.stderr.strip()}",
    )
    require(
        needle in result.stderr,
        f"{label} failed for the wrong reason: {result.stderr.strip()}",
    )


def main() -> int:
    try:
        checked_policy = audit_checked_in_authority()
        if not VERIFIER.is_file():
            print(
                f"DAY5_KERNEL_REDUCTIONS_SELFTEST RED production reducer/verifier absent: {VERIFIER}"
            )
            return 1
        internal = run("--self-test")
        require(
            internal.returncode == 0,
            f"production self-test failed: {internal.stderr.strip()}",
        )
        require(
            "DAY5_KERNEL_REDUCTIONS_SELFTEST" in internal.stdout,
            "production self-test marker missing",
        )
        with tempfile.TemporaryDirectory(
            prefix="c30-day5-reduction-source-"
        ) as directory:
            policy, evidence = build_small_source(Path(directory), checked_policy)
            audited = run(
                "audit-source", "--policy", str(policy), "--evidence", str(evidence)
            )
            require(
                audited.returncode == 0,
                f"synthetic 46-source audit failed: {audited.stderr.strip()}",
            )
            require(
                "DAY5_KERNEL_REDUCTIONS_AUDIT" in audited.stdout,
                "audit success marker missing",
            )
            require(
                "VERIFIED" not in audited.stdout,
                "source audit forged a VERIFIED reduction verdict",
            )
            require(
                f"policy_sha256={digest(policy.read_bytes())}" in audited.stdout
                and "source_run_id=synthetic-46" in audited.stdout,
                "source audit receipt omits policy/run identity",
            )

            run_id_mutant = load(policy)
            run_id_mutant["sourceEvidence"]["runId"] = "forged-run-id"
            run_id_mutant_path = Path(directory) / "run-id-mutant-policy.json"
            store(run_id_mutant_path, run_id_mutant)
            require_rejected(
                run(
                    "audit-source",
                    "--policy",
                    str(run_id_mutant_path),
                    "--evidence",
                    str(evidence),
                ),
                "runId",
                "source-run-id mutant",
            )

            authority_mutant = load(policy)
            authority_mutant["sourceEvidence"]["executionAuthority"][
                "referenceCompilers"
            ]["gcc"]["sha256"] = "0" * 64
            authority_mutant_path = Path(directory) / "authority-mutant-policy.json"
            store(authority_mutant_path, authority_mutant)
            require_rejected(
                run(
                    "audit-source",
                    "--policy",
                    str(authority_mutant_path),
                    "--evidence",
                    str(evidence),
                ),
                "GCC",
                "reference-compiler-authority mutant",
            )

            parser_input = evidence / "parser/tu-00.gcc.c"
            parser_bytes = parser_input.read_bytes()
            parser_input.write_bytes(parser_bytes + b"/* authority drift */\n")
            require_rejected(
                run(
                    "audit-source", "--policy", str(policy), "--evidence", str(evidence)
                ),
                "byte count drift",
                "record-artifact mutant",
            )
            parser_input.write_bytes(parser_bytes)

            subject = evidence / "cargo-target/debug/evocc"
            subject_bytes = subject.read_bytes()
            subject.write_bytes(subject_bytes + b"# subject drift\n")
            require_rejected(
                run(
                    "audit-source", "--policy", str(policy), "--evidence", str(evidence)
                ),
                "verified subject SHA-256 drift",
                "subject mutant",
            )
            subject.write_bytes(subject_bytes)

            valid_overlay = build_valid_overlay(Path(directory), policy, evidence)
            incomplete = run(
                "verify-overlay",
                "--policy",
                str(policy),
                "--evidence",
                str(evidence),
                "--overlay",
                str(valid_overlay),
            )
            require(
                incomplete.returncode == 0,
                f"valid 46-overlay structural audit failed: {incomplete.stderr.strip()}",
            )
            phase1 = json.loads(incomplete.stdout)
            require(
                phase1.get("verdict") == "INCOMPLETE"
                and phase1.get("acceptanceValid") is False
                and phase1.get("sourceFailures") == 46
                and phase1.get("structurallyValidCandidates") == 46,
                "valid 46-overlay produced a false or incomplete structural verdict",
            )
            require(
                phase1.get("policySha256") == digest(policy.read_bytes())
                and phase1.get("sourceEvidenceRunId") == "synthetic-46",
                "overlay receipt omits policy/run identity",
            )

            manifest_path = valid_overlay / "overlay.json"
            manifest = load(manifest_path)
            forged_state = copy.deepcopy(manifest)
            forged_state["records"][0]["state"] = "VERIFIED"
            store(manifest_path, forged_state)
            require_rejected(
                run(
                    "verify-overlay",
                    "--policy",
                    str(policy),
                    "--evidence",
                    str(evidence),
                    "--overlay",
                    str(valid_overlay),
                ),
                "state is not CANDIDATE",
                "forged-VERIFIED mutant",
            )
            store(manifest_path, manifest)

            raw_record = next(
                value
                for value in manifest["records"]
                if value["candidate"]["kind"] == "raw-closure"
            )
            wrong_family = copy.deepcopy(manifest)
            wrong_family_record = next(
                value
                for value in wrong_family["records"]
                if value["failureId"] == raw_record["failureId"]
            )
            wrong_family_record["candidate"]["referenceClosures"]["clang"][
                "seedFamily"
            ] = "gcc"
            store(manifest_path, wrong_family)
            require_rejected(
                run(
                    "verify-overlay",
                    "--policy",
                    str(policy),
                    "--evidence",
                    str(evidence),
                    "--overlay",
                    str(valid_overlay),
                ),
                "seedFamily drift",
                "cross-family mutant",
            )
            store(manifest_path, manifest)

            same_family_drift = copy.deepcopy(manifest)
            same_family_record = next(
                value
                for value in same_family_drift["records"]
                if value["failureId"] == raw_record["failureId"]
            )
            same_family_candidate = same_family_record["candidate"]
            gcc_reference = same_family_candidate["referenceClosures"]["gcc"]
            removed_file = next(
                value
                for value in gcc_reference["files"]
                if value["logicalPath"].startswith("output/")
            )
            gcc_reference["files"].remove(removed_file)
            gcc_reference["closureSha256"] = digest(
                b"".join(
                    (
                        f"{value['logicalPath']}\t{value['sha256']}\t"
                        f"{value['bytes']}\n"
                    ).encode("ascii")
                    for value in gcc_reference["files"]
                )
            )
            same_family_bundle = {
                "referenceClosures": {
                    family: {
                        "closureSha256": closure["closureSha256"],
                        "root": closure["root"],
                        "seedFamily": closure["seedFamily"],
                    }
                    for family, closure in same_family_candidate[
                        "referenceClosures"
                    ].items()
                },
                "rootSourcePath": same_family_candidate["rootSourcePath"],
                "subjectClosure": {
                    "closureSha256": same_family_candidate["subjectClosure"][
                        "closureSha256"
                    ],
                    "root": same_family_candidate["subjectClosure"]["root"],
                    "seedFamily": same_family_candidate["subjectClosure"]["seedFamily"],
                },
            }
            same_family_candidate["bundleSha256"] = digest(
                canonical(same_family_bundle)
            )
            removed_path = (
                valid_overlay
                / f"candidates/{raw_record['failureId']}/reference-gcc"
                / removed_file["logicalPath"]
            )
            removed_bytes = removed_path.read_bytes()
            removed_path.unlink()
            store(manifest_path, same_family_drift)
            require_rejected(
                run(
                    "verify-overlay",
                    "--policy",
                    str(policy),
                    "--evidence",
                    str(evidence),
                    "--overlay",
                    str(valid_overlay),
                ),
                "logical file sets differ",
                "same-family closure mutant",
            )
            removed_path.write_bytes(removed_bytes)
            store(manifest_path, manifest)

            parser_record = next(
                value
                for value in manifest["records"]
                if value["candidate"]["kind"] == "preprocessed-parser"
            )
            parser_candidate_path = valid_overlay / parser_record["candidate"]["path"]
            parser_candidate_bytes = parser_candidate_path.read_bytes()
            parser_candidate_path.write_bytes(parser_candidate_bytes + b"/* drift */\n")
            require_rejected(
                run(
                    "verify-overlay",
                    "--policy",
                    str(policy),
                    "--evidence",
                    str(evidence),
                    "--overlay",
                    str(valid_overlay),
                ),
                "byte count drift",
                "candidate-byte mutant",
            )
            parser_candidate_path.write_bytes(parser_candidate_bytes)

            empty_overlay = Path(directory) / "empty-overlay"
            empty_overlay.mkdir()
            store(empty_overlay / "overlay.json", {})
            require_rejected(
                run(
                    "verify-overlay",
                    "--policy",
                    str(policy),
                    "--evidence",
                    str(evidence),
                    "--overlay",
                    str(empty_overlay),
                ),
                "overlay",
                "empty-overlay mutant",
            )
        print(
            "DAY5_KERNEL_REDUCTIONS_SELFTEST pass=12 fail=0 source_failures=46 "
            f"obligation_manifest_sha256={MANIFEST_SHA256} phase1_verify=INCOMPLETE"
        )
        return 0
    except (
        TestFailure,
        OSError,
        ValueError,
        json.JSONDecodeError,
        subprocess.SubprocessError,
    ) as error:
        print(f"day5-kernel-reductions-selftest: FAIL: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
