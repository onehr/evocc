#!/usr/bin/env python3
"""Compare two answered vector files line by line.

Each side of the proof layer reads the same input file, copies every line and
appends its own `result`. This prints how many vectors the two agree on, how
many they differ on, and the first ten differences in full, so a disagreement is
read as the pair of answers it is rather than as a diff of two large files.

Usage: python3 tools/lean/diff.py <left.jsonl> <right.jsonl> [--limit 10]
Exit 0: the two agree on every vector. Exit 1: they differ, or the two files do
not answer the same vectors. Exit 2: usage or read error.

`skipped` marks a vector whose kind the runner does not implement and
`unsupported` one it cannot represent -- a width past what its fact carries, for
instance. Both are counted apart from agreements, because two runners that
answer `unsupported` in the same place agree about nothing except their limits.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

DEFAULT_LIMIT = 10
ABSTENTIONS = ("skipped", "unsupported")


def load(path: Path):
    try:
        with path.open("r", encoding="utf-8") as handle:
            for number, text in enumerate(handle, start=1):
                text = text.strip()
                if not text:
                    continue
                try:
                    yield number, json.loads(text)
                except ValueError as error:
                    raise SystemExit("%s:%d: not a JSON line: %s" % (path, number, error))
    except OSError as error:
        raise SystemExit("cannot read %s: %s" % (path, error))


def vector_of(record):
    """The input half of an answered line: everything but the answer."""
    return {key: value for key, value in record.items() if key != "result"}


def canonical(value):
    """A hashable, order-independent reading of one answer."""
    if isinstance(value, dict):
        return tuple(sorted((key, canonical(item)) for key, item in value.items()))
    if isinstance(value, list):
        return tuple(canonical(item) for item in value)
    return value


def check_expect(path: Path, limit: int) -> int:
    """One answered file against its own expectations; abstentions are failures here."""
    checked = 0
    failures = []
    for number, record in load(path):
        expected = record.get("expect")
        if expected is None:
            continue
        checked += 1
        if canonical(record.get("result")) != canonical(expected):
            failures.append((number, record))
    print("named expectations: %d checked, %d failed" % (checked, len(failures)))
    for number, record in failures[:limit]:
        print(
            "  line %d: expected %s, answered %s -- %s"
            % (
                number,
                json.dumps(record.get("expect")),
                json.dumps(record.get("result")),
                json.dumps(vector_of(record)),
            )
        )
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path, nargs="?")
    parser.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    parser.add_argument(
        "--expect",
        action="store_true",
        help="check the single given file against the expect field its named vectors carry",
    )
    options = parser.parse_args()

    if options.expect:
        if options.right is not None:
            parser.error("--expect takes one file")
        return check_expect(options.left, options.limit)
    if options.right is None:
        parser.error("two files are needed unless --expect is given")

    left = list(load(options.left))
    right = list(load(options.right))

    if len(left) != len(right):
        print(
            "the two files answer different numbers of vectors: %s has %d, %s has %d"
            % (options.left, len(left), options.right, len(right))
        )
        return 1

    vectors = 0
    agreements = 0
    differences = []
    mismatched_inputs = 0
    abstentions = {"left": 0, "right": 0}
    expectations = 0
    expectation_failures = []

    for (number, first), (_, second) in zip(left, right):
        vectors += 1
        if vector_of(first) != vector_of(second):
            mismatched_inputs += 1
            differences.append((number, first, second, "the two lines are not the same vector"))
            continue
        answer = first.get("result")
        other = second.get("result")
        if answer in ABSTENTIONS:
            abstentions["left"] += 1
        if other in ABSTENTIONS:
            abstentions["right"] += 1
        if canonical(answer) == canonical(other):
            agreements += 1
        else:
            differences.append((number, first, second, "the answers differ"))
        expected = first.get("expect")
        if expected is not None:
            expectations += 1
            for side, record in (("left", first), ("right", second)):
                if record.get("result") in ABSTENTIONS:
                    continue
                if canonical(record.get("result")) != canonical(expected):
                    expectation_failures.append((number, side, record))

    print("vectors: %d" % vectors)
    print("agreements: %d" % agreements)
    print("differences: %d" % len(differences))
    if mismatched_inputs:
        print("lines that are not the same vector on both sides: %d" % mismatched_inputs)
    print(
        "abstentions (skipped or unsupported): left=%d, right=%d"
        % (abstentions["left"], abstentions["right"])
    )
    if expectations:
        print(
            "named expectations: %d checked, %d failed"
            % (expectations, len(expectation_failures))
        )
        for number, side, record in expectation_failures[: options.limit]:
            print(
                "  line %d (%s): expected %s, answered %s -- %s"
                % (
                    number,
                    side,
                    json.dumps(record.get("expect")),
                    json.dumps(record.get("result")),
                    json.dumps(vector_of(record)),
                )
            )

    for number, first, second, reason in differences[: options.limit]:
        print("line %d: %s" % (number, reason))
        print("  left:  %s" % json.dumps(first, sort_keys=True))
        print("  right: %s" % json.dumps(second, sort_keys=True))

    if differences or expectation_failures:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
