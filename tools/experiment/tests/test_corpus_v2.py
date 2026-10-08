"""Qualify the second study's inputs before any model difficulty trials."""
import collections
import difflib
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from tools.experiment import repair

ROOT = Path(__file__).resolve().parents[3]
ITEMS = ROOT / "tests/fixtures/agent-repair-2"
FIELDS = {"program.c", "fixed.c", "reference_main.c", "expected.txt",
          "origin.md", "class", "fix-spans.json"}

# These are reviewed fixtures, not model responses. One isolated guest is
# sufficient for construction checks; measured responses keep their separate
# per-command isolation from repair.py. Each partial fix is a negative control.
GUEST = r'''
import difflib, json, pathlib, subprocess, tempfile

def run(argv, cwd):
    try:
        result = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=20)
        return dict(exit_code=result.returncode, stdout=result.stdout, stderr=result.stderr)
    except subprocess.TimeoutExpired as error:
        return dict(exit_code=124, stdout=(error.stdout or b"").decode("utf-8", "replace"),
                    stderr=(error.stderr or b"").decode("utf-8", "replace"), timeout=True)

rows = []
for item in sorted(pathlib.Path("/corpus").iterdir()):
    if not item.is_dir():
        continue
    original = (item / "program.c").read_text()
    fixed = (item / "fixed.c").read_text()
    before, after = original.splitlines(True), fixed.splitlines(True)
    edits = [op for op in difflib.SequenceMatcher(a=before, b=after, autojunk=False).get_opcodes() if op[0] != "equal"]
    variants = [("program.c", original), ("fixed.c", fixed)]
    for index, (_, lo, hi, new_lo, new_hi) in enumerate(edits):
        variants.append(("one-repair-" + str(index), "".join(before[:lo] + after[new_lo:new_hi] + before[hi:])))
    for name, source in variants:
        with tempfile.TemporaryDirectory() as scratch:
            cwd = pathlib.Path(scratch)
            (cwd / "program.c").write_text(source)
            row = dict(item=item.name, version=name, feedback={})
            if name in ("program.c", "fixed.c"):
                for compiler, warnings in (("gcc-14", ["-Wall", "-Wextra"]), ("clang-19", ["-Weverything"])):
                    built = run([compiler, *warnings, "-fsanitize=undefined", "-o", "prog", "program.c"], cwd)
                    executed = run([str(cwd / "prog")], cwd) if built["exit_code"] == 0 else None
                    row["feedback"][compiler] = dict(compile=built, run=executed)
            built = run(["gcc-14", "-O0", "-Dmain=program_main", "-c", "program.c", "-o", "program.o"], cwd)
            linked = run(["gcc-14", "-O0", "program.o", str(item / "reference_main.c"), "-o", "score"], cwd) if built["exit_code"] == 0 else None
            scored = run([str(cwd / "score")], cwd) if linked and linked["exit_code"] == 0 else None
            row["score"] = dict(compile=built, link=linked, run=scored)
            rows.append(row)
print(json.dumps(rows))
'''


def items():
    return sorted(p for p in ITEMS.iterdir() if p.is_dir())


class CorpusShapeTests(unittest.TestCase):
    def test_population_spans_and_sources(self):
        population = items()
        self.assertEqual(len(population), 30)
        self.assertEqual(collections.Counter((p / "class").read_text().strip() for p in population),
                         {"json_and_text": 10, "integer_and_checksum": 10, "binary_codecs": 10})
        synthetic = 0
        for item in population:
            with self.subTest(item=item.name):
                self.assertEqual({p.name for p in item.iterdir()}, FIELDS)
                source = (item / "program.c").read_text()
                fixed = (item / "fixed.c").read_text()
                self.assertTrue(150 <= len(source.splitlines()) <= 400)
                self.assertEqual(repair.extract("```c\n" + source + "```"), source)
                self.assertNotEqual(source, fixed)
                spans = json.loads((item / "fix-spans.json").read_text())["spans"]
                self.assertIn(len(spans), (2, 3))
                metrics, _ = repair.measure(source, fixed, spans)
                self.assertTrue(all(metrics["spans_touched"]))
                self.assertEqual(metrics["changed_inside_spans"], metrics["diff_added"] + metrics["diff_removed"])
                origin = (item / "origin.md").read_text()
                synthetic += "written for this corpus" in origin.lower()
                if "written for this corpus" not in origin.lower():
                    self.assertTrue(any(pin in origin for pin in ("cJSON", "Zstandard", "zstd", "zlib")))
                # Distinct labels alone do not establish distinct input sets;
                # construction review also checks the generated arguments.
                expected = (item / "expected.txt").read_text().splitlines()
                self.assertGreaterEqual(len(expected), 11)
                self.assertIn("program_main", (item / "reference_main.c").read_text())
        self.assertLessEqual(synthetic, 10)


class CorpusExecutionTests(unittest.TestCase):
    def test_original_fixed_and_partial_repairs(self):
        host = []
        for item in items():
            for version in ("program.c", "fixed.c"):
                source = item / version
                row = repair.run([repair.EVO, "check", "--json", source])
                host.append(dict(item=item.name, version=version,
                                 sha256=hashlib.sha256(source.read_bytes()).hexdigest(), **row))
        with tempfile.TemporaryDirectory(prefix="repair-corpus-check-") as work:
            script = Path(work) / "check.py"
            script.write_text(GUEST)
            result = subprocess.run([repair.CONTAINER, "run", "--rm", "--platform", "linux/arm64",
                                     "--network", "none", "--cpus", "2", "--memory", "1G",
                                     "-v", f"{ITEMS}:/corpus:ro", "-v", f"{work}:/check:ro",
                                     repair.IMAGE, "python3", "/check/check.py"],
                                    capture_output=True, text=True, timeout=600, env=repair.ENV)
        self.assertEqual(result.returncode, 0, result.stderr)
        native = json.loads(result.stdout)
        capture = os.environ.get("EVOCC_REPAIR_CAPTURE")
        if capture:
            Path(capture).write_text(json.dumps(dict(host=host, native=native), indent=2) + "\n")
        self.assertEqual(len(host), 60)
        for row in host:
            with self.subTest(item=row["item"], version=row["version"], tool="evo"):
                record = json.loads(row["stdout"])
                if row["version"] == "fixed.c":
                    self.assertEqual(row["exit_code"], 0, row)
                    self.assertEqual(record["completion"]["status"], "completed")
                else:
                    self.assertEqual(row["exit_code"], 1, row)
                    codes = {d["code"] for d in record["diagnostics"]}
                    self.assertNotIn("E3008", codes, row)
                    self.assertTrue(codes & {"E3002", "E3003", "E3004", "E3005", "E3009", "E3010", "E3011"}, row)
        self.assertGreaterEqual(len(native), 120)
        for row in native:
            with self.subTest(item=row["item"], version=row["version"], tool="sealed"):
                scored = row["score"]["run"]
                expected = (ITEMS / row["item"] / "expected.txt").read_text()
                passed = scored is not None and scored["exit_code"] == 0 and scored["stdout"] == expected
                self.assertEqual(passed, row["version"] == "fixed.c", row["score"])
                for compiler, feedback in row["feedback"].items():
                    self.assertEqual(feedback["compile"]["exit_code"], 0, feedback)
                    self.assertIsNotNone(feedback["run"], feedback)
                    if row["version"] == "fixed.c":
                        main_result = int(expected.splitlines()[-1].split()[-1]) & 255
                        self.assertEqual(feedback["run"]["exit_code"], main_result, feedback)
                        self.assertNotIn("runtime error:", feedback["run"]["stderr"])
                    else:
                        self.assertIn("runtime error:", feedback["run"]["stderr"], (compiler, feedback))


if __name__ == "__main__":
    unittest.main()
