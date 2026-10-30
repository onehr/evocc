"""Model-free tests of the Protocol A harness against a local fake HTTP server."""
from collections import Counter
import copy
import gzip
import hashlib
import http.server
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import tempfile
import threading
from types import SimpleNamespace
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("budget", HERE / "budget.py")
budget = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(budget)
ROOT = HERE.parents[1]
KEY = "unit-secret-key"
PROTOCOL = budget.load()
PILOT = PROTOCOL["phases"]["pilot"]["trials"]
EVO = str(budget.repair.EVO)
CHECK = '{"exit_code":1,"diagnostics":[{"code":"E3004","file":"%s/program.c"}]}\n'
EXPLAIN = '{"code":"E3004","title":"signed-integer-overflow","text":"# E3004: the page with its long example","exit_code":0}\n'
PLAN = '{"file":"%s/program.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}\n'
ENDPOINTS = json.dumps({"data": {"endpoints": [
    {"provider_name": "AtlasCloud", "tag": "atlas-cloud/fp8", "max_completion_tokens": 393216},
    {"provider_name": "Together", "tag": "together"}]}}).encode()
HASHES = dict(evo_sha256="evo", evocc_sha256="evocc")


class Feedback:
    """Stand-in for repair.feedback: check, explain and plan records in a fresh scratch path per call."""

    def __init__(self, check=CHECK, code="E3004"):
        self.check, self.code, self.calls = check, code, 0

    def __call__(self, condition, source):
        assert condition == "c3" and source.startswith("/* ")
        self.calls += 1
        scratch = "/tmp/agent-repair-evo-%08d" % self.calls
        records = [dict(argv=[EVO, "check", "--json", scratch + "/program.c"], exit_code=1, stdout=self.check % scratch, stderr=""),
                   dict(argv=[EVO, "explain", self.code, "--json"], exit_code=0, stdout=EXPLAIN, stderr=""),
                   dict(argv=[EVO, "fix", scratch + "/program.c", "--plan", "--json"], exit_code=0, stdout=PLAN % scratch, stderr="")]
        return "".join(record["stdout"] for record in records), records


def references():
    scratch = "/var/folders/p17/agent-repair-evo-qohxv48y"
    text = CHECK % scratch + EXPLAIN + PLAN % scratch
    return {name: dict(transcript="0060.json.gz", feedback=text, scratch=scratch, exit_codes=[1, 0, 0]) for name in PROTOCOL["items"]}


def answer(content="```c\nint main(void){return 0;}\n```", finish="stop", cost=0.001, prompt=790, completion=3000,
           reasoning=2900, provider="AtlasCloud", model="deepseek/deepseek-v4.1-flash", usage=True):
    record = dict(id="gen-unit", model=model, provider=provider,
                  choices=[dict(index=0, finish_reason=finish, native_finish_reason=finish,
                                message=dict(role="assistant", content=content, reasoning="private reasoning text"))])
    if usage:
        record["usage"] = dict(prompt_tokens=prompt, completion_tokens=completion, total_tokens=prompt + completion, cost=cost,
                               completion_tokens_details=dict(reasoning_tokens=reasoning), prompt_tokens_details=dict(cached_tokens=0))
    return record


def ok(record=None, **fields):
    """An HTTP 200 reply with OpenRouter's leading keep-alive whitespace."""
    return 200, b"\n         \n\n" + json.dumps(record or answer(**fields)).encode()


class Server:
    """A local fake OpenRouter: one queued reply per POST, the endpoint list for GET."""

    def __init__(self, replies):
        self.replies, self.seen, self.gets, self.release = list(replies), [], [], threading.Event()
        server = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = self.rfile.read(int(self.headers["Content-Length"]))
                server.seen.append(dict(path=self.path, headers={k.lower(): v for k, v in self.headers.items()}, body=body))
                reply = server.replies.pop(0)
                if reply == "stall":
                    server.release.wait(10)
                if reply in ("drop", "stall"):
                    return  # Close the connection without a status line.
                self.answer(*reply)

            def do_GET(self):
                server.gets.append({k.lower(): v for k, v in self.headers.items()})
                self.answer(200, ENDPOINTS)

            def answer(self, status, data):
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, *args):
                pass

        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.httpd.serve_forever, kwargs=dict(poll_interval=0.02), daemon=True).start()
        base = "http://127.0.0.1:%d" % self.httpd.server_port
        self.url, self.endpoints = base + "/api/v1/chat/completions", base + "/api/v1/models/{}/endpoints"

    def close(self):
        self.release.set()
        self.httpd.shutdown()
        self.httpd.server_close()


class BudgetTests(unittest.TestCase):
    def campaign(self, replies, trials=None, cap=0.30, feedback=None, score=(True, [dict(stdout="hidden")]),
                 hashes=None, protocol=None):
        protocol = protocol or PROTOCOL
        server, sleeps = Server(replies), []
        scorer = score if callable(score) else (lambda candidate, item: score)
        try:
            with tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "out"
                with patch.object(budget, "URL", server.url), patch.object(budget, "ENDPOINTS", server.endpoints), \
                     patch.object(budget, "preflight", return_value=dict(commit="fixture", **HASHES)), \
                     patch.object(budget, "references", return_value=references()), \
                     patch.object(budget, "compiler", return_value=dict(HASHES, version="fixture")), \
                     patch.object(budget, "hashes", side_effect=hashes or (lambda: dict(HASHES))), \
                     patch.object(budget.repair, "load_key", return_value=KEY), \
                     patch.object(budget.repair, "feedback", side_effect=feedback or Feedback()), \
                     patch.object(budget.repair, "score", side_effect=scorer) as scored, \
                     patch.object(budget.time, "sleep", side_effect=sleeps.append), \
                     patch.object(budget.shutil, "disk_usage", return_value=SimpleNamespace(free=100 * budget.GIB)), \
                     patch.dict(os.environ, {"no_proxy": "*", "NO_PROXY": "*"}), \
                     patch("sys.stdout", new_callable=io.StringIO):
                    status = budget.execute(protocol, "pilot", trials or PILOT[:1], out, cap)
                receipt = json.loads((out / "receipt.json").read_text())
                files = {path.name: path.read_bytes() for path in out.iterdir()}
        finally:
            server.close()
        return SimpleNamespace(status=status, receipt=receipt, files=files, seen=server.seen, gets=server.gets,
                               sleeps=sleeps, scores=scored.call_args_list, row=receipt["runs"][0] if receipt["runs"] else None)

    def test_protocol_file_matches_the_written_proposal(self):
        text = (ROOT / PROTOCOL["source"]).read_text()
        flat = " ".join(text.replace("`", "").split())
        line = re.search(r"exact line\. The check and plan records do not change\.\n\n  ```text\n  (.*)\n  ```", text)[1]
        self.assertEqual(PROTOCOL["g1_explain_line"], line)
        self.assertEqual(json.loads(line)["code"], "E3004")
        table = re.findall(r"^\| (\d+) \| `([a-z-]+)` \| (G[01]) \| (L16|L4) \|$", text, re.M)
        self.assertEqual(PILOT, [dict(request=int(n), item=i, arm=a, limit=b) for n, i, a, b in table])
        drafts = re.findall(r"^\| `(ub-[a-z]+-overflow)` \| `([^`]+)` \| `([^`]+)` \| (.+) \|$", text, re.M)
        titles = [title if title != "Same title" else drafts[0][3] for _, _, _, title in drafts]
        self.assertEqual([(d["item"], d["span"], d["replacement"], d["title"]) for d in PROTOCOL["part_b"]["drafts"]],
                         [(i, s, r, t) for (i, s, r, _), t in zip(drafts, titles)])
        for sentence in list(PROTOCOL["stopping"].values()) + PROTOCOL["phases"]["pilot"]["go_criteria"]:
            self.assertIn(sentence.replace("`", ""), flat)
        self.assertEqual(sorted(PROTOCOL["items"]), sorted({i for _, i, _, _ in table}))
        self.assertEqual(sum(cell["per_item"] for cell in PROTOCOL["cells"]) * len(PROTOCOL["items"]), 400)
        self.assertEqual((PROTOCOL["phases"]["pilot"]["cost_cap_usd"], PROTOCOL["phases"]["measured"]["cost_cap_usd"]), (0.30, 3.50))
        self.assertEqual((PROTOCOL["limits"], PROTOCOL["temperature"], PROTOCOL["model"]),
                         ({"L16": 16384, "L4": 4096}, 0, "deepseek/deepseek-v4.1-flash"))
        self.assertIn("Model " + PROTOCOL["model"] + " through OpenRouter", flat)
        self.assertEqual(budget.repair.digest(ROOT / PROTOCOL["prompt_file"]), PROTOCOL["prompt_sha256"])
        # Registered on 2026-10-20 with the pinned provider.
        self.assertEqual(PROTOCOL["status"], "registered")

    def test_payload_pins_the_provider_and_limit_and_sends_no_reasoning(self):
        run = self.campaign([ok(), ok()], trials=[PILOT[0], PILOT[7]])
        self.assertEqual((run.status, run.receipt["stop_reason"]), (0, "completed"))
        prompt = (ROOT / PROTOCOL["prompt_file"]).read_text()
        for seen, spec, limit in zip(run.seen, (PILOT[0], PILOT[7]), (16384, 4096)):
            payload = json.loads(seen["body"])
            self.assertEqual(set(payload), {"model", "temperature", "max_tokens", "messages", "provider"})
            self.assertEqual(payload["provider"], {"order": ["atlas-cloud"], "allow_fallbacks": False})
            self.assertEqual((payload["model"], payload["temperature"], payload["max_tokens"]), (PROTOCOL["model"], 0, limit))
            self.assertEqual(payload["messages"][0], dict(role="system", content=prompt))
            source = (ROOT / PROTOCOL["items_root"] / spec["item"] / "program.c").read_text()
            head, feedback = payload["messages"][1]["content"].split("\nFeedback:\n")
            self.assertEqual(head, "Current program.c:\n```c\n" + source + "\n```")
            self.assertEqual(feedback.count("\n"), 3)
            self.assertEqual(seen["headers"]["authorization"], "Bearer " + KEY)
            self.assertNotIn(KEY.encode(), seen["body"])
        self.assertEqual([row["max_tokens"] for row in run.receipt["runs"]], [16384, 4096])
        self.assertEqual(run.receipt["endpoints"][0]["entries"], [dict(name=None, tag="atlas-cloud/fp8", quantization=None,
                                                                      max_completion_tokens=393216, pricing=None, status=None)])
        self.assertEqual(len(run.gets), 2)
        self.assertNotIn("authorization", run.gets[0])

    def test_g1_replaces_only_the_explain_record(self):
        run = self.campaign([ok(), ok()], trials=PILOT[:2])
        g0, g1 = (json.loads(seen["body"])["messages"][1]["content"].split("\nFeedback:\n")[1] for seen in run.seen)
        same = re.compile(r"agent-repair-evo-\d+")
        (check0, explain0, plan0), (check1, explain1, plan1) = (text.splitlines(True) for text in (same.sub("S", g0), same.sub("S", g1)))
        self.assertEqual((explain0, explain1), (EXPLAIN, PROTOCOL["g1_explain_line"] + "\n"))
        self.assertEqual((check0, plan0), (check1, plan1))
        self.assertEqual([row["arm"] for row in run.receipt["runs"]], ["G0", "G1"])
        self.assertEqual([row["feedback_bytes"] for row in run.receipt["runs"]], [len(g0.encode()), len(g1.encode())])
        wrong = self.campaign([ok()], trials=PILOT[1:2], feedback=Feedback(code="E3005"))
        self.assertEqual((len(wrong.seen), wrong.receipt["stop_reason"]), (0, "g1_needs_one_E3004_explain_record"))

    def test_cost_cap_is_checked_before_each_request_and_the_last_may_cross_it(self):
        run = self.campaign([ok(cost=0.2), ok(cost=0.2), ok()], trials=PILOT[:3])
        self.assertEqual((len(run.seen), run.receipt["stop_reason"], run.receipt["unattempted"]), (2, "reported_cost_cap", 1))
        self.assertAlmostEqual(run.receipt["totals"]["cost_usd"], 0.4)
        self.assertEqual(run.status, 1)

    def test_unknown_cost_reserve_counts_against_the_cap_before_a_retry(self):
        run = self.campaign([(503, b'{"error":{"code":503}}')], cap=0.03)
        self.assertEqual((len(run.seen), run.sleeps, run.receipt["stop_reason"]), (1, [2], "reported_cost_cap"))
        self.assertEqual((run.row["status"], run.row["attempts"][0]["cost"]), ("stopped", None))
        totals = run.receipt["totals"]
        self.assertEqual((totals["cost_usd"], totals["unknown_cost_attempts"]), (0.0, 1))
        self.assertAlmostEqual(totals["unknown_cost_reserve_usd"], (1000 * 0.45 + 16384 * 2.40) / 1e6)

    def test_missing_or_invalid_usage_or_cost_stops_without_retry(self):
        def changed(edit):
            record = answer()
            edit(record["usage"])
            return ok(record)
        cases = [(ok(usage=False), "cost_unknown:missing_or_invalid_usage", 0.0),
                 (changed(lambda u: u.pop("cost")), "cost_unknown:missing_or_invalid_usage", 0.0),
                 (changed(lambda u: u.update(cost=-0.1)), "cost_unknown:missing_or_invalid_usage", 0.0),
                 (changed(lambda u: u.update(cost=True)), "cost_unknown:missing_or_invalid_usage", 0.0),
                 (changed(lambda u: u.update(cost=float("nan"))), "cost_unknown:missing_or_invalid_usage", 0.0),
                 (changed(lambda u: u.pop("completion_tokens_details")), "invalid_usage_tokens", 0.001),
                 (changed(lambda u: u.update(completion_tokens=10, completion_tokens_details=dict(reasoning_tokens=11))), "invalid_usage_tokens", 0.001),
                 (changed(lambda u: u.update(prompt_tokens="790")), "invalid_usage_tokens", 0.001)]
        for reply, reason, cost in cases:
            with self.subTest(reason=reason, body=reply[1][-160:]):
                run = self.campaign([reply, ok()], trials=PILOT[:2])
                self.assertEqual((len(run.seen), run.sleeps, run.receipt["stop_reason"]), (1, [], reason))
                self.assertEqual((run.row["status"], run.receipt["totals"]["cost_usd"]), ("stopped", cost))
                self.assertEqual(run.scores, [])

    def test_refusals_stop_without_retry(self):
        for reply, reason in [((401, b'{"error":{"code":401}}'), "provider_refused:http_401"),
                              ((402, b'{"error":{"code":402}}'), "provider_refused:http_402"),
                              ((403, b""), "provider_refused:http_403"),
                              ((200, b'\n  {"error":{"code":403,"message":"moderation"}}'), "provider_refused:embedded_403")]:
            with self.subTest(reason=reason):
                run = self.campaign([reply, ok()], trials=PILOT[:2])
                self.assertEqual((len(run.seen), run.sleeps, run.receipt["stop_reason"]), (1, [], reason))

    def test_unreadable_or_unexpected_replies_stop_without_retry(self):
        small = copy.deepcopy(PROTOCOL)
        small["harness"]["body_limit_bytes"] = 64
        for reply, reason, protocol in [((200, b"\n  not json"), "cost_unknown:invalid_json", PROTOCOL),
                                        ((400, b'{"error":{"code":400}}'), "http_400", PROTOCOL),
                                        ((404, b"no endpoint"), "http_404", PROTOCOL),
                                        ((200, b'{"error":{"code":404,"message":"no endpoint"}}'), "embedded_404", PROTOCOL),
                                        ((200, b" " * 65), "cost_unknown:response_limit", small)]:
            with self.subTest(reason=reason):
                run = self.campaign([reply, ok()], trials=PILOT[:2], protocol=protocol)
                self.assertEqual((len(run.seen), run.sleeps, run.receipt["stop_reason"]), (1, [], reason))
                self.assertEqual(run.receipt["totals"]["unknown_cost_attempts"], 1)

    def test_another_provider_or_model_stops_as_a_deviation(self):
        for fields, deviation in [(dict(provider="Alibaba"), "provider"), (dict(provider=None), "provider"),
                                  (dict(model="deepseek/deepseek-v4.1-flash:batch"), "model")]:
            with self.subTest(deviation=deviation, fields=fields):
                run = self.campaign([ok(**fields), ok()], trials=PILOT[:2])
                self.assertEqual((len(run.seen), run.receipt["stop_reason"]), (1, "deviation_" + deviation))
                self.assertEqual((run.row["status"], run.row["deviation"], run.row["correct"]), ("completed", deviation, True))
                self.assertFalse(run.receipt["go"]["held"])
        run = self.campaign([ok(model="deepseek/deepseek-v4.1-flash-20260910")])
        self.assertEqual((run.receipt["stop_reason"], run.row["deviation"]), ("completed", None))

    def test_retryable_failures_retry_the_identical_payload_once_after_two_seconds(self):
        for reply, reason in [((429, b'{"error":{"code":429}}'), "http_429"), ((503, b"busy"), "http_503"),
                              ((500, b""), "http_500"), ("drop", "transport_error"),
                              ((200, b'\n   {"error":{"code":502,"message":"upstream"}}'), "embedded_502")]:
            with self.subTest(reason=reason):
                run = self.campaign([reply, ok(cost=0.002)])
                self.assertEqual((len(run.seen), run.sleeps), (2, [2]))
                self.assertEqual(run.seen[0]["body"], run.seen[1]["body"])
                self.assertEqual([call["reason"] for call in run.row["attempts"]], [reason, None])
                self.assertEqual([call["cost"] for call in run.row["attempts"]], [None, 0.002])
                self.assertEqual((run.row["status"], run.receipt["stop_reason"], run.row["cost"]), ("completed", "completed", 0.002))
                totals = run.receipt["totals"]
                self.assertEqual((totals["requests"], totals["retries"], totals["unknown_cost_attempts"]), (2, 1, 1))
        run = self.campaign([(503, b'{"error":{"code":503},"usage":{"cost":0.0005,"prompt_tokens":790,"completion_tokens":0}}'), ok()])
        self.assertEqual([call["cost"] for call in run.row["attempts"]], [0.0005, 0.001])
        self.assertEqual(run.receipt["totals"]["unknown_cost_attempts"], 0)

    def test_a_failed_retry_is_a_request_error_and_stops_the_run(self):
        run = self.campaign([(503, b""), "drop", ok()], trials=PILOT[:2])
        self.assertEqual((run.row["status"], run.row["error"], len(run.seen)), ("request_error", "transport_error", 2))
        self.assertEqual((run.receipt["stop_reason"], run.receipt["unattempted"]), ("retry_failed:transport_error", 1))
        self.assertEqual((run.row["delivered"], run.row["correct"], run.scores), (False, False, []))

    def test_the_total_deadline_is_a_transport_error(self):
        protocol = copy.deepcopy(PROTOCOL)
        protocol["harness"]["deadline_seconds"] = 0.3
        run = self.campaign(["stall", ok()], protocol=protocol)
        first = run.row["attempts"][0]
        self.assertEqual(first["transport_error"], dict(kind="DeadlineExceeded", cause=None, errno=None))
        self.assertLess(first["seconds"], 2)
        self.assertEqual((run.row["status"], len(run.seen), run.sleeps), ("completed", 2, [2]))

    def test_outcomes_are_recorded_and_never_retried(self):
        cases = [(dict(finish="length", content="", completion=16384, reasoning=16384),
                  dict(delivered=False, valid=False, correct=False, undelivered="finish_length", long=True, answer_tokens=0)),
                 (dict(content="```c\n#include <limits.h>\nint main(void){return 0;}\n```"),
                  dict(delivered=True, valid=False, correct=False, invalid="invalid_candidate:preprocessor_or_size")),
                 (dict(content="The repair is to clamp the sum."), dict(delivered=False, undelivered="invalid_candidate:fenced_file")),
                 (dict(content="```c\nint a;\n```\n```c\nint b;\n```"), dict(delivered=False, undelivered="invalid_candidate:fenced_file")),
                 (dict(content=None), dict(delivered=False, undelivered="malformed_response"))]
        for fields, expected in cases:
            with self.subTest(expected=expected):
                run = self.campaign([ok(**fields), ok()], trials=PILOT[:2])
                self.assertEqual((len(run.seen), len(run.row["attempts"]), run.receipt["stop_reason"]), (2, 1, "completed"))
                self.assertEqual({key: run.row[key] for key in expected}, expected)
                self.assertEqual(len(run.scores), 1)  # Only the second, valid trial is scored.
        run = self.campaign([ok(), ok()], trials=PILOT[:2], score=(False, [dict(stdout="wrong")]))
        self.assertEqual([(row["valid"], row["correct"]) for row in run.receipt["runs"]], [(True, False), (True, False)])
        self.assertEqual((len(run.seen), len(run.scores)), (2, 2))
        self.assertEqual(run.scores[0].args[0], "int main(void){return 0;}\n")

    def test_a_scorer_failure_leaves_correct_unknown(self):
        def score(candidate, item):
            raise budget.Failure("infrastructure_timeout", [dict(argv=["container"], exit_code=None)])
        run = self.campaign([ok(), ok()], trials=PILOT[:2], score=score)
        self.assertEqual([(row["correct"], row["score_error"]) for row in run.receipt["runs"]], [(None, "infrastructure_timeout")] * 2)
        self.assertFalse(run.receipt["go"]["criteria"][3]["held"])

    def test_outcome_fields_and_the_raw_response_are_kept(self):
        replies = [ok(completion=5000, reasoning=4500), ok(completion=5000, reasoning=4500)]
        run = self.campaign(replies, trials=[PILOT[0], PILOT[6]])
        l16, l4 = run.receipt["runs"]
        self.assertEqual((l16["long"], l4["long"], l16["answer_tokens"]), (True, None, 500))
        self.assertEqual((l16["prompt_tokens"], l16["completion_tokens"], l16["reasoning_tokens"], l16["cached_tokens"]), (790, 5000, 4500, 0))
        self.assertEqual((l16["provider"], l16["model"], l16["finish_reason"], l16["cost"]), ("AtlasCloud", PROTOCOL["model"], "stop", 0.001))
        self.assertEqual((l16["delivered"], l16["valid"], l16["correct"]), (True, True, True))
        raw = run.files["001-a1.body"]
        self.assertEqual(raw, replies[0][1])
        self.assertEqual(l16["response_sha256"], hashlib.sha256(raw).hexdigest())
        self.assertIn(b"private reasoning text", raw)
        capture = json.loads(gzip.decompress(run.files["001.json.gz"]))
        self.assertEqual((capture["row"]["request"], capture["candidate"]), (1, "int main(void){return 0;}\n"))
        self.assertEqual(capture["request"], json.loads(run.seen[0]["body"]))
        self.assertEqual(l16["payload_sha256"], hashlib.sha256(run.seen[0]["body"]).hexdigest())
        for name, data in run.files.items():
            self.assertNotIn(KEY.encode(), gzip.decompress(data) if name.endswith(".gz") else data)

    def test_a_reply_with_a_credential_is_not_kept(self):
        for value in ("sk-or-v1-leak", KEY):
            with self.subTest(value=value):
                run = self.campaign([ok(provider=value), ok()], trials=PILOT[:2])
                self.assertEqual((len(run.seen), run.receipt["stop_reason"]), (1, "secret_capture_refused"))
                self.assertNotIn("001-a1.body", run.files)
                for name, data in run.files.items():
                    raw = gzip.decompress(data) if name.endswith(".gz") else data
                    self.assertNotIn(b"sk-or-", raw)
                    self.assertNotIn(KEY.encode(), raw)

    def test_changed_feedback_or_compiler_stops_before_the_request(self):
        run = self.campaign([ok()], feedback=Feedback(check='{"exit_code":1,"changed":"%s/program.c"}\n'))
        self.assertEqual((len(run.seen), run.receipt["stop_reason"], run.row["attempts"]), (0, "frozen_feedback_changed", []))
        states = iter([dict(HASHES), dict(HASHES, evo_sha256="rebuilt")])
        run = self.campaign([ok(), ok()], trials=PILOT[:2], hashes=lambda: next(states))
        self.assertEqual((len(run.seen), run.receipt["stop_reason"]), (1, "compiler_changed"))

    def test_a_clean_pilot_meets_every_go_criterion(self):
        replies = [ok(completion=5000, reasoning=4000) if spec["limit"] == "L16" else ok() for spec in PILOT]
        run = self.campaign(replies, trials=PILOT)
        self.assertEqual((run.status, run.receipt["stop_reason"], len(run.seen)), (0, "completed", 10))
        self.assertEqual([criterion["held"] for criterion in run.receipt["go"]["criteria"]], [True] * 5, run.receipt["go"])
        self.assertTrue(run.receipt["go"]["held"])
        self.assertEqual(run.receipt["totals"]["cells"]["G0-L16"], dict(trials=3, responses=3, delivered=3, valid=3, correct=3, long=3, length=0))
        clamped = self.campaign([ok(finish="length", completion=4096, reasoning=4096, content="")])
        self.assertFalse(clamped.receipt["go"]["criteria"][2]["held"])
        expensive = self.campaign([ok(cost=0.31)])
        self.assertFalse(expensive.receipt["go"]["criteria"][4]["held"])

    def test_measured_order_is_one_seeded_permutation_of_400_trials(self):
        first, again, other = (budget.order(PROTOCOL, seed) for seed in ("0123456789abcdef", "0123456789abcdef", "fedcba9876543210"))
        self.assertEqual(first, again)
        self.assertNotEqual(first, other)
        self.assertEqual([spec["request"] for spec in first], list(range(1, 401)))
        identity = [(s["item"], s["arm"], s["limit"], s["repetition"]) for s in first]
        self.assertEqual(sorted(identity), sorted((s["item"], s["arm"], s["limit"], s["repetition"]) for s in other))
        self.assertEqual(len(set(identity)), 400)
        cells = Counter((item, arm, limit) for item, arm, limit, _ in identity)
        self.assertEqual(set(cells.values()), {30, 10})
        self.assertEqual({key[1:] for key, count in cells.items() if count == 30}, {("G0", "L16"), ("G1", "L16")})
        for seed in ("0123", "0123456789ABCDEF", None, "g123456789abcdef"):
            with self.subTest(seed=seed), self.assertRaisesRegex(budget.Failure, "invalid_seed"):
                budget.order(PROTOCOL, seed)

    def test_the_measured_phase_refuses_an_unregistered_protocol_or_a_changed_order(self):
        seed = "0123456789abcdef"
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "order.json"
            path.write_text(json.dumps(dict(seed=seed, trials=budget.order(PROTOCOL, seed))))
            with patch.object(budget.repair, "load_key", side_effect=AssertionError("key")), \
                 patch.object(budget, "send", side_effect=AssertionError("network")):
                unregistered = Path(tmp) / "protocol.json"
                unregistered.write_text(json.dumps(dict(copy.deepcopy(PROTOCOL), status="unregistered")))
                with self.assertRaisesRegex(budget.Failure, "protocol_not_registered"):
                    budget.main(["--protocol", str(unregistered), "run", "--phase", "measured", "--order", str(path),
                                 "--out", str(Path(tmp) / "out")])
                registered = dict(copy.deepcopy(PROTOCOL), status="registered")
                plan = json.loads(path.read_text())
                plan["trials"][0], plan["trials"][1] = plan["trials"][1], plan["trials"][0]
                path.write_text(json.dumps(plan))
                with self.assertRaisesRegex(budget.Failure, "measured_order_mismatch"):
                    budget.registered(registered, path)
                with patch("sys.stderr", new_callable=io.StringIO), self.assertRaises(SystemExit):
                    budget.main(["run", "--phase", "pilot", "--order", str(path), "--out", str(Path(tmp) / "out")])
            self.assertFalse((Path(tmp) / "out").exists())

    def test_output_inside_the_repository_must_be_ignored(self):
        with self.assertRaisesRegex(budget.Failure, "output_must_be_ignored"):
            budget.fresh(ROOT / "tools/experiment/budget-output-that-git-tracks")
        # fresh() resolves the path; a worktree can link .evidence to another checkout.
        self.assertEqual(budget.fresh(ROOT / ".evidence/budget-unit-never-created"),
                         (ROOT / ".evidence/budget-unit-never-created").resolve())
        with self.assertRaisesRegex(budget.Failure, "output_directory_already_exists"):
            budget.fresh(HERE)

    def test_compare_g0_reports_identity_and_the_first_difference(self):
        for check, status in ((CHECK, 0), ('{"exit_code":1,"diagnostics":[{"code":"E3005","file":"%s/program.c"}]}\n', 1)):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as tmp:
                out = Path(tmp) / "out"
                with patch.object(budget, "references", return_value=references()), patch.object(budget, "compiler", return_value={}), \
                     patch.object(budget.repair, "feedback", side_effect=Feedback(check)), patch("sys.stdout", new_callable=io.StringIO):
                    self.assertEqual(budget.compare_g0(PROTOCOL, out), status)
                report = json.loads((out / "compare-g0.json").read_text())
                self.assertEqual(report["equal"], status == 0)
                row = report["items"][0]
                self.assertEqual(row["g1_bytes"], len((check + PROTOCOL["g1_explain_line"] + "\n" + PLAN).replace("%s", row["scratch"])))
                self.assertEqual(row["explain_records"], [["E3004", "--json"]])
                if status:
                    self.assertEqual(row["first_difference"]["offset"], CHECK.index("E3004") + len("E300"))
                else:
                    self.assertNotIn("first_difference", row)
                self.assertEqual((out / (row["item"] + ".g1.txt")).stat().st_size, row["g1_bytes"])

    def test_score_drafts_applies_each_replacement_at_its_span(self):
        seen = []

        def score(candidate, item):
            seen.append((item.name, candidate))
            return True, [dict(stdout="hidden")]
        with tempfile.TemporaryDirectory() as tmp, patch.object(budget, "sealed", return_value="sha256:fixture"), \
             patch.object(budget.repair, "score", side_effect=score), patch("sys.stdout", new_callable=io.StringIO):
            self.assertEqual(budget.score_drafts(PROTOCOL, Path(tmp) / "out"), 0)
        self.assertEqual([name for name, _ in seen], [draft["item"] for draft in PROTOCOL["part_b"]["drafts"]])
        for (name, candidate), draft in zip(seen, PROTOCOL["part_b"]["drafts"]):
            original = (ROOT / PROTOCOL["items_root"] / name / "program.c").read_text().splitlines()
            lines = candidate.splitlines()
            self.assertEqual(lines[5], "    return " + draft["replacement"] + ";")
            self.assertEqual(lines[:5] + lines[6:], original[:5] + original[6:])


if __name__ == "__main__":
    unittest.main()
