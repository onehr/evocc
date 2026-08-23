#!/usr/bin/env python3
"""Compare the renderer's canonical output shapes against the specification grammar.

The renderer is the authority for what canonical EvoIR looks like, so every shape
it can emit must have a production in Section 9.3.

An earlier check compared instruction *names* and passed while the grammar gave a
unary predicate a binary form. A name appearing somewhere in the grammar says
nothing about its arity, its fields, or the literal text between its operands.
This compares shapes instead: each renderer format string and each grammar
production is reduced to a skeleton in which every operand becomes one hole, and
the two sets are compared.

Establishes: every form the renderer can emit is described by some production,
with the same arity and the same literal text between operands.

Does not establish: that a parser built from the grammar accepts exactly those
bytes. Only a round-trip over real modules shows that, and it belongs in the test
suite rather than in a static check.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "crates/evocc-evoir/src/text.rs"
SPEC = ROOT / "docs/evoir-spec.md"

HOLE = "<>"
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
QUOTED = re.compile(r'"([^"]*)"')
MNEMONIC_PREFIX = ("br ", "ret ", "cond_br ", "trap.", "ub.assume.")


def renderer_shapes() -> set[str]:
    source = RENDERER.read_text()
    shapes = set()
    for literal in re.findall(r'"((?:[^"\\]|\\.)*)"', source):
        has_hole = PLACEHOLDER.search(literal) is not None
        looks_like_form = "=" in literal or literal.startswith(MNEMONIC_PREFIX)
        if not looks_like_form:
            continue
        if not has_hole:
            # An already-rendered example from the renderer's own tests. It is
            # covered by the format string that produced it.
            continue
        shapes.add(collapse(unescape(PLACEHOLDER.sub(HOLE, literal))))
    return shapes


def grammar_skeletons() -> set[str]:
    """The normative shape table of Section 9.3.1.

    A table rather than the EBNF: extracting shapes from productions means
    reimplementing an EBNF reader, and a checker that has to be trusted to parse
    the grammar correctly is a second thing that can be wrong.
    """
    spec = SPEC.read_text()
    body = spec[spec.index("#### 9.3.1 Canonical Shapes"):spec.index("### 9.4 Canonical Byte Form")]
    block = body[body.index("```text") + len("```text"):]
    block = block[:block.index("```")]
    return {collapse(line.strip()) for line in block.splitlines() if line.strip()}


def unescape(shape: str) -> str:
    """Undo Rust format-string escaping so a shape is the text that is emitted.

    A doubled brace renders as one brace, and a trailing newline escape is the
    line ending Section 9.4 governs rather than part of the shape.
    """
    shape = shape.replace("{{", "{").replace("}}", "}")
    shape = shape.replace("\\n", "\n")
    return shape.strip()


def collapse(shape: str) -> str:
    """One nonterminal matches one placeholder, so runs of holes fold to one."""
    return re.sub(f"(?:{re.escape(HOLE)})+", HOLE, shape)


def superseded_shapes() -> set[str]:
    """Forms the renderer still emits that this revision has replaced.

    Reported rather than failed: the specification leads, and the renderer has
    not caught up. It is not silently accepted, because a parser built from the
    specification cannot read these bytes.
    """
    spec = SPEC.read_text()
    marker = "The form each replaces is **superseded**"
    if marker not in spec:
        return set()
    body = spec[spec.index(marker):]
    block = body[body.index("```text") + len("```text"):]
    block = block[:block.index("```")]
    return {collapse(line.strip()) for line in block.splitlines() if line.strip()}


def pending_shapes() -> set[str]:
    """Forms the specification requires that the renderer does not emit yet.

    A revision leads its implementation, so this gap is normal. Declaring it
    keeps it visible: an undeclared gap is indistinguishable from an oversight.
    """
    spec = SPEC.read_text()
    marker = "#### 9.3.2 Shapes Not Yet Emitted"
    if marker not in spec:
        return set()
    body = spec[spec.index(marker):]
    block = body[body.index("```text") + len("```text"):]
    block = block[:block.index("```")]
    return {collapse(line.strip()) for line in block.splitlines() if line.strip()}


def main() -> int:
    shapes = renderer_shapes()
    specified = grammar_skeletons()
    pending = pending_shapes()

    # A renderer form the specification does not describe is the dangerous
    # direction: a checker built from the specification could not read it.
    superseded = superseded_shapes()
    undescribed = sorted(s for s in shapes if s not in specified and s not in superseded)
    still_emitted = sorted(s for s in shapes if s in superseded)
    # A specified form the renderer does not emit is implementation work, and is
    # a failure only when it was not declared.
    unimplemented = sorted(s for s in specified if s not in shapes)
    undeclared = [s for s in unimplemented if s not in pending]

    print(f"renderer shapes:     {len(shapes)}")
    print(f"specified shapes:    {len(specified)}")
    print(f"declared as pending: {len(pending)}")
    if still_emitted:
        print(f"\nsuperseded forms the renderer still emits ({len(still_emitted)}), "
              f"which a conforming parser rejects:")
        for shape in still_emitted:
            print("  " + shape)

    status = 0
    if undescribed:
        print(f"\nrenderer emits forms the specification does not describe ({len(undescribed)}):")
        for shape in undescribed:
            print("  " + shape)
        status = 1
    if undeclared:
        print(f"\nspecified forms the renderer does not emit, undeclared ({len(undeclared)}):")
        for shape in undeclared:
            print("  " + shape)
        status = 1
    if status == 0:
        print("\nevery renderer form is described; every gap is declared")
    return status


if __name__ == "__main__":
    sys.exit(main())
