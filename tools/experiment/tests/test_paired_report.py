import unittest
from tools.experiment import report


class PairedReportTests(unittest.TestCase):
    def test_exact_mcnemar_no_discordance_and_small_tail(self):
        self.assertEqual(report.exact_mcnemar(0, 0), 1.0)
        self.assertEqual(report.exact_mcnemar(0, 5), 0.0625)
        self.assertEqual(report.exact_mcnemar(0, 6), 0.03125)
        self.assertEqual(report.exact_mcnemar(6, 0), 0.03125)
        self.assertEqual(report.exact_mcnemar(3, 3), 1.0)

    def test_provider_errors_are_unknown_not_failed_pairs(self):
        rows = [dict(index=1, item="a", condition="c1", status="provider_overloaded", passed=False),
                dict(index=2, item="a", condition="c3", status="completed", passed=True)]
        receipt = dict(plan=dict(classes={"a": "E3001"}), runs=rows)
        r = report.paired_results(receipt)
        self.assertEqual(r["silent"]["paired"], 0)
        self.assertFalse(r["P1"])
        rows.append(dict(index=3, item="a", condition="c1", status="completed", passed=True, retry_of=1))
        r = report.paired_results(receipt)
        self.assertEqual(r["silent"]["paired"], 1)
        self.assertEqual(r["silent"]["p"], 1.0)
        self.assertEqual(r["silent"]["c1"], 1)
        self.assertEqual(r["silent"]["c3"], 1)

    def test_unknown_silent_items_cannot_establish_prediction(self):
        classes = {str(i): "E3001" for i in range(7)}
        rows = [dict(index=2*i+j, item=str(i), condition=c, status="completed", passed=c == "c3")
                for i in range(6) for j,c in enumerate(("c1", "c3"))]
        result = report.paired_results(dict(plan=dict(classes=classes), runs=rows))
        self.assertEqual(result["silent"]["p"], 0.03125)
        self.assertFalse(result["P1"])
