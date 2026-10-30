"""The README figures follow their receipts, their transcript and the release compiler."""

import copy
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("render_readme", ROOT / "tools/render-readme.py")
render = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(render)


def asm_block(text):
    """The assembly block of the checked abs, wherever it is in the document."""
    blocks = [part.split("```", 1)[0] for part in text.split("```asm\n")[1:]]
    return next(block for block in blocks if ".Labs_block4:" in block).splitlines()


def write(root, name, value):
    path = root / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))


def receipts(root, **changes):
    """Write the receipt fields that the scoreboard reads."""
    lanes = {name: {"status": "PASS", "passed": 9, "total": 9} for name in render.LANES}
    lanes["torture"] = {"status": "PROFILE_CLEAN", "dispositions": {"compiler-failure": 0}}
    lanes["csmith"] = {"status": "INCOMPLETE", "outcomes": {"agree": 7, "oracle-unusable": 3},
                       "total": 10}
    lanes["musl"] = {"status": "INCOMPLETE", "testVerdict": "libc-test-clean", "failingUnits": []}
    lanes["ctest"] = {"status": "PASS", "passed": 220, "total": 220}
    lanes["doom"] = {"status": "PASS", "passed": 81, "total": 81}
    for name, row in changes.items():
        lanes[name] = row
    write(root, "tests/contracts/bench-receipt.json", {"dirty": False, "benchmarks": lanes})
    write(root, "tests/contracts/kernel-direct-virt-receipt.json", {
        "status": "BUILD_BOOT_SELECTED_SELFTEST_PASS",
        "counts": {"evoccCObjects": 2799, "gccTargetCSourceRecords": 0},
        "boots": {name: {"verdict": "PASS"} for name in ("a", "b", "c", "d")},
        "selftests": {"status": "PASS", "subject": {"subtests": {"PASS": 125}}}})
    write(root, "tests/contracts/ladder-csmith-receipt.json",
          {"latest": {"programs": 10, "agreeing": 7, "refusalsByCode": {}}})
    write(root, "tests/contracts/lean-p0-receipt.json", {"binding": {
        "registry_rules": 55, "guard_registry": {"rules": "7"}}})
    write(root, "docs/images/showcase/captures.json", {"logs": {"sqlite": {"segments": [
        {"text": ["x"]}, {"text": ["0 errors out of 329803 tests in 01:36 on host Linux 64-bit"]}]}}})


class ProvePanelTests(unittest.TestCase):
    def test_documents_show_the_expected_lines_of_the_checked_abs(self):
        expected = render.PROVE_ASSEMBLY.splitlines()
        self.assertEqual(len(expected), 3)
        for name in ("README.md", "docs/paper.md"):
            with self.subTest(document=name):
                block = asm_block((ROOT / name).read_text(encoding="utf-8"))
                self.assertEqual([line for line in block if line in expected], expected)


class ScoreboardTests(unittest.TestCase):
    def test_counts_come_from_the_receipts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipts(root)
            with patch.object(render, "ROOT", root):
                data = render.scoreboard_data()
            self.assertEqual(data["linux"], {"compiled": 2799, "total": 2799, "boots": 4,
                                             "subtests": 125, "subtestsTotal": 125})
            self.assertEqual(data["sqlite"], {"errors": 0, "tests": 329803})
            self.assertEqual(data["lanes"], {"lanes": len(render.LANES), "failures": 0})
            self.assertEqual(data["ctest"], {"passed": 220, "total": 220})
            self.assertEqual(data["csmith"], {"programs": 10, "agree": 7, "mismatches": 0})
            self.assertEqual(data["lean"], {"rules": 62})

    def test_a_compiler_failure_or_an_unknown_lane_changes_the_card(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            receipts(root, torture={"status": "PROFILE_CLEAN", "dispositions": {"compiler-failure": 2}})
            with patch.object(render, "ROOT", root):
                self.assertEqual(render.scoreboard_data()["lanes"]["failures"], 2)
            receipts(root, toybox={"status": "INCOMPLETE"})
            with patch.object(render, "ROOT", root), self.assertRaisesRegex(ValueError, "toybox"):
                render.scoreboard_data()
            receipts(root, csmith={"status": "INCOMPLETE", "outcomes": {"agree": 7, "mismatch": 3},
                                   "total": 10})
            with patch.object(render, "ROOT", root):
                self.assertEqual(render.scoreboard_data()["lanes"]["failures"], 3)

    def test_readme_names_the_numbers_date_and_compiler_of_the_scoreboard(self):
        data = render.scoreboard_data()
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        alt = re.search(r'scoreboard-light\.svg" alt="([^"]+)"', readme).group(1)
        linux, csmith = data["linux"], data["csmith"]
        for value in (f"{linux['compiled']:,} of {linux['total']:,}",
                      f"{linux['subtests']} of {linux['subtestsTotal']}",
                      f"{data['sqlite']['errors']} errors in {data['sqlite']['tests']:,}",
                      f"{data['lanes']['failures']} compiler failures in {data['lanes']['lanes']}",
                      f"{data['ctest']['passed']} of {data['ctest']['total']}",
                      f"{csmith['mismatches']} Csmith mismatches",
                      f"{csmith['agree']:,} of {csmith['programs']:,}",
                      f"{data['lean']['rules']} rewrite rules"):
            self.assertIn(value, alt)
        kernel = render.receipt("kernel-direct-virt-receipt.json")
        run = render.receipt("bench-receipt.json")["run"]
        caption = readme.split("scoreboard-light.svg", 1)[1].split("## Quick start", 1)[0]
        self.assertIn(f"`{kernel['compilerCommit'][:8]}`", caption)
        self.assertIn(f"{run[:4]}-{run[4:6]}-{run[6:8]}", caption)

    def test_committed_scoreboard_is_current(self):
        data = render.scoreboard_data()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                committed = (render.OUT / f"scoreboard-{theme}.svg").read_text(encoding="utf-8")
                self.assertEqual(committed, render.scoreboard(data, theme))


class ExplicitCheckFigureTests(unittest.TestCase):
    EVOIR = """func @abs(%arg0: bits<32>) -> (bits<32>) cc = target_default
{
^entry(%v0: bits<32>):
    cond_br %v2, ^b1(%v0), ^b2(%v0)

^b2(%v4: bits<32>):
    %v5 = const.bits<32> 0
    %v6 = signed_sub_defined %v5, %v4
    ub.assume.signed_overflow %v6
    %v7 = sub.bits<32> %v5, %v4
    br ^b3(%v4, %v7)

^b3(%v8: bits<32>, %v9: bits<32>):
    ret %v9
}
"""
    ASM = """abs:
.Labs_block2:
    movz w1, #0
    subs w0, w1, w0
    b.vs .Labs_block4
.Labs_block3:
    ret
.Labs_block4:
    brk #3004
.size abs, .-abs
"""

    def test_the_excerpt_marks_the_check_and_the_trap(self):
        excerpt = render.ub_excerpt("int abs(int a);\n", self.EVOIR, self.ASM)
        self.assertEqual([line for line, mark in excerpt["evoir"] if mark],
                         ["%v6 = signed_sub_defined %v5, %v4", "ub.assume.signed_overflow %v6"])
        self.assertEqual(excerpt["evoir"][0], ("^b2(%v4: bits<32>):", False))
        self.assertEqual([line for line, mark in excerpt["asm"] if mark],
                         ["    subs w0, w1, w0", "    b.vs .Labs_block4", "    brk #3004"])
        with self.assertRaisesRegex(ValueError, "no explicit check"):
            render.ub_excerpt("", self.EVOIR.replace("ub.assume", "op"), self.ASM)
        with self.assertRaisesRegex(ValueError, "no trap"):
            render.ub_excerpt("", self.EVOIR, self.ASM.replace("brk #3004", "ret"))

    def test_committed_figure_is_the_output_of_the_release_compiler(self):
        if not (render.BIN / "evocc").is_file():
            self.skipTest("needs the release build in target/frontend/release")
        data = render.ub_data()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                committed = (render.OUT / f"explicit-check-{theme}.svg").read_text(encoding="utf-8")
                self.assertEqual(committed, render.explicit_check(data, theme))


class StagesFigureTests(unittest.TestCase):
    SOURCE = (ROOT / "examples/mean.c").read_text(encoding="utf-8")
    RECORD = {"exit_code": 1, "diagnostics": [{
        "code": "E3001", "message": "read-of-uninitialized-object",
        "spans": [{"file": "mean.c", "line_start": 7, "column_start": 9, "primary": True}]}]}
    EVOIR = """func @mean(%arg0: ptr<generic>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
{
^b3(%v29: bits<32>, %v30: ptr<generic>, %v31: bits<32>, %v32: ptr<generic>, %v33: bits<32>, %v34: mem<all>, %v35: world):
    %v36 = const.bits<32> 1
    %v37 = access_in_bounds %v34, %v32 [size=4, align=4]
    (%v38, %v39, %v40) = load %v37, %v35, %v32 : bits<32>
    %v41 = signed_add_defined %v38, %v36
    ub.assume.signed_overflow %v41
    %v42 = add.bits<32> %v38, %v36
    %v43 = access_in_bounds %v39, %v32 [size=4, align=4]
    (%v44, %v45) = store %v43, %v40, %v32, %v42
    %v46 = const.bits<64> 4
    br ^b6(%v29, %v30, %v31, %v32, %v44, %v45)
}
"""
    ASM = """mean:
.Lmean_block3:
    movz w5, #1
    ldr w6, [x2, #0]
    adds w5, w6, w5
    b.vs .Lmean_block18
.Lmean_block12:
    str w5, [x2, #0]
    add x5, x2, #4
.Lmean_block11:
    brk #3004
.Lmean_block18:
    b .Lmean_block11
.size mean, .-mean
"""

    def excerpt(self, source=None, record=None, evoir=None, asm=None):
        return render.stages_excerpt(source or self.SOURCE, record or self.RECORD,
                                     evoir or self.EVOIR, asm or self.ASM)

    def test_one_line_of_c_at_each_stage(self):
        data = self.excerpt()
        self.assertEqual((data["source"], data["where"]), ("s->count += 1;", "mean.c, line 7"))
        self.assertEqual([("".join(text for text, _ in parts), mark) for parts, mark in data["facts"]],
                         [("s->count is an int of 4 bytes", False),
                          ("its bytes are not initialized", True),
                          ("s points into the struct stats object of mean()", False)])
        self.assertEqual(data["check"],
                         "evo check stops here: E3001 read-of-uninitialized-object at mean.c:7:9")
        self.assertEqual(data["header"], "^b3(…, %v32: ptr<generic>, …, %v34: mem<all>, %v35: world):")
        body = self.EVOIR.splitlines()[3:11]
        self.assertEqual([line for line, _ in data["evoir"]], body)
        self.assertEqual([line.strip() for line, mark in data["evoir"] if mark],
                         ["%v41 = signed_add_defined %v38, %v36", "ub.assume.signed_overflow %v41"])
        self.assertEqual([line for line, _ in data["asm"]], self.ASM.splitlines()[1:8])
        self.assertEqual([line.split()[0] for line, mark in data["asm"] if mark], ["adds", "b.vs"])
        self.assertEqual(data["trap"], "brk #3004")

    def test_facts_and_lines_that_do_not_match_are_refused(self):
        changes = [
            ({"source": self.SOURCE.replace("struct stats s;", "struct stats s = {0};")}, "initializer"),
            ({"source": self.SOURCE.replace("int count;", "long count;")}, "not an int member"),
            ({"record": {"exit_code": 1, "diagnostics": [{
                "code": "E3004", "message": "signed-integer-overflow",
                "spans": self.RECORD["diagnostics"][0]["spans"]}]}}, "E3001"),
            ({"evoir": self.EVOIR.replace("[size=4, align=4]", "[size=8, align=8]")}, "4 bytes"),
            ({"evoir": self.EVOIR.replace("ub.assume.signed_overflow", "nop")}, "overflow obligation"),
            ({"asm": self.ASM.replace("brk #3004", "ret")}, "no brk #3004 trap"),
        ]
        for change, message in changes:
            with self.subTest(change=message), self.assertRaisesRegex(ValueError, message):
                self.excerpt(**change)

    def test_a_line_too_wide_for_its_panel_is_refused(self):
        for key in ("evoir", "asm"):
            with self.subTest(stage=key):
                data = self.excerpt()
                line, mark = data[key][0]
                data[key][0] = (line + "  ; " + "x" * 90, mark)
                with self.assertRaisesRegex(ValueError, "too wide"):
                    render.stages(data, "light")

    def test_text_stays_legible_in_the_readme_column(self):
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                svg = render.stages(self.excerpt(), theme)
                scale = 830 / float(re.search(r'viewBox="0 0 ([\d.]+) ', svg).group(1))
                sizes = re.findall(r'font-family="([^"]+)" font-size="([\d.]+)"', svg)
                self.assertTrue(sizes)
                for font, size in sizes:
                    self.assertGreaterEqual(float(size) * scale, 12 if font == render.MONO else 13)

    def test_readme_shows_the_figure_under_why_evocc(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        section = readme.split("## Why EvoCC\n", 1)[1].split("\n## ", 1)[0]
        self.assertLess(section.index("propose-check-light.svg"), section.index("\n### "))
        self.assertLess(section.index("- **Built for agents.**"), section.index("\n### "))
        self.assertIn('srcset="docs/images/readme/stages-dark.svg"', section)
        alt = re.search(r'stages-light\.svg" alt="([^"]+)"', section).group(1)
        for value in ("s->count += 1;", "E3001 read-of-uninitialized-object at mean.c:7:9",
                      "signed_add_defined", "ub.assume.signed_overflow", "ptr<generic>", "mem<all>",
                      "world", "ldr", "adds", "b.vs", "str", "brk #3004"):
            self.assertIn(value, alt)

    def test_committed_figure_is_the_output_of_the_release_compiler(self):
        if not all((render.BIN / tool).is_file() for tool in ("evocc", "evo")):
            self.skipTest("needs the release build in target/frontend/release")
        data = render.stages_data()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                committed = (render.OUT / f"stages-{theme}.svg").read_text(encoding="utf-8")
                self.assertEqual(committed, render.stages(data, theme))
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        alt = re.search(r'stages-light\.svg" alt="([^"]+)"', readme).group(1)
        for value in (data["source"], data["check"].split(": ", 1)[1], data["trap"]):
            self.assertIn(value, alt)


class HighlightsTests(unittest.TestCase):
    def transcript(self):
        return json.loads(render.TRANSCRIPT.read_text(encoding="utf-8"))

    def test_the_committed_transcript_keeps_its_claims(self):
        render.validate_transcript(self.transcript())

    def test_a_changed_transcript_is_refused(self):
        changes = (("Catches what UBSan misses", 1, "4\n", "returned the right answer"),
                   ("Proof-carrying compilation", 2, "    ret\n", "abs.s"),
                   ("A drop-in cc", 1, "Hello\n", "hello"))
        for title, step, output, message in changes:
            with self.subTest(panel=title):
                transcript = copy.deepcopy(self.transcript())
                panel = next(p for p in transcript["panels"] if p["title"] == title)
                panel["steps"][step][1] = output
                with self.assertRaisesRegex(ValueError, message):
                    render.validate_transcript(transcript)

    def test_both_variants_follow_the_transcript(self):
        transcript = self.transcript()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                name = render.HIGHLIGHTS[theme]
                committed = (render.OUT / name).read_text(encoding="utf-8")
                self.assertEqual(committed, render.highlights(transcript, theme))
        light, dark = (render.highlights(transcript, theme) for theme in ("light", "dark"))
        for key in ("window", "window_edge"):
            dark = dark.replace(render.WINDOW["dark"][key], render.WINDOW["light"][key])
        self.assertEqual(dark, light)

    def test_check_names_a_stale_figure(self):
        with tempfile.TemporaryDirectory() as directory:
            out, book = Path(directory), Path(directory) / "book"
            book.mkdir()
            with (patch.object(render, "OUT", out), patch.object(render, "PAPER", out),
                  patch.object(render, "BOOK", book)):
                files = {path: text for path, text in render.rendered(self.transcript(),
                                                                       with_compiler=False).items()}
                for path, text in files.items():
                    path.write_text(text, encoding="utf-8")
                self.assertEqual(render.stale(files), [])
                victim = out / "scoreboard-light.svg"
                victim.write_text("stale", encoding="utf-8")
                self.assertEqual(render.stale(files), [victim])


class AgentResultsFigureTests(unittest.TestCase):
    def test_committed_figure_is_current(self):
        data = render.agent_results()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                committed = (render.OUT / f"agent-results-{theme}.svg").read_text(encoding="utf-8")
                self.assertEqual(committed, render.agent_results_chart(data, theme))
                committed = (render.BOOK / f"agent-results-{theme}.svg").read_text(encoding="utf-8")
                self.assertEqual(committed, render.agent_results_chart(data, theme, card=True))

    def test_the_book_card_draws_the_same_figure_on_its_surface(self):
        data = render.agent_results()
        for theme, surface in (("light", "#ffffff"), ("dark", "#0d1117")):
            with self.subTest(theme=theme):
                plain = render.agent_results_chart(data, theme).splitlines()
                card = render.agent_results_chart(data, theme, card=True).splitlines()
                self.assertIn('viewBox="-16 -16 862 ', card[0])
                self.assertIn(f'rx="6" fill="{surface}"', card[1])
                self.assertEqual(card[2:], plain[1:])

    def test_readme_shows_every_value_in_the_alt_text(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        section = readme.split("## Built for agents\n", 1)[1].split("\n## ", 1)[0]
        self.assertIn('srcset="docs/images/readme/agent-results-dark.svg"', section)
        alt = re.search(r'agent-results-light\.svg" alt="([^"]+)"', section).group(1)
        self.assertEqual(alt, render.agent_results_alt(render.agent_results()))

    def test_book_shows_the_card_with_every_value_in_the_alt_text(self):
        book = (ROOT / "book" / "src" / "evaluation-agents.md").read_text(encoding="utf-8")
        self.assertIn('srcset="images/agent-results-dark.svg"', book)
        alt = re.search(r'src="images/agent-results-light\.svg" alt="([^"]+)"', book).group(1)
        self.assertEqual(alt, render.agent_results_alt(render.agent_results()))

    def test_rows_agree_with_the_registered_analyses(self):
        silent, controls = (rows for _, rows in render.agent_results()["groups"])
        five = render.receipt("agent-repair-5-20261010.json")
        # The registered tests of experiment 5 sum item means of two runs: half the totals.
        for (_, _, _, cells), test in ((silent[0], five["P1"]), (silent[1], five["P3"])):
            self.assertEqual((cells["c1"]["effort"], cells["c3"]["effort"]),
                             (2 * test["c1"], 2 * test["c3"]))
        tests = {row["model"]: row for row in render.receipt("agent-repair-8-20261029.json")}
        for (_, name), row, control in zip(render.CHAT_MODELS, silent[2:], controls[1:]):
            table = tests[render.receipt(name)["plan"]["model"]["model"]]["table"]
            for (_, _, _, cells), key in ((row, "silent"), (control, "controls")):
                self.assertEqual((cells["c1"]["repaired"], cells["c3"]["repaired"]),
                                 (table[key]["c1"], table[key]["c3"]))
            for condition in ("c1", "c3"):
                self.assertAlmostEqual(row[3][condition]["effort"],
                                       table["silent_tokens"][condition], delta=0.05)

    def test_a_stopped_run_or_a_failed_prediction_is_refused(self):
        original = render.receipt

        def stopped(name):
            value = copy.deepcopy(original(name))
            if name == render.CHAT_MODELS[0][1]:
                value["stop_reason"] = "retry_failed:http_429"
            return value

        def failed(name):
            value = copy.deepcopy(original(name))
            if name == "agent-repair-8-20261029.json":
                for row in value:
                    row["P3"]["p"] = 0.2
            return value

        for receipts, message in ((stopped, "stopped"), (failed, "p < 0.01")):
            with self.subTest(message=message), patch.object(render, "receipt", receipts):
                with self.assertRaisesRegex(ValueError, message):
                    render.agent_results()

    def test_text_stays_legible_in_the_readme_column(self):
        data = render.agent_results()
        for theme in render.THEMES:
            with self.subTest(theme=theme):
                svg = render.agent_results_chart(data, theme)
                scale = 830 / float(re.search(r'viewBox="0 0 ([\d.]+) ', svg).group(1))
                sizes = [float(size) for size in re.findall(r'font-size="([\d.]+)"', svg)]
                self.assertTrue(sizes)
                self.assertGreaterEqual(min(sizes) * scale, 13)


if __name__ == "__main__":
    unittest.main()
