"""Model-free tests of the experiment 8 harness. No request leaves the host."""
from collections import Counter
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("silent_chat", HERE / "silent_chat.py")
chat = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(chat)
PROTOCOL = chat.load()
DEEPSEEK = chat.model_entry(PROTOCOL, "deepseek/deepseek-v4.1-flash")
KEY = "unit-secret-key"
SOURCE = "int main(void) { return 0; }\n"


def answer(content="```c\n" + SOURCE + "```", finish="stop", provider="AtlasCloud",
           model="deepseek/deepseek-v4.1-flash", completion=900, reasoning=300):
    return dict(id="r1", provider=provider, model=model,
                choices=[dict(finish_reason=finish, native_finish_reason=finish, message=dict(content=content))],
                usage=dict(cost=0.0004, prompt_tokens=1500, completion_tokens=completion,
                           completion_tokens_details=dict(reasoning_tokens=reasoning)))


def http(status, body):
    return dict(status=status, body=json.dumps(body).encode())


def row(item="adler-reset", condition="c3", **fields):
    spec = dict(trial=1, item=item, condition=condition, repetition=None)
    value = chat.blank(spec, dict(code="E3001", c1="c1 record", c3="c3 record"))
    value.update(status="completed", provider="AtlasCloud", model="deepseek/deepseek-v4.1-flash",
                 attempts=[dict(http_status=200, cost=0.0004, seconds=30.0)], prompt_tokens=1500,
                 completion_tokens=900, reasoning_tokens=300, finish_reason="stop", delivered=True, valid=True,
                 correct=True)
    value.update(fields)
    return value


class OrderTests(unittest.TestCase):
    def test_measured_order_balances_conditions_and_alternates_the_first_one(self):
        trials = chat.order(PROTOCOL, "measured")
        self.assertEqual(len(trials), PROTOCOL["phases"]["measured"]["trials_per_model"])
        self.assertEqual([t["trial"] for t in trials], list(range(1, len(trials) + 1)))
        self.assertEqual(set(Counter((t["item"], t["condition"]) for t in trials).values()), {3})
        items = sorted(PROTOCOL["items"])
        for repetition in (1, 2, 3):
            pairs = [t for t in trials if t["repetition"] == repetition]
            firsts = [pairs[2 * i]["condition"] for i in range(len(items))]
            expected = ["c1" if (i + repetition) % 2 == 0 else "c3" for i in range(len(items))]
            self.assertEqual(firsts, expected)
            self.assertEqual([pairs[2 * i]["item"] for i in range(len(items))], items)

    def test_pilot_order_is_the_protocol_list(self):
        trials = chat.order(PROTOCOL, "pilot")
        listed = PROTOCOL["phases"]["pilot"]["trials"]
        self.assertEqual([(t["item"], t["condition"]) for t in trials], [(t["item"], t["condition"]) for t in listed])
        self.assertTrue(all(t["repetition"] is None for t in trials))


class PayloadTests(unittest.TestCase):
    def test_payload_pins_the_provider_and_limits_reasoning(self):
        request = chat.payload(PROTOCOL, DEEPSEEK, "the prompt", SOURCE, "the feedback")
        self.assertEqual(request["provider"], {"order": ["atlas-cloud"], "allow_fallbacks": False})
        self.assertEqual(request["reasoning"], {"effort": "low"})
        self.assertEqual((request["temperature"], request["max_tokens"]), (0, 16384))
        self.assertEqual(request["messages"][0], {"role": "system", "content": "the prompt"})
        self.assertTrue(request["messages"][1]["content"].endswith("\nFeedback:\nthe feedback"))
        chat.repair.encode(request, KEY)
        with self.assertRaises(chat.repair.Stop):
            chat.repair.encode(dict(request, note=KEY), KEY)

    def test_reserve_uses_the_input_allowance_and_the_output_limit(self):
        expected = (2000 * 0.135 + 16384 * 0.54) / 1e6
        self.assertAlmostEqual(chat.reserve(PROTOCOL, DEEPSEEK), expected)


class FrozenTests(unittest.TestCase):
    def test_frozen_records_follow_the_qualification(self):
        items = chat.frozen(PROTOCOL)
        self.assertEqual(sorted(items), sorted(PROTOCOL["items"]))
        groups = Counter(chat.group(PROTOCOL, item["code"]) for item in items.values())
        self.assertEqual(groups, Counter(silent=18, control=6))
        for item in items.values():
            self.assertIn("=== evo diagnostics", item["c3"])
            self.assertIn(item["code"], item["c3"])
            if chat.group(PROTOCOL, item["code"]) == "silent":
                self.assertNotIn("runtime error", item["c1"])

    def test_a_changed_qualification_is_refused(self):
        changed = copy.deepcopy(PROTOCOL)
        changed["qualification"]["sha256"] = "0" * 64
        with self.assertRaises(chat.budget.Failure):
            chat.frozen(changed)


class ExchangeTests(unittest.TestCase):
    def run_with(self, results):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        run = chat.budget.Run(PROTOCOL, Path(tmp.name), 1.0, KEY)
        target = row(attempts=[], status="stopped", provider=None, model=None)
        with patch.object(chat.budget, "send", side_effect=results), patch.object(chat.time, "sleep"):
            try:
                response = chat.exchange(run, PROTOCOL, DEEPSEEK, target, b"{}")
            except chat.STOPS as error:
                return run, target, error
        return run, target, response

    def test_one_retry_after_a_rate_limit_then_the_charge(self):
        run, target, response = self.run_with([http(429, dict(error=dict(code=429))), http(200, answer())])
        self.assertEqual(response["id"], "r1")
        self.assertEqual(len(target["attempts"]), 2)
        self.assertAlmostEqual(run.cost, 0.0004)
        self.assertEqual(run.unknown, 1)
        self.assertEqual((target["completion_tokens"], target["reasoning_tokens"]), (900, 300))

    def test_two_transport_errors_end_the_trial_and_stop_the_run(self):
        transport = dict(transport=dict(kind="TimeoutError", cause=None, errno=None))
        run, target, response = self.run_with([transport, transport])
        self.assertIsNone(response)
        self.assertEqual(target["status"], "request_error")
        self.assertTrue(run.stop.startswith("retry_failed:"))
        self.assertAlmostEqual(run.reserve, 2 * chat.reserve(PROTOCOL, DEEPSEEK))

    def test_a_refused_key_stops_without_a_retry(self):
        run, target, error = self.run_with([http(402, dict(error=dict(code=402)))])
        self.assertIsInstance(error, chat.STOPS)
        self.assertEqual(len(target["attempts"]), 1)


class OutcomeTests(unittest.TestCase):
    def outcome(self, response, builds=None, phase="measured"):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        run = chat.budget.Run(PROTOCOL, Path(tmp.name), 1.0, KEY)
        target = row(correct=False, delivered=False, valid=False, status="stopped",
                     provider=response["provider"], model=response["model"])
        passed = {"gcc-14-O0": dict(passed=True), "gcc-14-O2": dict(passed=True), "clang-19-O2": dict(passed=True)}
        with patch.object(chat.qualify_silent, "score_builds", return_value=builds or passed) as score:
            chat.outcome(run, DEEPSEEK, target, response, {}, Path("item"), phase)
        return run, target, score

    def test_a_valid_candidate_is_correct_only_on_all_three_builds(self):
        run, target, score = self.outcome(answer())
        self.assertEqual((target["status"], target["valid"], target["correct"]), ("completed", True, True))
        score.assert_called_once_with(SOURCE, Path("item"))
        failing = {"gcc-14-O0": dict(passed=True), "gcc-14-O2": dict(passed=False), "clang-19-O2": dict(passed=True)}
        _, target, _ = self.outcome(answer(), failing)
        self.assertIs(target["correct"], False)

    def test_a_length_stop_is_not_scored(self):
        run, target, score = self.outcome(answer(finish="length"))
        self.assertEqual((target["delivered"], target["undelivered"]), (False, "finish_length"))
        score.assert_not_called()

    def test_another_provider_stops_the_run(self):
        run, target, _ = self.outcome(answer(provider="Together"))
        self.assertEqual((target["deviation"], run.stop), ("provider", "deviation_provider"))

    def test_the_pilot_records_another_model_name_and_goes_on(self):
        run, target, _ = self.outcome(answer(model="deepseek/deepseek-v4.1-flash-1017"), phase="pilot")
        self.assertEqual((target["deviation"], run.stop), ("model", None))
        run, target, _ = self.outcome(answer(model="deepseek/deepseek-v4.1-flash-1017"))
        self.assertEqual(run.stop, "deviation_model")


class GoTests(unittest.TestCase):
    def receipt(self, rows, cost=0.002):
        return dict(runs=rows, totals=dict(cost_usd=cost, unknown_cost_reserve_usd=0.0), stop_reason="completed")

    def test_a_clean_pilot_passes_every_criterion(self):
        rows = [row(trial=n) for n in range(1, 6)]
        result = chat.go(PROTOCOL, DEEPSEEK, self.receipt(rows))
        self.assertTrue(result["held"], result)
        self.assertEqual(len(result["criteria"]), len(PROTOCOL["phases"]["pilot"]["go_criteria"]))

    def test_a_length_stop_at_the_limit_does_not_block_the_go(self):
        stopped = row(finish_reason="length", completion_tokens=16384, delivered=False, valid=False, correct=False)
        result = chat.go(PROTOCOL, DEEPSEEK, self.receipt([stopped] + [row()] * 4))
        self.assertTrue(result["held"], result)

    def test_each_failure_blocks_the_go(self):
        cases = [
            dict(rows=[row(finish_reason="length")] + [row()] * 4),
            dict(rows=[row(provider="Together")] + [row()] * 4),
            dict(rows=[row(attempts=[dict(http_status=200, cost=0.0004, seconds=200.0)])] * 5),
            dict(rows=[row(correct=None, score_error="qualification_guest_failed")] + [row()] * 4),
            dict(rows=[row()] * 4),
            dict(rows=[row()] * 5, cost=0.08),
        ]
        for case in cases:
            result = chat.go(PROTOCOL, DEEPSEEK, self.receipt(case["rows"], case.get("cost", 0.002)))
            self.assertFalse(result["held"], case)


class AnalysisTests(unittest.TestCase):
    def receipt(self, stop="completed"):
        items = chat.frozen(PROTOCOL)
        rows = []
        for trial in chat.order(PROTOCOL, "measured"):
            code = items[trial["item"]]["code"]
            silent = chat.group(PROTOCOL, code) == "silent"
            if silent:
                correct = trial["condition"] == "c3" or trial["item"].startswith("xxh")
            else:
                correct = trial["condition"] == "c1" or trial["repetition"] == 1
            tokens = (8000 if trial["condition"] == "c1" else 1000) if silent else 900
            rows.append(dict(trial, code=code, correct=correct, status="completed", delivered=True, score_error=None,
                             completion_tokens=tokens))
        return dict(runs=rows, plan=dict(model=DEEPSEEK), stop_reason=stop, totals=dict(cost_usd=0.5))

    def test_p1_holds_and_p2_fails_on_a_controls_loss(self):
        result = chat.analyze(PROTOCOL, self.receipt())
        self.assertTrue(result["complete"])
        self.assertEqual(result["table"]["silent"], dict(c1=18, c3=54, trials=54))
        self.assertTrue(result["P1"]["held"])
        self.assertLess(result["P1"]["p"], 0.05)
        self.assertEqual(result["P2"]["loss"], 12)
        self.assertFalse(result["P2"]["held"])
        self.assertTrue(result["P3"]["held"])
        self.assertEqual(result["table"]["silent_tokens"], dict(c1=8000, c3=1000))

    def test_a_run_that_stopped_early_tests_nothing(self):
        result = chat.analyze(PROTOCOL, self.receipt(stop="reported_cost_cap"))
        self.assertFalse(result["P1"]["held"] or result["P2"]["held"] or result["P3"]["held"])


if __name__ == "__main__":
    unittest.main()
