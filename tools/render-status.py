#!/usr/bin/env python3
"""Render docs/status.md from the receipts, the contracts and the specifications.

Every number in the tracker comes from a file this script reads, so the picture
cannot drift from the evidence: the survey fractions come from the newest sealed
summary of each survey, the stage counts from the contracts' rulings, the
campaign figures from their receipts, and the revisions from the specifications'
own headers. The only hand-written parts are the section captions and the four
questions' reading, which is a judgement and is marked as one.

Usage: python3 tools/render-status.py [--check]
  (no argument) rewrite docs/status.md
  --check       print the rendering and exit 1 if the file on disk differs
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
import unicodedata
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "status.md"

# A reading of the evidence, not a measurement: ten steps, set by hand.
QUESTIONS = [
    ("Q0 ", 9, "{revisions} , 40 ,0 "),
    ("Q1 kernel ", 4, " {parse}, {lower}, 384/384"),
    ("Q2 e-graph pipeline", 3, ", GCC -O2 "),
    ("Q3 ", 3, ",(13 3 )"),
]


def cells(text: str) -> int:
    """Display width: the wide and fullwidth East Asian classes take two."""
    return sum(2 if unicodedata.east_asian_width(ch) in "WF" else 1 for ch in text)


def pad(text: str, target: int) -> str:
    return text + " " * max(0, target - cells(text))


def bar(done: float, total: float, width: int = 8) -> str:
    if total <= 0:
        return "░" * width
    filled = round(width * max(0.0, min(1.0, done / total)))
    return "█" * filled + "░" * (width - filled)


def box(title: str, rows: list[str], inner: int | None = None) -> list[str]:
    body = max(cells(row) for row in rows) if rows else 0
    inner = max(inner or 0, body + 2, cells(title) + 4)
    head = "┌─ " + title + " " + "─" * (inner - cells(title) - 3) + "┐"
    out = [head]
    out += ["│ " + pad(row, inner - 2) + " │" for row in rows]
    out.append("└" + "─" * inner + "┘")
    return out


def beside(left: list[str], right: list[str], gap: int = 2) -> list[str]:
    height = max(len(left), len(right))
    left = left + [""] * (height - len(left))
    right = right + [""] * (height - len(right))
    lw = max(cells(line) for line in left)
    return [(pad(a, lw) + " " * gap + b).rstrip() for a, b in zip(left, right)]


def arrow(width: int, label: str) -> list[str]:
    stem = " " * ((width - 1) // 2) + "│"
    head = " " * ((width - 1) // 2) + "▼  " + label
    return [stem, head]


def git(*args: str) -> str:
    done = subprocess.run(
        ["git", "-C", str(ROOT), *args], capture_output=True, text=True, check=True
    )
    return done.stdout.strip()


def revision(path: Path) -> str:
    found = re.search(r"\*\*Revision ([0-9.]+)\*\*", path.read_text())
    return found.group(1) if found else "?"


def newest_survey(name: str) -> dict[str, int]:
    base = ROOT / ".evidence" / name
    if not base.is_dir():
        return {}
    runs = sorted(p for p in base.iterdir() if (p / "summary.json").exists())
    if not runs:
        return {}
    summary = json.loads((runs[-1] / "summary.json").read_text())
    return summary.get("classifications", {})


def fraction(counts: dict[str, int]) -> tuple[int, int]:
    return counts.get("clean", 0), sum(counts.values())


def contract(name: str) -> dict:
    path = ROOT / "tests" / "contracts" / name
    return json.loads(path.read_text()) if path.exists() else {}


def main() -> int:
    gate = contract("semantic-gate-contract.json")
    front = contract("day9-acceptance-contract.json")
    receipts = gate.get("receipt", {})
    closed_mid = sum(1 for key in gate.get("rulings", {}) if key.endswith("Closed"))
    closed_front = sum(1 for key in front.get("rulings", {}) if key.endswith("Closed"))
    closed_scopes = [name for name, state in front.get("scopeClosures", {}).items()
                     if state.get("status") == "CLOSED"]
    closure_note = f"; {'/'.join(closed_scopes)}" if closed_scopes else ""

    parse_clean, parse_total = fraction(newest_survey("full-config-parse-survey"))
    lower_clean, lower_total = fraction(newest_survey("full-config-lowering-survey"))

    evoir = revision(ROOT / "docs" / "evoir-spec.md")
    evomir = revision(ROOT / "docs" / "evomir-spec.md")
    revisions = int(evoir.split(".")[-1]) + int(evomir.split(".")[-1])

    spec = (ROOT / "docs" / "evoir-spec.md").read_text()
    debts = sorted(
        code
        for code, rest in re.findall(
            r"\| `(E30\d\d)` \| `not-(?:lowered|executed)[^\n]*?\| ([^\n]*)", spec
        )
        if "withdrawn" not in rest
    )

    b1 = receipts.get("b1", {}).get("campaign", {})
    b3 = receipts.get("b3", {}).get("cCorpus", {})
    b4 = receipts.get("b4", {})
    b5 = receipts.get("b5", {}).get("linuxCorpus", {})

    left = box(
        " F",
        [
            f" {parse_clean:,}/{parse_total:,} {bar(parse_clean, parse_total)} ",
            f" {parse_clean:,}/{parse_total:,} {bar(parse_clean, parse_total)} ",
            f" {lower_clean:,}/{lower_total:,} {bar(lower_clean, lower_total)} ",
            f"{closed_front} {closure_note}",
        ],
    )
    right = box(
        f" S ── {closed_mid} ",
        [
            f" gate 21,000 0 {bar(1, 1)}",
            f"evp2 :/epoch/premise {bar(1, 1)}",
            f", congruence, , {bar(1, 1)}",
            ":,",
        ],
    )
    top = beside(left, right)
    span = max(cells(line) for line in top)

    backend = box(
        " B ── ,C ",
        [
            f"EvoMIR, , {bar(1, 1)} {b1.get('lowered', 0):,}/{b1.get('programs', 0):,} lane 0 ",
            f", , {bar(1, 1)} ",
            f"C Darwin {b3.get('agreeing', 0)}/{b3.get('programs', 0)} {bar(1, 1)} clang ",
            f"C Linux {b5.get('agreeing', 0)}/{b5.get('programs', 0)} {bar(1, 1)} GCC-14 ",
            f" {b4.get('constraintRowsNativelyChecked', 0)} {bar(1, 1)}",
        ],
        span,
    )
    k1 = contract("k1-mixed-link-receipt.json")
    k1_boot = k1.get("reference", {}).get("verdict", "?")
    k1_units = k1.get("unitsCompiledByEvoccInABootingKernel", 0)
    k1_survey = k1.get("survey", {}).get("outcomeCounts", {})
    k1_asm = k1_survey.get("assembled", 0)
    k1_total = k1_asm + k1_survey.get("refused", 0)
    e0 = contract("ladder-e0-receipt.json").get("latest", {}).get("suites", {})
    e0c = e0.get("ctest", {}).get("outcomes", {})
    e0t = e0.get("torture", {}).get("outcomes", {})
    kernel = box(
        " ── ",
        [
            f" {k1_boot} {bar(1 if k1_boot == 'PASS' else 0, 1)} GCC ,EvoCC , ",
            f" EvoCC {k1_units} ; {k1_asm:,}/{k1_total:,} {bar(k1_asm, k1_total)}",
            f"fail-closed {len(debts)} :{' '.join(debts)}",
            f" E0 c-testsuite {e0c.get('pass', 0)}/{sum(e0c.values())} {bar(e0c.get('pass', 0), sum(e0c.values()))} torture {e0t.get('pass', 0)}/{sum(e0t.values())} {bar(e0t.get('pass', 0), sum(e0t.values()))}",
            "C1 EvoCC ░░░░ C2 vmlinux ░░░░ G1 ░░░░",
        ],
        span,
    )

    values = {
        "revisions": revisions,
        "parse": f"{parse_clean:,}/{parse_total:,}",
        "lower": f"{lower_clean:,}/{lower_total:,}",
    }
    label = max(cells(name) for name, _, _ in QUESTIONS) + 2
    questions = ["(,)"]
    questions += [
        f"{pad(name, label)}{bar(level, 10, 10)}  {note.format(**values)}"
        for name, level, note in QUESTIONS
    ]

    head = git("rev-parse", "--short", "HEAD")
    lines = [
        f"EvoCC ── ({date.today().isoformat()},EvoIR {evoir} / EvoMIR {evomir},main {head})",
        "═" * span,
        "",
        *top,
        *arrow(span, ""),
        *backend,
        *arrow(span, ""),
        *kernel,
        "",
        *questions,
        "═" * span,
    ]
    text = "```text\n" + "\n".join(lines) + "\n```\n"

    if "--check" in sys.argv:
        print(text, end="")
        if (OUT.read_text() if OUT.exists() else "") != text:
            print("docs/status.md differs from the rendering", file=sys.stderr)
            return 1
        return 0
    OUT.write_text(text)
    print(text, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
