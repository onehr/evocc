"""Registered effort tests use items, not sessions, as the unit of analysis."""
import unittest
from tools.experiment import report


def receipt(repetitions, code="E3001", status="completed"):
    rows = [dict(index=n, item="item", condition=c, repetition=r, status=status,
                 passed=True, steps=s, tokens={"total": s * 100}, seconds=1)
            for n, (r, c, s) in enumerate((r, c, s) for r in repetitions for c, s in (("c1", 8), ("c3", 4)))]
    return dict(plan=dict(classes={"item": code}, order=[{k: r[k] for k in ("item", "condition", "repetition")} for r in rows]),
                runs=rows, stop_reason="completed", unattempted=0)


class EffortTests(unittest.TestCase):
    def test_exact_signed_ranks_with_ties_and_zeros(self):
        self.assertEqual(report.exact_wilcoxon([0, 0])["p"], 1)
        self.assertEqual(report.exact_wilcoxon([1, 1, 0])["p"], 0.25)
        self.assertEqual(report.exact_wilcoxon([1, -1])["p"], 0.75)
        self.assertEqual(report.exact_wilcoxon([1, 2, -3])["p"], 0.625)
        self.assertEqual(report.exact_wilcoxon([-1, -2])["p"], 1)

    def test_repetitions_are_averaged_within_items(self):
        r = report.effort_results(receipt([2, 3]), receipt([1, 2]))
        self.assertEqual(r["P1"]["items"], 1)
        self.assertEqual(r["P1"]["p"], 0.5)
        self.assertEqual(r["P1"]["c1"], 8)
        self.assertEqual(r["P1"]["c3"], 4)
        self.assertEqual(r["P2"]["c1"], 800)
        self.assertFalse(r["P4"]["held"])
        self.assertEqual(r["P4"]["A"], 0.5)

    def test_exploratory_repetition_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "repetition"):
            report.effort_results(receipt([1, 2, 3]), receipt([1, 2]))

    def test_censor_is_a_failure_but_effort_stays_in_test(self):
        r = report.effort_results(receipt([2, 3], status="timeout"), receipt([1]))
        row = next(r for r in r["tables"] if r["part"] == "A" and r["group"] == "all" and r["condition"] == "c1")
        self.assertEqual((row["attempted"], row["passed"], row["steps"], row["censored"]), (2, 0, 16, 2))

    def test_provider_unknown_is_not_a_failed_or_zero_effort_pair(self):
        a = receipt([2, 3]); a["runs"][0]["status"] = "provider_overloaded"
        r = report.effort_results(a, receipt([1]))
        self.assertEqual(r["P1"]["items"], 1)
        self.assertTrue(r["P1"]["incomplete"])
        row = next(r for r in r["tables"] if r["part"] == "A" and r["group"] == "all" and r["condition"] == "c1")
        self.assertEqual((row["unknown"], row["passed"]), (1, 1))

    def test_authorized_retry_is_used_not_best_attempt(self):
        a = receipt([2, 3])
        a["runs"].append(dict(a["runs"][0], index=9, retry_of=0, steps=20, passed=False))
        r = report.effort_results(a, receipt([1]))
        self.assertEqual(r["P1"]["c1"], 14)

    def test_no_pairs_has_no_claim_or_ratio(self):
        a = receipt([2, 3], status="stopped")
        r = report.effort_results(a, receipt([1]))
        self.assertEqual(r["P1"]["items"], 0)
        self.assertIsNone(r["P4"]["A"])
        self.assertFalse(r["P1"]["held"])
