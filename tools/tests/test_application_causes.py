"""An incomplete lane names its recorded cause without changing its verdict."""

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("application_status", ROOT / "tools/render-status.py")
status = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(status)


# The legend of both views, from the review of 2026-10-27.
LEGEND = ("Raw refusals overlap the classes above and do not add to the EvoCC failures. "
          "A reference failure is a failure of the reference build.")
# The receipt fields of two rows of docs/status.md.
TORTURE = {"status": "INCOMPLETE", "total": 1692, "passed": 1538, "kind": "programs",
           "baselineFailures": 0,
           "dispositions": {"PASS": 1538, "compiler-failure": 5, "FP-deferred": 0,
                            "not-applicable": 146, "oracle-unusable": 3},
           "outcomes": {"fail": 6, "refused": 80}, "refusals": {"E1205": 80}}
LLVM = {"status": "INCOMPLETE", "total": 1735, "passed": 1054, "kind": "programs",
        "baselineFailures": 2,
        "dispositions": {"PASS": 1054, "compiler-failure": 11, "FP-deferred": 3,
                         "not-applicable": 665, "oracle-unusable": 2},
        "refusals": {"E2202": 61}}


def rendered(name, row):
    receipts = {"bench-receipt.json": {"benchmarks": {name: row}}}
    return (status.render_ascii(receipts, evoir="2.33.0", evomir="1.13.0"),
            status.render_document(receipts, evoir="2.33.0", evomir="1.13.0"))


def table_row(document, name):
    """The row of one benchmark in the Markdown table."""
    return next(line for line in document.splitlines() if line.startswith(f"| {name} |"))


def cause_lines(ascii_text, name):
    """The lines that the terminal view indents under the row of one benchmark."""
    lines = ascii_text.splitlines()
    first = next(n for n, line in enumerate(lines)
                 if line.startswith("  E") and f" {name} " in line)
    found = []
    for line in lines[first + 1:]:
        if not line.startswith("    "):
            break
        found.append(line)
    return found


class ApplicationCauseTests(unittest.TestCase):
    def test_shared_reference_failures_are_not_evocc_failures(self):
        for name, count, verdict, expected in [
                ("musl", 13, "libc-test-clean", "13 reference failures; 0 EvoCC failures"),
                ("toybox", 1, "no-regression", "1 reference failure; 0 EvoCC failures")]:
            row = {"status": "INCOMPLETE", "total": 20, "passed": 20, "kind": "units",
                   "baselineFailures": count, "testVerdict": verdict,
                   "failingUnits": [], "refusals": {}}
            before = copy.deepcopy(row)
            for text in rendered(name, row):
                self.assertIn(expected, text)
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
        # The table leaves out a count that its detail gives already, for example
        # compiler-failure: 2. The terminal view keeps the full line.
        omitted = {"ctest": ["2 EvoCC failures"],
                   "llvm-test-suite": ["11 EvoCC failures", "3 FP deferrals",
                                       "665 not applicable", "2 unusable oracles"],
                   "csmith": ["68 unusable oracles"],
                   "torture": ["5 EvoCC failures", "146 not applicable", "3 unusable oracles"]}
        for name, row, fragments in cases:
            before = copy.deepcopy(row)
            ascii_text, document = rendered(name, row)
            for fragment in fragments:
                self.assertIn(fragment, ascii_text, name)
                if fragment not in omitted[name]:
                    self.assertIn(fragment, document, name)
            self.assertEqual(row, before)

    def test_a_pass_row_keeps_its_original_presentation(self):
        row = {"status": "PASS", "total": 2, "passed": 2, "kind": "units",
               "baselineFailures": 0, "dispositions": {"PASS": 2}, "refusals": {}}
        ascii_text, document = rendered("zlib", row)
        self.assertIn("  E1   zlib             PASS            2/2 TU", ascii_text)
        self.assertIn("| zlib | PASS: 2 | `unrecorded` |", document)
        self.assertNotIn("reference failures", ascii_text)
        self.assertNotIn("EvoCC failures", document)

    def test_a_count_of_one_takes_the_singular(self):
        ones = {"status": "INCOMPLETE", "total": 20, "passed": 16, "kind": "programs",
                "baselineFailures": 1,
                "dispositions": {"PASS": 16, "compiler-failure": 1, "FP-deferred": 1,
                                 "not-applicable": 1, "oracle-unusable": 1},
                "refusals": {"E2202": 1}}
        ascii_text, document = rendered("torture", ones)
        for singular in ["1 reference failure", "1 EvoCC failure", "1 raw refusal",
                         "1 FP deferral", "1 unusable oracle"]:
            self.assertIn(singular, ascii_text)
            self.assertNotIn(singular + "s", ascii_text)
        # The table gives the other counts through its detail.
        for singular in ["1 reference failure", "1 raw refusal"]:
            self.assertIn(singular, table_row(document, "torture"))
            self.assertNotIn(singular + "s", document)
        # A count that the table has to add takes the singular too.
        derived = {"status": "INCOMPLETE", "total": 10, "passed": 8, "kind": "programs",
                   "baselineFailures": 0,
                   "outcomes": {"agree": 8, "mismatch": 1, "oracle-split": 1}}
        for text in rendered("csmith", derived):
            self.assertIn("0 reference failures; 1 EvoCC failure", text)
            self.assertIn("1 unusable oracle", text)
            self.assertNotIn("1 EvoCC failures", text)
            self.assertNotIn("1 unusable oracles", text)

    def test_the_table_gives_each_count_once(self):
        ascii_text, document = rendered("torture", TORTURE)
        self.assertEqual(
            table_row(document, "torture"),
            "| torture | PASS: 1538; compiler-failure: 5; FP-deferred: 0; not-applicable: 146; "
            "oracle-unusable: 3; 0 reference failures; 80 raw refusals | `unrecorded` |")
        self.assertEqual(cause_lines(ascii_text, "torture"), [
            "    0 reference failures; 5 EvoCC failures; 80 raw refusals; "
            "146 not applicable; 3 unusable oracles"])
        # Outcome names give a count the same way, and so do refusals and runs.
        counted = {"status": "INCOMPLETE", "total": 10, "passed": 6, "kind": "programs",
                   "baselineFailures": 0, "outcomes": {"pass": 6, "refused": 2, "not-run": 2},
                   "refusals": {"E2202": 2}}
        ascii_text, document = rendered("ctest", counted)
        self.assertEqual(
            table_row(document, "ctest"),
            "| ctest | pass: 6; refused: 2; not-run: 2; No measurement receipt.; "
            "0 reference failures | `unrecorded` |")
        self.assertEqual(cause_lines(ascii_text, "ctest"),
                         ["    0 reference failures; 2 raw refusals; 2 not run"])

    def test_a_cause_line_breaks_only_between_two_items(self):
        ascii_text, _ = rendered("llvm-test-suite", LLVM)
        self.assertEqual(cause_lines(ascii_text, "llvm-test-suite"), [
            "    2 reference failures; 11 EvoCC failures; 61 raw refusals; "
            "3 FP deferrals; 665 not applicable;",
            "    2 unusable oracles"])

    def test_both_views_explain_raw_refusals_and_reference_failures(self):
        ascii_text, document = rendered("torture", TORTURE)
        # The terminal view may put a sentence on a line of its own.
        applications = ascii_text.partition("KERNEL")[0]
        self.assertEqual(" ".join(applications.split()).count(LEGEND), 1)
        table = document.partition("## Application evidence and limits")[2]
        self.assertEqual(table.count(LEGEND), 1)
        self.assertGreater(table.index(LEGEND), table.index("| torture |"))
        self.assertIn("\n" + LEGEND + "\n", table)
        # A receipt with no incomplete row has no counts to explain.
        row = {"status": "PASS", "total": 2, "passed": 2, "kind": "units",
               "baselineFailures": 0, "dispositions": {"PASS": 2}, "refusals": {}}
        for text in rendered("zlib", row):
            self.assertNotIn("Raw refusals overlap", text)


if __name__ == "__main__":
    unittest.main()
