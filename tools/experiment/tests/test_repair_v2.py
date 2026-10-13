"""Model-free tests for the second experiment; never execute a candidate."""
import copy
import errno
import gzip
import hashlib
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

    def test_v2_request_stops_on_refused_key_or_credit(self):
        for code in (401, 402):
            opener = Mock()
            opener.open.side_effect = urllib.error.HTTPError(repair.URL, code, "refused", {}, io.BytesIO(b'{"error":{}}'))
            with self.subTest(code=code), patch.object(repair.urllib.request, "build_opener", return_value=opener):
                with self.assertRaisesRegex(repair.Stop, "^provider_refused_key:http_" + str(code) + "$"):
                    repair.request({}, "unit-secret-key", [], attempts=1)

    def test_bounded_request_has_a_total_deadline(self):
        release = repair.threading.Event()
        def stalled(payload, key, events, **kwargs):
            events.append(dict(http_status=200, response="\n   "))
            release.wait(30)
            return self.response()
        events = []
        with patch.object(repair, "request", side_effect=stalled):
            start = repair.time.monotonic()
            with self.assertRaisesRegex(repair.Stop, "^cost_unknown:timeout$"):
                repair.bounded({}, "unit-secret-key", events, 0.2)
            elapsed = repair.time.monotonic() - start
        release.set()
        self.assertLess(elapsed, 2)
        self.assertEqual(events[-1], dict(transport_error=dict(kind="DeadlineExceeded", cause=None, errno=None), deadline_seconds=0.2))
        with patch.object(repair, "request", return_value=self.response()):
            self.assertEqual(repair.bounded({}, "unit-secret-key", [], 5)["provider"], "actual-provider")
        with patch.object(repair, "request", side_effect=repair.Failure("http_503")), self.assertRaisesRegex(repair.Failure, "http_503"):
            repair.bounded({}, "unit-secret-key", [], 5)

    def run_many(self, count, request, parallel=1, cap=5, extra=None, score=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            protocol, items = self.fixture(root, count)
            for index in range(count):
                (items / str(index) / "program.c").write_text("/* item " + str(index) + " */\n" + SOURCE)
            for index in range(count):
                (items / str(index)).rename(items / ("item-%02d" % index))
            configured = json.loads(protocol.read_text())
            configured.update(extra or {})
            protocol.write_text(json.dumps(configured))
            metric = dict(diff_added=1, diff_removed=1, changed_inside_spans=2, spans_touched=[True])
            with patch.object(repair, "preflight", return_value={"commit": "fixture"}), \
                 patch.object(repair, "load_key", return_value="unit-secret-key"), \
                 patch.object(repair, "request", side_effect=request), \
                 patch.object(repair, "score", side_effect=score or (lambda source, item: (False, []))), \
                 patch.object(repair, "measure", return_value=(metric, "DIFF")), \
                 patch("sys.stdout", new_callable=io.StringIO):
                result = repair.main(["--protocol", str(protocol), "--out", str(root / "out"), "--condition", "c0",
                                      "--reps", "1", "--rounds", "1", "--max-cost-usd", str(cap), "--parallel", str(parallel)])
            receipt = json.loads((root / "out/receipt.json").read_text())
            names = sorted(path.name for path in (root / "out").glob("*.json.gz"))
            return result, receipt, names

    def test_stalled_response_is_response_error_and_the_run_continues(self):
        release = repair.threading.Event()
        def request(payload, key, events, **kwargs):
            if "item 0 " in payload["messages"][1]["content"]:
                release.wait(30)
            events.append(dict(http_status=200, response="{}"))
            return self.response()
        extra = dict(limits=dict(http_timeout_seconds=0.2), unknown_cost_reserve_usd=0.05)
        try:
            result, receipt, names = self.run_many(2, request, extra=extra)
        finally:
            release.set()
        stalled, later = receipt["runs"]
        self.assertEqual((result, receipt["stop_reason"], receipt["unattempted"]), (0, "completed", 0))
        self.assertEqual((stalled["status"], stalled["rounds_scored"]), ("response_error", 0))
        self.assertEqual([c["response_error"] for c in stalled["calls"]], ["cost_unknown:timeout"] * 2)
        self.assertEqual([c["cost"] for c in stalled["calls"]], [None, None])
        self.assertLess(stalled["seconds"], 2 * 0.2 + 1.5)
        self.assertEqual((later["status"], later["cost"]), ("failed", 0.25))
        self.assertEqual((receipt["unknown_cost_attempts"], receipt["total_cost"]), (2, 0.25))
        self.assertAlmostEqual(receipt["unknown_cost_reserve"], 0.1)
        self.assertEqual(names, ["0000.json.gz", "0001.json.gz"])

    def test_scoring_timeout_is_recorded_and_the_run_continues(self):
        def score(source, item):
            if item.name == "item-00": raise repair.Failure("infrastructure_timeout", [dict(argv=["container"], exit_code=None)])
            return True, []
        def request(payload, key, events, **kwargs):
            events.append(dict(http_status=200, response="{}"))
            return self.response()
        result, receipt, _ = self.run_many(2, request, parallel=2, score=score)
        self.assertEqual((result, receipt["stop_reason"]), (0, "completed"))
        self.assertEqual([(r["status"], r["error"]) for r in receipt["runs"]], [("error", "infrastructure_timeout"), ("passed", None)])

    def test_unknown_cost_reserve_counts_against_the_cap(self):
        def request(payload, key, events, **kwargs):
            raise repair.Stop("cost_unknown:transport_error")
        _, receipt, _ = self.run_many(3, request, cap=0.15, extra=dict(unknown_cost_reserve_usd=0.05))
        self.assertEqual(receipt["stop_reason"], "reported_cost_cap")
        self.assertEqual([r["status"] for r in receipt["runs"]], ["response_error", "stopped"])
        self.assertEqual((receipt["unknown_cost_attempts"], receipt["unattempted"]), (3, 1))

    def test_parallel_slots_run_concurrently_in_registered_order(self):
        lock, active, peak, started = repair.threading.Lock(), [0], [0], []
        def request(payload, key, events, **kwargs):
            with lock:
                active[0] += 1; peak[0] = max(peak[0], active[0])
                started.append(payload["messages"][1]["content"].split("```c\n")[1].split("*/")[0])
            repair.time.sleep(0.1)
            with lock: active[0] -= 1
            events.append(dict(http_status=200, response="{}"))
            return self.response()
        result, receipt, names = self.run_many(8, request, parallel=4)
        self.assertEqual((result, receipt["stop_reason"], receipt["parallel"]), (0, "completed", 4))
        self.assertGreater(peak[0], 1)
        self.assertLessEqual(peak[0], 4)
        self.assertEqual([r["start_order"] for r in receipt["runs"]], list(range(8)))
        self.assertEqual([r["item"] for r in receipt["runs"]], ["item-%02d" % n for n in range(8)])
        self.assertEqual([r["transcript"] for r in receipt["runs"]], names)
        self.assertEqual(receipt["total_cost"], 8 * 0.25)
        self.assertEqual(sorted(started), ["/* item %d " % n for n in range(8)])

    def test_parallel_cost_stop_keeps_an_attempted_prefix(self):
        def request(payload, key, events, **kwargs):
            repair.time.sleep(0.05)
            events.append(dict(http_status=200, response="{}"))
            return self.response(cost=0.5)
        result, receipt, names = self.run_many(12, request, parallel=3, cap=2)
        attempted = len(receipt["runs"])
        self.assertEqual((result, receipt["stop_reason"]), (0, "reported_cost_cap"))
        self.assertLess(attempted, 12)
        self.assertEqual([r["start_order"] for r in receipt["runs"]], list(range(attempted)))
        self.assertEqual(receipt["unattempted"], 12 - attempted)
        self.assertLessEqual(receipt["total_cost"], 2 + 3 * 0.5)
        self.assertEqual(names, ["%04d.json.gz" % n for n in range(attempted)])

    def test_worker_exception_stops_and_is_raised(self):
        def request(payload, key, events, **kwargs):
            raise AssertionError("unexpected harness failure")
        with self.assertRaisesRegex(AssertionError, "unexpected harness failure"):
            self.run_many(3, request, parallel=2)

    def test_transport_error_records_only_exception_classes_and_errno(self):
        secret = "unit-secret-key sk-or-fixture-transport-detail"
        variants = [
            (urllib.error.URLError(ConnectionResetError(errno.ECONNRESET, secret)),
             dict(kind="URLError", cause="ConnectionResetError", errno=errno.ECONNRESET)),
            (ConnectionResetError(errno.ECONNRESET, secret),
             dict(kind="ConnectionResetError", cause=None, errno=errno.ECONNRESET)),
            (TimeoutError(secret), dict(kind="TimeoutError", cause=None, errno=None)),
        ]
        for error, metadata in variants:
            opener, events = Mock(), []
            opener.open.side_effect = error
            with self.subTest(kind=metadata["kind"]), \
                 patch.object(repair.urllib.request, "build_opener", return_value=opener):
                with self.assertRaisesRegex(repair.Stop, "^cost_unknown:transport_error$"):
                    repair.request({}, "unit-secret-key", events, attempts=1)
                self.assertEqual(opener.open.call_count, 1)
                self.assertEqual(events, [dict(transport_error=metadata)])
                captured = repair.encode(events, "unit-secret-key")
                self.assertNotIn(secret, captured)
                self.assertNotIn("sk-or-", captured)

    def eligibility(self, items, models, hashes):
        rows = [dict(item=item.name, model_sent=model, condition="c0", repetition=rep,
                     rounds=1, rounds_scored=1, status="passed" if rep == 1 else "failed", passed=rep == 1)
                for item in items for model in models for rep in (1, 2, 3)]
        return dict(schema="agent-repair.v2", plan={"phase": "preflight"}, stop_reason="completed",
                    provenance={"hashes": hashes}, runs=rows)

    def test_eligibility_requires_three_first_response_slots_for_each_model(self):
        items, models, hashes = [Path("item")], ["first", "second"], {"program.c": "fixture-hash"}
        good = self.eligibility(items, models, hashes)
        self.assertEqual(repair.qualify(good, items, models, hashes), {"first": ["item"], "second": ["item"]})
        variants = []
        missing = copy.deepcopy(good)
        missing["runs"].pop()
        variants.append((missing, "incomplete_preflight"))
        for status in ("error", "stopped"):
            failed_delivery = copy.deepcopy(good)
            failed_delivery["runs"][-1]["status"] = status
            variants.append((failed_delivery, "incomplete_preflight"))
        too_easy = copy.deepcopy(good)
        too_easy["runs"][-1].update(status="passed", passed=True)
        # Easy items stay in the corpus; the population rule in predictions-2.md decides their use.
        self.assertEqual(repair.qualify(too_easy, items, models, hashes), {"first": ["item"], "second": []})
        decided = copy.deepcopy(good)
        current = dict(hashes, **{"tools/experiment/predictions-2.md": "population-decision"})
        decided["provenance"]["hashes"]["tools/experiment/predictions-2.md"] = "draft"
        self.assertEqual(repair.qualify(decided, items, models, current), {"first": ["item"], "second": ["item"]})
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

    def test_eligibility_accepts_error_slot_with_two_genuine_scored_failures(self):
        items, models, hashes = [Path("item")], ["first", "second"], {"program.c": "fixture-hash"}
        receipt = self.eligibility(items, models, hashes)
        receipt["runs"][0].update(status="response_error", rounds_scored=0,
                                  passed=False, error="response_error")
        original = copy.deepcopy(receipt)
        self.assertEqual(repair.qualify(receipt, items, models, hashes), {"first": ["item"], "second": ["item"]})
        self.assertEqual(receipt, original)

    def test_eligibility_response_errors_never_count_as_scored_failures(self):
        items, models, hashes = [Path("item")], ["first", "second"], {"program.c": "fixture-hash"}
        for error_repetitions in ((3,), (1, 3), (1, 2, 3)):
            receipt = self.eligibility(items, models, hashes)
            for rep in error_repetitions:
                receipt["runs"][rep - 1].update(status="response_error", rounds_scored=0,
                                               passed=False, error="response_error")
            with self.subTest(error_repetitions=error_repetitions):
                self.assertEqual(repair.qualify(receipt, items, models, hashes), {"first": [], "second": ["item"]})

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
                self.assertEqual(measured["preflight_hard"], {"first": ["0"], "second": ["0"]})
                (here / "predictions-2.md").write_text("PREDICTIONS AND POPULATION DECISION")
                self.assertEqual(repair.preflight(args, population, models, plan)["preflight_hard"], measured["preflight_hard"])
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

    def resume_fixture(self, root):
        previous, output = root / "previous", root / "output"
        previous.mkdir(); output.mkdir()
        order = [dict(item=item, model=model, repetition=rep, condition="c0")
                 for rep in (1, 2, 3) for item in ("adler", "bitstream")
                 for model in ("first", "second")]
        plan = dict(items=2, models=["first", "second"], conditions=["c0"], reps=3,
                    rounds=1, temperature=0, cost_cap=11, max_tokens=16384,
                    max_requests=12, max_http_attempts=24, phase="preflight")
        hashes = {path: "frozen-" + path for path in (
            "tools/experiment/repair.py", "tools/experiment/protocol-2.json",
            "tools/experiment/prompt-2.txt", "tools/experiment/models.txt",
            "tools/experiment/predictions-2.md", "contracts/build-environment.json",
            "tests/fixtures/agent-repair-2/adler/program.c",
            "tests/fixtures/agent-repair-2/bitstream/program.c")}
        provenance = dict(commit="old-commit", corpus_tag="agent-repair-experiment-2",
                          hashes=hashes, evo_sha256="same-evo", evocc_sha256="same-evocc",
                          version="fixture-version", image=repair.IMAGE,
                          image_inspect='[{"configuration":{"descriptor":{"digest":"sealed"}}}]')
        rows = []
        for index, slot in enumerate(order[:4]):
            error = index >= 2
            calls = [dict(attempt=attempt, cost=None if index == 3 else 0.25,
                          response_error="cost_unknown:transport_error" if index == 3
                          else "output_budget_exhausted" if error else None)
                     for attempt in range(1, 3 if error else 2)]
            row = dict(item=slot["item"], model_sent=slot["model"], condition="c0",
                       repetition=slot["repetition"], rounds=1, rounds_scored=0 if error else 1,
                       status="response_error" if error else "passed" if index == 0 else "failed",
                       passed=index == 0, error="response_error" if error else None,
                       cost=sum(call["cost"] or 0 for call in calls), calls=calls,
                       transcript=f"{index:04d}.json.gz")
            rows.append(row)
            capture = json.dumps([dict(slot=index, calls=calls)]).encode()
            (previous / row["transcript"]).write_bytes(gzip.compress(capture, mtime=0))
        prior = dict(schema="agent-repair.v2", date="prior-date", plan=plan,
                     provenance=provenance, runs=rows, total_cost=sum(row["cost"] for row in rows),
                     stop_reason="cost_unknown:transport_error", acceptance=False, unattempted=8)
        (previous / "receipt.json").write_text(json.dumps(prior, indent=2) + "\n")
        (previous / "plan.json").write_text(json.dumps(dict(plan=plan, provenance=provenance, order=order)))
        fresh = copy.deepcopy(prior)
        fresh.update(date="continuation-date", runs=[], total_cost=0, stop_reason=None)
        fresh["provenance"]["commit"] = "new-commit"
        for path in ("tools/experiment/repair.py", "tools/experiment/protocol-2.json"):
            fresh["provenance"]["hashes"][path] = "amended-" + path
        return previous, output, prior, fresh, order

    def test_resume_prefix_copies_evidence_costs_and_capture_totals_without_retry(self):
        with tempfile.TemporaryDirectory() as tmp:
            previous, output, prior, fresh, order = self.resume_fixture(Path(tmp))
            originals = {path.name: path.read_bytes() for path in previous.iterdir()}
            current_provenance = copy.deepcopy(fresh["provenance"])
            packed = [originals[row["transcript"]] for row in prior["runs"]]
            with patch.object(repair, "request", side_effect=AssertionError("no added retry")) as request:
                totals = repair.resume_prefix(previous, output, fresh, order, "unit-secret-key")
            request.assert_not_called()
            self.assertEqual(totals, (sum(map(len, packed)), sum(len(gzip.decompress(data)) for data in packed)))
            self.assertEqual(fresh["runs"], prior["runs"])
            self.assertEqual(fresh["total_cost"], prior["total_cost"])
            self.assertEqual(fresh["provenance"], current_provenance)
            self.assertIsNone(fresh["runs"][-1]["calls"][0]["cost"])
            self.assertEqual(len(fresh["runs"][-1]["calls"]), 2)
            self.assertEqual((output / "prior-receipt.json").read_bytes(), originals["receipt.json"])
            self.assertEqual(fresh["continuation"]["prior_receipt_sha256"],
                             hashlib.sha256(originals["receipt.json"]).hexdigest())
            self.assertEqual(fresh["continuation"]["prior_provenance"], prior["provenance"])
            for row in prior["runs"]:
                self.assertEqual((output / row["transcript"]).read_bytes(), originals[row["transcript"]])
            self.assertFalse((output / "0004.json.gz").exists())
            self.assertEqual({path.name: path.read_bytes() for path in previous.iterdir()}, originals)

    def test_resume_prefix_rejects_changed_order_and_row_identity(self):
        for field in ("order", "item", "model_sent", "repetition", "condition"):
            with self.subTest(field=field), tempfile.TemporaryDirectory() as tmp:
                previous, output, prior, fresh, order = self.resume_fixture(Path(tmp))
                if field == "order":
                    order[-1]["model"] = "changed-unattempted-model"
                else:
                    prior["runs"][0][field] = 2 if field == "repetition" else "changed"
                    (previous / "receipt.json").write_text(json.dumps(prior))
                with self.assertRaises(repair.Failure):
                    repair.resume_prefix(previous, output, fresh, order, "unit-secret-key")

    def test_resume_prefix_rejects_missing_transcript(self):
        with tempfile.TemporaryDirectory() as tmp:
            previous, output, prior, fresh, order = self.resume_fixture(Path(tmp))
            (previous / prior["runs"][2]["transcript"]).unlink()
            with self.assertRaises((repair.Failure, OSError)):
                repair.resume_prefix(previous, output, fresh, order, "unit-secret-key")

    def test_resume_prefix_rejects_changed_frozen_inputs_and_compiler(self):
        fields = ["tests/fixtures/agent-repair-2/adler/program.c", "tools/experiment/prompt-2.txt",
                  "tools/experiment/models.txt", "tools/experiment/predictions-2.md",
                  "contracts/build-environment.json", "evo_sha256", "evocc_sha256",
                  "version", "image", "image_inspect", "corpus_tag"]
        for field in fields:
            with self.subTest(field=field), tempfile.TemporaryDirectory() as tmp:
                previous, output, _, fresh, order = self.resume_fixture(Path(tmp))
                target = fresh["provenance"]["hashes"] if "/" in field else fresh["provenance"]
                target[field] = '[{"configuration":{"descriptor":{"digest":"changed"}}}]' if field == "image_inspect" else "changed"
                with self.assertRaises(repair.Failure):
                    repair.resume_prefix(previous, output, fresh, order, "unit-secret-key")

    def test_resume_prefix_refuses_non_preflight_and_v1_receipts(self):
        for variant in ("prior-measurement", "fresh-measurement", "both-measurement", "v1"):
            with self.subTest(variant=variant), tempfile.TemporaryDirectory() as tmp:
                previous, output, prior, fresh, order = self.resume_fixture(Path(tmp))
                if variant == "v1":
                    prior["schema"] = fresh["schema"] = "agent-repair.v1"
                else:
                    if variant in ("prior-measurement", "both-measurement"):
                        prior["plan"]["phase"] = "measurement"
                    if variant in ("fresh-measurement", "both-measurement"):
                        fresh["plan"]["phase"] = "measurement"
                (previous / "receipt.json").write_text(json.dumps(prior))
                (previous / "plan.json").write_text(json.dumps(dict(plan=prior["plan"],
                                                                   provenance=prior["provenance"], order=order)))
                with self.assertRaises(repair.Failure):
                    repair.resume_prefix(previous, output, fresh, order, "unit-secret-key")

    def test_resume_main_calls_only_unattempted_slots_and_preserves_prefix(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            previous, _, prior, fresh, order = self.resume_fixture(root)
            protocol, items = self.fixture(root, 2, ["first", "second"])
            for index, name in enumerate(("adler", "bitstream")):
                (items / str(index)).rename(items / name)
                (items / name / "program.c").write_text("/* " + name + " */\n" + SOURCE)
            configured = json.loads(protocol.read_text())
            configured["preflight_cost_cap_usd"] = 11
            protocol.write_text(json.dumps(configured))
            prior["plan"]["class_counts"] = {"logic": 2}
            (previous / "receipt.json").write_text(json.dumps(prior, indent=2) + "\n")
            (previous / "plan.json").write_text(json.dumps(dict(plan=prior["plan"],
                                                               provenance=prior["provenance"], order=order)))
            prefix_bytes = {row["transcript"]: (previous / row["transcript"]).read_bytes()
                            for row in prior["runs"]}
            prior_bytes = (previous / "receipt.json").read_bytes()
            sent = []
            def request(payload, key, events, **kwargs):
                program = payload["messages"][1]["content"]
                item = "adler" if "/* adler */" in program else "bitstream"
                sent.append((item, payload["model"]))
                response = self.response()
                events.append(dict(http_status=200, response=json.dumps(response)))
                return response
            metric = dict(diff_added=1, diff_removed=1, changed_inside_spans=2, spans_touched=[True])
            output = root / "continued"
            with patch.object(repair, "preflight", return_value=fresh["provenance"]), \
                 patch.object(repair, "load_key", return_value="unit-secret-key"), \
                 patch.object(repair, "request", side_effect=request), \
                 patch.object(repair, "score", return_value=(True, [])), \
                 patch.object(repair, "measure", return_value=(metric, "DIFF")), \
                 patch.object(repair.shutil, "disk_usage", return_value=SimpleNamespace(free=30 * 1024 ** 3)), \
                 patch("sys.stdout", new_callable=io.StringIO):
                result = repair.main(["--protocol", str(protocol), "--preflight", "--resume-from",
                                      str(previous), "--out", str(output), "--parallel", "3"])
            receipt = json.loads((output / "receipt.json").read_text())
            self.assertEqual(result, 0)
            self.assertEqual(sorted(sent), sorted((slot["item"], slot["model"]) for slot in order[4:]))
            self.assertEqual([r["start_order"] for r in receipt["runs"][4:]], list(range(4, len(order))))
            self.assertEqual(receipt["runs"][:4], prior["runs"])
            self.assertEqual(len(receipt["runs"]), len(order))
            self.assertEqual(receipt["total_cost"], prior["total_cost"] + len(sent) * 0.25)
            self.assertEqual((output / "prior-receipt.json").read_bytes(), prior_bytes)
            for name, packed in prefix_bytes.items():
                self.assertEqual((output / name).read_bytes(), packed)
            self.assertTrue((output / "0004.json.gz").exists())


if __name__ == "__main__":
    unittest.main()
