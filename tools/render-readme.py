#!/usr/bin/env python3
"""Render the README and paper figures from real compiler output and receipts.

Run from the repository root after a release build:

    python3 tools/render-readme.py                # run the sessions, then write every figure
    python3 tools/render-readme.py --render-only  # write every figure from the saved transcript
    python3 tools/render-readme.py --check        # compare every figure with its sources

The highlights figure runs four short terminal sessions and draws their real output. Each
session runs in one shell, in a temporary directory. The GCC session runs in the sealed
`evo-nexlab:sealed` container. The sessions go to `highlights.json`; both variants of the figure
are drawn from it. The scoreboard reads the receipts of the release campaign. The explicit-check
figure shows the EvoIR and the assembly that the release compiler makes for `examples/abs.c`.
The stages figure follows line 7 of `examples/mean.c`: the record of `evo check`, then the EvoIR
and the assembly of that line, and it refuses facts that do not match the source. The agent
results figure reads the receipts and the registered analyses of experiments 5 and 8. The book
shows it as a card on its own surface, because a reader can choose a book theme that differs from
the system theme. `--check` runs no session: it draws every figure again from the transcript, the receipts and the
release compiler, and it names each file that differs. Each figure has a light and a dark variant.
"""
import argparse
import html
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "images" / "readme"
PAPER = ROOT / "docs" / "images" / "paper"
BOOK = ROOT / "book" / "src" / "images"
TRANSCRIPT = OUT / "highlights.json"
EXAMPLES = ROOT / "examples"
BIN = ROOT / "target" / "frontend" / "release"
ZLIB = ROOT / ".build" / "ladder" / "zlib" / "zlib-1.3.2.tar.gz"
GCC_IMAGE = "evo-nexlab:sealed"
# The lines that `grep -E 'subs|b\.vs|brk'` prints from the checked abs.s. The README and the
# paper show them; tools/tests/test_render_readme.py keeps the three in agreement.
PROVE_ASSEMBLY = "    subs w0, w1, w0\n    b.vs .Labs_block4\n    brk #3004\n"

MONO = "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace"
SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"
ANSI = {"31": "#ff7b72", "32": "#7ee787", "33": "#d29922", "34": "#79c0ff",
        "35": "#d2a8ff", "36": "#39c5cf"}
SGR = re.compile(r"\x1b\[([0-9;]*)m")


def spans(line):
    """Split one line with ANSI SGR codes into (text, color, bold) parts."""
    parts, color, bold, pos = [], None, False, 0
    for match in SGR.finditer(line):
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


def session(commands, cwd, container=False):
    """Run commands in one shell, as typed at a prompt. Return [(command, output)]."""
    script = "exec 2>&1\n" + "".join("printf '\\036\\n'; " + c + "\n" for c in commands)
    if container:
        argv = ["container", "run", "--rm", "--platform", "linux/arm64", "--network", "none",
                "-v", f"{cwd}:/w", GCC_IMAGE, "sh", "-c", "cd /w\n" + script]
    else:
        argv = ["sh", "-c", script]
    env = dict(os.environ, PATH=f"{BIN}{os.pathsep}{os.environ['PATH']}", TMPDIR=str(cwd))
    result = subprocess.run(argv, cwd=cwd, env=env, text=True,
                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    outputs = result.stdout.split("\x1e\n")
    if outputs[0] or len(outputs) != len(commands) + 1:
        raise SystemExit(f"session did not run: {result.stdout!r}")
    return list(zip(commands, outputs[1:]))


def check_session(name, cwd):
    """`evo check NAME`, in color. The colors come from `evocc --check --color=always`."""
    (_, plain), (_, colored) = session([f"evo check {name}",
                                        f"evocc --check --color=always {name}"], cwd)
    if SGR.sub("", colored) != plain:
        raise SystemExit(f"evo check {name} and evocc --check print different text")
    return (f"evo check {name}", colored)


def drop_in(work):
    shutil.copy(EXAMPLES / "hello.c", work)
    subprocess.run(["tar", "xzf", str(ZLIB), "-C", str(work)], check=True)
    return session(["evocc --version", "evocc hello.c -o hello && ./hello",
                    "cd zlib-1.3.2 && CC=evocc ./configure > /dev/null",
                    "make test | grep -o 'zlib.*OK'"], work)


def find_ub(work):
    shutil.copy(EXAMPLES / "first_bad.c", work)
    return [check_session("first_bad.c", work)]


def ubsan(work):
    shutil.copy(EXAMPLES / "mean.c", work)
    steps = session(["gcc-14 -O2 -Wall -Wextra -fsanitize=undefined mean.c",
                     "./a.out; echo $?  # expect 4"], work, container=True)
    return steps + [check_session("mean.c", work)]


def prove(work):
    shutil.copy(EXAMPLES / "abs.c", work)
    return session(["cat abs.c", "evo prove abs.c -o abs.s --json | jq -r .result",
                    r"grep -E 'subs|b\.vs|brk' abs.s"], work)


PANELS = [("A drop-in cc", drop_in), ("Undefined behavior, with evidence", find_ub),
          ("Catches what UBSan misses", ubsan), ("Proof-carrying compilation", prove)]
FONT, CHAR, LINE, PAD, GAP, TOP = 13, 7.83, 19, 18, 16, 56
# The terminal windows. On a dark page a darker window would disappear, so it is lighter there.
WINDOW = {"light": {"window": "#0d1117", "window_edge": "#30363d"},
          "dark": {"window": "#161b22", "window_edge": "#3d444d"}}
HIGHLIGHTS = {"light": "highlights.svg", "dark": "highlights-dark.svg"}


def capture_highlights():
    """Run the four sessions. Return the transcript that both variants are drawn from."""
    panels = []
    with tempfile.TemporaryDirectory() as tmp:
        for i, (title, run) in enumerate(PANELS):
            work = Path(tmp).resolve() / str(i)
            work.mkdir()
            panels.append({"title": title, "steps": [list(step) for step in run(work)]})
    return {"panels": panels}


def validate_transcript(transcript):
    """Refuse a transcript whose sessions do not show what the README says."""
    panels = {panel["title"]: panel["steps"] for panel in transcript["panels"]}
    if list(panels) != [title for title, _ in PANELS]:
        raise ValueError("the transcript does not hold the four sessions in order")

    def expect(title, index, wanted, what):
        command, output = panels[title][index]
        if output != wanted:
            raise ValueError(f"`{command}` printed {output!r}, not {what}")

    expect("A drop-in cc", 1, "Hello, world!\n", "the hello line")
    expect("A drop-in cc", 2, "", "nothing")
    expect("A drop-in cc", 3, "zlib test OK\nzlib shared test OK\n", "both zlib OK lines")
    expect("Catches what UBSan misses", 0, "", "nothing")
    gcc = panels["Catches what UBSan misses"][1][1]
    if not re.fullmatch(r"\d+\n", gcc) or gcc == "4\n":
        raise ValueError(f"GCC's mean.c printed {gcc!r}, or it returned the right answer")
    for title, code in (("Undefined behavior, with evidence", "E3004"),
                        ("Catches what UBSan misses", "E3001")):
        if f"error[{code}]" not in SGR.sub("", panels[title][-1][1]):
            raise ValueError(f"evo check gives no {code} in the session {title!r}")
    expect("Proof-carrying compilation", 1, "CHECKED_TO_FINAL_ASSEMBLY\n", "the checked result")
    expect("Proof-carrying compilation", 2, PROVE_ASSEMBLY, "the lines of the checked abs.s")


def window(x, y, width, height, title, lines, theme):
    """Draw one terminal window. Each line is a list of (text, color, bold)."""
    colors = WINDOW[theme]
    out = [f'<g transform="translate({x} {y})">',
           f'<rect x="0.5" y="0.5" width="{width - 1}" height="{height - 1}" rx="10" '
           f'fill="{colors["window"]}" stroke="{colors["window_edge"]}"/>',
           '<circle cx="20" cy="18" r="6" fill="#ff5f56"/><circle cx="40" cy="18" r="6" '
           'fill="#ffbd2e"/><circle cx="60" cy="18" r="6" fill="#27c93f"/>',
           f'<text x="{width // 2}" y="23" fill="#e6edf3" font-family="{SANS}" font-size="13" '
           f'font-weight="600" text-anchor="middle">{html.escape(title)}</text>']
    bold_attr = ' font-weight="700"'
    for row, line in enumerate(lines):
        tspans = "".join(f'<tspan fill="{color or "#e6edf3"}"{bold_attr if bold else ""}>'
                         f'{html.escape(text)}</tspan>' for text, color, bold in line)
        if tspans:
            out.append(f'<text x="{PAD}" y="{TOP + LINE * row}" font-family="{MONO}" '
                       f'font-size="{FONT}" xml:space="preserve">{tspans}</text>')
    out.append("</g>")
    return "\n".join(out)


def highlights(transcript, theme):
    """Four terminal sessions in a 2 x 2 grid, one per highlight."""
    panels = []
    for panel in transcript["panels"]:
        lines = []
        for command, output in panel["steps"]:
            lines.append([("$ ", "#7ee787", True), (command, None, True)])
            lines += [spans(line.expandtabs()) for line in output.splitlines()]
        panels.append((panel["title"], lines))
    columns = max(sum(len(t) for t, _, _ in line) for _, lines in panels for line in lines)
    width = 2 * PAD + math.ceil(CHAR * columns)
    rows = [max(len(panels[i][1]), len(panels[i + 1][1])) for i in (0, 2)]
    heights = [TOP + LINE * (n - 1) + 20 for n in rows]
    total_w, total_h = 2 * width + GAP, sum(heights) + GAP
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" '
           f'viewBox="0 0 {total_w} {total_h}" role="img" aria-label="EvoCC highlights: '
           + "; ".join(title for title, _ in panels) + '">']
    for i, (title, lines) in enumerate(panels):
        x, y = (i % 2) * (width + GAP), (i // 2) * (heights[0] + GAP)
        out.append(window(x, y, width, heights[i // 2], title, lines, theme))
    out.append("</svg>")
    return "\n".join(out) + "\n"


THEMES = {
    "light": dict(box="#ffffff", edge="#d0d7de", title="#1f2328", sub="#59636e",
                  arrow="#8c959f", check="#1a7f37", rules="#8250df", prove="#0969da",
                  code="#f6f8fa", mark="#fff8c5", mark_edge="#d4a72c", trap="#cf222e"),
    "dark": dict(box="#161b22", edge="#3d444d", title="#f0f6fc", sub="#9198a1",
                 arrow="#656c76", check="#3fb950", rules="#a371f7", prove="#4493f8",
                 code="#0d1117", mark="#3d2e00", mark_edge="#9e6a03", trap="#f85149"),
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
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="888" height="216" '
           'viewBox="0 0 888 216" role="img" aria-label="The EvoCC pipeline">']
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


def kimi_cells(name, corpus, silent):
    """Steps and repaired sessions with C1 and C3 in one experiment 5 receipt. `silent` keeps
    the silent items (True), the controls (False) or every item (None)."""
    cells = {condition: dict(effort=0, repaired=0, runs=0) for condition in ("c1", "c3")}
    for run in receipt(name)["runs"]:
        code = (ROOT / "tests" / "fixtures" / corpus / run["item"] / "class").read_text().strip()
        if silent is not None and (code in CONTROL_CODES) == silent:
            continue
        cell = cells[run["condition"]]
        cell["effort"] += run["steps"]
        cell["repaired"] += run["passed"] is True
        cell["runs"] += 1
    return cells


def effort_totals():
    rows = []
    for label, sub, name, corpus, silent in EFFORT:
        cells = kimi_cells(name, corpus, silent)
        rows.append((label, sub, cells["c1"]["effort"], cells["c3"]["effort"]))
    return rows


def effort_chart(theme):
    t = THEMES[theme]
    base = "#afb8c1" if theme == "light" else "#6e7681"
    rows = effort_totals()
    peak = max(max(a, b) for _, _, a, b in rows)
    scale, x0 = 400 / peak, 300
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="820" height="300" viewBox="0 0 820 300" '
           'role="img" aria-label="Agent steps to repair, GCC and UBSan against EvoCC">']
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


# The chat models of experiment 8 that completed every run, and the one run that stopped.
CHAT_MODELS = [("Qwen3.8 27B", "agent-repair-8-qwen3.8-27b-20261029.json"),
               ("DeepSeek V4.1 Flash", "agent-repair-8-deepseek-v4.1-flash-20261029.json"),
               ("MiniMax M3.1 Flash", "agent-repair-8-minimax-m3.1-flash-20261029.json")]
STOPPED_CHAT = ("Mistral Small 4", "agent-repair-8-mistral-small-2603-20261029.json")


def chat_cells(runs, silent):
    """Mean output tokens per answer and repaired answers with C1 and C3 in one experiment 8
    receipt, on the silent items or on the controls."""
    cells = {}
    for condition in ("c1", "c3"):
        chosen = [run for run in runs
                  if run["condition"] == condition and (run["code"] in CONTROL_CODES) != silent]
        tokens = [run["completion_tokens"] for run in chosen if type(run["completion_tokens"]) is int]
        cells[condition] = dict(effort=sum(tokens) / len(tokens), runs=len(chosen),
                                repaired=sum(run["correct"] is True for run in chosen))
    return cells


def agent_results():
    """The rows of the agent results figure, and the registered p of each silent saving."""
    five, eight = receipt("agent-repair-5-20261010.json"), receipt("agent-repair-8-20261029.json")
    tests = {row["model"]: row for row in eight}
    kimi, a, b = "Kimi K3, coding agent", EFFORT[0][2], EFFORT[1][2]
    silent = [(kimi, "18 short programs", "steps", kimi_cells(a, "agent-repair-4", True)),
              (kimi, "12 programs of 722 lines", "steps", kimi_cells(b, "agent-repair-5", None))]
    controls = [(kimi, "6 programs", "steps", kimi_cells(a, "agent-repair-4", False))]
    held = [five["P1"], five["P3"]]
    for model, name in CHAT_MODELS:
        record = receipt(name)
        if record["stop_reason"] != "completed":
            raise ValueError(f"{name}: the run stopped ({record['stop_reason']})")
        held.append(tests[record["plan"]["model"]["model"]]["P3"])
        silent.append((f"{model}, chat model", "18 short programs", "tokens",
                       chat_cells(record["runs"], True)))
        controls.append((f"{model}, chat model", "6 programs", "tokens",
                         chat_cells(record["runs"], False)))
    if not all(test["held"] and test["p"] < 0.01 for test in held):
        raise ValueError("a silent saving is not a registered result with p < 0.01")
    stopped = receipt(STOPPED_CHAT[1])
    return dict(groups=[("Silent bugs: GCC 14's warnings and UBSan report nothing", silent),
                        ("Control bugs: GCC and UBSan report them", controls)],
                stopped=(STOPPED_CHAT[0], len(stopped["runs"]), len(stopped["plan"]["trials"])))


def change(before, after):
    """The relative change from C1 to C3, as the figures print it."""
    return f"{100 * (after - before) / before:+.0f}%".replace("-", "\u2212")


def repaired(cells):
    """The repaired runs with C1 and C3, as the agent results figure prints them."""
    c1, c3 = cells["c1"], cells["c3"]
    if (c1["repaired"], c1["runs"]) == (c3["repaired"], c3["runs"]):
        return f"{c3['repaired']} of {c3['runs']}, both"
    return f"{c1['repaired']} \u2192 {c3['repaired']} of {c3['runs']}"


def data_bar(x, y, width, height, color):
    """A bar that grows from x: square at the baseline, with a 4px rounded data end."""
    r = min(4, width, height / 2)
    return (f'<path d="M{x} {y} H{x + width - r:.1f} A{r} {r} 0 0 1 {x + width:.1f} {y + r} '
            f'V{y + height - r} A{r} {r} 0 0 1 {x + width - r:.1f} {y + height} H{x} Z" fill="{color}"/>')


def agent_notes(data):
    """The notes under the agent results figure."""
    name, ran, planned = data["stopped"]
    return ("Kimi K3: total agent steps over 2 runs of each program. Chat models: mean output "
            "tokens per answer over 3 runs.",
            "Each saving on silent bugs is a registered prediction that held, with p < 0.01.",
            f"{name} stopped after {ran} of {planned} runs and is not shown.")


def agent_results_alt(data):
    """The alt text of the agent results figure, with every value that the figure draws."""
    parts = ["The same bugs, with GCC 14's warnings and UBSan or with EvoCC: the effort with EvoCC "
             "as a share of the effort with GCC and UBSan, and the share of runs that repaired the bug."]
    for title, group in data["groups"]:
        parts.append(title + ".")
        for agent, population, unit, cells in group:
            c1, c3 = cells["c1"], cells["c3"]
            same = (c1["repaired"], c1["runs"]) == (c3["repaired"], c3["runs"])
            fixed = (f"{c3['repaired']} of {c3['runs']} with both" if same
                     else f"{c1['repaired']} to {c3['repaired']} of {c3['runs']}")
            parts.append(f"{agent}, {population}: {c1['effort']:,.0f} to {c3['effort']:,.0f} {unit} "
                         f"({change(c1['effort'], c3['effort'])}), repaired {fixed}.")
    return " ".join(parts + list(agent_notes(data)))


def agent_results_chart(data, theme, card=False):
    """Effort and repairs of each agent on the same bugs, with GCC and UBSan and with EvoCC. A card
    adds a margin and the surface of its theme under the same figure."""
    t = THEMES[theme]
    surface, base, grid = (("#ffffff", "#818b98", "#d1d9e0") if theme == "light"
                           else ("#0d1117", "#6e7681", "#3d444d"))
    width, a0, a100, change_x, b0, b100, count_x = 830, 292, 472, 500, 572, 700, 712
    bands, y = [], 92
    for _, group in data["groups"]:
        bands.append((y + 36, y + 36 + 46 * len(group)))
        y = bands[-1][1]
    bottom = bands[-1][1]
    height = bottom + 102
    pad = 16 if card else 0
    box_w, box_h = width + 2 * pad, height + 2 * pad
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{box_w}" height="{box_h}" '
           f'viewBox="{-pad} {-pad} {box_w} {box_h}" role="img" aria-label="Effort and repairs of each '
           f'agent, with GCC 14\'s warnings and UBSan and with EvoCC">']
    if card:
        out.append(f'<rect x="{-pad}" y="{-pad}" width="{box_w}" height="{box_h}" rx="6" fill="{surface}"/>')
    out.append(text(0, 22, "The same bugs, with GCC 14\'s warnings and UBSan or with EvoCC", t["title"], 15, 600))
    for x, color, name in ((600, base, "GCC and UBSan"), (730, t["prove"], "EvoCC")):
        out.append(f'<rect x="{x}" y="11" width="14" height="14" rx="2" fill="{color}"/>')
        out.append(text(x + 20, 22, name, t["sub"], 13))
    out += [text(a0, 58, "Effort with EvoCC", t["title"], 14, 600),
            text(a0, 76, "GCC and UBSan = 100% in each row", t["sub"], 13),
            text(b0, 58, "Bugs repaired", t["title"], 14, 600),
            text(b0, 76, "share of runs", t["sub"], 13)]
    for top, end in bands:
        for x in (a0, (a0 + a100) / 2, b0, (b0 + b100) / 2, b100):
            out.append(f'<path d="M{x} {top} V{end}" stroke="{grid}" stroke-width="1"/>')
    y, marks = 92, []
    for title, group in data["groups"]:
        out.append(f'<path d="M0 {y + 0.5} H{width}" stroke="{grid}" stroke-width="1"/>')
        out.append(text(0, y + 24, title, t["sub"], 13, 600))
        y += 36
        for agent, population, unit, cells in group:
            c1, c3 = cells["c1"], cells["c3"]
            out.append(text(0, y + 16, agent, t["title"], 14, 600))
            out.append(text(0, y + 34, f"{population} \u00b7 {c1['effort']:,.0f} \u2192 "
                                       f"{c3['effort']:,.0f} {unit}", t["sub"], 13))
            out.append(data_bar(a0, y + 4, (a100 - a0) * c3["effort"] / c1["effort"], 16, t["prove"]))
            out.append(text(change_x, y + 17, change(c1["effort"], c3["effort"]), t["title"], 13))
            gcc = b0 + (b100 - b0) * c1["repaired"] / c1["runs"]
            evo = b0 + (b100 - b0) * c3["repaired"] / c3["runs"]
            marks.append(f'<path d="M{gcc:.1f} {y + 12} H{evo:.1f}" stroke="{base}" stroke-width="2"/>')
            marks.append(f'<circle cx="{gcc:.1f}" cy="{y + 12}" r="6" fill="{surface}" '
                         f'stroke="{base}" stroke-width="2"/>')
            marks.append(f'<circle cx="{evo:.1f}" cy="{y + 12}" r="4" fill="{t["prove"]}"/>')
            out.append(text(count_x, y + 17, repaired(cells), t["title"], 13))
            y += 46
    out += [f'<path d="M{a100} {top} V{end}" stroke="{base}" stroke-width="2"/>' for top, end in bands]
    out += marks
    for x, label in ((a0, "0%"), ((a0 + a100) / 2, "50%"), (a100, "100%"),
                     (b0, "0%"), ((b0 + b100) / 2, "50%"), (b100, "100%")):
        out.append(text(x, bottom + 20, label, t["sub"], 13, anchor="middle"))
    for i, note in enumerate(agent_notes(data)):
        out.append(text(0, bottom + 52 + 20 * i, note, t["sub"], 13))
    out.append("</svg>")
    return "\n".join(out) + "\n"


def box(x, y, w, h, t, edge=None, width=1):
    return (f'<rect x="{x + 0.5}" y="{y + 0.5}" width="{w - 1}" height="{h - 1}" rx="12" '
            f'fill="{t["box"]}" stroke="{edge or t["edge"]}" stroke-width="{width}"/>')


def arrow_right(x0, x1, y, t):
    return (f'<path d="M{x0} {y} H{x1 - 8}" stroke="{t["arrow"]}" stroke-width="2"/>'
            f'<path d="M{x1} {y} l-10 -6 v12 z" fill="{t["arrow"]}"/>')


# One row for each level: what proposes, and the small checker that decides.
PROPOSE_CHECK = [
    ("AI agents", "write the code of the compiler", "check", "Tests and oracles",
     "a failing test first; two interpreters", "that must agree with GCC 14 and Clang"),
    ("The EqSat optimizer", "proposes each rewrite", "rules", "evocc-checker and Lean 4",
     "replays each rewrite against rules", "that Lean 4 proves for every bit width"),
    ("Each compiler stage", "for core-v1 functions", "prove", "Five Lean 4 checkers",
     "compare the input and output of each stage;", "one theorem covers the whole chain"),
]


def propose_check(theme):
    """The rule of the project: an untrusted part proposes, and a small checker decides."""
    t = THEMES[theme]
    rows = len(PROPOSE_CHECK)
    height = 40 + rows * 96 + 36
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="860" height="{height}" '
           f'viewBox="0 0 860 {height}" role="img" aria-label="At each level of EvoCC, an untrusted '
           f'part proposes and a small checker decides">']
    out.append(text(150, 22, "Proposes (untrusted)", t["sub"], 14, 700, "middle"))
    out.append(text(620, 22, "Decides (a small checker)", t["sub"], 14, 700, "middle"))
    for i, (who, what, color, checker, first, second) in enumerate(PROPOSE_CHECK):
        y = 40 + i * 96
        out.append(box(0, y, 300, 82, t))
        out.append(text(150, y + 36, who, t["title"], 16, 700, "middle"))
        out.append(text(150, y + 59, what, t["sub"], 14, anchor="middle"))
        out.append(arrow_right(308, 372, y + 41, t))
        out.append(box(380, y, 480, 82, t, t[color], 2))
        out.append(text(620, y + 27, checker, t[color], 16, 700, "middle"))
        out.append(text(620, y + 49, first, t["title"], 14, anchor="middle"))
        out.append(text(620, y + 69, second, t["title"], 14, anchor="middle"))
    out.append(text(430, height - 12, "When a check fails, nothing is accepted.", t["title"], 15, 600,
                    "middle"))
    out.append("</svg>")
    return "\n".join(out) + "\n"


def proved_rewrite(theme):
    """One rewrite: rule, proof, proposal, independent replay, result."""
    t = THEMES[theme]
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="860" height="188" viewBox="0 0 860 188" '
           'role="img" aria-label="How a proved rewrite reaches the output">']
    steps = [("Rule", t["rules"], [("x + 0 \u2192 x", MONO)], "in the registry"),
             ("Lean 4 proof", t["rules"], [("holds for", SANS)], "every bit width"),
             ("Compiler", t["prove"], [("proposes it", SANS)], "with a certificate"),
             ("Checker", t["check"], [("replays it", SANS)], "independently"),
             ("Result", t["title"], [], "")]
    for i, (title, color, lines, sub) in enumerate(steps):
        x = 177 * i
        out.append(box(x, 8, 152, 112, t, color if i < 4 else None, 2 if i < 4 else 1))
        out.append(text(x + 76, 40, title, color, 17, 700, "middle"))
        for row, (line, font) in enumerate(lines):
            out.append(text(x + 76, 70 + 22 * row, line, t["title"], 15, None, "middle", font))
        if sub:
            out.append(text(x + 76, 96, sub, t["sub"], 13, anchor="middle"))
        if i:
            out.append(arrow_right(x - 23, x - 2, 64, t))
    x = 177 * 4
    out.append(text(x + 76, 70, "replay passes:", t["check"], 13, 600, "middle"))
    out.append(text(x + 76, 88, "the rewrite is used", t["title"], 13, anchor="middle"))
    out.append(f'<path d="M{x + 76} 120 V134" stroke="{t["arrow"]}" stroke-width="2"/>')
    out.append(f'<rect x="{x + 0.5}" y="134.5" width="151" height="49" rx="12" fill="{t["box"]}" '
               f'stroke="{t["edge"]}" stroke-dasharray="4 3"/>')
    out.append(text(x + 76, 155, "replay fails:", t["trap"], 13, 600, "middle"))
    out.append(text(x + 76, 173, "the original stays", t["title"], 13, anchor="middle"))
    out.append("</svg>")
    return "\n".join(out) + "\n"


# The 15 application lanes of the release campaign. Doom is a separate lane.
LANES = ("ctest", "torture", "csmith", "zlib", "zstd", "cjson", "mbedtls", "tinycc", "toybox",
         "musl", "polybench", "llvm-test-suite", "jq", "lua", "sqlite")
# Csmith records one outcome per program; these outcomes are not compiler failures.
NOT_FAILURES = {"agree", "oracle-unusable", "oracle-split", "not-run"}


def receipt(name):
    return json.loads((ROOT / "tests" / "contracts" / name).read_text(encoding="utf-8"))


def compiler_failures(name, row):
    """The compiler failures of one lane, from the reviewed dispositions or the raw outcomes."""
    if "dispositions" in row:
        return row["dispositions"]["compiler-failure"]
    if row.get("status") == "PASS":
        return 0
    outcomes = row.get("outcomes") or {}
    if "agree" in outcomes:
        return sum(n for outcome, n in outcomes.items() if outcome not in NOT_FAILURES)
    if row.get("testVerdict") in ("no-regression", "libc-test-clean") and row.get("failingUnits") == []:
        return 0
    raise ValueError(f"the receipt gives no compiler-failure count for {name}")


def scoreboard_data():
    """Read the numbers of the scoreboard from the receipts of the release campaign."""
    kernel = receipt("kernel-direct-virt-receipt.json")
    if kernel["status"] != "BUILD_BOOT_SELECTED_SELFTEST_PASS":
        raise ValueError("the direct Kbuild replay did not pass")
    counts, subtests = kernel["counts"], kernel["selftests"]["subject"]["subtests"]
    boots = [boot["verdict"] for boot in kernel["boots"].values()]
    if kernel["selftests"]["status"] != "PASS" or set(boots) != {"PASS"}:
        raise ValueError("a boot or the selected selftests did not pass")
    bench = receipt("bench-receipt.json")
    if bench.get("dirty") is not False:
        raise ValueError("the bench receipt does not come from a clean tree")
    rows = bench["benchmarks"]
    failures = sum(compiler_failures(name, rows[name]) for name in (*LANES, "doom"))
    if rows["doom"]["status"] != "PASS":
        raise ValueError("the Doom lane did not pass")
    log = json.loads((ROOT / "docs/images/showcase/captures.json").read_text(encoding="utf-8"))
    summary = log["logs"]["sqlite"]["segments"][-1]["text"][-1]
    sqlite = re.match(r"(\d+) errors out of (\d+) tests", summary)
    if not sqlite:
        raise ValueError("the SQLite log card has no summary line")
    csmith = receipt("ladder-csmith-receipt.json")["latest"]
    lean = receipt("lean-p0-receipt.json")["binding"]
    return {
        "linux": {"compiled": counts["evoccCObjects"],
                  "total": counts["evoccCObjects"] + counts["gccTargetCSourceRecords"],
                  "boots": len(boots), "subtests": subtests.get("PASS", 0),
                  "subtestsTotal": sum(subtests.values())},
        "sqlite": {"errors": int(sqlite.group(1)), "tests": int(sqlite.group(2))},
        "lanes": {"lanes": len(LANES), "failures": failures},
        "ctest": {"passed": rows["ctest"]["passed"], "total": rows["ctest"]["total"]},
        "csmith": {"programs": csmith["programs"], "agree": csmith["agreeing"],
                   "mismatches": compiler_failures("csmith", rows["csmith"])
                   + sum((csmith.get("refusalsByCode") or {}).values())},
        "lean": {"rules": int(lean["registry_rules"]) + int(lean["guard_registry"]["rules"])},
    }


def text(x, y, value, color, size, weight=None, anchor=None, font=SANS):
    extra = (f' font-weight="{weight}"' if weight else "") + (f' text-anchor="{anchor}"' if anchor else "")
    return (f'<text x="{x}" y="{y}" fill="{color}" font-family="{font}" font-size="{size}"{extra}>'
            f'{html.escape(value)}</text>')


def scoreboard(data, theme):
    """Six cards, one number each, in two rows of three."""
    t = THEMES[theme]
    linux, sqlite, lanes = data["linux"], data["sqlite"], data["lanes"]
    ctest, csmith, lean = data["ctest"], data["csmith"], data["lean"]
    cards = [
        (f"{linux['compiled']:,}/{linux['total']:,}", "Linux kernel C files", t["prove"],
         ["boots under HVF and under TCG",
          f"{linux['subtests']}/{linux['subtestsTotal']} selected selftests pass"]),
        (f"{sqlite['errors']:,}", "SQLite test errors", t["check"],
         [f"in {sqlite['tests']:,} tests of its", "veryquick suite"]),
        (f"{lanes['failures']:,}", "compiler failures", t["check"],
         [f"in {lanes['lanes']} application lanes", "and Doom"]),
        (f"{ctest['passed']:,}/{ctest['total']:,}", "c-testsuite programs", t["prove"],
         ["pass, with no exclusion", "and no deferral"]),
        (f"{csmith['mismatches']:,}", "Csmith mismatches", t["check"],
         [f"{csmith['agree']:,} of {csmith['programs']:,} programs agree;",
          f"{csmith['programs'] - csmith['agree']:,} have no usable reference"]),
        (f"{lean['rules']:,}", "rewrite rules proved", t["rules"],
         ["in Lean 4, for every bit width;", "a checker replays each use"]),
    ]
    width, height, gap = 276, 136, 16
    total_h = 2 * height + gap
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="860" height="{total_h}" '
           f'viewBox="0 0 860 {total_h}" role="img" aria-label="Results of the release campaign">']
    for i, (number, label, color, notes) in enumerate(cards):
        x, y = (i % 3) * (width + gap) + 0.5, (i // 3) * (height + gap) + 0.5
        out.append(f'<rect x="{x}" y="{y}" width="{width - 1}" height="{height - 1}" rx="12" '
                   f'fill="{t["box"]}" stroke="{t["edge"]}"/>')
        out.append(text(x + 20, y + 50, number, color, 36, 700))
        out.append(text(x + 20, y + 78, label, t["title"], 16, 600))
        for row, note in enumerate(notes):
            out.append(text(x + 20, y + 102 + 18 * row, note, t["sub"], 13))
    out.append("</svg>")
    return "\n".join(out) + "\n"


def ub_excerpt(source, evoir, asm):
    """The function, the EvoIR block with its explicit check, and the assembly of its trap."""
    lines = evoir.splitlines()
    checks = [i for i, line in enumerate(lines) if line.strip().startswith("ub.assume.")]
    if not checks:
        raise ValueError("the EvoIR holds no explicit check")
    at = checks[0]
    first = max(i for i in range(at) if lines[i].startswith("^"))
    last = next(i for i in range(at, len(lines)) if lines[i].split()[:1] and
                lines[i].split()[0] in ("br", "cond_br", "ret"))
    operand = lines[at].split()[-1]
    marked = {at} | {i for i in range(first, at) if lines[i].strip().startswith(operand + " =")}
    block = [(lines[i].strip(), i in marked) for i in range(first, last + 1)]
    code = asm.splitlines()
    branch = next((i for i, line in enumerate(code) if line.strip().startswith("b.vs ")), None)
    if branch is None:
        raise ValueError("the assembly holds no overflow branch")
    label = code[branch].split()[-1] + ":"
    start = max(i for i in range(branch) if code[i].endswith(":") and not code[i].startswith(" "))
    trap = code.index(label) if label in code else None
    if trap is None or trap + 1 >= len(code) or not code[trap + 1].strip().startswith("brk "):
        raise ValueError("the assembly holds no trap at the overflow label")
    excerpt = [(code[i], i in (branch - 1, branch)) for i in range(start, branch + 1)]
    if trap != branch + 1:
        excerpt.append(("    ...", False))
    excerpt += [(code[trap], False), (code[trap + 1], True)]
    return {"source": source.splitlines(), "evoir": block, "asm": excerpt}


def ub_data():
    """Compile examples/abs.c with the release compiler, for the Linux target of `evo prove`."""
    evocc = BIN / "evocc"
    if not evocc.is_file():
        raise ValueError(f"{evocc} is missing; build the release compiler first")
    source = EXAMPLES / "abs.c"
    outputs = [subprocess.run([str(evocc), "--target=aarch64-unknown-linux-gnu", f"--emit={kind}",
                               str(source)], capture_output=True, text=True, check=True).stdout
               for kind in ("evoir", "asm")]
    return ub_excerpt(source.read_text(encoding="utf-8"), *outputs)


def explicit_check(data, theme):
    """C source, then its EvoIR with the explicit check, then the assembly with the trap."""
    t = THEMES[theme]
    columns = [("C source", "abs.c", [(line, False) for line in data["source"]],
                "abs(INT_MIN) overflows"),
               ("EvoIR", "the rule of C stays a check", data["evoir"], "no pass may remove it"),
               ("AArch64", "the check becomes a trap", data["asm"], "overflow stops at brk")]
    char, row_h, pad, arrow = 7.83, 21, 14, 44
    widths = [math.ceil(char * max(len(line) for line, _ in lines)) + 2 * pad
              for _, _, lines, _ in columns]
    rows = max(len(lines) for _, _, lines, _ in columns)
    top, panel_h = 54, 18 + row_h * rows
    left = (860 - sum(widths) - arrow * (len(columns) - 1)) // 2
    total_h = top + panel_h + 34
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="860" height="{total_h}" '
           f'viewBox="0 0 860 {total_h}" role="img" aria-label="abs.c, its EvoIR and its assembly">']
    x = left
    for i, (title, subtitle, lines, note) in enumerate(columns):
        w = widths[i]
        out.append(text(x, 20, title, t["title"], 16, 600))
        out.append(text(x, 40, subtitle, t["sub"], 13))
        out.append(f'<rect x="{x + 0.5}" y="{top + 0.5}" width="{w - 1}" height="{panel_h - 1}" '
                   f'rx="10" fill="{t["code"]}" stroke="{t["edge"]}"/>')
        for row, (line, mark) in enumerate(lines):
            y = top + 9 + row_h * row
            if mark:
                out.append(f'<rect x="{x + 6}" y="{y}" width="{w - 12}" height="{row_h - 2}" rx="4" '
                           f'fill="{t["mark"]}" stroke="{t["mark_edge"]}"/>')
            color = t["trap"] if mark and line.strip().startswith("brk") else t["title"]
            weight = ' font-weight="700"' if mark else ""
            out.append(f'<text x="{x + pad}" y="{y + 14}" fill="{color}" font-family="{MONO}" '
                       f'font-size="13" xml:space="preserve"{weight}>{html.escape(line)}</text>')
        out.append(text(x + w // 2, top + panel_h + 24, note, t["sub"], 13, anchor="middle"))
        if i + 1 < len(columns):
            ax = x + w + 8
            mid = top + panel_h // 2
            out.append(f'<path d="M{ax} {mid} H{ax + arrow - 22}" stroke="{t["arrow"]}" stroke-width="2"/>'
                       f'<path d="M{ax + arrow - 14} {mid} l-9 -5.5 v11 z" fill="{t["arrow"]}"/>')
        x += w + arrow
    out.append("</svg>")
    return "\n".join(out) + "\n"


def block_values(lines):
    """Map each value of an EvoIR text to the index of the line that defines it."""
    names = {}
    for i, line in enumerate(lines):
        if " = " in line:
            for name in re.findall(r"%v\d+", line.split(" = ", 1)[0]):
                names[name] = i
    return names


def last_function(lines, before):
    """The header line and the name of the C function that holds line index `before`."""
    header = [line for line in lines[:before + 1] if re.match(r"\w[^;]*\)\s*\{$", line)][-1]
    return header, re.search(r"(\w+)\(", header).group(1)


def stages_facts(source, number, pointer, member):
    """What typed C knows about `pointer->member` on line `number`, checked against the source."""
    lines = source.splitlines()
    header, callee = last_function(lines, number - 1)
    params = re.search(r"\((.*)\)", header).group(1).split(", ")
    position = next((i for i, p in enumerate(params) if re.fullmatch(rf"struct \w+ \*{pointer}", p)),
                    None)
    if position is None:
        raise ValueError(f"`{pointer}` is not a struct pointer parameter of {callee}()")
    tag = params[position].split()[1]
    members = re.search(rf"struct {tag} \{{([^}}]*)\}};", source)
    ctype = members and re.search(rf"(\w[\w ]*?) {member};", members.group(1))
    if not ctype or ctype.group(1) != "int":
        raise ValueError(f"`{pointer}->{member}` is not an int member of struct {tag}")
    calls = [(i, m) for i, line in enumerate(lines) for m in re.finditer(rf"\b{callee}\(([^;]*)\);",
                                                                          line)]
    if len(calls) != 1:
        raise ValueError(f"{callee}() has not exactly one call")
    at, call = calls[0]
    argument = call.group(1).split(", ")[position]
    if not re.fullmatch(r"&\w+", argument):
        raise ValueError(f"the call of {callee}() does not pass the address of an object")
    name = argument[1:]
    declaration = next((i for i in range(at, -1, -1)
                        if re.match(rf"\s*struct {tag} {name}\b", lines[i])), None)
    if declaration is None:
        raise ValueError(f"no struct {tag} {name} is declared before the call of {callee}()")
    if not re.fullmatch(rf"\s*struct {tag} {name};", lines[declaration]):
        raise ValueError(f"struct {tag} {name} has an initializer")
    owner = last_function(lines, declaration)[1]
    if last_function(lines, at)[1] != owner:
        raise ValueError(f"the call of {callee}() is not in {owner}()")
    return tag, owner


def stages_excerpt(source, record, evoir, asm):
    """One line of C at four stages: its source, typed C, typed SSA and the bits that remain."""
    diagnostic = record["diagnostics"][0]
    code, message = diagnostic["code"], diagnostic["message"]
    if (code, message) != ("E3001", "read-of-uninitialized-object"):
        raise ValueError(f"evo check reports {code} {message}, not E3001 for uninitialized bytes")
    span = next(span for span in diagnostic["spans"] if span["primary"])
    number = span["line_start"]
    line = source.splitlines()[number - 1].strip()
    target = re.fullmatch(r"(\w+)->(\w+) \+= 1;", line)
    if not target:
        raise ValueError(f"line {number} is not `pointer->member += 1;`: {line!r}")
    pointer, member = target.groups()
    tag, owner = stages_facts(source, number, pointer, member)

    header, evoir_lines = stages_evoir(evoir)
    asm_lines, trap = stages_asm(asm)
    facts = [([(f"{pointer}->{member}", True), (" is an int of 4 bytes", False)], False),
             ([("its bytes are not initialized", False)], True),
             ([(pointer, True), (f" points into the struct {tag} object of {owner}()", False)], False)]
    return {"source": line, "where": f"{span['file']}, line {number}", "facts": facts,
            "check": f"evo check stops here: {code} {message} at "
                     f"{span['file']}:{number}:{span['column_start']}",
            "header": header, "evoir": evoir_lines, "asm": asm_lines, "trap": trap}


def stages_evoir(evoir):
    """The EvoIR of `p->m += 1`, from the constant and the guarded load to the store."""
    lines = evoir.splitlines()
    names = block_values(lines)

    def defining(name):
        return lines[names[name]] if name in names else ""

    for at, text in enumerate(lines[:-1]):
        add = re.fullmatch(r"\s*(%v\d+) = signed_add_defined (%v\d+), (%v\d+)", text)
        if (add and lines[at + 1].strip() == f"ub.assume.signed_overflow {add.group(1)}"
                and re.fullmatch(rf"\s*{add.group(3)} = const\.bits<32> 1", defining(add.group(3)))
                and " = load " in defining(add.group(2))):
            break
    else:
        raise ValueError("the EvoIR holds no overflow obligation for `+= 1` of a loaded value")
    _, loaded, one = add.groups()
    load = re.fullmatch(rf"\s*\({loaded}, %v\d+, %v\d+\) = load (%v\d+), %v\d+, (%v\d+) : bits<(\d+)>",
                        defining(loaded))
    guard = load and re.fullmatch(rf"\s*{load.group(1)} = access_in_bounds %v\d+, {load.group(2)} "
                                  r"\[size=(\d+), align=\d+\]", defining(load.group(1)))
    if not guard or (int(guard.group(1)), int(load.group(3))) != (4, 32):
        raise ValueError("the load of the member does not read the 4 bytes of an int")
    sums = [re.fullmatch(rf"\s*(%v\d+) = add\.bits<32> {loaded}, {one}", text) for text in lines[at + 2:]]
    result = next((match.group(1) for match in sums if match), None)
    end = next((j for j in range(at + 2, len(lines)) if result and re.fullmatch(
        rf"\s*\(%v\d+, %v\d+\) = store %v\d+, %v\d+, {load.group(2)}, {result}", lines[j])), None)
    if end is None:
        raise ValueError("the EvoIR does not store the sum back to the member")
    start = min(names[one], names[load.group(1)])
    blocks = [j for j in range(end + 1) if lines[j].startswith("^")]
    header = blocks and re.fullmatch(r"(\^\w+)\((.*)\):", lines[blocks[-1]])
    if not header or blocks[-1] > start:
        raise ValueError("the EvoIR of the line is not in one block")
    used, shown = set(re.findall(r"%v\d+", "\n".join(lines[start:end + 1]))), []
    for param in header.group(2).split(", "):
        if param.split(":")[0] in used:
            shown.append(param)
        elif shown[-1:] != ["…"]:
            shown.append("…")
    return (f"{header.group(1)}({', '.join(shown)}):",
            [(lines[j], j in (at, at + 1)) for j in range(start, end + 1)])


def stages_asm(asm):
    """The assembly of `p->m += 1`: its block, the overflow branch and the store; and the trap."""
    code = asm.splitlines()
    adds = next((k for k, text in enumerate(code[:-1]) if re.fullmatch(r"\s*adds w\d+, w\d+, w\d+", text)
                 and code[k + 1].strip().startswith("b.vs ")), None)
    if adds is None:
        raise ValueError("the assembly holds no adds with an overflow branch")
    dest, first, second = re.fullmatch(r"\s*adds (w\d+), (w\d+), (w\d+)", code[adds]).groups()
    labels = [k for k in range(adds) if code[k].endswith(":") and not code[k].startswith(" ")]
    body = code[labels[-1] + 1:adds] if labels else []
    loads = [re.fullmatch(rf"\s*ldr (?:{first}|{second}), (\[x\d+, #\d+\])", text) for text in body]
    ones = [re.fullmatch(rf"\s*movz (?:{first}|{second}), #1", text) for text in body]
    if len(body) != 2 or not any(loads) or not any(ones):
        raise ValueError("the block of the adds does not only load the member and the constant 1")
    field = next(match for match in loads if match).group(1)
    store = next((k for k in range(adds + 2, len(code)) if code[k].strip() == f"str {dest}, {field}"),
                 None)
    if store is None or any(not code[k].endswith(":") for k in range(adds + 2, store)):
        raise ValueError("the sum is not stored back right after the overflow branch")
    target, seen = code[adds + 1].split()[-1], set()
    while target + ":" in code and target not in seen and code.index(target + ":") + 1 < len(code):
        seen.add(target)
        step = code[code.index(target + ":") + 1].strip()
        if step == "brk #3004":
            return [(code[k], k in (adds, adds + 1)) for k in range(labels[-1], store + 1)], step
        if not step.startswith("b "):
            break
        target = step.split()[1]
    raise ValueError("the overflow branch reaches no brk #3004 trap")


def stages_data():
    """Check and compile examples/mean.c with the release build, for the target of figure C."""
    tools = [BIN / "evo", BIN / "evocc"]
    for tool in tools:
        if not tool.is_file():
            raise ValueError(f"{tool} is missing; build the release compiler first")
    source = EXAMPLES / "mean.c"
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copy(source, tmp)
        env = dict(os.environ, PATH=f"{BIN}{os.pathsep}{os.environ['PATH']}")
        check = subprocess.run([str(tools[0]), "--json", "check", "mean.c"], cwd=tmp, env=env,
                               capture_output=True, text=True)
    if check.returncode != 1 or not check.stdout.strip():
        raise ValueError(f"evo check mean.c exited {check.returncode}, not 1 with a finding")
    record = json.loads(check.stdout.splitlines()[0])
    outputs = [subprocess.run([str(tools[1]), "--target=aarch64-unknown-linux-gnu", f"--emit={kind}",
                               str(source)], capture_output=True, text=True, check=True).stdout
               for kind in ("evoir", "asm")]
    return stages_excerpt(source.read_text(encoding="utf-8"), record, *outputs)


STAGE_X, STAGE_W = 172, 688
CODE_SIZE, TEXT_SIZE = 13, 14


def fit(parts, room):
    """Refuse a line that would not fit: `parts` is a list of (text, font size)."""
    width = sum(0.61 * size * len(value) for value, size in parts)
    if width > room:
        raise ValueError(f"{''.join(value for value, _ in parts)!r} is too wide for its panel")


def stages(data, theme):
    """One line of C, top to bottom, at four stages: what the program still knows at each."""
    t = THEMES[theme]
    rows = [
        ("C source", data["where"], [(data["source"], False)], []),
        ("C-HIR", "typed C", data["facts"], [(data["check"], t["check"], 600)]),
        ("EvoIR", "typed SSA", [(data["header"], False)] + data["evoir"],
         [("Each rule of C is a predicate and an obligation that no pass may delete.", t["rules"], 600),
          ("Lean proves the rewrite rules on bits<N> values, for every N.", t["rules"], 600)]),
        ("AArch64", "bits only", data["asm"],
         [("Only bits remain: no type, no init state, no pointer object.", t["sub"], None),
          (f"The overflow rule stays a trap: b.vs leads to {data['trap']}.", t["sub"], None)]),
    ]
    pad, out, y = 14, [], 2
    for i, (name, subtitle, lines, notes) in enumerate(rows):
        facts = name == "C-HIR"
        row_h = 26 if facts else 21
        panel_h = row_h * len(lines) + 16
        fit([(name, 17)], STAGE_X - 12)
        fit([(subtitle, TEXT_SIZE)], STAGE_X - 12)
        out.append(text(0, y + 20, name, t["title"], 17, 700))
        out.append(text(0, y + 42, subtitle, t["sub"], TEXT_SIZE))
        out.append(f'<rect x="{STAGE_X + 0.5}" y="{y + 0.5}" width="{STAGE_W - 1}" '
                   f'height="{panel_h - 1}" rx="10" fill="{t["box"] if facts else t["code"]}" '
                   f'stroke="{t["edge"]}"/>')
        for row, (line, mark) in enumerate(lines):
            top = y + 8 + row_h * row
            if mark:
                out.append(f'<rect x="{STAGE_X + 6}" y="{top}" width="{STAGE_W - 12}" '
                           f'height="{row_h - 2}" rx="4" fill="{t["mark"]}" stroke="{t["mark_edge"]}"/>')
            weight = f' font-weight="{600 if facts else 700}"' if mark else ""
            if facts:
                parts = [(value, CODE_SIZE if is_code else TEXT_SIZE, MONO if is_code else SANS)
                         for value, is_code in line]
                fit([(value, size) for value, size, _ in parts], STAGE_W - 2 * pad)
                tspans = "".join(f'<tspan font-family="{font}" font-size="{size}">'
                                 f'{html.escape(value)}</tspan>' for value, size, font in parts)
                out.append(f'<text x="{STAGE_X + pad}" y="{top + 17}" fill="{t["title"]}" '
                           f'xml:space="preserve"{weight}>{tspans}</text>')
            else:
                fit([(line, CODE_SIZE)], STAGE_W - 2 * pad)
                out.append(f'<text x="{STAGE_X + pad}" y="{top + 14}" fill="{t["title"]}" '
                           f'font-family="{MONO}" font-size="{CODE_SIZE}" xml:space="preserve"'
                           f'{weight}>{html.escape(line)}</text>')
        y += panel_h
        for note, color, note_weight in notes:
            fit([(note, TEXT_SIZE)], STAGE_W)
            y += 22
            out.append(text(STAGE_X, y, note, color, TEXT_SIZE, note_weight))
        if i + 1 < len(rows):
            mid = STAGE_X + STAGE_W // 2
            out.append(f'<path d="M{mid} {y + 12} V{y + 30}" stroke="{t["arrow"]}" stroke-width="2"/>'
                       f'<path d="M{mid} {y + 38} l-6 -10 h12 z" fill="{t["arrow"]}"/>')
            y += 50
    height = y + 8
    return "\n".join([f'<svg xmlns="http://www.w3.org/2000/svg" width="860" height="{height}" '
                      f'viewBox="0 0 860 {height}" role="img" aria-label="One line of C at four '
                      f'stages of EvoCC">'] + out + ["</svg>"]) + "\n"


def rendered(transcript, with_compiler=True):
    """Every generated figure: its path and its text."""
    files = {}
    board = scoreboard_data()
    agents = agent_results()
    check = ub_data() if with_compiler else None
    line = stages_data() if with_compiler else None
    for theme in THEMES:
        files[OUT / HIGHLIGHTS[theme]] = highlights(transcript, theme)
        files[OUT / f"pipeline-{theme}.svg"] = pipeline(theme)
        files[OUT / f"propose-check-{theme}.svg"] = propose_check(theme)
        files[OUT / f"proved-rewrite-{theme}.svg"] = proved_rewrite(theme)
        files[OUT / f"scoreboard-{theme}.svg"] = scoreboard(board, theme)
        if check is not None:
            files[OUT / f"explicit-check-{theme}.svg"] = explicit_check(check, theme)
            files[OUT / f"stages-{theme}.svg"] = stages(line, theme)
        files[PAPER / f"agent-effort-{theme}.svg"] = effort_chart(theme)
        files[OUT / f"agent-results-{theme}.svg"] = agent_results_chart(agents, theme)
        files[BOOK / f"agent-results-{theme}.svg"] = agent_results_chart(agents, theme, card=True)
    return files


def stale(files):
    """The files whose text on disk differs from the rendered text."""
    return [path for path, value in files.items()
            if not path.is_file() or path.read_text(encoding="utf-8") != value]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true",
                      help="draw every figure again and compare; write nothing")
    mode.add_argument("--render-only", action="store_true",
                      help="draw every figure from the saved transcript; run no session")
    args = parser.parse_args(argv)
    try:
        if args.check or args.render_only:
            transcript = json.loads(TRANSCRIPT.read_text(encoding="utf-8"))
        else:
            transcript = capture_highlights()
        validate_transcript(transcript)
        files = rendered(transcript)
        if args.check:
            differ = stale(files)
            for path in differ:
                print(f"stale: {path.relative_to(ROOT)}", file=sys.stderr)
            if differ:
                return 1
            print("README, paper and book figures match their transcript, receipts and compiler output")
            return 0
        OUT.mkdir(parents=True, exist_ok=True)
        PAPER.mkdir(parents=True, exist_ok=True)
        BOOK.mkdir(parents=True, exist_ok=True)
        if not args.render_only:
            TRANSCRIPT.write_text(json.dumps(transcript, indent=2, ensure_ascii=False) + "\n",
                                  encoding="utf-8")
        for path, value in files.items():
            path.write_text(value, encoding="utf-8")
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"render-readme: {error}", file=sys.stderr)
        return 1
    print("wrote docs/images/readme, docs/images/paper and book/src/images")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
