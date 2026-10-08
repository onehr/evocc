"""Model-free tests for the second experiment; never execute a candidate."""
import copy
import gzip
import importlib.util
import io
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch
import urllib.error

SPEC = importlib.util.spec_from_file_location("repair_v2", Path(__file__).parents[1] / "repair.py")
repair = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repair)
SOURCE = "int main(void){return 1;}\n"
FIXED = "int main(void){return 0;}\n"
MARKER = "\n--- C0 ONLY ---\n"


class V2Tests(unittest.TestCase):
    def response(self, answer=FIXED, cost=0.25, finish="stop"):
        return dict(model="actual-model", provider="actual-provider",
                    usage=dict(cost=cost, prompt_tokens=3, completion_tokens=4),
                    choices=[dict(finish_reason=finish, message=dict(content="```c\n" + answer + "```"))])

    def fixture(self, root, count=1, models=None):
        items = root / "items"
        for index in range(count):
            item = items / str(index)
            item.mkdir(parents=True)
            for name, value in {"program.c": SOURCE, "fixed.c": FIXED, "class": "logic\n",
                                "origin.md": "Written for this corpus.\n",
                                "reference_main.c": "HIDDEN_REFERENCE", "expected.txt": "HIDDEN_EXPECTED",
                                "fix-spans.json": '{"spans": [[1, 1]]}'}.items():
                (item / name).write_text(value)
        prompt = root / "prompt.txt"
        prompt.write_text("COMMON INSTRUCTIONS" + MARKER + "This program has defects. Return the complete corrected file.")
        protocol = dict(schema="agent-repair.protocol.v2", corpus_tag="agent-repair-experiment-2",
                        items=str(items), prompt_file=str(prompt), models=models or ["fixture-model"],
                        repetitions=3, rounds=3, temperature=0, max_tokens=16384,
                        reported_cost_cap_usd=5, request_limit=count * 5 * len(models or [1]) * 9,
                        population={"logic": count}, conditions={key: key for key in ("c0", "c1", "c2", "c3", "c4")},
                        preflight_receipt=str(root / "eligibility.json"))
        path = root / "protocol.json"
        path.write_text(json.dumps(protocol))
        return path, items

    def campaign(self, responses, *, condition="c3", passed=False, cap=5, prepare=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            protocol, items = self.fixture(root)
            payloads = []
            queue = iter(responses)
            def request(payload, key, events, **kwargs):
                self.assertEqual(kwargs, {"attempts": 1})
                payloads.append(copy.deepcopy(payload))
                response = next(queue)
                if isinstance(response, Exception):
                    raise response
                events.append(dict(http_status=200, response=json.dumps(response)))
                return response
            metric = dict(diff_added=1, diff_removed=1, changed_inside_spans=2, spans_touched=[True])
            with patch.object(repair, "preflight", return_value={"commit": "fixture"}), \
                 patch.object(repair, "load_key", return_value="unit-secret-key"), \
                 patch.object(repair, "feedback", return_value=("PUBLIC", [])) as feedback, \
                 patch.object(repair, "request", side_effect=request), \
                 patch.object(repair, "score", return_value=(passed, [{"stdout": "HIDDEN_EXPECTED"}])) as score, \
                 patch.object(repair, "measure", return_value=(metric, "DIFF"), create=True), \
                 patch("sys.stdout", new_callable=io.StringIO):
                extra = patch.object(repair, "prepare", side_effect=prepare) if prepare else patch.object(repair, "ENV", repair.ENV)
                with extra:
                    result = repair.main(["--protocol", str(protocol), "--out", str(root / "out"),
                                          "--condition", condition, "--reps", "1", "--rounds", "2",
                                          "--max-cost-usd", str(cap)])
            receipt = json.loads((root / "out/receipt.json").read_text())
            transcript = json.loads(gzip.decompress((root / "out" / receipt["runs"][0]["transcript"]).read_bytes()))
            return result, receipt, transcript, payloads, feedback.call_count, score.call_args_list

    def test_v2_dry_run_and_preflight_counts_are_model_free(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            protocol, _ = self.fixture(root, 30, ["first", "second"])
            configured = json.loads(protocol.read_text())
            configured["preflight_cost_cap_usd"] = 2
            protocol.write_text(json.dumps(configured))
            for preflight, slots, attempts, rounds, conditions in [(False, 2700, 5400, 3, 5), (True, 180, 360, 1, 1)]:
                with self.subTest(preflight=preflight), patch.object(repair, "load_key", side_effect=AssertionError("key")), \
                     patch.object(repair, "run", side_effect=AssertionError("execution")), \
                     patch("sys.stdout", new_callable=io.StringIO) as output:
                    argv = ["--protocol", str(protocol), "--out", str(root / "out"), "--dry-run"]
                    self.assertEqual(repair.main(argv + (["--preflight"] if preflight else [])), 0)
                plan = json.loads(output.getvalue())
                self.assertEqual((plan["max_requests"], plan["max_http_attempts"]), (slots, attempts))
                self.assertEqual((plan["max_tokens"], plan["rounds"], len(plan["conditions"])), (16384, rounds, conditions))
                self.assertEqual(plan["cost_cap"], 2 if preflight else 5)
                self.assertFalse((root / "out").exists())

    def test_retry_same_payload_retains_both_paid_attempts(self):
        first = self.response(finish="length")
        result, receipt, transcript, payloads, _, _ = self.campaign([first, self.response()], passed=True)
        row = receipt["runs"][0]
        self.assertEqual((result, row["status"], row["rounds"], row["rounds_scored"]), (0, "passed", 1, 1))
        self.assertEqual(payloads[0], payloads[1])
        self.assertEqual((row["tokens_in"], row["tokens_out"], receipt["total_cost"]), (6, 8, 0.5))
        self.assertEqual([call["attempt"] for call in row["calls"]], [1, 2])
        self.assertEqual(row["calls"][0]["response_error"], "output_budget_exhausted")
        self.assertEqual(row["calls"][1]["provider"], "actual-provider")
        self.assertNotIn("HIDDEN", json.dumps(payloads))

    def test_two_malformed_answers_are_response_error_not_failed_repair(self):
        malformed = self.response()
        malformed["choices"] = []
        _, receipt, _, payloads, _, scores = self.campaign([malformed, malformed])
        row = receipt["runs"][0]
        self.assertEqual((row["status"], row["rounds_scored"], len(payloads), len(scores)), ("response_error", 0, 2, 0))
        self.assertEqual(receipt["total_cost"], 0.5)

    def test_unknown_transport_retries_then_stops_even_after_success(self):
        _, receipt, _, payloads, _, scores = self.campaign([repair.Stop("cost_unknown:transport_error"), self.response()], passed=True)
        row = receipt["runs"][0]
        self.assertEqual((row["status"], len(payloads), len(scores)), ("passed", 2, 1))
        self.assertEqual(receipt["stop_reason"], "cost_unknown:transport_error")
        self.assertIsNone(row["calls"][0]["cost"])
        self.assertIsNone(row["calls"][0]["tokens_out"])
        self.assertEqual(receipt["total_cost"], 0.25)

    def test_unknown_transport_twice_records_response_error_and_stop(self):
        _, receipt, _, payloads, _, _ = self.campaign([repair.Stop("cost_unknown:transport_error")] * 2)
        self.assertEqual((receipt["runs"][0]["status"], len(payloads)), ("response_error", 2))
        self.assertEqual(receipt["stop_reason"], "cost_unknown:transport_error")

    def test_cap_prevents_retry_and_retains_paid_first_attempt(self):
        _, receipt, _, payloads, _, _ = self.campaign([self.response(finish="length", cost=1.25)], cap=1)
        self.assertEqual(len(payloads), 1)
        self.assertEqual((receipt["stop_reason"], receipt["total_cost"]), ("reported_cost_cap", 1.25))

    def test_c0_has_no_feedback_tools_and_only_its_extra_sentence(self):
        _, _, _, control, calls, _ = self.campaign([self.response()], condition="c0", passed=True)
        _, _, _, c3, _, _ = self.campaign([self.response()], passed=True)
        self.assertEqual(calls, 0)
        self.assertNotIn("Feedback:", control[0]["messages"][1]["content"])
        self.assertNotIn("PUBLIC", json.dumps(control))
        self.assertEqual(c3[0]["messages"][0]["content"], "COMMON INSTRUCTIONS")
        self.assertIn("This program has defects.", control[0]["messages"][0]["content"])
        self.assertNotIn("C0 ONLY", json.dumps(control + c3))

    def test_c4_initial_safe_repair_can_pass_without_model(self):
        def prepare(condition, source, row):
            self.assertEqual(condition, "c4")
            row["auto_applied"] += 1
            return FIXED, "APPLIED DIFF AND PUBLIC", []
        _, receipt, _, payloads, _, scores = self.campaign([], condition="c4", passed=True, prepare=prepare)
        row = receipt["runs"][0]
        self.assertEqual((row["status"], row["rounds"], row["tokens_in"], row["auto_applied"]), ("passed", 0, 0, 1))
        self.assertEqual(payloads, [])
        self.assertEqual(scores[0].args[0], FIXED)

    def test_c4_prepare_safe_only_and_uses_edited_bytes(self):
        row = {"auto_applied": 0}
        def run(argv, **kwargs):
            self.assertEqual(argv[1], "fix")
            self.assertIn("--apply", argv)
            self.assertNotIn("--review", argv)
            Path(argv[2]).write_text(FIXED)
            return dict(stdout='{"applied": 1}', stderr="", exit_code=0, argv=[str(a) for a in argv])
        with patch.object(repair, "run", side_effect=run), patch.object(repair, "feedback", return_value=("PUBLIC", [])) as feedback, \
             patch.object(repair, "measure", return_value=({}, "APPLIED_DIFF"), create=True):
            source, message, evidence = repair.prepare("c4", SOURCE, row)
        self.assertEqual((source, row["auto_applied"]), (FIXED, 1))
        feedback.assert_called_once_with("c3", FIXED)
        self.assertIn("APPLIED_DIFF", message)
        self.assertEqual(len(evidence), 1)

    def test_measure_original_coordinates_and_weighted_overlap(self):
        diff = "--- original\n+++ final\n@@ -0,0 +1 @@\n+prefix\n@@ -2,2 +3,3 @@\n-two\n-three\n+TWO\n+THREE\n+extra\n@@ -5 +7,0 @@\n-five\n"
        with patch.object(repair, "run", return_value=dict(exit_code=1, stdout=diff, stderr="")):
            metric, retained = repair.measure("one\ntwo\nthree\nfour\nfive\n", "replacement\n", [[1, 1], [2, 3], [3, 3], [5, 5]])
        self.assertEqual(retained, diff)
        self.assertEqual(metric, dict(diff_added=4, diff_removed=3, changed_inside_spans=7, spans_touched=[True] * 4))
        with patch.object(repair, "run", return_value=dict(exit_code=0, stdout="", stderr="")):
            metric, _ = repair.measure(SOURCE, SOURCE, [[1, 1]])
        self.assertEqual(metric["spans_touched"], [False])
        self.assertEqual(metric["diff_added"] + metric["diff_removed"], 0)

    def test_v2_request_disables_nested_http_retries(self):
        opener = Mock()
        opener.open.side_effect = urllib.error.HTTPError(repair.URL, 503, "failure", {}, io.BytesIO(b'{}'))
        with patch.object(repair.urllib.request, "build_opener", return_value=opener), patch.object(repair.time, "sleep") as sleep:
            with self.assertRaises(repair.Failure):
                repair.request({}, "unit-secret-key", [], attempts=1)
        self.assertEqual(opener.open.call_count, 1)
        sleep.assert_not_called()

    def eligibility(self, items, models, hashes):
        rows = [dict(item=item.name, model_sent=model, condition="c0", repetition=rep,
                     rounds=1, rounds_scored=1, status="passed" if rep == 1 else "failed", passed=rep == 1)
                for item in items for model in models for rep in (1, 2, 3)]
        return dict(schema="agent-repair.v2", plan={"phase": "preflight"}, stop_reason="completed",
                    provenance={"hashes": hashes}, runs=rows)

    def test_eligibility_requires_three_valid_first_responses_for_each_model(self):
        items, models, hashes = [Path("item")], ["first", "second"], {"program.c": "fixture-hash"}
        good = self.eligibility(items, models, hashes)
        repair.qualify(good, items, models, hashes)
        variants = []
        missing = copy.deepcopy(good)
        missing["runs"].pop()
        variants.append((missing, "incomplete_preflight"))
        for status in ("response_error", "error", "stopped"):
            failed_delivery = copy.deepcopy(good)
            failed_delivery["runs"][-1]["status"] = status
            variants.append((failed_delivery, "incomplete_preflight"))
        too_easy = copy.deepcopy(good)
        too_easy["runs"][-1].update(status="passed", passed=True)
        variants.append((too_easy, "preflight_item_too_easy"))
        late = copy.deepcopy(good)
        late["runs"][-1]["rounds"] = 2
        variants.append((late, "incomplete_preflight"))
        duplicate = copy.deepcopy(good)
        duplicate["runs"][-1]["repetition"] = 2
        variants.append((duplicate, "incomplete_preflight"))
        changed = copy.deepcopy(good)
        changed["provenance"]["hashes"]["program.c"] = "different"
        variants.append((changed, "preflight_input_mismatch"))
        for receipt, reason in variants:
            with self.subTest(reason=reason), self.assertRaisesRegex(repair.Failure, reason):
                repair.qualify(receipt, items, models, hashes)

    def test_preflight_clean_source_then_measurement_checks_receipt_and_hashes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp).resolve()
            here = root / "tools/experiment"
            here.mkdir(parents=True)
            protocol_path, item_root = self.fixture(root, models=["first", "second"])
            for name, value in {"repair.py": "SCRIPT", "models.txt": "first\nsecond\n", "predictions-2.md": "PREDICTIONS"}.items():
                (here / name).write_text(value)
            (root / "contracts").mkdir()
            (root / "contracts/build-environment.json").write_text('{"buildImage":{"executionReference":"sealed"}}')
            evo = root / "evo"
            evo.write_text("EVO")
            (root / "evocc").write_text("EVOCC")
            models, population = ["first", "second"], sorted(item_root.iterdir())
            protocol = json.loads(protocol_path.read_text())
            protocol["preflight_cost_cap_usd"] = 2
            protocol_path.write_text(json.dumps(protocol))
            args = SimpleNamespace(protocol=protocol_path, corpus_tag=protocol["corpus_tag"], items=item_root, preflight=True)
            plan = dict(reps=3, rounds=1, temperature=0, cost_cap=2, max_tokens=16384,
                        class_counts={"logic": 1}, conditions=["c0"], models=models, max_requests=6)
            seen = []
            def run(argv, **kwargs):
                seen.append(argv)
                if argv[:2] == ["git", "ls-files"]:
                    output = "\n".join(argv[3:])
                elif argv[:2] == ["git", "rev-parse"]:
                    output = "clean-commit"
                elif argv[0] == repair.CONTAINER:
                    output = '[{"configuration":{"descriptor":{"digest":"sealed"}}}]'
                else:
                    output = ""
                return dict(exit_code=0, stdout=output, stderr="")
            with patch.object(repair, "ROOT", root), patch.object(repair, "HERE", here), patch.object(repair, "EVO", evo), patch.object(repair, "run", side_effect=run):
                provenance = repair.preflight(args, population, models, plan)
                self.assertFalse(any("^{commit}" in str(word) for argv in seen for word in argv))
                receipt = self.eligibility(population, models, provenance["hashes"])
                Path(protocol["preflight_receipt"]).write_text(json.dumps(receipt))
                args.preflight = False
                plan.update(rounds=3, conditions=list(protocol["conditions"]), max_requests=90, cost_cap=5)
                measured = repair.preflight(args, population, models, plan)
                self.assertIn("eligibility.json", measured["hashes"])
                for target in (population[0] / "program.c", population[0] / "fix-spans.json", root / "prompt.txt", here / "repair.py"):
                    original = target.read_text()
                    target.write_text(original + "changed")
                    with self.subTest(target=target.name), self.assertRaisesRegex(repair.Failure, "preflight_input_mismatch"):
                        repair.preflight(args, population, models, plan)
                    target.write_text(original)

    def test_c4_rollback_is_not_counted_as_applied(self):
        record = dict(exit_code=1, stdout='{"applied":0}', stderr="", argv=[])
        with patch.object(repair, "run", return_value=record), patch.object(repair, "feedback", return_value=("PUBLIC", [])), \
             patch.object(repair, "measure", return_value=({}, "")):
            row = {"auto_applied": 0}
            source, _, evidence = repair.prepare("c4", SOURCE, row)
        self.assertEqual((source, row["auto_applied"]), (SOURCE, 0))
        self.assertEqual(evidence, [record])

    def test_remaining_preflight_budget_is_bounded_but_measurement_cap_is_exact(self):
        protocol_path = repair.HERE / "protocol-2.json"
        protocol = json.loads(protocol_path.read_text())
        models = protocol.get("models") or (repair.ROOT / protocol["models_file"]).read_text().split()
        allowance = protocol["preflight_cost_cap_usd"]
        args = SimpleNamespace(protocol=protocol_path, corpus_tag=protocol["corpus_tag"],
                               items=Path("/deliberately-not-the-corpus"), preflight=True)
        plan = dict(reps=3, rounds=1, temperature=protocol["temperature"], cost_cap=allowance,
                    max_tokens=protocol["max_tokens"], class_counts=protocol["population"],
                    conditions=["c0"], models=models, max_requests=0)
        def run(argv, **kwargs):
            self.assertEqual(argv[0], "git")
            return dict(exit_code=0, stdout="" if argv[1] == "status" else "clean-commit", stderr="")
        with patch.object(repair, "run", side_effect=run):
            for cap in (allowance / 2, allowance, 0, -1, allowance + 1):
                plan["cost_cap"] = cap
                # The next independent guard proves that a permitted cap passed.
                reason = "protocol_items_mismatch" if 0 < cap <= allowance else "preflight_cost_cap_exceeds_allowance"
                with self.subTest(cap=cap), self.assertRaisesRegex(repair.Failure, reason):
                    repair.preflight(args, [], models, plan)
            args.preflight = False
            plan.update(rounds=protocol["rounds"], conditions=list(protocol["conditions"]),
                        max_requests=protocol["request_limit"], cost_cap=protocol["reported_cost_cap_usd"] / 2)
            with self.assertRaisesRegex(repair.Failure, "protocol_parameter_mismatch"):
                repair.preflight(args, [], models, plan)

    def test_secret_response_is_not_retried_or_sent_to_tools(self):
        response = self.response()
        response["provider"] = "unit-secret-key"
        _, receipt, transcript, payloads, feedback_calls, scores = self.campaign([response])
        self.assertEqual((len(payloads), feedback_calls, len(scores)), (1, 1, 0))
        self.assertEqual((receipt["stop_reason"], receipt["total_cost"]), ("secret_capture_refused", 0.25))
        self.assertEqual(transcript, [])
        self.assertNotIn("unit-secret-key", json.dumps(receipt))

    def test_unknown_http_usage_stops_after_retry_instead_of_assuming_free(self):
        _, receipt, _, payloads, _, _ = self.campaign([repair.Failure("http_503"), self.response()], passed=True)
        self.assertEqual(len(payloads), 2)
        self.assertEqual(receipt["stop_reason"], "cost_unknown:http_503")
        self.assertIsNone(receipt["runs"][0]["calls"][0]["cost"])

    def test_invalid_usage_tokens_preserve_cost_and_nullable_tokens(self):
        invalid = self.response()
        del invalid["usage"]["completion_tokens"]
        _, receipt, _, payloads, _, _ = self.campaign([invalid, self.response()], passed=True)
        self.assertEqual((len(payloads), receipt["total_cost"]), (2, 0.5))
        first = receipt["runs"][0]["calls"][0]
        self.assertEqual(first["cost"], 0.25)
        self.assertEqual(first["tokens_in"], 3)
        self.assertIsNone(first["tokens_out"])
        self.assertEqual(receipt["runs"][0]["tokens_in"], 6)

    def test_unknown_cost_does_not_erase_known_tokens_or_provider(self):
        invalid = self.response()
        del invalid["usage"]["cost"]
        _, receipt, _, payloads, _, _ = self.campaign([invalid, self.response()], passed=True)
        self.assertEqual(len(payloads), 2)
        first = receipt["runs"][0]["calls"][0]
        self.assertIsNone(first["cost"])
        self.assertEqual((first["tokens_in"], first["tokens_out"], first["provider"]), (3, 4, "actual-provider"))
        self.assertEqual((receipt["runs"][0]["tokens_in"], receipt["runs"][0]["tokens_out"]), (6, 8))
        self.assertEqual(receipt["stop_reason"], "cost_unknown:missing_or_invalid_usage")

    def test_archive_splitting_does_not_stop_v2_but_preserves_v1_cap(self):
        from tools.experiment.tests.test_repair import RepairTests
        with patch.object(repair.gzip, "compress", return_value=b"x" * (9 * 1024 ** 2)), \
             patch.object(repair.gzip, "decompress", return_value=b"[]"):
            _, receipt, _, _, _, _ = self.campaign([self.response()], passed=True)
            self.assertEqual(receipt["stop_reason"], "completed")
            legacy = RepairTests()
            _, receipt, _, _ = legacy.run_campaign(legacy.response(), True)
            self.assertEqual(receipt["stop_reason"], "transcript_budget")

    def test_v2_response_capture_has_room_for_larger_output_without_changing_v1(self):
        response = self.response()
        response["padding"] = "x" * repair.LIMIT
        payload = json.dumps(response).encode()
        opener = Mock()
        for attempts, succeeds in ((4, False), (1, True)):
            opener.open.return_value = io.BytesIO(payload)
            with self.subTest(attempts=attempts), patch.object(repair.urllib.request, "build_opener", return_value=opener):
                if succeeds:
                    self.assertEqual(repair.request({}, "unit-secret-key", [], attempts=attempts)["usage"]["cost"], 0.25)
                else:
                    with self.assertRaisesRegex(repair.Stop, "response_limit"):
                        repair.request({}, "unit-secret-key", [], attempts=attempts)


if __name__ == "__main__":
    unittest.main()
