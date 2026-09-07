#!/usr/bin/env python3
"""Prove that the relinked kernel really carries EvoCC's code.

Two independent witnesses, both read out of the linked image:

* the producer note. The backend ends every ELF module with an ``.ident``,
  which the assembler puts in ``.comment`` and the linker keeps. A vmlinux
  containing that string was linked from at least one EvoCC object; the
  reference vmlinux must not contain it.

* byte identity per symbol. For every symbol a substituted object defines,
  the bytes at that symbol in the object and at the same symbol in vmlinux
  are extracted and compared. This is per unit, so it says which units are in
  the image rather than that some unit is. The window length comes from the
  subject object's ELF symbol, and must match the linked symbol's size and
  fit both sections. Another compiler's function size is never its extent.

An object still carries unresolved relocations where the linker has yet to
fill an address in; vmlinux carries the filled-in value. Those fields are
masked on both sides before the comparison, and a symbol that matches only
after masking is reported as ``proven-modulo-relocations`` with the number of
masked bytes, never silently folded into an exact match.

A substituted object that defines no symbol contributes no bytes and is
reported as ``no-symbols``: there is nothing of it in vmlinux to find, and
counting it as proven would be a lie. Membership of the link is read
separately from ``vmlinux.a``, which names every object the image is built
from; an object outside it is not part of the kernel however well it
compiled.

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
RELOCATION = re.compile(
    r"^(?P<offset>[0-9a-f]{16})\s+[0-9a-f]{16}\s+(?P<type>R_\S+)"
)
RELOCATION_SECTION = re.compile(r"^Relocation section '(?P<name>\S+)'")
#: Relocation types that patch eight bytes; every other AArch64 relocation
#: this backend can produce patches one four-byte instruction or word.
WIDE_RELOCATIONS = ("ABS64", "PREL64", "GLOB_DAT", "RELATIVE")

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
        f'echo "@@FILE {path}"; readelf -SW {path}; echo "@@SYMS {path}"; '
        f'readelf -sW {path}; echo "@@RELS {path}"; readelf -rW {path}'
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
    section: str | None = None
    for line in text.splitlines():
        if line.startswith("@@FILE "):
            current = {"sections": {}, "symbols": {}, "relocations": {}}
            files[line[len("@@FILE "):]] = current
            mode = "sections"
            continue
        if line.startswith("@@SYMS "):
            current = files[line[len("@@SYMS "):]]
            mode = "symbols"
            continue
        if line.startswith("@@RELS "):
            current = files[line[len("@@RELS "):]]
            current.setdefault("relocations", {})
            mode = "relocations"
            section = None
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
        elif mode == "relocations":
            header = RELOCATION_SECTION.match(line)
            if header:
                name = header.group("name")
                section = name.split(".rela", 1)[-1] if name.startswith(".rela") else name
                current["relocations"].setdefault(section, [])
                continue
            match = RELOCATION.match(line)
            if match and section is not None:
                width = 8 if any(
                    marker in match.group("type") for marker in WIDE_RELOCATIONS
                ) else 4
                current["relocations"][section].append(
                    (int(match.group("offset"), 16), width)
                )
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
    """Read a complete sized symbol, never an adjacent section or partial body."""
    symbol = table["symbols"].get(name)
    if symbol is None or size <= 0 or symbol["size"] != size:
        return None
    section = table["sections"].get(symbol["ndx"])
    if section is None or section["type"] == "NOBITS":
        return None
    start = symbol["value"] - section["addr"] if relative_to_address else symbol["value"]
    if start < 0 or start + size > section["size"]:
        return None
    offset = section["offset"] + start
    with path.open("rb") as handle:
        handle.seek(offset)
        data = handle.read(size)
    return data if len(data) == size else None


def relocated_ranges(table: dict, name: str, size: int) -> list[tuple[int, int]]:
    """Where, inside one symbol, the object still has a hole for the linker."""
    symbol = table["symbols"].get(name)
    if symbol is None:
        return []
    section = table["sections"].get(symbol["ndx"])
    if section is None:
        return []
    start, end = symbol["value"], symbol["value"] + size
    return [
        (offset - start, width)
        for offset, width in table.get("relocations", {}).get(section["name"], [])
        if start <= offset < end
    ]


def masked(data: bytes, ranges: list[tuple[int, int]]) -> bytes:
    """The same bytes with every relocated field zeroed."""
    if not ranges:
        return data
    buffer = bytearray(data)
    for start, width in ranges:
        buffer[start : start + width] = b"\x00" * min(width, max(0, len(buffer) - start))
    return bytes(buffer)


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

    membership = subprocess.run(
        [CONTAINER_BIN, "run", "--rm", "--platform", "linux/arm64", "--network", "none",
         "-v", f"{out_root}:/work/out:ro", "evo-flight-lab:sealed",
         "sh", "-c", "cd /work/out && ar t vmlinux.a"],
        capture_output=True, text=True, check=False,
    )
    linked = {line.strip() for line in membership.stdout.splitlines() if line.strip()}

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
        in_link = unit in linked
        if table is None:
            rows.append({"unit": unit, "verdict": "unreadable", "linkedIntoVmlinux": in_link})
            continue
        if not in_link:
            rows.append({"unit": unit, "verdict": "not-linked", "linkedIntoVmlinux": False})
            continue
        names = sorted(
            name
            for name, symbol in table["symbols"].items()
            if symbol["bind"] == "GLOBAL"
        )
        sizes = {
            name: table["symbols"][name]["size"]
            for name in names
        }
        unsized = [name for name in names if sizes[name] == 0]
        if unsized:
            rows.append({
                "unit": unit, "verdict": "unverifiable", "symbols": unsized,
                "reason": "defined global symbol has no subject extent",
                "linkedIntoVmlinux": True,
            })
            continue
        names = [name for name in names if sizes[name] > 0]
        if not names:
            rows.append({
                "unit": unit, "verdict": "no-symbols", "symbols": [],
                "linkedIntoVmlinux": True,
            })
            continue
        symbols = []
        for name in names:
            size = sizes[name]
            mine = slice_symbol(object_path, table, name, False, size)
            theirs = slice_symbol(vmlinux_path, vmlinux_table, name, True, size)
            ranges = relocated_ranges(table, name, size)
            exact = mine is not None and mine == theirs
            after_masking = (
                mine is not None
                and theirs is not None
                and masked(mine, ranges) == masked(theirs, ranges)
            )
            symbols.append({
                "symbol": name,
                "bytes": size,
                "objectSha256": hashlib.sha256(mine).hexdigest() if mine else None,
                "vmlinuxSha256": hashlib.sha256(theirs).hexdigest() if theirs else None,
                "relocatedBytes": sum(width for _, width in ranges),
                "identical": exact,
                "identicalModuloRelocations": after_masking,
            })
        if all(entry["identical"] for entry in symbols):
            verdict = "proven"
        elif all(entry["identicalModuloRelocations"] for entry in symbols):
            verdict = "proven-modulo-relocations"
        else:
            verdict = "mismatch"
        rows.append({
            "unit": unit, "verdict": verdict, "symbols": symbols, "linkedIntoVmlinux": True,
        })

    counts: dict[str, int] = {}
    for row in rows:
        counts[row["verdict"]] = counts.get(row["verdict"], 0) + 1
    with vmlinux_path.open("rb") as handle:
        digest = hashlib.sha256()
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    report = {
        "vmlinuxSha256": digest.hexdigest(),
        "producerNote": producer_note,
        "counts": counts,
        "unitsLinkedIntoVmlinux": sorted(row["unit"] for row in rows if row["linkedIntoVmlinux"]),
        "units": rows,
        "vmlinux": str(vmlinux_path),
    }
    rendered = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output:
        args.output.write_text(rendered, encoding="utf-8")
    print(rendered)
    ok = producer_note["identInMixedVmlinux"] and not producer_note["identInReferenceVmlinux"]
    return 0 if ok and all(
        verdict in ("proven", "proven-modulo-relocations", "no-symbols", "not-linked")
        for verdict in counts
    ) else 1


if __name__ == "__main__":
    raise SystemExit(main())
