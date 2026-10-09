"""Quota-free tests for the third experiment.

SandboxTests prove the Seatbelt profile with the real `sandbox-exec` and no model call. HarnessTests check
the orchestration with a fake `kimi` script and a stubbed check service, so they use no quota."""
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import types
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("agent_repair", Path(__file__).parents[1] / "agent_repair.py")
agent = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(agent)
SOURCE = "int main(void)\n{\n    return 1;\n}\n"
FIXED = "int main(void)\n{\n    return 0;\n}\n"
HAS_SANDBOX = Path("/usr/bin/sandbox-exec").exists()

# A fake kimi: it writes a Kimi-like wire log, prints stream-json, and runs each scripted Bash command for
# real so the opaque check client works. No sandbox and no model. The actions come from AGENT_SCRIPT (JSON).
FAKE = r'''#!{python}
import hashlib, json, os, subprocess, sys, time, uuid
argv, cwd = sys.argv[1:], os.getcwd()
script = json.loads(os.environ["AGENT_SCRIPT"])
json.dump(dict(cwd=cwd, env=dict(os.environ), argv=argv, pid=os.getpid()), open(os.environ["AGENT_NOTES"], "w"))
key = "wd_%s_%s" % (os.path.basename(cwd).lower(), hashlib.sha256(cwd.encode()).hexdigest()[:12])
wdir = os.path.join(os.environ["KIMI_CODE_HOME"], "sessions", key, "session_" + uuid.uuid4().hex, "agents/main")
os.makedirs(wdir)
rec = [dict(type="metadata", protocol_version="fake"),
       dict(type="llm.request", kind="loop", modelAlias=argv[argv.index("-m") + 1], thinkingEffort=os.environ.get("KIMI_MODEL_THINKING_EFFORT")),
       dict(type="usage.record", usage=dict(inputOther=100, output=20, inputCacheRead=300, inputCacheCreation=5), usageScope="turn")]
def loop(event): rec.append(dict(type="context.append_loop_event", event=event))
def emit(obj): print(json.dumps(obj), flush=True)
def flush(): open(os.path.join(wdir, "wire.jsonl"), "w").write("".join(json.dumps(x) + "\n" for x in rec))
emit(dict(role="meta", type="system.version", version="fake"))
for act in script:
    kind = act[0]
    if kind == "bash":
        cid = "c" + uuid.uuid4().hex[:8]
        loop(dict(type="tool.call", name="Bash", toolCallId=cid, args=dict(command=act[1])))
        emit(dict(role="assistant", tool_calls=[dict(type="function", id=cid, function=dict(name="Bash", arguments=json.dumps(dict(command=act[1]))))]))
        r = subprocess.run(["/bin/sh", "-c", act[1]], cwd=cwd, capture_output=True, text=True)
        out = r.stdout + r.stderr
        loop(dict(type="tool.result", toolCallId=cid, result=dict(output=out, isError=r.returncode != 0)))
        emit(dict(role="tool", tool_call_id=cid, content=out))
    elif kind == "result":  # record a tool call with a canned result, run nothing (for the audit unit)
        cid = "c" + uuid.uuid4().hex[:8]
        loop(dict(type="tool.call", name=act[1], toolCallId=cid, args=act[2]))
        loop(dict(type="tool.result", toolCallId=cid, result=dict(output=act[3], isError=act[4])))
    elif kind == "edit":
        open("program.c", "w").write(act[1])
        loop(dict(type="tool.call", name="Edit", toolCallId="e" + uuid.uuid4().hex[:8], args=dict(path="program.c")))
    elif kind == "quota":
        emit(dict(role="meta", type="turn.step.retrying", status_code=429, error_name="APIProviderRateLimitError", error_message="429 Rate limit reached"))
        flush(); time.sleep(60)
    elif kind == "auth":
        sys.stderr.write("error: failed to run prompt: provider.auth_error: 401 Invalid Authentication\n"); flush(); sys.exit(1)
    elif kind == "sleep":
        flush(); time.sleep(act[1])
    elif kind == "text":
        emit(dict(role="assistant", content=act[1]))
flush()
'''


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="agent-repair3-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.items = self.tmp / "items"
        for name in ("a", "b", "c"):
            d = self.items / name
            d.mkdir(parents=True)
            for f, v in {"program.c": SOURCE, "fixed.c": FIXED, "reference_main.c": "HIDDEN", "expected.txt": "OUT",
                         "fix-spans.json": '{"spans": [[3, 3]]}', "class": "logic\n"}.items():
                (d / f).write_text(v)

    def ns(self, **kw):
        base = dict(items=self.items, item=["a"], condition=None, reps=1, max_sessions=9, phase="pilot",
                    out=self.tmp / "out", kimi=self.tmp / "kimi", model="kimi-code/k3", effort="low", wall=20,
                    max_steps=100, prompt=agent.HERE / "prompt-3.txt", quota_port=None, max_week_ratio=0.6,
                    max_wait=5, dry_run=False)
        base.update(kw)
        args = types.SimpleNamespace(**base)
        args.wroot = (self.tmp / "wroot").resolve(); args.base = args.wroot / "env"
        (args.base / "skills").mkdir(parents=True, exist_ok=True)
        (args.base / "agent.md").write_text(agent.AGENT_FILE)
        agent.executable(args.base / "login-shell", agent.LOGIN_SHELL)
        return args


class SandboxTests(Base):
    """The real Seatbelt profile. These need macOS sandbox-exec; no model runs."""

    def build_profile(self):
        args = self.ns()
        out = self.tmp / "out"
        (out / "env").mkdir(parents=True)
        other = args.wroot / "02/tmp"
        other.mkdir(parents=True)
        (other / "secret.txt").write_text("SIBLING_SECRET")
        w = agent.workspace(args, out, 1, self.items / "a")
        return w, out, other

    @unittest.skipUnless(HAS_SANDBOX, "needs macOS sandbox-exec")
    def test_forbidden_actions_are_denied_and_allowed_ones_work(self):
        w, out, other = self.build_profile()
        prof = str(w["sdir"] / "profile.sb")
        def run(cmd):
            return subprocess.run(["/usr/bin/sandbox-exec", "-f", prof, "/bin/sh", "-c", cmd],
                                  cwd=str(w["ws"]), capture_output=True, text=True, env={"PATH": str(w["bin"]), "HOME": str(w["home"])})
        escape = Path("/tmp") / ("agent-repair-escape-%d.txt" % os.getpid())
        self.addCleanup(lambda: escape.unlink(missing_ok=True))
        forbidden = {
            "cc_abspath": "/usr/bin/cc --version",
            "clang_abspath": "/usr/bin/clang --version",
            "python3_abspath": "/usr/bin/python3 -c 'print(1)'",
            "perl_abspath": "/usr/bin/perl -e 'print 1'",
            "osascript": "/usr/bin/osascript -e 'return 1'",
            "curl": "/usr/bin/curl --version",
            "read_bashrc": "cat /Users/onehrxn/.bashrc",
            "read_repo": "cat %s/tools/experiment/repair.py" % agent.ROOT,
            "read_etc_hosts": "cat /etc/hosts",
            "read_sibling": "cat %s/secret.txt" % other,
            "write_outside": "echo x > %s" % escape,
        }
        for name, cmd in forbidden.items():
            with self.subTest(forbidden=name):
                r = run(cmd)
                self.assertNotEqual(r.returncode, 0, (name, r.stdout, r.stderr))
                self.assertNotIn("SIBLING_SECRET", r.stdout)
                self.assertNotIn("OPENROUTER", r.stdout + r.stderr)
        # gcc by a Homebrew absolute path, if present
        brew = shutil.which("gcc-16", path="/opt/homebrew/bin:/usr/local/bin")
        if brew:
            self.assertNotEqual(run("%s --version" % brew).returncode, 0)
        self.assertFalse(escape.exists(), "a write escaped the workspace")
        # allowed: write inside the workspace, read program.c, run a basic utility
        self.assertEqual(run("echo hi > note.txt && cat note.txt").stdout.strip(), "hi")
        self.assertTrue((w["ws"] / "note.txt").is_file())
        self.assertIn("int main", run("cat program.c").stdout)
        self.assertEqual(run("/bin/echo ok").stdout.strip(), "ok")

    @unittest.skipUnless(HAS_SANDBOX, "needs macOS sandbox-exec")
    def test_profile_exec_allowlist_lists_no_compiler(self):
        w, _, _ = self.build_profile()
        text = (w["sdir"] / "profile.sb").read_text()
        allow = text.split("process-exec*", 2)[2].split(")", 1)[0] if "process-exec*" in text else ""
        for bad in ("/usr/bin/cc", "/usr/bin/clang", "/usr/bin/python3", "/usr/bin/curl", "gcc"):
            self.assertNotIn('"%s"' % bad, allow)
        self.assertIn("(deny file-read-data", text)
        self.assertIn(str(agent.ROOT), text)


class HarnessTests(Base):
    def fake_kimi(self):
        path = self.tmp / "kimi"
        path.write_text(FAKE.format(python=sys.executable))
        path.chmod(0o755)
        return path

    def patched_popen(self, script):
        """Run the harness but drop the sandbox-exec prefix, point the Kimi home at a temp, and feed the
        agent script to the fake kimi through the environment."""
        khome = self.tmp / "khome"
        khome.mkdir()
        real_popen = agent.subprocess.Popen
        def popen(argv, **kw):
            if argv and argv[0].endswith("sandbox-exec"):
                argv = argv[3:]  # drop ['/usr/bin/sandbox-exec', '-f', '<profile>']
            kw.setdefault("env", {})
            kw["env"] = dict(kw["env"], AGENT_SCRIPT=json.dumps(script), AGENT_NOTES=str(self.tmp / "notes.json"))
            return real_popen(argv, **kw)
        return patch.object(agent, "KHOME", khome), patch.object(agent.subprocess, "Popen", side_effect=popen)

    def score(self, source, item):
        return source == FIXED, [dict(exit_code=0, stdout="")] * 3

    def session(self, script, condition="c0", feedback="=== build exit 0\n", wall=20):
        args = self.ns(kimi=self.fake_kimi(), wall=wall)
        args.prompt_text = "do it"
        khome_patch, popen_patch = self.patched_popen(script)
        with khome_patch, popen_patch, patch.object(agent, "feedback", return_value=feedback), \
             patch.object(agent.repair, "score", side_effect=self.score):
            row = agent.session(args, args.out, 1, self.items / "a", condition)
        return row

    def test_check_roundtrip_edit_scratch_and_pass(self):
        script = [["bash", "check program.c"], ["edit", FIXED], ["bash", "echo tmp-scratch > \"$TMPDIR/w.txt\""],
                  ["bash", "check program.c"], ["text", "done"]]
        row = self.session(script, feedback="=== build exit 0\n=== run exit 0\n")
        self.assertEqual(row["status"], "completed")
        self.assertTrue(row["passed"])
        self.assertGreaterEqual(row["check_served"], 2)
        self.assertEqual(row["check_runs"], 2)
        self.assertEqual(row["escapes"], [])
        self.assertEqual(row["audit"]["escapes"], 0)
        self.assertGreaterEqual(row["audit"]["scratch"], 1)
        self.assertEqual((self.tmp / "out/sessions/01/workspace/program.c").read_text(), FIXED)

    def test_check_rejects_a_file_outside_the_workspace(self):
        with tempfile.TemporaryDirectory() as tmp:
            ws = Path(tmp) / "ws"
            (ws / ".check/req").mkdir(parents=True)
            (ws / ".check/resp").mkdir(parents=True)
            (ws / "program.c").write_text(SOURCE)
            (ws / ".check/req/x.file").write_text("../escape.c\n")
            (ws / ".check/req/x.ready").write_text("")
            with patch.object(agent, "feedback", return_value="FEEDBACK"):
                agent.serve("c0", ws, set(), Path(tmp) / "log.jsonl")
            self.assertEqual((ws / ".check/resp/x.code").read_text(), "2")
            self.assertIn("name a C file", (ws / ".check/resp/x.out").read_text())
            # a valid request gets the feedback
            (ws / ".check/req/y.file").write_text("program.c\n")
            (ws / ".check/req/y.ready").write_text("")
            with patch.object(agent, "feedback", return_value="FEEDBACK"):
                agent.serve("c0", ws, set(), Path(tmp) / "log.jsonl")
            self.assertEqual((ws / ".check/resp/y.out").read_text(), "FEEDBACK")
            self.assertEqual((ws / ".check/resp/y.code").read_text(), "0")

    def test_timeout_kills_the_session(self):
        start = time.monotonic()
        row = self.session([["sleep", 60]], wall=2)
        self.assertLess(time.monotonic() - start, 40)
        self.assertEqual(row["status"], "timeout")
        notes = json.loads((self.tmp / "notes.json").read_text())
        with self.assertRaises(ProcessLookupError):
            os.kill(notes["pid"], 0)

    def test_rate_limit_notice_stops_the_session(self):
        start = time.monotonic()
        row = self.session([["quota"], ["sleep", 60]], wall=40)
        self.assertLess(time.monotonic() - start, 40)
        self.assertEqual(row["status"], "stopped")
        self.assertIn("429", row["stop"])

    def test_authentication_error_stops_the_session(self):
        row = self.session([["auth"]])
        self.assertEqual(row["status"], "stopped")
        self.assertIn("provider.auth_error", row["stop"])

    def test_clean_environment_and_controls(self):
        row = self.session([["bash", "echo PATH=$PATH; echo HOME=$HOME; echo KCH=$KIMI_CODE_HOME; echo EFF=$KIMI_MODEL_THINKING_EFFORT"], ["text", "ok"]])
        notes = json.loads((self.tmp / "notes.json").read_text())
        env, argv = notes["env"], notes["argv"]
        self.assertEqual(env["PATH"], str(self.tmp / "wroot/01/bin"))
        self.assertEqual(env["HOME"], str(self.tmp / "wroot/01/home"))
        self.assertEqual(env["KIMI_CODE_HOME"], str(self.tmp / "khome"))
        self.assertEqual((env["KIMI_MODEL_THINKING_EFFORT"], env["KIMI_CODE_BUILTIN_PRODUCT_SKILLS"]), ("low", "0"))
        self.assertNotIn("--auto", argv)
        self.assertEqual(argv[argv.index("--output-format") + 1], "stream-json")
        self.assertEqual(os.listdir(argv[argv.index("--skills-dir") + 1]), [])
        self.assertIn("disallowedTools: WebSearch, FetchURL, Agent, AgentSwarm", Path(argv[argv.index("--agent-file") + 1]).read_text())

    def test_audit_three_classes(self):
        w = dict(ws=Path("/private/tmp/exp/ws"), tmp=Path("/private/tmp/exp/tmp"))
        calls = [
            ("1", "Bash", {"command": "check program.c"}),
            ("2", "Bash", {"command": "sed -n 1,5p program.c"}),
            ("3", "Bash", {"command": "echo x > \"$TMPDIR/scratch.c\"", "cwd": "/private/tmp/exp/tmp"}),
            ("4", "Bash", {"command": "/usr/bin/cc -o /private/tmp/exp/tmp/p program.c"}),
            ("5", "Bash", {"command": "cat /etc/hosts"}),
            ("6", "Bash", {"command": "curl https://example.com"}),
            ("7", "Read", {"path": "/Users/onehrxn/.bashrc"}),
            ("8", "FetchURL", {"url": "https://example.com"}),
        ]
        results = {
            "1": ("=== build exit 0", False), "2": ("int main", False), "3": ("", False),
            "4": ("/bin/sh: /usr/bin/cc: Operation not permitted", True),
            "5": ("cat: /etc/hosts: Operation not permitted", True),
            "6": ("/bin/sh: curl: command not found", True),
            "7": ("Operation not permitted", True), "8": ("", True),
        }
        cl = agent.audit(calls, results, w)
        self.assertEqual(cl["escapes"], [])
        kinds = sorted((r["kind"], r["detail"]) for r in cl["blocked"])
        self.assertIn(("tool", "/usr/bin/cc"), kinds)
        self.assertIn(("tool", "curl"), kinds)
        self.assertIn(("path", "/etc/hosts"), kinds)
        self.assertIn(("web", "FetchURL"), kinds)
        self.assertTrue(cl["scratch"] and all(agent.inside(r["detail"], w) for r in cl["scratch"]))

    def test_audit_marks_a_real_escape(self):
        w = dict(ws=Path("/private/tmp/exp/ws"), tmp=Path("/private/tmp/exp/tmp"))
        calls = [("1", "Bash", {"command": "/usr/bin/cc -o p program.c && ./p"})]
        results = {"1": ("compiled and ran, output 42", False)}
        cl = agent.audit(calls, results, w)
        self.assertTrue(cl["escapes"])
        self.assertTrue(any(r["kind"] == "tool" for r in cl["escapes"]))
        self.assertEqual(cl["blocked"], [])

    def test_filesystem_escape_backstop(self):
        w = dict(ws=self.tmp / "ws", tmp=self.tmp / "tmp")
        started = time.time() - 1
        probe = Path("/private/tmp") / ("agent-repair-escape-%d.txt" % os.getpid())
        probe.write_text("x")
        self.addCleanup(lambda: probe.unlink(missing_ok=True))
        hits = agent.filesystem_escape(w, started)
        self.assertIn(str(probe), hits)

    def test_wire_parse_nested_and_errors(self):
        records = [
            {"type": "llm.request", "kind": "loop", "modelAlias": "m", "thinkingEffort": "low"},
            {"type": "llm.request", "kind": "compaction", "modelAlias": "m", "thinkingEffort": "low"},
            {"type": "usage.record", "usage": {"inputOther": 7, "output": 3, "inputCacheRead": 11, "inputCacheCreation": 0}},
            {"type": "context.append_loop_event", "event": {"type": "tool.call", "name": "Bash", "toolCallId": "t1", "args": {"command": "check program.c"}}},
            {"type": "context.append_loop_event", "event": {"type": "tool.result", "toolCallId": "t1", "result": {"output": "ok", "isError": False}}},
            {"type": "turn.ended", "reason": "failed", "error": {"code": "provider.rate_limit", "message": "429 slow down"}},
        ]
        path = self.tmp / "wire.jsonl"
        path.write_text("".join(json.dumps(r) + "\n" for r in records) + "bad\n")
        m, results = agent.parse([path])
        self.assertEqual((m["steps"], m["request_kinds"]), (2, {"loop": 1, "compaction": 1}))
        self.assertEqual(m["tokens"], {"inputOther": 7, "output": 3, "inputCacheRead": 11, "inputCacheCreation": 0})
        self.assertEqual(m["tool_calls"], {"Bash": 1})
        self.assertEqual(results["t1"], ("ok", False))
        self.assertTrue(agent.refusal(m["errors"][0]))

    def test_quota_pace_waits_then_stops(self):
        args = self.ns(quota_port=1, max_week_ratio=0.5, max_wait=100)
        reads = iter([
            {"data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.95, "resetAt": "2000-01-01T00:00:00Z"}, "limit7d": {"usedRatio": 0.1}}}}},
            {"data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.2}, "limit7d": {"usedRatio": 0.1}}}}},
        ])
        with patch.object(agent.time, "sleep") as sleep:
            q, decision = agent.pace(args, "before", read=lambda *a: next(reads))
        self.assertEqual(decision, "ok")
        self.assertTrue(sleep.called)
        stop_read = lambda *a: {"data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.1}, "limit7d": {"usedRatio": 0.7}}}}}
        q, decision = agent.pace(args, "before", read=stop_read)
        self.assertEqual(decision, "stop_week")

    def test_dry_run_rotation_and_guards(self):
        with patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(agent.main(["run", "--items", str(self.items), "--item", "a", "--item", "b", "--item", "c",
                                         "--out", str(self.tmp / "o"), "--max-week-ratio", "0.6", "--dry-run"]), 0)
        order = [(r["item"], r["condition"]) for r in json.loads(out.getvalue())["order"]]
        self.assertEqual(order[:4], [("a", "c0"), ("a", "c1"), ("a", "c3"), ("b", "c1")])
        with self.assertRaisesRegex(SystemExit, "outside the repository"):
            agent.main(["run", "--items", str(self.items), "--item", "a", "--out", str(agent.ROOT / "x"), "--max-week-ratio", "0.6"])
        with self.assertRaises(SystemExit):  # --max-week-ratio is required
            agent.main(["run", "--items", str(self.items), "--item", "a", "--out", str(self.tmp / "o2")])


if __name__ == "__main__":
    unittest.main()
