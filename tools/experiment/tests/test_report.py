import copy
import importlib.util
from pathlib import Path
import unittest

SPEC = importlib.util.spec_from_file_location("repair_report", Path(__file__).resolve().parents[1] / "report.py")
REPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(REPORT)


class ReportTests(unittest.TestCase):
    def receipt(self):
        rows = []
        for rep, passed, rounds, status in ((1, True, 1, "passed"), (2, False, 3, "failed"), (3, False, 0, "error")):
            rows.append(dict(item="x", condition="c1", model_sent="model", repetition=rep,
                             passed=passed, rounds=rounds, status=status, **{"class": "ub"}))
        return dict(schema="agent-repair.v1", plan=dict(models=["model"], conditions=["c1", "c2"], items=1, class_counts={"ub": 1, "frontend": 0, "logic": 0},
                    rounds=3, reps=3), runs=rows, stop_reason="reported_cost_cap", unattempted=3, total_cost=5.001)

    def test_errors_remain_in_denominator_and_unattempted_do_not(self):
        rows = REPORT.summarize(self.receipt())
        self.assertEqual(rows[0]["repair_rate"], 1 / 3)
        self.assertEqual(rows[0]["mean_rounds"], 4 / 3)
        self.assertEqual(rows[0]["mean_success_rounds"], 1)
        self.assertEqual(rows[0]["range"]["repair_rate"], [0, 1])
        self.assertIsNone(rows[4]["repair_rate"])
        self.assertIsNone(rows[4]["mean_success_rounds"])
        self.assertIn("Unattempted repetitions: 3", REPORT.render(self.receipt()))
        self.assertIn("Rounds range", REPORT.render(self.receipt()))
        self.assertIn("0.000 to 3.000", REPORT.render(self.receipt()))
        self.assertFalse(rows[4]["repetitions"][0]["complete"])

    def test_duplicate_repetition_is_not_extra_evidence(self):
        receipt = self.receipt()
        receipt["runs"].append(copy.deepcopy(receipt["runs"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            REPORT.summarize(receipt)

    def test_status_and_pass_must_agree(self):
        receipt = self.receipt()
        receipt["runs"][0]["status"] = "failed"
        with self.assertRaisesRegex(ValueError, "inconsistent"):
            REPORT.summarize(receipt)


if __name__ == "__main__":
    unittest.main()
