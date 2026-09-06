"""The decisions between the measurement's steps must be the intended ones."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import unittest

_spec = importlib.util.spec_from_file_location(
    "k1_plan", Path(__file__).resolve().parents[1] / "plan.py"
)
plan = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(plan)


ROWS = [
    {"unit": "lib/a.o", "outcome": "assembled"},
    {"unit": "lib/b.o", "outcome": "refused", "diagnosticCodes": ["E3033", "E3030"]},
    {"unit": "lib/c.o", "outcome": "substituted"},
    {"unit": "lib/d.o", "outcome": "refused", "diagnosticCodes": ["E4009"]},
    {"unit": "lib/e.o", "outcome": "symbol-drift"},
    {"unit": "lib/f.o", "outcome": "refused", "diagnosticCodes": ["E0004"]},
    {"unit": "lib/g.o", "outcome": "not-a-target-c-compile"},
    {"unit": "lib/h.o", "outcome": "assembler-refused"},
    {"unit": "lib/i.o", "outcome": "timeout"},
    {"unit": "lib/j.o", "outcome": "refused", "diagnosticCodes": ["E3033"]},
]


class AllowlistTest(unittest.TestCase):
    def test_every_outcome_that_produced_an_object_is_on_the_allowlist(self) -> None:
        self.assertEqual(plan.assembling_units(ROWS), ["lib/a.o", "lib/c.o", "lib/e.o"])

    def test_an_outcome_without_an_object_is_never_on_the_allowlist(self) -> None:
        for outcome in ("refused", "timeout", "assembler-refused",
                        "not-a-target-c-compile", "rebuilt-by-kbuild"):
            self.assertEqual(plan.assembling_units([{"unit": "x.o", "outcome": outcome}]), [])

    def test_an_unknown_outcome_is_not_assumed_to_have_produced_an_object(self) -> None:
        self.assertEqual(plan.assembling_units([{"unit": "x.o", "outcome": "invented"}]), [])

    def test_the_survey_order_is_preserved(self) -> None:
        rows = [{"unit": "z.o", "outcome": "assembled"}, {"unit": "a.o", "outcome": "assembled"}]
        self.assertEqual(plan.assembling_units(rows), ["z.o", "a.o"])


class RefusalTableTest(unittest.TestCase):
    def test_a_code_counts_the_units_it_stopped(self) -> None:
        table = plan.refusal_table(ROWS)
        self.assertEqual(table["E3033"]["units"], 2)
        self.assertEqual(table["E3030"]["units"], 1)
        self.assertEqual(table["E4009"]["units"], 1)

    def test_a_repeated_code_counts_once_for_its_unit(self) -> None:
        rows = [{"unit": "a.o", "outcome": "refused", "diagnosticCodes": ["E3033", "E3033"]}]
        self.assertEqual(plan.refusal_table(rows)["E3033"]["units"], 1)

    def test_first_codes_sum_to_the_refused_units(self) -> None:
        refused = [row for row in ROWS if row.get("diagnosticCodes")]
        self.assertEqual(sum(plan.first_code_table(ROWS).values()), len(refused))

    def test_the_machine_backend_owns_its_own_codes(self) -> None:
        self.assertEqual(plan.code_owner("E4009"), "machine")
        self.assertEqual(plan.code_owner("E4020"), "machine")
        self.assertEqual(plan.code_owner("E0004"), "machine")

    def test_the_front_end_owns_the_rest(self) -> None:
        for code in ("E1001", "E2903", "E3033", "E3101"):
            self.assertEqual(plan.code_owner(code), "front-end")

    def test_the_driver_owns_its_input_errors(self) -> None:
        for code in ("E0001", "E0002", "E0003"):
            self.assertEqual(plan.code_owner(code), "driver")

    def test_owner_totals_split_the_refused_units(self) -> None:
        totals = plan.owner_totals(ROWS)
        self.assertEqual(totals, {"front-end": 2, "machine": 2})


class SubtractTest(unittest.TestCase):
    def test_the_blamed_units_are_removed_and_the_order_kept(self) -> None:
        self.assertEqual(
            plan.subtract(["a.o", "b.o", "c.o", "d.o"], ["c.o", "a.o"]), ["b.o", "d.o"]
        )

    def test_removing_nothing_changes_nothing(self) -> None:
        self.assertEqual(plan.subtract(["a.o", "b.o"], []), ["a.o", "b.o"])

    def test_a_unit_that_was_never_on_the_list_is_ignored(self) -> None:
        self.assertEqual(plan.subtract(["a.o"], ["z.o"]), ["a.o"])


class SummaryTest(unittest.TestCase):
    def test_the_summary_states_all_three_numbers(self) -> None:
        self.assertEqual(
            plan.summary_line(5, 6, 2799),
            "units compiled by EvoCC in a booting kernel: 5 (assembling 6 of 2799)",
        )

    def test_a_kernel_that_did_not_boot_reports_zero(self) -> None:
        self.assertIn(": 0 ", plan.summary_line(0, 40, 2799))


class ReferenceBootTest(unittest.TestCase):
    SHA = "b6f206de"

    def receipt(self, **overrides) -> dict:
        fields = {"verdict": "PASS", "referenceImageSha256": self.SHA, "imageSha256": self.SHA}
        fields.update(overrides)
        return fields

    def test_a_pass_on_the_same_image_may_be_reused(self) -> None:
        self.assertEqual(
            plan.reusable_reference_boot([("a.json", self.receipt())], self.SHA), "a.json"
        )

    def test_the_newest_matching_receipt_wins(self) -> None:
        receipts = [("new.json", self.receipt()), ("old.json", self.receipt())]
        self.assertEqual(plan.reusable_reference_boot(receipts, self.SHA), "new.json")

    def test_a_failure_is_never_reused(self) -> None:
        for verdict in ("TIMEOUT", "MARKERS_MISSING", "NO_POWERDOWN"):
            receipts = [("a.json", self.receipt(verdict=verdict))]
            self.assertIsNone(plan.reusable_reference_boot(receipts, self.SHA))

    def test_a_pass_on_a_different_image_is_not_this_gate(self) -> None:
        receipts = [("a.json", self.receipt(referenceImageSha256="other", imageSha256="other"))]
        self.assertIsNone(plan.reusable_reference_boot(receipts, self.SHA))

    def test_a_receipt_that_booted_something_else_is_refused(self) -> None:
        # The gate boots the pristine copy; a receipt whose booted Image is not
        # the reference Image says nothing about the reference.
        receipts = [("a.json", self.receipt(imageSha256="other"))]
        self.assertIsNone(plan.reusable_reference_boot(receipts, self.SHA))

    def test_a_stale_receipt_is_skipped_for_a_matching_one(self) -> None:
        receipts = [("stale.json", self.receipt(verdict="TIMEOUT")), ("good.json", self.receipt())]
        self.assertEqual(plan.reusable_reference_boot(receipts, self.SHA), "good.json")

    def test_no_receipts_means_the_gate_must_be_flown(self) -> None:
        self.assertIsNone(plan.reusable_reference_boot([], self.SHA))


class ReadUnitsTest(unittest.TestCase):
    def test_comments_blank_lines_and_duplicates_are_dropped(self) -> None:
        import tempfile

        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name) / "allowlist.txt"
        path.write_text("a.o\n\n# note\nb.o  # trailing\na.o\n", encoding="utf-8")
        self.assertEqual(plan.read_units(path), ["a.o", "b.o"])


if __name__ == "__main__":
    unittest.main()
