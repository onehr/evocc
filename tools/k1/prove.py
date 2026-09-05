#!/usr/bin/env python3
"""Prove that the relinked kernel really carries EvoCC's code.

Two independent witnesses, both read out of the linked image:

* the producer note. The backend ends every ELF module with an ``.ident``,
  which the assembler puts in ``.comment`` and the linker keeps. A vmlinux
  containing that string was linked from at least one EvoCC object; the
  reference vmlinux must not contain it.

* byte identity per symbol. For every symbol a substituted object defines,
  the bytes of that symbol in the object and the bytes of the same symbol in
  vmlinux are extracted and compared. This is per unit, so it says which
  units are in the image rather than that some unit is.

A substituted object that defines no symbol contributes no bytes and is
reported as ``no-symbols``: there is nothing of it in vmlinux to find, and
counting it as proven would be a lie.

Section layout is read with the sealed container's readelf; the byte slicing
is done here, against the same files.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

REPO_ROOT = Path(__file__).resolve().parents[2]
CONTAINER_BIN = "/usr/local/bin/container"

SECTION = re.compile(
    r"^\s*\[\s*(?P<index>\d+)\]\s+(?P<name>\S+)\s+(?P<type>\S+)\s+"
    r"(?P<addr>[0-9a-f]+)\s+(?P<off>[0-9a-f]+)\s+(?P<size>[0-9a-f]+)"
)
SYMBOL = re.compile(
    r"^\s*\d+:\s+(?P<value>[0-9a-f]+)\s+(?P<size>\d+)\s+(?P<type>\S+)\s+"
    r"(?P<bind>\S+)\s+\S+\s+(?P<ndx>\S+)\s+(?P<name>\S+)\s*$"
)


def container_readelf(mounts: dict[str, Path], paths: list[str]) -> str:
    """Run readelf over several files in one container invocation."""
    volumes: list[str] = []
    for destination, source in mounts.items():
        volumes += ["-v", f"{source}:{destination}:ro"]
    script = "\n".join(
        f'echo "@@FILE {path}"; readelf -SW {path}; echo "@@SYMS {path}"; readelf -sW {path}'
        for path in paths
    )
    completed = subprocess.run(
        [CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
         *volumes, "evo-flight-lab:sealed", "sh", "-c", script],
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode != 0:
        raise RuntimeError(f"readelf failed: {completed.stderr.strip()[:500]}")
    return completed.stdout


def parse_readelf(text: str) -> dict[str, dict]:
    """Split one combined readelf stream into per-file section and symbol tables."""
    files: dict[str, dict] = {}
    current: dict | None = None
    mode = None
    for line in text.splitlines():
        if line.startswith("@@FILE "):
            current = {"sections": {}, "symbols": {}}
            files[line[len("@@FILE "):]] = current
            mode = "sections"
            continue
        if line.startswith("@@SYMS "):
            current = files[line[len("@@SYMS "):]]
            mode = "symbols"
            continue
        if current is None:
            continue
        if mode == "sections":
            match = SECTION.match(line)
            if match:
                current["sections"][int(match.group("index"))] = {
                    "name": match.group("name"),
                    "addr": int(match.group("addr"), 16),
                    "offset": int(match.group("off"), 16),
                    "size": int(match.group("size"), 16),
                    "type": match.group("type"),
                }
        else:
            match = SYMBOL.match(line)
            if match and match.group("ndx").isdigit():
                current["symbols"][match.group("name")] = {
                    "value": int(match.group("value"), 16),
                    "size": int(match.group("size")),
                    "type": match.group("type"),
                    "bind": match.group("bind"),
                    "ndx": int(match.group("ndx")),
                }
    return files


def slice_symbol(
    path: Path, table: dict, name: str, relative_to_address: bool, size: int
) -> bytes | None:
    """The first ``size`` bytes of one symbol, read straight out of the file.

    The size is supplied rather than taken from the symbol because the
    EvoCC emitter writes no ``.size`` directive, so its symbols measure zero.
    The reference object built from the same source supplies the length, and
    comparing that many bytes on both sides is exactly the question asked.
    """
    symbol = table["symbols"].get(name)
    if symbol is None or size == 0:
        return None
    section = table["sections"].get(symbol["ndx"])
    if section is None or section["type"] == "NOBITS":
        return None
    if relative_to_address:
        offset = section["offset"] + (symbol["value"] - section["addr"])
    else:
        offset = section["offset"] + symbol["value"]
    with path.open("rb") as handle:
        handle.seek(offset)
        data = handle.read(size)
    return data if len(data) == size else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--k1-root", type=Path, default=REPO_ROOT / ".build/k1")
    parser.add_argument("--units", type=Path, required=True,
                        help="the substituted.json state file or a plain unit list")
    parser.add_argument("--output", type=Path, default=None)
    args = parser.parse_args()

    text = args.units.read_text(encoding="utf-8")
    try:
        units = list(json.loads(text))
    except ValueError:
        units = [line.strip() for line in text.splitlines() if line.strip()]
    if not units:
        print("k1: no substituted units to prove", file=sys.stderr)
        return 2

    out_root = args.k1_root / "out"
    asm_root = args.k1_root / "evocc-out"
    pristine = args.k1_root / "pristine"

    ident_probe = subprocess.run(
        [CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
         "-v", f"{out_root}:/work/out:ro", "-v", f"{pristine}:/work/ref:ro",
         "evo-flight-lab:sealed", "sh", "-c",
         "readelf -p .comment /work/out/vmlinux; echo '@@REF'; readelf -p .comment /work/ref/vmlinux"],
        capture_output=True, text=True, check=False,
    )
    mixed_comment, _, reference_comment = ident_probe.stdout.partition("@@REF")
    ident = "EvoCC "
    producer_note = {
        "identInMixedVmlinux": ident in mixed_comment,
        "identInReferenceVmlinux": ident in reference_comment,
        "mixedCommentStrings": sorted(
            {line.split("]", 1)[1].strip() for line in mixed_comment.splitlines() if "]" in line}
        ),
    }

    paths = (
        ["/work/out/vmlinux"]
        + [f"/work/asm/{unit}" for unit in units]
        + [f"/work/ref/{unit}" for unit in units]
    )
    tables = parse_readelf(
        container_readelf(
            {"/work/out": out_root, "/work/asm": asm_root, "/work/ref": pristine}, paths
        )
    )
    vmlinux_table = tables["/work/out/vmlinux"]
    vmlinux_path = out_root / "vmlinux"

    rows = []
    for unit in units:
        table = tables.get(f"/work/asm/{unit}")
        object_path = asm_root / unit
        if table is None:
            rows.append({"unit": unit, "verdict": "unreadable"})
            continue
        reference = tables.get(f"/work/ref/{unit}", {"symbols": {}, "sections": {}})
        names = sorted(
            name
            for name, symbol in table["symbols"].items()
            if symbol["bind"] == "GLOBAL"
        )
        sizes = {
            name: table["symbols"][name]["size"]
            or reference["symbols"].get(name, {}).get("size", 0)
            for name in names
        }
        names = [name for name in names if sizes[name] > 0]
        if not names:
            rows.append({"unit": unit, "verdict": "no-symbols", "symbols": []})
            continue
        symbols = []
        for name in names:
            size = sizes[name]
            mine = slice_symbol(object_path, table, name, False, size)
            theirs = slice_symbol(vmlinux_path, vmlinux_table, name, True, size)
            symbols.append({
                "symbol": name,
                "bytes": size,
                "objectSha256": hashlib.sha256(mine).hexdigest() if mine else None,
                "vmlinuxSha256": hashlib.sha256(theirs).hexdigest() if theirs else None,
                "identical": mine is not None and mine == theirs,
            })
        verdict = "proven" if all(entry["identical"] for entry in symbols) else "mismatch"
        rows.append({"unit": unit, "verdict": verdict, "symbols": symbols})

    counts: dict[str, int] = {}
    for row in rows:
        counts[row["verdict"]] = counts.get(row["verdict"], 0) + 1
    report = {
        "producerNote": producer_note,
        "counts": counts,
        "units": rows,
        "vmlinux": str(vmlinux_path),
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered)
    ok = producer_note["identInMixedVmlinux"] and not producer_note["identInReferenceVmlinux"]
    return 0 if ok and counts.get("mismatch", 0) == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
