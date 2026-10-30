"""Check the unregistered repair corpus against the actual experiment tools."""

import collections
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
ITEMS = ROOT / "tests/fixtures/agent-repair"
EVO = ROOT / "target/release/evo"
FIELDS = {"program.c", "fixed.c", "reference_main.c", "expected.txt", "origin.md", "class"}
UB_MESSAGES = {
    "addition-overflow": ("E3004", "signed integer overflow"),
    "array-bound": ("E3011", "out of bounds"),
    "multiplication-overflow": ("E3004", "signed integer overflow"),
    "negation-overflow": ("E3004", "negation of"),
    "negative-shift-count": ("E3005", "shift exponent -1 is negative"),
    "quotient-overflow": ("E3004", "division of"),
    "quotient-zero": ("E3002", "division by zero"),
    "remainder-zero": ("E3003", "division by zero"),
    "subtraction-overflow": ("E3004", "signed integer overflow"),
    "wide-shift-count": ("E3005", "shift exponent 32 is too large"),
}

# This runs only trusted, reviewed fixtures. Model candidates use repair.py's
# separate, per-command container. No hidden inputs enter compiler feedback.
GUEST = r'''
import json, pathlib, subprocess, tempfile

def run(argv, cwd):
    p = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, timeout=20)
    return {"exit_code": p.returncode, "stdout": p.stdout, "stderr": p.stderr}

rows = []
for item in sorted(pathlib.Path("/corpus").iterdir()):
    if not item.is_dir():
        continue
    for version in ("program.c", "fixed.c"):
        with tempfile.TemporaryDirectory() as work:
            cwd = pathlib.Path(work)
            source = str(item / version)
            row = {"item": item.name, "version": version, "feedback": {}}
            for compiler, warnings in (("gcc-14", ["-Wall", "-Wextra"]),
                                       ("clang-19", ["-Weverything"])):
                output = cwd / compiler
                built = run([compiler, *warnings, "-fsanitize=undefined",
                             "-o", str(output), source], cwd)
                executed = run([str(output)], cwd) if built["exit_code"] == 0 else None
                row["feedback"][compiler] = {"compile": built, "run": executed}
            built = run(["gcc-14", "-O0", "-Dmain=program_main", "-c", source,
                         "-o", "program.o"], cwd)
            linked = run(["gcc-14", "-O0", "program.o",
                          str(item / "reference_main.c"), "-o", "score"], cwd
                         ) if built["exit_code"] == 0 else None
            scored = run([str(cwd / "score")], cwd
                        ) if linked and linked["exit_code"] == 0 else None
            row["score"] = {"compile": built, "link": linked, "run": scored}
            rows.append(row)
print(json.dumps(rows))
'''


def items():
    return sorted(path for path in ITEMS.iterdir() if path.is_dir())


class CorpusShapeTests(unittest.TestCase):
    def test_population_and_origins(self):
        population = items()
        self.assertEqual(len(population), 30)
        self.assertEqual(collections.Counter((p / "class").read_text().strip()
                                            for p in population),
                         {"frontend": 10, "logic": 10, "ub": 10})
        synthetic = 0
        for item in population:
            with self.subTest(item=item.name):
                self.assertEqual({p.name for p in item.iterdir()}, FIELDS)
                origin = (item / "origin.md").read_text()
                if "written for this corpus" in origin.lower():
                    synthetic += 1
                else:
                    self.assertIn("5c7275656d751de0e68b2d340a95b5681858ed07", origin)
                expected = (item / "expected.txt").read_text().splitlines()
                self.assertTrue(5 <= len(expected) <= 10)
                self.assertTrue(all(line.lstrip("-").isdigit() for line in expected))
                self.assertNotEqual((item / "program.c").read_bytes(),
                                    (item / "fixed.c").read_bytes())
                for name in ("program.c", "fixed.c"):
                    source = (item / name).read_text()
                    self.assertIn("main(void)", source)
                    self.assertTrue(all(marker not in source for marker in ("#", "%:", "??=")))
        self.assertLessEqual(synthetic, 10)


class CorpusExecutionTests(unittest.TestCase):
    def test_hidden_driver_requires_the_program_entry(self):
        from tools.experiment import repair
        item = ITEMS / "logic-square"
        source = (item / "fixed.c").read_text()
        start = source.rfind("int main(void)\n{")
        self.assertGreaterEqual(start, 0)
        passed, records = repair.score(source[:start], item)
        self.assertFalse(passed, records)
        self.assertNotEqual(records[-1]["exit_code"], 0)
        self.assertIn("program_main", records[-1]["stderr"])

    def test_actual_tools_and_hidden_tests(self):
        # The capture is optional, but assertions always run: no platform skip
        # or fallback compiler can turn a missing prerequisite into a pass.
        host = []
        for item in items():
            for name in ("program.c", "fixed.c"):
                p = subprocess.run([str(EVO), "check", "--json", str(item / name)],
                                   capture_output=True, text=True, timeout=30,
                                   env={"PATH": os.defpath})
                host.append({"item": item.name, "version": name,
                             "source_sha256": hashlib.sha256((item / name).read_bytes()).hexdigest(),
                             "exit_code": p.returncode, "stdout": p.stdout, "stderr": p.stderr})
        with tempfile.TemporaryDirectory(prefix="evocc-corpus-check-") as work:
            script = Path(work) / "check.py"
            script.write_text(GUEST)
            p = subprocess.run(["container", "run", "--rm", "--platform", "linux/arm64",
                                "--network", "none", "--cpus", "2", "--memory", "1G",
                                "-v", f"{ITEMS}:/corpus:ro", "-v", f"{work}:/check:ro",
                                "evo-nexlab:sealed", "python3", "/check/check.py"],
                               capture_output=True, text=True, timeout=300)
        self.assertEqual(p.returncode, 0, p.stderr)
        native = json.loads(p.stdout)
        capture = os.environ.get("EVOCC_REPAIR_CAPTURE")
        if capture:
            Path(capture).write_text(json.dumps({"host": host, "native": native}, indent=2) + "\n")
        self.assertEqual(len(native), 60)
        for row in native:
            item = ITEMS / row["item"]
            kind = (item / "class").read_text().strip()
            fixed = row["version"] == "fixed.c"
            with self.subTest(item=item.name, version=row["version"]):
                scored = row["score"]["run"]
                passed = (scored is not None and scored["exit_code"] == 0 and
                          scored["stdout"] == (item / "expected.txt").read_text())
                self.assertEqual(passed, fixed, row["score"])
                for compiler, feedback in row["feedback"].items():
                    if fixed or kind == "logic":
                        self.assertEqual(feedback["compile"]["exit_code"], 0, feedback)
                        self.assertNotIn("runtime error:", feedback["run"]["stderr"])
                    if kind == "logic":
                        self.assertEqual(feedback["compile"]["stderr"], "", feedback)
                    if kind == "ub" and not fixed:
                        self.assertEqual(feedback["compile"]["exit_code"], 0, feedback)
                        self.assertIn("runtime error:", feedback["run"]["stderr"], feedback)
                        if compiler == "gcc-14":
                            self.assertIn(UB_MESSAGES[item.name[3:]][1], feedback["run"]["stderr"])
        for row in host:
            record = json.loads(row["stdout"])
            kind = (ITEMS / row["item"] / "class").read_text().strip()
            with self.subTest(host=row["item"], version=row["version"]):
                if row["version"] == "fixed.c" or kind == "logic":
                    self.assertEqual(row["exit_code"], 0, row)
                elif kind == "ub":
                    self.assertEqual(row["exit_code"], 1, row)
                    self.assertIn(UB_MESSAGES[row["item"][3:]][0],
                                  [d["code"] for d in record["diagnostics"]])


if __name__ == "__main__":
    unittest.main()
