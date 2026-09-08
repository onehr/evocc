#!/usr/bin/env python3
"""Generate the correspondence vectors both sides of the proof layer answer.

The vectors are inputs, not answers. Each line names one call -- a value
operation, a known-bits transfer, a lattice operation or a side condition -- and
each implementation copies the line and appends its own `result`. Nothing here
computes an expected value except the handful of named vectors, whose expected
results come from EvoIR Section 3.4 rather than from any implementation.

Usage: python3 tools/lean/vectors.py [--out lean/vectors] [--seed 20260909]
                                     [--set transfers|lattice|values|random|named|all]

Format (EvoIR Lean plan, section 6). One JSON object per line, `\\n` terminated,
no spaces, keys in the fixed order below, numbers as lowercase hexadecimal
strings with a `0x` prefix and no leading zeros:

    {"kind":"value","op":"shl","n":8,"m":16,"args":["0x1","0x100"]}
    {"kind":"value","op":"sext","w":8,"n":32,"args":["0x80"]}
    {"kind":"transfer","op":"add","n":4,"facts":[{"mask":"0xc","value":"0x8"},...]}
    {"kind":"transfer","op":"shl","n":8,"m":128,"facts":[...]}
    {"kind":"meet","n":8,"facts":[A,B]}
    {"kind":"join","n":8,"facts":[A,B]}
    {"kind":"covers","n":8,"facts":[M,X]}

`n` is the result width, `w` the source width of a cast, `m` the independent
width of a shift amount. A fact is normalized: no bit is set in `value` outside
`mask`. A named vector carries `name` and `expect` as well; `expect` is what
Section 3.4 says, so a run that disagrees with it is a finding about the
implementation and not about the vector.

The generated tree is not tracked. It is reproducible from this file and a
seed, and a checked-in copy would be a second thing to keep in step with the
generator.
"""

from __future__ import annotations

import argparse
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[2]

# Section 8.5 bounds every root and witness to `bits<N>` with 1 <= N <= 64; the
# exhaustive sets stay at the widths where a full enumeration is small.
SMALL_WIDTHS = (1, 2, 3, 4)
EXHAUSTIVE_VALUE_WIDTH = 8

BINARY = ("add", "sub", "mul", "and", "or", "xor")
COMPARISONS = (
    "icmp.eq",
    "icmp.ne",
    "icmp.slt",
    "icmp.sle",
    "icmp.ult",
    "icmp.ule",
)
SHIFTS = ("shl", "lshr", "ashr")
CASTS = ("zext", "sext", "trunc")

# The widths a random vector prefers: the ones a C front end rarely produces, so
# that a transfer written for 8, 16, 32 and 64 is not the only one measured.
UNCOMMON_WIDTHS = (1, 2, 3, 5, 7, 9, 11, 13, 17, 23, 31, 33, 47, 63)
COMMON_WIDTHS = (8, 16, 32, 64)
# Independent shift-amount widths. 65 and 128 are in the set because Section 8.5
# writes the amount as "any bits<M>" with no upper bound.
AMOUNT_WIDTHS = (1, 2, 3, 4, 8, 16, 32, 64, 65, 128)

RANDOM_VECTORS = 20_000


def span(width: int) -> int:
    """Every bit of a `bits<width>` value."""
    return (1 << width) - 1


def hex_literal(value: int) -> str:
    return '"0x%x"' % value


def fact_literal(mask: int, value: int) -> str:
    return '{"mask":%s,"value":%s}' % (hex_literal(mask), hex_literal(value))


def render(fields) -> str:
    return "{" + ",".join('"%s":%s' % (key, value) for key, value in fields) + "}"


def value_line(op, n, args, w=None, m=None, name=None, expect=None) -> str:
    fields = [("kind", '"value"'), ("op", '"%s"' % op)]
    if w is not None:
        fields.append(("w", str(w)))
    fields.append(("n", str(n)))
    if m is not None:
        fields.append(("m", str(m)))
    fields.append(("args", "[" + ",".join(hex_literal(arg) for arg in args) + "]"))
    if name is not None:
        fields.append(("name", '"%s"' % name))
    if expect is not None:
        fields.append(("expect", expect))
    return render(fields)


def transfer_line(op, n, facts, w=None, m=None) -> str:
    fields = [("kind", '"transfer"'), ("op", '"%s"' % op)]
    if w is not None:
        fields.append(("w", str(w)))
    fields.append(("n", str(n)))
    if m is not None:
        fields.append(("m", str(m)))
    fields.append(
        ("facts", "[" + ",".join(fact_literal(mask, value) for mask, value in facts) + "]")
    )
    return render(fields)


def lattice_line(kind, n, facts) -> str:
    return render(
        [
            ("kind", '"%s"' % kind),
            ("n", str(n)),
            (
                "facts",
                "[" + ",".join(fact_literal(mask, value) for mask, value in facts) + "]",
            ),
        ]
    )


def normalized_facts(width: int):
    """Every normalized fact of `bits<width>`, in a fixed order.

    A bit is unknown, known zero or known one, so there are `3^width` of them:
    81 at width four, which is the figure the plan states.
    """
    facts = []
    for code in range(3**width):
        mask = 0
        value = 0
        remaining = code
        for bit in range(width):
            digit = remaining % 3
            remaining //= 3
            if digit == 1:
                mask |= 1 << bit
            elif digit == 2:
                mask |= 1 << bit
                value |= 1 << bit
        facts.append((mask, value))
    return facts


# -- exhaustive transfers ---------------------------------------------------


def transfers(write) -> int:
    """Every transfer of Section 8.10 over every normalized fact, widths 1..4."""
    written = 0
    small = {width: normalized_facts(width) for width in SMALL_WIDTHS}
    for n in SMALL_WIDTHS:
        for op in BINARY + COMPARISONS:
            for left in small[n]:
                for right in small[n]:
                    write(transfer_line(op, n, [left, right]))
                    written += 1
    for n in SMALL_WIDTHS:
        for m in SMALL_WIDTHS:
            for op in SHIFTS:
                for left in small[n]:
                    for amount in small[m]:
                        write(transfer_line(op, n, [left, amount], m=m))
                        written += 1
    for w in SMALL_WIDTHS:
        for n in SMALL_WIDTHS:
            for op in CASTS:
                if op == "trunc" and n > w:
                    continue
                if op != "trunc" and n < w:
                    continue
                for source in small[w]:
                    write(transfer_line(op, n, [source], w=w))
                    written += 1
    return written


def lattice(write) -> int:
    """`meet`, `join` and `covers` over every pair of normalized facts, 1..4."""
    written = 0
    for n in SMALL_WIDTHS:
        facts = normalized_facts(n)
        for kind in ("meet", "join", "covers"):
            for left in facts:
                for right in facts:
                    write(lattice_line(kind, n, [left, right]))
                    written += 1
    return written


# -- exhaustive values ------------------------------------------------------


def values(write) -> int:
    """Every binary value operation at width eight, over all 65,536 pairs."""
    written = 0
    n = EXHAUSTIVE_VALUE_WIDTH
    limit = 1 << n
    for op in BINARY + COMPARISONS:
        for left in range(limit):
            for right in range(limit):
                write(value_line(op, n, [left, right]))
                written += 1
    for op in SHIFTS:
        for left in range(limit):
            for amount in range(limit):
                write(value_line(op, n, [left, amount], m=n))
                written += 1
    return written


# -- random -----------------------------------------------------------------


def random_fact(rng: random.Random, width: int):
    """One normalized fact, with the fully known and fully unknown ends kept."""
    roll = rng.random()
    if roll < 0.1:
        return (span(width), rng.getrandbits(width) & span(width))
    if roll < 0.2:
        return (0, 0)
    mask = rng.getrandbits(width) & span(width)
    return (mask, rng.getrandbits(width) & mask)


def conflicting_pair(rng: random.Random, width: int):
    """Two facts that disagree on a bit both know, so their meet conflicts."""
    bit = rng.randrange(width)
    left_mask = (rng.getrandbits(width) & span(width)) | (1 << bit)
    right_mask = (rng.getrandbits(width) & span(width)) | (1 << bit)
    left_value = rng.getrandbits(width) & left_mask & ~(1 << bit)
    right_value = (rng.getrandbits(width) & right_mask) | (1 << bit)
    return (left_mask, left_value), (right_mask, right_value & right_mask)


def random_width(rng: random.Random) -> int:
    roll = rng.random()
    if roll < 0.6:
        return rng.choice(UNCOMMON_WIDTHS)
    if roll < 0.9:
        return rng.choice(COMMON_WIDTHS)
    return rng.randrange(1, 65)


def boundary_amounts(n: int, m: int):
    """The amounts the plan names, kept inside the amount's own width."""
    candidates = [0, 1, n - 1, n, n + 1, 2 * n, span(m), span(m) // 2]
    if m >= 64:
        candidates.append(1 << 64)
    if m > 1:
        candidates.append(1 << (m - 1))
    return [amount for amount in candidates if 0 <= amount <= span(m)]


def random_vectors(write, rng: random.Random) -> int:
    written = 0
    for _ in range(RANDOM_VECTORS):
        roll = rng.random()
        if roll < 0.3:
            written += random_value(write, rng)
        elif roll < 0.7:
            written += random_transfer(write, rng)
        else:
            written += random_lattice(write, rng)
    return written


def random_value(write, rng: random.Random) -> int:
    n = random_width(rng)
    choice = rng.random()
    if choice < 0.45:
        op = rng.choice(BINARY + COMPARISONS)
        write(value_line(op, n, [rng.getrandbits(n), rng.getrandbits(n)]))
        return 1
    if choice < 0.8:
        op = rng.choice(SHIFTS)
        m = rng.choice(AMOUNT_WIDTHS)
        amounts = boundary_amounts(n, m)
        amount = rng.choice(amounts) if rng.random() < 0.6 else rng.getrandbits(m)
        # The sign bit is the case `ashr` treats apart, so it is reached often.
        left = 1 << (n - 1) if rng.random() < 0.25 else rng.getrandbits(n)
        write(value_line(op, n, [left & span(n), amount & span(m)], m=m))
        return 1
    op = rng.choice(CASTS)
    if op == "trunc":
        w = n if rng.random() < 0.2 else rng.randrange(n, 65)
    else:
        w = n if rng.random() < 0.2 else rng.randrange(1, n + 1)
    write(value_line(op, n, [rng.getrandbits(w) & span(w)], w=w))
    return 1


def random_transfer(write, rng: random.Random) -> int:
    n = random_width(rng)
    choice = rng.random()
    if choice < 0.45:
        op = rng.choice(BINARY + COMPARISONS)
        write(transfer_line(op, n, [random_fact(rng, n), random_fact(rng, n)]))
        return 1
    if choice < 0.8:
        op = rng.choice(SHIFTS)
        m = rng.choice(AMOUNT_WIDTHS)
        if rng.random() < 0.6:
            amount = rng.choice(boundary_amounts(n, m))
            fact = (span(m), amount)
        else:
            fact = random_fact(rng, m)
        write(transfer_line(op, n, [random_fact(rng, n), fact], m=m))
        return 1
    op = rng.choice(CASTS)
    if op == "trunc":
        w = n if rng.random() < 0.2 else rng.randrange(n, 65)
    else:
        w = n if rng.random() < 0.2 else rng.randrange(1, n + 1)
    write(transfer_line(op, n, [random_fact(rng, w)], w=w))
    return 1


def random_lattice(write, rng: random.Random) -> int:
    n = random_width(rng)
    kind = rng.choice(("meet", "join", "covers"))
    if kind == "meet" and rng.random() < 0.4:
        left, right = conflicting_pair(rng, n)
    else:
        left, right = random_fact(rng, n), random_fact(rng, n)
    write(lattice_line(kind, n, [left, right]))
    return 1


# -- named ------------------------------------------------------------------

# The five vectors the plan names, with the results EvoIR Section 3.4 states:
# the amount is decoded at its own width, and an amount of at least N gives zero
# for `shl` and `lshr` and a sign fill for `ashr`.
NAMED = (
    ("shl-256-at-16", "shl", 8, 16, 0x1, 0x100, 0x0),
    ("shl-0-at-128", "shl", 8, 128, 0x1, 0x0, 0x1),
    ("shl-1-at-128", "shl", 8, 128, 0x1, 0x1, 0x2),
    ("shl-two-to-the-64-at-128", "shl", 8, 128, 0x1, 1 << 64, 0x0),
    ("ashr-two-to-the-64-at-128", "ashr", 8, 128, 0x80, 1 << 64, 0xFF),
    # M = 1, and the two orders of N against M.
    ("shl-one-bit-amount-zero", "shl", 8, 1, 0x1, 0x0, 0x1),
    ("shl-one-bit-amount-one", "shl", 8, 1, 0x1, 0x1, 0x2),
    ("shl-wide-value-narrow-amount", "shl", 32, 4, 0x1, 0xF, 0x8000),
    ("shl-narrow-value-wide-amount", "shl", 4, 32, 0x1, 0x3, 0x8),
    ("lshr-narrow-value-wide-amount", "lshr", 4, 32, 0x8, 0x4, 0x0),
    ("ashr-narrow-value-wide-amount", "ashr", 4, 32, 0x8, 0x4, 0xF),
)


def named(write) -> int:
    for name, op, n, m, left, amount, expect in NAMED:
        write(
            value_line(
                op,
                n,
                [left, amount],
                m=m,
                name=name,
                expect=hex_literal(expect),
            )
        )
    return len(NAMED)


SETS = {
    "transfers": ("transfers-small.jsonl", transfers),
    "lattice": ("lattice-small.jsonl", lattice),
    "values": ("values-width8.jsonl", values),
    "named": ("named.jsonl", named),
}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out",
        default=str(ROOT / "lean" / "vectors"),
        help="directory the .jsonl files are written to (default lean/vectors)",
    )
    parser.add_argument("--seed", type=int, default=20260909)
    parser.add_argument(
        "--set",
        default="all",
        choices=sorted(SETS) + ["random", "all"],
        help="which vector set to generate (default all)",
    )
    options = parser.parse_args()

    out = Path(options.out)
    out.mkdir(parents=True, exist_ok=True)
    wanted = sorted(SETS) + ["random"] if options.set == "all" else [options.set]

    total = 0
    for name in wanted:
        if name == "random":
            path = out / "random.jsonl"
            rng = random.Random(options.seed)
            with path.open("w", encoding="utf-8") as handle:

                def write(line, handle=handle):
                    handle.write(line + "\n")

                written = random_vectors(write, rng)
        else:
            filename, build = SETS[name]
            path = out / filename
            with path.open("w", encoding="utf-8") as handle:

                def write(line, handle=handle):
                    handle.write(line + "\n")

                written = build(write)
        total += written
        print("%s: %d vectors" % (path, written))
    print("total: %d vectors" % total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
