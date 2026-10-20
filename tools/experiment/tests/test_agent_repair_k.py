"""Quota-free tests for Protocol B (P22): condition k, the F and K order, the pilot list, the stop rules, the
precondition and the pilot go criteria.

Fakes stand in for evocc, evo, the sealed container and the Kimi CLI. They replay the frozen experiment 4
records, so no test needs a compiler binary, the network or the quota."""
from contextlib import ExitStack
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


agent = load("agent_repair", HERE.parent / "agent_repair.py")
FAKE_KIMI = load("agent_repair_fakes", HERE / "test_agent_repair.py").FAKE  # the fake Kimi CLI of the c1/c3 tests
CORPUS = agent.ROOT / "tests/fixtures/agent-repair-4"
QUALIFICATION = json.loads((CORPUS / "qualification.json").read_text())["items"]
FROZEN = {e["item"]: e["feedback"] for e in QUALIFICATION}
PROTOCOL = json.loads((HERE.parent / "protocol-7.json").read_text())
SCOPE = "No undefined behavior was found on the executed path. This does not check that the results are correct."
# The concise-1 record of adler-byte program.c in p22-feedback-design.md.
ADLER = ('{"form":"concise-1","status":"stopped","diagnostics":[{"code":"E3011","message":"out-of-bounds-memory-access",'
         '"at":"input.c:36:12","source":"view->data[index + 1]","why":"The executed access extends outside the object\'s '
         'permitted storage.","fix":"Check indices and access sizes before reading or writing."}],"exit_code":1}\n')
CLEAN = '{"form":"concise-1","status":"completed","main_return":"0","scope":"%s","exit_code":0}\n' % SCOPE
EDITED = "int main(void)\n{\n    return 0;\n}\n"
VERSIONS = dict(evo_version="evocc 0.1.0 (test)\n", evocc_version="evocc 0.1.0 (test)\n", evo_sha256="e" * 64,
                evocc_sha256="c" * 64, kimi_version="2.0.2", kimi_sha256="a" * 64)
BINARIES = {k: VERSIONS[k] for k in ("evo_sha256", "evocc_sha256", "kimi_sha256")}
GIB = 1024 ** 3


def disk(*free_gib):
    """A fake shutil.disk_usage that reports the given free sizes in turn, then the last one."""
    sizes = list(free_gib)
    return lambda path: types.SimpleNamespace(free=int((sizes.pop(0) if len(sizes) > 1 else sizes[0]) * GIB))


def passed_precondition(path, **versions):
    path.write_text(json.dumps(dict(passed=True, summary={}, versions=dict(VERSIONS, **versions))))
    return path


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def records(text):
    """The header, check record, explain records by code and plan record of a frozen C3 string."""
    head, part = agent.split(text)
    lines = part.splitlines(keepends=True)
    return head, lines[0], {json.loads(line)["code"]: line for line in lines[1:-1]}, lines[-1]


def concise_fixture(path, special=None):
    """A concise-1 fixture in the format of tests/fixtures/agent-repair-4/concise-1-feedback.json, with
    synthetic records. Each one names the scratch directory once, so the tests cover its replacement."""
    scratch = "/private/var/folders/xx/T/silent-feedback-recorded"
    items = []
    for entry in QUALIFICATION:
        found = (special or {}).get(entry["item"]) or ('{"form":"concise-1","status":"stopped","diagnostics":[{"code":"%s",'
                                                       '"message":"m","at":"input.c:1:1","also":["%s/input.c:2:1 x"]}],"exit_code":1}\n'
                                                       % (entry["code"], scratch))
        items.append(dict(item=entry["item"], code=entry["code"], feedback={
            "program.c": {"scratch": scratch, "concise-1": found},
            "fixed.c": {"scratch": scratch, "concise-1": CLEAN}}))
    path.write_text(json.dumps(dict(schema="agent-repair.concise-feedback.v1", form="concise-1", items=items)))
    return path


class Commands:
    """Fake evocc, evo and sealed container. For a corpus source they print the frozen records, with the
    recorded scratch directory replaced by the live one, as evo prints it. An edited source is clean."""

    def __init__(self, concise=None, current=False):
        # current: print the T03s scope of the current compiler instead of the frozen one.
        self.calls, self.concise, self.current = [], concise or {}, current
        self.known = {sha(CORPUS / e["item"] / s): (e["item"], s) for e in QUALIFICATION for s in ("program.c", "fixed.c")}

    def source(self, cwd):
        return self.known.get(sha(Path(cwd) / "input.c"))

    def run(self, argv, cwd=agent.ROOT, data=None):
        argv, cwd = [str(a) for a in argv], str(cwd)
        self.calls.append([os.path.basename(argv[0])] + argv[1:])
        if os.path.basename(argv[0]) != "evo":
            return dict(argv=argv, exit_code=0, stdout="", stderr="")
        key, words = self.source(cwd), argv[1:]
        if key:
            frozen = FROZEN[key[0]][key[1]]["c3"]
            _, check, explain, plan = records(frozen.replace(agent.scratch_of(frozen, "silent-feedback-"), cwd))
        else:
            check = ('{"exit_code":0,"completion":{"claim":"executed-path-only","kind":"check","main_return":"0",'
                     '"schema":"evocc.check.v1","scope":"%s","status":"completed"},"diagnostics":[]}\n' % SCOPE)
            explain, plan = {}, '{"file":"%s/input.c","repairs":[],"diagnostics":[],"applied":0,"exit_code":0}\n' % cwd
        if words[0] == "explain":
            text = explain.get(words[1], "")
        elif words[0] == "fix":
            text = plan
        elif "--form=concise-1" in words:
            text = self.concise.get(key, CLEAN).replace("<scratch>", cwd)
        else:
            text = check
        if self.current:
            text = agent.current_scope(text)
        return dict(argv=argv, exit_code=0, stdout=text, stderr="")

    def container(self, stage, mode):
        self.calls.append(["container", mode])
        key = self.source(stage)
        head = agent.split(FROZEN[key[0]][key[1]]["c3"])[0] if key else "=== evocc build exit 0\n=== assemble and link exit 0\n=== run exit 0\n--- stdout\n--- stderr\n"
        return head.split("\n", 1)[1]


def feedback(condition, item, source, fakes):
    with tempfile.TemporaryDirectory(prefix="agent-repair-check-") as tmp, \
            patch.object(agent.repair, "run", side_effect=fakes.run), patch.object(agent, "container_build", side_effect=fakes.container):
        return agent.feedback(condition, CORPUS / item / source, Path(tmp)), tmp


class FeedbackTests(unittest.TestCase):
    def test_c3_bytes_are_unchanged_for_recorded_inputs(self):
        for item, source in (("adler-byte", "program.c"), ("adler-byte", "fixed.c"), ("mean-empty", "program.c"), ("crc-reset", "program.c")):
            with self.subTest(item=item, source=source):
                fakes, frozen = Commands(), FROZEN[item][source]["c3"]
                text, stage = feedback("c3", item, source, fakes)
                self.assertEqual(text.replace(stage, agent.scratch_of(frozen, "silent-feedback-")), frozen)
                target = stage + "/input.c"
                self.assertEqual(fakes.calls, [["evocc", "-S", "--target=aarch64-unknown-linux-gnu", "-O0", target, "-o", stage + "/input.s"],
                                               ["container", "c3"], ["evo", "check", "--json", target]]
                                 + [["evo", "explain", code, "--json"] for code in sorted(records(frozen)[2])]
                                 + [["evo", "fix", target, "--plan", "--json"]])

    def test_k_is_the_c3_header_and_one_concise_record(self):
        fakes = Commands({("adler-byte", "program.c"): ADLER})
        text, stage = feedback("k", "adler-byte", "program.c", fakes)
        full, _ = feedback("c3", "adler-byte", "program.c", Commands())
        head = agent.split(FROZEN["adler-byte"]["program.c"]["c3"])[0]
        self.assertEqual(text, head + "=== evo diagnostics\n" + ADLER)
        self.assertEqual(agent.split(text)[0], agent.split(full)[0])
        self.assertEqual(fakes.calls[1:], [["container", "c3"], ["evo", "check", "--json", "--form=concise-1", stage + "/input.c"]])
        self.assertFalse(any(c[:2] in (["evo", "explain"], ["evo", "fix"]) for c in fakes.calls))

    def test_a_failed_build_keeps_the_shared_header_and_k_takes_standard_error(self):
        error = '{"form":"concise-1","status":"stopped","exit_code":64}\n'

        def run(argv, cwd=None, data=None):
            if os.path.basename(str(argv[0])) == "evocc":
                return dict(exit_code=1, stdout="", stderr="e" * 5000)
            return dict(exit_code=64, stdout=" \n", stderr=error)
        head = "=== evocc build exit 1\n" + "e" * 4000 + "=== evocc build failed; the container did not run\n"
        for condition, records in (("k", error), ("c3", error * 2)):  # c3: the check and the plan record
            with self.subTest(condition=condition), tempfile.TemporaryDirectory() as tmp, \
                    patch.object(agent.repair, "run", side_effect=run), patch.object(agent, "container_build") as container:
                text = agent.feedback(condition, CORPUS / "adler-byte/program.c", Path(tmp))
                container.assert_not_called()
                self.assertEqual(text, head + "=== evo diagnostics\n" + records)

    def test_c1_runs_only_the_container(self):
        fakes = Commands()
        text, _ = feedback("c1", "adler-byte", "program.c", fakes)
        self.assertEqual(fakes.calls, [["container", "c1"]])
        self.assertTrue(text.startswith("=== assemble and link exit 0"))

    def test_serve_logs_the_request_and_the_checked_bytes(self):
        with tempfile.TemporaryDirectory() as tmp:
            ws, log = Path(tmp) / "ws", Path(tmp) / "log.jsonl"
            (ws / ".check/req").mkdir(parents=True)
            (ws / ".check/resp").mkdir(parents=True)
            shutil.copyfile(CORPUS / "adler-byte/program.c", ws / "program.c")
            for rid, name in (("7-1", "program.c"), ("7-2", "../outside.c")):
                (ws / ".check/req" / (rid + ".file")).write_text(name + "\n")
                (ws / ".check/req" / (rid + ".ready")).write_text("")
            with patch.object(agent, "feedback", return_value="TEXT") as served:
                agent.serve("k", ws, set(), log)
            rows = [json.loads(line) for line in log.read_text().splitlines()]
            self.assertEqual([(r["id"], r["file"], r["exit"], r["sha256"]) for r in rows],
                             [("7-1", "program.c", 0, sha(CORPUS / "adler-byte/program.c")), ("7-2", "../outside.c", 2, None)])
            self.assertEqual(served.call_args[0][0], "k")
            self.assertEqual((ws / ".check/resp/7-1.out").read_text(), "TEXT")


class OrderTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="protocol-b-order-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.items = self.tmp / "items"
        for name in PROTOCOL["items"]:
            (self.items / name).mkdir(parents=True)
            for f in ("program.c", "fix-spans.json"):
                (self.items / name / f).write_text("x")

    def plan(self, *extra, items=PROTOCOL["items"]):
        argv = ["run", "--items", str(self.items), "--out", str(self.tmp / "out"), "--max-week-ratio", "0.85",
                "--max-week-increase", "0.15", "--dry-run"] + [a for name in items for a in ("--item", name)] + list(extra)
        with patch("sys.stdout", new_callable=io.StringIO) as out:
            self.assertEqual(agent.main(argv), 0)
        return json.loads(out.getvalue())

    def test_protocol_b_order_over_the_corpus(self):
        plan = self.plan("--order", "protocol-b", "--reps", "2", "--max-sessions", "96", items=list(reversed(PROTOCOL["items"])))
        order = [(r["item"], r["condition"], r["repetition"]) for r in plan["order"]]
        expected = [(name, c, rep) for rep in (1, 2) for i, name in enumerate(sorted(PROTOCOL["items"]))
                    for c in (("c3", "k") if (i + rep) % 2 else ("k", "c3"))]
        self.assertEqual((order, plan["order_rule"], len(order)), (expected, "protocol-b", 96))
        first = {(name, rep): c for name, c, rep in reversed(order)}  # the condition that runs first in each pair
        for i, name in enumerate(sorted(PROTOCOL["items"])):
            self.assertEqual(first[name, 1], "c3" if i % 2 == 0 else "k")
            self.assertNotEqual(first[name, 2], first[name, 1])

    def test_pilot_order_is_the_protocol_table(self):
        plan = self.plan("--order", "pilot-b", "--max-sessions", "10", items=sorted(agent.PILOT_B))
        order = [(r["item"], r["condition"], r["repetition"]) for r in plan["order"]]
        table = PROTOCOL["phases"]["pilot"]["sessions"]
        self.assertEqual(order, [(s["item"], s["condition"], 1) for s in table])
        self.assertEqual(order, [("adler-byte", "c3", 1), ("adler-byte", "k", 1), ("crc-reset", "k", 1), ("crc-reset", "c3", 1),
                                 ("minify-view", "c3", 1), ("minify-view", "k", 1), ("increment-limit", "k", 1),
                                 ("increment-limit", "c3", 1), ("mean-empty", "c3", 1), ("mean-empty", "k", 1)])
        self.assertEqual([s["arm"] for s in table], [{"c3": "F", "k": "K"}[s["condition"]] for s in table])
        self.assertEqual(PROTOCOL["phases"]["pilot"]["items"], list(agent.PILOT_B))

    def test_protocol_b_refuses_other_conditions_items_and_settings(self):
        cases = {"four pilot items": (("--order", "pilot-b"), list(agent.PILOT_B)[:4]),
                 "two pilot repetitions": (("--order", "pilot-b", "--reps", "2", "--max-sessions", "20"), agent.PILOT_B),
                 "c1": (("--order", "protocol-b", "--condition", "c1", "--condition", "c3", "--max-sessions", "48"), PROTOCOL["items"]),
                 "K first": (("--order", "protocol-b", "--condition", "k", "--condition", "c3", "--max-sessions", "48"), PROTOCOL["items"]),
                 "wall": (("--order", "protocol-b", "--wall", "1800", "--max-sessions", "48"), PROTOCOL["items"]),
                 "effort": (("--order", "protocol-b", "--effort", "default", "--max-sessions", "48"), PROTOCOL["items"]),
                 "steps": (("--order", "protocol-b", "--max-steps", "50", "--max-sessions", "48"), PROTOCOL["items"]),
                 "prompt": (("--order", "protocol-b", "--prompt", str(agent.HERE / "prompt-2.txt"), "--max-sessions", "48"), PROTOCOL["items"]),
                 "increase": (("--order", "protocol-b", "--max-week-increase", "0.30", "--max-sessions", "48"), PROTOCOL["items"])}
        for name, (extra, items) in cases.items():
            with self.subTest(case=name), self.assertRaises(SystemExit):
                self.plan(*extra, items=items)
        self.assertEqual(len(self.plan("--condition", "c3", "--condition", "k", "--max-sessions", "48")["order"]), 48)

    def test_protocol_file_matches_the_harness(self):
        self.assertEqual(PROTOCOL["status"], "unregistered")
        keys = ("model", "effort", "max_steps", "wall", "prompt")
        self.assertEqual({k: PROTOCOL["agent"][k] for k in keys}, {k: agent.SETTINGS_B[k] for k in keys})
        self.assertEqual(PROTOCOL["phases"]["pilot"]["go_criteria"], list(agent.GO_B))
        self.assertEqual([PROTOCOL["conditions"][arm]["harness_condition"] for arm in ("F", "K")], list(agent.F_K))
        self.assertEqual(PROTOCOL["items"], sorted(p.name for p in CORPUS.iterdir() if p.is_dir()))
        classes = [(CORPUS / name / "class").read_text().strip() for name in PROTOCOL["items"]]
        self.assertEqual(PROTOCOL["classes"], {c: classes.count(c) for c in set(classes)})
        self.assertEqual(PROTOCOL["phases"]["pilot"]["codes"], {n: (CORPUS / n / "class").read_text().strip() for n in agent.PILOT_B})
        self.assertEqual(PROTOCOL["qualification"]["sha256"], sha(CORPUS / "qualification.json"))
        self.assertEqual(PROTOCOL["agent"]["prompt_sha256"], sha(agent.HERE / "prompt-3.txt"))


class StopTests(unittest.TestCase):
    """The package stop rules, with the session and the quota read replaced by fakes."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="protocol-b-stop-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        for name in agent.PILOT_B:
            (self.tmp / "items" / name).mkdir(parents=True)
            for f, text in (("program.c", "x"), ("fix-spans.json", "{}"), ("class", "E3001\n")):
                (self.tmp / "items" / name / f).write_text(text)

    def pilot(self, statuses=(), decisions=(), free=(50,), fail_at=None, changed_at=None, precondition=True):
        args = types.SimpleNamespace(items=self.tmp / "items", item=list(agent.PILOT_B), condition=None, reps=1, first_repetition=1,
                                     max_sessions=10, phase="test", out=self.tmp / "out", kimi=self.tmp / "kimi", model="kimi-code/k3",
                                     effort="low", wall=900, max_steps=100, prompt=agent.HERE / "prompt-3.txt", quota_port=1,
                                     max_week_ratio=0.85, max_week_increase=0.15, max_wait=5, dry_run=False, order="pilot-b",
                                     precondition=passed_precondition(self.tmp / "pre.json") if precondition else None)
        statuses, decisions, reads, calls = dict(statuses), dict(decisions), [], []

        def hashes(kimi):  # call 1 freezes the values; calls 2 to 11 precede sessions 1 to 10; call 12 follows the last
            calls.append(kimi)
            return dict(BINARIES, evo_sha256="f" * 64) if changed_at and len(calls) >= changed_at else BINARIES

        def session(args, out, index, item, condition):
            if index == fail_at:
                raise RuntimeError("fake harness failure")
            status = statuses.get(index, "completed")
            return dict(index=index, item=item.name, condition=condition, status="completed" if status == "escape" else status,
                        passed=status == "completed", escapes=[dict(kind="tool", detail="cc")] if status == "escape" else [])

        def pace(args, label, read=None):
            reads.append(label)
            return {"label": label}, decisions.get(label, "ok")
        with patch.object(agent, "session", side_effect=session), patch.object(agent, "pace", side_effect=pace), \
                patch.object(agent, "provenance", return_value={}), patch("sys.stdout", io.StringIO()), \
                patch.object(agent, "binaries", side_effect=hashes), patch.object(agent.time, "sleep") as self.sleep, \
                patch.object(agent.shutil, "disk_usage", side_effect=disk(*free)):
            code = agent.run(args)
        self.addCleanup(shutil.rmtree, args.wroot, True)
        receipt = json.loads((args.out / "receipt.json").read_text())
        return code, receipt, reads

    def test_a_complete_pilot_reads_the_quota_before_each_session_and_at_the_end(self):
        code, receipt, reads = self.pilot()
        self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"]), receipt["unattempted"]), (0, "completed", 10, 0))
        self.assertEqual(reads, ["before %02d" % i for i in range(1, 11)] + ["final"])

    def test_a_quota_decision_stops_the_package_before_the_session(self):
        for decision in ("stop_week", "quota_unavailable", "five_hour_quota"):
            with self.subTest(decision=decision):
                shutil.rmtree(self.tmp / "out", ignore_errors=True)
                code, receipt, reads = self.pilot(decisions={"before 04": decision})
                self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"]), receipt["unattempted"]), (1, decision, 3, 7))
                self.assertEqual(reads[-1], "before 04")  # the stopping read is the last one

    def test_a_refusal_an_escape_or_a_harness_error_stops_the_package_and_reads_the_quota_at_the_end(self):
        for status, reason in (("stopped", "provider_refusal"), ("escape", "escape_detected"), (None, "harness_error")):
            with self.subTest(reason=reason):
                shutil.rmtree(self.tmp / "out", ignore_errors=True)
                code, receipt, reads = self.pilot(statuses={3: status} if status else {}, fail_at=None if status else 3)
                self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"]), receipt["unattempted"]), (1, reason, 3, 7))
                self.assertEqual(reads, ["before 01", "before 02", "before 03", "final"])

    def test_low_disk_is_waited_out_before_a_session_and_recorded(self):
        code, receipt, reads = self.pilot(free=(24.5, 24.9, 25, 50))
        self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"])), (0, "completed", 10))
        [wait] = receipt["disk_waits"]
        self.assertEqual((wait["label"], wait["seconds"], wait["free"], wait["free_gib"]), ("before 01", 600.0, True, [24.5, 24.9, 25.0]))
        self.assertEqual([c.args for c in self.sleep.call_args_list], [(300.0,), (300.0,)])
        self.assertEqual(reads[0], "before 01")  # the quota read follows the wait

    def test_disk_below_25_gib_for_6_hours_stops_the_package(self):
        code, receipt, reads = self.pilot(free=(25 - 1 / GIB,))
        self.assertEqual((code, receipt["stop_reason"], receipt["runs"], receipt["unattempted"]), (1, "disk_below_25_GiB", [], 10))
        [wait] = receipt["disk_waits"]
        self.assertEqual((wait["seconds"], wait["free"], len(wait["free_gib"])), (21600.0, False, 73))
        self.assertEqual(reads, ["final"])

    def test_changed_binaries_stop_the_package(self):
        code, receipt, _ = self.pilot(changed_at=4)
        self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"])), (1, "binaries_changed", 2))
        self.assertEqual(receipt["binaries"], BINARIES)
        shutil.rmtree(self.tmp / "out")
        code, receipt, _ = self.pilot(changed_at=12)
        self.assertEqual((code, receipt["stop_reason"], len(receipt["runs"])), (1, "binaries_changed", 10))

    def test_a_live_run_needs_a_passed_precondition_for_the_same_binaries(self):
        for name, path in (("missing", None), ("failed", self.tmp / "failed.json"), ("other evo", self.tmp / "other.json")):
            with self.subTest(precondition=name):
                if name == "failed":
                    path.write_text(json.dumps(dict(passed=False, versions=VERSIONS)))
                if name == "other evo":
                    passed_precondition(path, evo_sha256="0" * 64)
                args = types.SimpleNamespace(**dict(vars(self.args()), precondition=path))
                with patch.object(agent, "binaries", return_value=BINARIES), self.assertRaisesRegex(SystemExit, "precondition"):
                    agent.run(args)
                self.assertFalse((self.tmp / "out").exists())

    def args(self):
        return types.SimpleNamespace(items=self.tmp / "items", item=list(agent.PILOT_B), condition=None, reps=1, first_repetition=1,
                                     max_sessions=10, phase="test", out=self.tmp / "out", kimi=self.tmp / "kimi", model="kimi-code/k3",
                                     effort="low", wall=900, max_steps=100, prompt=agent.HERE / "prompt-3.txt", quota_port=1,
                                     max_week_ratio=0.85, max_week_increase=0.15, max_wait=5, dry_run=False, order="pilot-b")

    def test_an_overloaded_session_is_retried_once_after_all_initial_sessions(self):
        code, receipt, _ = self.pilot(statuses={2: "provider_overloaded"})
        rows = receipt["runs"]
        self.assertEqual((code, len(rows), rows[10]["retry_of"], (rows[10]["item"], rows[10]["condition"])), (0, 11, 2, ("adler-byte", "k")))
        shutil.rmtree(self.tmp / "out")
        code, receipt, _ = self.pilot(statuses={2: "provider_overloaded", 11: "provider_overloaded"})
        self.assertEqual((len(receipt["runs"]), receipt["runs"][-1]["status"]), (11, "provider_overloaded"))  # no second retry

    def test_the_week_stop_line_is_the_lower_of_the_first_ratio_plus_0_15_and_0_85(self):
        def read(week, five=0.1):
            return lambda *unused: {"data": {"quota": {"usages": {"limit5h": {"usedRatio": five}, "limit7d": {"usedRatio": week}}}}}
        for first, below, line in ((0.30, 0.449999, 0.45), (0.75, 0.849999, 0.85)):
            with self.subTest(first=first):
                args = types.SimpleNamespace(kimi=None, quota_port=1, max_week_ratio=0.85, max_week_increase=0.15, max_wait=5)
                self.assertEqual(agent.pace(args, "before 01", read=read(first))[1], "ok")
                self.assertEqual(agent.pace(args, "before 02", read=read(below))[1], "ok")
                self.assertEqual(agent.pace(args, "before 03", read=read(line))[1], "stop_week")
        args = types.SimpleNamespace(kimi=None, quota_port=1, max_week_ratio=0.85, max_week_increase=0.15, max_wait=5)
        self.assertEqual(agent.pace(args, "before 01", read=read(0.86))[1], "stop_week")
        self.assertEqual(agent.pace(args, "before 02", read=lambda *unused: {"error": "URLError"})[1], "quota_unavailable")

    def test_a_five_hour_ratio_at_0_90_is_waited_out(self):
        args = types.SimpleNamespace(kimi=None, quota_port=1, max_week_ratio=0.85, max_week_increase=0.15, max_wait=100)
        reads = iter([{"data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.90, "resetAt": "2000-01-01T00:00:00Z"}, "limit7d": {"usedRatio": 0.3}}}}},
                      {"data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.2}, "limit7d": {"usedRatio": 0.3}}}}}])
        with patch.object(agent.time, "sleep") as sleep:
            q, decision = agent.pace(args, "before 01", read=lambda *unused: next(reads))
        self.assertEqual((decision, q["label"]), ("ok", "before 01-waited"))
        self.assertTrue(sleep.called)


class PilotTests(unittest.TestCase):
    """Sessions with the fake Kimi CLI. The harness serves real c3 and k feedback built from fake commands."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="protocol-b-pilot-")).resolve()
        self.addCleanup(shutil.rmtree, self.tmp, True)
        (self.tmp / "kimi").write_text(FAKE_KIMI.format(python=sys.executable))
        (self.tmp / "kimi").chmod(0o755)
        (self.tmp / "khome").mkdir()
        self.fixture = concise_fixture(self.tmp / "concise.json", {"adler-byte": ADLER})
        self.fakes, self.disk = Commands(agent.concise_strings(self.fixture)), disk(50)

    def args(self):
        return types.SimpleNamespace(items=CORPUS, item=list(agent.PILOT_B), condition=None, reps=1, first_repetition=1, max_sessions=10,
                                     phase="test", out=self.tmp / "out", kimi=self.tmp / "kimi", model="kimi-code/k3", effort="low",
                                     wall=900, max_steps=100, prompt=agent.HERE / "prompt-3.txt", quota_port=1, max_week_ratio=0.85,
                                     max_week_increase=0.15, max_wait=5, dry_run=False, order="pilot-b",
                                     precondition=passed_precondition(self.tmp / "precondition.json"))

    def harness(self, stack, script):
        real, reads = agent.subprocess.Popen, []

        def popen(argv, **kw):
            if argv and str(argv[0]).endswith("sandbox-exec"):
                argv = argv[3:]  # drop the sandbox; the fake CLI runs each scripted command for real
            kw["env"] = dict(kw.get("env") or {}, AGENT_SCRIPT=json.dumps(script), AGENT_NOTES=str(self.tmp / "notes.json"))
            return real(argv, **kw)

        def pace(args, label, read=None):
            q = {"label": label, "data": {"quota": {"usages": {"limit5h": {"usedRatio": 0.1}, "limit7d": {"usedRatio": round(0.3 + 0.001 * len(reads), 6)}}}}}
            reads.append(q)
            return q, "ok"

        def score(source, item):
            return {cc + opt: dict(passed=source == EDITED) for cc, opt in agent.qualify_silent.BUILDS}
        stack.enter_context(patch.object(agent, "KHOME", self.tmp / "khome"))
        stack.enter_context(patch.object(agent, "DISK_WAIT", (0.125, 1.0)))  # real sleeps, short ones: the sessions run for real
        stack.enter_context(patch("sys.stdout", io.StringIO()))
        for target, name, fake in ((agent.subprocess, "Popen", popen), (agent.repair, "run", self.fakes.run),
                                   (agent, "container_build", self.fakes.container), (agent, "pace", pace),
                                   (agent, "provenance", lambda args: {}), (agent, "repo_untracked", set),
                                   (agent, "versions", lambda kimi: VERSIONS), (agent, "binaries", lambda kimi: BINARIES),
                                   (agent.qualify_silent, "score_builds", score), (agent.shutil, "disk_usage", self.disk)):
            stack.enter_context(patch.object(target, name, side_effect=fake))

    def session(self, script, condition="k"):
        args = self.args()
        args.wroot = (self.tmp / "wroot").resolve()
        args.base, args.prompt_text = args.wroot / "env", "repair it"
        (args.base / "skills").mkdir(parents=True)
        (args.base / "agent.md").write_text(agent.AGENT_FILE)
        agent.executable(args.base / "login-shell", agent.LOGIN_SHELL)
        with ExitStack() as stack:
            self.harness(stack, script)
            return agent.session(args, args.out, 1, CORPUS / "adler-byte", condition)

    def test_k_session_stop_rules(self):
        cases = (([["bash", "check program.c"], ["quota"], ["sleep", 60]], "stopped", "429"),
                 ([["auth"]], "stopped", "provider.auth_error"),
                 ([["overload"]] * 4 + [["sleep", 60]], "provider_overloaded", "overloaded"),
                 ([["overload"]] * 3 + [["edit", EDITED]], "completed", None))
        for script, status, stop in cases:
            with self.subTest(status=status, stop=stop):
                for path in ("out", "wroot", "khome"):
                    shutil.rmtree(self.tmp / path, ignore_errors=True)
                (self.tmp / "khome").mkdir()
                row = self.session(script)
                self.assertEqual(row["status"], status)
                if stop:
                    self.assertIn(stop, row["stop"])
                self.assertEqual(row["passed"], status == "completed")

    def test_scoring_waits_for_disk_and_records_the_wait(self):
        self.disk = disk(24, 24.5, 30)
        row = self.session([["edit", EDITED], ["text", "done"]])
        self.assertEqual((row["status"], row["passed"]), ("completed", True))
        self.assertEqual({k: row["disk_wait"][k] for k in ("label", "seconds", "free", "free_gib")},
                         dict(label="score 01", seconds=0.25, free=True, free_gib=[24.0, 24.5, 30.0]))

    def test_k_session_serves_the_k_layout(self):
        row = self.session([["bash", "check program.c"], ["edit", EDITED], ["bash", "check"], ["text", "done"]])
        self.assertEqual((row["status"], row["passed"], row["check_runs"], row["check_served"]), ("completed", True, 2, 2))
        served = agent.served(self.tmp / "out", row)
        head = agent.split(FROZEN["adler-byte"]["program.c"]["c3"])[0]
        edited = "=== evocc build exit 0\n=== assemble and link exit 0\n=== run exit 0\n--- stdout\n--- stderr\n"
        self.assertEqual([text for _, text in served], [head + agent.MARK + ADLER, edited + agent.MARK + CLEAN])
        self.assertEqual([r["sha256"] for r, _ in served], [sha(CORPUS / "adler-byte/program.c"), hashlib.sha256(EDITED.encode()).hexdigest()])

    def test_pilot_go_criteria(self):
        args = self.args()
        with ExitStack() as stack:
            self.harness(stack, [["bash", "check program.c"], ["edit", EDITED], ["bash", "check program.c"], ["text", "done"]])
            self.assertEqual(agent.run(args), 0)
            self.addCleanup(shutil.rmtree, args.wroot, True)
            pre = self.tmp / "precondition.json"
            pre.write_text(json.dumps(dict(passed=True, summary={}, versions=VERSIONS)))
            check = types.SimpleNamespace(out=args.out, precondition=pre, qualification=CORPUS / "qualification.json",
                                          concise=self.fixture, items=CORPUS, kimi=args.kimi)

            def evaluate():
                self.assertIn(agent.go(check), (0, 1))
                return json.loads((args.out / "pilot-receipt.json").read_text())
            pilot = evaluate()
            self.assertTrue(pilot["go"]["held"], pilot["go"])
            self.assertEqual(pilot["go"]["checks"], dict(k=10, k_evocc=10, k_form=10, k_known=5, k_equal=5, f=10, f_known=5, f_equal=5, missing=0))
            self.assertEqual(pilot["quota_change"], dict(first=0.3, last=0.31, increase=0.01, reads=11))
            self.assertEqual([r["passed"] for r in pilot["runs"]], [True] * 10)
            self.assertTrue(pilot["binaries_unchanged"])
            self.assertEqual(pilot["protocol"], dict(path="tools/experiment/protocol-7.json", sha256=sha(HERE.parent / "protocol-7.json"), status="unregistered"))
            self.assertEqual([c["criterion"] for c in pilot["go"]["criteria"]], list(agent.GO_B))
            # Each criterion fails when its evidence is wrong.
            receipt, first = (args.out / "receipt.json").read_text(), {}
            for row in pilot["runs"]:
                for record, text in agent.served(args.out, row):
                    first.setdefault(row["condition"], (args.out / "sessions" / ("%02d" % row["index"]) / "workspace/.check/resp" / (record["id"] + ".out"), text))
            cases = ((1, lambda: first["k"][0].write_text(first["k"][1].replace('{"form":"concise-1"', '{"form":"full-1"'))),
                     (2, lambda: first["c3"][0].write_text(first["c3"][1].replace("out-of-bounds", "out-of-range"))),
                     (3, lambda: (args.out / "receipt.json").write_text(json.dumps(dict(json.loads(receipt), quota=json.loads(receipt)["quota"][:-1])))),
                     (4, lambda: (args.out / "receipt.json").write_text(receipt.replace('"status": "completed"', '"status": "error"', 1))),
                     (0, lambda: (args.out / "receipt.json").write_text(receipt.replace('"escapes": 0', '"escapes": 1', 1))))
            for index, damage in cases:
                with self.subTest(criterion=index):
                    damage()
                    self.assertEqual([c["held"] for c in evaluate()["go"]["criteria"]], [i != index for i in range(5)])
                    (args.out / "receipt.json").write_text(receipt)
                    for path, text in first.values():
                        path.write_text(text)


class PreconditionTests(unittest.TestCase):
    def run_precondition(self, current, changed=None):
        with tempfile.TemporaryDirectory() as tmp:
            fixture = concise_fixture(Path(tmp) / "concise.json")
            out = Path(tmp) / "precondition.json"
            args = types.SimpleNamespace(qualification=CORPUS / "qualification.json", concise=fixture, items=CORPUS, kimi=Path(tmp) / "kimi", out=out)
            fakes = Commands(agent.concise_strings(fixture), current=current)
            if changed:
                fakes.concise[changed] = ADLER
            with patch.object(agent.repair, "run", side_effect=fakes.run), patch.object(agent, "container_build", side_effect=fakes.container), \
                    patch.object(agent, "versions", return_value=VERSIONS), patch("sys.stdout", io.StringIO()), \
                    patch.object(agent.shutil, "disk_usage", return_value=types.SimpleNamespace(free=50 * GIB)):
                code = agent.precondition(args)
            return code, json.loads(out.read_text())

    def test_the_current_compiler_passes_and_one_changed_record_fails(self):
        # Frozen EvoCC parts with a completed record carry the old scope; only they differ in raw bytes.
        completed = sum('"scope":' + json.dumps(agent.OLD_SCOPE) in agent.split(e["feedback"][s]["c3"])[1]
                        for e in QUALIFICATION for s in ("program.c", "fixed.c"))
        self.assertGreater(completed, 0)
        for changed in (None, ("crc-view", "program.c")):
            with self.subTest(changed=changed):
                code, record = self.run_precondition(current=True, changed=changed)
                self.assertEqual((code, record["passed"]), (1, False) if changed else (0, True))
                self.assertEqual(record["summary"], dict(
                    full_1_identical=48 - completed, full_1_identical_after_scope=48, c3_identical=48 - completed,
                    concise_1_identical=24 - bool(changed), concise_1_identical_after_scope=48 - bool(changed),
                    k_header_identical=48, concise_1_form=48))
                self.assertEqual(record["scope_correction"], dict(old=agent.OLD_SCOPE, new=agent.NEW_SCOPE))
                self.assertEqual(len(record["records"]), 48)

    def test_the_frozen_scope_no_longer_passes(self):
        code, record = self.run_precondition(current=False)
        self.assertEqual((code, record["passed"]), (1, False))
        self.assertEqual(record["summary"]["full_1_identical"], 48)
        self.assertLess(record["summary"]["full_1_identical_after_scope"], 48)

    def test_the_scope_correction_changes_only_completed_scopes(self):
        stopped = '{"form":"concise-1","status":"stopped","diagnostics":[],"exit_code":1}\n'
        self.assertEqual(agent.current_scope(stopped), stopped)
        self.assertEqual(agent.current_scope(CLEAN), CLEAN.replace(agent.OLD_SCOPE, agent.NEW_SCOPE))
        other = CLEAN.replace(agent.OLD_SCOPE, "another scope")
        self.assertEqual(agent.current_scope(other), other)
        self.assertEqual(agent.current_scope("=== not json\n"), "=== not json\n")
        self.assertIsNone(agent.current_scope(None))
        twice = CLEAN.replace('"exit_code"', '"note":"%s","scope":"%s","exit_code"' % ("x", agent.OLD_SCOPE))
        with self.assertRaises(ValueError):
            agent.current_scope(twice)

if __name__ == "__main__":
    unittest.main()
