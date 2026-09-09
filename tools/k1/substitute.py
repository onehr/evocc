#!/usr/bin/env python3
"""Replace Kbuild objects with EvoCC-compiled ones and relink the kernel.

Mechanism: substitution by installed object files, not by a ``CC=`` wrapper.

Kbuild decides whether to rebuild an object from two things: the object being
newer than its prerequisites, and the recorded command line in the object's
``.o.cmd`` being unchanged (``if_changed``). Installing a fresh object and
leaving its ``.o.cmd`` untouched satisfies both, so ``make Image vmlinux``
consumes the installed object and relinks around it. This is verified rather
than assumed: every installed object's digest is recorded before the relink
and re-read afterwards, and a unit whose object Kbuild overwrote is reported
as ``rebuilt-by-kbuild`` instead of counted.

A ``CC=`` wrapper was rejected for two reasons. It would have to reproduce
Kbuild's own decision about which invocations are compilations, and it would
run EvoCC inside the sealed container, where the macOS binary does not exist;
the wrapper would therefore have to shuttle precomputed assembly in anyway,
which is what installing objects does directly and with fewer moving parts.

Per-unit outcomes are closed and mutually exclusive:

  substituted            EvoCC emitted assembly, GCC assembled it, symbol
                         parity with the reference object held, and the
                         object was installed
  refused                EvoCC declined the unit; the diagnostic codes are
                         recorded and the reference object is left in place
  timeout                EvoCC did not finish inside the limit
  assembler-refused      EvoCC emitted assembly the reference GCC would not
                         assemble
  symbol-drift           the assembled object does not define the same global
                         symbols as the reference object
  not-a-target-c-compile the record is a link, a host tool, or assembly

A refused unit is never quietly replaced by its GCC object: the GCC object is
what is already there, and the unit is simply not counted.

The front-end pass over every unit is the slow part of a survey and its answer
depends on three things only: the driver binary, the unit's source, and the
command the record dictates. Those three are hashed into a cache key, so a
re-run examines the units whose inputs changed and reuses the rest. A timeout
is never cached, because it is a fact about the machine rather than about the
unit, and an emitted answer is reused only while its assembly is still on
disk. ``--no-cache`` examines everything.
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
import shutil
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from cmdrecord import (  # noqa: E402
    Record,
    RecordError,
    assemble_argv,
    evocc_argv,
    is_target_c_compile,
    parse_record,
    record_path,
    source_path,
)

REPO_ROOT = Path(__file__).resolve().parents[2]


def default_evocc() -> Path:
    """The release driver, honoring a shared CARGO_TARGET_DIR."""
    if os.environ.get("K1_EVOCC"):
        return Path(os.environ["K1_EVOCC"])
    target = os.environ.get("CARGO_TARGET_DIR")
    if target:
        return Path(target) / "release/evocc"
    return REPO_ROOT / "target/release/evocc"
CONTAINER_BIN = "/usr/local/bin/container"
DIAGNOSTIC = re.compile(r"^.+:[0-9]+:[0-9]+: error\[(?P<code>[^]]+)\]: (?P<message>.*)$")
TARGET_COMPILER = "gcc-14"


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def diagnostic_codes(stderr: str) -> list[str]:
    codes: list[str] = []
    for line in stderr.splitlines():
        match = DIAGNOSTIC.match(line)
        if match and match.group("code") not in codes:
            codes.append(match.group("code"))
    return codes


#: Bumped when the shape of a cached answer changes, so an old cache is
#: ignored rather than misread.
CACHE_VERSION = 1

#: Outcomes of the front-end pass that are a function of the inputs alone and
#: may therefore be reused. A ``timeout`` is not: it says how loaded the
#: machine was, not what the compiler thinks of the unit.
CACHEABLE_OUTCOMES = frozenset({"emitted", "refused", "not-a-target-c-compile"})

#: How many units to examine between cache checkpoints. A survey of the whole
#: kernel takes half an hour, and an interrupted one used to be worth nothing;
#: checkpointing means a restart picks up where the last one stopped.
CACHE_CHECKPOINT = 100


def cache_key(unit: str, evocc_sha256: str, source_sha256: str, argv: list[str]) -> str:
    """The identity of one survey answer.

    Everything the front-end pass reads is in the key: which driver ran, which
    source it read, and the command it was given. Two runs that agree on all
    three must get the same answer, and a run that differs in any of them gets
    a miss rather than a stale row.
    """
    payload = json.dumps(
        {
            "version": CACHE_VERSION,
            "unit": unit,
            "evocc": evocc_sha256,
            "source": source_sha256,
            "argv": list(argv),
        },
        sort_keys=True,
        separators=(",", ":"),
    )
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def load_cache(path: Path) -> dict[str, dict]:
    """A previous run's answers, or nothing at all if they cannot be read."""
    if not path.is_file():
        return {}
    try:
        stored = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}
    if not isinstance(stored, dict) or stored.get("version") != CACHE_VERSION:
        return {}
    entries = stored.get("entries")
    return entries if isinstance(entries, dict) else {}


def save_cache(path: Path, entries: dict[str, dict]) -> None:
    """Replace the cache in one step.

    The survey checkpoints as it goes, so a write can be interrupted. Writing
    beside the cache and renaming means an interrupted run leaves the previous
    cache intact rather than a half-written file the next run must discard.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".new")
    temporary.write_text(
        json.dumps({"version": CACHE_VERSION, "entries": entries}, indent=1, sort_keys=True)
        + "\n",
        encoding="utf-8",
    )
    os.replace(temporary, path)


def assembly_path(asm_root: Path, unit: str) -> Path:
    return asm_root / f"{unit[:-2]}.s"


def reusable(entry: dict | None, unit: str, asm_root: Path) -> dict | None:
    """The cached answer for a unit, if it can still stand for a fresh run.

    An answer that says assembly was emitted is only good while that assembly
    is still on disk: the later steps assemble the file, not the record of it.
    """
    if not isinstance(entry, dict):
        return None
    outcome = entry.get("outcome")
    if outcome not in CACHEABLE_OUTCOMES:
        return None
    if outcome == "emitted":
        assembly = assembly_path(asm_root, unit)
        if not assembly.is_file() or entry.get("assemblySha256") != sha256_file(assembly):
            return None
    row = dict(entry)
    row["unit"] = unit
    row["fromCache"] = True
    return row


def target_c_records(pristine: Path) -> list[tuple[str, Record]]:
    """Every record in the reference tree that is a target C compilation."""
    found: list[tuple[str, Record]] = []
    for path in sorted(pristine.rglob("*.o.cmd")):
        try:
            record = parse_record(path)
        except RecordError:
            continue
        if is_target_c_compile(record):
            found.append((record.unit, record))
    return found


def read_allowlist(path: Path) -> list[str]:
    units: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if line and line not in units:
            units.append(line)
    return units


def emit_assembly(
    unit: str,
    record: Record,
    evocc: Path,
    src_root: Path,
    out_root: Path,
    asm_root: Path,
    timeout: int,
) -> dict:
    """Run EvoCC on one unit and record what happened."""
    row: dict = {"unit": unit, "record": str(record.path)}
    try:
        argv, omitted = evocc_argv(record, evocc, src_root, out_root)
    except RecordError as error:
        return {**row, "outcome": "not-a-target-c-compile", "detail": str(error)}
    row["omittedFlagCount"] = len(omitted)
    started = time.monotonic()
    try:
        completed = subprocess.run(
            argv, capture_output=True, text=True, timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired:
        return {**row, "outcome": "timeout", "seconds": timeout}
    row["seconds"] = round(time.monotonic() - started, 3)
    if completed.returncode != 0:
        return {
            **row,
            "outcome": "refused",
            "returnCode": completed.returncode,
            "diagnosticCodes": diagnostic_codes(completed.stderr),
            "firstDiagnostic": completed.stderr.strip().splitlines()[:1],
        }
    assembly = assembly_path(asm_root, unit)
    assembly.parent.mkdir(parents=True, exist_ok=True)
    assembly.write_text(completed.stdout, encoding="utf-8")
    return {
        **row,
        "outcome": "emitted",
        "assembly": str(assembly),
        "assemblyBytes": len(completed.stdout),
        "assemblySha256": sha256_file(assembly),
    }


def assemble_batch(rows: list[dict], records: dict[str, Record], asm_root: Path) -> None:
    """Assemble every emitted unit in one container run with the reference GCC."""
    emitted = [row for row in rows if row["outcome"] == "emitted"]
    if not emitted:
        return
    script_lines = ["#!/bin/sh", "cd /work/asm || exit 2", ""]
    for row in emitted:
        unit = row["unit"]
        record = records[unit]
        argv = assemble_argv(record, TARGET_COMPILER, f"{unit[:-2]}.s", unit)
        quoted = " ".join(_quote(argument) for argument in argv)
        script_lines.append(f"mkdir -p '{os.path.dirname(unit) or '.'}'")
        script_lines.append(
            f"if {quoted} 2>'{unit}.aserr'; then echo 'OK {unit}';"
            f" else echo 'FAIL {unit}'; fi"
        )
    (asm_root / "assemble.sh").write_text("\n".join(script_lines) + "\n", encoding="utf-8")
    (asm_root / "assemble.sh").chmod(0o755)

    completed = subprocess.run(
        [
            CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-v", f"{asm_root}:/work/asm", "evo-flight-lab:sealed", "/work/asm/assemble.sh",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    verdicts = {}
    for line in completed.stdout.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2 and parts[0] in {"OK", "FAIL"}:
            verdicts[parts[1]] = parts[0]
    for row in emitted:
        unit = row["unit"]
        verdict = verdicts.get(unit)
        if verdict != "OK":
            error_file = asm_root / f"{unit}.aserr"
            row["outcome"] = "assembler-refused"
            row["assemblerError"] = (
                error_file.read_text(encoding="utf-8", errors="replace").strip()[:600]
                if error_file.is_file()
                else f"no verdict from the container run: {completed.stderr.strip()[:300]}"
            )
            continue
        row["outcome"] = "assembled"
        row["object"] = str(asm_root / unit)
        row["objectSha256"] = sha256_file(asm_root / unit)


def _quote(argument: str) -> str:
    if re.fullmatch(r"[A-Za-z0-9_@%+=:,./-]+", argument):
        return argument
    return "'" + argument.replace("'", "'\\''") + "'"


def defined_symbols(objects: list[tuple[str, Path]], work: Path) -> dict[str, list[str]]:
    """The global symbols each object defines, read by the container's nm."""
    if not objects:
        return {}
    listing = work / "nm-inputs.txt"
    listing.write_text("\n".join(name for name, _ in objects) + "\n", encoding="utf-8")
    script = work / "symbols.sh"
    script.write_text(
        "#!/bin/sh\n"
        "cd /work/nm || exit 2\n"
        "while IFS= read -r name; do\n"
        '  echo "## $name"\n'
        '  nm --defined-only --extern-only "$name" 2>/dev/null | awk \'{print $2, $3}\'\n'
        "done < /work/nm/nm-inputs.txt\n",
        encoding="utf-8",
    )
    script.chmod(0o755)
    completed = subprocess.run(
        [
            CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-v", f"{work}:/work/nm", "evo-flight-lab:sealed", "/work/nm/symbols.sh",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    table: dict[str, list[str]] = {}
    current = None
    for line in completed.stdout.splitlines():
        if line.startswith("## "):
            current = line[3:]
            table[current] = []
        elif current is not None and line.strip():
            table[current].append(line.strip())
    return table


def check_symbol_parity(rows: list[dict], asm_root: Path, pristine: Path, work: Path) -> None:
    """Compare each assembled object's global symbols with the reference object."""
    assembled = [row for row in rows if row["outcome"] == "assembled"]
    if not assembled:
        return
    work.mkdir(parents=True, exist_ok=True)
    pairs: list[tuple[str, Path]] = []
    for row in assembled:
        unit = row["unit"]
        mine = work / "mine" / unit
        theirs = work / "theirs" / unit
        mine.parent.mkdir(parents=True, exist_ok=True)
        theirs.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(asm_root / unit, mine)
        shutil.copy2(pristine / unit, theirs)
        pairs.append((f"mine/{unit}", mine))
        pairs.append((f"theirs/{unit}", theirs))
    table = defined_symbols(pairs, work)
    for row in assembled:
        unit = row["unit"]
        mine_kinds = {name: kind for kind, name in
                      (entry.split(" ", 1) for entry in table.get(f"mine/{unit}", []))}
        their_kinds = {name: kind for kind, name in
                       (entry.split(" ", 1) for entry in table.get(f"theirs/{unit}", []))}
        mine = set(mine_kinds)
        theirs = set(their_kinds)
        missing = sorted(theirs - mine)
        extra = sorted(mine - theirs)
        row["definedSymbols"] = sorted(mine)
        row["definedSymbolKinds"] = dict(sorted(mine_kinds.items()))
        row["referenceSymbols"] = sorted(theirs)
        row["referenceSymbolKinds"] = dict(sorted(their_kinds.items()))
        row["missingSymbols"] = missing
        row["extraSymbols"] = extra


def install(rows: list[dict], asm_root: Path, out_root: Path, allow_drift: bool) -> None:
    """Install every object that passed, leaving its ``.o.cmd`` untouched."""
    for row in rows:
        if row["outcome"] != "assembled":
            continue
        unit = row["unit"]
        drifted = bool(row.get("missingSymbols") or row.get("extraSymbols"))
        if drifted and not allow_drift:
            row["outcome"] = "symbol-drift"
            continue
        destination = out_root / unit
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(asm_root / unit, destination)
        os.utime(destination, None)
        row["outcome"] = "substituted"
        row["installedSha256"] = row["objectSha256"]


def restore(units: list[str], pristine: Path, out_root: Path) -> list[str]:
    """Put the reference object back for each unit; report the ones restored."""
    restored = []
    for unit in units:
        source = pristine / unit
        if not source.is_file():
            continue
        destination = out_root / unit
        shutil.copyfile(source, destination)
        os.utime(destination, None)
        restored.append(unit)
    return restored


def state_path(k1_root: Path) -> Path:
    return k1_root / "substituted.json"


def load_state(k1_root: Path) -> list[str]:
    path = state_path(k1_root)
    if not path.is_file():
        return []
    try:
        return list(json.loads(path.read_text(encoding="utf-8")))
    except (OSError, ValueError):
        return []


def save_state(k1_root: Path, units: list[str]) -> None:
    state_path(k1_root).write_text(json.dumps(sorted(units), indent=2) + "\n", encoding="utf-8")


def relink(src_root: Path, out_root: Path, log: Path, jobs: int) -> int:
    """Relink Image and vmlinux with the reference make invocation."""
    contract = json.loads((REPO_ROOT / "contracts/build-environment.json").read_text())
    reproducibility = contract["reproducibility"]
    image = contract["buildImage"]["sealedImageTag"]
    script = (
        "set -eu\n"
        "umask 022\n"
        f"export KBUILD_BUILD_TIMESTAMP='{reproducibility['KBUILD_BUILD_TIMESTAMP']}' "
        f"SOURCE_DATE_EPOCH='{reproducibility['SOURCE_DATE_EPOCH']}' "
        f"KBUILD_BUILD_USER='{reproducibility['KBUILD_BUILD_USER']}' "
        f"KBUILD_BUILD_HOST='{reproducibility['KBUILD_BUILD_HOST']}'\n"
        f"make -C /work/src O=/work/out ARCH=arm64 -j{jobs} CC=gcc-14 Image vmlinux\n"
    )
    completed = subprocess.run(
        [
            CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-c", "10", "-m", "24G",
            "-v", f"{src_root}:/work/src:ro",
            "-v", f"{out_root}:/work/out",
            "-w", "/work/out", image, "sh", "-c", script,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    log.write_text(completed.stdout + "\n--- stderr ---\n" + completed.stderr, encoding="utf-8")
    return completed.returncode


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--allowlist", type=Path, help="file of unit paths, one per line")
    parser.add_argument("--all", action="store_true", help="every target C compilation")
    parser.add_argument("--k1-root", type=Path, default=REPO_ROOT / ".build/k1")
    parser.add_argument("--src-root", type=Path, default=REPO_ROOT / ".build/linux-src")
    parser.add_argument("--evocc", type=Path, default=default_evocc())
    parser.add_argument("--evidence-root", type=Path, default=REPO_ROOT / ".evidence/k1")
    parser.add_argument("--run-id", default=None)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--relink-jobs", type=int, default=10)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--no-install", action="store_true", help="survey only; change nothing")
    parser.add_argument("--no-relink", action="store_true")
    parser.add_argument("--allow-symbol-drift", action="store_true")
    parser.add_argument(
        "--cache",
        type=Path,
        default=None,
        help="survey cache file (default <k1-root>/survey-cache.json)",
    )
    parser.add_argument(
        "--no-cache", action="store_true", help="examine every unit even if unchanged"
    )
    args = parser.parse_args()

    if not args.allowlist and not args.all:
        parser.error("one of --allowlist or --all is required")
    pristine = args.k1_root / "pristine"
    out_root = args.k1_root / "out"
    if not pristine.is_dir() or not out_root.is_dir():
        print(
            "k1: run tools/k1/reproduce-reference.sh first; the working copy is missing",
            file=sys.stderr,
        )
        return 2
    if not args.evocc.is_file():
        print(f"k1: EvoCC binary is missing: {args.evocc}", file=sys.stderr)
        return 2

    run_id = args.run_id or dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.evidence_root / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    if args.all:
        wanted = [unit for unit, _ in target_c_records(pristine)]
    else:
        wanted = read_allowlist(args.allowlist)

    records: dict[str, Record] = {}
    rows: list[dict] = []
    pending: list[str] = []
    for unit in wanted:
        path = record_path(pristine, unit)
        if not path.is_file():
            rows.append({"unit": unit, "outcome": "not-a-target-c-compile",
                         "detail": f"no command record at {path}"})
            continue
        try:
            record = parse_record(path)
        except RecordError as error:
            rows.append({"unit": unit, "outcome": "not-a-target-c-compile", "detail": str(error)})
            continue
        if not is_target_c_compile(record):
            rows.append({"unit": unit, "outcome": "not-a-target-c-compile",
                         "detail": f"tool={record.tool} source={record.source_token}"})
            continue
        records[unit] = record
        pending.append(unit)

    asm_root = args.k1_root / "evocc-out"
    asm_root.mkdir(parents=True, exist_ok=True)

    cache_path = args.cache or (args.k1_root / "survey-cache.json")
    cache = {} if args.no_cache else load_cache(cache_path)
    evocc_sha256 = sha256_file(args.evocc)
    keys: dict[str, str] = {}
    reused: list[dict] = []
    fresh: list[str] = []
    for unit in pending:
        if args.no_cache:
            fresh.append(unit)
            continue
        try:
            argv, _ = evocc_argv(records[unit], args.evocc, args.src_root, pristine)
            source_sha256 = sha256_file(source_path(records[unit], args.src_root, pristine))
        except (RecordError, OSError):
            fresh.append(unit)
            continue
        key = cache_key(unit, evocc_sha256, source_sha256, argv)
        keys[unit] = key
        answer = reusable(cache.get(key), unit, asm_root)
        if answer is None:
            fresh.append(unit)
        else:
            reused.append(answer)

    if reused:
        print(
            f"k1: survey cache: {len(reused)} units unchanged since the last survey,"
            f" {len(fresh)} to examine",
            flush=True,
        )
    print(f"k1: EvoCC over {len(fresh)} units", flush=True)
    produced: list[dict] = []
    since_checkpoint = 0
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for row in pool.map(
            lambda unit: emit_assembly(
                unit, records[unit], args.evocc, args.src_root, pristine,
                asm_root, args.timeout,
            ),
            fresh,
        ):
            produced.append(row)
            if args.no_cache:
                continue
            key = keys.get(row["unit"])
            if key is not None and row.get("outcome") in CACHEABLE_OUTCOMES:
                cache[key] = row
            since_checkpoint += 1
            if since_checkpoint >= CACHE_CHECKPOINT:
                save_cache(cache_path, cache)
                since_checkpoint = 0
                print(f"k1: {len(produced)} of {len(fresh)} examined", flush=True)
    if not args.no_cache:
        save_cache(cache_path, cache)
    rows.extend(produced)
    rows.extend(reused)

    print("k1: assembling with the reference GCC in the sealed container", flush=True)
    assemble_batch(rows, records, asm_root)
    print("k1: comparing defined symbols against the reference objects", flush=True)
    check_symbol_parity(rows, asm_root, pristine, args.k1_root / "symbols")

    previously = load_state(args.k1_root)
    substituted: list[str] = []
    if not args.no_install:
        restored = restore(previously, pristine, out_root)
        print(f"k1: restored {len(restored)} previously substituted objects", flush=True)
        install(rows, asm_root, out_root, args.allow_symbol_drift)
        substituted = [row["unit"] for row in rows if row["outcome"] == "substituted"]
        save_state(args.k1_root, substituted)
        print(f"k1: installed {len(substituted)} EvoCC objects", flush=True)

    relink_code = None
    if not args.no_install and not args.no_relink:
        print("k1: relinking Image and vmlinux", flush=True)
        relink_code = relink(args.src_root, out_root, run_dir / "relink.log", args.relink_jobs)
        print(f"k1: relink exit {relink_code}", flush=True)
        for row in rows:
            if row["outcome"] != "substituted":
                continue
            after = sha256_file(out_root / row["unit"])
            row["objectSha256AfterRelink"] = after
            if after != row["installedSha256"]:
                row["outcome"] = "rebuilt-by-kbuild"
        substituted = [row["unit"] for row in rows if row["outcome"] == "substituted"]
        save_state(args.k1_root, substituted)

    counts: dict[str, int] = {}
    for row in rows:
        counts[row["outcome"]] = counts.get(row["outcome"], 0) + 1
    refusal_codes: dict[str, int] = {}
    for row in rows:
        for code in row.get("diagnosticCodes", []):
            refusal_codes[code] = refusal_codes.get(code, 0) + 1

    with (run_dir / "units.jsonl").open("w", encoding="utf-8") as handle:
        for row in sorted(rows, key=lambda item: item["unit"]):
            handle.write(json.dumps(row, sort_keys=True) + "\n")
    summary = {
        "runId": run_id,
        "requestedUnits": len(wanted),
        "evoccSha256": evocc_sha256,
        "cacheHits": len(reused),
        "counts": counts,
        "refusalCodeCounts": dict(sorted(refusal_codes.items())),
        "substitutedUnits": sorted(substituted),
        "relinkExitCode": relink_code,
        "installed": not args.no_install,
        "allowSymbolDrift": args.allow_symbol_drift,
        "imageSha256": sha256_file(out_root / "arch/arm64/boot/Image")
        if (out_root / "arch/arm64/boot/Image").is_file()
        else None,
        "vmlinuxSha256": sha256_file(out_root / "vmlinux")
        if (out_root / "vmlinux").is_file()
        else None,
    }
    (run_dir / "substitution.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps({k: v for k, v in summary.items() if k != "substitutedUnits"}, indent=2))
    print(f"evidence: {run_dir}")
    if relink_code not in (None, 0):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
