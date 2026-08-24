#!/usr/bin/env python3
"""External parent, process runner, and receipt sealer for Day-5 pilots."""

from __future__ import annotations

import argparse
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import subprocess
import sys
import tempfile
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
VERIFIER = ROOT / "tests/acceptance/day5-kernel-reductions.py"
FAILURE_PREFIX = "day5-kernel-reductions-observer: FAIL:"


class ObserverError(RuntimeError):
    pass


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ObserverError(message)


def canonical(value: Any) -> bytes:
    return (
        json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")


def digest(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def load_object(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ObserverError(f"cannot load {label}: {error}") from error
    require(isinstance(value, dict), f"{label} is not an object")
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


def safe_relative(value: Any, label: str) -> str:
    require(isinstance(value, str) and bool(value), f"{label} is not text")
    pure = PurePosixPath(value)
    require(
        not pure.is_absolute()
        and pure.as_posix() == value
        and all(part not in {"", ".", ".."} for part in pure.parts),
        f"{label} is not a safe relative path",
    )
    return value


def confined_artifact(root: Path, value: Any, label: str) -> Path:
    descriptor = exact_keys(value, {"bytes", "path", "sha256"}, label)
    relative = safe_relative(descriptor["path"], f"{label}.path")
    path = root / relative
    require(path.is_file() and not path.is_symlink(), f"{label} is absent")
    info = path.stat()
    require(info.st_nlink == 1, f"{label} is hard-linked")
    data = path.read_bytes()
    require(
        descriptor["bytes"] == len(data) and descriptor["sha256"] == digest(data),
        f"{label} descriptor drift",
    )
    return path


def json_artifact(root: Path, value: Any, label: str) -> dict[str, Any]:
    path = confined_artifact(root, value, label)
    parsed = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(parsed, list):
        require(len(parsed) == 1, f"{label} contains multiple objects")
        parsed = parsed[0]
    require(isinstance(parsed, dict), f"{label} is not one object")
    return parsed


def regular_closure(root: Path, *, excluded: set[str] | None = None) -> dict[str, Any]:
    excluded = excluded or set()
    require(root.is_dir() and not root.is_symlink(), f"closure root is unsafe: {root}")
    entries: list[dict[str, Any]] = []
    seen_inodes: set[tuple[int, int]] = set()
    for path in sorted(root.rglob("*"), key=lambda item: item.as_posix()):
        relative = path.relative_to(root).as_posix()
        if relative in excluded:
            continue
        mode = path.lstat().st_mode
        require(not stat.S_ISLNK(mode), f"closure contains symlink: {relative}")
        if stat.S_ISDIR(mode):
            continue
        require(stat.S_ISREG(mode), f"closure contains special entry: {relative}")
        info = path.stat()
        require(info.st_nlink == 1, f"closure contains hard link: {relative}")
        inode = (info.st_dev, info.st_ino)
        require(inode not in seen_inodes, f"closure aliases inode: {relative}")
        seen_inodes.add(inode)
        entries.append(file_descriptor(root, path))
    rows = b"".join(
        f"{item['path']}\t{item['sha256']}\t{item['bytes']}\n".encode("utf-8")
        for item in entries
    )
    return {
        "bytes": sum(item["bytes"] for item in entries),
        "files": entries,
        "sha256": digest(rows),
    }


def directory_closure_sha256(root: Path, label: str) -> str:
    require(root.is_dir() and not root.is_symlink(), f"{label} root is unsafe")
    files = regular_closure(root)["files"]
    require(bool(files), f"{label} closure is empty")
    return digest(
        b"".join(
            f"{item['path']}\t{item['sha256']}\t{item['bytes']}\n".encode("utf-8")
            for item in files
        )
    )


def publish_no_replace(staging: Path, destination: Path) -> None:
    source_bytes = os.fsencode(str(staging.absolute()))
    destination_bytes = os.fsencode(str(destination.absolute()))
    libc = ctypes.CDLL(None, use_errno=True)
    if sys.platform == "darwin":
        primitive = libc.renameatx_np
        primitive.argtypes = [
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_uint,
        ]
        primitive.restype = ctypes.c_int
        result = primitive(-2, source_bytes, -2, destination_bytes, 0x00000004)
    elif sys.platform.startswith("linux"):
        primitive = libc.renameat2
        primitive.argtypes = [
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_int,
            ctypes.c_char_p,
            ctypes.c_uint,
        ]
        primitive.restype = ctypes.c_int
        result = primitive(-100, source_bytes, -100, destination_bytes, 0x1)
    else:
        raise ObserverError("atomic no-replace publication is unsupported")
    if result != 0:
        error_number = ctypes.get_errno()
        if error_number in {errno.EEXIST, errno.ENOTEMPTY}:
            raise ObserverError(f"publication target already exists: {destination}")
        raise ObserverError(
            "atomic no-replace publication failed: "
            + (os.strerror(error_number) if error_number else "unknown error")
        )


def write_new(path: Path, data: bytes, label: str) -> None:
    require(not path.exists(), f"{label} already exists")
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        with path.open("xb") as output:
            output.write(data)
    except OSError as error:
        raise ObserverError(f"cannot write {label}: {error}") from error


def file_descriptor(root: Path, path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    return {
        "bytes": len(data),
        "path": path.relative_to(root).as_posix(),
        "sha256": digest(data),
    }


def observer_identity() -> dict[str, Any]:
    data = Path(__file__).read_bytes()
    return {
        "bytes": len(data),
        "path": Path(__file__).resolve().relative_to(ROOT).as_posix(),
        "sha256": digest(data),
    }


def expected_run_ids(contract: dict[str, Any]) -> tuple[list[str], list[str]]:
    pilots = contract.get("pilots")
    require(isinstance(pilots, list) and len(pilots) == 3, "pilot set drift")
    oracle = ["identity:gcc", "identity:clang"]
    subject: list[str] = []
    observed_ids: list[str] = []
    for pilot in pilots:
        require(isinstance(pilot, dict), "pilot contract entry is not an object")
        failure_id = pilot.get("failureId")
        atom_count = pilot.get("atomCount")
        require(
            isinstance(failure_id, str)
            and len(failure_id) == 64
            and isinstance(atom_count, int)
            and not isinstance(atom_count, bool)
            and atom_count > 0,
            "pilot identity or atom count drift",
        )
        observed_ids.append(failure_id)
        oracle.extend(f"baseline:{failure_id}:{family}" for family in ("gcc", "clang"))
        for index in range(atom_count):
            oracle.extend(
                f"atom:{failure_id}:{index:03d}:{family}"
                for family in ("gcc", "clang")
            )
        subject.extend(
            f"subject:baseline:{failure_id}:{index:03d}" for index in range(3)
        )
        subject.extend(
            f"subject:atom:{failure_id}:{index:03d}"
            for index in range(atom_count)
        )
    require(observed_ids == sorted(observed_ids), "pilot IDs are not sorted")
    require(len(oracle) == 46 and len(subject) == 28, "pilot run denominator drift")
    return oracle, subject


def expected_oracle_input_root(root: Path, run_id: str) -> Path | None:
    parts = run_id.split(":")
    if len(parts) == 2 and parts[0] == "identity":
        return None
    if len(parts) == 3 and parts[0] == "baseline":
        return root / "artifacts" / parts[1] / "baseline" / "oracle" / parts[2]
    if len(parts) == 4 and parts[0] == "atom":
        return (
            root
            / "artifacts"
            / parts[1]
            / "atoms"
            / parts[2]
            / "oracle"
            / parts[3]
        )
    raise ObserverError(f"invalid oracle run ID: {run_id}")


def expected_subject_input_root(root: Path, run_id: str) -> Path:
    parts = run_id.split(":")
    require(
        len(parts) == 4 and parts[0] == "subject",
        f"invalid subject run ID: {run_id}",
    )
    if parts[1] == "baseline":
        return root / parts[2] / "baseline-private" / "inputs"
    require(parts[1] == "atom", f"invalid subject run kind: {run_id}")
    return root / parts[2] / f"atom-{parts[3]}" / "inputs"


def producer_receipts(
    receipt: dict[str, Any], contract: dict[str, Any]
) -> tuple[dict[str, dict[str, Any]], dict[str, dict[str, Any]]]:
    records = receipt.get("records")
    pilots = contract["pilots"]
    require(isinstance(records, list) and len(records) == len(pilots), "record count drift")
    oracle: dict[str, dict[str, Any]] = {}
    identities = receipt.get("executionEvidence", {}).get("compilerIdentities")
    require(isinstance(identities, dict), "compiler identities are absent")
    for family in ("gcc", "clang"):
        value = identities.get(family)
        require(isinstance(value, dict), f"{family} identity receipt is absent")
        oracle[f"identity:{family}"] = value
    subjects: dict[str, dict[str, Any]] = {}
    for record, pilot in zip(records, pilots):
        require(
            isinstance(record, dict)
            and record.get("failureId") == pilot.get("failureId"),
            "producer receipt record order drift",
        )
        failure_id = record["failureId"]
        replays = record.get("subjectReplays")
        require(isinstance(replays, list) and len(replays) == 3, "subject replay drift")
        for index, value in enumerate(replays):
            require(isinstance(value, dict), "subject replay is not an object")
            subjects[f"subject:baseline:{failure_id}:{index:03d}"] = value
        references = record.get("referenceAcceptance")
        require(isinstance(references, dict), "baseline references are absent")
        for family in ("gcc", "clang"):
            value = references.get(family)
            require(isinstance(value, dict), "baseline oracle receipt is absent")
            oracle[f"baseline:{failure_id}:{family}"] = value
        atoms = record.get("minimality", {}).get("atoms")
        require(
            isinstance(atoms, list) and len(atoms) == pilot.get("atomCount"),
            "minimality atom count drift",
        )
        for index, atom in enumerate(atoms):
            require(isinstance(atom, dict), "minimality atom is not an object")
            subject = atom.get("subjectObservation")
            references = atom.get("referenceAcceptance")
            require(
                isinstance(subject, dict) and isinstance(references, dict),
                "minimality receipts are absent",
            )
            subjects[f"subject:atom:{failure_id}:{index:03d}"] = subject
            for family in ("gcc", "clang"):
                value = references.get(family)
                require(isinstance(value, dict), "atom oracle receipt is absent")
                oracle[f"atom:{failure_id}:{index:03d}:{family}"] = value
    return oracle, subjects


def copy_artifact(
    source_root: Path,
    value: Any,
    destination_root: Path,
    relative: str,
    label: str,
) -> dict[str, Any]:
    source = confined_artifact(source_root, value, label)
    destination = destination_root / safe_relative(relative, f"{label} destination")
    write_new(destination, source.read_bytes(), f"{label} copy")
    return file_descriptor(destination_root, destination)


def input_mount_source(descriptor: dict[str, Any], label: str) -> str | None:
    configuration = descriptor.get("configuration")
    require(isinstance(configuration, dict), f"{label} configuration is absent")
    mounts = configuration.get("mounts")
    require(isinstance(mounts, list), f"{label} mounts are absent")
    sources = [
        item.get("source")
        for item in mounts
        if isinstance(item, dict) and item.get("destination") == "/inputs"
    ]
    require(len(sources) <= 1, f"{label} has duplicate /inputs mounts")
    if not sources:
        return None
    source = sources[0]
    require(isinstance(source, str) and Path(source).is_absolute(), f"{label} source invalid")
    return source


def run_control(argv: list[str], label: str) -> subprocess.CompletedProcess[bytes]:
    try:
        return subprocess.run(
            argv,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise ObserverError(f"{label} timed out") from error


def capture(args: argparse.Namespace) -> int:
    argv = list(args.argv)
    if argv and argv[0] == "--":
        argv = argv[1:]
    require(bool(argv), "capture command is empty")
    require(args.timeout > 0, "capture timeout must be positive")
    environment: dict[str, str] = {}
    for item in args.environment:
        require("=" in item, "capture environment item lacks '='")
        name, value = item.split("=", 1)
        require(bool(name) and name not in environment, "capture environment is invalid")
        environment[name] = value
    for path, label in (
        (args.stdout, "capture stdout"),
        (args.stderr, "capture stderr"),
        (args.status, "capture status"),
    ):
        require(not path.exists(), f"{label} already exists")
        path.parent.mkdir(parents=True, exist_ok=True)

    timed_out = False
    try:
        with args.stdout.open("xb") as stdout, args.stderr.open("xb") as stderr:
            completed = subprocess.run(
                argv,
                env=environment,
                stdin=subprocess.DEVNULL,
                stdout=stdout,
                stderr=stderr,
                timeout=args.timeout,
                check=False,
            )
        return_code = completed.returncode
    except subprocess.TimeoutExpired:
        timed_out = True
        return_code = 124

    status = {
        "argvSha256": digest(canonical(argv)),
        "environmentSha256": digest(canonical(environment)),
        "returnCode": return_code,
        "stderrSha256": digest(args.stderr.read_bytes()),
        "stdoutSha256": digest(args.stdout.read_bytes()),
        "timedOut": timed_out,
    }
    write_new(args.status, canonical(status), "capture status")
    return 0


def audit_producer_receipt(
    receipt_root: Path,
    contract: dict[str, Any],
    policy: dict[str, Any],
    overlay_sha256: str,
    expected_oracle_ids: list[str],
    child_result: dict[str, Any],
    contract_sha256: str,
    policy_sha256: str,
) -> dict[str, Any]:
    receipt_path = receipt_root / "receipt.json"
    receipt = load_object(receipt_path, "producer receipt")
    receipt_bytes = receipt_path.read_bytes()
    require(canonical(receipt) == receipt_bytes, "producer receipt is noncanonical")
    require(receipt.get("schemaVersion") == 2, "producer receipt schema drift")
    require("observerEvidence" not in receipt, "producer forged observer evidence")
    require(
        receipt.get("pilotContractSha256") == contract_sha256
        and receipt.get("policySha256") == policy_sha256
        and receipt.get("overlayClosureSha256") == overlay_sha256,
        "producer root authority drift",
    )
    require(
        receipt.get("pilotFailureIds")
        == [value.get("failureId") for value in contract["pilots"]],
        "producer pilot identity drift",
    )
    require(
        receipt.get("executionAuthority")
        == policy.get("sourceEvidence", {}).get("executionAuthority"),
        "producer execution authority drift",
    )
    require(
        receipt.get("artifactClosure")
        == regular_closure(receipt_root, excluded={"receipt.json"}),
        "producer artifact closure drift",
    )
    evidence = receipt.get("executionEvidence")
    require(isinstance(evidence, dict), "producer execution evidence is absent")
    runs = evidence.get("runs")
    require(
        isinstance(runs, list)
        and [value.get("runId") for value in runs if isinstance(value, dict)]
        == expected_oracle_ids
        and evidence.get("runOrderSha256") == digest(canonical(expected_oracle_ids)),
        "producer oracle run order drift",
    )
    require(
        child_result.get("receiptSha256") == digest(receipt_bytes),
        "producer result does not bind its receipt",
    )
    return receipt


def build_subject_evidence(
    producer_root: Path,
    receipt: dict[str, Any],
    contract: dict[str, Any],
    execution_root: Path,
    expected_ids: list[str],
    expected_subject_sha256: str,
) -> list[dict[str, Any]]:
    _oracle, receipts = producer_receipts(receipt, contract)
    require(list(receipts) == expected_ids, "subject receipt order drift")
    values: list[dict[str, Any]] = []
    status_keys = {
        "argvSha256",
        "environmentSha256",
        "returnCode",
        "stderrSha256",
        "stdoutSha256",
        "timedOut",
    }
    for run_id in expected_ids:
        value = receipts[run_id]
        root = expected_subject_input_root(execution_root, run_id)
        root_text = root.as_posix()
        observed_closure = directory_closure_sha256(root, f"subject {run_id}")
        status_path = confined_artifact(
            producer_root, value.get("processStatus"), f"subject {run_id} status"
        )
        status = exact_keys(
            load_object(status_path, f"subject {run_id} status"),
            status_keys,
            f"subject {run_id} status",
        )
        expected_status = {
            "argvSha256": value.get("argvSha256"),
            "environmentSha256": value.get("environmentSha256"),
            "returnCode": value.get("returnCode"),
            "stderrSha256": value.get("stderrSha256"),
            "stdoutSha256": value.get("stdoutSha256"),
            "timedOut": value.get("timedOut"),
        }
        require(status == expected_status, f"subject {run_id} process status drift")
        require(
            value.get("processStatusSha256") == digest(status_path.read_bytes())
            and value.get("subjectSha256") == expected_subject_sha256
            and value.get("inputClosureSha256") == observed_closure
            and value.get("timedOut") is False,
            f"subject {run_id} evidence drift",
        )
        values.append(
            {
                "argvSha256": value["argvSha256"],
                "environmentSha256": value["environmentSha256"],
                "expectedInputClosureSha256": value["inputClosureSha256"],
                "inputRoot": root_text,
                "inputRootSha256": digest(canonical(root_text)),
                "observedInputClosureSha256": observed_closure,
                "processStatusSha256": value["processStatusSha256"],
                "returnCode": value["returnCode"],
                "runId": run_id,
                "stderrSha256": value["stderrSha256"],
                "stdoutSha256": value["stdoutSha256"],
                "subjectSha256": value["subjectSha256"],
                "timedOut": value["timedOut"],
            }
        )
    return values


def build_oracle_evidence(
    producer_root: Path,
    final_root: Path,
    receipt: dict[str, Any],
    contract: dict[str, Any],
    oracle_input_root: Path,
    expected_ids: list[str],
) -> list[dict[str, Any]]:
    receipts, _subjects = producer_receipts(receipt, contract)
    require(list(receipts) == expected_ids, "oracle receipt order drift")
    execution_runs = receipt["executionEvidence"]["runs"]
    runs = {value["runId"]: value for value in execution_runs}
    require(list(runs) == expected_ids, "execution run order drift")
    values: list[dict[str, Any]] = []
    for index, run_id in enumerate(expected_ids):
        run = runs[run_id]
        measured = receipts[run_id]
        parts = run_id.split(":")
        purpose = (
            "compiler-identity"
            if parts[0] == "identity"
            else "baseline-acceptance"
            if parts[0] == "baseline"
            else "minimality-deletion"
        )
        family = parts[-1]
        require(
            run.get("purpose") == purpose and run.get("family") == family,
            f"oracle {run_id} purpose/family drift",
        )
        before_value = json_artifact(
            producer_root, run.get("containerBefore"), f"oracle {run_id} before"
        )
        after_value = json_artifact(
            producer_root, run.get("containerAfter"), f"oracle {run_id} after"
        )
        name = run.get("containerName")
        require(
            isinstance(name, str)
            and before_value.get("id") == name
            and after_value.get("id") == name,
            f"oracle {run_id} descriptor identity drift",
        )
        after_status = after_value.get("status")
        require(
            isinstance(after_status, dict)
            and after_status.get("state") == "stopped"
            and isinstance(after_status.get("startedDate"), str)
            and bool(after_status["startedDate"]),
            f"oracle {run_id} after status does not prove execution",
        )
        process_status_path = confined_artifact(
            producer_root, run.get("processStatus"), f"oracle {run_id} process status"
        )
        process_status = exact_keys(
            load_object(process_status_path, f"oracle {run_id} process status"),
            {
                "argvSha256",
                "environmentSha256",
                "returnCode",
                "stderrSha256",
                "stdoutSha256",
                "timedOut",
            },
            f"oracle {run_id} process status",
        )
        exit_code = process_status["returnCode"]
        expected_exit = 0 if parts[0] == "identity" else measured.get("returnCode")
        timed_out = process_status["timedOut"]
        require(
            isinstance(exit_code, int)
            and not isinstance(exit_code, bool)
            and exit_code == expected_exit
            and timed_out is False
            and run.get("processStatusSha256") == digest(process_status_path.read_bytes())
            and process_status["argvSha256"]
            == digest(
                canonical(
                    [
                        receipt["executionEvidence"]["containerCli"]["path"],
                        "start",
                        "--attach",
                        name,
                    ]
                )
            )
            and process_status["stdoutSha256"] == measured.get("stdoutSha256")
            and process_status["stderrSha256"] == measured.get("stderrSha256"),
            f"oracle {run_id} outcome drift",
        )
        input_root = expected_oracle_input_root(oracle_input_root, run_id)
        mount_source = input_mount_source(before_value, f"oracle {run_id} before")
        require(
            input_mount_source(after_value, f"oracle {run_id} after") == mount_source,
            f"oracle {run_id} mount changed during execution",
        )
        if input_root is None:
            observed_closure: str | None = None
            require(mount_source is None, f"identity run {run_id} has /inputs")
        else:
            observed_closure = directory_closure_sha256(input_root, f"oracle {run_id}")
            require(mount_source == input_root.as_posix(), f"oracle {run_id} mount drift")
        expected_closure = run.get("inputClosureSha256")
        if parts[0] != "identity":
            require(
                measured.get("inputClosureSha256") == expected_closure,
                f"oracle {run_id} receipt input closure drift",
            )
        require(observed_closure == expected_closure, f"oracle {run_id} closure drift")
        before = copy_artifact(
            producer_root,
            run["containerBefore"],
            final_root,
            f"observer/oracle-runs/{index:03d}/before.json",
            f"oracle {run_id} before",
        )
        after = copy_artifact(
            producer_root,
            run["containerAfter"],
            final_root,
            f"observer/oracle-runs/{index:03d}/after.json",
            f"oracle {run_id} after",
        )
        environment = measured.get("environment")
        require(isinstance(environment, dict), f"oracle {run_id} environment absent")
        command_sha256 = measured.get("commandSha256")
        require(isinstance(command_sha256, str), f"oracle {run_id} command hash absent")
        values.append(
            {
                "afterDescriptor": after,
                "beforeDescriptor": before,
                "cleanupTargetIndex": index,
                "commandSha256": command_sha256,
                "containerName": name,
                "environmentSha256": digest(canonical(environment)),
                "exitCode": exit_code,
                "expectedInputClosureSha256": expected_closure,
                "family": family,
                "mountSource": mount_source,
                "mountSourceSha256": (
                    digest(canonical(mount_source)) if mount_source is not None else None
                ),
                "observedInputClosureSha256": observed_closure,
                "purpose": purpose,
                "processStatusSha256": run["processStatusSha256"],
                "runId": run_id,
                "timedOut": timed_out,
            }
        )
    return values


def parsed_inventory_artifact(
    producer_root: Path, value: Any, label: str
) -> list[Any]:
    path = confined_artifact(producer_root, value, label)
    parsed = json.loads(path.read_text(encoding="utf-8"))
    require(isinstance(parsed, list), f"{label} is not an array")
    return parsed


def image_alias_absent(inventory: list[Any], alias: str) -> bool:
    canonical_alias = alias if "/" in alias.split(":", 1)[0] else f"docker.io/library/{alias}"
    return all(
        not isinstance(item, dict)
        or not isinstance(item.get("configuration"), dict)
        or item["configuration"].get("name") not in {alias, canonical_alias}
        for item in inventory
    )


def build_cleanup_ledger(
    producer_root: Path,
    final_root: Path,
    receipt: dict[str, Any],
) -> dict[str, Any]:
    evidence = receipt["executionEvidence"]
    runs = evidence["runs"]
    names = [value["containerName"] for value in runs]
    alias = evidence["preflight"]["alias"]
    attempts: list[dict[str, Any]] = []
    for run in runs:
        cleanup = run.get("cleanup")
        require(isinstance(cleanup, dict), f"cleanup absent: {run['runId']}")
        inventory = parsed_inventory_artifact(
            producer_root,
            cleanup.get("postDeleteInventoryStdout"),
            f"cleanup inventory {run['runId']}",
        )
        target = run["containerName"]
        absent = all(
            not isinstance(item, dict) or item.get("id") != target
            for item in inventory
        )
        require(
            cleanup.get("deleteReturnCode") == 0
            and cleanup.get("postDeleteInventoryReturnCode") == 0
            and cleanup.get("targetAbsent") is True
            and absent,
            f"cleanup did not prove absence: {run['runId']}",
        )
        attempts.append(
            {
                "deleteAttempted": True,
                "deleteReturnCode": 0,
                "error": None,
                "stopAttempted": False,
                "stopReturnCode": None,
                "target": target,
                "targetKind": "container",
            }
        )
    alias_cleanup = evidence.get("aliasCleanup")
    require(isinstance(alias_cleanup, dict), "alias cleanup is absent")
    alias_inventory = parsed_inventory_artifact(
        producer_root,
        alias_cleanup.get("postDeleteInventoryStdout"),
        "alias cleanup inventory",
    )
    require(
        alias_cleanup.get("deleteReturnCode") == 0
        and alias_cleanup.get("postDeleteInventoryReturnCode") == 0
        and alias_cleanup.get("targetAbsent") is True
        and image_alias_absent(alias_inventory, alias),
        "alias cleanup did not prove absence",
    )
    attempts.append(
        {
            "deleteAttempted": True,
            "deleteReturnCode": 0,
            "error": None,
            "stopAttempted": False,
            "stopReturnCode": None,
            "target": alias,
            "targetKind": "image-alias",
        }
    )

    container_bin = evidence.get("containerCli", {}).get("path")
    require(isinstance(container_bin, str), "container CLI path is absent")
    container_inventory = run_control(
        [container_bin, "list", "--all", "--format", "json"],
        "observer container inventory",
    )
    image_inventory = run_control(
        [container_bin, "image", "list", "--format", "json", "--verbose"],
        "observer image inventory",
    )
    container_stdout = final_root / "observer/cleanup/container-inventory.json"
    image_stdout = final_root / "observer/cleanup/image-inventory.json"
    write_new(container_stdout, container_inventory.stdout, "container inventory")
    write_new(image_stdout, image_inventory.stdout, "image inventory")
    try:
        containers = json.loads(container_inventory.stdout)
        images = json.loads(image_inventory.stdout)
    except json.JSONDecodeError as error:
        raise ObserverError(f"observer inventory is malformed: {error}") from error
    require(isinstance(containers, list), "observer container inventory is not an array")
    require(isinstance(images, list), "observer image inventory is not an array")
    names_set = set(names)
    containers_absent = all(
        not isinstance(item, dict) or item.get("id") not in names_set
        for item in containers
    )
    alias_absent = image_alias_absent(images, alias)
    require(
        container_inventory.returncode == 0
        and image_inventory.returncode == 0
        and containers_absent
        and alias_absent,
        "observer post-cleanup inventory found residue or failed",
    )
    return {
        "aliasAbsent": alias_absent,
        "allAbsent": containers_absent and alias_absent,
        "attempts": attempts,
        "containerInventory": file_descriptor(final_root, container_stdout),
        "containerInventoryReturnCode": container_inventory.returncode,
        "containerNamesAbsent": containers_absent,
        "imageInventory": file_descriptor(final_root, image_stdout),
        "imageInventoryReturnCode": image_inventory.returncode,
        "plannedAlias": alias,
        "plannedContainerNames": names,
    }
def run(args: argparse.Namespace) -> int:
    require(not os.path.lexists(args.receipt_dir), "receipt directory already exists")
    require(not os.path.lexists(args.receipt_index), "receipt index already exists")
    args.receipt_dir.parent.mkdir(parents=True, exist_ok=True)
    args.receipt_index.parent.mkdir(parents=True, exist_ok=True)

    contract_bytes = args.pilot_contract.read_bytes()
    policy_bytes = args.policy.read_bytes()
    contract = load_object(args.pilot_contract, "pilot contract")
    policy = load_object(args.policy, "reduction policy")
    identity = observer_identity()
    observer = contract.get("observerAuthority")
    require(isinstance(observer, dict), "observer authority is absent")
    require(
        observer.get("authorityState") == "PINNED"
        and observer.get("liveMeasurementPermitted") is True
        and observer.get("provisional") is False
        and observer.get("protocolId") == "c30.day5.phase2-external-observer.v2"
        and observer.get("executable") == identity,
        "observer executable is not the externally pinned live authority",
    )
    contract_sha256 = digest(contract_bytes)
    policy_sha256 = digest(policy_bytes)
    overlay_sha256 = regular_closure(args.overlay)["sha256"]
    oracle_ids, subject_ids = expected_run_ids(contract)
    execution_authority = policy.get("sourceEvidence", {}).get("executionAuthority")
    require(isinstance(execution_authority, dict), "execution authority is absent")

    with tempfile.TemporaryDirectory(
        prefix=".day5-observer-session-", dir=args.receipt_dir.parent
    ) as session_value:
        session = Path(session_value).resolve(strict=True)
        execution_root = session / "execution"
        oracle_input_root = execution_root / "oracle-inputs"
        producer_root = session / "producer-receipt"
        plan = {
            "containerCliSha256": execution_authority["containerCliSha256"],
            "expectedImage": execution_authority["sealedImageId"],
            "observerExecutableSha256": identity["sha256"],
            "oracleInputRoot": oracle_input_root.as_posix(),
            "oracleInputRootSha256": digest(canonical(oracle_input_root.as_posix())),
            "oracleRunIds": oracle_ids,
            "oracleRunOrderSha256": digest(canonical(oracle_ids)),
            "overlayClosureSha256": overlay_sha256,
            "pilotContractSha256": contract_sha256,
            "policySha256": policy_sha256,
            "protocolId": "c30.day5.phase2-external-observer.v2",
            "schemaVersion": 1,
            "subjectExecutionRoot": execution_root.as_posix(),
            "subjectExecutionRootSha256": digest(canonical(execution_root.as_posix())),
            "subjectRunIds": subject_ids,
            "subjectRunOrderSha256": digest(canonical(subject_ids)),
        }
        plan_path = session / "plan.json"
        write_new(plan_path, canonical(plan), "observer pre-execution plan")

        completed = subprocess.run(
            [
                sys.executable,
                str(VERIFIER),
                "verify-pilots",
                "--pilot-contract",
                str(args.pilot_contract),
                "--policy",
                str(args.policy),
                "--evidence",
                str(args.evidence),
                "--overlay",
                str(args.overlay),
                "--receipt-dir",
                str(producer_root),
                "--execution-root",
                str(execution_root),
            ],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=1800,
            check=False,
        )
        require(
            completed.returncode == 0,
            "single verifier execution failed: "
            + completed.stderr.decode("utf-8", errors="replace").strip(),
        )
        require(completed.stderr == b"", "single verifier execution wrote stderr")
        try:
            child_result = json.loads(completed.stdout)
        except json.JSONDecodeError as error:
            raise ObserverError(f"verifier result is malformed: {error}") from error
        require(isinstance(child_result, dict), "verifier result is not an object")
        require(canonical(child_result) == completed.stdout, "verifier result is noncanonical")
        producer_receipt = audit_producer_receipt(
            producer_root,
            contract,
            policy,
            overlay_sha256,
            oracle_ids,
            child_result,
            contract_sha256,
            policy_sha256,
        )

        final_staging = Path(
            tempfile.mkdtemp(
                prefix=".day5-sealed-receipt-", dir=args.receipt_dir.parent
            )
        )
        try:
            shutil.copytree(producer_root, final_staging, dirs_exist_ok=True)
            (final_staging / "receipt.json").unlink()
            final_plan_path = final_staging / "observer/plan.json"
            write_new(final_plan_path, plan_path.read_bytes(), "sealed observer plan")
            subject_runs = build_subject_evidence(
                producer_root,
                producer_receipt,
                contract,
                execution_root,
                subject_ids,
                execution_authority["subjectSha256"],
            )
            oracle_runs = build_oracle_evidence(
                producer_root,
                final_staging,
                producer_receipt,
                contract,
                oracle_input_root,
                oracle_ids,
            )
            cleanup_ledger = build_cleanup_ledger(
                producer_root, final_staging, producer_receipt
            )
            plan_descriptor = file_descriptor(final_staging, final_plan_path)
            attestation = {
                "cleanupLedger": cleanup_ledger,
                "observerIdentity": identity,
                "oracleRunOrderSha256": plan["oracleRunOrderSha256"],
                "oracleRuns": oracle_runs,
                "planSha256": plan_descriptor["sha256"],
                "protocolId": plan["protocolId"],
                "schemaVersion": 1,
                "subjectRunOrderSha256": plan["subjectRunOrderSha256"],
                "subjectRuns": subject_runs,
            }
            attestation_path = final_staging / "observer/attestation.json"
            write_new(attestation_path, canonical(attestation), "observer attestation")
            attestation_descriptor = file_descriptor(final_staging, attestation_path)
            observer_evidence = {
                **attestation,
                "attestation": attestation_descriptor,
                "attestationSha256": attestation_descriptor["sha256"],
                "plan": plan_descriptor,
            }
            sealed_receipt = dict(producer_receipt)
            sealed_receipt["observerEvidence"] = observer_evidence
            sealed_receipt["schemaVersion"] = 3
            sealed_receipt["artifactClosure"] = regular_closure(final_staging)
            sealed_bytes = canonical(sealed_receipt)
            sealed_sha256 = digest(sealed_bytes)
            write_new(
                final_staging / "receipt.json", sealed_bytes, "sealed pilot receipt"
            )
            publish_no_replace(final_staging, args.receipt_dir)
        finally:
            if final_staging.exists():
                shutil.rmtree(final_staging)

        index = {
            "observerAttestationSha256": observer_evidence["attestationSha256"],
            "observerExecutableSha256": identity["sha256"],
            "oracleRunOrderSha256": plan["oracleRunOrderSha256"],
            "overlayClosureSha256": overlay_sha256,
            "pilotContractSha256": contract_sha256,
            "policySha256": policy_sha256,
            "receiptSha256": sealed_sha256,
            "schemaVersion": 1,
            "subjectRunOrderSha256": plan["subjectRunOrderSha256"],
        }
        staged_index = session / "receipt-index.json"
        write_new(staged_index, canonical(index), "trusted receipt index")
        publish_no_replace(staged_index, args.receipt_index)
        child_result["receiptSha256"] = sealed_sha256
        child_result["mode"] = "executed"
        sys.stdout.buffer.write(canonical(child_result))
        return 0


def parser() -> argparse.ArgumentParser:
    value = argparse.ArgumentParser(description=__doc__)
    commands = value.add_subparsers(dest="command", required=True)
    capture_command = commands.add_parser("capture")
    capture_command.add_argument("--stdout", type=Path, required=True)
    capture_command.add_argument("--stderr", type=Path, required=True)
    capture_command.add_argument("--status", type=Path, required=True)
    capture_command.add_argument("--timeout", type=int, required=True)
    capture_command.add_argument("--environment", action="append", required=True)
    capture_command.add_argument("argv", nargs=argparse.REMAINDER)
    run_command = commands.add_parser("run")
    run_command.add_argument("--pilot-contract", type=Path, required=True)
    run_command.add_argument("--policy", type=Path, required=True)
    run_command.add_argument("--evidence", type=Path, required=True)
    run_command.add_argument("--overlay", type=Path, required=True)
    run_command.add_argument("--receipt-dir", type=Path, required=True)
    run_command.add_argument("--receipt-index", type=Path, required=True)
    return value


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        if args.command == "capture":
            return capture(args)
        if args.command == "run":
            return run(args)
        raise ObserverError("unknown command")
    except (ObserverError, OSError, ValueError, subprocess.SubprocessError) as error:
        print(f"{FAILURE_PREFIX} {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
