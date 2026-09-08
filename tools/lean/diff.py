#!/usr/bin/env python3
"""Compare two runners' answers to the same vector file, line by line.

Usage:
    tools/lean/diff.py <a.jsonl> <b.jsonl>
    tools/lean/diff.py --expect <a.jsonl>

Each file is one runner's output: the input line's own fields with a `result`
added.  The two must agree on the vector itself, line for line, and on the
result wherever both answered.  `"unsupported"` (the implementation cannot
answer this vector) and `"skipped"` (this kind is outside the runner's scope)
are abstentions, counted and reported but never a difference.

`--expect` checks one file against the `expect` field the named vectors carry,
which needs no second runner.

The exit status is 0 when nothing differs, 1 when something does, and 2 when
the two files are not answers to the same vectors.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ABSTENTIONS = {"unsupported", "skipped"}


def load(path: Path) -> list[dict]:
    rows = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as error:
            print(f"{path}:{number}: not JSON: {error}", file=sys.stderr)
            raise SystemExit(2)
    return rows


def vector_of(row: dict) -> dict:
    return {k: v for k, v in row.items() if k not in ("result", "expect")}


def compare(left: Path, right: Path) -> int:
    a = load(left)
    b = load(right)
    if len(a) != len(b):
        print(f"{left} has {len(a)} lines, {right} has {len(b)}", file=sys.stderr)
        return 2
    differences = 0
    abstained = 0
    agreed = 0
    for number, (x, y) in enumerate(zip(a, b), 1):
        if vector_of(x) != vector_of(y):
            print(f"line {number}: the two files answer different vectors", file=sys.stderr)
            return 2
        rx = x.get("result")
        ry = y.get("result")
        if rx in ABSTENTIONS or ry in ABSTENTIONS:
            abstained += 1
            continue
        if rx != ry:
            differences += 1
            if differences <= 20:
                print(f"line {number}: {json.dumps(vector_of(x))}")
                print(f"  {left.name}: {json.dumps(rx)}")
                print(f"  {right.name}: {json.dumps(ry)}")
        else:
            agreed += 1
    print(f"{len(a)} vectors: {agreed} agree, {abstained} abstained, {differences} differ")
    return 1 if differences else 0


def check_expect(path: Path) -> int:
    rows = load(path)
    checked = 0
    failures = 0
    for number, row in enumerate(rows, 1):
        if "expect" not in row:
            continue
        checked += 1
        if row.get("result") != row["expect"]:
            failures += 1
            print(
                f"line {number}: {json.dumps(vector_of(row))}\n"
                f"  expected {json.dumps(row['expect'])}, got {json.dumps(row.get('result'))}"
            )
    print(f"{checked} named vectors checked, {failures} wrong")
    return 1 if failures else 0


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--expect", action="store_true")
    parser.add_argument("files", nargs="+")
    args = parser.parse_args(argv)
    if args.expect:
        if len(args.files) != 1:
            parser.error("--expect takes one file")
        return check_expect(Path(args.files[0]))
    if len(args.files) != 2:
        parser.error("two files are needed to compare")
    return compare(Path(args.files[0]), Path(args.files[1]))


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
