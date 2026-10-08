"""Check the second study's denominators and success-conditional measures."""
import copy
import unittest

from tools.experiment import report


def receipt():
    rows = []
    for condition, tokens, rounds, diff, overlap in (
        ("c0", 900, 3, 20, False), ("c1", 500, 2, 12, False),
        ("c2", 400, 2, 10, False), ("c3", 300, 2, 4, True),
        ("c4", 100, 1, 4, True),
    ):
        rows.append(dict(item="x", condition=condition, model_sent="model",
                         repetition=1, passed=True, rounds=rounds, status="passed",
                         tokens_in=tokens, tokens_out=20, auto_applied=int(condition == "c4"),
                         diff_added=diff // 2, diff_removed=diff // 2,
                         changed_inside_spans=4, spans_touched=[True, overlap],
                         calls=[dict(tokens_in=tokens, tokens_out=20)],
                         **{"class": "binary_codecs"}))
    return dict(schema="agent-repair.v2", plan=dict(models=["model"],
                conditions=["c0", "c1", "c2", "c3", "c4"], items=1,
                class_counts={"binary_codecs": 1}, rounds=3, reps=1), runs=rows,
                stop_reason="completed", unattempted=0, total_cost=0.1)


class ReportV2Tests(unittest.TestCase):
    def test_measures_and_dynamic_classes(self):
        rows = report.summarize(receipt())
        self.assertEqual(len(rows), 10)
        row = next(r for r in rows if r["condition"] == "c3" and r["group"] == "all")
        self.assertEqual(row["success_tokens"], 320)
        self.assertEqual(row["success_token_range"], [320, 320])
        self.assertEqual(row["success_rounds"], 2)
        self.assertEqual(row["success_diff"], 4)
        self.assertEqual(row["all_spans_share"], 1)
        self.assertEqual(row["inside_span_ratio"], 1)
        self.assertEqual(row["completion_rate"], 1)

    def test_only_response_errors_leave_repair_denominator(self):
        data = receipt()
        first = data["runs"][0]
        for rep, status in ((2, "response_error"), (3, "error"), (4, "failed")):
            other = dict(first, repetition=rep, passed=False, status=status)
            data["runs"].append(other)
        data["plan"]["reps"] = 4
        row = report.summarize(data)[0]
        self.assertEqual(row["response_errors"], 1)
        self.assertEqual(row["repair_rate"], 1 / 3)
        self.assertEqual(row["completion_rate"], 1 / 4)
        self.assertEqual(row["errors"], 1)

    def test_unknown_usage_is_not_zero_or_complete(self):
        data = receipt()
        data["runs"][0]["calls"].append(dict(tokens_in=None, tokens_out=None))
        row = report.summarize(data)[0]
        self.assertIsNone(row["success_tokens"])
        self.assertIsNone(row["success_token_range"])
        self.assertEqual(row["unknown_success_usage"], 1)
        # The unknown C0 usage affects only the comparison that uses C0 tokens.
        results = report.predictions(data)
        self.assertTrue(results[0]["held"])
        self.assertIn("missing data: unknown_success_usage", results[4]["evidence"])
        self.assertIn("success_tokens c1=520 (n=1) vs c0=none (n=0)", results[4]["evidence"])

    def test_zero_call_success_and_zero_diff(self):
        data = receipt()
        data["runs"][0].update(tokens_in=0, tokens_out=0, rounds=0, calls=[],
                               diff_added=0, diff_removed=0, changed_inside_spans=0,
                               spans_touched=[False, False], auto_applied=2)
        row = report.summarize(data)[0]
        self.assertEqual(row["success_tokens"], 0)
        self.assertEqual(row["auto_only"], 1)
        self.assertEqual(row["auto_applied"], 2)
        self.assertIsNone(row["inside_span_ratio"])
        self.assertEqual(row["all_spans_share"], 0)

    def test_predictions_require_strict_complete_comparisons(self):
        data = receipt()
        results = report.predictions(data)
        self.assertEqual([r["held"] for r in results], [True, True, True, True, False])
        # C0 ties C1/C2 on the location hit rate, so 'worst on all' is false.
        data["unattempted"] = 1
        self.assertTrue(all(not r["held"] for r in report.predictions(data)))
        self.assertIn("incomplete", report.predictions(data)[0]["evidence"])

    def test_prediction_ties_and_missing_successes_do_not_hold(self):
        data = receipt()
        data["runs"][3]["tokens_in"] = 500
        data["runs"][3]["calls"][0]["tokens_in"] = 500
        self.assertFalse(report.predictions(data)[0]["held"])
        data["runs"][3].update(passed=False, status="failed")
        self.assertTrue(all(not r["held"] for r in report.predictions(data)))

    def test_render_has_numbers_and_all_five_predictions(self):
        text = report.render(receipt())
        for field in ("Completion rate", "Response errors", "Tokens", "Diff", "Mean rounds",
                      "All-span share", "Inside-span", "Auto repairs", "P5", "Class binary_codecs"):
            self.assertIn(field, text)
        self.assertIn("320", text)
        self.assertIn("did not hold", text)

    def test_hard_population_from_preflight_decides_predictions(self):
        data = receipt()
        for condition in ("c0", "c1", "c2", "c3", "c4"):
            spent = 5000 if condition == "c3" else 5
            data["runs"].append(dict(copy.deepcopy(next(r for r in data["runs"] if r["condition"] == condition)), item="easy",
                                     tokens_in=spent, calls=[dict(tokens_in=spent, tokens_out=20)]))
        data["plan"]["items"] = 2
        data["plan"]["class_counts"] = {"binary_codecs": 2}
        self.assertFalse(report.predictions(data)[0]["held"])
        data["provenance"] = {"preflight_hard": {"model": ["x"]}}
        rows = report.summarize(data)
        hard = next(r for r in rows if r["group"] == "hard" and r["condition"] == "c3")
        self.assertEqual((hard["attempted"], hard["success_tokens"], hard["repetitions"][0]["planned"]), (1, 320, 1))
        results = report.predictions(data)
        self.assertEqual([r["held"] for r in results], [True, True, True, True, False])
        self.assertIn("(hard items)", results[0]["evidence"])
        self.assertIn("tie in all_spans_share", results[4]["reasons"])
        self.assertIn("Items hard for that model", report.render(data))

    def test_key_refusal_ends_the_campaign_at_the_first_refused_slot(self):
        data = receipt()
        refused = dict(response_error="http_403", tokens_in=None, tokens_out=None, cost=None)
        for order, row in enumerate(data["runs"]):
            row.update(start_order=order, cost=0.25)
        data["runs"][1]["calls"] = [dict(tokens_in=1, tokens_out=1, cost=0.25), refused]
        data["runs"][1].update(passed=False, status="response_error")
        for row in data["runs"][2:]:
            row.update(passed=False, status="response_error", cost=0.0, calls=[refused, refused])
        data["total_cost"] = 0.5
        normalized = report.key_limit_stop(data)
        self.assertEqual([r["status"] for r in normalized["runs"]], ["passed", "stopped", "stopped"])
        self.assertEqual((normalized["stop_reason"], normalized["unattempted"], normalized["total_cost"]), ("provider_refused_key:http_403", 2, 0.5))
        self.assertEqual(normalized["key_limit_stop"]["first_refused_slot"], 2)
        self.assertIs(report.key_limit_stop(normalized), normalized)
        self.assertEqual(next(r for r in report.summarize(data) if r["condition"] == "c3")["attempted"], 0)
        self.assertTrue(all("incomplete population" in r["reasons"] for r in report.predictions(data)))
        self.assertIn("Key-limit stop applied by the report: the first refused slot is 2", report.render(data))
        clean = receipt()
        self.assertIs(report.key_limit_stop(clean), clean)

    def test_duplicate_and_inconsistent_outcomes_still_reject(self):
        data = receipt()
        data["runs"].append(copy.deepcopy(data["runs"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate"):
            report.summarize(data)
        data["runs"].pop()
        data["runs"][0]["status"] = "response_error"
        with self.assertRaisesRegex(ValueError, "inconsistent"):
            report.summarize(data)

    def test_failed_final_measurement_does_not_erase_real_pass(self):
        data = receipt()
        del data["runs"][0]["diff_added"]
        data["stop_reason"] = "final_metrics_failed"
        row = report.summarize(data)[0]
        self.assertEqual(row["passed"], 1)
        self.assertIsNone(row["success_diff"])
        self.assertEqual(row["unknown_success_metrics"], 1)
        self.assertIn("Missing successful diff records", report.render(data))
        self.assertTrue(all(not p["held"] for p in report.predictions(data)))


if __name__ == "__main__":
    unittest.main()
