#!/usr/bin/env python3
"""Render the README figures.

One palette, one geometry vocabulary, four figures. Built things are solid and
coloured; unbuilt things are dashed and muted, so a reader can see the shape of
the compiler and how much of it exists without reading a word.

    python3 tools/render-readme-figures.py
"""

from __future__ import annotations

import argparse
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
from matplotlib import font_manager  # noqa: E402
from matplotlib.patches import FancyArrowPatch, FancyBboxPatch  # noqa: E402
import matplotlib.pyplot as plt  # noqa: E402

INK = "#0d1117"
PANEL = "#161b22"
EDGE = "#30363d"
TEXT = "#e6edf3"
MUTED = "#8b949e"
FAINT = "#6e7681"
GREEN = "#3fb950"
AMBER = "#d29922"
BLUE = "#58a6ff"
PURPLE = "#bc8cff"
RED = "#f85149"
TEAL = "#39c5cf"

SANS = next(
    (n for n in ("Helvetica Neue", "Helvetica", "Arial", "DejaVu Sans")
     if any(f.name == n for f in font_manager.fontManager.ttflist)),
    "DejaVu Sans",
)
MONO = next(
    (n for n in ("SF Mono", "Menlo", "Monaco", "DejaVu Sans Mono")
     if any(f.name == n for f in font_manager.fontManager.ttflist)),
    "DejaVu Sans Mono",
)


def canvas(width: float, height: float):
    fig = plt.figure(figsize=(width, height), dpi=200)
    fig.patch.set_facecolor(INK)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_xlim(0, 100)
    ax.set_ylim(0, 100 * height / width)
    ax.set_facecolor(INK)
    ax.set_xticks([])
    ax.set_yticks([])
    for side in ax.spines.values():
        side.set_visible(False)
    return fig, ax


def box(ax, x, y, w, h, label, *, sub=None, colour=BLUE, built=True, mono=False, fs=9):
    """A rounded node. Unbuilt nodes are dashed and drained of colour."""
    face = PANEL if built else INK
    line = colour if built else EDGE
    ax.add_patch(
        FancyBboxPatch(
            (x, y), w, h,
            boxstyle="round,pad=0,rounding_size=1.1",
            linewidth=1.6 if built else 1.1,
            edgecolor=line,
            facecolor=face,
            linestyle="solid" if built else (0, (4, 3)),
            zorder=2,
        )
    )
    if built:
        ax.add_patch(
            FancyBboxPatch(
                (x, y), 0.55, h,
                boxstyle="round,pad=0,rounding_size=0.4",
                linewidth=0, facecolor=colour, alpha=0.95, zorder=3,
            )
        )
    ax.text(
        x + w / 2, y + h / 2 + (0.9 if sub else 0), label,
        ha="center", va="center", color=TEXT if built else FAINT,
        fontsize=fs, fontfamily=MONO if mono else SANS,
        fontweight="medium" if built else "normal", zorder=4,
    )
    if sub:
        ax.text(
            x + w / 2, y + h / 2 - 1.5, sub,
            ha="center", va="center", color=MUTED if built else FAINT,
            fontsize=fs - 2.2, fontfamily=SANS, zorder=4,
        )


def arrow(ax, start, end, *, colour=EDGE, style="-|>", dashed=False, rad=0.0, lw=1.4):
    ax.add_patch(
        FancyArrowPatch(
            start, end,
            arrowstyle=style, mutation_scale=11,
            linewidth=lw, color=colour, zorder=1,
            linestyle=(0, (4, 3)) if dashed else "solid",
            connectionstyle=f"arc3,rad={rad}",
            shrinkA=3, shrinkB=3,
        )
    )


def title(ax, x, y, text, sub=None, size=15):
    ax.text(x, y, text, ha="left", va="top", color=TEXT, fontsize=size,
            fontfamily=SANS, fontweight="bold")
    if sub:
        ax.text(x, y - 3.1, sub, ha="left", va="top", color=MUTED,
                fontsize=8.6, fontfamily=SANS)


def caption(ax, x, y, text, colour=MUTED, size=8.2, ha="left", mono=False):
    ax.text(x, y, text, ha=ha, va="center", color=colour, fontsize=size,
            fontfamily=MONO if mono else SANS)


def legend(ax, x, y, items):
    for i, (colour, text, built) in enumerate(items):
        cx = x + i * 21
        ax.add_patch(
            FancyBboxPatch(
                (cx, y - 0.7), 2.0, 1.5,
                boxstyle="round,pad=0,rounding_size=0.4",
                linewidth=1.2, edgecolor=colour if built else EDGE,
                facecolor=colour if built else INK,
                linestyle="solid" if built else (0, (3, 2)),
            )
        )
        ax.text(cx + 2.9, y, text, ha="left", va="center", color=MUTED,
                fontsize=8, fontfamily=SANS)


# ---------------------------------------------------------------- architecture
def figure_architecture(out: Path) -> None:
    fig, ax = canvas(13, 6.0)
    title(ax, 5, 44,
          "One pipeline, no LLVM, no GCC",
          "Solid is built and gated. Dashed is not built yet.")

    row, h = 25.0, 6.4
    stages = [
        (5.0, 9.0, "C / GNU C11", None, TEAL, True, False),
        (16.0, 10.0, "EvoCPP", "preprocessor", BLUE, True, True),
        (28.0, 10.0, "C-HIR", "checked, spans", BLUE, True, True),
        (40.0, 12.5, "EvoIR", "block-arg SSA", BLUE, True, True),
        (56.0, 11.0, "extraction", "cost model", PURPLE, False, False),
        (70.0, 10.0, "EvoMIR", "regalloc", AMBER, False, True),
        (83.0, 12.0, "AArch64 asm", None, AMBER, False, False),
    ]
    for x, w, label, sub, colour, built, mono in stages:
        box(ax, x, row, w, h, label, sub=sub, colour=colour, built=built, mono=mono)
    for i, (a, b) in enumerate([(14.0, 16.0), (26.0, 28.0), (38.0, 40.0),
                                (52.5, 56.0), (67.0, 70.0), (80.0, 83.0)]):
        arrow(ax, (a, row + h / 2), (b, row + h / 2),
              colour=EDGE if i < 3 else FAINT, dashed=i >= 3)

    # equality engine above EvoIR, proofs to its right: no arrow crosses a node
    prow = 36.0
    box(ax, 40.0, prow, 12.5, h, "e-graph", sub="alternatives kept", colour=PURPLE, built=True)
    arrow(ax, (43.0, row + h), (43.0, prow), colour=PURPLE)
    arrow(ax, (49.5, prow), (49.5, row + h), colour=PURPLE)
    box(ax, 58.0, prow, 11.0, h, "EVP1", sub="certificate", colour=PURPLE, built=True, mono=True)
    box(ax, 74.0, prow, 17.0, h, "evo-proofcheck", sub="independent replay",
        colour=PURPLE, built=True, mono=True)
    arrow(ax, (52.5, prow + h / 2), (58.0, prow + h / 2), colour=PURPLE)
    arrow(ax, (69.0, prow + h / 2), (74.0, prow + h / 2), colour=PURPLE)
    caption(ax, 82.5, 33.4, "does not trust the optimizer", colour=FAINT, ha="center")

    # the two authorities, each beneath the form it executes
    irow = 11.0
    box(ax, 24.0, irow, 15.0, 6.0, "C-HIR interpreter", colour=GREEN, built=True, fs=8.6)
    box(ax, 44.0, irow, 15.0, 6.0, "EvoIR interpreter", colour=GREEN, built=True, fs=8.6)
    arrow(ax, (32.0, row), (32.0, irow + 6.0), colour=EDGE)
    arrow(ax, (47.0, row), (50.0, irow + 6.0), colour=EDGE)
    arrow(ax, (39.0, irow + 3.0), (44.0, irow + 3.0), colour=GREEN, style="<|-|>")
    caption(ax, 61.0, irow + 3.0, "differential: the two must agree on every corpus program",
            colour=MUTED)

    legend(ax, 5, 4.0, [(BLUE, "front end", True), (PURPLE, "equality and proofs", True),
                        (GREEN, "verification", True), (EDGE, "not built", False)])
    fig.savefig(out, facecolor=INK)
    plt.close(fig)


# ------------------------------------------------------------------------ why
def figure_why(out: Path) -> None:
    fig, ax = canvas(13, 6.2)
    title(ax, 5, 45, "Why keep every form",
          "A rewrite that discards its input makes phase order permanent.")

    # left: destructive
    ax.add_patch(FancyBboxPatch((4.5, 8.5), 40, 27, boxstyle="round,pad=0,rounding_size=1.4",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    caption(ax, 6.5, 32.5, "CONVENTIONAL", colour=RED, size=8.4, mono=True)
    chain = [("(a + a)", 28.0, TEXT), ("(a << 1)", 22.0, TEXT), ("(a * 2)", 16.0, TEXT)]
    for i, (label, y, colour) in enumerate(chain):
        live = i == len(chain) - 1
        box(ax, 8.0, y - 2.0, 15.0, 4.4, label, colour=BLUE if live else EDGE,
            built=live, mono=True, fs=9)
        if i:
            arrow(ax, (15.5, y + 4.0), (15.5, y + 2.4), colour=FAINT)
        if not live:
            ax.text(25.5, y, "discarded", ha="left", va="center", color=FAINT,
                    fontsize=8, fontfamily=SANS, style="italic")
    caption(ax, 6.5, 11.5, "one survivor; the choice was made\nbefore the cost was known", colour=FAINT)

    # right: saturation
    ax.add_patch(FancyBboxPatch((52.0, 8.5), 43.5, 27, boxstyle="round,pad=0,rounding_size=1.4",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    caption(ax, 54.0, 32.5, "EQUALITY SATURATION", colour=PURPLE, size=8.4, mono=True)
    ax.add_patch(FancyBboxPatch((56.5, 14.0), 21.0, 15.0, boxstyle="round,pad=0,rounding_size=1.2",
                                linewidth=1.5, edgecolor=PURPLE, facecolor=PANEL,
                                linestyle=(0, (5, 3)), zorder=1))
    caption(ax, 67.0, 30.6, "one e-class", colour=PURPLE, size=8, ha="center", mono=True)
    for i, label in enumerate(["(a + a)", "(a << 1)", "(a * 2)"]):
        box(ax, 58.5, 24.0 - i * 4.6, 17.0, 3.8, label, colour=PURPLE, built=True, mono=True, fs=8.6)
    box(ax, 83.0, 19.0, 10.0, 5.0, "cost pick", colour=GREEN, built=True, fs=8.6)
    arrow(ax, (77.5, 21.5), (83.0, 21.5), colour=GREEN)
    caption(ax, 54.0, 11.5, "all forms retained; the cost model chooses last", colour=FAINT)

    # the hard part
    ax.add_patch(FancyBboxPatch((4.5, 0.8), 91.0, 5.2, boxstyle="round,pad=0,rounding_size=1.0",
                                linewidth=1.2, edgecolor=AMBER, facecolor="#1a1710", zorder=0))
    ax.text(6.5, 3.4, "Why this is hard on real C:", ha="left", va="center", color=AMBER,
            fontsize=8.6, fontfamily=SANS, fontweight="bold")
    ax.text(30.0, 3.4,
            "effects, undefined behaviour and pointer provenance are things an e-graph will happily "
            "optimize away  ->  make them explicit before it ever sees the program",
            ha="left", va="center", color=MUTED, fontsize=8.4, fontfamily=SANS)
    fig.savefig(out, facecolor=INK)
    plt.close(fig)


# ----------------------------------------------------------------------- loop
def figure_loop(out: Path) -> None:
    fig, ax = canvas(13, 6.0)
    title(ax, 5, 44, "How the work is driven",
          "Nothing is claimed from intuition; a specification gap stops execution.")

    steps = [
        (7.0, 26.0, 16.0, "freeze the rule", "in the specification", BLUE),
        (27.0, 26.0, 16.0, "tests-only RED", "failures declared first", RED),
        (47.0, 26.0, 16.0, "implement", "narrow the red set", BLUE),
        (67.0, 26.0, 14.0, "gate", "tests, lint, corpora", GREEN),
        (85.0, 26.0, 11.0, "publish", "dashboard", TEAL),
    ]
    for x, y, w, label, sub, colour in steps:
        box(ax, x, y, w, 7.0, label, sub=sub, colour=colour, built=True, fs=9)
    for a, b in [(23.0, 27.0), (43.0, 47.0), (63.0, 67.0), (81.0, 85.0)]:
        arrow(ax, (a, 29.5), (b, 29.5), colour=EDGE)

    # feedback edge, routed beneath the row so it crosses nothing
    arrow(ax, (90.0, 26.0), (15.0, 26.0), colour=TEAL, rad=-0.20, lw=1.5)
    caption(ax, 52.0, 17.2, "the next first-error site decides the next window", colour=TEAL, ha="center")

    # a specification gap is a return edge, not a footnote
    arrow(ax, (55.0, 33.0), (15.0, 33.0), colour=RED, rad=0.32, lw=1.4, dashed=True)
    caption(ax, 35.0, 35.6, "specification gap: stop, measure, rule, resume", colour=RED, ha="center")

    # oracle feeds the freeze
    box(ax, 5.0, 5.5, 24.0, 7.0, "sealed oracle container",
        sub="GCC 14.2  /  Clang 19.1", colour=AMBER, built=True, fs=8.6)
    arrow(ax, (15.0, 12.5), (15.0, 26.0), colour=AMBER, dashed=True)
    caption(ax, 30.5, 9.0, "every rule measured before it is written;\n"
                           "on disagreement, take the answer that refuses", colour=FAINT)

    # evidence feeds publish
    box(ax, 72.0, 5.5, 24.0, 7.0, "observer-sealed evidence",
        sub="argv, stdout, stderr, status", colour=TEAL, built=True, fs=8.6)
    arrow(ax, (84.0, 12.5), (88.0, 26.0), colour=TEAL, dashed=True)
    caption(ax, 70.5, 9.0, "refuses to publish on a\nregression or a stale run",
            colour=FAINT, ha="right")

    fig.savefig(out, facecolor=INK)
    plt.close(fig)


# -------------------------------------------------------------------- roadmap
def figure_roadmap(out: Path, parse_clean: int, preprocess_clean: int, total: int) -> None:
    fig, ax = canvas(13, 8.0)
    title(ax, 5, 59, "Roadmap 0.1", "Status is what a gate measures, not an estimate.")

    rows = [
        ("EvoCPP preprocessor", "done", "16/16 units preprocess clean"),
        ("Lexer and C11 + GNU parser", "done", "142 goldens, 1,000-snippet corpus"),
        ("Type system and layout", "done", "128/128 and 22/22 vs sealed oracles"),
        ("C-HIR and semantic interpreter", "done", "typed UB, provenance, initializedness"),
        ("EvoIR block-argument SSA", "done", "explicit mem and world tokens"),
        ("Semantic lowering", "done", "UB explicit before any rewrite"),
        ("Equality engine and proofs", "done", "EVP1 replayed by evo-proofcheck"),
        ("Kernel telemetry and publication", "done", "sealed evidence, refuses regressions"),
        ("Diagnostic survey", "partial", "works; cascades and recovery unfinished"),
        ("Kernel parse closure", "open", f"{parse_clean}/{total} units parse clean"),
        ("Analysis lattices, bounded scheduler", "open", ""),
        ("Randomized semantic gate", "open", ""),
        ("EvoMIR, register allocation, AArch64", "open", ""),
        ("ABIs and native flight gates", "open", ""),
        ("Kernel codegen, link, boot", "open", ""),
    ]
    palette = {"done": GREEN, "partial": AMBER, "open": EDGE}
    y = 52.0
    for label, state, note in rows:
        colour = palette[state]
        ax.add_patch(FancyBboxPatch((5.0, y - 1.15), 2.0, 2.3,
                                    boxstyle="round,pad=0,rounding_size=0.5",
                                    linewidth=1.3, edgecolor=colour,
                                    facecolor=colour if state != "open" else INK,
                                    linestyle="solid" if state != "open" else (0, (3, 2))))
        ax.text(9.0, y, label, ha="left", va="center",
                color=TEXT if state != "open" else FAINT,
                fontsize=9.2, fontfamily=SANS,
                fontweight="medium" if state != "open" else "normal")
        if note:
            ax.text(46.0, y, note, ha="left", va="center", color=MUTED if state != "open" else FAINT,
                    fontsize=8.2, fontfamily=SANS)
        y -= 3.05

    # the one kernel number
    ax.add_patch(FancyBboxPatch((72.0, 24.0), 24.0, 30.0, boxstyle="round,pad=0,rounding_size=1.3",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    ax.text(74.0, 51.0, "THE ONE KERNEL NUMBER", ha="left", va="center", color=TEXT,
            fontsize=8.4, fontfamily=MONO)
    ax.text(74.0, 48.0, f"pinned Linux 6.18.44, {total}-unit cohort", ha="left", va="center",
            color=FAINT, fontsize=7.6, fontfamily=SANS)

    for i, (name, clean, colour) in enumerate(
        [("preprocess", preprocess_clean, GREEN), ("parse (both oracles)", parse_clean, AMBER)]
    ):
        by = 42.0 - i * 9.0
        ax.text(74.0, by + 2.6, name, ha="left", va="center", color=MUTED,
                fontsize=8.0, fontfamily=SANS)
        ax.add_patch(FancyBboxPatch((74.0, by - 0.9), 20.0, 2.4,
                                    boxstyle="round,pad=0,rounding_size=0.7",
                                    linewidth=0, facecolor="#21262d"))
        width = 20.0 * clean / total
        if width > 0.1:
            ax.add_patch(FancyBboxPatch((74.0, by - 0.9), width, 2.4,
                                        boxstyle="round,pad=0,rounding_size=0.7",
                                        linewidth=0, facecolor=colour))
        ax.text(74.0, by - 3.4, f"{clean}/{total}", ha="left", va="center", color=colour,
                fontsize=11, fontfamily=MONO, fontweight="bold")

    ax.text(74.0, 27.5,
            "every published number carries\nthe command that produced it",
            ha="left", va="center", color=FAINT, fontsize=7.6, fontfamily=SANS)

    # the frontier is the progress; the count is a lagging conjunction
    ax.add_patch(FancyBboxPatch((72.0, 3.0), 24.0, 19.5,
                                boxstyle="round,pad=0,rounding_size=1.3",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    ax.text(74.0, 20.6, "WHAT MOVED INSTEAD", ha="left", va="center", color=TEXT,
            fontsize=8.0, fontfamily=MONO)
    frontier = ["__attribute__", "typeof / _Generic", "pointer updates", "asm, overflow",
                "statement expressions", "compile-time builtins", "aggregate copies",
                "__typeof_unqual__", "address-of places"]
    for i, item in enumerate(frontier):
        ax.text(74.6, 18.0 - i * 1.45, f"{i + 1}.", ha="left", va="center", color=FAINT,
                fontsize=6.8, fontfamily=MONO)
        ax.text(77.4, 18.0 - i * 1.45, item, ha="left", va="center",
                color=MUTED if i < len(frontier) - 1 else AMBER,
                fontsize=7.2, fontfamily=SANS)
    ax.text(74.0, 4.6, "each cleared, each republished with zero regressions;\n"
                       "the count only moves when a unit's last one falls",
            ha="left", va="center", color=FAINT, fontsize=7.0, fontfamily=SANS)

    legend(ax, 5, 5.0, [(GREEN, "done", True), (AMBER, "partial", True), (EDGE, "open", False)])
    fig.savefig(out, facecolor=INK)
    plt.close(fig)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("docs/images"))
    parser.add_argument("--parse-clean", type=int, default=2)
    parser.add_argument("--preprocess-clean", type=int, default=16)
    parser.add_argument("--cohort", type=int, default=16)
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    figure_architecture(args.out / "architecture.png")
    figure_why(args.out / "why.png")
    figure_loop(args.out / "loop.png")
    figure_roadmap(args.out / "roadmap.png", args.parse_clean, args.preprocess_clean, args.cohort)
    for name in ("architecture", "why", "loop", "roadmap"):
        path = args.out / f"{name}.png"
        print(f"{path}  {path.stat().st_size // 1024} KiB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
