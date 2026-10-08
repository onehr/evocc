#!/usr/bin/env python3
"""Preregistered repair experiment; reported-cost stopping is not an absolute spend guarantee."""
import argparse, datetime, gzip, hashlib, http.client, json, math, os, re, shutil, signal, subprocess, tempfile, threading, time, urllib.error, urllib.request, uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
URL = "https://openrouter.ai/api/v1/chat/completions"
CONTAINER, IMAGE = "/usr/local/bin/container", "evo-flight-lab:sealed"
EVO = ROOT / "target/frontend/release/evo"
ENV = {"PATH": "/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin", "TMPDIR": tempfile.gettempdir()}
LIMIT, MAX_TOKENS = 131072, 4096
class Failure(Exception):
    def __init__(self, message, evidence=None): super().__init__(message); self.evidence = evidence
class Stop(Failure):
    def __init__(self, message, cost=0): super().__init__(message); self.cost = cost

WORKER = r'''
import json, os, pathlib, resource, shutil, subprocess, sys, tempfile
resource.setrlimit(resource.RLIMIT_FSIZE, (1048576, 1048576))
resource.setrlimit(resource.RLIMIT_CPU, (30, 30))
resource.setrlimit(resource.RLIMIT_NPROC, (64, 64))
os.chdir(tempfile.mkdtemp(prefix="agent-repair-"))
shutil.copyfile("/input/program.c", "program.c")
payload = json.load(sys.stdin)
steps = []
def step(argv):
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        try:
            result = subprocess.run(argv, stdout=out, stderr=err, timeout=20, env={"PATH":"/usr/bin:/bin"})
            code = result.returncode
        except subprocess.TimeoutExpired:
            code = 124
        out.seek(0); err.seek(0)
        stdout, stderr = out.read(131073), err.read(131073)
        if len(stdout) > 131072 or len(stderr) > 131072:
            raise RuntimeError("capture_limit")
        steps.append(dict(argv=argv, exit_code=code, stdout=stdout.decode("utf-8", "replace"), stderr=stderr.decode("utf-8", "replace")))
        return code == 0
mode = sys.argv[1]
if mode == "score":
    if step(["gcc-14", "-O0", "-Dmain=program_main", "-c", "program.c", "-o", "program.o"]):
        pathlib.Path("reference_main.c").write_text(payload["reference"])
        linked = step(["gcc-14", "-O0", "program.o", "reference_main.c", "-o", "prog"])
        pathlib.Path("reference_main.c").unlink()
        if linked: step(["./prog"])
else:
    compiler, flags = ("gcc-14", ["-Wall", "-Wextra"]) if mode == "c1" else ("clang-19", ["-Weverything"])
    if step([compiler, *flags, "-fsanitize=undefined", "-o", "prog", "program.c"]): step(["./prog"])
print(json.dumps(steps))
'''

def run(argv, cwd=ROOT, data=None):
    with tempfile.TemporaryFile() as inp, tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        inp.write(data or b""); inp.seek(0)
        try:
            process = subprocess.Popen([str(x) for x in argv], stdin=inp, stdout=out, stderr=err, cwd=cwd, env=ENV, start_new_session=True)
        except OSError as error:
            raise Failure("infrastructure_" + type(error).__name__) from None
        deadline = time.monotonic() + 100
        try:
            while process.poll() is None:
                if max(os.fstat(out.fileno()).st_size, os.fstat(err.fileno()).st_size) > LIMIT: raise Stop("capture_limit")
                if time.monotonic() > deadline: raise Failure("infrastructure_timeout")
                time.sleep(0.02)
        finally:
            if process.poll() is None: os.killpg(process.pid, signal.SIGKILL); process.wait()
        out.seek(0); err.seek(0)
        stdout, stderr = out.read(LIMIT + 1), err.read(LIMIT + 1)
        if max(len(stdout), len(stderr)) > LIMIT: raise Stop("capture_limit")
        return dict(argv=[str(x) for x in argv], exit_code=process.returncode, stdout=stdout.decode("utf-8", "replace"), stderr=stderr.decode("utf-8", "replace"))

def encode(value, key=""):
    text = json.dumps(value, ensure_ascii=True)
    if "sk-or-" in text or (key and key in text): raise Stop("secret_capture_refused")
    return text

def load_key(root=ROOT):
    key = os.environ.get("OPENROUTER_KEY")
    if key: return key
    for line in (root / ".env").read_text().splitlines():
        name, separator, value = line.partition("=")
        if separator and name.strip() == "OPENROUTER_KEY": return value.strip().strip("'\"")
    raise Failure("missing_OPENROUTER_KEY")

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl): return None

def request(payload, key, events, attempts=4):
    req = urllib.request.Request(URL, data=encode(payload, key).encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer " + key})
    opener = urllib.request.build_opener(NoRedirect())
    limit = 4 * LIMIT if attempts == 1 else LIMIT
    for attempt in range(attempts):
        try:
            with opener.open(req, timeout=120) as response: raw = response.read(limit + 1)
            events.append(dict(http_status=200, response=raw.decode("utf-8", "replace")))
            if len(raw) > limit: raise Stop("cost_unknown:response_limit")
            try: return json.loads(raw)
            except (ValueError, UnicodeError): raise Stop("cost_unknown:invalid_json") from None
        except urllib.error.HTTPError as error:
            raw = error.read(limit + 1)
            error.close()
            events.append(dict(http_status=error.code, response=raw.decode("utf-8", "replace")))
            if len(raw) > limit: raise Stop("cost_unknown:http_error")
            if b'"usage"' in raw:
                if attempts == 1:
                    try: return json.loads(raw)
                    except ValueError: raise Stop("cost_unknown:http_error") from None
                try: cost, *_ = usage(json.loads(raw))
                except ValueError: raise Stop("cost_unknown:http_error") from None
                raise Stop("http_error_usage", cost)
            if attempts == 1 and error.code in (401, 402): raise Stop("provider_refused_key:http_" + str(error.code)) from None
            if attempt == attempts - 1 or not (error.code == 429 or 500 <= error.code < 600): raise Failure("http_" + str(error.code)) from None
            events[-1]["retry_after_seconds"] = 2 ** attempt
            time.sleep(2 ** attempt)
        except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException) as error:
            # Exception text can contain request details; retain only typed transport evidence.
            cause = getattr(error, "reason", None)
            events.append(dict(transport_error=dict(kind=type(error).__name__, cause=type(cause).__name__ if cause is not None else None, errno=getattr(cause or error, "errno", None))))
            raise Stop("cost_unknown:transport_error") from None

def bounded(payload, key, events, seconds):
    # urllib's timeout limits each socket read. OpenRouter sends keep-alive whitespace while a
    # model generates, so one response could last 14 minutes; this is the total deadline.
    box, captured = {}, []
    def call():
        try: box["response"] = request(payload, key, captured, attempts=1)
        except BaseException as error: box["error"] = error
    worker = threading.Thread(target=call, daemon=True); worker.start(); worker.join(seconds)
    events.extend(list(captured))
    if worker.is_alive():
        events.append(dict(transport_error=dict(kind="DeadlineExceeded", cause=None, errno=None), deadline_seconds=seconds))
        raise Stop("cost_unknown:timeout")
    if "error" in box: raise box["error"]
    return box["response"]

def usage(response):
    try:
        record = response["usage"]
        cost = float(record["cost"])
        if type(record["cost"]) is bool or not math.isfinite(cost) or cost < 0: raise ValueError()
    except (KeyError, TypeError, ValueError, OverflowError): raise Stop("cost_unknown:missing_or_invalid_usage") from None
    try:
        tokens = record["prompt_tokens"], record["completion_tokens"]
        if any(type(n) is not int or n < 0 for n in tokens): raise ValueError()
        return cost, *tokens
    except (KeyError, TypeError, ValueError): raise Stop("invalid_usage_tokens", cost) from None

def extract(answer):
    match = re.fullmatch(r"\s*```c[ \t]*\r?\n([\s\S]*?)```\s*", answer)
    if not match or "```" in match[1]: raise Failure("invalid_candidate:fenced_file")
    source = match[1].rstrip() + "\n"
    normalized = re.sub(r"\\\r?\n", "", source)
    if len(source.encode()) > 65536 or re.search(r"#|%:|\?\?=", normalized): raise Failure("invalid_candidate:preprocessor_or_size")
    return source

def messages(prompt, source, feedback):
    return [{"role": "system", "content": prompt}, {"role": "user", "content": "Current program.c:\n```c\n" + source + "\n```" + ("\nFeedback:\n" + feedback if feedback is not None else "")}]

def measure(original, source, spans):
    with tempfile.TemporaryDirectory(prefix="agent-repair-diff-") as scratch:
        before, after = Path(scratch) / "original.c", Path(scratch) / "final.c"
        before.write_text(original); after.write_text(source)
        result = run(["diff", "-U0", before, after])
    if result["exit_code"] not in (0, 1): raise Failure("infrastructure_diff_failed", result)
    if any(len(span) != 2 or any(type(n) is not int for n in span) or not 1 <= span[0] <= span[1] <= len(original.splitlines()) for span in spans): raise Failure("invalid_fix_spans")
    changed, added, removed = [], 0, 0
    for line in result["stdout"].splitlines()[2:]:
        match = re.match(r"@@ -(\d+)(?:,(\d+))? \+", line)
        if match: old = int(match[1]) + (match[2] == "0")
        elif line.startswith("-"): changed.append(old); old += 1; removed += 1
        elif line.startswith("+"): changed.append(max(1, old - 1)); added += 1
    touched = [any(start <= n <= end for n in changed) for start, end in spans]
    inside = sum(any(start <= n <= end for start, end in spans) for n in changed)
    return dict(diff_added=added, diff_removed=removed, changed_inside_spans=inside, spans_touched=touched), result["stdout"]

def prepare(condition, source, row):
    if condition == "c0": return source, None, []
    records, applied_diff = [], ""
    if condition == "c4":
        with tempfile.TemporaryDirectory(prefix="agent-repair-apply-") as scratch:
            path = Path(scratch) / "program.c"; path.write_text(source)
            record = run([EVO, "fix", path, "--apply", "--json"], cwd=scratch)
            try: applied = json.loads(record["stdout"])["applied"]
            except (ValueError, KeyError, TypeError): raise Failure("infrastructure_apply_invalid_json", record) from None
            if type(applied) is not int or applied < 0: raise Failure("infrastructure_apply_invalid_count", record)
            edited = path.read_text()
        if bool(applied) != (edited != source): raise Failure("infrastructure_apply_source_mismatch", record)
        row["auto_applied"] += applied
        _, applied_diff = measure(source, edited, [])
        source, condition, records = edited, "c3", [record]
    current, evidence = feedback(condition, source)
    return source, applied_diff + current, records + evidence

def qualify(receipt, items, models, hashes):
    if receipt.get("schema") != "agent-repair.v2" or receipt.get("plan", {}).get("phase") != "preflight" or receipt.get("stop_reason") != "completed": raise Failure("invalid_preflight_receipt")
    # The population decision is written to predictions-2.md after preflight and frozen by the tag.
    if any(receipt.get("provenance", {}).get("hashes", {}).get(path) != value for path, value in hashes.items() if not path.endswith("predictions-2.md")): raise Failure("preflight_input_mismatch")
    runs, hard = receipt.get("runs", []), {model: [] for model in models}
    if len(runs) != len(items) * len(models) * 3: raise Failure("incomplete_preflight")
    for item in items:
        for model in models:
            rows = [r for r in runs if r.get("item") == item.name and r.get("model_sent") == model]
            # Delivery errors occupy a trial but cannot establish a failed repair.
            outcomes = {("passed", 1, True), ("failed", 1, False), ("response_error", 0, False)}
            if len(rows) != 3 or {r.get("repetition") for r in rows} != {1, 2, 3} or any(r.get("condition") != "c0" or r.get("rounds") != 1 or (r.get("status"), r.get("rounds_scored"), r.get("passed")) not in outcomes for r in rows): raise Failure("incomplete_preflight")
            if sum(r["status"] == "failed" for r in rows) >= 2: hard[model].append(item.name)
    return hard

def native(mode, source, reference=None):
    name = "agent-repair-" + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix="agent-repair-") as scratch:
        work = Path(scratch); work.chmod(0o755)
        (work / "program.c").write_text(source)
        try:
            result = run([CONTAINER, "run", "--name", name, "--platform", "linux/arm64", "--network", "none", "--cpus", "1", "--memory", "512M", "--user", "65534:65534", "-i", "-v", str(work) + ":/input:ro", IMAGE, "python3", "-c", WORKER, mode], data=json.dumps({"reference": reference}).encode())
        finally:
            try: cleaned = subprocess.run([CONTAINER, "delete", "--force", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=ENV, timeout=20)
            except (subprocess.TimeoutExpired, OSError): raise Stop("container_cleanup_failed") from None
        if cleaned.returncode: raise Stop("container_cleanup_failed")
        if result["exit_code"]: raise Failure("infrastructure_container_failed", result)
        try: return json.loads(result["stdout"])
        except ValueError: raise Failure("infrastructure_container_invalid_json", result) from None

def feedback(condition, source):
    if condition != "c3":
        records = native(condition, source)
        return encode([{k: row[k] for k in ("argv", "exit_code", "stderr")} for row in records]), records
    with tempfile.TemporaryDirectory(prefix="agent-repair-evo-") as scratch:
        path = Path(scratch) / "program.c"; path.write_text(source)
        records = [run([EVO, "check", "--json", path], cwd=scratch)]
        try: codes = sorted({d["code"] for line in records[0]["stdout"].splitlines() for d in json.loads(line).get("diagnostics", [])})
        except (ValueError, KeyError, TypeError): raise Failure("infrastructure_evo_invalid_json", records) from None
        records.extend(run([EVO, "explain", code, "--json"], cwd=scratch) for code in codes)
        records.append(run([EVO, "fix", path, "--plan", "--json"], cwd=scratch))
        if any(not row["stdout"].strip() for row in records): raise Failure("infrastructure_evo_missing_json")
        if sum(len(row["stdout"]) + len(row["stderr"]) for row in records) > 4 * LIMIT: raise Stop("capture_limit")
        return "".join(row["stdout"] for row in records), records

def score(source, item):
    records = native("score", source, (item / "reference_main.c").read_text())
    passed = len(records) == 3 and records[-1]["exit_code"] == 0 and records[-1]["stdout"] == (item / "expected.txt").read_text()
    return passed, records

def digest(path): return hashlib.sha256(path.read_bytes()).hexdigest()

def preflight(args, items, models, plan):
    def git(*words):
        result = run(["git", *words])
        if result["exit_code"]: raise Failure("preregistration_git_failure")
        return result["stdout"].strip()
    commit = git("rev-parse", "HEAD")
    is_preflight = getattr(args, "preflight", False)
    if git("status", "--porcelain") or (not is_preflight and git("rev-parse", args.corpus_tag + "^{commit}") != commit): raise Failure("preregistration_requires_clean_tagged_HEAD")
    protocol_path = getattr(args, "protocol", HERE / "protocol.json")
    protocol = json.loads(protocol_path.read_text())
    v2 = protocol["schema"] == "agent-repair.protocol.v2"
    frozen_models = protocol.get("models") or (ROOT / protocol["models_file"]).read_text().split()
    if not set(models) <= set(frozen_models): raise Failure("model_not_preregistered")
    registered = dict(reps=protocol["repetitions"], rounds=protocol["rounds"], temperature=protocol["temperature"], cost_cap=protocol.get("preflight_cost_cap_usd", protocol["reported_cost_cap_usd"]) if is_preflight else protocol["reported_cost_cap_usd"], max_tokens=protocol["max_tokens"], class_counts=protocol["population"], conditions=list(protocol["conditions"]), models=frozen_models, max_requests=protocol["request_limit"])
    if is_preflight:
        if not 0 < plan["cost_cap"] <= registered["cost_cap"]: raise Failure("preflight_cost_cap_exceeds_allowance")
        registered.update(reps=3, rounds=1, conditions=["c0"], max_requests=len(items) * len(frozen_models) * 3, cost_cap=plan["cost_cap"])
    if args.corpus_tag != protocol["corpus_tag"] or any(plan[key] != value for key, value in registered.items()): raise Failure("protocol_parameter_mismatch")
    if args.items.resolve() != (ROOT / protocol.get("items", "tests/fixtures/agent-repair")).resolve(): raise Failure("protocol_items_mismatch")
    paths = [HERE / "repair.py", ROOT / protocol.get("prompt_file", "tools/experiment/prompt.txt"), ROOT / protocol.get("models_file", "tools/experiment/models.txt"), protocol_path] + [ROOT / "contracts/build-environment.json"]
    paths += [item / name for item in items for name in ("program.c", "fixed.c", "reference_main.c", "expected.txt", "origin.md", "class")]
    if v2: paths += [item / "fix-spans.json" for item in items] + [HERE / "predictions-2.md"]
    if v2 and not is_preflight: paths.append(ROOT / protocol["preflight_receipt"])
    if any(path.is_symlink() or not path.resolve().is_relative_to(ROOT) for path in paths): raise Failure("unregistered_input_path")
    if v2 and not is_preflight:
        hard = qualify(json.loads(paths[-1].read_text()), items, models, {str(p.relative_to(ROOT)): digest(p) for p in paths[:-1]})
    tracked = set(git("ls-files", "--", *(str(path.relative_to(ROOT)) for path in paths)).splitlines())
    if any(str(path.relative_to(ROOT)) not in tracked for path in paths): raise Failure("untracked_input")
    if run(["git", "check-ignore", ".env"])["exit_code"]: raise Failure("env_not_ignored")
    image = run([CONTAINER, "image", "inspect", IMAGE])
    if image["exit_code"]: raise Failure("sealed_image_unavailable")
    try: observed = json.loads(image["stdout"])[0]["configuration"]["descriptor"]["digest"]
    except (ValueError, KeyError, IndexError, TypeError): raise Failure("sealed_image_invalid_inspect") from None
    if observed != json.loads((ROOT / "contracts/build-environment.json").read_text())["buildImage"]["executionReference"]: raise Failure("sealed_image_digest_mismatch")
    result = dict(commit=commit, corpus_tag=args.corpus_tag, hashes={str(p.relative_to(ROOT)): digest(p) for p in paths}, evo_sha256=digest(EVO), evocc_sha256=digest(EVO.with_name("evocc")), version=run([EVO, "version"])["stdout"], image=IMAGE, image_inspect=image["stdout"])
    if v2 and not is_preflight: result["preflight_hard"] = hard
    return result

def resume_prefix(previous, output, receipt, order, key):
    old = json.loads((previous / "receipt.json").read_text())
    saved = json.loads((previous / "plan.json").read_text())
    if receipt["schema"] != "agent-repair.v2" or receipt["plan"].get("phase") != "preflight" or old.get("schema") != receipt["schema"] or old.get("plan") != receipt["plan"] or not old.get("stop_reason") or old["stop_reason"] == "completed": raise Failure("invalid_preflight_resume")
    if saved != dict(plan=old["plan"], provenance=old["provenance"], order=order): raise Failure("resume_plan_mismatch")
    before, after = old["provenance"], receipt["provenance"]
    amended = {"tools/experiment/repair.py", "tools/experiment/protocol-2.json"}
    if {k: v for k, v in before["hashes"].items() if k not in amended} != {k: v for k, v in after["hashes"].items() if k not in amended} or any(before.get(k) != after.get(k) for k in ("corpus_tag", "evo_sha256", "evocc_sha256", "version", "image", "image_inspect")): raise Failure("resume_input_mismatch")
    rows = old["runs"]
    if not 0 < len(rows) < len(order): raise Failure("invalid_resume_prefix")
    archive_bytes, raw_bytes = 0, 0
    for index, row in enumerate(rows):
        identity = dict(item=row["item"], model=row["model_sent"], repetition=row["repetition"], condition=row["condition"])
        name = str(index).zfill(4) + ".json.gz"
        if identity != order[index] or row["transcript"] != name: raise Failure("invalid_resume_prefix")
        packed = (previous / name).read_bytes()
        raw = gzip.decompress(packed); encode(json.loads(raw), key)
        archive_bytes += len(packed); raw_bytes += len(raw)
        if raw_bytes >= 400 * 1024 ** 2: raise Stop("capture_budget")
        (output / name).write_bytes(packed)
    # Keep attempted slots unchanged, including errors and unknown per-call costs.
    receipt.update(runs=rows, total_cost=old["total_cost"], continuation=dict(prior_receipt_sha256=digest(previous / "receipt.json"), prior_provenance=before, prior_stop_reason=old["stop_reason"], history=old.get("continuation")))
    shutil.copyfile(previous / "receipt.json", output / "prior-receipt.json")
    return archive_bytes, raw_bytes

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", action="append"); parser.add_argument("--condition", action="append", choices=("c0", "c1", "c2", "c3", "c4"))
    parser.add_argument("--reps", type=int); parser.add_argument("--rounds", type=int)
    parser.add_argument("--items", type=Path); parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--temperature", type=float); parser.add_argument("--cost-cap", "--max-cost-usd", type=float)
    parser.add_argument("--corpus-tag"); parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--protocol", type=Path, default=HERE / "protocol.json"); parser.add_argument("--preflight", action="store_true")
    parser.add_argument("--resume-from", type=Path); parser.add_argument("--parallel", type=int, default=1)
    args = parser.parse_args(argv)
    args.protocol = args.protocol.resolve()
    protocol = json.loads(args.protocol.read_text()); v2 = protocol["schema"] == "agent-repair.protocol.v2"
    if protocol["schema"] not in ("agent-repair.protocol.v1", "agent-repair.protocol.v2"): parser.error("unsupported protocol schema")
    cap_field = "preflight_cost_cap_usd" if args.preflight and "preflight_cost_cap_usd" in protocol else "reported_cost_cap_usd"
    for option, field in (("reps", "repetitions"), ("rounds", "rounds"), ("temperature", "temperature"), ("cost_cap", cap_field), ("corpus_tag", "corpus_tag")):
        if getattr(args, option) is None: setattr(args, option, protocol[field])
    if args.preflight: args.reps, args.rounds, args.condition = 3, 1, ["c0"]
    if not v2 and (args.preflight or any(c in ("c0", "c4") for c in args.condition or [])): parser.error("v2 protocol required")
    if args.resume_from and not (v2 and args.preflight): parser.error("resume is only for unattempted preflight slots")
    args.items = args.items or ROOT / protocol.get("items", "tests/fixtures/agent-repair")
    if min(args.reps, args.rounds) < 1 or not math.isfinite(args.cost_cap) or args.cost_cap <= 0 or not math.isfinite(args.temperature) or args.parallel < 1: parser.error("invalid numeric limits")
    items = sorted(path.parent for path in args.items.resolve().glob("*/program.c"))
    models, conditions = args.model or protocol.get("models") or (ROOT / protocol["models_file"]).read_text().split(), args.condition or list(protocol["conditions"])
    if not items or not models or len(set(models)) != len(models) or len(set(conditions)) != len(conditions): parser.error("empty items/models or duplicate options")
    args.out = args.out.resolve()
    if args.out.is_relative_to(ROOT): parser.error("--out must be outside the repository")
    trials = [(item, model, rep, condition) for rep in range(1, args.reps + 1) for index, item in enumerate(items) for model in models for condition in conditions[(index + rep - 1) % len(conditions):] + conditions[:(index + rep - 1) % len(conditions)]]
    plan = dict(items=len(items), conditions=conditions, models=models, reps=args.reps, rounds=args.rounds, temperature=args.temperature, cost_cap=args.cost_cap, max_tokens=protocol["max_tokens"], max_requests=len(trials) * args.rounds, max_http_attempts=len(trials) * args.rounds * (2 if v2 else 4))
    if v2: plan["phase"] = "preflight" if args.preflight else "measurement"
    classes = [(item / "class").read_text().strip() for item in items]
    plan["class_counts"] = {name: classes.count(name) for name in sorted(set(classes))}
    if args.dry_run: print(encode(plan)); return 0
    if args.out.exists(): raise Failure("output_directory_already_exists")
    if shutil.disk_usage(ROOT).free < 25 * 1024 ** 3: raise Stop("disk_below_25_GiB")
    provenance = preflight(args, items, models, plan)
    key, prompt = load_key(), (ROOT / protocol.get("prompt_file", "tools/experiment/prompt.txt")).read_text()
    c0 = ""
    if v2:
        prompt, marker, c0 = prompt.partition("\n--- C0 ONLY ---\n")
        if not marker or not c0.strip(): raise Failure("invalid_c0_prompt")
    args.out.mkdir(mode=0o700, parents=True)
    receipt = dict(schema="agent-repair.v2" if v2 else "agent-repair.v1", date=datetime.datetime.now(datetime.timezone.utc).isoformat(), provenance=provenance, plan=plan, runs=[], total_cost=0.0, stop_reason=None, acceptance=False, parallel=args.parallel)
    order = [dict(item=item.name, model=model, repetition=rep, condition=condition) for item, model, rep, condition in trials]
    archive_bytes, raw_bytes = resume_prefix(args.resume_from, args.out, receipt, order, key) if args.resume_from else (0, 0)
    lock, state, failures = threading.RLock(), dict(next=len(receipt["runs"]), archive=archive_bytes, raw=raw_bytes), []
    reserve, timeout = protocol.get("unknown_cost_reserve_usd"), protocol.get("limits", {}).get("http_timeout_seconds", 120)
    if v2 and reserve is not None: receipt.update(unknown_cost_attempts=0, unknown_cost_reserve=0.0)
    def spent(): return receipt["total_cost"] + receipt.get("unknown_cost_reserve", 0)
    def halt(reason):
        with lock: receipt["stop_reason"] = receipt["stop_reason"] or reason
        return reason
    def charge(row, amount):
        with lock: row["cost"] += amount; receipt["total_cost"] += amount
    def unknown(reason):
        # Per-call cost stays null. A registered reserve replaces the old stop and counts against the cap.
        if not v2 or reserve is None: return halt(reason)
        with lock: receipt["unknown_cost_attempts"] += 1; receipt["unknown_cost_reserve"] += reserve
    (args.out / "plan.json").write_text(encode(dict(plan=plan, provenance=provenance, order=order), key))
    (args.out / "receipt.json").write_text(encode(receipt, key))
    def run_slot(index, item, model, rep, condition):
        started = time.monotonic()
        row = dict(item=item.name, **{"class": (item / "class").read_text().strip()}, condition=condition, model_sent=model, repetition=rep, rounds=0, rounds_scored=0, passed=False, status="failed", error=None, tokens_in=0, tokens_out=0, cost=0.0, calls=[])
        row["synthetic"] = "written for this corpus" in (item / "origin.md").read_text().lower()
        if v2: row["auto_applied"] = 0
        row.update(start_order=index, started_at=datetime.datetime.now(datetime.timezone.utc).isoformat())
        events = []
        source = original = ""
        try:
            source = original = (item / "program.c").read_text()
            if v2: source, current, tools = prepare(condition, source, row)
            else: current, tools = feedback(condition, source)
            events.append(dict(phase="initial", source=source, feedback=current, tools=tools))
            if v2 and source != original:
                row["passed"], events[-1]["score"] = score(source, item)
                if row["passed"]: row["status"] = "passed"
            for number in range(1, args.rounds + 1):
                if row["passed"]: break
                if spent() >= args.cost_cap: raise Stop("reported_cost_cap")
                if receipt["stop_reason"]: raise Stop(receipt["stop_reason"])
                if shutil.disk_usage(ROOT).free < 15 * 1024 ** 3: raise Stop("disk_below_15_GiB")
                row["rounds"] = number
                payload = dict(model=model, temperature=args.temperature, max_tokens=plan["max_tokens"], messages=messages(prompt + ("\n" + c0 if condition == "c0" else ""), source, current))
                for attempt in range(1, 3 if v2 else 2):
                    if spent() >= args.cost_cap: raise Stop("reported_cost_cap")
                    event = dict(round=number, request=payload, http=[]); events.append(event)
                    call = dict(round=number, model_sent=model, reported_model=None, provider=None, cost=None, tokens_in=None, tokens_out=None, score_passed=None)
                    if v2: call.update(attempt=attempt, response_error=None); row["calls"].append(call)
                    response, clock = None, time.monotonic()
                    try:
                        response = bounded(payload, key, event["http"], timeout) if v2 else request(payload, key, event["http"])
                        cost, token_in, token_out = usage(response)
                        charge(row, cost)
                        row["tokens_in"] += token_in; row["tokens_out"] += token_out
                        call.update(cost=cost, tokens_in=token_in, tokens_out=token_out)
                        encode(response, key)
                        call.update(reported_model=response.get("model"), provider=response.get("provider"))
                        if not v2: row["calls"].append(call)
                        if v2 and event["http"] and event["http"][-1]["http_status"] != 200: raise Failure("http_" + str(event["http"][-1]["http_status"]))
                        try: answer = response["choices"][0]["message"]["content"]
                        except (KeyError, IndexError, TypeError): raise Failure("malformed_response") from None
                        if v2 and response["choices"][0].get("finish_reason") == "length": raise Failure("output_budget_exhausted")
                        if not isinstance(answer, str): raise Failure("malformed_response")
                        source = extract(answer)
                        break
                    except Failure as error:
                        if not v2 or (isinstance(error, Stop) and not str(error).startswith(("cost_unknown:", "invalid_usage_tokens"))): raise
                        call["response_error"] = str(error)
                        if str(error).startswith("cost_unknown:"): unknown(str(error))
                        elif isinstance(error, Stop): call["cost"] = error.cost; charge(row, error.cost)
                        elif call["cost"] is None: unknown("cost_unknown:" + str(error))
                        for field, name in (("tokens_in", "prompt_tokens"), ("tokens_out", "completion_tokens")):
                            value = response["usage"].get(name) if isinstance(response, dict) and isinstance(response.get("usage"), dict) else None
                            if call[field] is None and type(value) is int and value >= 0: call[field] = value; row[field] += value
                        if isinstance(response, dict):
                            encode(response, key)
                            call.update(reported_model=response.get("model"), provider=response.get("provider"))
                        if attempt == 2: raise Failure("response_error") from None
                    finally: call["seconds"] = round(time.monotonic() - clock, 3)
                if v2: source, current, tools = prepare(condition, source, row)
                else: current, tools = feedback(condition, source)
                event.update(source_sha256=hashlib.sha256(source.encode()).hexdigest(), feedback=current, tools=tools)
                row["passed"], event["score"] = score(source, item)
                row["rounds_scored"] += 1
                event["score_passed"] = row["calls"][-1]["score_passed"] = row["passed"]
                if row["passed"]: row["status"] = "passed"; break
        except Stop as error:
            charge(row, error.cost)
            row["status"] = "stopped"
            row["error"] = halt(str(error))
        except (Failure, OSError) as error:
            row["status"] = "response_error" if v2 and str(error) == "response_error" else "error"
            row["error"] = str(error) if isinstance(error, Failure) else type(error).__name__
            if getattr(error, "evidence", None) is not None: events.append(dict(error=row["error"], tools=error.evidence))
        except KeyboardInterrupt:
            row["status"] = "stopped"; row["error"] = halt("interrupted_cost_may_be_unknown")
        if v2:
            try:
                metrics, delta = measure(original, source, json.loads((item / "fix-spans.json").read_text())["spans"])
                row.update(metrics); events.append(dict(phase="final", diff=delta))
            except (Failure, OSError, ValueError, KeyError, TypeError): row["error"] = halt("final_metrics_failed")
        try:
            serialized = encode(events, key).encode()
            packed = gzip.compress(serialized, mtime=0)
        except Stop:
            row["error"] = halt("secret_capture_refused"); serialized, packed = b"", gzip.compress(b'[]', mtime=0)
        name = str(index).zfill(4) + ".json.gz"
        (args.out / name).write_bytes(packed); row.update(transcript=name, seconds=round(time.monotonic() - started, 3))
        with lock:
            state["archive"] += len(packed); state["raw"] += len(serialized)
            receipt["runs"].append(row); receipt["runs"].sort(key=lambda r: r.get("start_order", -1))
            if not v2 and state["archive"] >= 9 * 1024 ** 2: receipt["stop_reason"] = "transcript_budget"
            if state["raw"] >= 400 * 1024 ** 2: receipt["stop_reason"] = "capture_budget"
            receipt["unattempted"] = len(trials) - len(receipt["runs"])
            (args.out / "receipt.json").write_text(encode(receipt, key))
    def worker():
        # Slots start in registered order under the lock, so the attempted slots stay a prefix.
        while True:
            with lock:
                if receipt["stop_reason"] or state["next"] >= len(trials): return
                if spent() >= args.cost_cap: receipt["stop_reason"] = "reported_cost_cap"; return
                index = state["next"]; state["next"] += 1
            try: run_slot(index, *trials[index])
            except BaseException as error: failures.append(error); halt("harness_" + type(error).__name__); return
    threads = [threading.Thread(target=worker, daemon=True) for _ in range(args.parallel)]
    for thread in threads: thread.start()
    try:
        for thread in threads:
            while thread.is_alive(): thread.join(1)
    except KeyboardInterrupt:
        halt("interrupted_cost_may_be_unknown")
        for thread in threads: thread.join()
    if receipt["stop_reason"] is None: receipt["stop_reason"] = "completed" if len(receipt["runs"]) == len(trials) else "reported_cost_cap"
    receipt["unattempted"] = len(trials) - len(receipt["runs"])
    receipt["acceptance"] = receipt["stop_reason"] in ("completed", "reported_cost_cap") and all(row["error"] in (None, "reported_cost_cap") or (v2 and not args.preflight and row["status"] == "response_error") for row in receipt["runs"])
    (args.out / "receipt.json").write_text(encode(receipt, key))
    if failures: raise failures[0]
    print(encode(dict(runs=len(receipt["runs"]), total_cost=receipt["total_cost"], stop_reason=receipt["stop_reason"])))
    return 0 if receipt["stop_reason"] in ("completed", "reported_cost_cap") else 1

if __name__ == "__main__":
    try: raise SystemExit(main())
    except (Failure, OSError) as error: raise SystemExit(str(error) if isinstance(error, Failure) else type(error).__name__)
