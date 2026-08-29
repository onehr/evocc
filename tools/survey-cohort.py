#!/usr/bin/env python3
"""Enumerate what a body of C requires, in one pass per unit.

A build reports one diagnostic so a failure has a single unambiguous site.
Enumerating what a corpus needs that way costs one build per unsupported
construct. `evocc --parse-only --survey` reports every diagnostic instead;
this aggregates them.

A caveat the output states rather than hides: once a declaration fails, its
name is undeclared and every later use reports too, so a single root can
dominate a unit. Findings are therefore ranked by how early they appear, and
the count of findings that share a unit's first line is shown, because those
are the ones a cascade cannot explain.
"""

import argparse
import collections
import json
import re
import subprocess
import sys
from pathlib import Path

DIAGNOSTIC = re.compile(r":(\d+):(\d+): error\[(E\d{4})\]: ([^\n]*)")


def survey_unit(evocc: Path, unit: Path):
    """Findings for one unit, or None when recovery could not complete.

    Recovery is sound across external declarations but not yet after a
    declaration fails part way through a function body: the symbol table rolls
    back a scope the parser is still standing in. A unit that hits that path is
    reported as unavailable, never as clean.
    """
    out = subprocess.run(
        [str(evocc), "--parse-only", "--survey", str(unit)],
        capture_output=True,
        text=True,
    )
    if "panicked" in out.stderr:
        return None
    found = []
    for line in out.stderr.splitlines():
        match = DIAGNOSTIC.search(line)
        if match:
            found.append((int(match.group(1)), match.group(3), match.group(4)))
    return found


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evocc", type=Path, default=Path("target/release/evocc"))
    parser.add_argument("--units", type=Path, required=True, help="directory of preprocessed units")
    parser.add_argument("--glob", default="*.gcc.c")
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()

    units = sorted(args.units.glob(args.glob))
    if not units:
        print(f"no units matched {args.glob} under {args.units}", file=sys.stderr)
        return 2

    at_root: collections.Counter = collections.Counter()
    anywhere: collections.Counter = collections.Counter()
    units_with: collections.defaultdict = collections.defaultdict(set)
    roots = {}
    unavailable = []
    for unit in units:
        found = survey_unit(args.evocc, unit)
        if found is None:
            unavailable.append(unit.stem)
            roots[unit.stem] = {"unavailable": "recovery incomplete"}
            continue
        if not found:
            roots[unit.stem] = None
            continue
        root_line = found[0][0]
        roots[unit.stem] = {"line": root_line, "code": found[0][1], "message": found[0][2]}
        for line, code, message in found:
            key = f"{code} {message.split(':')[0]}"
            anywhere[key] += 1
            units_with[key].add(unit.stem)
            if line == root_line:
                at_root[key] += 1

    print(f"{len(units)} units, {sum(anywhere.values())} diagnostics")
    print(f"{sum(1 for r in roots.values() if r is None)} units clean")
    if unavailable:
        print(f"{len(unavailable)} units unavailable (recovery incomplete): {', '.join(unavailable)}")
    print()
    print(f"{'at-root':>8} {'total':>8} {'units':>6}  finding")
    for key, total in anywhere.most_common():
        print(f"{at_root[key]:8} {total:8} {len(units_with[key]):6}  {key}")

    if args.json:
        args.json.write_text(
            json.dumps(
                {
                    "units": len(units),
                    "diagnostics": sum(anywhere.values()),
                    "unavailable": unavailable,
                    "roots": roots,
                    "findings": [
                        {
                            "finding": key,
                            "atRoot": at_root[key],
                            "total": total,
                            "units": sorted(units_with[key]),
                        }
                        for key, total in anywhere.most_common()
                    ],
                },
                indent=2,
            )
            + "\n"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
