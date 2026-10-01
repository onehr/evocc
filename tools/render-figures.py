#!/usr/bin/env python3
"""Render the repository's figures as SVG and PNG from one palette.

Usage: python3 tools/render-figures.py        writes docs/images/*.svg and *.png
Requires rsvg-convert on PATH for the PNGs (the SVGs are written regardless).
"""

from __future__ import annotations

import html
import math
import shutil
import subprocess
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "docs" / "images"

BG, PANEL, PANEL2, STROKE = "#0b1117", "#111925", "#0e151f", "#232d3a"
TEXT, MUTED, GRAY = "#f0f3f6", "#9aa4b2", "#4a5462"
BLUE, CYAN, GREEN = "#58a6ff", "#79c0ff", "#3fb950"
RED, AMBER = "#f85149", "#d4a72c"
SANS = "Helvetica Neue, Helvetica, Arial, sans-serif"
MONO = "JetBrains Mono, Menlo, monospace"


class Canvas:
    def __init__(self, width: int, height: int) -> None:
        self.w, self.h = width, height
        self.parts: list[str] = []
        markers = "".join(
            f'<marker id="arr-{c[1:]}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" '
            f'markerHeight="7" orient="auto"><path d="M0 0L10 5L0 10z" fill="{c}"/></marker>'
            for c in (STROKE, BLUE, CYAN, GREEN, GRAY, RED, AMBER)
        )
        self.parts.append(
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
            f'viewBox="0 0 {width} {height}"><defs>{markers}'
            f'<linearGradient id="hair" x1="0" x2="1"><stop offset="0" stop-color="{BLUE}"/>'
            f'<stop offset="0.55" stop-color="{CYAN}"/><stop offset="1" stop-color="{GREEN}"/></linearGradient>'
            f'<radialGradient id="glow" cx="0.5" cy="0.5" r="0.5"><stop offset="0" stop-color="{BLUE}" '
            f'stop-opacity="0.16"/><stop offset="1" stop-color="{BLUE}" stop-opacity="0"/></radialGradient>'
            f'</defs><rect width="{width}" height="{height}" fill="{BG}"/>'
        )

    def text(self, x, y, s, size=15, fill=TEXT, weight="normal", family=SANS, anchor="start", opacity=1.0, spacing=0):
        self.parts.append(
            f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" font-weight="{weight}" '
            f'fill="{fill}" text-anchor="{anchor}" opacity="{opacity}" letter-spacing="{spacing}">{html.escape(s)}</text>'
        )

    def rect(self, x, y, w, h, fill=PANEL, stroke=STROKE, r=12, sw=1.2, opacity=1.0):
        self.parts.append(
            f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" fill="{fill}" stroke="{stroke}" '
            f'stroke-width="{sw}" opacity="{opacity}"/>'
        )

    def bar(self, x, y, h, color):
        self.parts.append(f'<rect x="{x + 9}" y="{y + 14}" width="3" height="{h - 28}" rx="1.5" fill="{color}"/>')

    def arrow(self, x1, y1, x2, y2, color=STROKE, sw=1.6, opacity=1.0):
        self.parts.append(
            f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{sw}" '
            f'opacity="{opacity}" marker-end="url(#arr-{color[1:]})"/>'
        )

    def line(self, x1, y1, x2, y2, color=STROKE, sw=1.2, opacity=1.0, dash=None):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        self.parts.append(
            f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{color}" stroke-width="{sw}" opacity="{opacity}"{d}/>'
        )

    def raw(self, s):
        self.parts.append(s)

    def seal(self, x, y, r=9):
        self.raw(
            f'<circle cx="{x}" cy="{y}" r="{r}" fill="{BG}" stroke="{GREEN}" stroke-width="1.4"/>'
            f'<path d="M{x - 4} {y} L{x - 1} {y + 3} L{x + 4.5} {y - 3.5}" fill="none" stroke="{GREEN}" '
            f'stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>'
        )

    def pill(self, cx, cy, s, color, opacity=1.0, size=17, fill=PANEL, strike=False, sw=1.4):
        w = 10.6 * len(s) * size / 17 + 34
        h = 40
        self.raw(f'<g opacity="{opacity}">')
        self.raw(f'<rect x="{cx - w / 2}" y="{cy - h / 2}" width="{w}" height="{h}" rx="20" fill="{fill}" stroke="{color}" stroke-width="{sw}"/>')
        self.raw(f'<text x="{cx}" y="{cy + 6}" font-family="{MONO}" font-size="{size}" fill="{TEXT}" text-anchor="middle">{html.escape(s)}</text>')
        if strike:
            self.raw(f'<line x1="{cx - w / 2 + 14}" y1="{cy}" x2="{cx + w / 2 - 14}" y2="{cy}" stroke="{color}" stroke-width="1.4"/>')
        self.raw("</g>")
        return w, h

    def chip(self, x, y, s, color):
        w = 8.6 * len(s) + 26
        self.raw(f'<rect x="{x}" y="{y}" width="{w}" height="28" rx="14" fill="{color}" fill-opacity="0.12" stroke="{color}" stroke-opacity="0.5"/>')
        self.text(x + w / 2, y + 19, s, 13, color, "500", MONO, "middle")
        return w

    def write(self, name: str) -> None:
        self.parts.append("</svg>")
        svg = OUT / f"{name}.svg"
        svg.write_text("\n".join(self.parts))
        if shutil.which("rsvg-convert"):
            subprocess.run(["rsvg-convert", "-w", str(self.w * 2), str(svg), "-o", str(OUT / f"{name}.png")], check=True)


def wrap(s: str, width: int) -> list[str]:
    lines, cur = [], ""
    for word in s.split():
        if len(cur) + len(word) + 1 > width:
            lines.append(cur)
            cur = word
        else:
            cur = (cur + " " + word).strip()
    lines.append(cur)
    return lines


def concept() -> None:
    c = Canvas(1200, 640)
    c.text(60, 84, "EvoCC", 46, TEXT, "700", spacing=-1)
    c.text(60, 128, "a C compiler whose optimizer keeps every equivalent form", 26, TEXT, "500")
    c.text(60, 162, "Written from scratch in Rust, no LLVM or GCC inside. A rewrite adds a form, a separate checker", 16, MUTED)
    c.text(60, 184, "verifies each union, and a cost model picks one at the end. Target: AArch64, the Linux 6.18 kernel.", 16, MUTED)
    lx = 220
    c.text(lx, 250, "a pass-based compiler", 14, MUTED, "500", SANS, "middle", 1, 1)
    chain = ["x * 2", "x + x", "(x << 1) | 0", "x << 1"]
    ys = [300, 366, 432, 498]
    for i, (s, y) in enumerate(zip(chain, ys)):
        last = i == len(chain) - 1
        w, h = c.pill(lx, y, s, GRAY, 1.0 if last else 0.3 + 0.18 * i, 18, strike=not last)
        if not last:
            c.arrow(lx, y + h / 2 + 2, lx, ys[i + 1] - h / 2 - 4, GRAY, 1.4, 0.4 + 0.15 * i)
    c.text(lx, 560, "each pass replaces the last form", 14, MUTED, "normal", SANS, "middle")
    c.text(lx, 580, "the others are gone", 14, MUTED, "normal", SANS, "middle")
    c.line(430, 250, 430, 590, STROKE)
    cx, cy, r = 760, 420, 138
    c.raw(f'<circle cx="{cx}" cy="{cy}" r="{r + 80}" fill="url(#glow)"/>')
    c.text(cx, 250, "EvoCC", 14, BLUE, "600", SANS, "middle", 1, 1)
    forms = ["x * 2", "x + x", "x << 1", "(x << 1) | 0", "2 * x"]
    n, chosen = len(forms), 2
    c.raw(f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="none" stroke="{BLUE}" stroke-opacity="0.35" stroke-width="1.2" stroke-dasharray="3 6"/>')
    c.raw(f'<circle cx="{cx}" cy="{cy}" r="7" fill="{BLUE}"/>')
    c.text(cx, cy + 32, "one e-class", 13, BLUE, "500", SANS, "middle")
    for i, s in enumerate(forms):
        ang = -math.pi / 2 + i * 2 * math.pi / n
        px, py = cx + r * math.cos(ang), cy + r * math.sin(ang)
        c.line(cx + 12 * math.cos(ang), cy + 12 * math.sin(ang), cx + (r - 34) * math.cos(ang), cy + (r - 34) * math.sin(ang), BLUE, 1.2, 0.7)
        c.seal(cx + (r / 2) * math.cos(ang), cy + (r / 2) * math.sin(ang), 8)
        c.pill(px, py, s, GREEN if i == chosen else "#2f4f7f", 1.0, 18, "#10231a" if i == chosen else PANEL, sw=2 if i == chosen else 1.4)
    ang = -math.pi / 2 + chosen * 2 * math.pi / n
    px, py = cx + r * math.cos(ang), cy + r * math.sin(ang)
    bx, by = 1000, py - 4
    c.arrow(px + 52, py, bx - 14, py, GREEN, 1.6)
    c.rect(bx, by - 34, 150, 88, PANEL, GREEN, 10, 1.4)
    c.text(bx + 14, by - 10, "picked by cost", 12, MUTED)
    c.text(bx + 14, by + 14, "lsl x0, x0, #1", 15, GREEN, "normal", MONO)
    c.text(bx + 14, by + 38, "AArch64", 12, MUTED)
    c.seal(470, 611, 8)
    c.text(486, 616, "each union has a certificate; evo-proofcheck, a separate program, re-checks it", 14, MUTED)
    c.write("evocc-concept")


def overview() -> None:
    c = Canvas(1600, 960)
    c.text(60, 70, "EvoCC", 44, TEXT, "700")
    c.text(60, 100, "a C compiler written from scratch in Rust; e-graph optimizer with certificates; target AArch64 and the Linux 6.18 kernel", 18, MUTED)
    c.raw('<rect x="60" y="116" width="1480" height="2" fill="url(#hair)" opacity="0.8"/>')
    x = 60
    for s, col in (("Rust workspace, 16 crates", CYAN), ("no LLVM, no GCC in the pipeline", BLUE), ("AArch64: Linux + Darwin", BLUE), ("each rewrite has a certificate", GREEN), ("numbers come from receipts", GREEN)):
        x += c.chip(x, 132, s, col) + 12
    nodes = [
        ("C / GNU C11", ["Linux 6.18.44", "2,798 translation units"], GRAY, SANS),
        ("EvoCPP", ["preprocessor", "C11 pp-tokens", "the GCC dialect"], BLUE, MONO),
        ("C-HIR", ["checked high-level IR", "typed UB, provenance,", "per-byte initializedness"], BLUE, MONO),
        ("EvoIR", ["block-argument SSA", "mem / world tokens", "ub.assume obligations"], CYAN, MONO),
        ("e-graph", ["rewrites add forms,", "never replace them", "cost is a policy"], CYAN, SANS),
        ("EvoMIR", ["AArch64 machine IR", "linear scan + verifier", "128-bit, varargs, asm"], GREEN, MONO),
        ("Assembly", ["platform as and ld", "nothing else outside"], GREEN, SANS),
        ("Runs", ["Linux AArch64 in QEMU", "Darwin arm64 natively"], GRAY, SANS),
    ]
    nx, nw, nh, ny, gap = 60, 150, 132, 216, 40
    for a, b, label, col in ((1, 2, "front end", BLUE), (3, 4, "semantics and equality", CYAN), (5, 6, "backend", GREEN)):
        xa, xb = nx + a * (nw + gap), nx + b * (nw + gap) + nw
        c.line(xa, ny - 12, xb, ny - 12, col, 1, 0.6)
        c.text((xa + xb) / 2, ny - 20, label, 13, col, "600", SANS, "middle")
    for i, (name, subs, col, fam) in enumerate(nodes):
        x = nx + i * (nw + gap)
        c.rect(x, ny, nw, nh)
        c.raw(f'<rect x="{x + 12}" y="{ny}" width="{nw - 24}" height="3" rx="1.5" fill="{col}"/>')
        c.text(x + nw / 2, ny + 42, name, 21, TEXT, "700", fam, "middle")
        for j, s in enumerate(subs):
            c.text(x + nw / 2, ny + 68 + j * 19, s, 13, MUTED, "normal", SANS, "middle")
        if i < len(nodes) - 1:
            c.arrow(x + nw + 4, ny + nh / 2, x + nw + gap - 4, ny + nh / 2, col if col != GRAY else STROKE)
    ty = 392
    c.text(60, ty + 40, "Three", 17, TEXT, "600")
    c.text(60, ty + 62, "checks", 17, TEXT, "600")
    for j, s in enumerate(("independent of", "the optimizer", "and of each other")):
        c.text(60, ty + 88 + j * 16, s, 12.5, MUTED)
    boxes = [
        (2, "Two interpreters must agree", ["C-HIR interpreter and EvoIR interpreter are", "separate implementations; every corpus program", "runs through both, and a difference is a defect"], [2, 3]),
        (4, "Certificates, checked independently", ["each e-graph union emits an EVP2 certificate;", "evo-proofcheck replays it against the before and", "after programs and shares no code with the optimizer"], [4]),
        (6, "Random programs, four ways", ["21,000 generated programs, 0 mismatches, run as", "source IR, optimized IR, machine IR and native", "binary on both targets"], [5, 7]),
    ]
    for first, title, lines_, feeds in boxes:
        bx, bw = nx + first * (nw + gap), 2 * nw + gap
        c.rect(bx, ty + 8, bw, 110, PANEL2)
        c.bar(bx, ty + 8, 110, GREEN)
        c.text(bx + 24, ty + 36, title, 16, TEXT, "600")
        for j, s in enumerate(lines_):
            c.text(bx + 24, ty + 60 + j * 18, s, 13, MUTED)
        for ni in feeds:
            sx = nx + ni * (nw + gap) + nw / 2
            c.arrow(sx, ny + nh + 4, sx, ty + 2, GREEN, 1.4)
    by = 552
    c.text(60, by - 6, "How the work is done", 17, TEXT, "600")
    cols = [
        ("Rules come from the specification", BLUE, ["EvoIR spec 1.9.68 and EvoMIR spec 0.15 hold every rule", "before a rule is written, the behaviour is checked in a", "sealed container with GCC 14.2 and Clang 19.1", "when the two disagree, the rule refuses the construct", "anything unspecified is refused with a numbered diagnostic"]),
        ("How the AI agents work", CYAN, ["tests first: a commit whose failures are listed in advance,", "then the implementation, then a receipt with the numbers", "an agent that finds a gap in the spec stops and reports;", "it can run the reference compilers, it cannot guess", "every status number is produced by a script"]),
        ("How it is measured", GREEN, ["survey: compile all 2,798 kernel units, group them by the", "first diagnostic, write one rule per group, repeat", "kernel: GCC builds it, EvoCC replaces units, kbuild relinks,", "QEMU boots it, a failure is bisected to one unit", "programs: c-testsuite, GCC torture, zlib, zstd, Mbed TLS, musl"]),
    ]
    cw = (1480 - 2 * 24) / 3
    for i, (title, col, lines_) in enumerate(cols):
        cx = 60 + i * (cw + 24)
        c.rect(cx, by + 8, cw, 166, PANEL2)
        c.bar(cx, by + 8, 166, col)
        c.text(cx + 26, by + 38, title, 16, TEXT, "600")
        for j, s in enumerate(lines_):
            c.text(cx + 26, by + 63 + j * 21, s, 13.5, MUTED)
    sy = 766
    c.text(60, sy - 6, "Numbers on 2026-09-06", 17, TEXT, "600")
    stats = [
        ("2,798 / 2,798", "kernel units preprocess and parse clean", BLUE),
        ("30 / 2,798", "kernel units lower to EvoIR with no diagnostic; the rest stop at a known construct", CYAN),
        ("384 + 384", "C programs agree with clang on Darwin and with GCC-14 on Linux, source to native", GREEN),
        ("12", "kernel units compiled by EvoCC inside a kernel that boots; GCC compiled the rest", BLUE),
        ("187 / 220", "c-testsuite programs pass; 725 of 1,692 GCC torture programs pass; failures are filed by construct", GREEN),
    ]
    sw = (1480 - 4 * 16) / 5
    for i, (big, small, col) in enumerate(stats):
        sx = 60 + i * (sw + 16)
        c.rect(sx, sy + 8, sw, 112, PANEL2)
        c.text(sx + 18, sy + 48, big, 27, col, "700", MONO)
        for j, s in enumerate(wrap(small, 36)[:3]):
            c.text(sx + 18, sy + 71 + j * 17, s, 12.5, MUTED)
    c.text(60, 930, "github.com/onehr/evocc", 14, MUTED, "normal", MONO)
    c.text(1540, 930, "docs/status.md is rendered from the receipts", 13, MUTED, "normal", SANS, "end")
    c.write("evocc-overview")


def ladder() -> None:
    c = Canvas(1600, 620)
    c.text(60, 64, "The evaluation ladder", 34, TEXT, "700")
    c.text(60, 96, "each program is pinned to a release and judged by its own test suite", 17, MUTED)
    groups = [
        ("compiler test suites", BLUE, [("E0a", "GCC C torture", "GCC 16.2", "1,265 / 1,692 pass"), ("E0b", "c-testsuite", "5c7275656d75", "207 / 220 pass"), ("E0c", "Csmith, no float", "0cdc710315cf", "932 / 1,000 agree, 0 wrong")]),
        ("integer programs", CYAN, [("E1", "zlib", "1.3.2", "19 / 19 units pass"), ("E2", "zstd", "1.5.7", "2 / 44 units"), ("E4", "Mbed TLS", "4.2.0", "selftest"), ("E5", "TinyCC", "0.9.27", "7 / 11 units"), ("E6", "toybox", "0.8.14", "195 / 203 units"), ("E7", "musl + libc-test", "1.2.6", "494 / 1,339 units")]),
        ("after the floating-point rung F1", GRAY, [("E3", "cJSON", "1.7.19", "0 / 2 units, needs FP"), ("E8", "PolyBench/C", "4.2.1-beta", "reference outputs"), ("E9", "llvm-test-suite", "llvmorg-23.1.0", "SingleSource"), ("E10", "jq", "1.8.2", "make check"), ("E11", "Lua", "5.5.1", "official suite"), ("E12", "SQLite", "3.53.4", "TH3 / TCL")]),
        ("Linux 6.18.44", GREEN, [("K5", "lib/ + kernel/", "mixed link", "76 units boot"), ("K6", "full configuration", "EvoCC alone", "boots to init"), ("K7", "kselftest", "in initramfs", "pass rate")]),
    ]
    x0, y_track = 60, 330
    total = sum(len(g[2]) for g in groups)
    step = (1480 - 40 * (len(groups) - 1)) / total
    c.line(x0, y_track, 1540, y_track, STROKE, 2)
    x = x0
    k = 0
    for name, col, rungs in groups:
        gx0 = x
        for code, subject, pin, oracle in rungs:
            cx = x + step / 2
            above = k % 2 == 0
            done = code in ("E0a", "E0b", "E0c", "E1", "E2", "E3", "E5", "E6", "E7", "K5")
            c.raw(f'<circle cx="{cx}" cy="{y_track}" r="8" fill="{col if done else BG}" stroke="{col}" stroke-width="2"/>')
            ty = y_track - 100 if above else y_track + 40
            c.line(cx, y_track - 10 if above else y_track + 10, cx, y_track - 30 if above else y_track + 30, col, 1, 0.6)
            c.text(cx, ty, code, 13, col, "700", MONO, "middle")
            c.text(cx, ty + 20, subject, 15, TEXT, "600", SANS, "middle")
            c.text(cx, ty + 38, pin, 12, MUTED, "normal", MONO, "middle")
            c.text(cx, ty + 54, oracle, 12, GREEN if done else MUTED, "normal", SANS, "middle")
            x += step
            k += 1
        c.line(gx0 + 6, y_track + 120, x - 6, y_track + 120, col, 1.2, 0.8)
        c.text((gx0 + x) / 2, y_track + 142, name, 13, col, "600", SANS, "middle", 1, 1)
        x += 40
    c.text(60, 520, "filled: measured at least once, with its reading. For each program the receipt records the units EvoCC compiled in a passing build and, for the rest, the diagnostic that kept them out.", 13, MUTED)
    c.text(60, 544, "the backend has no floating point yet; the programs after F1 wait for it.", 13, MUTED)
    c.text(60, 590, "github.com/onehr/evocc  ·  docs/roadmap.md", 14, MUTED, "normal", MONO)
    c.write("evocc-ladder")


def milestones() -> None:
    c = Canvas(1200, 330)
    c.text(60, 52, "Milestones, in order", 26, TEXT, "700")
    c.text(60, 78, "no dates; each is done when its receipt says so", 15, MUTED)
    items = [
        ("parse the kernel", "2,798 / 2,798", "done"),
        ("backend", "AArch64, two targets", "done"),
        ("kernel objects boot", "76 units by EvoCC", "done"),
        ("rules proved", "55 rules, Lean 4", "done"),
        ("lower every unit", "90 / 2,798 so far", "now"),
        ("machine forms", "asm, alignment, 128-bit", "now"),
        ("outside programs", "torture 1,265 / 1,692", "now"),
        ("most kernel units", "by EvoCC, boots", "next"),
        ("whole kernel", "EvoCC alone, boots", "later"),
        ("floating point", "then SQLite, Lua", "later"),
    ]
    x0, x1, y = 120, 1080, 190
    step = (x1 - x0) / (len(items) - 1)
    c.line(x0, y, x1, y, STROKE, 2)
    colors = {"done": GREEN, "now": BLUE, "next": BLUE, "later": GRAY}
    for i, (title, sub, state) in enumerate(items):
        x = x0 + i * step
        col = colors[state]
        if state == "done":
            c.raw(f'<circle cx="{x}" cy="{y}" r="9" fill="{col}"/>')
        elif state == "now":
            c.raw(f'<circle cx="{x}" cy="{y}" r="9" fill="{BG}" stroke="{col}" stroke-width="2.5"/><circle cx="{x}" cy="{y}" r="4" fill="{col}"/>')
        else:
            c.raw(f'<circle cx="{x}" cy="{y}" r="9" fill="{BG}" stroke="{col}" stroke-width="2"/>')
        above = i % 2 == 0
        ty = y - 44 if above else y + 44
        c.line(x, y - 11 if above else y + 11, x, y - 26 if above else y + 26, col, 1, 0.7)
        c.text(x, ty, title, 15, TEXT, "600", SANS, "middle")
        c.text(x, ty + 20, sub, 12.5, MUTED, "normal", SANS, "middle")
    lx = 60
    for label, col, filled in (("done", GREEN, True), ("in progress", BLUE, False), ("next", BLUE, False), ("later", GRAY, False)):
        c.raw(f'<circle cx="{lx + 8}" cy="292" r="6" fill="{col if filled else BG}" stroke="{col}" stroke-width="2"/>')
        if label == "in progress":
            c.raw(f'<circle cx="{lx + 8}" cy="292" r="2.5" fill="{col}"/>')
        c.text(lx + 22, 297, label, 13, MUTED)
        lx += 105
    c.text(1140, 297, "docs/roadmap.md", 13, MUTED, "normal", MONO, "end")
    c.write("evocc-milestones")


def eqsat() -> None:
    """Verified EqSat: the path of one translation unit, the trust layers, the numbers."""
    c = Canvas(1600, 1010)
    c.text(60, 64, "Verified EqSat", 40, TEXT, "700")
    c.text(60, 94, "every rewrite carries a certificate that a separate program replays; nothing optimized is published on the optimizer's own authority", 17, MUTED)
    c.raw('<rect x="60" y="110" width="1480" height="2" fill="url(#hair)" opacity="0.8"/>')

    # offline Lean audit
    lx, ly, lw, lh = 560, 130, 480, 66
    c.rect(lx, ly, lw, lh, PANEL2, GREEN, 10, 1.4)
    c.text(lx + lw / 2, ly + 27, "Lean 4 audit, offline; never runs at compile time", 16, GREEN, "600", SANS, "middle")
    c.text(lx + lw / 2, ly + 49, "registry text in the spec → generated rule data → RuleCovered proofs → audit receipt", 12.5, MUTED, "normal", SANS, "middle")

    nx, nw, nh, ny, gap = 70, 200, 120, 250, 52
    stations = [
        ("1 · LOWERING", "C → EvoIR", ["ordinary -O2 folds, CSE,", "block joins; every C check", "explicit as ub.assume"], STROKE),
        ("2 · CHAIN PRODUCER", "transaction k ≤ 8", ["eligible scalar regions", "p0.scalar.v4, known bits", "policy accepts or rejects"], STROKE),
        ("3 · CERTIFICATE", "EVP bundle", ["before.evo · after.evo", "proof.evp: every union,", "binding and premise"], STROKE),
        ("4 · CHECKER", "independent replay", ["own eligibility and facts", "whole-module splice", "binary pinned by SHA-256"], BLUE),
        ("5 · PUBLICATION", "chain VERIFIED", ["immutable chain record", "retained bundles", "state committed atomically"], STROKE),
        ("6 · BACKEND (tested)", "EvoMIR twins", ["checked: ub.assume → trap", "unchecked: only after the", "checked twin passed"], STROKE),
    ]
    for i, (eyebrow, name, subs, col) in enumerate(stations):
        x = nx + i * (nw + gap)
        c.rect(x, ny, nw, nh, PANEL, col, 10, 1.8 if col != STROKE else 1.2)
        c.text(x + 16, ny + 24, eyebrow, 11.5, GRAY, "600", SANS, "start", 1.0, 0.6)
        c.text(x + 16, ny + 50, name, 16 if i == 1 else 18, TEXT, "700", MONO if i in (1, 2, 5) else SANS)
        for j, s_ in enumerate(subs):
            c.text(x + 16, ny + 74 + j * 17, s_, 12.5, MUTED)
        if i < len(stations) - 1:
            c.arrow(x + nw + 4, ny + nh / 2, x + nw + gap - 4, ny + nh / 2, STROKE)
    labels = [("SHA-256", "input digest"), ("rewrites and", "fact premises"), ("three files,", "nothing else"), ("exit 0", "per transaction"), ("one verified IR,", "two native profiles")]
    for i, (a, b) in enumerate(labels):
        cx = nx + i * (nw + gap) + nw + gap / 2
        c.text(cx, ny + nh + 22, a, 12, GRAY, "normal", SANS, "middle")
        c.text(cx, ny + nh + 38, b, 12, GRAY, "normal", SANS, "middle")
    # registry digest into producer and checker
    for tx in (nx + 1 * (nw + gap) + nw / 2, nx + 3 * (nw + gap) + nw / 2):
        sx = 700 if tx < 800 else 900
        c.raw(f'<path d="M{sx} {ly + lh} C{sx} {ly + lh + 30} {tx} {ny - 34} {tx} {ny - 4}" fill="none" stroke="{GREEN}" stroke-width="1.4" stroke-dasharray="6 4" marker-end="url(#arr-{GREEN[1:]})"/>')
    c.text(950, ny - 14, "same registry digest, compiled into producer and checker", 12.5, GREEN, "normal", SANS, "start")
    # loop back and reject branch
    x2 = nx + 1 * (nw + gap) + nw / 2
    x4 = nx + 3 * (nw + gap) + 60
    c.raw(f'<path d="M{x4} {ny + nh} C{x4} {ny + nh + 100} {x2 + 40} {ny + nh + 100} {x2} {ny + nh + 6}" fill="none" stroke="{STROKE}" stroke-width="1.6" marker-end="url(#arr-{STROKE[1:]})"/>')
    c.text(620, ny + nh + 126, "exit 0 and changed → next transaction (≤ 8); unchanged → terminal no-op proof, then publish", 13, MUTED, "normal", SANS, "middle")
    rx, rw = 900, 470
    c.arrow(nx + 3 * (nw + gap) + 170, ny + nh + 4, nx + 3 * (nw + gap) + 170, ny + nh + 60, RED, 1.6)
    c.rect(rx, ny + nh + 62, rw, 58, PANEL2, RED, 10, 1.4)
    c.text(rx + rw / 2, ny + nh + 86, "exit 2 malformed · exit 3 invalid · timeout · crash", 13.5, RED, "600", SANS, "middle")
    c.text(rx + rw / 2, ny + nh + 106, "whole chain REJECTED, staged state discarded, nothing emitted", 12.5, MUTED, "normal", SANS, "middle")

    # trust layers
    by0 = 536
    bands = [
        ("PROVED IN LEAN", ["offline, per registry revision;", "kernel-only except nine", "enumerated analysis rows"], GREEN, [
            ("55 rule equalities", ["RuleCovered: every width", "environment, stated facts/guards"]),
            ("known-bits analysis", ["transfers, meet/join, covers,", "known-zero; add/sub/mul/icmp SAT"]),
            ("audit receipt", ["axioms per theorem, Lean 4.32.2,", "registry digest 1a539160…"]),
        ]),
        ("REPLAYED PER COMPILE", ["independent Rust checker,", "separate process, pinned", "binary, closed exit classes"], BLUE, [
            ("every union", ["is an admitted rule firing", "or a congruence"]),
            ("facts recomputed", ["eligibility, taint, seed facts", "re-derived; premises checked"]),
            ("whole-module splice", ["after.evo must equal the", "rebuilt module byte for byte"]),
            ("chain and identity", ["adjacent digests, trusted", "SHA-256, 45 legal state tuples"]),
        ]),
        ("TESTED, NOT PROVED", ["differential and corpus", "evidence; the trusted", "implementation base"], AMBER, [
            ("Rust ↔ Lean model", ["1,325,045 differential vectors", "agree (evidence, not proof)"]),
            ("parser · lowering · facts", ["21,000-program scalar and MIR", "gates, 0 mismatches"]),
            ("backend · allocation · ABI", ["384/384 native corpora, Linux", "and Darwin; kernel boot"]),
        ]),
    ]
    bh, bgap = 92, 14
    for bi, (title, subs, col, items) in enumerate(bands):
        y = by0 + bi * (bh + bgap)
        c.rect(70, y, 1460, bh, PANEL2, col, 10, 1.4)
        c.text(90, y + 28, title, 14, col, "700", SANS, "start", 1.0, 0.6)
        for j, s_ in enumerate(subs):
            c.text(90, y + 49 + j * 15, s_, 11.5, MUTED)
        iw = 290
        for k, (name, lines_) in enumerate(items):
            ix = 320 + k * (iw + 18)
            c.rect(ix, y + 12, iw, bh - 24, PANEL, STROKE, 7, 1.0)
            c.text(ix + 14, y + 35, name, 14, TEXT, "600")
            for j, s_ in enumerate(lines_):
                c.text(ix + 14, y + 54 + j * 15, s_, 11.5, MUTED)
    # arrows between layers
    c.raw(f'<path d="M1390 {by0 + bh} V{by0 + bh + bgap - 4}" stroke="{GREEN}" stroke-width="1.4" stroke-dasharray="6 4" fill="none" marker-end="url(#arr-{GREEN[1:]})"/>')
    c.raw(f'<path d="M445 {by0 + bh} V{by0 + bh + bgap - 4}" stroke="{GREEN}" stroke-width="1.4" stroke-dasharray="6 4" fill="none" marker-end="url(#arr-{GREEN[1:]})"/>')
    c.raw(f'<path d="M445 {by0 + 2 * (bh + bgap)} V{by0 + 2 * (bh + bgap) - bgap + 4}" stroke="{AMBER}" stroke-width="1.4" fill="none" marker-end="url(#arr-{AMBER[1:]})"/>')

    # numbers
    sy = 868
    c.text(70, sy - 8, "Numbers on 2026-10-01, from receipts", 15, TEXT, "600")
    stats = [
        ("55 / 55", "rules proved in Lean 4, kernel-only; 9 analysis rows SAT-backed", GREEN),
        ("8,854 + 138", "certificates replayed by the independent checker, 0 rejections", BLUE),
        ("1,024", "policy evaluations: best policy beats the default by at most 4 bytes", CYAN),
        ("−0.8 … −2.4 %", "object .text on changed zlib, zstd, cJSON units; latency not measurable", GREEN),
        ("NOT_ACCEPTED", "G2 whole-kernel, Q2 versus GCC, Q3 AI improvement; AI campaign not admitted", RED),
    ]
    sw_ = (1460 - 4 * 16) / 5
    for i, (big, small, col) in enumerate(stats):
        sx = 70 + i * (sw_ + 16)
        c.rect(sx, sy + 4, sw_, 100, PANEL2)
        c.text(sx + 16, sy + 40, big, 24, col, "700", MONO)
        for j, s_ in enumerate(wrap(small, 34)[:3]):
            c.text(sx + 16, sy + 62 + j * 15, s_, 12, MUTED)
    c.text(70, 992, "docs/framing-draft.md · docs/notebook/eqsat-*.md · tests/contracts/eqsat-*.json · tests/fixtures/eqsat-chains/verified-chain", 12.5, MUTED, "normal", MONO)
    c.text(1530, 992, "the claim ceiling is the weakest layer a result depends on; this is not a verified compiler", 12.5, MUTED, "normal", SANS, "end")
    c.write("evocc-eqsat")


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    concept()
    overview()
    ladder()
    milestones()
    eqsat()
    print("rendered:", ", ".join(p.name for p in sorted(OUT.iterdir()) if p.suffix in (".svg", ".png")))
