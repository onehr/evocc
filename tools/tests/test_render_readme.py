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
            out = Path(directory)
            with patch.object(render, "OUT", out), patch.object(render, "PAPER", out):
                files = {path: text for path, text in render.rendered(self.transcript(),
                                                                       with_compiler=False).items()}
                for path, text in files.items():
                    path.write_text(text, encoding="utf-8")
                self.assertEqual(render.stale(files), [])
                victim = out / "scoreboard-light.svg"
                victim.write_text("stale", encoding="utf-8")
                self.assertEqual(render.stale(files), [victim])


if __name__ == "__main__":
    unittest.main()
