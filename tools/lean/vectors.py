#!/usr/bin/env python3
"""Write the differential vectors both sides read.

The format is the plan's section 6, one JSON object per line:

    {"kind":"value","op":"shl","n":8,"m":16,"args":["0x1","0x100"]}
    {"kind":"transfer","op":"add","n":4,"facts":[{"mask":"0xc","value":"0x8"}, ...]}
    {"kind":"meet","n":8,"facts":[...]}
    {"kind":"join","n":8,"facts":[...]}
    {"kind":"covers","n":8,"facts":[M,X]}

Widths are integers: `n` the value width, `m` a shift amount's own width, `w`
a width change's source width.  Numbers are hexadecimal strings with a `0x`
prefix, lower case, no leading zeros, never padded to the width.

Each side copies the input line's bytes and appends `"result"`.  A line the
implementation cannot answer gets `"result":"unsupported"`; a line of a kind
outside a runner's scope gets `"result":"skipped"`.  `tools/lean/diff.py`
treats both as abstentions.

`named.jsonl` carries an `expect` field, so it is checkable on its own.

The files go to `lean/vectors/`, which is not tracked: they are derived from
this script and are large.
"""

from __future__ import annotations

import argparse
import itertools
import random
from pathlib import Path

BIN_OPS = ["add", "sub", "mul", "and", "or", "xor"]
CMP_OPS = ["icmp.eq", "icmp.ne", "icmp.slt", "icmp.sle", "icmp.ult", "icmp.ule"]
SHIFT_OPS = ["shl", "lshr", "ashr"]
CAST_OPS = ["zext", "sext", "trunc"]

# Section 8.5, as the eligibility implementations enforce it.
MAX_WIDTH = 64


def hexs(value: int) -> str:
    return "0x" + format(value, "x")


def fact(mask: int, value: int) -> str:
    return '{"mask":"%s","value":"%s"}' % (hexs(mask), hexs(value))


def normalized_facts(width: int) -> list[tuple[int, int]]:
    """Every normalized fact of this width: each bit known zero, known one, or
    unknown.  A value bit outside the mask is malformed (11.3), so the count is
    3^width rather than 4^width."""
    out = []
    for bits in itertools.product([(0, 0), (1, 0), (1, 1)], repeat=width):
        mask = 0
        value = 0
        for index, (m, v) in enumerate(reversed(bits)):
            mask |= m << index
            value |= v << index
        out.append((mask, value))
    return out


def value_line(op: str, args: list[int], **widths: int) -> str:
    fields = "".join(f',"{k}":{v}' for k, v in widths.items())
    return '{"kind":"value","op":"%s"%s,"args":[%s]}' % (
        op,
        fields,
        ",".join('"%s"' % hexs(a) for a in args),
    )


def transfer_line(op: str, facts: list[tuple[int, int]], **widths: int) -> str:
    fields = "".join(f',"{k}":{v}' for k, v in widths.items())
    return '{"kind":"transfer","op":"%s"%s,"facts":[%s]}' % (
        op,
        fields,
        ",".join(fact(m, v) for m, v in facts),
    )


def lattice_line(kind: str, n: int, facts: list[tuple[int, int]]) -> str:
    return '{"kind":"%s","n":%d,"facts":[%s]}' % (
        kind,
        n,
        ",".join(fact(m, v) for m, v in facts),
    )


def transfers_small(width: int, shift_widths: list[int]) -> list[str]:
    """Every normalized fact pair at a small width, for every row of the
    Section 8.10 table."""
    lines = []
    facts = normalized_facts(width)
    for op in BIN_OPS + CMP_OPS:
        for a in facts:
            for b in facts:
                lines.append(transfer_line(op, [a, b], n=width))
    for m in shift_widths:
        amounts = normalized_facts(m)
        for op in SHIFT_OPS:
            for a in facts:
                for k in amounts:
                    lines.append(transfer_line(op, [a, k], n=width, m=m))
    for op in CAST_OPS:
        for target in [1, width - 1, width, width + 1, 2 * width]:
            if not 1 <= target <= MAX_WIDTH:
                continue
            for a in facts:
                lines.append(transfer_line(op, [a], w=width, n=target))
    return lines


def lattice_small(width: int) -> list[str]:
    lines = []
    facts = normalized_facts(width)
    for kind in ["meet", "join", "covers"]:
        for a in facts:
            for b in facts:
                lines.append(lattice_line(kind, width, [a, b]))
    return lines


def values_width8(ops: list[str]) -> list[str]:
    lines = []
    for op in ops:
        for a in range(256):
            for b in range(256):
                lines.append(value_line(op, [a, b], n=8))
    return lines


def random_vectors(count: int, seed: int) -> list[str]:
    """Widths up to 64, with the shapes the plan calls out: uncommon widths,
    a shift amount of an independent width, the boundary amounts, the sign
    bit, same-width and cross-width changes, and conflicting facts."""
    rng = random.Random(seed)
    lines = []

    def rand_value(width: int) -> int:
        pick = rng.random()
        if pick < 0.2:
            return 0
        if pick < 0.35:
            return (1 << width) - 1
        if pick < 0.5:
            return 1 << (width - 1)
        return rng.getrandbits(width)

    def rand_fact(width: int) -> tuple[int, int]:
        mask = rand_value(width)
        return mask, rand_value(width) & mask

    while len(lines) < count:
        width = rng.randint(1, MAX_WIDTH)
        choice = rng.randrange(6)
        if choice == 0:
            op = rng.choice(BIN_OPS + CMP_OPS)
            lines.append(value_line(op, [rand_value(width), rand_value(width)], n=width))
        elif choice == 1:
            op = rng.choice(SHIFT_OPS)
            m = rng.randint(1, MAX_WIDTH)
            amount = rng.choice(
                [width - 1, width, width + 1, 2 * width, 0, rng.getrandbits(m)]
            )
            amount = max(amount, 0) % (1 << m)
            lines.append(value_line(op, [rand_value(width), amount], n=width, m=m))
        elif choice == 2:
            op = rng.choice(CAST_OPS)
            target = rng.randint(1, MAX_WIDTH)
            lines.append(value_line(op, [rand_value(width)], w=width, n=target))
        elif choice == 3:
            op = rng.choice(BIN_OPS + CMP_OPS)
            lines.append(transfer_line(op, [rand_fact(width), rand_fact(width)], n=width))
        elif choice == 4:
            op = rng.choice(SHIFT_OPS)
            m = rng.randint(1, MAX_WIDTH)
            lines.append(transfer_line(op, [rand_fact(width), rand_fact(m)], n=width, m=m))
        else:
            kind = rng.choice(["meet", "join", "covers"])
            if kind == "meet" and rng.random() < 0.5:
                # a conflicting pair: one bit known differently on each side
                bit = rng.randrange(width)
                left = (1 << bit, 1 << bit)
                right = (1 << bit, 0)
                lines.append(lattice_line(kind, width, [left, right]))
            else:
                lines.append(lattice_line(kind, width, [rand_fact(width), rand_fact(width)]))
    return lines


def named() -> list[str]:
    """The named vectors of the plan's section 6, with the value EvoIR's own
    definitions give (not C's), plus the shapes it asks to cover: `M = 1`,
    `N > M`, and `M > N`."""
    rows = [
        (value_line("shl", [1, 0x100], n=8, m=16), "0x0"),
        (value_line("shl", [1, 0], n=8, m=128), "0x1"),
        (value_line("shl", [1, 1], n=8, m=128), "0x2"),
        (value_line("shl", [1, 1 << 64], n=8, m=128), "0x0"),
        (value_line("ashr", [0x80, 1 << 64], n=8, m=128), "0xff"),
        (value_line("shl", [1, 1], n=8, m=1), "0x2"),
        (value_line("shl", [1, 0], n=8, m=1), "0x1"),
        (value_line("lshr", [0x80, 1], n=8, m=1), "0x40"),
        (value_line("ashr", [0x80, 1], n=8, m=1), "0xc0"),
        (value_line("shl", [1, 3], n=32, m=4), "0x8"),
        (value_line("shl", [1, 3], n=4, m=32), "0x8"),
        (value_line("shl", [1, 4], n=4, m=32), "0x0"),
        (value_line("icmp.slt", [0x80000000, 0], n=32), "0x1"),
        (value_line("icmp.ult", [0x80000000, 0], n=32), "0x0"),
        (value_line("sext", [0x80], w=8, n=32), "0xffffff80"),
        (value_line("zext", [0x80], w=8, n=32), "0x80"),
        (value_line("trunc", [0x1FF], w=32, n=8), "0xff"),
        (value_line("sext", [0x80], w=8, n=8), "0x80"),
    ]
    return [line[:-1] + ',"expect":"%s"}' % want for line, want in rows]


PROFILES = {
    # The plan's section 6 vector set: every normalized fact pair at width 4
    # (3^4 = 81 facts per operand), every width-8 pair for every binary value
    # operation, and random vectors up to width 64.
    "full": {
        "small_width": 4,
        "shift_widths": [1, 4],
        "width8_ops": BIN_OPS + CMP_OPS,
        "random": 50000,
    },
    # A tenth of the time, for iterating.
    "small": {
        "small_width": 3,
        "shift_widths": [1, 3],
        "width8_ops": ["add", "icmp.slt"],
        "random": 5000,
    },
}


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", default="lean/vectors")
    parser.add_argument("--profile", choices=sorted(PROFILES), default="full")
    parser.add_argument("--seed", type=int, default=20260909)
    args = parser.parse_args(argv)

    settings = PROFILES[args.profile]
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    files = {
        "transfers-small.jsonl": transfers_small(
            settings["small_width"], settings["shift_widths"]
        ),
        "lattice-small.jsonl": lattice_small(settings["small_width"]),
        "values-width8.jsonl": values_width8(settings["width8_ops"]),
        "random.jsonl": random_vectors(settings["random"], args.seed),
        "named.jsonl": named(),
    }
    for name, lines in files.items():
        (out / name).write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"{out / name}: {len(lines)} vectors")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(__import__("sys").argv[1:]))
