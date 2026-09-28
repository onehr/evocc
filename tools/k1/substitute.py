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

Frontend reuse binds the driver, command and source/reference input trees,
including headers and generated files. Object reuse instead binds the emitted
assembly, assembler command and immutable image: a new compiler may produce
the same object. External assembly includes are always reassembled. Timeouts
are never cached. ``--no-cache`` examines everything.
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
import shutil
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))

from cmdrecord import (  # noqa: E402
    Record,
    RecordError,
    SAVED_COMMAND,
    assemble_argv,
    evocc_argv,
    is_target_c_compile,
    omitted_option_class,
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
CACHE_VERSION = 2

#: Outcomes of the front-end pass that are a function of the inputs alone and
#: may therefore be reused. A ``timeout`` is not: it says how loaded the
#: machine was, not what the compiler thinks of the unit.
CACHEABLE_OUTCOMES = frozenset({"emitted", "refused", "not-a-target-c-compile"})

#: How many units to examine between cache checkpoints. A survey of the whole
#: kernel takes half an hour, and an interrupted one used to be worth nothing;
#: checkpointing means a restart picks up where the last one stopped.
CACHE_CHECKPOINT = 100


def cache_key(unit: str, evocc_sha256: str, source_sha256: str, argv: list[str],
              inputs_sha256: str = "") -> str:
    """The identity of one survey answer, including its input tree snapshot."""
    payload = json.dumps(
        {
            "version": CACHE_VERSION,
            "unit": unit,
            "evocc": evocc_sha256,
            "source": source_sha256,
            "inputs": inputs_sha256,
            "argv": list(argv),
        },
        sort_keys=True,
        separators=(",", ":"),
    )
    return hashlib.sha256(payload.encode("utf-8")).hexdigest()


def input_snapshot(src_root: Path, pristine: Path, *, content_only: bool = False,
                   toolchain_root: Path | None = None) -> str:
    """Hash all read-only input trees once, including exported system headers.

    This deliberately over-invalidates instead of guessing dependencies from
    the reference compiler's .cmd file, which may not match EvoCC's includes.
    No copies or persistent per-file manifests are made.
    """
    digest = hashlib.sha256()
    # Match the driver's i64 SOURCE_DATE_EPOCH parse. Only an unfrozen date
    # changes at midnight; an overnight run with a valid epoch keeps exactly
    # the same preprocessing input. Malformed values still fall back to UTC.
    epoch = os.environ.get("SOURCE_DATE_EPOCH")
    frozen_date = (epoch is not None and re.fullmatch(r'[+-]?[0-9]+', epoch) is not None
                   and -(1 << 63) <= int(epoch) < (1 << 63))
    date = None if frozen_date else dt.datetime.now(dt.timezone.utc).date().isoformat()
    digest.update(json.dumps([epoch, date]).encode())
    roots = (src_root.resolve(), pristine.resolve())
    if toolchain_root is not None:
        roots += (toolchain_root.resolve(),)
    def walk_error(error):
        raise error

    for root in roots:
        for parent, directories, files in os.walk(root, onerror=walk_error):
            directories.sort()
            for name in sorted(directories + files):
                path = Path(parent) / name
                relative = path.relative_to(root).as_posix()
                if path.is_symlink():
                    target = os.readlink(path)
                    resolved = path.resolve()
                    if target != "/work/src" and not any(
                        resolved == base or base in resolved.parents for base in roots
                    ):
                        raise ValueError(f"input symlink leaves the measured trees: {path}")
                    item = [str(root), relative, "symlink", target]
                elif path.is_file():
                    stat = path.stat()
                    item = [str(root), relative]
                    if not content_only:
                        item.extend([stat.st_mode, stat.st_mtime_ns])
                    item.append(sha256_file(path))
                else:
                    item = [str(root), relative, "directory"]
                digest.update(json.dumps(item, separators=(",", ":")).encode())
                digest.update(b"\n")
    return digest.hexdigest()


def sealed_image() -> str:
    image = json.loads((REPO_ROOT / "contracts/build-environment.json").read_text())["buildImage"]["executionReference"]
    if not re.fullmatch(r"sha256:[0-9a-f]{64}", image):
        raise ValueError("assembly reuse requires an immutable toolchain image")
    return image


def verify_image(expected: str) -> None:
    # The local runtime takes a name, not a bare digest. Verify its resolution
    # around execution; the cache key is the immutable descriptor, never the tag.
    result = subprocess.run([CONTAINER_BIN, "image", "inspect", "evo-flight-lab:sealed"],
                            capture_output=True, text=True, check=True)
    actual = json.loads(result.stdout)[0]["configuration"]["descriptor"]["digest"]
    if actual != expected:
        raise ValueError("installed assembler image differs from the sealed toolchain")


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
    # Kbuild leaves old generated module metadata behind when a configuration
    # changes a module to built-in. Counting that .mod.o command as current
    # target-C would inflate the denominator and could feed an obsolete source
    # to the compiler. Reject the dirty inventory; do not silently omit or
    # delete it. The operator must reconcile/archive it after Kbuild finishes.
    metadata = [unit for unit, record in found
                if unit.endswith('.mod.o') and record.source_token.endswith('.mod.c')]
    if metadata:
        order = pristine / 'modules.order'
        if not order.is_file():
            raise ValueError('generated module metadata requires current modules.order')
        modules = order.read_text().splitlines()
        if (len(set(modules)) != len(modules)
                or any(Path(name).is_absolute() or '..' in Path(name).parts
                       or Path(name).suffix != '.o' for name in modules)):
            raise ValueError('unsafe or duplicate module inventory in modules.order')
        expected = {name[:-2] + '.mod.o' for name in modules}
        stale = sorted(set(metadata) - expected)
        if stale:
            raise ValueError('stale module metadata outside current modules.order: '
                             + ', '.join(stale))
    return found


def read_allowlist(path: Path) -> list[str]:
    units: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if line and line not in units:
            units.append(line)
    return units


def resume_prepared_rows(run: Path, records: dict[str, Record], args,
                         cache: dict, inputs: str) -> tuple[list[dict], dict]:
    """Recover only a complete first pass; revalidate prepared-input consumers.

    The normal cache binds source, all inputs, command and compiler. The
    producer graph may have replaced installed first-pass objects with GCC
    output. Those bytes receive no EvoCC credit: every recovered row is pending
    until the complete prepared-input pass validates and installs its object.
    The separate prepared cache validates completed batches; uncheckpointed
    partial outputs are not evidence. Never modify the earlier inventory.
    """
    inventory = run / 'units.jsonl'
    rows = [json.loads(line) for line in inventory.read_text().splitlines()]
    if (len(rows) != len(records) or not records
            or {row['unit'] for row in rows} != set(records)):
        raise ValueError('prepared resume requires the complete original denominator')
    pristine, output = args.k1_root / 'pristine', args.k1_root / 'out'
    compiler = sha256_file(args.evocc)
    recovered = []
    for row in rows:
        unit = row['unit']
        record = records[unit]
        argv, _ = evocc_argv(record, args.evocc, args.src_root, pristine,
                             toolchain_root=args.toolchain_root)
        key = cache_key(unit, compiler,
                        sha256_file(source_path(record, args.src_root, pristine)), argv, inputs)
        entry = cache.get(key, {})
        if (row.get('outcome') != 'assembled' or entry.get('outcome') != 'emitted'
                or entry.get('unit') != unit
                or row.get('record') != str(record.path)
                or not row.get('assemblySha256')
                or entry.get('assemblySha256') != row['assemblySha256']
                or not (output / unit).is_file()):
            raise ValueError(f'prepared resume input changed or working object missing: {unit}')
        pending = dict(row, outcome='prepared-pending',
                       objectSha256BeforeResume=sha256_file(output / unit))
        pending.pop('installedSha256', None)
        recovered.append(pending)
    return recovered, {'run': str(run), 'inventorySha256': sha256_file(inventory),
                       'firstPassUnits': len(rows), 'preparedCompilationsReused': 0,
                       'policy': 'verify every prepared-input compilation; repeat fixed-point check, link and boot'}


def emit_assembly(
    unit: str,
    record: Record,
    evocc: Path,
    src_root: Path,
    out_root: Path,
    asm_root: Path,
    timeout: int,
    toolchain_root: Path | None = None,
) -> dict:
    """Run EvoCC on one unit and record what happened."""
    row: dict = {"unit": unit, "record": str(record.path)}
    try:
        argv, omitted = evocc_argv(record, evocc, src_root, out_root,
                                   toolchain_root=toolchain_root)
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


def assemble_batch(
    rows: list[dict], records: dict[str, Record], asm_root: Path, pristine: Path,
    *, use_cache: bool = True,
) -> None:
    """Reuse exact objects; assemble misses in one immutable container."""
    emitted = [row for row in rows if row["outcome"] == "emitted"]
    if not emitted:
        return
    image = sealed_image()
    cache_path = asm_root / "assembly-cache.json"
    cache = load_cache(cache_path) if use_cache else {}
    keys = {}
    fresh = []
    for row in emitted:
        unit = row["unit"]
        assembly = assembly_path(asm_root, unit)
        argv = assemble_argv(records[unit], TARGET_COMPILER, f"/work/asm/{unit[:-2]}.s", f"/work/asm/{unit}")
        # Includes/incbin may depend on files outside this text. They are cheap
        # exceptions: execute them afresh instead of inventing an asm parser.
        if not re.search(rb"\.(?:include|incbin)\b", assembly.read_bytes()):
            keys[unit] = cache_key(unit, 'assembler-verdict-v1:' + image, sha256_file(assembly), argv)
        previous = cache.get(keys.get(unit))
        obj = asm_root / unit
        if (isinstance(previous, dict) and obj.is_file() and obj.stat().st_size > 0
                and previous.get("objectSha256") == sha256_file(obj)):
            row.update(outcome="assembled", object=str(obj),
                       objectSha256=previous["objectSha256"], assemblyFromCache=True,
                       assemblerExitCode=0)
        else:
            row["assemblyFromCache"] = False
            fresh.append(row)
    print(f"k1: object cache: {len(emitted) - len(fresh)} reused, {len(fresh)} to assemble", flush=True)
    if not fresh:
        return
    # Inline .incbin paths are relative to the original Kbuild output tree.
    script_lines = ["#!/bin/sh", "cd /work/build || exit 2", ""]
    for row in fresh:
        unit = row["unit"]
        record = records[unit]
        argv = assemble_argv(record, TARGET_COMPILER, f"/work/asm/{unit[:-2]}.s", f"/work/asm/{unit}")
        quoted = " ".join(_quote(argument) for argument in argv)
        script_lines.append(f"mkdir -p '/work/asm/{os.path.dirname(unit) or '.'}'")
        script_lines.append(
            f"if {quoted} 2>'/work/asm/{unit}.aserr'; then echo 'OK {unit}';"
            f" else echo 'FAIL {unit}'; fi"
        )
    (asm_root / "assemble.sh").write_text("\n".join(script_lines) + "\n", encoding="utf-8")
    (asm_root / "assemble.sh").chmod(0o755)

    verify_image(image)
    completed = subprocess.run(
        [
            CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-v", f"{asm_root}:/work/asm",
            "-v", f"{pristine}:/work/build:ro",
            "evo-flight-lab:sealed", "/work/asm/assemble.sh",
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    verify_image(image)
    verdicts = {}
    duplicates = set()
    for line in completed.stdout.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2 and parts[0] in {"OK", "FAIL"}:
            if parts[1] in verdicts:
                duplicates.add(parts[1])
            verdicts[parts[1]] = parts[0]
    requested = {row['unit'] for row in fresh}
    protocol_error = None
    if completed.returncode:
        protocol_error = f'assembler container exited {completed.returncode}: {completed.stderr.strip()[:300]}'
    elif duplicates or set(verdicts) != requested:
        protocol_error = ('invalid assembler verdict population: '
                          f'duplicate={sorted(duplicates)} '
                          f'missing={sorted(requested - set(verdicts))} '
                          f'unexpected={sorted(set(verdicts) - requested)}')
    for row in fresh:
        unit = row["unit"]
        verdict = verdicts.get(unit)
        obj = asm_root / unit
        row['assemblerExitCode'] = completed.returncode
        if protocol_error or verdict != "OK" or not obj.is_file() or obj.stat().st_size == 0:
            error_file = asm_root / f"{unit}.aserr"
            row["outcome"] = "assembler-refused"
            diagnostic = (
                error_file.read_text(encoding="utf-8", errors="replace").strip()[:600]
                if error_file.is_file()
                else ''
            )
            row["assemblerError"] = (protocol_error or diagnostic or
                                     f"assembler did not produce a successful object: {unit}")[:600]
            continue
        row["outcome"] = "assembled"
        row["object"] = str(asm_root / unit)
        row["objectSha256"] = sha256_file(asm_root / unit)
        if unit in keys and use_cache:
            cache[keys[unit]] = {"objectSha256": row["objectSha256"]}
    if use_cache:
        save_cache(cache_path, cache)


def _quote(argument: str) -> str:
    if re.fullmatch(r"[A-Za-z0-9_@%+=:,./-]+", argument):
        return argument
    return "'" + argument.replace("'", "'\\''") + "'"


def release_successful_assembly(rows: list[dict], asm_root: Path) -> None:
    """Keep verified objects and hashes, not rebuildable successful assembly.

    Failed assembly and symbol mismatches remain available for diagnosis.
    Missing assembly deliberately invalidates the normal emission cache.
    """
    for row in rows:
        if (row['outcome'] != 'assembled' or row.get('missingSymbols')
                or row.get('extraSymbols')):
            continue
        assembly = assembly_path(asm_root, row['unit'])
        obj = asm_root / row['unit']
        if (not obj.is_file() or sha256_file(obj) != row.get('objectSha256')
                or not assembly.is_file()
                or sha256_file(assembly) != row.get('assemblySha256')):
            raise ValueError(f"cannot release unverified assembly: {row['unit']}")
        assembly.unlink()
        row['assemblyRetained'] = False


def defined_symbols(objects: list[tuple[str, Path]], work: Path) -> dict[str, list[str]]:
    """The global symbols each object defines, read by the container's nm."""
    if not objects:
        return {}
    listing = work / "nm-inputs.txt"
    listing.write_text("\n".join(name for name, _ in objects) + "\n", encoding="utf-8")
    script = work / "symbols.sh"
    script.write_text(
        "#!/bin/sh\nset -eu\n"
        "cd /work/nm || exit 2\n"
        "while IFS= read -r name; do\n"
        '  symbols=$(nm --defined-only --extern-only "$name") || exit 2\n'
        '  echo "## $name"\n'
        '  printf "%s\\n" "$symbols" | awk \'NF {print $2, $3}\'\n'
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
    if completed.returncode:
        raise RuntimeError(f"symbol reader failed ({completed.returncode}): {completed.stderr.strip()[:600]}")
    table: dict[str, list[str]] = {}
    current = None
    for line in completed.stdout.splitlines():
        if line.startswith("## "):
            current = line[3:]
            table[current] = []
        elif current is not None and line.strip():
            table[current].append(line.strip())
    if set(table) != {name for name, _ in objects}:
        raise RuntimeError("symbol reader returned an incomplete object inventory")
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
    # These copies exist solely to mount both object sets for one nm command.
    # The result above retains the complete symbol tables; no later stage
    # consumes the copies, and large configurations must not retain two more
    # object trees just to preserve a comparison that is already recorded.
    for path in {path for _, path in pairs}:
        path.unlink()


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
        same = destination.is_file() and sha256_file(destination) == row["objectSha256"]
        # A genuinely repeated compilation consumed the prepared headers even
        # when its output is byte-identical. Do not make Kbuild run GCC merely
        # because the retained object predates those headers. Ordinary cache
        # reuse still leaves the object untouched.
        if not same or row.get("recompiledAfterPrepare"):
            shutil.copyfile(asm_root / unit, destination)
            os.utime(destination, None)
        row["installUnchanged"] = same
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


def complete_coverage(rows: list[dict], wanted: list[str], outcome: str) -> bool:
    """One successful, unchanged object for every original target-C record."""
    if not wanted or len(rows) != len(wanted) or len(set(wanted)) != len(wanted):
        return False
    if {row["unit"] for row in rows} != set(wanted):
        return False
    return all(row.get("outcome") == outcome and (
        outcome != "substituted" or (
            row.get("installedSha256")
            and row.get("objectSha256AfterRelink") == row["installedSha256"]
    )) for row in rows)


def module_link_plan(pristine: Path, out_root: Path, installed: list[dict]) -> dict:
    """Original Kbuild links, gated by transitive installed-object provenance.

    A composite object still on disk is not evidence that its C inputs were
    compiled by EvoCC. Walk its original response file down to compilation
    leaves, and never make a link eligible through an unaudited C object.
    """
    rows = {row['unit']: row for row in installed}
    if len(rows) != len(installed):
        raise RecordError('duplicate installed object identity')
    result = {'modules': [], 'blocked': {}, 'links': [], 'cInputs': {},
              'assemblyInputs': {}, 'linkInputs': {}}
    completed, visiting = {}, set()

    def path(root, unit):
        relative = Path(unit)
        if relative.is_absolute() or '..' in relative.parts or not relative.parts:
            raise RecordError(f'unsafe module input {unit}')
        return root / relative

    def unchanged(unit):
        reference = path(pristine, unit)
        actual = path(out_root, unit)
        if not actual.is_file() or sha256_file(reference) != sha256_file(actual):
            raise RecordError(f'original module link input changed: {unit}')
        result['linkInputs'][unit] = sha256_file(actual)
        return reference

    def visit(unit):
        if unit in completed:
            return completed[unit]
        if unit in visiting:
            raise RecordError(f'cyclic module link: {unit}')
        visiting.add(unit)
        record = record_path(pristine, unit)
        lines = record.read_text().splitlines()
        match = SAVED_COMMAND.fullmatch(lines[0]) if lines else None
        if not match or match[1] != unit:
            raise RecordError(f'invalid module command record: {record}')
        argv = shlex.split(match[2])
        if not argv:
            raise RecordError(f'empty module command record: {record}')
        errors = []
        if Path(argv[0]).name == 'ld':
            if argv.count('-o') != 1 or argv[argv.index('-o') + 1] != unit:
                raise RecordError(f'module link output mismatch: {record}')
            unchanged(str(record.relative_to(pristine)))
            inputs = []
            for argument in argv[1:]:
                if argument == unit:
                    continue
                if argument.startswith('@'):
                    inputs.extend(shlex.split(unchanged(argument[1:]).read_text()))
                elif argument.endswith('.o'):
                    inputs.append(argument)
            if '-T' in argv:
                unchanged(argv[argv.index('-T') + 1])
            if not inputs:
                raise RecordError(f'module link has no object inputs: {record}')
            for child in inputs:
                path(out_root, child)
                errors.extend(visit(child))
            if not errors:
                result['links'].append({'unit': unit, 'argv': argv, 'inputs': inputs})
        else:
            compilation = parse_record(record)
            actual = path(out_root, unit)
            digest = sha256_file(actual) if actual.is_file() else None
            if is_target_c_compile(compilation):
                row = rows.get(unit, {})
                if (row.get('outcome') != 'substituted' or not digest
                        or digest != row.get('installedSha256')):
                    errors.append(f'{unit}: missing or changed EvoCC object')
                else:
                    result['cInputs'][unit] = digest
            elif compilation.tool == TARGET_COMPILER and compilation.source_token.endswith('.S'):
                if not digest or digest != sha256_file(path(pristine, unit)):
                    errors.append(f'{unit}: missing or changed reference assembly')
                else:
                    result['assemblyInputs'][unit] = digest
            else:
                raise RecordError(f'unsupported module leaf: {record}')
        visiting.remove(unit)
        completed[unit] = sorted(set(errors))
        return completed[unit]

    for item in unchanged('modules.order').read_text().splitlines():
        unit = str(Path(item).with_suffix('.ko'))
        path(out_root, unit)
        if errors := visit(unit):
            result['blocked'][unit] = errors
        else:
            result['modules'].append(unit)
    return result


def relink_modules(src_root: Path, pristine: Path, out_root: Path,
                   installed: list[dict], run_dir: Path, jobs: int,
                   *, finalize: bool = True) -> dict:
    """Replay original module links without allowing Kbuild to compile C.

    Final links alone omit the subsequent BTF recipe from Makefile.modfinal.
    Evaluate its flags with the original Makefile.btf, then run both pahole
    and resolve_btfids against this composition's vmlinux. Never copy the
    reference module's BTF or treat an old .ko as proof of a new link.
    """
    plan = module_link_plan(pristine, out_root, installed)
    if plan['blocked']:
        raise ValueError(f"module links have unaudited inputs: {sorted(plan['blocked'])}")
    if jobs < 1:
        raise ValueError('module link jobs must be positive')
    if not finalize:
        # modpost consumes these aggregate .o files, not their individual C
        # leaves. Refresh them before generating imported-symbol versions.
        plan = {**plan, 'links': [link for link in plan['links']
                                 if not link['unit'].endswith('.ko')]}
    for link in plan['links']:
        if not (out_root / link['unit']).resolve().is_relative_to(out_root.resolve()):
            raise ValueError(f"module link output escapes its build: {link['unit']}")
    protected = {**plan['cInputs'], **plan['assemblyInputs'], **plan['linkInputs'],
                 '.config': sha256_file(out_root / '.config')}
    if finalize:
        protected['vmlinux'] = sha256_file(out_root / 'vmlinux')
    def unchanged():
        return all((out_root / name).is_file() and sha256_file(out_root / name) == digest
                   for name, digest in protected.items())
    if not unchanged():
        raise ValueError('module inputs changed before linking')
    prefix = 'module-link' if finalize else 'module-input-link'
    path = run_dir / (prefix + '-plan.json')
    path.write_text(json.dumps({**plan, 'protectedInputs': protected, 'jobs': jobs,
                               'finalize': finalize}, indent=2) + '\n')
    image = sealed_image()
    verify_image(image)
    worker = r'''
import hashlib, json, os, shlex, subprocess, sys
from pathlib import Path
plan = json.loads(Path(sys.argv[1]).read_text())
def audit():
    for name, expected in plan['protectedInputs'].items():
        digest = hashlib.sha256()
        with open(name, 'rb') as source:
            for part in iter(lambda: source.read(1 << 20), b''):
                digest.update(part)
        if digest.hexdigest() != expected:
            raise ValueError('module input changed: ' + name)
audit()
btf = plan['finalize'] and 'CONFIG_DEBUG_INFO_BTF_MODULES=y' in Path('.config').read_text().splitlines()
flags = []
if btf:
    flags = shlex.split(subprocess.check_output([
        'make', '-s', '-j' + str(plan['jobs']), '-f', 'include/config/auto.conf',
        '-f', '/work/src/scripts/Kbuild.include', '-f', '/work/src/scripts/Makefile.btf',
        '--eval=print-pahole: ; @echo $(PAHOLE_FLAGS) $(MODULE_PAHOLE_FLAGS)',
        'print-pahole'], text=True))
env = {**os.environ, 'LLVM_OBJCOPY': 'objcopy'}
for number, link in enumerate(plan['links'], 1):
    subprocess.run(link['argv'], env=env, check=True)
    if btf and link['unit'].endswith('.ko'):
        subprocess.run(['pahole', '-J', *flags, '--btf_base', 'vmlinux', link['unit']],
                       env=env, check=True)
        subprocess.run(['./tools/bpf/resolve_btfids/resolve_btfids', '-b', 'vmlinux',
                        link['unit']], check=True)
    print('module link', number, 'of', len(plan['links']), link['unit'], flush=True)
audit()
'''
    done = subprocess.run([
        CONTAINER_BIN, 'run', '--rm', '--platform', 'linux/arm64', '--network', 'none',
        '--cpus', str(jobs), '--memory', f'{max(4, 2 * jobs)}G',
        '-v', f'{src_root}:/work/src:ro', '-v', f'{out_root}:/work/out',
        '-v', f'{run_dir}:/work/evidence:ro', '-w', '/work/out',
        'evo-flight-lab:sealed', 'python3', '-c', worker,
        '/work/evidence/' + path.name], capture_output=True, text=True, check=False)
    (run_dir / (prefix + '.log')).write_text(done.stdout + '\n--- stderr ---\n' + done.stderr)
    verify_image(image)
    inputs_stable = unchanged()
    success = done.returncode == 0 and inputs_stable
    result = {'status': 'PASS' if success else 'INCOMPLETE', 'linkExitCode': done.returncode,
              'inputHashesUnchanged': inputs_stable, 'assemblerImage': image,
              'planSha256': sha256_file(path), 'targetCInputs': len(plan['cInputs']),
              'referenceAssemblyInputs': len(plan['assemblyInputs']),
              'btfBaseSha256': protected.get('vmlinux'), 'configSha256': protected['.config'],
              'modules': [{'unit': name, 'sha256': sha256_file(out_root / name)}
                          for name in plan['modules']] if success and finalize else [],
              'scope': ('Module link and BTF only; signing, installation and runtime are separate'
                        if finalize else 'Module aggregate inputs for modpost; no final modules or BTF')}
    (run_dir / (prefix + '.json')).write_text(json.dumps(result, indent=2) + '\n')
    return result


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


def relink(src_root: Path, out_root: Path, log: Path, jobs: int,
           targets: tuple[str, ...] = ("Image", "vmlinux")) -> int:
    """Run the reference make invocation, including its preparation target."""
    contract = json.loads((REPO_ROOT / "contracts/build-environment.json").read_text())
    reproducibility = contract["reproducibility"]
    image = contract["buildImage"]["sealedImageTag"]
    version_header = out_root.parent / "pristine/include/generated/utsversion.h"
    version = re.search(r'^#define UTS_VERSION "#([0-9]+) ', version_header.read_text(), re.M)
    if version is None:
        raise ValueError("reference UTS_VERSION has no build number")
    script = (
        "set -eu\n"
        "umask 022\n"
        f"export KBUILD_BUILD_TIMESTAMP='{reproducibility['KBUILD_BUILD_TIMESTAMP']}' "
        f"SOURCE_DATE_EPOCH='{reproducibility['SOURCE_DATE_EPOCH']}' "
        f"KBUILD_BUILD_USER='{reproducibility['KBUILD_BUILD_USER']}' "
        f"KBUILD_BUILD_HOST='{reproducibility['KBUILD_BUILD_HOST']}' "
        f"KBUILD_BUILD_VERSION='{version[1]}'\n"
        f"make -C /work/src O=/work/out ARCH=arm64 -j{jobs} CC=gcc-14 "
        + ('KBUILD_MODULES=1 ' if ('modpost' in targets or 'modules.order' in targets)
           and (out_root / 'modules.order').is_file()
           and (out_root / 'modules.order').read_text().strip() else '')
        + " ".join(_quote(target) for target in targets) + "\n"
    )
    if "prepare" in targets:
        # link-vmlinux normally creates the final version header too late for
        # substituted consumers. Run its original Kbuild header recipes now;
        # no C object is compiled by this header-only invocation.
        script += (
            "make -C /work/out -f /work/src/scripts/Makefile.build "
            "srctree=/work/src srcroot=/work/src objtree=. ARCH=arm64 SRCARCH=arm64 "
            "CONFIG_SHELL=/bin/sh obj=init include/generated/utsversion.h init/utsversion-tmp.h\n"
        )
    completed = subprocess.run(
        [
            CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
            "-c", str(jobs), "-m", f"{max(8, min(24, 4 * jobs))}G",
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


def recompile_after_prepare(rows: list[dict], records: dict[str, Record], args,
                            asm_root: Path, pristine: Path, out_root: Path,
                            run_dir: Path) -> dict:
    """Compile against the headers produced by this composition's vDSO.

    Every selected object must consume the actual generated inputs, not an
    assumed GCC dependency subset. Completed batches can survive interruption
    only with the same complete input snapshot, compiler, commands and objects.
    A second prepare must still reach a fixed point before the final link.
    """
    toolchain_root = getattr(args, 'toolchain_root', None)
    compiler = sha256_file(args.evocc)
    image = sealed_image()
    checkpoint_path = args.k1_root / 'prepared-cache.json'
    checkpoint = {} if args.no_cache else load_cache(checkpoint_path)
    state = checkpoint.get('prepared', {})
    implementation = sha256_file(Path(__file__))
    before = None
    if (getattr(args, 'resume_prepared_run', None) and isinstance(state, dict)
            and state.get('compiler') == compiler and state.get('image') == image
            and state.get('implementation') == implementation
            and isinstance(state.get('log'), str) and Path(state['log']).is_file()
            and sha256_file(Path(state['log'])) == state.get('logSha256')):
        before = input_snapshot(args.src_root, out_root, toolchain_root=toolchain_root)
    resumed = before is not None and before == state.get('inputs')
    result = {'prepareExitCode': 0, 'preparedCacheHits': 0}
    entries = checkpoint.get('objects', {}) if resumed else {}
    if not isinstance(entries, dict):
        entries = {}
    if resumed:
        result['prepareReusedFrom'] = state['log']
    else:
        log = run_dir / 'prepare.log'
        # `prepare` alone does not run per-directory host generators (ASN.1,
        # CRC/RAID tables, logos, ...). Materialize the original producer graph
        # before freezing inputs. These preparatory artifacts are not credited
        # to EvoCC: every selected target-C object is recompiled below, and the
        # existing post-link hash audit still rejects any later GCC replacement.
        targets = ('prepare', 'vmlinux_o')
        module_order = out_root / 'modules.order'
        if module_order.is_file() and module_order.read_text().strip():
            targets += ('modules.order',)
        result['prepareExitCode'] = relink(args.src_root, out_root, log,
                                          args.relink_jobs, targets)
        if result['prepareExitCode'] != 0:
            return result
        before = input_snapshot(args.src_root, out_root, toolchain_root=toolchain_root)
        state = {'compiler': compiler, 'image': image, 'implementation': implementation,
                 'inputs': before, 'log': str(log.resolve()), 'logSha256': sha256_file(log)}
    selected = [row for row in rows if row["outcome"] in ("substituted", "prepared-pending")]
    keys = {}
    for old in selected:
        unit = old['unit']
        argv, _ = evocc_argv(records[unit], args.evocc, args.src_root, out_root,
                             toolchain_root=toolchain_root)
        assembler = assemble_argv(records[unit], TARGET_COMPILER,
                                  f'/work/asm/{unit[:-2]}.s', f'/work/asm/{unit}')
        assembler_identity = hashlib.sha256(json.dumps(assembler).encode()).hexdigest()
        keys[unit] = cache_key(unit, compiler, 'prepared-v1:' + assembler_identity, argv, before)

    def compile_one(old):
        unit = old['unit']
        cached = entries.get(keys[unit])
        obj = asm_root / unit
        if (isinstance(cached, dict) and cached.get('unit') == unit
                and cached.get('outcome') == 'assembled'
                and cached.get('preparedReusable') is True
                and not cached.get('missingSymbols') and not cached.get('extraSymbols')
                and obj.is_file() and sha256_file(obj) == cached.get('objectSha256')):
            return dict(cached, preparedCacheHit=True, object=str(obj),
                        record=str(records[unit].path))
        row = emit_assembly(old["unit"], records[old["unit"]], args.evocc,
                            args.src_root, out_root, asm_root, args.timeout, toolchain_root)
        row["compileInputRoot"] = str(out_root)
        row["compileInputSnapshotSha256"] = before
        row['preparedCacheHit'] = False
        if row['outcome'] == 'emitted':
            row['preparedReusable'] = not re.search(
                rb'\.(?:include|incbin)\b', assembly_path(asm_root, unit).read_bytes())
        return row
    compiled = []
    size = 64 if getattr(args, 'low_disk', False) else max(1, len(selected))
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for start in range(0, len(selected), size):
            batch = list(pool.map(compile_one, selected[start:start + size]))
            assemble_batch(batch, records, asm_root, out_root, use_cache=not args.no_cache)
            check_symbol_parity(batch, asm_root, pristine, args.k1_root / "symbols")
            if getattr(args, 'low_disk', False):
                release_successful_assembly([row for row in batch if not row['preparedCacheHit']], asm_root)
            compiled.extend(batch)
            result['preparedCacheHits'] += sum(row['preparedCacheHit'] for row in batch)
            for row in batch:
                key = keys[row['unit']]
                entries.pop(key, None)
                if (row['outcome'] == 'assembled' and row.get('preparedReusable')
                        and not row.get('missingSymbols') and not row.get('extraSymbols')):
                    entries[key] = dict(row)
            if not args.no_cache:
                save_cache(checkpoint_path, {'prepared': state, 'objects': entries})
            progress = {'state': 'CHECKPOINT', 'examinedUnits': len(compiled),
                        'pid': os.getpid(),
                        'observedAtUTC': dt.datetime.now(dt.timezone.utc).isoformat(),
                        'requestedUnits': len(selected), 'preparedCacheHits': result['preparedCacheHits'],
                        'compilerSha256': compiler, 'inputSnapshotSha256': before}
            save_cache(run_dir / 'prepared-progress.json', progress)
            print(f"k1: prepared {len(compiled)} of {len(selected)} examined; "
                  f"{result['preparedCacheHits']} resumed compilations", flush=True)
    if before != input_snapshot(args.src_root, out_root, toolchain_root=toolchain_root) or compiler != sha256_file(args.evocc):
        if not args.no_cache:
            save_cache(checkpoint_path, {})
        raise ValueError("prepared inputs changed while compiling dependent objects")
    for row in compiled:
        # The vDSO already ran through preparation: leaving identical objects
        # untouched prevents unnecessary regeneration of vdso-offsets.h.
        if not row["unit"].startswith("arch/arm64/kernel/vdso/"):
            row["recompiledAfterPrepare"] = True
    install(compiled, asm_root, out_root, args.allow_symbol_drift)
    failed = [row["unit"] for row in compiled if row["outcome"] != "substituted"]
    if not getattr(args, "require_all", False):
        restore(failed, pristine, out_root)
    replacement = {row["unit"]: row for row in compiled}
    rows[:] = [replacement.get(row["unit"], row) for row in rows]
    result["recompiledUnits"] = len(compiled)
    result["failedUnits"] = failed
    result["compileInputSnapshotSha256"] = before
    if failed:
        return result
    # Kbuild's GEN Makefile rewrites identical bytes on every prepare. The
    # fixed point concerns consumed content, not that housekeeping timestamp;
    # normal survey caching remains sensitive to metadata as before.
    stable = input_snapshot(args.src_root, out_root, content_only=True, toolchain_root=toolchain_root)
    result["verifyPrepareExitCode"] = relink(args.src_root, out_root,
        run_dir / "prepare-verify.log", args.relink_jobs, ("prepare",))
    result["preparedInputsStable"] = stable == input_snapshot(args.src_root, out_root, content_only=True, toolchain_root=toolchain_root)
    return result


def recompile_export_table(rows: list[dict], records: dict[str, Record], args,
                           asm_root: Path, out_root: Path, run_dir: Path) -> dict:
    """Compile modpost's export table and module descriptors from this link.

    These are original denominator members, not host-tool exemptions. The
    historical receipt field is exportTable; sources records its full set.
    """
    units = sorted(row['unit'] for row in rows if row['unit'] in records
                   and (row['unit'] == '.vmlinux.export.o'
                        or (row['unit'].endswith('.mod.o')
                            and records[row['unit']].source_token.endswith('.mod.c'))))
    if not units:
        return {"required": False, "ready": True}
    result = {"required": True, "ready": False}
    if any(unit.endswith('.mod.o') for unit in units):
        result['moduleInputs'] = relink_modules(args.src_root, args.k1_root / 'pristine',
                                                out_root, rows, run_dir, args.relink_jobs,
                                                finalize=False)
        if result['moduleInputs']['status'] != 'PASS':
            return result
    result['producerExitCode'] = relink(args.src_root, out_root, run_dir / "modpost.log",
                                       args.relink_jobs, ("modpost",))
    if result["producerExitCode"] != 0:
        return result
    sources = {unit: source_path(records[unit], args.src_root, out_root) for unit in units}
    result['sources'] = {unit: {'path': str(source), 'sha256': sha256_file(source)}
                         for unit, source in sources.items()}
    if '.vmlinux.export.o' in sources:
        export = result['sources']['.vmlinux.export.o']
        result.update(source=export['path'], sourceSha256=export['sha256'])
    compiler = sha256_file(args.evocc)
    def unchanged():
        return (compiler == sha256_file(args.evocc)
                and all(source.is_file() and sha256_file(source) == result['sources'][unit]['sha256']
                        for unit, source in sources.items()))
    def compile_one(unit):
        row = emit_assembly(unit, records[unit], args.evocc, args.src_root,
                            out_root, asm_root, args.timeout, getattr(args, 'toolchain_root', None))
        row.update(compileInputRoot=str(out_root),
                   compileSourceSha256=result['sources'][unit]['sha256'],
                   recompiledAfterPrepare=True, recompiledAfterModpost=True)
        return row
    compiled = {}
    size = 64 if getattr(args, 'low_disk', False) else len(units)
    with ThreadPoolExecutor(max_workers=getattr(args, 'jobs', 1)) as pool:
        for start in range(0, len(units), size):
            batch = list(pool.map(compile_one, units[start:start + size]))
            if not unchanged():
                result['inputsChanged'] = True
                return result
            assemble_batch(batch, records, asm_root, out_root, use_cache=not args.no_cache)
            if not unchanged():
                result['inputsChanged'] = True
                return result
            if getattr(args, 'low_disk', False):
                release_successful_assembly(batch, asm_root)
            install(batch, asm_root, out_root, args.allow_symbol_drift)
            compiled.update((row['unit'], row) for row in batch)
    rows[:] = [compiled.get(old['unit'], old) for old in rows]
    result['outcomes'] = {unit: row['outcome'] for unit, row in compiled.items()}
    result['ready'] = unchanged() and all(row['outcome'] == 'substituted' for row in compiled.values())
    result['outcome'] = 'substituted' if result['ready'] else 'incomplete'
    return result


def main() -> int:
    started = time.monotonic()
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--allowlist", type=Path, help="file of unit paths, one per line")
    parser.add_argument("--all", action="store_true", help="every target C compilation")
    parser.add_argument("--k1-root", type=Path, default=REPO_ROOT / ".build/k1")
    parser.add_argument("--src-root", type=Path, default=REPO_ROOT / ".build/linux-src")
    parser.add_argument('--toolchain-root', type=Path,
                        default=Path(os.environ.get('EVOCC_BENCH_SYSROOT',
                                                    str(REPO_ROOT / '.build/ladder/sysroot'))),
                        help='unchanged sealed-image header export for recorded /usr/ include paths')
    parser.add_argument("--evocc", type=Path, default=default_evocc())
    parser.add_argument("--evidence-root", type=Path, default=REPO_ROOT / ".evidence/k1")
    parser.add_argument("--run-id", default=None)
    parser.add_argument("--jobs", type=int, default=8)
    parser.add_argument("--relink-jobs", type=int, default=10)
    parser.add_argument("--timeout", type=int, default=180)
    parser.add_argument("--low-disk", action="store_true",
                        help="assemble/verify batches of 64; discard verified successful assembly")
    parser.add_argument("--no-install", action="store_true", help="survey only; change nothing")
    parser.add_argument("--no-relink", action="store_true")
    parser.add_argument("--allow-symbol-drift", action="store_true")
    parser.add_argument("--require-all", action="store_true", help="no reference target-C fallback or exclusions")
    parser.add_argument("--boot", action="store_true", help="boot only a complete, verified composition")
    parser.add_argument('--receipt', type=Path,
                        default=REPO_ROOT / 'tests/contracts/kernel-full-receipt.json',
                        help='complete-composition receipt; keep supplemental configurations separate')
    parser.add_argument('--resume-prepared-run', type=Path,
                        help='verify a completed first pass and repeat all prepared-input compilation, link and boot')
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
    if args.require_all and (not args.all or args.allowlist or args.no_install or args.no_relink):
        parser.error("--require-all needs --all and an actual full link")
    if args.boot and not args.require_all:
        parser.error("--boot requires --require-all")
    if args.resume_prepared_run and (not args.require_all or args.no_cache):
        parser.error('--resume-prepared-run requires --require-all and the original input cache')
    if args.require_all:
        # Reference optimization may remove symbols; never exclude their
        # translation unit. The full linker decides required symbol closure.
        args.allow_symbol_drift = True
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
    if args.low_disk and (run_dir / 'units.jsonl').exists():
        raise ValueError('low-disk survey requires a fresh run id')

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

    # Most kernel records use only -nostdinc plus source/generated headers.
    # Explicit toolchain includes (for example arm_neon.h) must instead name
    # the unchanged export, not an unrelated or absent host /usr directory.
    uses_toolchain = any(argument.startswith(('/usr/', '-I/usr/'))
                         for record in records.values() for argument in record.argv[1:])
    if not uses_toolchain:
        args.toolchain_root = None
    elif not args.toolchain_root.is_dir():
        raise ValueError(f'missing sealed-image header export: {args.toolchain_root}')

    asm_root = args.k1_root / "evocc-out"
    asm_root.mkdir(parents=True, exist_ok=True)
    omitted_options = {}
    for record in records.values():
        _, omitted = evocc_argv(record, args.evocc, args.src_root, pristine,
                               toolchain_root=args.toolchain_root)
        for option in set(omitted):
            if not option.startswith("-"):
                continue
            key = "-Wp,-MMD,<dependency-file>" if option.startswith("-Wp,-MMD,") else option
            entry = omitted_options.setdefault(key, {"units": 0, "classification": omitted_option_class(option)})
            entry["units"] += 1
    unreviewed = [key for key, value in omitted_options.items() if value["classification"] == "UNREVIEWED"]
    if args.require_all and unreviewed:
        raise ValueError(f"unreviewed effective build options: {unreviewed}")

    cache_path = args.cache or (args.k1_root / "survey-cache.json")
    cache = {} if args.no_cache else load_cache(cache_path)
    evocc_sha256 = sha256_file(args.evocc)
    print("k1: snapshotting source and reference inputs", flush=True)
    inputs_sha256 = input_snapshot(args.src_root, pristine, toolchain_root=args.toolchain_root)
    resumption = None
    if args.resume_prepared_run:
        if args.resume_prepared_run.resolve() == run_dir.resolve():
            raise ValueError('prepared resume requires a fresh run id')
        rows, resumption = resume_prepared_rows(args.resume_prepared_run, records,
                                                args, cache, inputs_sha256)
        pending = []
        print(f"k1: recovered {len(rows)} verified first-pass records, no installed-object credit; "
              "every prepared-input consumer requires matching compilation evidence", flush=True)
    snapshot_seconds = time.monotonic() - started
    keys: dict[str, str] = {}
    reused: list[dict] = []
    fresh: list[str] = []
    for unit in pending:
        if args.no_cache:
            fresh.append(unit)
            continue
        try:
            argv, _ = evocc_argv(records[unit], args.evocc, args.src_root, pristine,
                                toolchain_root=args.toolchain_root)
            source_sha256 = sha256_file(source_path(records[unit], args.src_root, pristine))
        except (RecordError, OSError):
            fresh.append(unit)
            continue
        key = cache_key(unit, evocc_sha256, source_sha256, argv, inputs_sha256)
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
    emission_seconds = assembly_seconds = symbols_seconds = 0.0
    batch_size = 64 if args.low_disk else max(1, len(fresh))
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        for start in range(0, len(fresh), batch_size):
            emission_started = time.monotonic()
            batch = []
            for row in pool.map(
                lambda unit: emit_assembly(
                    unit, records[unit], args.evocc, args.src_root, pristine,
                    asm_root, args.timeout, args.toolchain_root,
                ),
                fresh[start:start + batch_size],
            ):
                batch.append(row)
                produced.append(row)
                if args.no_cache:
                    continue
                key = keys.get(row["unit"])
                if key is not None and row.get("outcome") in CACHEABLE_OUTCOMES:
                    cache[key] = dict(row)
                since_checkpoint += 1
                if since_checkpoint >= CACHE_CHECKPOINT:
                    save_cache(cache_path, cache)
                    since_checkpoint = 0
                    print(f"k1: {len(produced)} of {len(fresh)} examined", flush=True)
            emission_seconds += time.monotonic() - emission_started
            if args.low_disk:
                stage = time.monotonic()
                assemble_batch(batch, records, asm_root, pristine, use_cache=not args.no_cache)
                assembly_seconds += time.monotonic() - stage
                stage = time.monotonic()
                check_symbol_parity(batch, asm_root, pristine, args.k1_root / "symbols")
                symbols_seconds += time.monotonic() - stage
                release_successful_assembly(batch, asm_root)
                with (run_dir / 'units.jsonl').open('a', encoding='utf-8') as handle:
                    for row in batch:
                        handle.write(json.dumps(row, sort_keys=True) + '\n')
                print(f"k1: verified {len(produced)} of {len(fresh)}; successful assembly released", flush=True)
    if not args.no_cache:
        save_cache(cache_path, cache)
    rows.extend(produced)
    rows.extend(reused)

    # Low-disk batches are already assembled and compared; only cache hits
    # still need those stages. Normal runs keep their single-container path.
    unfinished = reused if args.low_disk else rows
    assembly_started = time.monotonic()
    print("k1: assembling with the reference GCC in the sealed container", flush=True)
    assemble_batch(unfinished, records, asm_root, pristine, use_cache=not args.no_cache)
    assembly_seconds += time.monotonic() - assembly_started
    symbols_started = time.monotonic()
    print("k1: comparing defined symbols against the reference objects", flush=True)
    check_symbol_parity(unfinished, asm_root, pristine, args.k1_root / "symbols")
    symbols_seconds += time.monotonic() - symbols_started
    if args.low_disk:
        release_successful_assembly(unfinished, asm_root)
    if inputs_sha256 != input_snapshot(args.src_root, pristine, toolchain_root=args.toolchain_root) or evocc_sha256 != sha256_file(args.evocc):
        raise ValueError("compiler or input trees changed during the survey")

    previously = load_state(args.k1_root)
    substituted: list[str] = []
    compilation_complete = resumption is not None or complete_coverage(rows, wanted, "assembled")
    install_permitted = not args.no_install and (not args.require_all or compilation_complete)
    if install_permitted:
        retained = {row["unit"] for row in rows if row["outcome"] == "assembled"
                    and (args.allow_symbol_drift or not (row.get("missingSymbols") or row.get("extraSymbols")))}
        restored = [] if args.require_all else restore([unit for unit in previously if unit not in retained], pristine, out_root)
        print(f"k1: restored {len(restored)} previously substituted objects", flush=True)
        if resumption is None:
            install(rows, asm_root, out_root, args.allow_symbol_drift)
            substituted = [row["unit"] for row in rows if row["outcome"] == "substituted"]
            save_state(args.k1_root, substituted)
            print(f"k1: installed {len(substituted)} EvoCC objects", flush=True)
        else:
            save_state(args.k1_root, [])
            print(f"k1: {len(rows)} working objects require prepared-input validation and installation",
                  flush=True)

    relink_code = None
    preparation = None
    module_order = pristine / 'modules.order'
    modules_required = (args.require_all and module_order.is_file()
                        and bool(module_order.read_text().strip()))
    module_link = {'required': modules_required,
                   'status': 'NOT_RUN' if modules_required else 'NOT_REQUIRED'}
    if install_permitted and not args.no_relink:
        print("k1: preparing generated headers and recompiling their consumers", flush=True)
        preparation = recompile_after_prepare(rows, records, args, asm_root, pristine,
                                               out_root, run_dir)
        if resumption is not None:
            resumption['preparedCompilationsReused'] = preparation.get('preparedCacheHits', 0)
            resumption['policy'] = 'verify every prepared-input compilation; repeat fixed-point check, link and boot'
        ready = (preparation.get("prepareExitCode") == 0
                 and preparation.get("verifyPrepareExitCode") == 0
                 and preparation.get("preparedInputsStable")
                 and not preparation.get("failedUnits"))
        if ready:
            preparation["exportTable"] = recompile_export_table(
                rows, records, args, asm_root, out_root, run_dir)
            ready = preparation["exportTable"]["ready"]
        relink_code = (relink(args.src_root, out_root, run_dir / "relink.log", args.relink_jobs)
                       if ready else 1)
        export = preparation.get("exportTable", {})
        if export.get("ready") and export.get("required"):
            export["unchangedAfterLink"] = all(
                Path(item['path']).is_file() and sha256_file(Path(item['path'])) == item['sha256']
                for item in export['sources'].values())
            if not export["unchangedAfterLink"]:
                relink_code = 1
        print(f"k1: relink exit {relink_code}", flush=True)
        if modules_required and relink_code == 0:
            print('k1: linking audited module inputs and finalizing BTF', flush=True)
            module_link = {**relink_modules(args.src_root, pristine, out_root,
                                           rows, run_dir, args.relink_jobs),
                           'required': True}
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
    first_codes: dict[str, int] = {}
    for row in rows:
        if row.get("diagnosticCodes"):
            code = row["diagnosticCodes"][0]
            first_codes[code] = first_codes.get(code, 0) + 1
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
        "assemblyCacheHits": sum(bool(row.get("assemblyFromCache")) for row in rows),
        "freshCompilations": len(fresh),
        "resumption": resumption,
        "inputSnapshotSha256": inputs_sha256,
        "sourceRoot": str(args.src_root),
        "referenceRoot": str(pristine),
        "toolchainRoot": str(args.toolchain_root) if args.toolchain_root else None,
        "outputRoot": str(out_root),
        "configSha256": sha256_file(pristine / '.config'),
        "assemblerImage": sealed_image(),
        "omittedOptions": dict(sorted(omitted_options.items())),
        "jobs": args.jobs,
        "lowDisk": args.low_disk,
        "assemblyBatchSize": 64 if args.low_disk else None,
        "seconds": {"snapshot": round(snapshot_seconds, 3), "compile": round(emission_seconds, 3),
                    "assemble": round(assembly_seconds, 3), "symbols": round(symbols_seconds, 3),
                    "total": round(time.monotonic() - started, 3)},
        "counts": counts,
        "refusalsByFirstCode": dict(sorted(first_codes.items())),
        "refusalCodeCounts": dict(sorted(refusal_codes.items())),
        "substitutedUnits": sorted(substituted),
        "relinkExitCode": relink_code,
        "moduleLink": module_link,
        "preparation": preparation,
        "installed": install_permitted,
        "allowSymbolDrift": args.allow_symbol_drift,
        "imageSha256": sha256_file(out_root / "arch/arm64/boot/Image")
        if (out_root / "arch/arm64/boot/Image").is_file()
        else None,
        "vmlinuxSha256": sha256_file(out_root / "vmlinux")
        if (out_root / "vmlinux").is_file()
        else None,
    }
    summary["unitsSha256"] = sha256_file(run_dir / "units.jsonl")
    if args.require_all:
        whole = (relink_code == 0 and complete_coverage(rows, wanted, "substituted")
                 and module_link['status'] in ('PASS', 'NOT_REQUIRED'))
        boot = {"verdict": "NOT_RUN"}
        if args.boot and whole:
            subprocess.run([str(REPO_ROOT / "tools/k1/boot.sh"),
                            str(out_root / "arch/arm64/boot/Image"), str(run_dir / "boot")], check=False)
            boot_path = run_dir / "boot/boot.json"
            if boot_path.exists():
                boot = json.loads(boot_path.read_text())
            whole = whole and boot.get("imageSha256") == summary["imageSha256"]
        summary.update(mode="full-kernel", completeComposition=whole, boot=boot,
                       driverSha256=evocc_sha256,
                       status="PASS" if whole and boot["verdict"] == "PASS" else "INCOMPLETE",
                       evidence=str(run_dir.relative_to(REPO_ROOT)))
        args.receipt.write_text(
            json.dumps(summary, indent=2, sort_keys=True) + "\n")
    (run_dir / "substitution.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    print(json.dumps({k: v for k, v in summary.items() if k != "substitutedUnits"}, indent=2))
    print(f"evidence: {run_dir}")
    if args.require_all:
        return int(summary["status"] != "PASS")
    if relink_code not in (None, 0) or counts.get("rebuilt-by-kbuild", 0):
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
