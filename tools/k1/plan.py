#!/usr/bin/env python3
"""The decisions the measurement makes between its steps.

The measurement is a pipeline of processes, and the choices between them are
small enough to hide in shell and important enough to be wrong: which units
go on the allowlist, what the refusal table says, which units are left after
a bisection removes the guilty ones, and what the summary line claims. They
live here as functions with tests rather than as pipelines of awk.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

#: An outcome that means the unit reached an object file the linker can take.
ASSEMBLING_OUTCOMES = frozenset({"assembled", "substituted", "symbol-drift"})

#: Diagnostic code prefixes and the stage that owns them. The machine backend
#: reports its lowering refusals as E4xxx and every later stop through the
#: driver's E0004, so both belong to the same owner.
MACHINE_CODES = frozenset({"E0004"})


def code_owner(code: str) -> str:
    """Which stage a diagnostic code belongs to."""
    if code in MACHINE_CODES or code.startswith("E4"):
        return "machine"
    if code.startswith("E0"):
        return "driver"
    return "front-end"


def load_rows(path: Path) -> list[dict]:
    rows = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def assembling_units(rows: list[dict]) -> list[str]:
    """Every unit that produced an object, in the order the survey lists them."""
    return [row["unit"] for row in rows if row.get("outcome") in ASSEMBLING_OUTCOMES]


def refusal_table(rows: list[dict]) -> dict[str, dict]:
    """Per code: how many units it stopped, and which stage owns it.

    A unit that reports the same code twice counts once; a unit that reports
    two codes counts under both, so the columns do not sum to the number of
    refused units and the table says so by construction.
    """
    table: dict[str, dict] = {}
    for row in rows:
        for code in sorted(set(row.get("diagnosticCodes", []))):
            entry = table.setdefault(code, {"units": 0, "owner": code_owner(code)})
            entry["units"] += 1
    return dict(sorted(table.items()))


def first_code_table(rows: list[dict]) -> dict[str, int]:
    """Per code: how many units it stopped first. These columns do sum."""
    counts: dict[str, int] = {}
    for row in rows:
        codes = row.get("diagnosticCodes") or []
        if codes:
            counts[codes[0]] = counts.get(codes[0], 0) + 1
    return dict(sorted(counts.items()))


def owner_totals(rows: list[dict]) -> dict[str, int]:
    """Per stage: how many units it stopped first."""
    totals: dict[str, int] = {}
    for code, count in first_code_table(rows).items():
        owner = code_owner(code)
        totals[owner] = totals.get(owner, 0) + count
    return dict(sorted(totals.items()))


def link_collisions(rows: list[dict]) -> dict:
    """The candidate objects that cannot share a link with each other.

    A symbol defined by two objects is a multiple definition, and the linker
    refuses the whole vmlinux over it. The survey already records what each
    object defines, so the objects that can never be substituted together are
    named here, before any link is attempted, rather than discovered one
    reduction at a time. Units are grouped into the sets that conflict, so a
    group of size n costs at least n-1 units of the measurement.
    """
    defined = {
        row["unit"]: set(row["definedSymbols"])
        for row in rows
        if row.get("outcome") in ASSEMBLING_OUTCOMES and row.get("definedSymbols")
    }
    owners: dict[str, list[str]] = {}
    for unit, symbols in defined.items():
        for symbol in symbols:
            owners.setdefault(symbol, []).append(unit)
    shared = {symbol: units for symbol, units in owners.items() if len(units) > 1}

    parent: dict[str, str] = {}

    def find(unit: str) -> str:
        parent.setdefault(unit, unit)
        while parent[unit] != unit:
            parent[unit] = parent[parent[unit]]
            unit = parent[unit]
        return unit

    for units in shared.values():
        first = find(units[0])
        for other in units[1:]:
            parent[find(other)] = first

    groups: dict[str, list[str]] = {}
    for unit in parent:
        groups.setdefault(find(unit), []).append(unit)
    return {
        "symbolsDefinedTwice": len(shared),
        "units": sorted(parent),
        "groups": sorted((sorted(group) for group in groups.values()), key=lambda g: g[0]),
    }


def failures_by_reason(failing: list[str], rows: list[dict]) -> dict[str, int]:
    """How many removed units the link refused for a multiple definition.

    A unit that shares a defined symbol with another candidate was removed for
    a reason the linker states; anything else was removed for a reason the
    measurement has yet to explain, and the two must not be added together.
    """
    colliding = set(link_collisions(rows)["units"])
    counts = {"multiple-definition": 0, "other": 0}
    for unit in failing:
        counts["multiple-definition" if unit in colliding else "other"] += 1
    return counts


def reusable_reference_boot(receipts: list[tuple[str, dict]], image_sha256: str) -> str | None:
    """The newest reference-boot receipt that still speaks for this reference.

    The gate it stands for is "the untouched reference kernel boots on this
    machine". That verdict is about one Image, so a receipt may only be reused
    when it passed and when the Image it booted is the one on disk now. The
    receipts are taken newest first; the caller supplies that order.
    """
    for path, receipt in receipts:
        if receipt.get("verdict") != "PASS":
            continue
        if receipt.get("referenceImageSha256") != image_sha256:
            continue
        if receipt.get("imageSha256") != image_sha256:
            continue
        return path
    return None


def reference_boots(evidence_root: Path) -> list[tuple[str, dict]]:
    """Every reference-boot receipt under the evidence root, newest first."""
    found = []
    for path in evidence_root.glob("*/reference-boot.json"):
        try:
            found.append((path, json.loads(path.read_text(encoding="utf-8"))))
        except (OSError, ValueError):
            continue
    found.sort(key=lambda item: item[0].stat().st_mtime, reverse=True)
    return [(str(path), receipt) for path, receipt in found]


def subtract(units: list[str], removed: list[str]) -> list[str]:
    """The allowlist without the units a bisection blamed, order preserved."""
    blamed = set(removed)
    return [unit for unit in units if unit not in blamed]


def summary_line(booting: int, assembling: int, total: int) -> str:
    return (
        f"units compiled by EvoCC in a booting kernel: {booting} "
        f"(assembling {assembling} of {total})"
    )


def read_units(path: Path) -> list[str]:
    units = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.split("#", 1)[0].strip()
        if line and line not in units:
            units.append(line)
    return units


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)

    allow = sub.add_parser("allowlist", help="the units that produced an object")
    allow.add_argument("units_jsonl", type=Path)

    refusals = sub.add_parser("refusals", help="the refusal table as JSON")
    refusals.add_argument("units_jsonl", type=Path)

    clash = sub.add_parser("collisions", help="candidates that cannot share a link")
    clash.add_argument("units_jsonl", type=Path)

    minus = sub.add_parser("subtract", help="an allowlist without a bisection's verdict")
    minus.add_argument("allowlist", type=Path)
    minus.add_argument("bisection", type=Path)

    line = sub.add_parser("summary", help="the one-line summary of a receipt")
    line.add_argument("receipt", type=Path)

    gate = sub.add_parser(
        "reference-boot", help="a reusable reference-boot receipt for this Image"
    )
    gate.add_argument("evidence_root", type=Path)
    gate.add_argument("image_sha256")

    args = parser.parse_args()

    if args.command == "allowlist":
        for unit in assembling_units(load_rows(args.units_jsonl)):
            print(unit)
        return 0
    if args.command == "refusals":
        rows = load_rows(args.units_jsonl)
        print(json.dumps(
            {
                "byCode": refusal_table(rows),
                "byFirstCode": first_code_table(rows),
                "byOwner": owner_totals(rows),
            },
            indent=2,
            sort_keys=True,
        ))
        return 0
    if args.command == "collisions":
        print(json.dumps(link_collisions(load_rows(args.units_jsonl)), indent=2, sort_keys=True))
        return 0
    if args.command == "subtract":
        bisection = json.loads(args.bisection.read_text(encoding="utf-8"))
        for unit in subtract(read_units(args.allowlist), bisection.get("minimalFailingSet") or []):
            print(unit)
        return 0
    if args.command == "summary":
        receipt = json.loads(args.receipt.read_text(encoding="utf-8"))
        print(summary_line(
            receipt["unitsCompiledByEvoccInABootingKernel"],
            receipt["survey"]["assemblingUnits"],
            receipt["survey"]["requestedUnits"],
        ))
        return 0
    if args.command == "reference-boot":
        path = reusable_reference_boot(reference_boots(args.evidence_root), args.image_sha256)
        if path is None:
            return 1
        print(path)
        return 0
    print(f"k1: unknown command {args.command}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
