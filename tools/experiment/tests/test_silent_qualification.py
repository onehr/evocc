"""Check the silent-corpus boundaries before any model session."""
import unittest
import collections
import hashlib
import json
from tools.experiment import qualify_silent as q


class QualificationTests(unittest.TestCase):
    def test_only_findings_on_changed_lines_count(self):
        self.assertTrue(q.finding("input.c:8:3: runtime error: overflow", [[8, 8]]))
        self.assertFalse(q.finding("input.c:7:3: warning: bad", [[8, 8]]))
        self.assertFalse(q.finding("input.c:8:3: note: see here", [[8, 8]]))

    def test_exact_single_line_span(self):
        self.assertEqual(q.changed_lines("a\nb\nc\n", "a\nd\nc\n"), [[2, 2]])
        self.assertNotEqual(q.changed_lines("a\nb\nc\n", "a\nd\nc\n"), [[1, 3]])

    def test_hidden_success_requires_all_builds_and_runs(self):
        build = dict(compile=dict(exit_code=0), link=dict(exit_code=0),
                     runs=[dict(exit_code=0, stdout="ok\n", stderr="") for _ in range(5)])
        self.assertTrue(q.passes(build, "ok\n", 5))
        build["runs"][4]["exit_code"] = 1
        self.assertFalse(q.passes(build, "ok\n", 5))

    def test_undefined_control_must_not_count_as_silent(self):
        self.assertEqual(q.main_result("=== run exit 16\n--- stdout\n--- stderr\n"), (16, ""))
        self.assertIsNone(q.main_result("=== build exit 1\n"))

    def test_corpus_profile_and_exact_spans(self):
        items = sorted(p for p in (q.repair.ROOT / "tests/fixtures/agent-repair-4").iterdir() if p.is_dir())
        self.assertEqual(collections.Counter((p / "class").read_text().strip() for p in items),
                         dict(E3001=6, E3010=6, E3011=6, E3002=2, E3004=2, E3005=2))
        synthetic = 0
        for item in items:
            with self.subTest(item=item.name):
                source, fixed = [(item / name).read_text() for name in ("program.c", "fixed.c")]
                self.assertTrue(80 <= len(source.splitlines()) <= 200)
                self.assertLessEqual(len(source.encode()), 65536)
                self.assertFalse(any(t in source for t in ("#", "%:", "??=")))
                self.assertEqual(json.loads((item / "fix-spans.json").read_text())["spans"], q.changed_lines(source, fixed))
                self.assertEqual(len(q.changed_lines(source, fixed)), 1)
                synthetic += "written for this corpus" in (item / "origin.md").read_text().lower()
        self.assertLessEqual(synthetic, 6)

    def test_recorded_qualification_matches_inputs(self):
        root = q.repair.ROOT / "tests/fixtures/agent-repair-4"
        receipt = json.loads((root / "qualification.json").read_text())
        self.assertEqual(len(receipt["items"]), 24)
        for row in receipt["items"]:
            with self.subTest(item=row["item"]):
                self.assertTrue(row["qualified"], row["checks"])
                for name, expected in row["hashes"].items():
                    self.assertEqual(hashlib.sha256((root / row["item"] / name).read_bytes()).hexdigest(), expected)
