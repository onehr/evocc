#!/usr/bin/env python3
"""Audit hint: compare the renderer's instruction shapes against Section 9.3.1.

**This is not a completeness gate and must not be cited as one.** An earlier
version of this tool was, and it was wrong twice over: it skipped format strings
containing no placeholder, so a form the renderer genuinely emits was reported as
not yet implemented, and it printed a success line while a known incompatibility
stood. A check that manufactures confidence is worse than no check.

Governance runs the other way from what that version assumed. The specification
is authoritative; the renderer is an implementation that must conform to it. This
tool reports where the two disagree, and every disagreement is a defect in the
renderer unless the specification has declared the gap.

Covers: instruction and global format strings in the renderer's non-test code,
compared by shape, where a shape is the literal text with each operand replaced
by one hole.

Does not cover: block headers, the closing brace, blank-line placement,
inter-item separators, operand *types*, field ordering inside attribute lists, or
whether a parser built from Section 9.3 accepts these bytes. Whole-module
round-trip tests cover parser/renderer agreement; the independently maintained
field-sentinel manifest covers omission. Neither property is established by this
shape audit.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RENDERER = ROOT / "crates/evocc-evoir/src/text.rs"
SPEC = ROOT / "spec/evoir.md"

HOLE = "<>"
PLACEHOLDER = re.compile(r"\{[^{}]*\}")
STRING = re.compile(
    r'''(?P<comment>//[^\n]*|/\*[\s\S]*?\*/)|'''
    r'''(?P<char>b?'(?:\\.|[^'\\])')|'''
    r'''b?r(?P<hashes>\#*)"(?P<raw>[\s\S]*?)"(?P=hashes)|'''
    r'''b?"(?P<normal>(?:[^"\\]|\\.)*)"'''
)
MNEMONIC = ("br", "br.indirect", "ret", "cond_br", "trap.", "ub.assume.")


def production_code(source: str) -> str:
    """The renderer without its test module.

    Test modules contain already-rendered examples, which are outputs of the
    format strings rather than additional forms, and would otherwise be counted
    as shapes in their own right.
    """
    marker = "#[cfg(test)]"
    return source[: source.index(marker)] if marker in source else source


def unescape(shape: str) -> str:
    """Undo Rust format-string escaping so a shape is the text emitted."""
    return shape.replace("{{", "{").replace("}}", "}").replace("\\n", "\n").strip()


def collapse(shape: str) -> str:
    return re.sub(f"(?:{re.escape(HOLE)})+", HOLE, shape)


def is_form(literal: str) -> bool:
    """A rendered instruction or global, as opposed to a fragment or a field name.

    Fixed strings count. The renderer emits a bare `ret` for a function that
    returns nothing, and a version of this tool that required a placeholder
    missed it.
    """
    return is_rendered_form(unescape(PLACEHOLDER.sub(HOLE, literal)))


def is_rendered_form(text: str) -> bool:
    """Whether already-normalized renderer text is one canonical shape line."""
    # Validation uses the fixed prefix `global @` when checking an already
    # rendered line. It is not itself a renderer form: the canonical global
    # form always carries the complete attribute sequence.
    if text.startswith("global @"):
        return " origin=" in text
    if text.startswith("func @"):
        return " cc = " in text
    if text.startswith("effects {"):
        # Like `global @`, the historical reader's line-identification prefix
        # is not an emitted form. A summary includes its closing delimiter.
        return text.endswith("}")
    if text == "{":
        return True
    if " = " in text:
        lhs = text.split(" = ", 1)[0]
        # Local Rust bindings are not emitted SSA definitions. In particular,
        # do not turn `let output = ...` inside raw strings into IR shapes.
        return lhs == HOLE or lhs.startswith("(" + HOLE)
    return any(text == m or text.startswith(m if not m[-1].isalnum() else m + " ")
               for m in MNEMONIC)


def renderer_shapes(source: str) -> set[str]:
    shapes = set()
    for match in STRING.finditer(source):
        literal = match.group("normal")
        if literal is None:
            literal = match.group("raw")
        if literal is None:
            continue
        rendered = unescape(PLACEHOLDER.sub(HOLE, literal))
        for line in rendered.splitlines():
            if is_rendered_form(line):
                shapes.add(collapse(line.strip()))
    # asm.goto composes the common assembly template with an edge suffix.
    # Inspect that actual composition; do not infer emission from the spec.
    goto = re.search(r'Inst::AsmGoto[\s\S]*?format!\(\s*"([^"]+)",\s*'
                     r'self\.assembly\(assembly, true\)', source)
    if goto and re.search(r'if is_goto\s*\{\s*"\.goto"', source):
        suffix = unescape(PLACEHOLDER.sub(HOLE, goto[1]))
        for base in tuple(shapes):
            if " = asm<> " in base:
                shapes.add(suffix.replace(HOLE, base.replace(" = asm", " = asm.goto"), 1))
    return shapes


def spec_block(marker: str) -> set[str]:
    spec = SPEC.read_text()
    if marker not in spec:
        return set()
    body = spec[spec.index(marker):]
    block = body[body.index("```text") + len("```text"):]
    return {collapse(line.strip())
            for line in block[: block.index("```")].splitlines() if line.strip()}


def main() -> int:
    source = production_code(RENDERER.read_text())
    emitted = renderer_shapes(source)
    specified = spec_block("#### 9.3.1 Canonical Shapes")
    pending = spec_block("#### 9.3.2 Shapes Not Yet Emitted")
    corrections = spec_block("### Implemented instruction shape corrections")
    specified |= corrections
    pending -= corrections
    superseded = spec_block("The form each replaces is **superseded**")

    undescribed = sorted(s for s in emitted if s not in specified | superseded)
    still_emitted = sorted(s for s in emitted if s in superseded)
    undeclared_gap = sorted(s for s in specified if s not in emitted and s not in pending)
    wrongly_pending = sorted(s for s in emitted if s in pending)

    print(f"renderer shapes:  {len(emitted)}")
    print(f"specified shapes: {len(specified)}")

    findings = 0
    for title, items in (
        ("renderer emits forms the specification does not describe", undescribed),
        ("renderer still emits superseded forms a conforming parser rejects", still_emitted),
        ("specified forms the renderer does not emit and that are not declared", undeclared_gap),
        ("forms declared as not yet emitted that the renderer already emits", wrongly_pending),
    ):
        if items:
            findings += len(items)
            print(f"\n{title} ({len(items)}):")
            for shape in items:
                print("  " + shape)

    print(f"\n{findings} disagreement(s) between renderer and specification")
    print("this tool covers instruction shapes only; see its docstring for what it does not check")
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
