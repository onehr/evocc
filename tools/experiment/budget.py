#!/usr/bin/env python3
"""Run Protocol A of P17a: one response per trial from one pinned provider.

The cost cap applies to the reported cost plus a reserve for each attempt of
unknown cost. It is not an absolute spend guarantee.
"""
import argparse
import datetime
import gzip
import hashlib
import http.client
import importlib.util
import json
import platform
import random
import re
import shutil
import tarfile
import tempfile
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
# Reuse the frozen P17 functions; never copy them.
_SPEC = importlib.util.spec_from_file_location("repair", HERE / "repair.py")
repair = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(repair)
Failure, Stop = repair.Failure, repair.Stop
ROOT = repair.ROOT
_HISTORY = importlib.util.spec_from_file_location("history_map", ROOT / "tools/history_map.py")
history_map = importlib.util.module_from_spec(_HISTORY)
_HISTORY.loader.exec_module(history_map)
PROTOCOL = HERE / "protocol-6.json"
URL = repair.URL
ENDPOINTS = "https://openrouter.ai/api/v1/models/{}/endpoints"
SCRATCH = "<scratch>"
GIB = 1024 ** 3


def sha(data):
    """Return the SHA-256 of bytes, or of text encoded as UTF-8."""
    return hashlib.sha256(data if isinstance(data, bytes) else data.encode()).hexdigest()


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def secret(data, key):
    return b"sk-or-" in data or (bool(key) and key.encode() in data)


def write(path, value, key=""):
    repair.encode(value, key)  # Refuses the key and the credential prefix.
    path.write_text(json.dumps(value, indent=2, ensure_ascii=True) + "\n")


def load(path=PROTOCOL):
    protocol = json.loads(Path(path).read_text())
    if protocol.get("schema") != "agent-repair.protocol.a1":
        raise Failure("unsupported_protocol")
    return protocol


def items(protocol):
    root = ROOT / protocol["items_root"]
    return {name: root / name for name in protocol["items"]}


def git(*words):
    result = repair.run(["git", *words])
    if result["exit_code"]:
        raise Failure("git_failed:" + words[0])
    return result["stdout"].strip()


def fresh(out):
    """Refuse an existing output path and a tracked location in the repository."""
    out = Path(out).resolve()
    if out.exists():
        raise Failure("output_directory_already_exists")
    if out.is_relative_to(ROOT) and repair.run(["git", "check-ignore", "-q", str(out)])["exit_code"]:
        raise Failure("output_must_be_ignored_or_outside_the_repository")
    return out


def hashes():
    return dict(evo_sha256=repair.digest(repair.EVO), evocc_sha256=repair.digest(repair.EVO.with_name("evocc")))


def compiler():
    return dict(hashes(), version=repair.run([repair.EVO, "version"])["stdout"])


def sealed():
    """Return the digest of the sealed image after it matches the build environment."""
    image = repair.run([repair.CONTAINER, "image", "inspect", repair.IMAGE])
    try:
        digest = json.loads(image["stdout"])[0]["configuration"]["descriptor"]["digest"]
    except (ValueError, KeyError, IndexError, TypeError):
        raise Failure("sealed_image_unavailable") from None
    environment = json.loads((ROOT / "contracts/build-environment.json").read_text())
    if digest != environment["buildImage"]["executionReference"]:
        raise Failure("sealed_image_digest_mismatch")
    return digest


def scratch_of(records):
    """Return the scratch directory of one feedback run: the parent of the checked program.c."""
    return str(Path(records[0]["argv"][-1]).parent)


def frozen(text, scratch, reference):
    """Compare feedback with the P17 reference after scratch-path replacement."""
    return text.replace(scratch, SCRATCH) == reference["feedback"].replace(reference["scratch"], SCRATCH)


def references(protocol):
    """Return the P17 condition c3, repetition 1 feedback that each round 1 request sent."""
    p17 = protocol["p17"]
    receipt, archive = ROOT / p17["receipt"], ROOT / p17["transcripts"]
    if repair.digest(receipt) != p17["receipt_sha256"] or repair.digest(archive) != p17["transcripts_sha256"]:
        raise Failure("p17_reference_hash_mismatch")
    runs = json.loads(receipt.read_text())["runs"]
    result = {}
    # Read the members in memory; nothing from the archive reaches the file system.
    with tarfile.open(archive, "r:gz") as bundle:
        for name in protocol["items"]:
            rows = [r for r in runs if r["item"] == name and r["condition"] == "c3" and r["repetition"] == 1]
            if len(rows) != 1:
                raise Failure("p17_reference_missing")
            events = json.loads(gzip.decompress(bundle.extractfile(rows[0]["transcript"]).read()))
            initial, first = events[0], events[1]
            sent = first["request"]["messages"][1]["content"]
            if initial.get("phase") != "initial" or first.get("round") != 1 or not sent.endswith("\nFeedback:\n" + initial["feedback"]):
                raise Failure("p17_reference_not_round_1")
            result[name] = dict(transcript=rows[0]["transcript"], feedback=initial["feedback"],
                                scratch=scratch_of(initial["tools"]), exit_codes=[t["exit_code"] for t in initial["tools"]])
    return result


def feedback(protocol, arm, source, reference):
    """Return the arm's feedback, the evo records and the scratch directory; stop on changed G0 bytes."""
    text, records = repair.feedback("c3", source)
    scratch = scratch_of(records)
    if not frozen(text, scratch, reference):
        raise Stop("frozen_feedback_changed")
    if arm == "G1":
        explain = records[1:-1]
        if len(explain) != 1 or explain[0]["argv"][-2:] != ["E3004", "--json"]:
            raise Stop("g1_needs_one_E3004_explain_record")
        text = records[0]["stdout"] + protocol["g1_explain_line"] + "\n" + records[-1]["stdout"]
    elif arm != "G0":
        raise Failure("unknown_arm")
    return text, records, scratch


def preflight(protocol, protocol_path):
    """Check the frozen inputs and return their provenance. Makes no model request."""
    prompt = ROOT / protocol["prompt_file"]
    if repair.digest(prompt) != protocol["prompt_sha256"]:
        raise Failure("prompt_changed")
    # A registered protocol keeps the commit it recorded; the history rewrite maps name its successor.
    if git("rev-parse", protocol["corpus_tag"] + "^{commit}") != history_map.current(protocol["corpus_commit"]):
        raise Failure("corpus_tag_moved")
    corpus = [path / name for path in items(protocol).values() for name in protocol["item_files"]]
    if repair.run(["git", "diff", "--quiet", protocol["corpus_tag"], "--", *(str(p.relative_to(ROOT)) for p in corpus)])["exit_code"]:
        raise Failure("corpus_differs_from_tag")
    if repair.run(["git", "check-ignore", "-q", ".env"])["exit_code"]:
        raise Failure("env_not_ignored")
    digest = sealed()
    inputs = [HERE / "budget.py", HERE / "repair.py", Path(protocol_path).resolve(), prompt,
              ROOT / "contracts/build-environment.json", ROOT / protocol["p17"]["receipt"],
              ROOT / protocol["p17"]["transcripts"]] + corpus
    names = [str(path.relative_to(ROOT)) for path in inputs]
    status = repair.run(["git", "status", "--porcelain", "--untracked-files=all", "--", *names])
    return dict(commit=git("rev-parse", "HEAD"), uncommitted_inputs=status["stdout"].splitlines(),
                hashes={name: repair.digest(path) for name, path in zip(names, inputs)},
                image=repair.IMAGE, image_digest=digest, scratch_parent=tempfile.gettempdir(),
                python=platform.python_version(), **compiler())


def endpoints(protocol, out, label, key):
    """Keep the public endpoint list, fetched without credentials, and the pinned provider's entries."""
    record = dict(label=label, time=now())
    try:
        with urllib.request.urlopen(ENDPOINTS.format(protocol["model"]), timeout=30) as response:
            body = response.read(protocol["harness"]["body_limit_bytes"] + 1)
        if secret(body, key):
            raise Stop("secret_capture_refused")
        name = "endpoints-" + label + ".json"
        (out / name).write_bytes(body)
        rows = json.loads(body)["data"]["endpoints"]
        fields = ("name", "tag", "quantization", "max_completion_tokens", "pricing", "status")
        record.update(file=name, sha256=sha(body), entries=[{k: row.get(k) for k in fields} for row in rows
                                                            if row.get("provider_name") == protocol["provider"]["name"]])
    except (OSError, ValueError, KeyError, TypeError, http.client.HTTPException) as error:
        record["error"] = type(error).__name__
    return record


def send(data, key, harness):
    """Make one HTTP attempt with a total deadline. Return the status and raw body, or a transport error."""
    request = urllib.request.Request(URL, data=data, headers={"Content-Type": "application/json", "Authorization": "Bearer " + key})
    box = {}

    def call():
        opener = urllib.request.build_opener(repair.NoRedirect())
        try:
            try:
                with opener.open(request, timeout=harness["socket_timeout_seconds"]) as response:
                    box["status"], box["body"] = response.status, response.read(harness["body_limit_bytes"] + 1)
            except urllib.error.HTTPError as error:
                box["status"], box["body"] = error.code, error.read(harness["body_limit_bytes"] + 1)
                error.close()
        except (OSError, http.client.HTTPException) as error:
            # URLError and timeouts are OSError. Exception text can contain request details; keep only types.
            cause = getattr(error, "reason", None)
            box["transport"] = dict(kind=type(error).__name__, cause=type(cause).__name__ if cause is not None else None,
                                    errno=getattr(cause or error, "errno", None))

    # OpenRouter sends keep-alive whitespace while the model works, so the socket timeout alone never ends a slow body.
    worker = threading.Thread(target=call, daemon=True)
    worker.start()
    worker.join(harness["deadline_seconds"])
    if worker.is_alive():
        return dict(transport=dict(kind="DeadlineExceeded", cause=None, errno=None))
    return dict(box)


def classify(result, limit):
    """Return (verdict, reason, parsed body) for one attempt. The verdict is response, retry or stop."""
    if "transport" in result:
        return "retry", "transport_error", None
    status, body = result["status"], result["body"]
    if len(body) > limit:
        return "stop", "cost_unknown:response_limit", None
    try:
        parsed = json.loads(body)
    except ValueError:
        parsed = None
    code, where = status, "http"
    if status == 200:
        if not isinstance(parsed, dict):
            return "stop", "cost_unknown:invalid_json", None
        if "choices" in parsed or not isinstance(parsed.get("error"), dict):
            return "response", None, parsed
        # The status was sent before the model failed; the error object carries the real code.
        code, where = parsed["error"].get("code"), "embedded"
    reason = "%s_%s" % (where, code)
    if code in (401, 402, 403):
        return "stop", "provider_refused:" + reason, parsed
    if code == 429 or (type(code) is int and 500 <= code < 600):
        return "retry", reason, parsed
    return "stop", reason, parsed


def error_cost(parsed):
    """Return the reported cost of an error body, or None when it has no valid usage."""
    if not isinstance(parsed, dict) or not isinstance(parsed.get("usage"), dict):
        return None
    try:
        return repair.usage(parsed)[0]
    except Stop as error:
        return error.cost if str(error) == "invalid_usage_tokens" else None


def counts(response):
    """Validate the usage record of a response. Missing or invalid usage or cost stops the run."""
    cost, prompt, completion = repair.usage(response)
    details = response["usage"].get("completion_tokens_details")
    reasoning = details.get("reasoning_tokens") if isinstance(details, dict) else None
    if type(reasoning) is not int or not 0 <= reasoning <= completion:
        raise Stop("invalid_usage_tokens", cost)
    cached = response["usage"].get("prompt_tokens_details")
    return dict(cost=cost, prompt_tokens=prompt, completion_tokens=completion, reasoning_tokens=reasoning,
                answer_tokens=completion - reasoning, cached_tokens=cached.get("cached_tokens") if isinstance(cached, dict) else None)


def judge(response):
    """Apply the delivered and valid definitions. Return the outcome fields and the candidate."""
    choices = response.get("choices")
    choice = choices[0] if isinstance(choices, list) and choices and isinstance(choices[0], dict) else {}
    message = choice.get("message") if isinstance(choice.get("message"), dict) else {}
    answer, thought = message.get("content"), message.get("reasoning")
    fields = dict(finish_reason=choice.get("finish_reason"), native_finish_reason=choice.get("native_finish_reason"),
                  delivered=False, valid=False, undelivered=None, invalid=None,
                  answer_sha256=sha(answer) if isinstance(answer, str) else None,
                  reasoning_sha256=sha(thought) if isinstance(thought, str) else None)
    candidate = None
    if not choice:
        fields["undelivered"] = "malformed_response"
    elif fields["finish_reason"] != "stop":
        fields["undelivered"] = "finish_" + str(fields["finish_reason"])
    elif not isinstance(answer, str):
        fields["undelivered"] = "malformed_response"
    else:
        try:
            candidate = repair.extract(answer)  # The P17 parse rule, then the candidate profile.
            fields.update(delivered=True, valid=True)
        except Failure as error:
            if str(error) == "invalid_candidate:fenced_file":
                fields["undelivered"] = str(error)
            else:
                fields.update(delivered=True, invalid=str(error))
    return fields, candidate


def reserve(protocol, max_tokens):
    """Return the planning maximum of one attempt: the cap charge for an attempt of unknown cost."""
    basis = protocol["harness"]["unknown_cost_reserve"]
    return (basis["input_tokens"] * basis["input_usd_per_million"] + max_tokens * basis["output_usd_per_million"]) / 1e6


class Run:
    """The cost, the unknown-cost reserve, the capture size and the stop reason of one trial list."""

    def __init__(self, protocol, out, cap, key):
        self.protocol, self.out, self.cap, self.key = protocol, out, cap, key
        self.cost, self.reserve, self.unknown, self.captured, self.stop = 0.0, 0.0, 0, 0, None

    def spent(self):
        return self.cost + self.reserve

    def halt(self, reason):
        self.stop = self.stop or reason

    def charge(self, row, call, amount):
        call["cost"] = amount
        row["cost"] += amount
        self.cost += amount


def trial(run, spec, refs, prompt, start):
    """Run one trial: feedback, one request with at most one retry, then the outcome."""
    protocol, key, harness = run.protocol, run.key, run.protocol["harness"]
    item, limit, number = items(protocol)[spec["item"]], protocol["limits"][spec["limit"]], spec["request"]
    row = dict(request=number, item=spec["item"], arm=spec["arm"], limit=spec["limit"], repetition=spec.get("repetition"),
               max_tokens=limit, started_at=now(), status="stopped", error=None, deviation=None, scratch=None,
               feedback_bytes=None, feedback_sha256=None, payload_sha256=None, attempts=[], cost=0.0,
               provider=None, model=None, response_id=None, response_sha256=None, prompt_tokens=None,
               completion_tokens=None, reasoning_tokens=None, answer_tokens=None, cached_tokens=None,
               finish_reason=None, native_finish_reason=None, delivered=False, valid=False, correct=False,
               long=None, undelivered=None, invalid=None, score_error=None, answer_sha256=None,
               reasoning_sha256=None, candidate_sha256=None, transcript="%03d.json.gz" % number)
    capture, clock = dict(spec=spec, row=row), time.monotonic()
    try:
        if hashes() != start:
            raise Stop("compiler_changed")
        source = (item / "program.c").read_text()
        text, records, scratch = feedback(protocol, spec["arm"], source, refs[spec["item"]])
        pin = {k: v for k, v in protocol["provider"].items() if k != "name"}
        payload = dict(model=protocol["model"], temperature=protocol["temperature"], max_tokens=limit,
                       messages=repair.messages(prompt, source, text), provider=pin)
        data = repair.encode(payload, key).encode()
        row.update(scratch=scratch, feedback_bytes=len(text.encode()), feedback_sha256=sha(text), payload_sha256=sha(data))
        capture.update(source=source, feedback=text, tools=records, request=payload)
        response = reason = None
        for attempt in (1, 2):
            if attempt == 2:
                time.sleep(harness["retry_delay_seconds"])
            if run.spent() >= run.cap:
                raise Stop("reported_cost_cap")
            call = dict(attempt=attempt, started_at=now(), seconds=None, http_status=None, transport_error=None,
                        response_file=None, response_bytes=None, response_sha256=None, cost=None,
                        provider=None, model=None, verdict=None, reason=None)
            row["attempts"].append(call)
            began = time.monotonic()
            result = send(data, key, harness)
            call.update(seconds=round(time.monotonic() - began, 3), transport_error=result.get("transport"))
            verdict, reason, parsed = classify(result, harness["body_limit_bytes"])
            call.update(verdict=verdict, reason=reason)
            try:
                if "body" in result:
                    body = result["body"]
                    call.update(http_status=result["status"], response_bytes=len(body), response_sha256=sha(body))
                    if secret(body, key):
                        raise Stop("secret_capture_refused")
                    call["response_file"] = "%03d-a%d.body" % (number, attempt)
                    (run.out / call["response_file"]).write_bytes(body)
                    run.captured += len(body)
                if isinstance(parsed, dict):
                    call.update(provider=parsed.get("provider"), model=parsed.get("model"))
                if verdict == "response":
                    usage = counts(parsed)
                    run.charge(row, call, usage.pop("cost"))
                    row.update(usage, provider=call["provider"], model=call["model"], response_id=parsed.get("id"),
                               response_sha256=call["response_sha256"])
                    response = parsed
                    break
                if error_cost(parsed) is not None:
                    run.charge(row, call, error_cost(parsed))
                if verdict == "stop":
                    raise Stop(reason)
            except Stop as error:
                if call["cost"] is None and error.cost:
                    run.charge(row, call, error.cost)
                raise
            finally:
                if call["cost"] is None:
                    # Unknown cost is never zero: it counts against the cap at the planning maximum.
                    run.unknown += 1
                    run.reserve += reserve(protocol, limit)
        if response is None:
            row.update(status="request_error", error=reason)
            run.halt("retry_failed:" + reason)
            return row
        if row["provider"] != protocol["provider"]["name"]:
            row["deviation"] = "provider"
        elif row["model"] not in protocol["reported_models"]:
            row["deviation"] = "model"
        fields, candidate = judge(response)
        row.update(fields)
        if spec["limit"] == "L16":
            row["long"] = row["completion_tokens"] > protocol["long_threshold_tokens"]
        if candidate is not None:
            row["candidate_sha256"], capture["candidate"] = sha(candidate), candidate
            try:
                row["correct"], capture["score"] = repair.score(candidate, item)
            except Stop:
                row["correct"] = None
                raise
            except (Failure, OSError) as error:
                row.update(correct=None, score_error=str(error) if isinstance(error, Failure) else type(error).__name__)
                capture["score_error"] = getattr(error, "evidence", None)
        row["status"] = "completed"
        if row["deviation"]:
            run.halt("deviation_" + row["deviation"])
    except Stop as error:
        row.update(status="stopped", error=str(error))
        run.halt(str(error))
    except (Failure, OSError) as error:
        row.update(status="stopped", error=str(error) if isinstance(error, Failure) else type(error).__name__)
        run.halt(row["error"])
    except KeyboardInterrupt:
        row.update(status="stopped", error="interrupted_cost_may_be_unknown")
        run.halt(row["error"])
    except Exception as error:  # A harness defect: keep the row and its evidence, then stop.
        row.update(status="stopped", error="harness_" + type(error).__name__)
        run.halt(row["error"])
    finally:
        row["seconds"] = round(time.monotonic() - clock, 3)
        try:
            packed = gzip.compress(repair.encode(capture, key).encode(), mtime=0)
        except Stop:
            packed = gzip.compress(b"[]", mtime=0)
            row["error"] = row["error"] or "secret_capture_refused"
            run.halt("secret_capture_refused")
        (run.out / row["transcript"]).write_bytes(packed)
        run.captured += len(packed)
    return row


def totals(runs, run):
    def total(rows, name):
        return sum(row[name] for row in rows if type(row[name]) is int)
    cells = {}
    for row in runs:
        cell = cells.setdefault(row["arm"] + "-" + row["limit"], dict(trials=0, responses=0, delivered=0, valid=0, correct=0, long=0, length=0))
        cell["trials"] += 1
        cell["responses"] += row["status"] == "completed"
        cell["delivered"] += row["delivered"]
        cell["valid"] += row["valid"]
        cell["correct"] += row["correct"] is True
        cell["long"] += row["long"] is True
        cell["length"] += row["finish_reason"] == "length"
    calls = [call for row in runs for call in row["attempts"]]
    return dict(trials=len(runs), completed=sum(row["status"] == "completed" for row in runs), requests=len(calls),
                retries=sum(call["attempt"] == 2 for call in calls), cost_usd=run.cost, unknown_cost_attempts=run.unknown,
                unknown_cost_reserve_usd=run.reserve, delivered=sum(row["delivered"] for row in runs),
                valid=sum(row["valid"] for row in runs), correct=sum(row["correct"] is True for row in runs),
                long=sum(row["long"] is True for row in runs), cells=cells,
                **{name: total(runs, name) for name in ("prompt_tokens", "completion_tokens", "reasoning_tokens", "answer_tokens")})


def go(protocol, receipt, cap):
    """Evaluate the pilot go criteria of the protocol from the receipt rows."""
    runs, totals_ = receipt["runs"], receipt["totals"]
    expected = len(protocol["phases"]["pilot"]["trials"])
    complete = len(runs) == expected and all(row["status"] == "completed" for row in runs)
    pinned = all(row["provider"] == protocol["provider"]["name"] and row["model"] in protocol["reported_models"] for row in runs)
    usage = all(row["attempts"] and row["attempts"][-1]["cost"] is not None and None not in
                (row["prompt_tokens"], row["completion_tokens"], row["reasoning_tokens"]) for row in runs)
    l16 = [row for row in runs if row["limit"] == "L16"]
    clamped = [row["request"] for row in l16 if row["finish_reason"] == "length" and row["completion_tokens"] != protocol["limits"]["L16"]]
    accepted = bool(l16) and not clamped and all(row["status"] == "completed" and row["attempts"][-1]["http_status"] == 200 for row in l16)
    delivered = [row for row in runs if row["delivered"]]
    scored = [row for row in delivered if row["valid"] and row["correct"] is not None]
    handled = bool(delivered) and all(not row["valid"] or row["correct"] is not None for row in delivered)
    spent = totals_["cost_usd"] + totals_["unknown_cost_reserve_usd"]
    largest = max((row["completion_tokens"] for row in l16 if type(row["completion_tokens"]) is int), default=None)
    rows = [
        (complete and pinned, "%d of %d trials completed; providers %s; models %s" % (
            sum(row["status"] == "completed" for row in runs), expected, sorted({str(row["provider"]) for row in runs}),
            sorted({str(row["model"]) for row in runs}))),
        (complete and usage, "every final attempt has a cost and prompt, completion and reasoning tokens: %s" % usage),
        (accepted, "%d L16 trials with HTTP 200; largest completion %s tokens; length stops below the limit: %s" % (
            sum(row["attempts"][-1]["http_status"] == 200 for row in l16 if row["attempts"]), largest, clamped)),
        (handled, "%d delivered, %d valid, %d scored, %d correct" % (
            len(delivered), sum(row["valid"] for row in delivered), len(scored), sum(row["correct"] is True for row in delivered))),
        (spent < cap, "reported USD %.9f plus unknown-cost reserve USD %.9f against the USD %.2f cap" % (
            totals_["cost_usd"], totals_["unknown_cost_reserve_usd"], cap)),
    ]
    criteria = [dict(criterion=text, held=held, evidence=evidence)
                for text, (held, evidence) in zip(protocol["phases"]["pilot"]["go_criteria"], rows)]
    return dict(criteria=criteria, held=receipt["stop_reason"] == "completed" and all(row["held"] for row in criteria))


def execute(protocol, phase, trials, out, cap, protocol_path=PROTOCOL):
    """Run one trial list, one request at a time, and write its receipt."""
    out, harness = fresh(out), protocol["harness"]
    if shutil.disk_usage(ROOT).free < harness["disk_start_gib"] * GIB:
        raise Stop("disk_below_start_limit")
    provenance = preflight(protocol, protocol_path)
    refs, key = references(protocol), repair.load_key()
    prompt = (ROOT / protocol["prompt_file"]).read_text()
    out.mkdir(mode=0o700, parents=True)
    run = Run(protocol, out, cap, key)
    start = {name: provenance[name] for name in ("evo_sha256", "evocc_sha256")}
    plan = dict(phase=phase, trials=trials, model=protocol["model"], reported_models=protocol["reported_models"],
                provider=protocol["provider"], temperature=protocol["temperature"], limits=protocol["limits"],
                reasoning_parameter=None, prompt_file=protocol["prompt_file"], corpus_tag=protocol["corpus_tag"],
                items_root=protocol["items_root"], cost_cap_usd=cap, max_requests=len(trials),
                max_http_attempts=2 * len(trials), harness={k: v for k, v in harness.items() if k != "rules"})
    path = Path(protocol_path).resolve()
    receipt = dict(schema="agent-repair.v6", phase="budget-" + phase, date=now(),
                   protocol=dict(path=str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path),
                                 sha256=repair.digest(path), status=protocol["status"]),
                   plan=plan, provenance=provenance, endpoints=[], runs=[], totals=None, go=None,
                   stop_reason=None, unattempted=len(trials))
    write(out / "plan.json", dict(plan=plan, provenance=provenance), key)
    receipt["endpoints"].append(endpoints(protocol, out, "before", key))
    write(out / "receipt.json", receipt, key)
    try:
        for spec in trials:
            if run.stop:
                break
            if shutil.disk_usage(ROOT).free < harness["disk_stop_gib"] * GIB:
                run.halt("disk_below_stop_limit")
            elif run.captured >= harness["capture_stop_mib"] * 1024 ** 2:
                run.halt("capture_budget")
            elif run.spent() >= cap:
                run.halt("reported_cost_cap")
            if run.stop:
                break
            row = trial(run, spec, refs, prompt, start)
            receipt["runs"].append(row)
            receipt["unattempted"] = len(trials) - len(receipt["runs"])
            write(out / "receipt.json", receipt, key)
            print(repair.encode({k: row[k] for k in ("request", "item", "arm", "limit", "status", "provider", "finish_reason",
                                                     "completion_tokens", "delivered", "valid", "correct", "cost")}, key), flush=True)
    except KeyboardInterrupt:
        run.halt("interrupted_cost_may_be_unknown")
    receipt["endpoints"].append(endpoints(protocol, out, "after", key))
    receipt.update(provenance_after=compiler(), stop_reason=run.stop or "completed", totals=totals(receipt["runs"], run))
    if phase == "pilot":
        receipt["go"] = go(protocol, receipt, cap)
    write(out / "receipt.json", receipt, key)
    print(repair.encode(dict(stop_reason=receipt["stop_reason"], cost_usd=run.cost, go=(receipt["go"] or {}).get("held")), key))
    return 0 if receipt["stop_reason"] == "completed" else 1


def order(protocol, seed):
    """Return the measured order: one seeded permutation of every trial of every cell."""
    if not isinstance(seed, str) or not re.fullmatch(r"[0-9a-f]{16}", seed):
        raise Failure("invalid_seed")
    trials = [dict(item=item, arm=cell["arm"], limit=cell["limit"], repetition=rep)
              for cell in protocol["cells"] for item in protocol["items"] for rep in range(1, cell["per_item"] + 1)]
    random.Random(int(seed, 16)).shuffle(trials)
    return [dict(request=number, **spec) for number, spec in enumerate(trials, 1)]


def registered(protocol, path):
    """Return the committed measured order, or refuse when the run is not registered."""
    if protocol.get("status") != "registered":
        raise Failure("protocol_not_registered")
    plan = json.loads(Path(path).read_text())
    seed = plan.get("seed")
    if plan.get("trials") != order(protocol, seed) or len(plan["trials"]) != protocol["phases"]["measured"]["requests"]:
        raise Failure("measured_order_mismatch")
    commit = git("rev-parse", "--verify", seed + "^{commit}")
    if repair.run(["git", "merge-base", "--is-ancestor", commit, "HEAD"])["exit_code"]:
        raise Failure("registration_commit_not_in_history")
    if git("status", "--porcelain"):
        raise Failure("measured_run_needs_a_clean_tree")
    # Only the order file may change after the registration commit.
    if set(git("diff", "--name-only", commit, "HEAD").splitlines()) - {str(Path(path).resolve().relative_to(ROOT))}:
        raise Failure("inputs_changed_after_registration")
    return plan["trials"]


def compare_g0(protocol, out):
    """Free check: compare the current G0 bytes with the P17 reference for each item."""
    out = fresh(out)
    refs, before = references(protocol), compiler()
    out.mkdir(mode=0o700, parents=True)
    rows = []
    for name, item in items(protocol).items():
        text, records = repair.feedback("c3", (item / "program.c").read_text())
        scratch, reference = scratch_of(records), refs[name]
        mine, theirs = text.replace(scratch, SCRATCH), reference["feedback"].replace(reference["scratch"], SCRATCH)
        g1 = records[0]["stdout"] + protocol["g1_explain_line"] + "\n" + records[-1]["stdout"]
        row = dict(item=name, p17_transcript=reference["transcript"], p17_scratch=reference["scratch"], scratch=scratch,
                   p17_bytes=len(reference["feedback"].encode()), g0_bytes=len(text.encode()), g1_bytes=len(g1.encode()),
                   explain_records=[r["argv"][2:] for r in records[1:-1]], equal=mine == theirs,
                   normalized_sha256=sha(mine), p17_normalized_sha256=sha(theirs),
                   exit_codes=[r["exit_code"] for r in records], p17_exit_codes=reference["exit_codes"],
                   stderr=[r["stderr"] for r in records])
        if mine != theirs:
            index = next((i for i, (a, b) in enumerate(zip(mine, theirs)) if a != b), min(len(mine), len(theirs)))
            row["first_difference"] = dict(offset=index, current=mine[max(0, index - 80):index + 80], p17=theirs[max(0, index - 80):index + 80])
        for label, value in (("g0", text), ("g1", g1), ("p17", reference["feedback"])):
            (out / (name + "." + label + ".txt")).write_text(value)
        rows.append(row)
    report = dict(schema="agent-repair.p17a-g0-identity.v1", date=now(), compiler_before=before, compiler_after=compiler(),
                  scratch_parent=tempfile.gettempdir(), items=rows, equal=all(row["equal"] for row in rows))
    write(out / "compare-g0.json", report)
    print(json.dumps(dict(equal=report["equal"], items={row["item"]: row["equal"] for row in rows})))
    return 0 if report["equal"] else 1


def score_drafts(protocol, out):
    """Free check: score each Part B draft with the sealed GCC 14 scorer. Makes no model request."""
    out, paths = fresh(out), items(protocol)
    digest = sealed()
    out.mkdir(mode=0o700, parents=True)
    rows = []
    for draft in protocol["part_b"]["drafts"]:
        item = paths[draft["item"]]
        lines = (item / "program.c").read_text().split("\n")
        line, start = lines[draft["line"] - 1], draft["column"] - 1
        if line[start:start + len(draft["span"])] != draft["span"]:
            raise Failure("draft_span_mismatch:" + draft["item"])
        lines[draft["line"] - 1] = line[:start] + draft["replacement"] + line[start + len(draft["span"]):]
        candidate = repair.extract("```c\n" + "\n".join(lines) + "```")
        passed, records = repair.score(candidate, item)
        rows.append(dict(item=draft["item"], span=draft["span"], replacement=draft["replacement"],
                         candidate_sha256=sha(candidate), candidate=candidate, passed=passed, score=records))
    report = dict(schema="agent-repair.p17a-part-b-drafts.v1", date=now(), image=repair.IMAGE, image_digest=digest,
                  rows=rows, passed=sum(row["passed"] for row in rows))
    write(out / "drafts.json", report)
    print(json.dumps(dict(passed=report["passed"], items={row["item"]: row["passed"] for row in rows})))
    return 0 if report["passed"] == len(rows) else 1


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--protocol", type=Path, default=PROTOCOL)
    commands = parser.add_subparsers(dest="command", required=True)
    runner = commands.add_parser("run", help="send the requests of one phase, one at a time")
    runner.add_argument("--phase", choices=("pilot", "measured"), required=True)
    runner.add_argument("--order", type=Path, help="the committed measured order")
    runner.add_argument("--out", type=Path, required=True)
    commands.add_parser("order", help="print the measured order for a seed").add_argument("--seed", required=True)
    commands.add_parser("compare-g0", help="model-free G0 byte comparison").add_argument("--out", type=Path, required=True)
    commands.add_parser("score-drafts", help="model-free Part B draft scoring").add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    protocol = load(args.protocol)
    if args.command == "order":
        print(json.dumps(dict(seed=args.seed, trials=order(protocol, args.seed)), indent=2))
        return 0
    if args.command == "compare-g0":
        return compare_g0(protocol, args.out)
    if args.command == "score-drafts":
        return score_drafts(protocol, args.out)
    if (args.phase == "measured") != (args.order is not None):
        parser.error("--order is required for the measured phase and refused for the pilot")
    phase = protocol["phases"][args.phase]
    trials = phase["trials"] if args.phase == "pilot" else registered(protocol, args.order)
    return execute(protocol, args.phase, trials, args.out, phase["cost_cap_usd"], args.protocol)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (Failure, OSError) as error:
        raise SystemExit(str(error) if isinstance(error, Failure) else type(error).__name__)
