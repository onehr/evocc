#!/usr/bin/env python3
"""Preregistered repair experiment; reported-cost stopping is not an absolute spend guarantee."""
import argparse, datetime, gzip, hashlib, http.client, json, math, os, re, shutil, signal, subprocess, tempfile, time, urllib.error, urllib.request, uuid
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

def request(payload, key, events):
    req = urllib.request.Request(URL, data=encode(payload, key).encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer " + key})
    opener = urllib.request.build_opener(NoRedirect())
    for attempt in range(4):
        try:
            with opener.open(req, timeout=120) as response: raw = response.read(LIMIT + 1)
            events.append(dict(http_status=200, response=raw.decode("utf-8", "replace")))
            if len(raw) > LIMIT: raise Stop("cost_unknown:response_limit")
            try: return json.loads(raw)
            except (ValueError, UnicodeError): raise Stop("cost_unknown:invalid_json") from None
        except urllib.error.HTTPError as error:
            raw = error.read(LIMIT + 1)
            error.close()
            events.append(dict(http_status=error.code, response=raw.decode("utf-8", "replace")))
            if len(raw) > LIMIT: raise Stop("cost_unknown:http_error")
            if b'"usage"' in raw:
                try: cost, *_ = usage(json.loads(raw))
                except ValueError: raise Stop("cost_unknown:http_error") from None
                raise Stop("http_error_usage", cost)
            if attempt == 3 or not (error.code == 429 or 500 <= error.code < 600): raise Failure("http_" + str(error.code)) from None
            events[-1]["retry_after_seconds"] = 2 ** attempt
            time.sleep(2 ** attempt)
        except (urllib.error.URLError, TimeoutError, OSError, http.client.HTTPException):
            raise Stop("cost_unknown:transport_error") from None

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
    return [{"role": "system", "content": prompt}, {"role": "user", "content": "Current program.c:\n```c\n" + source + "\n```\nFeedback:\n" + feedback}]

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
    if git("status", "--porcelain") or git("rev-parse", args.corpus_tag + "^{commit}") != commit: raise Failure("preregistration_requires_clean_tagged_HEAD")
    frozen_models = (HERE / "models.txt").read_text().split()
    if not set(models) <= set(frozen_models): raise Failure("model_not_preregistered")
    protocol = json.loads((HERE / "protocol.json").read_text())
    registered = dict(reps=protocol["repetitions"], rounds=protocol["rounds"], temperature=protocol["temperature"], cost_cap=protocol["reported_cost_cap_usd"], max_tokens=protocol["max_tokens"], class_counts=protocol["population"], conditions=list(protocol["conditions"]), models=frozen_models, max_requests=protocol["request_limit"])
    if args.corpus_tag != protocol["corpus_tag"] or any(plan[key] != value for key, value in registered.items()): raise Failure("protocol_parameter_mismatch")
    if args.items.resolve() != (ROOT / protocol.get("items", "tests/fixtures/agent-repair")).resolve(): raise Failure("protocol_items_mismatch")
    paths = [HERE / name for name in ("repair.py", "prompt.txt", "models.txt", "protocol.json")] + [ROOT / "contracts/build-environment.json"]
    paths += [item / name for item in items for name in ("program.c", "fixed.c", "reference_main.c", "expected.txt", "origin.md", "class")]
    if any(path.is_symlink() or not path.resolve().is_relative_to(ROOT) for path in paths): raise Failure("unregistered_input_path")
    tracked = set(git("ls-files").splitlines())
    if any(str(path.relative_to(ROOT)) not in tracked for path in paths): raise Failure("untracked_input")
    if run(["git", "check-ignore", ".env"])["exit_code"]: raise Failure("env_not_ignored")
    image = run([CONTAINER, "image", "inspect", IMAGE])
    if image["exit_code"]: raise Failure("sealed_image_unavailable")
    try: observed = json.loads(image["stdout"])[0]["configuration"]["descriptor"]["digest"]
    except (ValueError, KeyError, IndexError, TypeError): raise Failure("sealed_image_invalid_inspect") from None
    if observed != json.loads((ROOT / "contracts/build-environment.json").read_text())["buildImage"]["executionReference"]: raise Failure("sealed_image_digest_mismatch")
    return dict(commit=commit, corpus_tag=args.corpus_tag, hashes={str(p.relative_to(ROOT)): digest(p) for p in paths}, evo_sha256=digest(EVO), evocc_sha256=digest(EVO.with_name("evocc")), version=run([EVO, "version"])["stdout"], image=IMAGE, image_inspect=image["stdout"])

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", action="append"); parser.add_argument("--condition", action="append", choices=("c1", "c2", "c3"))
    parser.add_argument("--reps", type=int, default=3); parser.add_argument("--rounds", type=int, default=3)
    parser.add_argument("--items", type=Path, default=ROOT / "tests/fixtures/agent-repair"); parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--temperature", type=float, default=0); parser.add_argument("--cost-cap", type=float, default=5)
    parser.add_argument("--corpus-tag", default="agent-repair-corpus-1"); parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args(argv)
    if min(args.reps, args.rounds) < 1 or not math.isfinite(args.cost_cap) or args.cost_cap <= 0 or not math.isfinite(args.temperature): parser.error("invalid numeric limits")
    items = sorted(path.parent for path in args.items.resolve().glob("*/program.c"))
    models, conditions = args.model or (HERE / "models.txt").read_text().split(), args.condition or ["c1", "c2", "c3"]
    if not items or not models or len(set(models)) != len(models) or len(set(conditions)) != len(conditions): parser.error("empty items/models or duplicate options")
    args.out = args.out.resolve()
    if args.out.is_relative_to(ROOT): parser.error("--out must be outside the repository")
    trials = [(item, model, rep, condition) for rep in range(1, args.reps + 1) for index, item in enumerate(items) for model in models for condition in conditions[(index + rep - 1) % len(conditions):] + conditions[:(index + rep - 1) % len(conditions)]]
    plan = dict(items=len(items), conditions=conditions, models=models, reps=args.reps, rounds=args.rounds, temperature=args.temperature, cost_cap=args.cost_cap, max_tokens=MAX_TOKENS, max_requests=len(trials) * args.rounds, max_http_attempts=len(trials) * args.rounds * 4)
    classes = [(item / "class").read_text().strip() for item in items]
    plan["class_counts"] = {name: classes.count(name) for name in sorted(set(classes))}
    if args.dry_run: print(encode(plan)); return 0
    if args.out.exists(): raise Failure("output_directory_already_exists")
    if shutil.disk_usage(ROOT).free < 25 * 1024 ** 3: raise Stop("disk_below_25_GiB")
    provenance = preflight(args, items, models, plan)
    key, prompt = load_key(), (HERE / "prompt.txt").read_text()
    args.out.mkdir(mode=0o700, parents=True)
    receipt = dict(schema="agent-repair.v1", date=datetime.datetime.now(datetime.timezone.utc).isoformat(), provenance=provenance, plan=plan, runs=[], total_cost=0.0, stop_reason=None, acceptance=False)
    order = [dict(item=item.name, model=model, repetition=rep, condition=condition) for item, model, rep, condition in trials]
    (args.out / "plan.json").write_text(encode(dict(plan=plan, provenance=provenance, order=order), key))
    (args.out / "receipt.json").write_text(encode(receipt, key))
    archive_bytes, raw_bytes = 0, 0
    for index, (item, model, rep, condition) in enumerate(trials):
        if receipt["total_cost"] >= args.cost_cap: receipt["stop_reason"] = "reported_cost_cap"; break
        row = dict(item=item.name, **{"class": (item / "class").read_text().strip()}, condition=condition, model_sent=model, repetition=rep, rounds=0, rounds_scored=0, passed=False, status="failed", error=None, tokens_in=0, tokens_out=0, cost=0.0, calls=[])
        row["synthetic"] = "written for this corpus" in (item / "origin.md").read_text().lower()
        events = []
        try:
            source = (item / "program.c").read_text()
            current, tools = feedback(condition, source); events.append(dict(phase="initial", source=source, feedback=current, tools=tools))
            for number in range(1, args.rounds + 1):
                if receipt["total_cost"] >= args.cost_cap: raise Stop("reported_cost_cap")
                if shutil.disk_usage(ROOT).free < 15 * 1024 ** 3: raise Stop("disk_below_15_GiB")
                row["rounds"] = number
                payload = dict(model=model, temperature=args.temperature, max_tokens=MAX_TOKENS, messages=messages(prompt, source, current))
                event = dict(round=number, request=payload, http=[]); events.append(event)
                response = request(payload, key, event["http"])
                cost, token_in, token_out = usage(response)
                row["cost"] += cost; receipt["total_cost"] += cost
                row["tokens_in"] += token_in; row["tokens_out"] += token_out
                encode(response, key)
                row["calls"].append(dict(round=number, model_sent=model, reported_model=response.get("model"), provider=response.get("provider"), cost=cost, tokens_in=token_in, tokens_out=token_out, score_passed=None))
                try: answer = response["choices"][0]["message"]["content"]
                except (KeyError, IndexError, TypeError): raise Failure("malformed_response") from None
                if not isinstance(answer, str): raise Failure("malformed_response")
                source = extract(answer)
                current, tools = feedback(condition, source); event.update(source_sha256=hashlib.sha256(source.encode()).hexdigest(), feedback=current, tools=tools)
                row["passed"], event["score"] = score(source, item)
                row["rounds_scored"] += 1
                event["score_passed"] = row["calls"][-1]["score_passed"] = row["passed"]
                if row["passed"]: row["status"] = "passed"; break
        except Stop as error:
            row["cost"] += error.cost; receipt["total_cost"] += error.cost
            row["status"] = "stopped"
            row["error"] = receipt["stop_reason"] = str(error)
        except (Failure, OSError) as error:
            row["status"] = "error"
            row["error"] = str(error) if isinstance(error, Failure) else type(error).__name__
            if getattr(error, "evidence", None) is not None: events.append(dict(error=row["error"], tools=error.evidence))
        except KeyboardInterrupt:
            row["status"] = "stopped"; row["error"] = receipt["stop_reason"] = "interrupted_cost_may_be_unknown"
        try:
            serialized = encode(events, key).encode(); raw_bytes += len(serialized)
            packed = gzip.compress(serialized, mtime=0)
        except Stop:
            row["error"] = receipt["stop_reason"] = "secret_capture_refused"; packed = gzip.compress(b'[]', mtime=0)
        archive_bytes += len(packed)
        name = str(index).zfill(4) + ".json.gz"
        (args.out / name).write_bytes(packed); row["transcript"] = name
        receipt["runs"].append(row)
        if archive_bytes >= 9 * 1024 ** 2: receipt["stop_reason"] = "transcript_budget"
        if raw_bytes >= 400 * 1024 ** 2: receipt["stop_reason"] = "capture_budget"
        receipt["unattempted"] = len(trials) - len(receipt["runs"])
        (args.out / "receipt.json").write_text(encode(receipt, key))
        if receipt["stop_reason"]: break
    if receipt["stop_reason"] is None: receipt["stop_reason"] = "completed" if len(receipt["runs"]) == len(trials) else "reported_cost_cap"
    receipt["unattempted"] = len(trials) - len(receipt["runs"])
    receipt["acceptance"] = receipt["stop_reason"] in ("completed", "reported_cost_cap") and all(row["error"] in (None, "reported_cost_cap") for row in receipt["runs"])
    (args.out / "receipt.json").write_text(encode(receipt, key))
    print(encode(dict(runs=len(receipt["runs"]), total_cost=receipt["total_cost"], stop_reason=receipt["stop_reason"])))
    return 0 if receipt["stop_reason"] in ("completed", "reported_cost_cap") else 1

if __name__ == "__main__":
    try: raise SystemExit(main())
    except (Failure, OSError) as error: raise SystemExit(str(error) if isinstance(error, Failure) else type(error).__name__)
