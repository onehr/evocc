#!/usr/bin/env python3
"""Render the README and paper figures from real compiler output and receipts.

Run from the repository root after a release build:

    python3 tools/render-readme.py

The terminal figure runs `evo check` on `examples/first_bad.c` and draws its
colored output. The pipeline figure has a light and a dark variant.
"""
import html
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images" / "readme"
EVO = ROOT / "target" / "frontend" / "release" / "evo"
EVOCC = ROOT / "target" / "frontend" / "release" / "evocc"

MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
ANSI = {"31": "#ff7b72", "32": "#7ee787", "33": "#d29922", "34": "#79c0ff",
        "35": "#d2a8ff", "36": "#39c5cf"}


def spans(line):
    """Split one line with ANSI SGR codes into (text, color, bold) parts."""
    parts, color, bold, pos = [], None, False, 0
    for match in re.finditer(r"\x1b\[([0-9;]*)m", line):
        if match.start() > pos:
            parts.append((line[pos:match.start()], color, bold))
        for code in (match.group(1) or "0").split(";"):
            if code in ("", "0"):
                color, bold = None, False
            elif code == "1":
                bold = True
            elif code in ANSI:
                color = ANSI[code]
        pos = match.end()
    if pos < len(line):
        parts.append((line[pos:], color, bold))
    return parts


def terminal(lines, title):
    """Draw a terminal window. Each line is a list of (text, color, bold)."""
    width = 48 + int(8.43 * max(sum(len(t) for t, _, _ in line) for line in lines))
    height = 56 + 21 * len(lines)
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
           f'viewBox="0 0 {width} {height}" role="img" aria-label="{html.escape(title)}">',
           f'<rect width="{width}" height="{height}" rx="10" fill="#0d1117" stroke="#30363d"/>',
           '<circle cx="20" cy="18" r="6" fill="#ff5f56"/><circle cx="40" cy="18" r="6" '
           'fill="#ffbd2e"/><circle cx="60" cy="18" r="6" fill="#27c93f"/>',
           f'<text x="{width // 2}" y="22" fill="#8b949e" font-family="{SANS}" font-size="12" '
           f'text-anchor="middle">{html.escape(title)}</text>']
    for row, line in enumerate(lines):
        y = 58 + 21 * row
        bold_attr = ' font-weight="700"'
        tspans = "".join(
            f'<tspan fill="{color or "#e6edf3"}"{bold_attr if bold else ""}>'
            f'{html.escape(text)}</tspan>' for text, color, bold in line)
        out.append(f'<text x="24" y="{y}" font-family="{MONO}" font-size="14" '
                   f'xml:space="preserve">{tspans}</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


def check_figure():
    command = "evo check first_bad.c"
    result = subprocess.run([str(EVOCC), "--check", "--color=always", "first_bad.c"],
                            cwd=ROOT / "examples", capture_output=True, text=True)
    output = (result.stdout + result.stderr).rstrip("\n").split("\n")
    lines = [[("$ ", "#7ee787", True), (command, None, True)]]
    lines += [spans(line) for line in output]
    return terminal(lines, "evo check")


THEMES = {
    "light": dict(box="#ffffff", edge="#d0d7de", title="#1f2328", sub="#59636e",
                  arrow="#8c959f", check="#1a7f37", rules="#8250df", prove="#0969da"),
    "dark": dict(box="#161b22", edge="#3d444d", title="#f0f6fc", sub="#9198a1",
                 arrow="#656c76", check="#3fb950", rules="#a371f7", prove="#4493f8"),
}
STAGES = [("C source", "your .c files"), ("C-HIR", "checked C semantics"),
          ("EvoIR", "explicit UB checks"), ("EvoMIR", "registers and ABI"),
          ("AArch64", "assembly text")]
LEFT, BOX, STEP = 16, 152, 176


def pill(x, y, text, color):
    w = 16 + int(6.6 * len(text))
    return (f'<rect x="{x - w // 2}" y="{y}" width="{w}" height="24" rx="12" fill="none" '
            f'stroke="{color}"/><text x="{x}" y="{y + 16}" fill="{color}" font-family="{SANS}" '
            f'font-size="12" text-anchor="middle">{html.escape(text)}</text>')


def pipeline(theme):
    t = THEMES[theme]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="888" height="216" '
           f'viewBox="0 0 888 216" role="img" aria-label="The EvoCC pipeline">']
    for i, (name, sub) in enumerate(STAGES):
        x = LEFT + STEP * i
        mid = x + BOX // 2
        out.append(f'<rect x="{x}" y="24" width="{BOX}" height="64" rx="10" fill="{t["box"]}" '
                   f'stroke="{t["edge"]}"/>')
        out.append(f'<text x="{mid}" y="51" fill="{t["title"]}" font-family="{SANS}" '
                   f'font-size="15" font-weight="600" text-anchor="middle">{name}</text>')
        out.append(f'<text x="{mid}" y="71" fill="{t["sub"]}" font-family="{SANS}" '
                   f'font-size="12" text-anchor="middle">{sub}</text>')
        if i:
            out.append(f'<path d="M{x - 21} 56 H{x - 5}" stroke="{t["arrow"]}" stroke-width="2"/>'
                       f'<path d="M{x - 3} 56 l-8 -5 v10 z" fill="{t["arrow"]}"/>')
    out.append(pill(LEFT + STEP + BOX // 2, 102, "evo check runs here", t["check"]))
    out.append(pill(LEFT + 2 * STEP + BOX // 2, 102, "proved rewrite rules", t["rules"]))
    start, end = LEFT + STEP, LEFT + 4 * STEP + BOX
    out.append(f'<path d="M{start} 154 v12 H{end} v-12" fill="none" stroke="{t["prove"]}" '
               f'stroke-width="2"/>')
    out.append(f'<text x="{(start + end) // 2}" y="194" fill="{t["prove"]}" font-family="{SANS}" '
               f'font-size="14" font-weight="600" text-anchor="middle">evo prove: Lean 4 checks '
               f'every stage, or EvoCC writes nothing</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


EFFORT = [("Silent bugs, short programs", "18 programs, 2 runs each", "agent-repair-5-replication-20261010.json", "agent-repair-4", True),
          ("Silent bugs, 722-line programs", "12 programs, 2 runs each", "agent-repair-5-scale-20261010.json", "agent-repair-5", None),
          ("Controls: GCC and UBSan report them", "6 programs, 2 runs each", "agent-repair-5-replication-20261010.json", "agent-repair-4", False)]
CONTROL_CODES = {"E3002", "E3004", "E3005"}


def effort_totals():
    import json
    rows = []
    for label, sub, receipt, corpus, silent in EFFORT:
        runs = json.loads((ROOT / "tests" / "contracts" / receipt).read_text())["runs"]
        total = {"c1": 0, "c3": 0}
        for run in runs:
            code = (ROOT / "tests" / "fixtures" / corpus / run["item"] / "class").read_text().strip()
            if silent is not None and (code in CONTROL_CODES) == silent:
                continue
            total[run["condition"]] += run["steps"]
        rows.append((label, sub, total["c1"], total["c3"]))
    return rows


def effort_chart(theme):
    t = THEMES[theme]
    base = "#afb8c1" if theme == "light" else "#6e7681"
    rows = effort_totals()
    peak = max(max(a, b) for _, _, a, b in rows)
    scale, x0 = 400 / peak, 300
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="820" height="300" viewBox="0 0 820 300" '
           f'role="img" aria-label="Agent steps to repair, GCC and UBSan against EvoCC">']
    for i, (label, sub, c1, c3) in enumerate(rows):
        y = 20 + 82 * i
        out.append(f'<text x="0" y="{y + 20}" fill="{t["title"]}" font-family="{SANS}" font-size="14" '
                   f'font-weight="600">{html.escape(label)}</text>')
        out.append(f'<text x="0" y="{y + 40}" fill="{t["sub"]}" font-family="{SANS}" font-size="12">'
                   f'{html.escape(sub)}</text>')
        for j, (value, color) in enumerate(((c1, base), (c3, t["prove"]))):
            w = int(value * scale)
            out.append(f'<rect x="{x0}" y="{y + 6 + 26 * j}" width="{w}" height="20" rx="3" fill="{color}"/>')
            note = f"{value} steps" if j == 0 else f"{value} steps ({100 * (c3 - c1) / c1:+.0f}%)".replace("-", "\u2212")
            out.append(f'<text x="{x0 + w + 8}" y="{y + 21 + 26 * j}" fill="{t["title"]}" '
                       f'font-family="{SANS}" font-size="13">{note}</text>')
    y = 268
    out.append(f'<rect x="{x0}" y="{y}" width="14" height="14" rx="2" fill="{base}"/>'
               f'<text x="{x0 + 20}" y="{y + 12}" fill="{t["sub"]}" font-family="{SANS}" font-size="13">'
               f'GCC warnings and UBSan</text>')
    out.append(f'<rect x="{x0 + 200}" y="{y}" width="14" height="14" rx="2" fill="{t["prove"]}"/>'
               f'<text x="{x0 + 220}" y="{y + 12}" fill="{t["sub"]}" font-family="{SANS}" font-size="13">'
               f'EvoCC</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "check.svg").write_text(check_figure())
    paper = ROOT / "docs" / "images" / "paper"
    paper.mkdir(parents=True, exist_ok=True)
    for theme in THEMES:
        (OUT / f"pipeline-{theme}.svg").write_text(pipeline(theme))
        (paper / f"agent-effort-{theme}.svg").write_text(effort_chart(theme))
    print("wrote docs/images/readme (check, pipeline) and docs/images/paper (agent-effort)")


if __name__ == "__main__":
    main()
