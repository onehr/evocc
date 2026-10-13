"""Check the larger corpus without changing the original qualification profile."""
import collections
import hashlib
import json
import unittest
from tools.experiment import qualify_silent as q


class ScaleCorpusTests(unittest.TestCase):
    def test_line_profile_spans_and_sources(self):
        root = q.repair.ROOT / "tests/fixtures/agent-repair-5"
        items = sorted(p for p in root.iterdir() if p.is_dir())
        self.assertEqual(collections.Counter((p / "class").read_text().strip() for p in items),
                         dict(E3001=4, E3010=4, E3011=4))
        for item in items:
            with self.subTest(item=item.name):
                source = (item / "program.c").read_text()
                self.assertTrue(600 <= len(source.splitlines()) <= 1200)
                self.assertLessEqual(len(source.encode()), 65536)
                self.assertFalse(any(t in source for t in ("#", "%:", "??=")))
                spans = q.changed_lines(source, (item / "fixed.c").read_text())
                self.assertEqual(json.loads((item / "fix-spans.json").read_text())["spans"], spans)
                self.assertEqual(len(spans), 1)
                self.assertGreater(source[:source.index("int main(void)")].count("\n") - spans[0][1], 300)
                for project in ("cJSON 1.7.19", "zlib 1.3.2", "Zstandard 1.5.7"):
                    self.assertIn(project, (item / "origin.md").read_text())
        self.assertEqual(q.qualify.__defaults__, ((80, 200),))

    def test_all_recorded_qualifications_match_the_seven_files(self):
        root = q.repair.ROOT / "tests/fixtures/agent-repair-5"
        receipt = json.loads((root / "qualification.json").read_text())
        self.assertEqual(receipt["line_limits"], [600, 1200])
        self.assertEqual(len(receipt["items"]), 12)
        for row in receipt["items"]:
            with self.subTest(item=row["item"]):
                self.assertTrue(row["qualified"], row["checks"])
                self.assertEqual(len(row["hashes"]), 7)
                for name, expected in row["hashes"].items():
                    self.assertEqual(hashlib.sha256((root / row["item"] / name).read_bytes()).hexdigest(), expected)

    def test_replication_feedback_is_byte_identical_without_normalization(self):
        root = q.repair.ROOT
        evidence = json.loads((root / "tests/contracts/agent-repair-5-feedback-identity.json").read_text())
        old = root / evidence["source"]
        self.assertEqual(hashlib.sha256(old.read_bytes()).hexdigest(), evidence["source_sha256"])
        prior = {r["item"]: r for r in json.loads(old.read_text())["items"]}
        self.assertEqual(len(evidence["records"]), 96)
        self.assertEqual(len({(r["item"], r["source"], r["condition"]) for r in evidence["records"]}), 96)
        for row in evidence["records"]:
            expected = prior[row["item"]]["feedback"][row["source"]][row["condition"]].encode()
            self.assertEqual(row["expected_sha256"], hashlib.sha256(expected).hexdigest())
            self.assertEqual(row["actual_sha256"], row["expected_sha256"])
            self.assertTrue(row["identical"])
