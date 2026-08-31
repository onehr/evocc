#!/usr/bin/env python3
"""Replay the frozen Day-5 kernel cohort without modifying kernel inputs.

The verifier fails closed on source, config, command-record, decoded argv,
selection, or dependency-closure drift.  The observation pass then records:

* EvoCPP on each raw source using the preprocessing-affecting Kbuild argv
  projection; and
* EvoCC parsing GCC-14 and Clang-19 preprocessed input produced in the sealed,
  networkless reference image.

Compiler failures are telemetry, not harness failures.  Every one of the 48
required observations is emitted; there is deliberately no SKIP state.
"""

from __future__ import annotations

import argparse
from concurrent.futures import ThreadPoolExecutor
import datetime as dt
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import sys
import tempfile
from typing import Any, Iterable


PROJECT_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_CONTRACT = PROJECT_ROOT / "tests/contracts/day5-kernel-cohort.json"
BUILD_ENV_CONTRACT = PROJECT_ROOT / "contracts/build-environment.json"
KERNEL_CONTRACT = PROJECT_ROOT / "contracts/kernel-target.json"
SOURCE_ROOT = PROJECT_ROOT / ".build/linux-src"
REFERENCE_ROOT = PROJECT_ROOT / ".build/reference"
DEFAULT_EVIDENCE_ROOT = PROJECT_ROOT / ".evidence/day5-kernel-reconnaissance"
SUBJECT_BUILD_PROFILE = "release"

FAMILIES = ("gcc", "clang")
LANES = ("gcc-lane-1", "gcc-lane-2", "clang-lane-1", "clang-lane-2")
REQUIRED_DOMAINS = ("init", "kernel", "lib", "mm")
FAILURE_CLASSES = (
    "driver",
    "preprocess",
    "lex",
    "parse",
    "sema",
    "layout",
    "out-of-Day5",
)
DIAGNOSTIC_RE = re.compile(
    r"^(?P<path>.+):(?P<line>[0-9]+):(?P<column>[0-9]+): "
    r"error\[(?P<code>[^]]+)\]: (?P<message>.*)$"
)
LINE_MARKER_RE = re.compile(r'^#\s+([0-9]+)\s+"((?:[^"\\]|\\.)*)"(?:\s+.*)?$')
FORBIDDEN_BUILD_ENVIRONMENT = frozenset(
    {
        "AR",
        "CC",
        "CFLAGS",
        "CPPFLAGS",
        "CXX",
        "CXXFLAGS",
        "LDFLAGS",
        "RANLIB",
        "RUSTC",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
        "RUSTDOC",
        "RUSTDOCFLAGS",
        "RUSTFLAGS",
        "CARGO_BUILD_RUSTC",
        "CARGO_BUILD_RUSTC_WRAPPER",
        "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
        "CARGO_BUILD_RUSTFLAGS",
        "CARGO_BUILD_RUSTDOC",
        "CARGO_BUILD_TARGET",
        "CARGO_ENCODED_RUSTDOCFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
    }
)


class ContractError(RuntimeError):
    """The frozen input contract or local reference state is invalid."""


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def canonical_json(value: Any) -> bytes:
    return (json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n").encode()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ContractError(message)


def contains_skip_state(value: Any) -> bool:
    if isinstance(value, dict):
        return any(
            str(key).lower() in {"skip", "skipped"} or contains_skip_state(item)
            for key, item in value.items()
        )
    if isinstance(value, list):
        return any(contains_skip_state(item) for item in value)
    return isinstance(value, str) and value.lower() in {"skip", "skipped"}


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot read JSON {path}: {error}") from error
    require(isinstance(value, dict), f"JSON root is not an object: {path}")
    return value


def exact_file_closure(files: dict[str, Path]) -> bytes:
    rows: list[bytes] = []
    for logical, path in sorted(files.items()):
        require(bool(logical) and not logical.startswith("/"), f"invalid closure path {logical!r}")
        require(path.is_file() and not path.is_symlink(), f"closure input is absent or symbolic: {path}")
        data = path.read_bytes()
        rows.append(f"{logical}\t{sha256_bytes(data)}\t{len(data)}\n".encode("utf-8"))
    require(bool(rows), "input closure is empty")
    return b"".join(rows)


def require_exact_closure(expected: bytes, observed: bytes, label: str) -> None:
    require(observed == expected, f"{label} closure drift")


def directory_file_closure(root: Path) -> bytes:
    require(root.is_dir() and not root.is_symlink(), f"closure directory is absent or symbolic: {root}")
    files = {
        path.relative_to(root).as_posix(): path
        for path in root.rglob("*")
        if path.is_file()
    }
    return exact_file_closure(files)


def evocc_build_input_closure() -> bytes:
    files: dict[str, Path] = {}
    for relative in (
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "rust-toolchain",
        ".cargo/config.toml",
        ".cargo/config",
    ):
        path = PROJECT_ROOT / relative
        if path.is_file():
            files[relative] = path
    crates_root = PROJECT_ROOT / "crates"
    for path in crates_root.rglob("*"):
        if path.is_file():
            files[path.relative_to(PROJECT_ROOT).as_posix()] = path
    return exact_file_closure(files)


def verify_pinned_executable(
    requested_path: Path,
    expected_path: str,
    expected_sha256: str,
    label: str,
) -> Path:
    require(requested_path.is_absolute(), f"{label} path is not absolute")
    require(str(requested_path) == expected_path, f"{label} path drift")
    require(
        requested_path.is_file() and not requested_path.is_symlink(),
        f"{label} is absent or symbolic: {requested_path}",
    )
    require(os.access(requested_path, os.X_OK), f"{label} is not executable: {requested_path}")
    require(
        re.fullmatch(r"[0-9a-f]{64}", expected_sha256) is not None,
        f"{label} authority hash is malformed",
    )
    require(sha256_file(requested_path) == expected_sha256, f"{label} binary hash drift")
    return requested_path


def verify_execution_authority(build_env: dict[str, Any]) -> dict[str, Path]:
    require(build_env.get("schemaVersion") == 1, "unsupported build-environment schema")
    require(build_env.get("contractState") == "locked", "build-environment contract is not locked")
    runtime = build_env["containerRuntime"]
    host = build_env["toolchain"]["hostReplay"]
    container = verify_pinned_executable(
        Path(runtime["cliPath"]),
        runtime["cliPath"],
        runtime["cliSha256"],
        "Apple Container CLI",
    )
    cargo = verify_pinned_executable(
        Path(host["cargo"]["path"]),
        host["cargo"]["path"],
        host["cargo"]["sha256"],
        "host cargo",
    )
    rustc = verify_pinned_executable(
        Path(host["rustc"]["path"]),
        host["rustc"]["path"],
        host["rustc"]["sha256"],
        "host rustc",
    )
    rust_authority = build_env["toolchain"]["rust"]
    require(rust_authority["rustcPath"] == str(rustc), "Rust authority path disagreement")
    require(rust_authority["rustcSha256"] == host["rustc"]["sha256"], "Rust authority hash disagreement")
    return {"container": container, "cargo": cargo, "rustc": rustc}


def build_override_names(environment: dict[str, str]) -> list[str]:
    forbidden: list[str] = []
    for name in environment:
        if name in FORBIDDEN_BUILD_ENVIRONMENT:
            forbidden.append(name)
            continue
        if name.startswith("CARGO_PROFILE_"):
            forbidden.append(name)
            continue
        if name.startswith("CARGO_TARGET_") and name.endswith(
            ("_LINKER", "_RUNNER", "_RUSTDOCFLAGS", "_RUSTFLAGS")
        ):
            forbidden.append(name)
    return sorted(forbidden)


def require_no_build_overrides(environment: dict[str, str]) -> None:
    forbidden = build_override_names(environment)
    require(not forbidden, "forbidden host build overrides: " + ",".join(forbidden))


def build_shaping_names(environment: dict[str, str]) -> list[str]:
    exact = {
        "AR",
        "CC",
        "CFLAGS",
        "CPPFLAGS",
        "CXX",
        "CXXFLAGS",
        "DEVELOPER_DIR",
        "HOME",
        "LANG",
        "LDFLAGS",
        "MACOSX_DEPLOYMENT_TARGET",
        "PATH",
        "RANLIB",
        "SDKROOT",
        "SOURCE_DATE_EPOCH",
        "TMPDIR",
        "TZ",
    }
    return sorted(
        name
        for name in environment
        if name in exact
        or name.startswith("CARGO")
        or name.startswith("RUST")
        or name.startswith("LC_")
    )


def sanitized_cargo_environment(
    incoming: dict[str, str],
    evidence: Path,
    rustc: Path,
    build_env: dict[str, Any],
) -> tuple[dict[str, str], list[str]]:
    require_no_build_overrides(incoming)
    home = evidence / "host-build-home"
    cargo_home = evidence / "cargo-home"
    temporary = evidence / "host-build-tmp"
    target = evidence / "cargo-target"
    for directory in (home, cargo_home, temporary, target):
        directory.mkdir()
    environment = {
        "CARGO_BUILD_JOBS": "1",
        "CARGO_HOME": str(cargo_home),
        "CARGO_INCREMENTAL": "0",
        "CARGO_TARGET_DIR": str(target),
        "CARGO_TERM_COLOR": "never",
        "HOME": str(home),
        "LANG": "C",
        "LC_ALL": "C",
        "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
        "RUST_BACKTRACE": "0",
        "RUSTC": str(rustc),
        "SOURCE_DATE_EPOCH": build_env["reproducibility"]["SOURCE_DATE_EPOCH"],
        "TMPDIR": str(temporary),
        "TZ": "UTC",
        "ZERO_AR_DATE": "1",
    }
    return environment, build_shaping_names(incoming)


def first_saved_command(record: Path) -> tuple[str, list[str]]:
    try:
        first_line = record.read_bytes().splitlines()[0].decode("utf-8")
    except (OSError, IndexError, UnicodeDecodeError) as error:
        raise ContractError(f"cannot decode first command from {record}: {error}") from error
    require(" := " in first_line, f"missing saved-command delimiter in {record}")
    command = first_line.split(" := ", 1)[1]
    try:
        argv = shlex.split(command, posix=True)
    except ValueError as error:
        raise ContractError(f"cannot decode POSIX argv in {record}: {error}") from error
    require(bool(argv), f"empty saved command in {record}")
    return command, argv


def argv_sha256(argv: Iterable[str]) -> str:
    return sha256_bytes(b"".join(arg.encode("utf-8") + b"\0" for arg in argv))


def source_declaration(record: Path) -> str | None:
    for line in record.read_text(encoding="utf-8").splitlines():
        match = re.match(r"^source_[^ ]+ := /work/src/(.+)$", line)
        if match:
            return match.group(1)
    return None


def verify_record_source(record: Path, source: str) -> None:
    require(
        source_declaration(record) == source,
        f"missing or mismatched source declaration in four-lane C command: {record}",
    )
    _, argv = first_saved_command(record)
    require(
        argv.count(f"/work/src/{source}") == 1,
        f"source argv is missing or ambiguous in four-lane C command: {record}",
    )


def dependency_closure(lane: str, source: str, record_rel: str) -> tuple[bytes, int, int]:
    record = REFERENCE_ROOT / lane / record_rel
    lines = record.read_text(encoding="utf-8").splitlines()
    try:
        start = next(index for index, line in enumerate(lines) if line.startswith("deps_"))
    except StopIteration as error:
        raise ContractError(f"missing dependency declaration in {record}") from error

    files: dict[str, Path] = {}
    for raw_line in lines[start + 1 :]:
        value = raw_line.strip()
        if not value:
            break
        if value.endswith("\\"):
            value = value[:-1].rstrip()
        wildcard = re.fullmatch(r"\$\(wildcard ([^)]+)\)", value)
        if wildcard:
            rel = wildcard.group(1)
            path = REFERENCE_ROOT / lane / rel
            if path.is_file():
                files[f"output/{rel}"] = path
            continue
        if value.startswith("/work/src/"):
            rel = value[len("/work/src/") :]
            path = SOURCE_ROOT / rel
            logical = f"source/{rel}"
        else:
            rel = value
            path = REFERENCE_ROOT / lane / rel
            logical = f"output/{rel}"
        require(path.is_file(), f"declared dependency is absent: {lane}:{value}")
        files[logical] = path

    files[f"source/{source}"] = SOURCE_ROOT / source
    files["output/.config"] = REFERENCE_ROOT / lane / ".config"
    rows: list[bytes] = []
    total_bytes = 0
    for logical, path in sorted(files.items()):
        data = path.read_bytes()
        total_bytes += len(data)
        rows.append(f"{logical}\t{sha256_bytes(data)}\t{len(data)}\n".encode("utf-8"))
    return b"".join(rows), len(rows), total_bytes


def candidate_inventory(domain: str, seed: str) -> list[tuple[str, str, str]]:
    candidates: dict[str, tuple[str, str]] = {}
    source_domain = SOURCE_ROOT / domain
    require(source_domain.is_dir(), f"missing kernel source directory {source_domain}")
    for source_path in sorted(source_domain.rglob("*.c")):
        if not source_path.is_file() or source_path.is_symlink() or source_path.stat().st_size == 0:
            continue
        source = source_path.relative_to(SOURCE_ROOT).as_posix()
        source_rel = Path(source)
        record_rel = (source_rel.parent / f".{source_rel.stem}.o.cmd").as_posix()
        records = [REFERENCE_ROOT / lane / record_rel for lane in LANES]
        if not all(record.is_file() for record in records):
            continue
        for record in records:
            require(not record.is_symlink(), f"command record is symbolic: {record}")
            verify_record_source(record, source)
        rank = sha256_bytes(f"{seed}\t{source}\n".encode("utf-8"))
        candidates[source] = (rank, record_rel)
    return sorted((rank, source, record) for source, (rank, record) in candidates.items())


def verify_config_contract(contract: dict[str, Any]) -> None:
    for family in FAMILIES:
        entry = contract["configs"][family]
        record = PROJECT_ROOT / entry["recordPath"]
        require(record.is_file() and not record.is_symlink(), f"missing config record {record}")
        require(sha256_file(record) == entry["sha256"], f"{family} config record drift")
        expected_lanes = {f"{family}-lane-1", f"{family}-lane-2"}
        require(set(entry["laneCopies"]) == expected_lanes, f"{family} config lane set drift")
        for lane, expected_hash in entry["laneCopies"].items():
            lane_config = REFERENCE_ROOT / lane / ".config"
            require(lane_config.is_file(), f"missing lane config {lane_config}")
            require(sha256_file(lane_config) == expected_hash, f"lane config drift: {lane}")
            require(expected_hash == entry["sha256"], f"lane config differs from record: {lane}")


def verify_contract(contract_path: Path, evidence_inputs: Path | None) -> dict[str, Any]:
    contract = load_json(contract_path)
    kernel_contract = load_json(KERNEL_CONTRACT)
    require(contract.get("schemaVersion") == 1, "unsupported cohort schema")
    require(contract.get("contractState") == "locked", "cohort is not locked")
    require(tuple(contract.get("classifications", ())) == FAILURE_CLASSES, "failure-class enum drift")
    require(not contains_skip_state(contract), "cohort contract contains a skip state")
    require(contract["kernel"]["resolvedCommit"] == kernel_contract["kernel"]["tarball"]["resolvedCommit"], "kernel commit mismatch")
    require(contract["kernel"]["sourceArchiveSha256"] == kernel_contract["kernel"]["tarball"]["sha256"], "kernel archive mismatch")
    archive_path = (PROJECT_ROOT / contract["kernel"]["sourceArchivePath"]).resolve()
    require(PROJECT_ROOT.resolve() in archive_path.parents, "kernel archive path escapes project")
    require(archive_path.is_file() and not archive_path.is_symlink(), "pinned kernel archive is absent or symbolic")
    require(sha256_file(archive_path) == contract["kernel"]["sourceArchiveSha256"], "pinned kernel archive bytes drift")

    day4 = contract["frozenBeforeProductionEdit"]
    require(day4["day4Commit"] == "2f1c1570ee5268a082734dd8061e49bd2ff37dbf", "Day-4 baseline commit drift")
    tree_probe = subprocess.run(
        ["git", "rev-parse", f"{day4['day4Commit']}^{{tree}}"],
        cwd=PROJECT_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=10,
        check=False,
    )
    require(tree_probe.returncode == 0, "cannot resolve frozen Day-4 commit tree")
    observed_tree = tree_probe.stdout.decode("ascii", errors="strict").strip()
    require(observed_tree == day4["day4Tree"], "frozen Day-4 tree authority drift")

    selection = contract["selection"]
    require(selection["selectedTotal"] == 16, "cohort must contain exactly 16 TUs")
    require(selection["selectedPerDomain"] == 4, "cohort must contain four TUs per domain")
    units = contract["translationUnits"]
    require(len(units) == selection["selectedTotal"], "selectedTotal does not match TU list")
    require(len({unit["id"] for unit in units}) == len(units), "duplicate TU id")
    require(len({unit["sourcePath"] for unit in units}) == len(units), "duplicate TU source")

    inventory: dict[str, list[tuple[str, str, str]]] = {}
    for domain in REQUIRED_DOMAINS:
        inventory[domain] = candidate_inventory(domain, selection["seed"])
        require(
            len(inventory[domain]) == selection["eligibleCounts"][domain],
            f"eligible denominator drift for {domain}",
        )
        selected = [unit for unit in units if unit["domain"] == domain]
        require(len(selected) == 4, f"domain does not have four selected TUs: {domain}")
        expected = inventory[domain][:4]
        observed = [(unit["selectionRankSha256"], unit["sourcePath"], unit["commandRecordPath"]) for unit in selected]
        require(observed == expected, f"post-hoc cohort substitution detected in {domain}")

    verify_config_contract(contract)
    if evidence_inputs is not None:
        evidence_inputs.mkdir(parents=True, exist_ok=False)

    for unit in units:
        source = unit["sourcePath"]
        source_path = SOURCE_ROOT / source
        require(source_path.is_file() and not source_path.is_symlink(), f"source is absent/generated: {source}")
        source_data = source_path.read_bytes()
        require(bool(source_data), f"source is empty: {source}")
        require(len(source_data) == unit["sourceBytes"], f"source byte length drift: {source}")
        require(sha256_bytes(source_data) == unit["sourceSha256"], f"source hash drift: {source}")

        for family in FAMILIES:
            command_entry = unit["commands"][family]
            expected_lanes = (f"{family}-lane-1", f"{family}-lane-2")
            require(set(command_entry["recordSha256ByLane"]) == set(expected_lanes), f"lane membership drift: {source}:{family}")
            closure_rows_for_evidence: bytes | None = None
            for lane in expected_lanes:
                record = REFERENCE_ROOT / lane / unit["commandRecordPath"]
                require(record.is_file() and not record.is_symlink(), f"missing command record {record}")
                verify_record_source(record, source)
                require(sha256_file(record) == command_entry["recordSha256ByLane"][lane], f"command record drift: {record}")
                saved_command, argv = first_saved_command(record)
                require(sha256_bytes(saved_command.encode("utf-8")) == command_entry["savedCommandSha256"], f"saved command drift: {record}")
                require(len(argv) == command_entry["argvCount"], f"argv count drift: {record}")
                require(argv_sha256(argv) == command_entry["argvSha256"], f"argv hash drift: {record}")
                require(argv[0] == ("gcc-14" if family == "gcc" else "clang-19"), f"compiler identity drift: {record}")

                closure_rows, closure_files, closure_bytes = dependency_closure(lane, source, unit["commandRecordPath"])
                closure = command_entry["inputClosure"]
                require(closure_files == closure["files"], f"input file count drift: {source}:{lane}")
                require(closure_bytes == closure["bytes"], f"input byte count drift: {source}:{lane}")
                require(sha256_bytes(closure_rows) == closure["sha256"], f"input closure drift: {source}:{lane}")
                if closure_rows_for_evidence is None:
                    closure_rows_for_evidence = closure_rows
                else:
                    require(closure_rows == closure_rows_for_evidence, f"same-family input closure differs: {source}:{family}")
            if evidence_inputs is not None:
                output = evidence_inputs / f"{unit['id']}.{family}.inputs.tsv"
                output.write_bytes(closure_rows_for_evidence or b"")

    return contract


def artifact(path: Path) -> dict[str, Any]:
    require(path.is_file(), f"expected artifact is absent: {path}")
    return {"path": path.name, "sha256": sha256_file(path), "bytes": path.stat().st_size}


def run_process(
    argv: list[str],
    stdout_path: Path,
    stderr_path: Path,
    timeout: int,
    cwd: Path = PROJECT_ROOT,
    env: dict[str, str] | None = None,
) -> tuple[int, bool]:
    with stdout_path.open("wb") as stdout, stderr_path.open("wb") as stderr:
        try:
            completed = subprocess.run(
                argv,
                cwd=cwd,
                env=env,
                stdout=stdout,
                stderr=stderr,
                timeout=timeout,
                check=False,
            )
            return completed.returncode, False
        except subprocess.TimeoutExpired:
            return 124, True


def translate_container_path(value: str, lane: str) -> str:
    if value.startswith("/work/src/"):
        return str(SOURCE_ROOT / value[len("/work/src/") :])
    if not value.startswith("/"):
        relative = value[2:] if value.startswith("./") else value
        translated = (REFERENCE_ROOT / lane / relative).resolve()
        lane_root = (REFERENCE_ROOT / lane).resolve()
        require(
            translated == lane_root or lane_root in translated.parents,
            f"relative Kbuild path escapes lane output: {value}",
        )
        return str(translated)
    return value


def raw_projection(evocc: Path, original: list[str], lane: str, source: str) -> tuple[list[str], list[str]]:
    projected = [str(evocc), "-E", "-P", "--target=aarch64-unknown-linux-gnu"]
    omitted: list[str] = []
    separate_value = {"-I", "-D", "-U", "-include", "-imacros", "-isystem", "-iquote"}
    index = 1
    while index < len(original):
        value = original[index]
        if value == f"/work/src/{source}":
            index += 1
            continue
        if value in separate_value:
            require(index + 1 < len(original), f"missing value after {value}")
            following = original[index + 1]
            if value in {"-I", "-include", "-imacros", "-isystem", "-iquote"}:
                following = translate_container_path(following, lane)
            projected.extend((value, following))
            index += 2
            continue
        if value.startswith("-I") and len(value) > 2:
            projected.append("-I" + translate_container_path(value[2:], lane))
        elif value.startswith(("-D", "-U")) and len(value) > 2:
            projected.append(value)
        elif value in {"-nostdinc", "-undef"} or value.startswith("-std="):
            projected.append(value)
        elif value.startswith("-fmacro-prefix-map="):
            projected.append(value.replace("/work/src/", str(SOURCE_ROOT) + "/"))
        else:
            omitted.append(value)
        index += 1
    projected.append(str(SOURCE_ROOT / source))
    return projected, omitted


def oracle_projection(original: list[str], compiler_path: str, output_path: str) -> list[str]:
    projected = [compiler_path, "-E", "-o", output_path]
    index = 1
    while index < len(original):
        value = original[index]
        if value.startswith("-Wp,-MMD,") or value.startswith("-Wp,-MD,"):
            index += 1
            continue
        if value == "-c":
            index += 1
            continue
        if value == "-o":
            require(index + 1 < len(original), "missing original -o argument")
            index += 2
            continue
        projected.append(value)
        index += 1
    return projected


def normalize_origin(path: str, family: str) -> str:
    if path.startswith("/work/src/"):
        return path[len("/work/src/") :]
    if path.startswith("/work/out/"):
        return f"@{family}-output/{path[len('/work/out/') :]}"
    if not path.startswith("/") and not path.startswith("<"):
        return f"@{family}-output/{path.removeprefix('./')}"
    return path


def sanitize_preprocessed(raw_path: Path, parser_path: Path, line_map_path: Path, family: str) -> list[dict[str, Any]]:
    current_path = "<unknown>"
    current_line = 1
    mappings: list[dict[str, Any]] = []
    output_lines: list[str] = []
    raw_text = raw_path.read_text(encoding="utf-8", errors="strict")
    for line in raw_text.splitlines(keepends=True):
        marker = LINE_MARKER_RE.match(line.rstrip("\r\n"))
        if marker:
            current_line = int(marker.group(1))
            current_path = bytes(marker.group(2), "utf-8").decode("unicode_escape")
            continue
        output_lines.append(line)
        mappings.append(
            {
                "parserLine": len(output_lines),
                "originPath": normalize_origin(current_path, family),
                "originLine": current_line,
            }
        )
        current_line += 1
    parser_path.write_text("".join(output_lines), encoding="utf-8")
    with line_map_path.open("w", encoding="utf-8") as output:
        for mapping in mappings:
            output.write(json.dumps(mapping, sort_keys=True, separators=(",", ":")) + "\n")
    return mappings


def raw_diagnostic_source_path(emitted_path: str) -> str:
    path = Path(emitted_path)
    require(path.is_absolute(), f"raw diagnostic path is not absolute: {emitted_path}")
    try:
        source_root = SOURCE_ROOT.resolve(strict=True)
        output_root = (REFERENCE_ROOT / "gcc-lane-1").resolve(strict=True)
        resolved = path.resolve(strict=True)
    except OSError as error:
        raise ContractError(f"raw diagnostic path cannot be resolved: {emitted_path}: {error}") from error
    require(resolved.is_file(), f"raw diagnostic path is not a file: {emitted_path}")
    if resolved != source_root and source_root in resolved.parents:
        return resolved.relative_to(source_root).as_posix()
    if resolved != output_root and output_root in resolved.parents:
        return f"@gcc-output/{resolved.relative_to(output_root).as_posix()}"
    raise ContractError(f"raw diagnostic path is outside frozen raw-input roots: {emitted_path}")


def verify_raw_source_diagnostic_identity(diagnostic: dict[str, Any]) -> None:
    emitted_path = diagnostic.get("emittedPath")
    if emitted_path is None:
        return
    expected_path = raw_diagnostic_source_path(emitted_path)
    require(
        diagnostic.get("sourcePath") == expected_path,
        "raw diagnostic source path does not match its emitted kernel path",
    )
    require(
        diagnostic.get("sourceSpan") == diagnostic.get("emittedSpan"),
        "raw diagnostic source span does not preserve its emitted span",
    )


def parse_diagnostic(
    stderr: str,
    default_source: str,
    mappings: list[dict[str, Any]] | None,
    *,
    raw_source: bool = False,
) -> dict[str, Any]:
    require(not raw_source or mappings is None, "raw diagnostics cannot use oracle line mappings")
    first_nonempty = next((line for line in stderr.splitlines() if line.strip()), "")
    diagnostic: dict[str, Any] = {
        "code": "DRIVER_NONZERO_EXIT",
        "message": first_nonempty or "nonzero exit without diagnostic",
        "emittedPath": None,
        "emittedSpan": None,
        "sourcePath": default_source,
        "sourceSpan": None,
    }
    match = DIAGNOSTIC_RE.match(first_nonempty)
    if match:
        line = int(match.group("line"))
        column = int(match.group("column"))
        diagnostic.update(
            {
                "code": match.group("code"),
                "message": match.group("message"),
                "emittedPath": match.group("path"),
                "emittedSpan": {"line": line, "column": column},
            }
        )
        if mappings is not None and 1 <= line <= len(mappings):
            mapped = mappings[line - 1]
            diagnostic["sourcePath"] = mapped["originPath"]
            diagnostic["sourceSpan"] = {"line": mapped["originLine"], "column": None}
            diagnostic["sourceMapping"] = "oracle-line-marker; column unavailable after macro expansion"
        else:
            if raw_source:
                diagnostic["sourcePath"] = raw_diagnostic_source_path(match.group("path"))
            diagnostic["sourceSpan"] = {"line": line, "column": column}
    elif "unsupported option" in first_nonempty:
        diagnostic["code"] = "DRIVER_UNSUPPORTED_OPTION"
    elif "panicked at" in stderr:
        diagnostic["code"] = "DRIVER_RUST_PANIC"
    return diagnostic


def failure_class(diagnostic: dict[str, Any], phase: str, timed_out: bool) -> str:
    if timed_out:
        if phase == "oracle-preprocessed-parse":
            diagnostic["code"] = "PARSE_TIMEOUT"
            classification = "parse"
        elif phase in {"raw-source-preprocess", "oracle-preprocess"}:
            diagnostic["code"] = "PREPROCESS_TIMEOUT"
            classification = "preprocess"
        else:
            diagnostic["code"] = "DRIVER_TIMEOUT"
            classification = "driver"
        diagnostic["message"] = "process exceeded the fixed step timeout"
        return classification
    code = diagnostic["code"]
    message = diagnostic["message"].lower()
    if "not_covered" in message or "not-covered" in message or code.startswith("NOT_COVERED"):
        return "out-of-Day5"
    if phase == "oracle-preprocess":
        return "preprocess" if not code.startswith("DRIVER_") else "driver"
    if code.startswith("DRIVER_"):
        return "driver"
    if code.startswith("E00"):
        return "lex"
    if code.startswith("E01"):
        return "preprocess"
    if code.startswith("E3") or "layout" in message or "bit-field" in message or "bitfield" in message:
        return "layout"
    if code.startswith("E1"):
        return "parse"
    if code.startswith("E2"):
        return "sema"
    return "driver"


def observed_result(
    unit: dict[str, Any],
    lane: str,
    phase: str,
    return_code: int,
    timed_out: bool,
    stdout_path: Path,
    stderr_path: Path,
    mappings: list[dict[str, Any]] | None = None,
    extra_artifacts: dict[str, Any] | None = None,
) -> dict[str, Any]:
    stdout_bytes = stdout_path.stat().st_size
    stderr_bytes = stderr_path.stat().st_size
    clean_success = return_code == 0 and not timed_out
    if phase == "raw-source-preprocess":
        clean_success = clean_success and stdout_bytes > 0 and stderr_bytes == 0
    elif phase == "oracle-preprocess":
        clean_success = clean_success and stdout_bytes == 0 and stderr_bytes == 0
    elif phase == "oracle-preprocessed-parse":
        clean_success = clean_success and stdout_bytes == 0 and stderr_bytes == 0
    result: dict[str, Any] = {
        "tuId": unit["id"],
        "sourcePath": unit["sourcePath"],
        "lane": lane,
        "phase": phase,
        "returnCode": return_code,
        "timedOut": timed_out,
        "artifacts": {
            "stdout": artifact(stdout_path),
            "stderr": artifact(stderr_path),
        },
    }
    if extra_artifacts:
        result["artifacts"].update(extra_artifacts)
    if clean_success:
        result.update({"outcome": "success", "classification": None, "diagnostic": None, "reproducer": None})
    else:
        stderr = stderr_path.read_text(encoding="utf-8", errors="replace")
        raw_source = phase == "raw-source-preprocess"
        diagnostic = parse_diagnostic(
            stderr,
            unit["sourcePath"],
            mappings,
            raw_source=raw_source,
        )
        if return_code == 0 and not timed_out:
            diagnostic.update(
                {
                    "code": "DRIVER_DIRTY_SUCCESS",
                    "message": (
                        "zero exit violated lane output contract: "
                        f"stdoutBytes={stdout_bytes} stderrBytes={stderr_bytes}"
                    ),
                    "emittedPath": None,
                    "emittedSpan": None,
                    "sourcePath": unit["sourcePath"],
                    "sourceSpan": None,
                }
            )
        if raw_source:
            verify_raw_source_diagnostic_identity(diagnostic)
        classification = failure_class(diagnostic, phase, timed_out)
        require(classification in FAILURE_CLASSES, f"invalid classification {classification}")
        result.update(
            {
                "outcome": "failure",
                "classification": classification,
                "diagnostic": diagnostic,
                "reproducer": {
                    "state": "PENDING_REDUCTION",
                    "sha256": None,
                    "acceptanceValid": False,
                },
            }
        )
    return result


def single_inspection(path: Path) -> dict[str, Any]:
    inspected = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(inspected, list):
        require(len(inspected) == 1, f"unexpected inspection result count in {path}")
        inspected = inspected[0]
    require(isinstance(inspected, dict), f"unexpected inspection shape in {path}")
    return inspected


def require_image_descriptor(inspected: dict[str, Any], expected: str) -> None:
    observed = "sha256:" + str(inspected.get("id", "")).removeprefix("sha256:")
    require(observed == expected, f"oracle image descriptor drift: expected={expected} observed={observed}")


def inspect_image(container_bin: str, reference: str, stdout_path: Path, stderr_path: Path) -> dict[str, Any]:
    rc, timed_out = run_process(
        [container_bin, "image", "inspect", reference],
        stdout_path,
        stderr_path,
        60,
    )
    require(not timed_out and rc == 0, f"cannot inspect oracle image reference {reference}")
    return single_inspection(stdout_path)


def prepare_oracle_alias(
    container_bin: str,
    build_env: dict[str, Any],
    evidence: Path,
    run_id: str,
) -> str:
    discovery = build_env["buildImage"]["sealedImageTag"]
    expected = build_env["buildImage"]["executionReference"]
    discovery_inspect = inspect_image(
        container_bin,
        discovery,
        evidence / "oracle-discovery.inspect.json",
        evidence / "oracle-discovery.inspect.stderr",
    )
    require_image_descriptor(discovery_inspect, expected)

    alias = f"c30-day5-oracle-{run_id.lower()}-{os.getpid()}:bound"
    tag_stdout = evidence / "oracle-alias-tag.stdout"
    tag_stderr = evidence / "oracle-alias-tag.stderr"
    rc, timed_out = run_process(
        [container_bin, "image", "tag", discovery, alias],
        tag_stdout,
        tag_stderr,
        60,
    )
    require(not timed_out and rc == 0, "cannot create run-unique oracle image alias")
    alias_inspect = inspect_image(
        container_bin,
        alias,
        evidence / "oracle-alias.inspect.json",
        evidence / "oracle-alias.inspect.stderr",
    )
    require_image_descriptor(alias_inspect, expected)
    binding = {
        "schemaVersion": 1,
        "directDigestAddressable": False,
        "reason": "Apple Container 1.2.2 treats the local image ID as a registry reference; replay uses a run-unique alias whose descriptor is checked before container creation and on each stopped container before and after execution",
        "discoveryReference": discovery,
        "runUniqueAlias": alias,
        "expectedDescriptor": expected,
    }
    (evidence / "oracle-binding.json").write_bytes(canonical_json(binding))
    return alias


def delete_oracle_alias(container_bin: str, alias: str, evidence: Path) -> None:
    rc, timed_out = run_process(
        [container_bin, "image", "delete", alias],
        evidence / "oracle-alias-delete.stdout",
        evidence / "oracle-alias-delete.stderr",
        60,
    )
    require(not timed_out and rc == 0, f"cannot delete run-unique oracle alias {alias}")


def require_container_binding(inspected: dict[str, Any], expected: str, alias: str) -> None:
    configuration = inspected.get("configuration", {})
    image = configuration.get("image", {})
    require(image.get("descriptor", {}).get("digest") == expected, "created container image descriptor drift")
    canonical_alias = alias if "/" in alias.split(":", 1)[0] else f"docker.io/library/{alias}"
    require(image.get("reference") == canonical_alias, "created container image alias drift")
    platform = configuration.get("platform", {})
    require(platform.get("os") == "linux" and platform.get("architecture") == "arm64", "created container platform drift")
    require(configuration.get("readOnly") is True, "created container root is not read-only")
    require(configuration.get("networks") == [], "created container unexpectedly has a network")
    require(configuration.get("capDrop") == ["ALL"], "created container capability drop drift")


def run_bound_container(
    container_bin: str,
    alias: str,
    expected_descriptor: str,
    name: str,
    lane: str,
    family_dir: Path,
    timeout: int,
) -> tuple[int, bool]:
    create_command = [
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
        "--volume",
        f"{SOURCE_ROOT}:/work/src:ro",
        "--volume",
        f"{REFERENCE_ROOT / lane}:/work/out:ro",
        "--volume",
        f"{family_dir}:/evidence",
        "-w",
        "/work/out",
        alias,
        "/bin/sh",
        "/evidence/replay.sh",
    ]
    created = False
    try:
        create_rc, create_timeout = run_process(
            create_command,
            family_dir / "container-create.stdout",
            family_dir / "container-create.stderr",
            60,
        )
        require(not create_timeout and create_rc == 0, "cannot create digest-bound oracle container")
        created = True

        inspect_rc, inspect_timeout = run_process(
            [container_bin, "inspect", name],
            family_dir / "container-before.inspect.json",
            family_dir / "container-before.inspect.stderr",
            60,
        )
        require(not inspect_timeout and inspect_rc == 0, "cannot inspect stopped oracle container")
        require_container_binding(
            single_inspection(family_dir / "container-before.inspect.json"),
            expected_descriptor,
            alias,
        )

        run_rc, run_timeout = run_process(
            [container_bin, "start", "--attach", name],
            family_dir / "container.stdout",
            family_dir / "container.stderr",
            timeout,
        )
        if run_timeout:
            subprocess.run(
                [container_bin, "stop", name],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                timeout=60,
                check=False,
            )
        post_rc, post_timeout = run_process(
            [container_bin, "inspect", name],
            family_dir / "container-after.inspect.json",
            family_dir / "container-after.inspect.stderr",
            60,
        )
        require(not post_timeout and post_rc == 0, "cannot inspect completed oracle container")
        require_container_binding(
            single_inspection(family_dir / "container-after.inspect.json"),
            expected_descriptor,
            alias,
        )
        return run_rc, run_timeout
    finally:
        if created:
            delete = subprocess.run(
                [container_bin, "delete", name],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=60,
                check=False,
            )
            require(delete.returncode == 0, f"cannot delete exact oracle container {name}")


def oracle_batches(
    contract: dict[str, Any],
    build_env: dict[str, Any],
    evidence: Path,
    container_bin: str,
    oracle_alias: str,
    timeout: int,
) -> dict[str, dict[str, tuple[int, Path, Path]]]:
    batches: dict[str, dict[str, tuple[int, Path, Path]]] = {}
    expected_descriptor = build_env["buildImage"]["executionReference"]
    for family in FAMILIES:
        lane = f"{family}-lane-1"
        family_dir = evidence / f"oracle-{family}"
        family_dir.mkdir()
        compiler = build_env["toolchain"][family]["binaryPathInImage"]
        script = family_dir / "replay.sh"
        script_lines = ["#!/bin/sh", "set +e", f"/usr/bin/sha256sum {shlex.quote(compiler)} > /evidence/toolchain.sha256"]
        for unit in contract["translationUnits"]:
            record = REFERENCE_ROOT / lane / unit["commandRecordPath"]
            _, original = first_saved_command(record)
            raw_output = f"/evidence/{unit['id']}.oracle.i"
            argv = oracle_projection(original, compiler, raw_output)
            script_lines.extend(
                (
                    f"{shlex.join(argv)} > /evidence/{unit['id']}.compiler.stdout 2> /evidence/{unit['id']}.compiler.stderr",
                    "rc=$?",
                    f"printf '%s\\n' \"$rc\" > /evidence/{unit['id']}.compiler.rc",
                )
            )
        script.write_text("\n".join(script_lines) + "\n", encoding="utf-8")
        name = f"c30-day5-recon-{family}-{os.getpid()}"
        rc, timed_out = run_bound_container(
            container_bin,
            oracle_alias,
            expected_descriptor,
            name,
            lane,
            family_dir,
            timeout * len(contract["translationUnits"]),
        )
        require(not timed_out and rc == 0, f"oracle {family} container batch failed")
        tool_hash_line = (family_dir / "toolchain.sha256").read_text(encoding="utf-8").split()
        require(tool_hash_line and tool_hash_line[0] == build_env["toolchain"][family]["binarySha256"], f"oracle {family} binary drift")

        family_results: dict[str, tuple[int, Path, Path]] = {}
        for unit in contract["translationUnits"]:
            prefix = family_dir / unit["id"]
            rc_path = Path(str(prefix) + ".compiler.rc")
            stdout_path = Path(str(prefix) + ".compiler.stdout")
            stderr_path = Path(str(prefix) + ".compiler.stderr")
            require(rc_path.is_file() and stdout_path.is_file() and stderr_path.is_file(), f"oracle {family} omitted {unit['id']}")
            try:
                compiler_rc = int(rc_path.read_text(encoding="utf-8").strip())
            except ValueError as error:
                raise ContractError(f"invalid oracle rc for {family}:{unit['id']}") from error
            family_results[unit["id"]] = (compiler_rc, stdout_path, stderr_path)
        batches[family] = family_results
    return batches


def subject_build_command(cargo: Path, profile: str) -> list[str]:
    require(profile == SUBJECT_BUILD_PROFILE, f"unsupported subject build profile: {profile}")
    return [
        str(cargo),
        "build",
        "--locked",
        "--offline",
        "--release",
        "-p",
        "evocc",
    ]


def subject_binary_path(target_root: Path, profile: str) -> Path:
    require(profile == SUBJECT_BUILD_PROFILE, f"unsupported subject build profile: {profile}")
    return (target_root / profile / "evocc").resolve()


def resolve_evocc(
    args: argparse.Namespace,
    evidence: Path,
    build_env: dict[str, Any],
    authority: dict[str, Path],
    build_env_snapshot: Path,
) -> tuple[Path, bytes]:
    build_inputs = evocc_build_input_closure()
    build_inputs_path = evidence / "evocc-build-inputs.tsv"
    build_inputs_path.write_bytes(build_inputs)

    build_environment, discarded_names = sanitized_cargo_environment(
        dict(os.environ),
        evidence,
        authority["rustc"],
        build_env,
    )
    target_root = Path(build_environment["CARGO_TARGET_DIR"])
    build_stdout = evidence / "cargo-build.stdout"
    build_stderr = evidence / "cargo-build.stderr"
    build_command = subject_build_command(authority["cargo"], args.subject_profile)
    host = build_env["toolchain"]["hostReplay"]
    runtime = build_env["containerRuntime"]
    receipt = {
        "schemaVersion": 1,
        "policy": "exact-pinned-tools-hermetic-environment-v1",
        "buildEnvironmentContract": artifact(build_env_snapshot),
        "tools": {
            "container": {
                "path": str(authority["container"]),
                "sha256": runtime["cliSha256"],
                "bytes": authority["container"].stat().st_size,
            },
            "cargo": {
                "path": str(authority["cargo"]),
                "sha256": host["cargo"]["sha256"],
                "bytes": authority["cargo"].stat().st_size,
                "version": host["cargo"]["version"],
            },
            "rustc": {
                "path": str(authority["rustc"]),
                "sha256": host["rustc"]["sha256"],
                "bytes": authority["rustc"].stat().st_size,
                "version": host["rustc"]["version"],
            },
        },
        "parentEnvironment": {
            "valuesInherited": False,
            "discardedBuildShapingVariableNames": discarded_names,
            "forbiddenOverrideVariableNames": [],
        },
        "effectiveEnvironment": dict(sorted(build_environment.items())),
        "subjectBuildProfile": args.subject_profile,
        "command": build_command,
        "buildInputs": artifact(build_inputs_path),
    }
    (evidence / "host-build-authority.json").write_bytes(canonical_json(receipt))
    rc, timed_out = run_process(
        build_command,
        build_stdout,
        build_stderr,
        args.timeout * 5,
        env=build_environment,
    )
    require(not timed_out and rc == 0, "cannot freshly build evocc for reconnaissance")
    binary = subject_binary_path(target_root, args.subject_profile)
    require(binary.is_file() and os.access(binary, os.X_OK), f"evocc binary is unavailable: {binary}")
    return binary, build_inputs


def validate_evocc_controls(binary: Path, evidence: Path, timeout: int) -> dict[str, Any]:
    controls = evidence / "evocc-controls"
    controls.mkdir(parents=True, exist_ok=False)
    positive = controls / "parse-positive.c"
    negative = controls / "parse-negative.c"
    positive.write_text("int control(void) { return 0; }\n", encoding="utf-8")
    negative.write_text("int control(void) { return ;\n", encoding="utf-8")
    common = [str(binary), "--parse-only", "--target=aarch64-unknown-linux-gnu"]

    positive_stdout = controls / "positive.stdout"
    positive_stderr = controls / "positive.stderr"
    positive_rc, positive_timeout = run_process(
        common + [str(positive)],
        positive_stdout,
        positive_stderr,
        timeout,
    )
    require(not positive_timeout, "evocc positive control timed out")
    require(positive_rc == 0, "evocc positive control was rejected")
    require(positive_stdout.stat().st_size == 0, "evocc positive parse control wrote stdout")
    require(positive_stderr.stat().st_size == 0, "evocc positive parse control wrote stderr")

    negative_stdout = controls / "negative.stdout"
    negative_stderr = controls / "negative.stderr"
    negative_rc, negative_timeout = run_process(
        common + [str(negative)],
        negative_stdout,
        negative_stderr,
        timeout,
    )
    require(not negative_timeout, "evocc negative control timed out")
    require(negative_rc != 0, "evocc negative control was falsely accepted")
    require(negative_stdout.stat().st_size == 0, "evocc negative parse control wrote stdout")
    negative_text = negative_stderr.read_text(encoding="utf-8", errors="strict")
    negative_diagnostic = parse_diagnostic(negative_text, str(negative), None)
    require(negative_diagnostic["code"].startswith("E1"), "evocc negative control lacks parser diagnostic")

    cpp_positive = controls / "cpp-positive.c"
    cpp_negative = controls / "cpp-negative.c"
    cpp_positive.write_text("#define DAY5_CONTROL 7\nDAY5_CONTROL\n", encoding="utf-8")
    cpp_negative.write_text('#include "missing-day5-control.h"\n', encoding="utf-8")
    cpp_common = [str(binary), "-E", "-P", "--target=aarch64-unknown-linux-gnu"]
    cpp_positive_stdout = controls / "cpp-positive.stdout"
    cpp_positive_stderr = controls / "cpp-positive.stderr"
    cpp_positive_rc, cpp_positive_timeout = run_process(
        cpp_common + [str(cpp_positive)],
        cpp_positive_stdout,
        cpp_positive_stderr,
        timeout,
    )
    require(not cpp_positive_timeout and cpp_positive_rc == 0, "evocc CPP positive control failed")
    require(cpp_positive_stdout.read_bytes() == b"7\n", "evocc CPP positive control output drift")
    require(cpp_positive_stderr.stat().st_size == 0, "evocc CPP positive control wrote stderr")

    cpp_negative_stdout = controls / "cpp-negative.stdout"
    cpp_negative_stderr = controls / "cpp-negative.stderr"
    cpp_negative_rc, cpp_negative_timeout = run_process(
        cpp_common + [str(cpp_negative)],
        cpp_negative_stdout,
        cpp_negative_stderr,
        timeout,
    )
    require(not cpp_negative_timeout and cpp_negative_rc != 0, "evocc CPP negative control was falsely accepted")
    require(cpp_negative_stdout.stat().st_size == 0, "evocc CPP negative control wrote stdout")
    cpp_negative_text = cpp_negative_stderr.read_text(encoding="utf-8", errors="strict")
    cpp_negative_diagnostic = parse_diagnostic(cpp_negative_text, str(cpp_negative), None)
    require(cpp_negative_diagnostic["code"] == "E0105", "evocc CPP negative control diagnostic drift")
    return {
        "status": "PASS",
        "parsePositive": {
            "returnCode": positive_rc,
            "source": artifact(positive),
            "stdout": artifact(positive_stdout),
            "stderr": artifact(positive_stderr),
        },
        "parseNegative": {
            "returnCode": negative_rc,
            "diagnosticCode": negative_diagnostic["code"],
            "source": artifact(negative),
            "stdout": artifact(negative_stdout),
            "stderr": artifact(negative_stderr),
        },
        "cppPositive": {
            "returnCode": cpp_positive_rc,
            "source": artifact(cpp_positive),
            "stdout": artifact(cpp_positive_stdout),
            "stderr": artifact(cpp_positive_stderr),
        },
        "cppNegative": {
            "returnCode": cpp_negative_rc,
            "diagnosticCode": cpp_negative_diagnostic["code"],
            "source": artifact(cpp_negative),
            "stdout": artifact(cpp_negative_stdout),
            "stderr": artifact(cpp_negative_stderr),
        },
    }


def parser_observation(
    family: str,
    unit: dict[str, Any],
    compiler_result: tuple[int, Path, Path],
    evidence: Path,
    evocc: Path,
    parser_timeout: int,
) -> dict[str, Any]:
    compiler_rc, compiler_stdout, compiler_stderr = compiler_result
    lane_name = f"{family}-preprocessed-parser"
    if (
        compiler_rc != 0
        or compiler_stdout.stat().st_size != 0
        or compiler_stderr.stat().st_size != 0
    ):
        return observed_result(
            unit,
            lane_name,
            "oracle-preprocess",
            compiler_rc,
            False,
            compiler_stdout,
            compiler_stderr,
        )

    family_dir = evidence / f"oracle-{family}"
    parser_dir = evidence / "parser"
    oracle_raw = family_dir / f"{unit['id']}.oracle.i"
    require(
        oracle_raw.is_file() and not oracle_raw.is_symlink() and oracle_raw.stat().st_size > 0,
        f"oracle omitted or emptied preprocessed file: {family}:{unit['id']}",
    )
    parser_input = parser_dir / f"{unit['id']}.{family}.c"
    line_map = parser_dir / f"{unit['id']}.{family}.linemap.jsonl"
    mappings = sanitize_preprocessed(oracle_raw, parser_input, line_map, family)
    parser_stdout = parser_dir / f"{unit['id']}.{family}.stdout"
    parser_stderr = parser_dir / f"{unit['id']}.{family}.stderr"
    parser_argv = [
        str(evocc),
        "--parse-only",
        "--target=aarch64-unknown-linux-gnu",
        str(parser_input),
    ]
    parser_rc, timed_out = run_process(
        parser_argv,
        parser_stdout,
        parser_stderr,
        parser_timeout,
    )
    return observed_result(
        unit,
        lane_name,
        "oracle-preprocessed-parse",
        parser_rc,
        timed_out,
        parser_stdout,
        parser_stderr,
        mappings=mappings,
        extra_artifacts={
            "oraclePreprocessed": artifact(oracle_raw),
            "parserInput": artifact(parser_input),
            "lineMap": artifact(line_map),
            "compilerStdout": artifact(compiler_stdout),
            "compilerStderr": artifact(compiler_stderr),
        },
    )


def write_result(output: Any, result: dict[str, Any]) -> None:
    output.write(json.dumps(result, sort_keys=True, separators=(",", ":")) + "\n")
    output.flush()


def verify_results_evidence(results: list[dict[str, Any]], path: Path) -> None:
    expected = b"".join(canonical_json(result) for result in results)
    observed = path.read_bytes()
    require(observed == expected, "results.jsonl bytes disagree with in-memory observations")
    try:
        replayed = [json.loads(line) for line in observed.decode("utf-8").splitlines()]
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ContractError(f"cannot replay results.jsonl: {error}") from error
    require(replayed == results, "results.jsonl semantic replay disagrees with observations")


def summarize(
    results: list[dict[str, Any]],
    contract: dict[str, Any],
    evidence: Path,
    evocc: Path,
    controls: dict[str, Any],
    parser_timeout: int,
    parser_jobs: int,
    subject_profile: str,
) -> dict[str, Any]:
    expected = len(contract["translationUnits"]) * 3
    require(len(results) == expected, f"missing observation records: expected {expected}, got {len(results)}")
    keys = {(result["tuId"], result["lane"]) for result in results}
    require(len(keys) == expected, "duplicate observation lane")
    for unit in contract["translationUnits"]:
        for lane in ("evocpp-raw", "gcc-preprocessed-parser", "clang-preprocessed-parser"):
            require((unit["id"], lane) in keys, f"silent lane omission: {unit['id']}:{lane}")
    failures = [result for result in results if result["outcome"] == "failure"]
    class_counts = {classification: 0 for classification in FAILURE_CLASSES}
    for result in failures:
        class_counts[result["classification"]] += 1
    pending = sum(1 for result in failures if result["reproducer"]["state"] != "VERIFIED")
    parser_results = [result for result in results if result["lane"].endswith("-preprocessed-parser")]
    clean_parser_successes = [result for result in parser_results if result["outcome"] == "success"]
    clean_both_oracles = 0
    for unit in contract["translationUnits"]:
        unit_results = [result for result in parser_results if result["tuId"] == unit["id"]]
        if len(unit_results) == 2 and all(result["outcome"] == "success" for result in unit_results):
            clean_both_oracles += 1
    blockers: list[str] = []
    if controls.get("status") != "PASS":
        blockers.append("mandatory evocc positive/negative controls did not pass")
    if pending:
        blockers.append(f"{pending} failed lanes still require verified reduced reproducers")
    if clean_both_oracles < 3:
        blockers.append(
            "fewer than 3 frozen TUs parse cleanly from both oracle preprocessors "
            f"({clean_both_oracles}/3)"
        )
    return {
        "schemaVersion": 1,
        "status": "OBSERVED",
        "acceptanceReady": not blockers,
        "acceptanceBlockers": blockers,
        "contractSha256": sha256_file(Path(contract["_contractPath"])),
        "evocc": {
            "path": str(evocc),
            "sha256": sha256_file(evocc),
            "freshPerRunCargoTarget": True,
            "buildProfile": subject_profile,
            "buildInputs": artifact(evidence / "evocc-build-inputs.tsv"),
            "controls": controls,
        },
        "parserBudget": {"timeoutSecondsPerTu": parser_timeout, "jobs": parser_jobs},
        "requiredRecords": expected,
        "observedRecords": len(results),
        "successes": len(results) - len(failures),
        "failures": len(failures),
        "cleanParserLaneSuccesses": len(clean_parser_successes),
        "cleanParserTusBothOracles": clean_both_oracles,
        "classificationCounts": class_counts,
        "pendingReductions": pending,
        "resultsJsonl": artifact(evidence / "results.jsonl"),
    }


def run_self_tests(contract_path: Path) -> None:
    contract = verify_contract(contract_path, None)
    with tempfile.TemporaryDirectory(prefix="c30-day5-recon-selftest-") as directory:
        temporary = Path(directory)
        true_rejected = False
        try:
            validate_evocc_controls(Path("/usr/bin/true"), temporary / "true-mutant", 5)
        except ContractError:
            true_rejected = True
        require(true_rejected, "/usr/bin/true mutant bypassed evocc controls")

        unit = contract["translationUnits"][0]
        original_record = REFERENCE_ROOT / "gcc-lane-1" / unit["commandRecordPath"]
        mutant_record = temporary / "source-declaration-mutant.o.cmd"
        original_text = original_record.read_text(encoding="utf-8")
        mutant_text, substitutions = re.subn(
            r"(?m)^source_[^\n]+\n",
            "",
            original_text,
            count=1,
        )
        require(substitutions == 1, "self-test could not construct source-declaration mutant")
        mutant_record.write_text(mutant_text, encoding="utf-8")
        declaration_rejected = False
        try:
            verify_record_source(mutant_record, unit["sourcePath"])
        except ContractError:
            declaration_rejected = True
        require(declaration_rejected, "missing source-declaration mutant bypassed verifier")

        def self_test_observation(
            name: str,
            phase: str,
            stderr: str,
            mappings: list[dict[str, Any]] | None = None,
        ) -> dict[str, Any]:
            stdout_path = temporary / f"{name}.stdout"
            stderr_path = temporary / f"{name}.stderr"
            stdout_path.write_bytes(b"")
            stderr_path.write_text(stderr, encoding="utf-8")
            return observed_result(
                unit,
                f"self-test-{name}",
                phase,
                1,
                False,
                stdout_path,
                stderr_path,
                mappings=mappings,
            )

        raw_header = SOURCE_ROOT / "include/linux/compiler_types.h"
        raw_header_result = self_test_observation(
            "raw-header",
            "raw-source-preprocess",
            f'{raw_header}:189:1: error[E0104]: error-directive: "Unknown compiler"\n',
        )
        raw_header_diagnostic = raw_header_result["diagnostic"]
        require(
            raw_header_diagnostic["sourcePath"] == "include/linux/compiler_types.h",
            "raw header diagnostic was misattributed to the root TU",
        )
        require(
            raw_header_diagnostic["sourceSpan"] == {"line": 189, "column": 1},
            "raw header diagnostic lost its exact source span",
        )

        generated_header = REFERENCE_ROOT / "gcc-lane-1/include/generated/autoconf.h"
        generated_header_result = self_test_observation(
            "raw-generated-header",
            "raw-source-preprocess",
            f"{generated_header}:23:7: error[E0104]: generated-header-control\n",
        )
        generated_header_diagnostic = generated_header_result["diagnostic"]
        require(
            generated_header_diagnostic["sourcePath"]
            == "@gcc-output/include/generated/autoconf.h",
            "raw generated-header diagnostic lost its stable output identity",
        )
        require(
            generated_header_diagnostic["sourceSpan"] == {"line": 23, "column": 7},
            "raw generated-header diagnostic lost its exact source span",
        )

        root_tu_mutant = dict(raw_header_diagnostic)
        root_tu_mutant["sourcePath"] = unit["sourcePath"]
        root_tu_mutant_rejected = False
        try:
            verify_raw_source_diagnostic_identity(root_tu_mutant)
        except ContractError:
            root_tu_mutant_rejected = True
        require(root_tu_mutant_rejected, "root-TU attribution mutant bypassed verifier")

        other_lane_header = REFERENCE_ROOT / "clang-lane-1/include/generated/autoconf.h"
        other_lane_rejected = False
        try:
            self_test_observation(
                "other-lane-header",
                "raw-source-preprocess",
                f"{other_lane_header}:23:7: error[E0104]: other-lane-control\n",
            )
        except ContractError:
            other_lane_rejected = True
        require(other_lane_rejected, "non-raw-lane generated header bypassed verifier")

        outside_source = temporary / "outside-source.c"
        outside_source.write_text("#error outside\n", encoding="utf-8")
        outside_path_rejected = False
        try:
            self_test_observation(
                "outside-source",
                "raw-source-preprocess",
                f"{outside_source}:1:1: error[E0104]: error-directive: outside\n",
            )
        except ContractError:
            outside_path_rejected = True
        require(outside_path_rejected, "outside-root raw diagnostic mutant bypassed verifier")

        parser_result = self_test_observation(
            "parser-line-marker",
            "oracle-preprocessed-parse",
            "/tmp/parser-input.c:7:13: error[E1001]: expected-declaration\n",
            [{"originPath": "include/linux/compiler.h", "originLine": 265}] * 7,
        )
        parser_diagnostic = parser_result["diagnostic"]
        require(
            parser_diagnostic["sourcePath"] == "include/linux/compiler.h"
            and parser_diagnostic["sourceSpan"] == {"line": 265, "column": None}
            and parser_diagnostic.get("sourceMapping")
            == "oracle-line-marker; column unavailable after macro expansion",
            "parser line-marker diagnostic mapping drifted",
        )

        build_env = load_json(BUILD_ENV_CONTRACT)
        expected = build_env["buildImage"]["executionReference"]
        image_swap_rejected = False
        try:
            require_image_descriptor({"id": "0" * 64}, expected)
        except ContractError:
            image_swap_rejected = True
        require(image_swap_rejected, "wrong-image-descriptor mutant bypassed verifier")

        fake_container = temporary / "container"
        fake_container.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        fake_container.chmod(0o755)
        runtime = build_env["containerRuntime"]
        for expected_path, mutant in (
            (runtime["cliPath"], "path"),
            (str(fake_container), "hash"),
        ):
            fake_container_rejected = False
            try:
                verify_pinned_executable(
                    fake_container,
                    expected_path,
                    runtime["cliSha256"],
                    "Apple Container CLI",
                )
            except ContractError:
                fake_container_rejected = True
            require(
                fake_container_rejected,
                f"fake container {mutant} mutant bypassed authority",
            )

        fake_cargo = temporary / "cargo"
        fake_cargo.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
        fake_cargo.chmod(0o755)
        cargo = build_env["toolchain"]["hostReplay"]["cargo"]
        for expected_path, mutant in (
            (cargo["path"], "path"),
            (str(fake_cargo), "hash"),
        ):
            fake_cargo_rejected = False
            try:
                verify_pinned_executable(
                    fake_cargo,
                    expected_path,
                    cargo["sha256"],
                    "host cargo",
                )
            except ContractError:
                fake_cargo_rejected = True
            require(fake_cargo_rejected, f"fake cargo {mutant} mutant bypassed authority")

        release_cargo = Path("/pinned/cargo")
        require(
            subject_build_command(release_cargo, SUBJECT_BUILD_PROFILE)
            == [
                str(release_cargo),
                "build",
                "--locked",
                "--offline",
                "--release",
                "-p",
                "evocc",
            ],
            "reconnaissance release build command drifted",
        )
        require(
            subject_binary_path(Path("/fresh-target"), SUBJECT_BUILD_PROFILE)
            == Path("/fresh-target/release/evocc"),
            "reconnaissance release binary path drifted",
        )

        for override in (
            {"RUSTC_WRAPPER": str(temporary / "wrapper")},
            {"RUSTFLAGS": "-C opt-level=3"},
        ):
            build_override_rejected = False
            try:
                require_no_build_overrides(override)
            except ContractError:
                build_override_rejected = True
            require(build_override_rejected, "Rust build override mutant bypassed authority")

        drift_source = temporary / "source.c"
        drift_source.write_text("int day5;\n", encoding="utf-8")
        closure_start = exact_file_closure({"source/source.c": drift_source})
        drift_source.write_text("int day5_drift;\n", encoding="utf-8")
        source_drift_rejected = False
        try:
            require_exact_closure(
                closure_start,
                exact_file_closure({"source/source.c": drift_source}),
                "self-test source input",
            )
        except ContractError:
            source_drift_rejected = True
        require(source_drift_rejected, "source-drift mutant bypassed closure replay")

    print(
        "DAY5_KERNEL_RECON_SELFTEST "
        "true_mutant=PASS source_declaration_mutant=PASS image_swap_mutant=PASS "
        "fake_container_path_hash_mutants=PASS fake_cargo_path_hash_mutants=PASS "
        "wrapper_rustflags_mutants=PASS source_drift_mutant=PASS "
        "raw_root_tu_attribution_mutant=PASS raw_outside_root_mutant=PASS "
        "raw_generated_output_control=PASS raw_other_lane_mutant=PASS "
        "parser_line_marker_control=PASS"
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--contract", type=Path, default=DEFAULT_CONTRACT)
    parser.add_argument("--verify-only", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--evidence-root", type=Path, default=DEFAULT_EVIDENCE_ROOT)
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("--parser-timeout", type=int, default=5)
    parser.add_argument("--parser-jobs", type=int, default=4)
    parser.add_argument(
        "--subject-profile",
        choices=(SUBJECT_BUILD_PROFILE,),
        default=SUBJECT_BUILD_PROFILE,
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        require(args.timeout > 0, "timeout must be positive")
        require(args.parser_timeout > 0, "parser timeout must be positive")
        require(args.parser_jobs > 0, "parser jobs must be positive")
        contract_path = args.contract.resolve()
        if args.self_test:
            run_self_tests(contract_path)
            return 0
        if args.verify_only:
            contract = verify_contract(contract_path, None)
            print(
                "DAY5_KERNEL_COHORT_VERIFY "
                f"pass={len(contract['translationUnits'])} fail=0 "
                f"contract_sha256={sha256_file(contract_path)}"
            )
            return 0

        run_id = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ") + f"-{os.getpid()}"
        evidence = args.evidence_root.resolve() / run_id
        evidence.mkdir(parents=True, exist_ok=False)
        runner_path = Path(__file__).resolve()
        runner_start = runner_path.read_bytes()
        runner_snapshot = evidence / "runner.py"
        runner_snapshot.write_bytes(runner_start)

        build_env_start = BUILD_ENV_CONTRACT.read_bytes()
        build_env_snapshot = evidence / "build-environment.json"
        build_env_snapshot.write_bytes(build_env_start)
        build_env = load_json(build_env_snapshot)
        authority = verify_execution_authority(build_env)
        container_bin = str(authority["container"])

        contract_start = contract_path.read_bytes()
        contract_snapshot = evidence / "kernel-cohort.json"
        contract_snapshot.write_bytes(contract_start)
        inputs_dir = evidence / "inputs"
        contract = verify_contract(contract_snapshot, inputs_dir)
        kernel_inputs_start = directory_file_closure(inputs_dir)
        kernel_inputs_start_path = evidence / "kernel-input-closure.start.tsv"
        kernel_inputs_start_path.write_bytes(kernel_inputs_start)
        contract["_contractPath"] = str(contract_snapshot)
        contract_start_sha256 = sha256_bytes(contract_start)
        (evidence / "contract.sha256").write_text(contract_start_sha256 + "\n", encoding="utf-8")
        run_config = {
            "schemaVersion": 1,
            "generalTimeoutSeconds": args.timeout,
            "parserTimeoutSecondsPerTu": args.parser_timeout,
            "parserJobs": args.parser_jobs,
            "subjectBuildProfile": args.subject_profile,
            "runner": {
                "path": "runner.py",
                "sha256": sha256_bytes(runner_start),
                "bytes": len(runner_start),
            },
            "buildEnvironmentContract": artifact(build_env_snapshot),
            "kernelCohortContract": artifact(contract_snapshot),
            "requiredObservationLanes": [
                "evocpp-raw",
                "gcc-preprocessed-parser",
                "clang-preprocessed-parser",
            ],
        }
        (evidence / "run-config.json").write_bytes(canonical_json(run_config))

        evocc, build_inputs_start = resolve_evocc(
            args,
            evidence,
            build_env,
            authority,
            build_env_snapshot,
        )
        evocc_start_sha256 = sha256_file(evocc)
        evocc_sha256_path = evidence / "evocc.sha256"
        evocc_sha256_path.write_text(evocc_start_sha256 + "\n", encoding="utf-8")
        controls = validate_evocc_controls(evocc, evidence, args.timeout)
        results: list[dict[str, Any]] = []
        results_path = evidence / "results.jsonl"
        raw_dir = evidence / "evocpp-raw"
        raw_dir.mkdir()

        with results_path.open("w", encoding="utf-8") as results_output:
            for unit in contract["translationUnits"]:
                _, original = first_saved_command(
                    REFERENCE_ROOT / "gcc-lane-1" / unit["commandRecordPath"]
                )
                projection, omitted = raw_projection(evocc, original, "gcc-lane-1", unit["sourcePath"])
                argv_path = raw_dir / f"{unit['id']}.argv.json"
                argv_record = {
                    "argv": projection,
                    "argvSha256": argv_sha256(projection),
                    "omittedOriginalArgv": omitted,
                    "omittedOriginalArgvSha256": argv_sha256(omitted),
                    "projectionPolicy": "preprocessing-affecting Kbuild options; omitted argv is recorded, never silent",
                }
                argv_path.write_bytes(canonical_json(argv_record))
                stdout_path = raw_dir / f"{unit['id']}.stdout"
                stderr_path = raw_dir / f"{unit['id']}.stderr"
                rc, timed_out = run_process(projection, stdout_path, stderr_path, args.timeout)
                result = observed_result(
                    unit,
                    "evocpp-raw",
                    "raw-source-preprocess",
                    rc,
                    timed_out,
                    stdout_path,
                    stderr_path,
                    extra_artifacts={"argv": artifact(argv_path)},
                )
                results.append(result)
                write_result(results_output, result)

            oracle_alias = prepare_oracle_alias(container_bin, build_env, evidence, run_id)
            try:
                batches = oracle_batches(
                    contract,
                    build_env,
                    evidence,
                    container_bin,
                    oracle_alias,
                    args.timeout,
                )
            finally:
                delete_oracle_alias(container_bin, oracle_alias, evidence)
            parser_dir = evidence / "parser"
            parser_dir.mkdir()
            scheduled: list[tuple[str, str, Any]] = []
            with ThreadPoolExecutor(max_workers=args.parser_jobs) as executor:
                for family in FAMILIES:
                    for unit in contract["translationUnits"]:
                        future = executor.submit(
                            parser_observation,
                            family,
                            unit,
                            batches[family][unit["id"]],
                            evidence,
                            evocc,
                            args.parser_timeout,
                        )
                        scheduled.append((family, unit["id"], future))
                for _family, _unit_id, future in scheduled:
                    result = future.result()
                    results.append(result)
                    write_result(results_output, result)

        verify_results_evidence(results, results_path)
        end_inputs_dir = evidence / "inputs-end"
        verify_contract(contract_snapshot, end_inputs_dir)
        kernel_inputs_end = directory_file_closure(end_inputs_dir)
        kernel_inputs_end_path = evidence / "kernel-input-closure.end.tsv"
        kernel_inputs_end_path.write_bytes(kernel_inputs_end)
        require_exact_closure(kernel_inputs_start, kernel_inputs_end, "kernel source/input")

        build_inputs_end = evocc_build_input_closure()
        build_inputs_end_path = evidence / "evocc-build-inputs.end.tsv"
        build_inputs_end_path.write_bytes(build_inputs_end)
        require_exact_closure(build_inputs_start, build_inputs_end, "evocc build input")

        require(
            BUILD_ENV_CONTRACT.read_bytes() == build_env_start,
            "build-environment contract changed during run",
        )
        require(
            build_env_snapshot.read_bytes() == build_env_start,
            "captured build-environment contract changed",
        )
        require(contract_snapshot.read_bytes() == contract_start, "captured cohort contract changed")
        require(
            kernel_inputs_start_path.read_bytes() == kernel_inputs_start,
            "captured kernel input closure changed",
        )
        require(
            (evidence / "evocc-build-inputs.tsv").read_bytes() == build_inputs_start,
            "captured evocc build input closure changed",
        )
        require(sha256_file(evocc) == evocc_start_sha256, "evocc binary changed during observation")
        require(
            evocc_sha256_path.read_bytes() == (evocc_start_sha256 + "\n").encode(),
            "captured evocc hash changed",
        )
        verify_results_evidence(results, results_path)
        verify_execution_authority(build_env)

        summary = summarize(
            results,
            contract,
            evidence,
            evocc,
            controls,
            args.parser_timeout,
            args.parser_jobs,
            args.subject_profile,
        )
        require(runner_path.read_bytes() == runner_start, "reconnaissance runner changed during run")
        require(
            runner_snapshot.read_bytes() == runner_start,
            "captured reconnaissance runner changed during run",
        )
        require(contract_path.read_bytes() == contract_start, "cohort contract changed during run")
        summary["runner"] = artifact(runner_snapshot)
        summary["runConfig"] = artifact(evidence / "run-config.json")
        summary["authority"] = {
            "buildEnvironmentContract": artifact(build_env_snapshot),
            "kernelCohortContract": artifact(contract_snapshot),
            "hostBuild": artifact(evidence / "host-build-authority.json"),
            "kernelInputClosureStart": artifact(kernel_inputs_start_path),
            "kernelInputClosureEnd": artifact(kernel_inputs_end_path),
            "evoccBuildInputClosureStart": artifact(evidence / "evocc-build-inputs.tsv"),
            "evoccBuildInputClosureEnd": artifact(build_inputs_end_path),
            "startEndClosuresEqual": True,
        }
        summary_path = evidence / "summary.json"
        summary_path.write_bytes(canonical_json(summary))
        print(
            "DAY5_KERNEL_RECON "
            f"observed={summary['observedRecords']}/{summary['requiredRecords']} "
            f"success={summary['successes']} failure={summary['failures']} "
            f"pending_reductions={summary['pendingReductions']} "
            f"evidence={evidence} summary_sha256={sha256_file(summary_path)}"
        )
        return 0
    except (ContractError, OSError, subprocess.SubprocessError, json.JSONDecodeError) as error:
        print(f"day5-kernel-reconnaissance: FAIL: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
