"""An incomplete lane names its recorded cause without changing its verdict."""

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("application_status", ROOT / "tools/render-status.py")
status = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(status)


def rendered(name, row):
    receipts = {"bench-receipt.json": {"benchmarks": {name: row}}}
    return (status.render_ascii(receipts, evoir="2.33.0", evomir="1.13.0"),
            status.render_document(receipts, evoir="2.33.0", evomir="1.13.0"))


class ApplicationCauseTests(unittest.TestCase):
    def test_shared_reference_failures_are_not_evocc_failures(self):
        for name, count, verdict in [("musl", 13, "libc-test-clean"),
                                     ("toybox", 1, "no-regression")]:
            row = {"status": "INCOMPLETE", "total": 20, "passed": 20, "kind": "units",
                   "baselineFailures": count, "testVerdict": verdict,
                   "failingUnits": [], "refusals": {}}
            before = copy.deepcopy(row)
            for text in rendered(name, row):
                self.assertIn(f"{count} reference failures; 0 EvoCC failures", text)
                self.assertIn("INCOMPLETE", text)
            self.assertEqual(row, before)

    def test_compiler_failures_and_coverage_causes_keep_their_own_counts(self):
        cases = [
            ("ctest", {"status": "INCOMPLETE", "total": 220, "passed": 218,
                       "baselineFailures": 0,
                       "dispositions": {"PASS": 218, "compiler-failure": 2},
                       "refusals": {"E2917": 1, "E2952": 1}},
             ["2 EvoCC failures", "2 raw refusals"]),
            ("llvm-test-suite", {"status": "INCOMPLETE", "total": 1735, "passed": 1054,
                                 "baselineFailures": 2,
                                 "dispositions": {"PASS": 1054, "compiler-failure": 11,
                                                  "FP-deferred": 3, "not-applicable": 665,
                                                  "oracle-unusable": 2}},
             ["2 reference failures", "11 EvoCC failures", "3 FP deferrals",
              "665 not applicable", "2 unusable oracles"]),
            ("csmith", {"status": "INCOMPLETE", "total": 1000, "passed": 932,
                        "baselineFailures": 0,
                        "outcomes": {"agree": 932, "oracle-unusable": 68}},
             ["0 EvoCC failures", "68 unusable oracles"]),
            ("torture", {"status": "INCOMPLETE", "total": 1692, "passed": 1538,
                         "baselineFailures": 0,
                         "dispositions": {"PASS": 1538, "compiler-failure": 5,
                                          "not-applicable": 146, "oracle-unusable": 3},
                         "outcomes": {"fail": 6, "refused": 80}, "refusals": {"E1205": 80}},
             ["5 EvoCC failures", "80 raw refusals", "146 not applicable", "3 unusable oracles"]),
        ]
        for name, row, fragments in cases:
            before = copy.deepcopy(row)
            for text in rendered(name, row):
                for fragment in fragments:
                    self.assertIn(fragment, text, name)
            self.assertEqual(row, before)

    def test_a_pass_row_keeps_its_original_presentation(self):
        row = {"status": "PASS", "total": 2, "passed": 2, "kind": "units",
               "baselineFailures": 0, "dispositions": {"PASS": 2}, "refusals": {}}
        ascii_text, document = rendered("zlib", row)
        self.assertIn("  E1   zlib             PASS            2/2 TU", ascii_text)
        self.assertIn("| zlib | PASS: 2 | `unrecorded` |", document)
        self.assertNotIn("reference failures", ascii_text)
        self.assertNotIn("EvoCC failures", document)


if __name__ == "__main__":
    unittest.main()
