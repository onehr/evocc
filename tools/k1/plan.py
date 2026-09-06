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

    minus = sub.add_parser("subtract", help="an allowlist without a bisection's verdict")
    minus.add_argument("allowlist", type=Path)
    minus.add_argument("bisection", type=Path)

    line = sub.add_parser("summary", help="the one-line summary of a receipt")
    line.add_argument("receipt", type=Path)

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
    print(f"k1: unknown command {args.command}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
