import importlib.util
import io
import gzip
import json
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import urllib.error
import subprocess

SPEC = importlib.util.spec_from_file_location("repair", Path(__file__).parents[1] / "repair.py")
repair = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(repair)


class RepairTests(unittest.TestCase):
    def test_key_environment_precedes_file(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {"OPENROUTER_KEY": "environment"}, clear=True):
            with patch.object(Path, "read_text", side_effect=AssertionError("must not read fallback")):
                self.assertEqual(repair.load_key(Path(tmp)), "environment")

    def test_key_fallback(self):
        with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=True):
            root = Path(tmp)
            (root / ".env").write_text("OTHER=x\nOPENROUTER_KEY='fixture-key'\n")
            self.assertEqual(repair.load_key(root), "fixture-key")

    def test_extract_full_file_only(self):
        source = "int main(void) { return 0; }\n"
        self.assertEqual(repair.extract("```c\n" + source + "```"), source)
        for invalid in ["no fence", "before\n```c\nx\n```", "```c\nx\n```\n```c\ny\n```", "```c\n#include <stdio.h>\n```", "```c\n%:include x\n```", "```c\n??=include x\n```"]:
            with self.subTest(invalid=invalid), self.assertRaises(repair.Failure):
                repair.extract(invalid)

    def test_prompt_has_no_hidden_data(self):
        messages = repair.messages("fixed instruction", "VISIBLE", "DIAGNOSTIC")
        self.assertEqual(messages, [{"role": "system", "content": "fixed instruction"}, {"role": "user", "content": "Current program.c:\n```c\nVISIBLE\n```\nFeedback:\nDIAGNOSTIC"}])
        self.assertNotIn("reference_main", json.dumps(messages))

    def test_dry_run_never_authenticates_or_executes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            items = root / "items"
            items.mkdir()
            for index in range(30):
                item = items / str(index)
                item.mkdir()
                (item / "program.c").write_text("int main(void){return 1;}\n")
                (item / "class").write_text(["ub", "frontend", "logic"][index % 3])
            with patch.object(repair, "load_key", side_effect=AssertionError("key read")), patch.object(repair, "run", side_effect=AssertionError("execution")), patch.object(repair, "request", side_effect=AssertionError("network")), patch("sys.stdout", new_callable=io.StringIO) as output:
                self.assertEqual(repair.main(["--items", str(items), "--out", str(root / "out"), "--model", "fixture-model", "--dry-run"]), 0)
            plan = json.loads(output.getvalue())
            self.assertEqual(plan["max_requests"], 810)
            self.assertEqual(plan["max_http_attempts"], 3240)
            self.assertFalse((root / "out").exists())

    def test_no_redirect(self):
        redirect = repair.NoRedirect()
        self.assertIsNone(redirect.redirect_request(None, None, 302, "redirect", {}, "https://example.com"))

    def test_http_retry_and_nonretry(self):
        response = json.dumps({"model": "fixture", "provider": "mock", "usage": {"cost": 0.01, "prompt_tokens": 1, "completion_tokens": 2}, "choices": [{"message": {"content": "```c\nint main(void){return 0;}\n```"}}]}).encode()
        class Reply(io.BytesIO):
            status = 200
        for code, attempts in [(429, 4), (503, 4), (400, 1), (302, 1)]:
            events = []
            errors = [urllib.error.HTTPError(repair.URL, code, "failure", {}, io.BytesIO(b'{}')) for _ in range(attempts)]
            opener = unittest.mock.Mock()
            opener.open.side_effect = errors
            with self.subTest(code=code), patch.object(repair.urllib.request, "build_opener", return_value=opener), patch.object(repair.time, "sleep") as sleep, self.assertRaises(repair.Failure):
                repair.request({"messages": []}, "fixture-key", events)
            self.assertEqual(len(events), attempts)
            self.assertEqual(sleep.call_count, attempts - 1)
        events = []
        opener = unittest.mock.Mock()
        opener.open.return_value = Reply(response)
        with patch.object(repair.urllib.request, "build_opener", return_value=opener):
            result = repair.request({"messages": []}, "fixture-key", events)
        self.assertEqual(result["provider"], "mock")
        sent = opener.open.call_args.args[0]
        self.assertEqual(sent.full_url, repair.URL)
        self.assertEqual(sent.get_header("Authorization"), "Bearer fixture-key")
        self.assertNotIn("fixture-key", json.dumps(events))

    def test_missing_cost_stops(self):
        with self.assertRaises(repair.Stop):
            repair.usage({"usage": {"prompt_tokens": 1, "completion_tokens": 1}})

    def test_secret_capture_rejected(self):
        with self.assertRaises(repair.Stop):
            repair.encode({"response": "sk-or-" + "fixture"})

    def test_c3_commands_and_verbatim_json(self):
        records = []
        def mocked_run(argv, **kwargs):
            records.append([str(arg) for arg in argv])
            stdout = '{"diagnostics":[{"code":"E002"},{"code":"E001"},{"code":"E002"}]}\n' if argv[1] == "check" else '{"kind":"' + argv[1] + '"}\n'
            return {"argv": records[-1], "exit_code": 0, "stdout": stdout, "stderr": ""}
        with patch.object(repair, "run", side_effect=mocked_run):
            feedback, evidence = repair.feedback("c3", "int main(void){return 0;}\n")
        self.assertEqual([record[1] for record in records], ["check", "explain", "explain", "fix"])
        self.assertEqual([records[1][2], records[2][2]], ["E001", "E002"])
        self.assertTrue(records[-1][2].endswith("/program.c"))
        self.assertEqual(records[-1][3:], ["--plan", "--json"])
        self.assertEqual(feedback, "".join(record["stdout"] for record in evidence))

    def test_native_readonly_input_and_cleanup_timeout(self):
        def mocked_run(argv, **kwargs):
            mount = argv[argv.index("-v") + 1]
            self.assertTrue(mount.endswith(":/input:ro"))
            host = Path(mount.removesuffix(":/input:ro"))
            self.assertEqual(sorted(p.name for p in host.iterdir()), ["program.c"])
            self.assertEqual(json.loads(kwargs["data"])["reference"], "HIDDEN")
            self.assertEqual(argv[argv.index("--network") + 1], "none")
            self.assertNotIn("OPENROUTER_KEY", repair.ENV)
            return {"exit_code": 0, "stdout": "[]", "stderr": ""}
        with patch.object(repair, "run", side_effect=mocked_run), patch.object(repair.subprocess, "run", side_effect=subprocess.TimeoutExpired("cleanup", 20)):
            with self.assertRaisesRegex(repair.Stop, "container_cleanup_failed"):
                repair.native("score", "int main(void){return 0;}", "HIDDEN")
        self.assertLess(repair.WORKER.index('"-c", "program.c"'), repair.WORKER.index('write_text(payload["reference"])'))
        self.assertLess(repair.WORKER.index('Path("reference_main.c").unlink()'), repair.WORKER.index('if linked: step(["./prog"])'))

    def run_campaign(self, response, score_passed=False, cap=5, feedback_error=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            item = root / "items" / "sample"
            item.mkdir(parents=True)
            (item / "program.c").write_text("int main(void){return 1;}\n")
            (item / "class").write_text("logic\n")
            (item / "origin.md").write_text("Written for this corpus.\n")
            out = root / "out"
            def mocked_request(payload, key, events):
                events.append({"http_status": 200, "response": json.dumps(response)})
                return response
            def mocked_feedback(condition, source):
                if feedback_error and "return 0" in source:
                    raise feedback_error
                return "public diagnostic", []
            with patch.object(repair, "preflight", return_value={"commit": "fixture"}), patch.object(repair, "load_key", return_value="unit-secret-key"), patch.object(repair, "feedback", side_effect=mocked_feedback) as feedback, patch.object(repair, "request", side_effect=mocked_request) as request, patch.object(repair, "score", return_value=(score_passed, [{"stdout": "HIDDEN"}])) as score, patch("sys.stdout", new_callable=io.StringIO):
                result = repair.main(["--items", str(item.parent), "--out", str(out), "--model", "fixture-model", "--condition", "c3", "--reps", "1", "--rounds", "2", "--cost-cap", str(cap)])
            receipt = json.loads((out / "receipt.json").read_text())
            transcript = json.loads(gzip.decompress((out / receipt["runs"][0]["transcript"]).read_bytes()))
            return result, receipt, transcript, (feedback.call_count, request.call_count, score.call_count)

    def response(self, cost=0.25):
        return {"model": "actual-model", "provider": "actual-provider", "usage": {"cost": cost, "prompt_tokens": 3, "completion_tokens": 4}, "choices": [{"message": {"content": "```c\nint main(void){return 0;}\n```"}}]}

    def test_campaign_success_stops_and_records_successful_round(self):
        result, receipt, transcript, calls = self.run_campaign(self.response(), True)
        self.assertEqual(result, 0)
        self.assertEqual(calls, (2, 1, 1))
        row = receipt["runs"][0]
        self.assertEqual((row["status"], row["rounds"], row["rounds_scored"], row["synthetic"]), ("passed", 1, 1, True))
        self.assertEqual(receipt["total_cost"], 0.25)
        self.assertTrue(row["calls"][0]["score_passed"])
        self.assertEqual(row["calls"][0]["reported_model"], "actual-model")
        self.assertTrue(transcript[-1]["score_passed"])
        self.assertNotIn("HIDDEN", json.dumps(transcript[-1]["request"]))

    def test_campaign_budget_stops_incomplete_trial(self):
        _, receipt, _, calls = self.run_campaign(self.response(1.25), False, cap=1)
        self.assertEqual(calls, (2, 1, 1))
        self.assertEqual(receipt["total_cost"], 1.25)
        self.assertEqual(receipt["stop_reason"], "reported_cost_cap")
        self.assertEqual(receipt["runs"][0]["status"], "stopped")

    def test_campaign_unknown_cost_stops_before_tools(self):
        response = self.response()
        del response["usage"]["cost"]
        result, receipt, _, calls = self.run_campaign(response)
        self.assertEqual(result, 1)
        self.assertEqual(calls, (1, 1, 0))
        self.assertIn("cost_unknown", receipt["stop_reason"])
        self.assertFalse(receipt["acceptance"])

    def test_campaign_invalid_tokens_retains_reported_cost(self):
        response = self.response()
        del response["usage"]["completion_tokens"]
        _, receipt, _, calls = self.run_campaign(response)
        self.assertEqual(calls, (1, 1, 0))
        self.assertEqual(receipt["total_cost"], 0.25)
        self.assertEqual(receipt["runs"][0]["cost"], 0.25)
        self.assertEqual(receipt["stop_reason"], "invalid_usage_tokens")

    def test_campaign_secret_never_reaches_tool_or_saved_metadata(self):
        response = self.response()
        response["provider"] = "unit-secret-key"
        _, receipt, transcript, calls = self.run_campaign(response)
        self.assertEqual(calls, (1, 1, 0))
        self.assertEqual(receipt["total_cost"], 0.25)
        self.assertEqual(receipt["stop_reason"], "secret_capture_refused")
        self.assertEqual(transcript, [])
        self.assertNotIn("unit-secret-key", json.dumps(receipt))
        self.assertFalse(receipt["acceptance"])

    def test_campaign_cleanup_error_preserves_paid_response(self):
        _, receipt, transcript, calls = self.run_campaign(self.response(), feedback_error=repair.Stop("container_cleanup_failed"))
        self.assertEqual(calls, (2, 1, 0))
        self.assertEqual(receipt["total_cost"], 0.25)
        self.assertEqual(receipt["runs"][0]["status"], "stopped")
        self.assertEqual(receipt["stop_reason"], "container_cleanup_failed")
        self.assertIn("http", transcript[-1])

    def test_campaign_exhaustion_is_failed_not_error(self):
        result, receipt, _, calls = self.run_campaign(self.response())
        self.assertEqual(result, 0)
        self.assertEqual(calls, (3, 2, 2))
        row = receipt["runs"][0]
        self.assertEqual((row["status"], row["rounds"], row["rounds_scored"], row["error"]), ("failed", 2, 2, None))
        self.assertEqual(receipt["total_cost"], 0.5)

    def test_campaign_malformed_choice_keeps_paid_failure(self):
        response = self.response()
        del response["choices"]
        _, receipt, _, calls = self.run_campaign(response)
        self.assertEqual(calls, (1, 1, 0))
        row = receipt["runs"][0]
        self.assertEqual((row["status"], row["rounds"], row["rounds_scored"], row["error"]), ("error", 1, 0, "malformed_response"))
        self.assertEqual(receipt["total_cost"], 0.25)

    def test_preflight_rejects_unregistered_rounds_before_container(self):
        protocol = json.loads((repair.HERE / "protocol.json").read_text())
        models = (repair.HERE / "models.txt").read_text().split()
        args = SimpleNamespace(corpus_tag=protocol["corpus_tag"], items=repair.ROOT / "tests/fixtures/agent-repair")
        plan = dict(reps=protocol["repetitions"], rounds=protocol["rounds"] + 1, temperature=protocol["temperature"], cost_cap=protocol["reported_cost_cap_usd"], max_tokens=protocol["max_tokens"], class_counts=protocol["population"], conditions=list(protocol["conditions"]), models=models, max_requests=protocol["request_limit"])
        def mocked_run(argv, **kwargs):
            self.assertEqual(argv[0], "git")
            return {"exit_code": 0, "stdout": "" if argv[1] == "status" else "fixture-commit", "stderr": ""}
        with patch.object(repair, "run", side_effect=mocked_run), self.assertRaisesRegex(repair.Failure, "protocol_parameter_mismatch"):
            repair.preflight(args, [], models, plan)

    def test_spliced_preprocessor_marker_is_rejected(self):
        answer = "```c\n%\\\n:include x\n```"
        with self.assertRaisesRegex(repair.Failure, "preprocessor_or_size"):
            repair.extract(answer)

    def test_http_error_usage_preserves_known_cost(self):
        body = json.dumps({"usage": {"cost": 0.1, "prompt_tokens": 1, "completion_tokens": 1}}).encode()
        error = urllib.error.HTTPError(repair.URL, 503, "failure", {}, io.BytesIO(body))
        opener = unittest.mock.Mock()
        opener.open.side_effect = error
        with patch.object(repair.urllib.request, "build_opener", return_value=opener), self.assertRaises(repair.Stop) as caught:
            repair.request({}, "fixture-key", [])
        self.assertEqual(caught.exception.cost, 0.1)


if __name__ == "__main__":
    unittest.main()
