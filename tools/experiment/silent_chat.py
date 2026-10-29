#!/usr/bin/env python3
"""Run experiment 8: chat models repair silent undefined behavior with frozen GCC or EvoCC feedback.

Each trial sends one request to a pinned provider: the prompt of experiment 1, the original program.c and
the frozen experiment 4 record of condition c1 (GCC 14 warnings and UBSan) or c3 (EvoCC). No compiler runs
while a request is open. The hidden driver scores each candidate under the three sealed builds of
experiments 4 and 5. The cost cap applies to the reported cost plus a reserve for each attempt of unknown
cost; it is not an absolute spend guarantee.
"""
import argparse
import gzip
import importlib.util
import json
import platform
import shutil
import statistics
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import qualify_silent  # noqa: E402  the three-build score of experiments 4 and 5
import report  # noqa: E402  the exact tests of experiments 4 and 5

# Reuse the Protocol A transport, outcome and cost functions; never copy them.
_SPEC = importlib.util.spec_from_file_location("budget", HERE / "budget.py")
budget = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(budget)
repair = qualify_silent.repair
ROOT = repair.ROOT
PROTOCOL = HERE / "protocol-8.json"
TAG = "agent-repair-experiment-8"
GIB = 1024 ** 3
# budget and qualify_silent each load their own copy of repair; catch the exceptions of both.
STOPS = (repair.Stop, budget.Stop)
FAILURES = (repair.Failure, budget.Failure)
SCORED_FILES = ("program.c", "reference_main.c", "expected.txt")


def load(path=PROTOCOL):
    protocol = json.loads(Path(path).read_text())
    if protocol.get("schema") != "agent-repair.protocol.c1":
        raise budget.Failure("unsupported_protocol")
    return protocol


def model_entry(protocol, name):
    for model in protocol["models"]:
        if model["model"] == name:
            return model
    raise budget.Failure("unknown_model")


def frozen(protocol):
    """Return each item's source, class code and frozen c1 and c3 records, after the hash checks."""
    path = ROOT / protocol["qualification"]["path"]
    if repair.digest(path) != protocol["qualification"]["sha256"]:
        raise budget.Failure("qualification_changed")
    rows = {row["item"]: row for row in json.loads(path.read_text())["items"]}
    result = {}
    for name in protocol["items"]:
        row, source = rows.get(name), ROOT / protocol["items_root"] / name / "program.c"
        if not row or not row.get("qualified") or repair.digest(source) != row["hashes"]["program.c"]:
            raise budget.Failure("item_not_frozen:" + name)
        records = row["feedback"]["program.c"]
        result[name] = dict(source=source.read_text(), code=row["code"], c1=records["c1"], c3=records["c3"])
    return result


def order(protocol, phase):
    """Return the trials of a phase. The pilot list comes from the protocol; the measured list is balanced."""
    if phase == "pilot":
        trials = [dict(item=t["item"], condition=t["condition"], repetition=None)
                  for t in protocol["phases"]["pilot"]["trials"]]
    else:
        trials = []
        for repetition in range(1, protocol["repetitions"] + 1):
            for index, item in enumerate(sorted(protocol["items"])):
                pair = ("c1", "c3") if (index + repetition) % 2 == 0 else ("c3", "c1")
                trials += [dict(item=item, condition=condition, repetition=repetition) for condition in pair]
    return [dict(trial=number, **spec) for number, spec in enumerate(trials, 1)]


def payload(protocol, model, prompt, source, feedback):
    return dict(model=model["model"], temperature=protocol["temperature"], max_tokens=protocol["max_tokens"],
                reasoning=protocol["reasoning"], provider=model["provider"],
                messages=repair.messages(prompt, source, feedback))


def reserve(protocol, model):
    """Return the cap charge of one attempt of unknown cost."""
    price = model["price_usd_per_million"]
    tokens = protocol["harness"]["unknown_cost_input_tokens"]
    return (tokens * price["input"] + protocol["max_tokens"] * price["output"]) / 1e6


def group(protocol, code):
    return "silent" if code in protocol["silent_codes"] else "control"


def provenance(protocol, phase, path):
    """Check the frozen inputs and, for the measured phase, the registration. Makes no model request."""
    prompt = ROOT / protocol["prompt_file"]
    if repair.digest(prompt) != protocol["prompt_sha256"]:
        raise budget.Failure("prompt_changed")
    if budget.git("rev-parse", protocol["corpus_tag"] + "^{commit}") != protocol["corpus_commit"]:
        raise budget.Failure("corpus_tag_moved")
    corpus = [ROOT / protocol["items_root"] / item / name for item in protocol["items"] for name in SCORED_FILES]
    relative = [str(p.relative_to(ROOT)) for p in corpus]
    if repair.run(["git", "diff", "--quiet", protocol["corpus_tag"], "--", *relative])["exit_code"]:
        raise budget.Failure("corpus_differs_from_tag")
    if repair.run(["git", "check-ignore", "-q", ".env"])["exit_code"]:
        raise budget.Failure("env_not_ignored")
    inputs = [HERE / name for name in ("silent_chat.py", "budget.py", "repair.py", "qualify_silent.py", "report.py")]
    inputs += [Path(path).resolve(), prompt, ROOT / protocol["qualification"]["path"]]
    names = [str(p.relative_to(ROOT)) for p in inputs]
    status = repair.run(["git", "status", "--porcelain", "--untracked-files=all", "--", *names])["stdout"].splitlines()
    commit = budget.git("rev-parse", "HEAD")
    if phase == "measured":
        tagged = repair.run(["git", "rev-parse", "--verify", "--quiet", TAG + "^{commit}"])["stdout"].strip()
        if protocol["status"] != "registered" or status or commit != tagged:
            raise budget.Failure("measured_run_needs_the_clean_registered_commit")
    return dict(commit=commit, uncommitted_inputs=status, hashes={n: repair.digest(p) for n, p in zip(names, inputs)},
                image=repair.IMAGE, image_digest=budget.sealed(), python=platform.python_version())


def keep(run, row, call, result):
    """Record and store one response body, after the credential check."""
    body = result["body"]
    call.update(http_status=result["status"], response_bytes=len(body), response_sha256=budget.sha(body))
    if budget.secret(body, run.key):
        raise budget.Stop("secret_capture_refused")
    call["response_file"] = "%03d-a%d.body" % (row["trial"], call["attempt"])
    (run.out / call["response_file"]).write_bytes(body)
    run.captured += len(body)


def exchange(run, protocol, model, row, data):
    """Send one request with at most one retry. Return the response, or None after a failed retry."""
    harness, reason = protocol["harness"], None
    for attempt in (1, 2):
        if attempt == 2:
            time.sleep(harness["retry_delay_seconds"])
        if run.spent() >= run.cap:
            raise budget.Stop("reported_cost_cap")
        call = dict(attempt=attempt, started_at=budget.now(), seconds=None, http_status=None, transport_error=None,
                    response_file=None, response_bytes=None, response_sha256=None, cost=None, provider=None,
                    model=None, verdict=None, reason=None)
        row["attempts"].append(call)
        began = time.monotonic()
        result = budget.send(data, run.key, harness)
        call.update(seconds=round(time.monotonic() - began, 3), transport_error=result.get("transport"))
        verdict, reason, parsed = budget.classify(result, harness["body_limit_bytes"])
        call.update(verdict=verdict, reason=reason)
        try:
            if "body" in result:
                keep(run, row, call, result)
            if isinstance(parsed, dict):
                call.update(provider=parsed.get("provider"), model=parsed.get("model"))
            if verdict == "response":
                usage = budget.counts(parsed)
                run.charge(row, call, usage.pop("cost"))
                row.update(usage, provider=call["provider"], model=call["model"], response_id=parsed.get("id"))
                return parsed
            if budget.error_cost(parsed) is not None:
                run.charge(row, call, budget.error_cost(parsed))
            if verdict == "stop":
                raise budget.Stop(reason)
        except STOPS as error:
            if call["cost"] is None and error.cost:
                run.charge(row, call, error.cost)
            raise
        finally:
            if call["cost"] is None:
                # Unknown cost is never zero: it counts against the cap at the planning maximum.
                run.unknown += 1
                run.reserve += reserve(protocol, model)
    row.update(status="request_error", error=reason)
    run.halt("retry_failed:" + str(reason))
    return None


def outcome(run, model, row, response, capture, item_dir):
    """Check the provider and model, judge the answer and score a valid candidate."""
    if row["provider"] != model["provider_name"]:
        row["deviation"] = "provider"
    elif row["model"] not in model["reported_models"]:
        row["deviation"] = "model"
    fields, candidate = budget.judge(response)
    row.update(fields)
    if candidate is not None:
        row["candidate_sha256"], capture["candidate"] = budget.sha(candidate), candidate
        try:
            builds = qualify_silent.score_builds(candidate, item_dir)
            row["builds"] = {name: build["passed"] for name, build in builds.items()}
            row["correct"] = len(row["builds"]) == 3 and all(row["builds"].values())
            capture["score"] = builds
        except STOPS:
            raise
        except FAILURES + (OSError,) as error:
            row.update(correct=None, score_error=str(error) if isinstance(error, FAILURES) else type(error).__name__)
    row["status"] = "completed"
    if row["deviation"]:
        run.halt("deviation_" + row["deviation"])


def blank(spec, item):
    return dict(spec, code=item["code"], started_at=budget.now(), status="stopped", error=None, deviation=None,
                feedback_sha256=budget.sha(item[spec["condition"]]), payload_sha256=None, attempts=[], cost=0.0,
                provider=None, model=None, response_id=None, prompt_tokens=None, completion_tokens=None,
                reasoning_tokens=None, answer_tokens=None, cached_tokens=None, finish_reason=None,
                native_finish_reason=None, delivered=False, valid=False, correct=False, undelivered=None,
                invalid=None, score_error=None, builds=None, answer_sha256=None, reasoning_sha256=None,
                candidate_sha256=None, transcript="%03d.json.gz" % spec["trial"])


def trial(run, protocol, model, spec, prompt, items):
    """Run one trial and return its row. Every exception stops the run and keeps the row."""
    item = items[spec["item"]]
    row = blank(spec, item)
    request = payload(protocol, model, prompt, item["source"], item[spec["condition"]])
    capture, clock = dict(spec=spec, request=request), time.monotonic()
    try:
        data = repair.encode(request, run.key).encode()
        row["payload_sha256"] = budget.sha(data)
        response = exchange(run, protocol, model, row, data)
        if response is not None:
            outcome(run, model, row, response, capture, ROOT / protocol["items_root"] / spec["item"])
    except STOPS as error:
        row.update(status="stopped", error=str(error))
        run.halt(str(error))
    except FAILURES + (OSError,) as error:
        row.update(status="stopped", error=str(error) if isinstance(error, FAILURES) else type(error).__name__)
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
            packed = gzip.compress(repair.encode(capture, run.key).encode(), mtime=0)
        except STOPS:
            packed = gzip.compress(b"[]", mtime=0)
            row["error"] = row["error"] or "secret_capture_refused"
            run.halt("secret_capture_refused")
        (run.out / row["transcript"]).write_bytes(packed)
        run.captured += len(packed)
    return row


def totals(protocol, runs, run):
    cells = {}
    for row in runs:
        cell = cells.setdefault(row["condition"] + "-" + group(protocol, row["code"]),
                                dict(trials=0, completed=0, delivered=0, valid=0, correct=0, length=0))
        cell["trials"] += 1
        cell["completed"] += row["status"] == "completed"
        cell["delivered"] += row["delivered"]
        cell["valid"] += row["valid"]
        cell["correct"] += row["correct"] is True
        cell["length"] += row["finish_reason"] == "length"
    calls = [call for row in runs for call in row["attempts"]]
    tokens = {name: sum(row[name] for row in runs if type(row[name]) is int)
              for name in ("prompt_tokens", "completion_tokens", "reasoning_tokens", "answer_tokens")}
    return dict(trials=len(runs), completed=sum(row["status"] == "completed" for row in runs), requests=len(calls),
                retries=sum(call["attempt"] == 2 for call in calls), cost_usd=run.cost,
                unknown_cost_attempts=run.unknown, unknown_cost_reserve_usd=run.reserve,
                correct=sum(row["correct"] is True for row in runs), cells=cells, **tokens)


def go(protocol, model, receipt):
    """Evaluate the pilot go criteria of the protocol from the receipt rows."""
    runs, sums, caps = receipt["runs"], receipt["totals"], model["cost_cap_usd"]
    expected = len(protocol["phases"]["pilot"]["trials"])
    complete = len(runs) == expected and all(row["status"] == "completed" for row in runs)
    pinned = all(row["provider"] == model["provider_name"] and row["model"] in model["reported_models"]
                 and row["attempts"] and row["attempts"][-1]["http_status"] == 200 for row in runs)
    usage = all(row["attempts"] and row["attempts"][-1]["cost"] is not None and None not in
                (row["prompt_tokens"], row["completion_tokens"], row["reasoning_tokens"]) for row in runs)
    longest = max((row["completion_tokens"] for row in runs if type(row["completion_tokens"]) is int), default=None)
    lengths = sum(row["finish_reason"] == "length" for row in runs)
    valid = [row for row in runs if row["valid"]]
    scored = bool(valid) and all(row["correct"] is not None and not row["score_error"] for row in valid)
    spent = sums["cost_usd"] + sums["unknown_cost_reserve_usd"]
    projected = (sums["cost_usd"] / len(runs) * protocol["phases"]["measured"]["trials_per_model"]
                 + sums["unknown_cost_reserve_usd"]) if runs else float("inf")
    times = [row["attempts"][-1]["seconds"] for row in runs if row["attempts"]]
    median = statistics.median(times) if times else None
    rows = [
        (complete and pinned, "%d of %d trials completed; providers %s; models %s" % (
            sum(row["status"] == "completed" for row in runs), expected,
            sorted({str(row["provider"]) for row in runs}), sorted({str(row["model"]) for row in runs}))),
        (complete and usage, "every final attempt has a cost and prompt, completion and reasoning tokens: %s" % usage),
        (lengths == 0 and longest is not None and longest < protocol["max_tokens"],
         "%d length stops; largest completion %s of %d tokens" % (lengths, longest, protocol["max_tokens"])),
        (scored, "%d valid, %d scored, %d correct" % (
            len(valid), sum(row["correct"] is not None for row in valid), sum(row["correct"] is True for row in valid))),
        (spent < caps["pilot"] and projected < caps["measured"],
         "pilot USD %.6f with reserve USD %.6f against USD %.2f; projected measured USD %.4f against USD %.2f" % (
             sums["cost_usd"], sums["unknown_cost_reserve_usd"], caps["pilot"], projected, caps["measured"])),
        (median is not None and median <= 120, "median request time %s s" % median),
    ]
    criteria = [dict(criterion=text, held=bool(held), evidence=evidence)
                for text, (held, evidence) in zip(protocol["phases"]["pilot"]["go_criteria"], rows)]
    return dict(criteria=criteria, held=receipt["stop_reason"] == "completed" and all(c["held"] for c in criteria))


def guard(run, harness):
    """Return the reason to stop before the next trial, or None."""
    if shutil.disk_usage(ROOT).free < harness["disk_stop_gib"] * GIB:
        return "disk_below_stop_limit"
    if run.captured >= harness["capture_stop_mib"] * 1024 ** 2:
        return "capture_budget"
    if run.spent() >= run.cap:
        return "reported_cost_cap"
    return None


def execute(protocol, model, phase, out, path=PROTOCOL):
    """Run the trials of one phase for one model, one request at a time, and write the receipt."""
    out, harness = budget.fresh(out), protocol["harness"]
    if shutil.disk_usage(ROOT).free < harness["disk_start_gib"] * GIB:
        raise budget.Stop("disk_below_start_limit")
    origin = provenance(protocol, phase, path)
    items, key = frozen(protocol), repair.load_key()
    prompt = (ROOT / protocol["prompt_file"]).read_text()
    trials, cap = order(protocol, phase), model["cost_cap_usd"][phase]
    out.mkdir(mode=0o700, parents=True)
    run = budget.Run(protocol, out, cap, key)
    plan = dict(phase=phase, model=model, trials=trials, temperature=protocol["temperature"],
                max_tokens=protocol["max_tokens"], reasoning=protocol["reasoning"], prompt_file=protocol["prompt_file"],
                cost_cap_usd=cap, max_requests=len(trials), max_http_attempts=2 * len(trials), harness=harness)
    resolved = Path(path).resolve()
    receipt = dict(schema="agent-repair.v8", phase="silent-chat-" + phase, date=budget.now(),
                   protocol=dict(path=str(resolved.relative_to(ROOT)), sha256=repair.digest(resolved),
                                 status=protocol["status"]),
                   plan=plan, provenance=origin, runs=[], totals=None, go=None, stop_reason=None,
                   unattempted=len(trials))
    budget.write(out / "receipt.json", receipt, key)
    try:
        for spec in trials:
            reason = guard(run, harness)
            if reason:
                run.halt(reason)
            if run.stop:
                break
            row = trial(run, protocol, model, spec, prompt, items)
            receipt["runs"].append(row)
            receipt["unattempted"] = len(trials) - len(receipt["runs"])
            budget.write(out / "receipt.json", receipt, key)
            print(repair.encode({k: row[k] for k in ("trial", "item", "condition", "repetition", "status", "provider",
                                                     "finish_reason", "completion_tokens", "reasoning_tokens",
                                                     "valid", "correct", "cost")}, key), flush=True)
    except KeyboardInterrupt:
        run.halt("interrupted_cost_may_be_unknown")
    receipt.update(stop_reason=run.stop or "completed", totals=totals(protocol, receipt["runs"], run))
    if phase == "pilot":
        receipt["go"] = go(protocol, model, receipt)
    budget.write(out / "receipt.json", receipt, key)
    print(repair.encode(dict(stop_reason=receipt["stop_reason"], cost_usd=run.cost,
                             go=(receipt["go"] or {}).get("held")), key))
    return 0 if receipt["stop_reason"] == "completed" else 1


def analyze(protocol, receipt):
    """Apply the registered predictions P1 and P2 to one measured receipt."""
    runs, model = receipt["runs"], receipt["plan"]["model"]["model"]
    complete = (receipt["stop_reason"] == "completed"
                and len(runs) == protocol["phases"]["measured"]["trials_per_model"])
    passed, codes = {}, {}
    for row in runs:
        passed.setdefault((row["item"], row["condition"]), []).append(row["correct"] is True)
        codes[row["item"]] = row["code"]

    def repaired(items, condition):
        return sum(sum(passed.get((item, condition), [])) for item in items)

    silent = [i for i in protocol["items"] if group(protocol, codes.get(i)) == "silent"]
    controls = [i for i in protocol["items"] if i in codes and i not in silent]
    test = report.exact_wilcoxon([repaired([i], "c3") - repaired([i], "c1") for i in silent])
    loss = repaired(controls, "c1") - repaired(controls, "c3")
    table = {name: dict(c1=repaired(items, "c1"), c3=repaired(items, "c3"), trials=protocol["repetitions"] * len(items))
             for name, items in (("silent", silent), ("controls", controls))}
    return dict(model=model, complete=complete, table=table,
                P1=dict(test, held=complete and test["p"] < 0.05),
                P2=dict(loss=loss, held=complete and loss <= 2),
                undelivered=sum(row["status"] == "completed" and not row["delivered"] for row in runs),
                score_errors=sum(bool(row["score_error"]) for row in runs), cost_usd=receipt["totals"]["cost_usd"])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    run_parser = commands.add_parser("run", help="run the pilot or the measured trials for one model")
    run_parser.add_argument("--phase", choices=("pilot", "measured"), required=True)
    run_parser.add_argument("--model", required=True)
    run_parser.add_argument("--out", required=True)
    run_parser.add_argument("--protocol", default=str(PROTOCOL))
    analyze_parser = commands.add_parser("analyze", help="apply P1 and P2 to measured receipts")
    analyze_parser.add_argument("receipts", nargs="+")
    analyze_parser.add_argument("--protocol", default=str(PROTOCOL))
    args = parser.parse_args(argv)
    protocol = load(args.protocol)
    if args.command == "run":
        try:
            return execute(protocol, model_entry(protocol, args.model), args.phase, args.out, args.protocol)
        except FAILURES as error:
            print(json.dumps(dict(error=str(error))))
            return 2
    print(json.dumps([analyze(protocol, json.loads(Path(p).read_text())) for p in args.receipts], indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
