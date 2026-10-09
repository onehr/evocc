"""Quota-free tests for the third experiment; a fake `kimi` script stands in for the Kimi Code CLI."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tarfile
import tempfile
import time
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("agent_repair", Path(__file__).parents[1] / "agent_repair.py")
agent = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(agent)
SOURCE = "int main(void)\n{\n    return 1;\n}\n"
FIXED = "int main(void)\n{\n    return 0;\n}\n"

# The fake writes a Kimi-like session log under $HOME/.kimi-code, prints stream-json and records what it saw.
FAKE = r'''#!{python}
import hashlib, json, os, shutil, subprocess, sys, time, uuid
MODE, NOTES, FIXED = {mode!r}, {notes!r}, {fixed!r}
argv, cwd, path = sys.argv[1:], os.getcwd(), os.environ.get("PATH", "")
note = dict(argv=argv, cwd=cwd, env=dict(os.environ), listing=sorted(os.listdir(cwd)), pid=os.getpid(),
            found={{tool: shutil.which(tool, path=path) for tool in ("gcc", "cc", "clang", "python3", "check", "sed")}})
key = "wd_%s_%s" % (os.path.basename(cwd).lower(), hashlib.sha256(cwd.encode()).hexdigest()[:12])
wire = os.path.join(os.environ["HOME"], ".kimi-code/sessions", key, "session_" + str(uuid.uuid4()), "agents/main")
os.makedirs(wire)
def loop(event): return dict(type="context.append_loop_event", agentId="main", event=event)
def bash(command): return loop(dict(type="tool.call", name="Bash", args=dict(command=command)))
request = dict(type="llm.request", kind="loop", modelAlias=argv[argv.index("-m") + 1], thinkingEffort=os.environ.get("KIMI_MODEL_THINKING_EFFORT"))
usage = dict(type="usage.record", usage=dict(inputOther=100, output=20, inputCacheRead=300, inputCacheCreation=5), usageScope="turn")
records = [dict(type="metadata", protocol_version="1.5"), dict(type="profile.bind", agentsMdPaths=[], thinkingEffort="max"), request, usage]
if MODE in ("fix", "deviate", "tamper"):
    note["check"] = subprocess.run(["check", "program.c"], capture_output=True, text=True).stdout
    with open("program.c", "w") as handle: handle.write(FIXED)
    records += [bash("check program.c"), loop(dict(type="tool.call", name="Read", args=dict(path="program.c"))),
                loop(dict(type="tool.call", name="Write", args=dict(path=os.path.join(cwd, "program.c"), content=FIXED))),
                request, usage, bash("check program.c 2>/dev/null | head -5"), dict(type="turn.ended", reason="completed")]
if MODE == "deviate":
    records += [bash("cat /etc/hosts"), bash("gcc -o prog program.c"), bash("ls .."), bash("sh -c 'clang -v'"),
                loop(dict(type="tool.call", name="Read", args=dict(path="/Users/someone/fixed.c"))),
                loop(dict(type="tool.call", name="WebSearch", args=dict(query="cJSON merge patch")))]
if MODE == "tamper":
    with open("check", "a") as handle: handle.write("echo tampered\n")
with open(os.path.join(wire, "wire.jsonl"), "w") as handle: handle.write("".join(json.dumps(r) + "\n" for r in records))
with open(NOTES, "w") as handle: json.dump(note, handle)
print(json.dumps(dict(role="meta", type="system.version", version="2.0.2")), flush=True)
if MODE == "quota":
    print(json.dumps(dict(role="meta", type="turn.step.retrying", status_code=429, error_name="APIProviderRateLimitError",
                          error_message="429 Rate limit reached")), flush=True)
if MODE in ("sleep", "quota"): time.sleep(60)
if MODE == "auth":
    sys.stderr.write("error: failed to run prompt: provider.auth_error: 401 Invalid Authentication\n")
    sys.exit(1)
print(json.dumps(dict(role="assistant", content="done")))
'''


class AgentRepairTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="agent-repair-test-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.home, self.items = self.tmp / "home", self.tmp / "items"
        for name in ("a", "b", "c"):
            item = self.items / name
            item.mkdir(parents=True)
            for file, text in {"program.c": SOURCE, "fixed.c": FIXED, "reference_main.c": "HIDDEN_REFERENCE",
                               "expected.txt": "HIDDEN_EXPECTED", "fix-spans.json": '{"spans": [[3, 3]]}', "class": "logic\n"}.items():
                (item / file).write_text(text)
        self.scored = []

    def fake(self, mode):
        path = self.tmp / ("kimi-" + mode)
        path.write_text(FAKE.format(python=sys.executable, mode=mode, notes=str(self.tmp / "notes.json"), fixed=FIXED))
        path.chmod(0o755)
        return path

    def score(self, source, item):
        self.scored.append((source, item.name))
        return source == FIXED, [dict(exit_code=0, stdout="")] * 3

    def campaign(self, mode, items=("a",), conditions=("c0",), extra=()):
        argv = ["run", "--items", str(self.items), "--out", str(self.tmp / "out"), "--workspaces", str(self.tmp / "exp3"),
                "--kimi", str(self.fake(mode)), "--wall", "20", *extra]
        argv += [word for name in items for word in ("--item", name)] + [word for c in conditions for word in ("--condition", c)]
        with patch.dict(os.environ, {"HOME": str(self.home), "AGENT_REPAIR_TEST_SECRET": "must-not-leak"}), \
             patch.object(agent, "provenance", return_value={"commit": "test"}), \
             patch.object(agent.repair, "score", side_effect=self.score), patch("sys.stdout", new_callable=io.StringIO):
            code = agent.main(argv)
        receipt = json.loads((self.tmp / "out/receipt.json").read_text())
        notes = json.loads((self.tmp / "notes.json").read_text()) if (self.tmp / "notes.json").exists() else None
        return code, receipt, notes

    def test_dry_run_rotates_conditions_and_starts_nothing(self):
        argv = ["run", "--items", str(self.items), "--item", "a", "--item", "b", "--item", "c", "--out", str(self.tmp / "out"),
                "--workspaces", str(self.tmp / "exp3"), "--kimi", str(self.tmp / "absent"), "--dry-run"]
        with patch("sys.stdout", new_callable=io.StringIO) as output:
            self.assertEqual(agent.main(argv), 0)
        order = [(row["item"], row["condition"]) for row in json.loads(output.getvalue())["order"]]
        self.assertEqual(order, [("a", "c0"), ("a", "c1"), ("a", "c3"), ("b", "c1"), ("b", "c3"), ("b", "c0"),
                                 ("c", "c3"), ("c", "c0"), ("c", "c1")])
        self.assertFalse((self.tmp / "out").exists() or (self.tmp / "exp3").exists())

    def test_plan_above_session_limit_or_inside_repository_is_refused(self):
        base = ["run", "--items", str(self.items), "--item", "a", "--item", "b", "--item", "c", "--kimi", str(self.tmp / "absent")]
        with self.assertRaisesRegex(SystemExit, "12 sessions"):
            agent.main(base + ["--item", "a", "--out", str(self.tmp / "out"), "--workspaces", str(self.tmp / "exp3")])
        with self.assertRaisesRegex(SystemExit, "outside the repository"):
            agent.main(base + ["--out", str(agent.ROOT / "out-test"), "--workspaces", str(self.tmp / "exp3")])
        self.assertFalse((self.tmp / "out").exists() or (agent.ROOT / "out-test").exists())

    def test_session_isolation_path_restriction_logs_and_scoring(self):
        code, receipt, notes = self.campaign("fix")
        row, out = receipt["runs"][0], self.tmp / "out"
        work, sdir = Path(notes["cwd"]), out / "sessions/01"
        self.assertEqual((code, receipt["stop_reason"], row["status"], row["exit_status"]), (0, "completed", "completed", 0))
        self.assertEqual(notes["listing"], ["check", "program.c"])
        self.assertEqual(work.parent, self.tmp / "exp3")
        self.assertFalse(work.is_relative_to(agent.ROOT))
        self.assertNotIn(str(agent.ROOT), (sdir / "workspace/check").read_text())
        env = notes["env"]
        self.assertEqual((env["PATH"], env["SHELL"], env["HOME"]), (str(sdir / "bin"), str(out / "env/login-shell"), str(self.home)))
        self.assertEqual((env["KIMI_MODEL_THINKING_EFFORT"], env["KIMI_LOOP_MAX_STEPS_PER_TURN"]), ("low", "100"))
        self.assertNotIn("AGENT_REPAIR_TEST_SECRET", env)
        self.assertEqual({k: v for k, v in notes["found"].items() if v}, {"check": str(sdir / "bin/check"), "sed": str(sdir / "bin/sed")})
        self.assertEqual(notes["check"], agent.C0_TEXT)
        expected_tools = {t for t in agent.TOOLS + agent.EXTRA_TOOLS if shutil.which(t, path="/bin:/usr/bin:/usr/local/bin:/opt/homebrew/bin")}
        self.assertEqual(set(os.listdir(sdir / "bin")), expected_tools | {"check"})
        argv = notes["argv"]
        self.assertEqual(argv[:2], ["-p", (agent.HERE / "prompt-3.txt").read_text().strip()])
        self.assertEqual(argv[argv.index("-m") + 1], "kimi-code/k3")
        self.assertEqual(argv[argv.index("--output-format") + 1], "stream-json")
        self.assertNotIn("--auto", argv)
        self.assertEqual(os.listdir(argv[argv.index("--skills-dir") + 1]), [])
        self.assertIn("disallowedTools: WebSearch, FetchURL, Agent, AgentSwarm", Path(argv[argv.index("--agent-file") + 1]).read_text())
        self.assertEqual((row["steps"], row["efforts_seen"], row["models_seen"]), (2, ["low"], ["kimi-code/k3"]))
        self.assertEqual(row["tokens"], dict(inputOther=200, output=40, inputCacheRead=600, inputCacheCreation=10, total=850))
        self.assertEqual(row["tool_calls"], {"Bash": 2, "Read": 1, "Write": 1})
        self.assertEqual((row["check_runs"], row["check_invocations"], row["deviations"]), (2, 1, []))
        self.assertEqual(row["bash_commands"], ["check program.c", "check program.c 2>/dev/null | head -5"])
        self.assertEqual((row["passed"], row["diff_size"], row["span_hits"], row["all_spans"]), (True, 2, [True], True))
        self.assertEqual(self.scored, [(FIXED, "a")])
        self.assertEqual(os.listdir(self.tmp / "exp3"), [])
        self.assertEqual((sdir / "workspace/program.c").read_text(), FIXED)
        with tarfile.open(out / "wire.tar.gz") as archive:
            self.assertEqual(len([m for m in archive.getnames() if m.endswith("main.jsonl")]), 1)
        self.assertEqual((receipt["totals"]["passed"], receipt["totals"]["tokens"]["total"]), (1, 850))

    def test_timeout_kills_the_session_and_records_it(self):
        start = time.monotonic()
        code, receipt, notes = self.campaign("sleep", extra=("--wall", "1"))
        self.assertLess(time.monotonic() - start, 30)
        self.assertEqual((receipt["runs"][0]["status"], receipt["stop_reason"], code), ("timeout", "completed", 0))
        with self.assertRaises(ProcessLookupError):
            os.kill(notes["pid"], 0)
        self.assertEqual(self.scored, [(SOURCE, "a")])

    def test_rate_limit_notice_stops_at_once_and_starts_no_further_session(self):
        start = time.monotonic()
        code, receipt, notes = self.campaign("quota", items=("a", "b"))
        self.assertLess(time.monotonic() - start, 30)
        self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"]), receipt["unattempted"]), (1, "provider_refusal", 1, 1))
        self.assertEqual(receipt["runs"][0]["status"], "stopped")
        self.assertIn("429", receipt["runs"][0]["stop"])

    def test_authentication_error_after_exit_stops_the_run(self):
        code, receipt, _ = self.campaign("auth", items=("a", "b"))
        row = receipt["runs"][0]
        self.assertEqual((code, row["status"], row["exit_status"], len(receipt["runs"])), (1, "stopped", 1, 1))
        self.assertIn("provider.auth_error", row["stop"])

    def test_deviations_and_modified_check_are_recorded(self):
        _, receipt, _ = self.campaign("deviate")
        kinds = sorted((d["kind"], d["detail"]) for d in receipt["runs"][0]["deviations"])
        self.assertEqual(kinds, [("compiler", "gcc -o prog program.c"), ("compiler", "sh -c 'clang -v'"), ("path", ".."),
                                 ("path", "/Users/someone/fixed.c"), ("path", "/etc/hosts"), ("web", '{"query": "cJSON merge patch"}')])
        _, receipt, _ = self.campaign_again("tamper")
        self.assertEqual(receipt["runs"][0]["deviations"], [dict(kind="check_modified", tool=None, detail="check")])

    def campaign_again(self, mode):
        shutil.rmtree(self.tmp / "out")
        return self.campaign(mode)

    def test_wire_parsing_handles_nested_and_top_level_records(self):
        records = [{"type": "llm.request", "kind": "loop", "modelAlias": "m", "thinkingEffort": "low"},
                   {"type": "llm.request", "kind": "compaction", "modelAlias": "m", "thinkingEffort": "low"},
                   {"type": "usage.record", "usage": {"inputOther": 7, "output": 3, "inputCacheRead": 11, "inputCacheCreation": 0}},
                   {"type": "usage.record", "usage": {"inputOther": 1, "output": 2}},
                   {"type": "context.append_loop_event", "event": {"type": "tool.call", "name": "Grep", "args": {"pattern": "x"}}},
                   {"type": "tool.call", "name": "Bash", "args": {"command": "check"}},
                   {"type": "turn.step.retrying", "errorName": "APIStatusError", "statusCode": 500},
                   {"type": "turn.ended", "reason": "failed", "error": {"code": "provider.rate_limit", "message": "429 slow down"}}]
        path = self.tmp / "wire.jsonl"
        path.write_text("".join(json.dumps(r) + "\n" for r in records) + "not json\n")
        m = agent.parse([path])
        self.assertEqual((m["steps"], m["request_kinds"], m["efforts"]), (2, {"loop": 1, "compaction": 1}, ["low"]))
        self.assertEqual(m["tokens"], dict(inputOther=8, output=5, inputCacheRead=11, inputCacheCreation=0))
        self.assertEqual((m["tool_calls"], len(m["errors"])), ({"Grep": 1, "Bash": 1}, 2))
        self.assertEqual([agent.refusal(e) for e in m["errors"]], [False, True])
        self.assertTrue(agent.refusal({"type": "turn.step.retrying", "errorName": "APIStatusError", "statusCode": 429}))
        self.assertFalse(agent.meta_stop(b'{"role": "tool", "content": "program.c:429:3: runtime error: 401 auth quota"}'))
        self.assertFalse(agent.STOP.search("error: failed to run prompt: provider.api_error: 500 upstream 429 items"))
        self.assertTrue(agent.STOP.search("error: failed to run prompt: provider.api_error: 402 Payment required"))

    def test_audit_rules_and_command_parsing(self):
        work = Path("/private/tmp/exp3/w01-abc")
        heredoc = "cat > program.c <<'EOF'\n/* ok */ int x = a / b; // fine\nreturn ~0 / 2;\nEOF\ncheck program.c"
        clean = [("Bash", {"command": heredoc}), ("Bash", {"command": "sed -n '/main/,/}/p' program.c 2>/dev/null"}),
                 ("Bash", {"command": "awk '/^static/ {print}' program.c; cp program.c ./backup.c"}),
                 ("Read", {"path": str(work / "program.c")}), ("Edit", {"path": "/tmp/exp3/w01-abc/program.c"}),
                 ("Grep", {"pattern": "/usr/x", "path": "program.c"}), ("Glob", {"pattern": "*.c"})]
        self.assertEqual(agent.audit(clean, work), [])
        dirty = [("Bash", {"command": "ls", "cwd": "/private/tmp"}), ("Bash", {"command": "cat ~/.ssh/id"}),
                 ("Bash", {"command": "x=$(command -v cc) && FOO=1 env xcrun clang -c program.c"}),
                 ("Bash", {"command": "/usr/bin/evocc check program.c"}), ("Glob", {"pattern": "/private/tmp/**"}),
                 ("Write", {"path": "../escape.c"}), ("FetchURL", {"url": "https://example.com"}), ("mcp__x__y", {})]
        kinds = [(d["kind"], d["tool"]) for d in agent.audit(dirty, work)]
        self.assertEqual(kinds, [("path", "Bash"), ("path", "Bash"), ("compiler", "Bash"), ("path", "Bash"), ("compiler", "Bash"),
                                 ("path", "Glob"), ("path", "Write"), ("web", "FetchURL"), ("web", "mcp__x__y")])
        self.assertEqual(agent.command_words("bash -c \"gcc -v; check x.c\" | tee out.txt"), ["bash", "gcc", "check", "tee"])
        self.assertEqual(agent.command_words(heredoc), ["cat", "check"])

    def test_check_conditions_and_refusals(self):
        work = self.tmp / "work"
        work.mkdir()
        (work / "program.c").write_text(SOURCE)
        log = self.tmp / "checks.jsonl"
        steps = [dict(argv=["gcc-14", "-o", "prog", "program.c"], exit_code=0, stdout="", stderr=""),
                 dict(argv=["./prog"], exit_code=1, stdout="SECRET-STDOUT", stderr="program.c:3: runtime error: x\n")]
        def run(argv, cwd=None):
            self.assertEqual(cwd, work)
            return dict(argv=argv, exit_code=0, stderr="", stdout='{"diagnostics":[{"code":"E1"}]}\n' if argv[1] == "check" else " ".join(map(str, argv[1:])) + "\n")
        cwd = os.getcwd()
        os.chdir(work)
        self.addCleanup(os.chdir, cwd)
        with patch.object(agent.repair, "native", return_value=steps) as native, patch.object(agent.repair, "run", side_effect=run), \
             patch("sys.stdout", new_callable=io.StringIO) as output, patch.object(agent.signal, "signal"):
            codes = [agent.check("c0", str(work), str(log), ["../elsewhere.c"]), agent.check("c1", str(work), str(log), []),
                     agent.check("c3", str(work), str(log), ["program.c"]), agent.check("c1", str(work), str(log), ["/etc/hosts"])]
        native.assert_called_once_with("c1", SOURCE)
        text = output.getvalue()
        self.assertEqual(codes, [0, 0, 0, 2])
        self.assertTrue(text.startswith(agent.C0_TEXT + "$ gcc-14 -o prog program.c\nexit status 0\n$ ./prog\nprogram.c:3: runtime error: x\nexit status 1\n"))
        self.assertIn('{"diagnostics":[{"code":"E1"}]}\nexplain E1 --json\nfix program.c --plan --json\n', text)
        self.assertNotIn("SECRET-STDOUT", text)
        self.assertIn("check: give a C file in the working directory", text)
        self.assertEqual(len(log.read_text().splitlines()), 4)


if __name__ == "__main__":
    unittest.main()
