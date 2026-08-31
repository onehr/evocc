#!/usr/bin/env python3
"""Render the README figures.

Three figures, each showing a mechanism the prose cannot: what the pipeline is
and how much of it exists, what an e-class does that a replacement cannot undo,
and the two return edges that decide what gets built next.

Status belongs in a table, not a picture: a rendered list is not selectable,
not searchable, not diffable, and goes stale in silence.

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
def eclass(ax, x, y, w, h, name, nodes, *, colour=PURPLE):
    """An e-class: a dashed container holding one or more e-nodes."""
    ax.add_patch(FancyBboxPatch((x, y), w, h, boxstyle="round,pad=0,rounding_size=1.0",
                                linewidth=1.5, edgecolor=colour, facecolor="#191426",
                                linestyle=(0, (5, 3)), zorder=2))
    ax.text(x + 1.0, y + h + 1.3, name, ha="left", va="center", color=colour,
            fontsize=7.4, fontfamily=MONO)
    anchors = {}
    nw = (w - 1.6 - 1.2 * (len(nodes) - 1)) / len(nodes)
    for i, label in enumerate(nodes):
        nx = x + 0.8 + i * (nw + 1.2)
        ax.add_patch(FancyBboxPatch((nx, y + 0.9), nw, h - 1.8,
                                    boxstyle="round,pad=0,rounding_size=0.7",
                                    linewidth=1.3, edgecolor=TEXT, facecolor=PANEL, zorder=3))
        ax.text(nx + nw / 2, y + h / 2, label, ha="center", va="center", color=TEXT,
                fontsize=9.5, fontfamily=MONO, zorder=4)
        anchors[label] = (nx + nw / 2, y, nx + nw / 2, y + h - 0.9)
    return anchors


def tree(ax, nodes, edges, *, dim=False, mark=None):
    """A small expression tree: (label, x, y) nodes and (parent, child) edges."""
    pos = {}
    for label, x, y in nodes:
        pos[label] = (x, y)
    for a, b in edges:
        arrow(ax, (pos[a][0], pos[a][1] - 1.6), (pos[b][0], pos[b][1] + 1.6),
              colour=FAINT if dim else EDGE, style="-")
    for label, x, y in nodes:
        text = label.split("#")[0]
        hot = mark is not None and label == mark
        colour = RED if hot else (FAINT if dim else TEXT)
        ax.add_patch(FancyBboxPatch((x - 2.1, y - 1.6), 4.2, 3.2,
                                    boxstyle="round,pad=0,rounding_size=0.8",
                                    linewidth=1.4, edgecolor=colour,
                                    facecolor=INK if dim else PANEL, zorder=3))
        ax.text(x, y, text, ha="center", va="center", color=colour,
                fontsize=9.5, fontfamily=MONO, zorder=4)
    return pos


def figure_why(out: Path) -> None:
    fig, ax = canvas(13, 6.8)
    title(ax, 5, 50,
          "A rewrite that replaces cannot be taken back",
          "One program, optimized two ways. Only one of them can still reach the answer.")

    # ---------------- left: rewriting in place destroys the pattern
    ax.add_patch(FancyBboxPatch((4.0, 4.5), 40.0, 36.5, boxstyle="round,pad=0,rounding_size=1.4",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    caption(ax, 6.0, 38.6, "REWRITE IN PLACE", colour=RED, size=8.4, mono=True)

    tree(ax,
         [("/", 14.0, 35.0), ("*", 9.0, 30.0), ("2", 19.0, 30.0),
          ("a", 6.0, 25.0), ("2#a", 12.0, 25.0)],
         [("/", "*"), ("/", "2"), ("*", "a"), ("*", "2#a")])
    caption(ax, 24.0, 33.0, "(a * 2) / 2", colour=MUTED, mono=True)

    arrow(ax, (38.0, 29.0), (38.0, 24.0), colour=RED, dashed=True)
    caption(ax, 24.0, 26.5, "a * 2  ->  a << 1", colour=RED, size=7.8, mono=True)

    tree(ax,
         [("/", 14.0, 20.0), ("<<", 9.0, 15.0), ("2", 19.0, 15.0),
          ("a", 6.0, 10.0), ("1", 12.0, 10.0)],
         [("/", "<<"), ("/", "2"), ("<<", "a"), ("<<", "1")], mark="<<")
    caption(ax, 24.0, 18.0, "(a << 1) / 2", colour=MUTED, mono=True)
    ax.text(6.0, 6.4, "the `* 2` that the cancellation matched on is gone.\n"
                      "`(a * 2) / 2  ->  a` can never fire again.",
            ha="left", va="center", color=RED, fontsize=7.8, fontfamily=SANS)

    # ---------------- right: the same rewrite as a merge
    ax.add_patch(FancyBboxPatch((48.0, 4.5), 48.0, 36.5, boxstyle="round,pad=0,rounding_size=1.4",
                                linewidth=1.2, edgecolor=EDGE, facecolor="#12171e", zorder=0))
    caption(ax, 50.0, 38.6, "MERGE INTO AN E-CLASS", colour=PURPLE, size=8.4, mono=True)

    c0 = eclass(ax, 58.0, 30.0, 22.0, 6.0, "c0", ["/", "a"])
    c1 = eclass(ax, 58.0, 18.0, 22.0, 6.0, "c1", ["*", "<<"])
    eclass(ax, 86.0, 26.0, 8.0, 5.5, "c2", ["2"])
    eclass(ax, 86.0, 15.0, 8.0, 5.5, "c3", ["1"])

    arrow(ax, (c0["/"][0] + 1.0, 30.0), (c1["*"][0] + 2.0, 24.0), colour=PURPLE)  # / -> c1
    arrow(ax, (c0["/"][0] + 3.0, 30.0), (86.0, 29.0), colour=PURPLE, rad=-0.2)  # / -> c2
    arrow(ax, (c1["*"][0] + 3.0, 18.0), (86.0, 27.5), colour=PURPLE, rad=0.28)  # * -> c2
    arrow(ax, (c1["<<"][0], 18.0), (86.0, 18.5), colour=PURPLE, rad=-0.18)      # << -> c3
    arrow(ax, (58.0, 20.5), (58.0, 33.5), colour=TEAL, rad=-0.75, lw=1.6)        # c1 -> c0
    caption(ax, 51.6, 27.0, "a", colour=TEAL, size=9.5, mono=True)

    caption(ax, 50.0, 12.5, "both e-nodes in c1 take c0's `a` as their left operand",
            colour=FAINT, size=7.4)
    ax.text(50.0, 8.6, "the cancellation did not delete the multiply. it merged\n"
                       "`(a * 2) / 2` into the class that already held `a`.",
            ha="left", va="center", color=TEAL, fontsize=7.8, fontfamily=SANS)
    ax.text(50.0, 5.9, "extraction reads `a` straight out of c0.",
            ha="left", va="center", color=GREEN, fontsize=7.8, fontfamily=SANS)

    # ---------------- the hard part
    ax.add_patch(FancyBboxPatch((4.0, 0.4), 92.0, 3.2, boxstyle="round,pad=0,rounding_size=0.9",
                                linewidth=1.2, edgecolor=AMBER, facecolor="#1a1710", zorder=0))
    ax.text(6.0, 2.0, "On real C this only works if", ha="left", va="center", color=AMBER,
            fontsize=8.0, fontfamily=SANS, fontweight="bold")
    ax.text(28.0, 2.0,
            "effects, undefined behaviour and provenance are already explicit -- an e-graph "
            "merges anything it is told is equal, including things that are not",
            ha="left", va="center", color=MUTED, fontsize=8.0, fontfamily=SANS)
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


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", type=Path, default=Path("docs/images"))
    args = parser.parse_args()

    args.out.mkdir(parents=True, exist_ok=True)
    figure_architecture(args.out / "architecture.png")
    figure_why(args.out / "why.png")
    figure_loop(args.out / "loop.png")
    for name in ("architecture", "why", "loop"):
        path = args.out / f"{name}.png"
        print(f"{path}  {path.stat().st_size // 1024} KiB")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
