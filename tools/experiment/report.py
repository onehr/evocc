#!/usr/bin/env python3
"""Render measured repair outcomes without treating unattempted cells as failures."""
import argparse
import json
import math
import statistics
from fractions import Fraction
from pathlib import Path


def statistics_for(rows, version=1):
    passed = [row for row in rows if row["passed"]]
    result = {
        "attempted": len(rows),
        "passed": len(passed),
        "errors": sum(row["status"] in ("error", "stopped") for row in rows),
        "synthetic": sum(row.get("synthetic", False) for row in rows),
        "repair_rate": len(passed) / len(rows) if rows else None,
        "mean_rounds": statistics.mean(row["rounds"] for row in rows) if rows else None,
        "mean_success_rounds": statistics.mean(row["rounds"] for row in passed) if passed else None,
    }
    if version == 2:
        median = lambda values: statistics.median(values) if values else None
        known = [r for r in passed if all(c["tokens_in"] is not None and c["tokens_out"] is not None for c in r["calls"])]
        tokens = [r["tokens_in"] + r["tokens_out"] for r in known]
        response_errors = sum(r["status"] == "response_error" for r in rows)
        eligible = len(rows) - response_errors
        measured = [r for r in passed if all(f in r for f in ("diff_added", "diff_removed", "changed_inside_spans", "spans_touched"))]
        diffs = [r["diff_added"] + r["diff_removed"] for r in measured]
        result.update(response_errors=response_errors, completion_rate=result["repair_rate"],
                      repair_rate=len(passed) / eligible if eligible else None,
                      success_tokens=median(tokens), success_token_range=[min(tokens), max(tokens)] if tokens else None,
                      unknown_success_usage=len(passed) - len(known), success_samples=len(known), success_rounds=median([r["rounds"] for r in passed]),
                      unknown_success_metrics=len(passed) - len(measured), success_diff=median(diffs),
                      all_spans_share=sum(all(r["spans_touched"]) for r in measured) / len(measured) if measured else None,
                      inside_span_ratio=median([r["changed_inside_spans"] / n for r, n in zip(measured, diffs) if n]),
                      auto_applied=sum(r["auto_applied"] for r in rows), auto_only=sum(r["rounds"] == 0 for r in passed))
    return result


REFUSED = ("http_401", "http_402", "http_403")


def key_limit_stop(receipt):
    """Apply the registered key-refusal stop that the tagged harness missed for HTTP 403.

    OpenRouter refused the exhausted key with HTTP 403 "Key limit exceeded"; the harness
    recorded each refusal as a response error and kept starting slots. The campaign ends
    at the first slot whose first request was refused. Earlier slots keep their genuine
    attempts; one that also met a refusal is stopped. Later slots are unattempted.
    """
    if receipt["schema"] != "agent-repair.v2" or receipt.get("key_limit_stop"):
        return receipt
    runs = sorted(receipt["runs"], key=lambda r: r.get("start_order", 0))
    first = next((r["start_order"] for r in runs if r["calls"] and r["calls"][0].get("response_error") in REFUSED), None)
    if first is None:
        return receipt
    kept = [dict(r, status="stopped", passed=False, error="provider_refused_key:" + next(c["response_error"] for c in r["calls"] if c.get("response_error") in REFUSED))
            if any(c.get("response_error") in REFUSED for c in r["calls"]) else r for r in runs if r["start_order"] <= first]
    unknown = sum(c.get("cost") is None and c.get("response_error") not in REFUSED for r in kept for c in r["calls"])
    return dict(receipt, runs=kept, stop_reason=kept[-1]["error"], unattempted=len(runs) + receipt["unattempted"] - len(kept), total_cost=sum(r["cost"] for r in kept),
                unknown_cost_attempts=unknown, key_limit_stop=dict(first_refused_slot=first, recorded_rows=len(runs), recorded_stop_reason=receipt["stop_reason"]))


def summarize(receipt):
    if receipt["schema"] == "agent-repair.v4": return paired_results(receipt)
    if receipt["schema"] not in ("agent-repair.v1", "agent-repair.v2"):
        raise ValueError("unsupported receipt schema")
    receipt = key_limit_stop(receipt)
    version = 2 if receipt["schema"] == "agent-repair.v2" else 1
    plan, runs = receipt["plan"], receipt["runs"]
    hard = receipt.get("provenance", {}).get("preflight_hard")
    keys = [(r["item"], r["condition"], r["model_sent"], r["repetition"]) for r in runs]
    if len(set(keys)) != len(keys):
        raise ValueError("duplicate repetition")
    for row in runs:
        if not 0 <= row["rounds"] <= plan["rounds"] or row["passed"] != (row["status"] == "passed"):
            raise ValueError("inconsistent outcome")
    result = []
    for model in plan["models"]:
        for condition in plan["conditions"]:
            for kind in ("all", *plan["class_counts"], *(("hard",) if hard else ())) if version == 2 else ("all", "ub", "frontend", "logic"):
                group = [r for r in runs if r["model_sent"] == model and r["condition"] == condition
                         and (kind == "all" or r["class"] == kind or (kind == "hard" and r["item"] in hard[model]))]
                stats = statistics_for(group, version)
                reps = [dict(repetition=n, **statistics_for([r for r in group if r["repetition"] == n], version))
                        for n in range(1, plan["reps"] + 1)]
                planned = plan["items"] if kind == "all" else len(hard[model]) if kind == "hard" else plan["class_counts"][kind]
                for rep in reps:
                    rep.update(planned=planned, complete=rep["attempted"] == planned)
                bounds = {}
                for field in ("repair_rate", "mean_rounds"):
                    observed = [r[field] for r in reps if r[field] is not None]
                    bounds[field] = [min(observed), max(observed)] if observed else None
                result.append(dict(model=model, condition=condition, group=kind,
                                   **stats, range=bounds, repetitions=reps))
    return result


# Each prediction is a list of strict comparisons (better, worse, measure); predictions-2.md fixes them.
PLAN = [[("c3", c, "success_tokens") for c in ("c1", "c2")], [("c3", c, "success_diff") for c in ("c1", "c2")],
        [("c3", c, "all_spans_share") for c in ("c1", "c2")], [("c4", "c3", f) for f in ("success_rounds", "success_tokens")],
        [(c, "c0", f) for c in ("c1", "c2", "c3", "c4") for f in ("success_tokens", "success_rounds", "success_diff", "all_spans_share", "inside_span_ratio")]]
MISSING = {"success_tokens": "unknown_success_usage", "success_diff": "unknown_success_metrics", "all_spans_share": "unknown_success_metrics", "inside_span_ratio": "unknown_success_metrics"}


def predictions(receipt):
    receipt = key_limit_stop(receipt)
    rows, results = summarize(receipt), []
    population = "hard" if receipt.get("provenance", {}).get("preflight_hard") else "all"
    for model in receipt["plan"]["models"]:
        groups = {r["condition"]: r for r in rows if r["model"] == model and r["group"] == population}
        complete = receipt["stop_reason"] == "completed" and receipt["unattempted"] == 0 and all(all(p["complete"] for p in r["repetitions"]) for r in groups.values())
        def sample(condition, field):
            group = groups.get(condition, {})
            return group.get("success_samples") if field == "success_tokens" else group.get("passed", 0) - (group.get("unknown_success_metrics", 0) if field in MISSING else 0)
        for number, comparisons in enumerate(PLAN, 1):
            reasons, observed = [], []
            for better, worse, field in comparisons:
                x, y = groups.get(better, {}).get(field), groups.get(worse, {}).get(field)
                shown = lambda value: "none" if value is None else round(value, 3)
                observed.append(f"{field} {better}={shown(x)} (n={sample(better, field)}) vs {worse}={shown(y)} (n={sample(worse, field)})")
                if field in MISSING and any(groups.get(c, {}).get(MISSING[field]) for c in (better, worse)): reasons.append("missing data: " + MISSING[field])
                elif x is None or y is None: reasons.append("no measured success in " + (better if x is None else worse))
                elif x == y: reasons.append("tie in " + field)
                elif (x < y) != (field in ("all_spans_share", "inside_span_ratio")): continue
                else: reasons.append("opposite direction in " + field)
            reasons = ([] if complete else ["incomplete population"]) + list(dict.fromkeys(reasons))
            evidence = ("complete" if complete else "incomplete") + f" ({population} items): " + "; ".join(observed) + (". Reasons: " + ", ".join(reasons) if reasons else "")
            results.append(dict(model=model, prediction=number, held=not reasons, reasons=reasons, evidence=evidence))
    return results


def render_v2(receipt):
    receipt = key_limit_stop(receipt)
    fmt = lambda value: "NOT OBSERVED" if value is None else f"{value:.3f}".rstrip("0").rstrip(".") if isinstance(value, float) else json.dumps(value)
    rows, lines = summarize(receipt), []
    titles = {"all": "All items", "hard": "Items hard for that model in preflight (prediction population)"}
    for group in dict.fromkeys(r["group"] for r in rows):
        lines += [f"### {titles.get(group, 'Class ' + group)}", "",
                  "| Model | Condition | Pass / attempted | Repair rate | Completion rate | Response errors | Other errors/stops | Mean rounds | Success rounds median | Tokens per success median (range; n) | Unknown success usage | Diff median | All-span share | Inside-span median | Auto repairs / auto-only passes |",
                  "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- | ---: | ---: | ---: | ---: | ---: |"]
        for row in (r for r in rows if r["group"] == group):
            tokens = f'{fmt(row["success_tokens"])} ({fmt(row["success_token_range"])}; n={row["success_samples"]})'
            values = [row["model"], row["condition"], f'{row["passed"]}/{row["attempted"]}', *(fmt(row[f]) for f in ("repair_rate", "completion_rate", "response_errors", "errors", "mean_rounds", "success_rounds")), tokens, str(row["unknown_success_usage"])]
            values += [fmt(row[f]) for f in ("success_diff", "all_spans_share", "inside_span_ratio")] + [f'{row["auto_applied"]}/{row["auto_only"]}']
            lines.append("| " + " | ".join(values) + " |")
        lines.append("")
    lines += ["Repair rate excludes response errors from the denominator; completion rate counts every attempt. Tokens, rounds, diff and overlap are conditional on success. Token aggregates exclude unknown usage; diff and overlap aggregates exclude missing final records. Neither absence is zero. Empty diffs have no inside-span ratio.",
              "Missing successful diff records: " + json.dumps({r["model"] + "/" + r["condition"]: r["unknown_success_metrics"] for r in rows if r["group"] == "all"}, sort_keys=True), ""]
    for result in predictions(receipt):
        lines.append(f'- {result["model"]} P{result["prediction"]}: {"held" if result["held"] else "did not hold"}. {result["evidence"]}')
    lines += ["", f'Stop: {receipt["stop_reason"]}. Unattempted repetitions: {receipt["unattempted"]}. Reported cost: USD {receipt["total_cost"]:.6f}. Unknown-usage attempts: {receipt.get("unknown_cost_attempts", "not recorded")}.',
              *([f'Key-limit stop applied by the report: the first refused slot is {receipt["key_limit_stop"]["first_refused_slot"]}; the harness recorded {receipt["key_limit_stop"]["recorded_rows"]} rows with stop {receipt["key_limit_stop"]["recorded_stop_reason"]}.'] if receipt.get("key_limit_stop") else []),
              "Incomplete populations or missing measures do not establish a prediction. A tie is not a strict improvement. C0 preflight selects a C0-hard population; these results do not establish a general benefit of feedback."]
    return "\n".join(lines) + "\n"


def render(receipt):
    if receipt["schema"] == "agent-repair.v4": return json.dumps(paired_results(receipt), indent=2) + "\n"
    if receipt["schema"] == "agent-repair.v2": return render_v2(receipt)
    fmt = lambda value: "NOT RUN" if value is None else f"{value:.3f}"
    lines = ["| Model | Condition | Class | Pass / attempted | Repair rate | Mean rounds | Mean rounds (success) | Rate range | Rounds range | Attempts / planned by repetition | Synthetic attempts | Errors/stops |",
             "| --- | --- | --- | ---: | ---: | ---: | ---: | --- | --- | --- | ---: | ---: |"]
    for row in summarize(receipt):
        span = row["range"]["repair_rate"]
        interval = "NOT RUN" if span is None else " to ".join(fmt(value) for value in span)
        rounds = row["range"]["mean_rounds"]
        round_span = "NOT RUN" if rounds is None else " to ".join(fmt(value) for value in rounds)
        counts = ", ".join(f'{r["attempted"]}/{r["planned"]}' for r in row["repetitions"])
        lines.append(f'| {row["model"]} | {row["condition"]} | {row["group"]} | '
                     f'{row["passed"]}/{row["attempted"]} | {fmt(row["repair_rate"])} | '
                     f'{fmt(row["mean_rounds"])} | {fmt(row["mean_success_rounds"])} | '
                     f'{interval} | {round_span} | {counts} | {row["synthetic"]} | {row["errors"]} |')
    lines.extend(["", "| Item | Origin | Model | Condition | Pass / attempted | Mean rounds |",
                  "| --- | --- | --- | --- | ---: | ---: |"])
    for model, item, condition in sorted({(r["model_sent"], r["item"], r["condition"]) for r in receipt["runs"]}):
        group = [r for r in receipt["runs"] if (r["model_sent"], r["item"], r["condition"]) == (model, item, condition)]
        stats = statistics_for(group)
        origin = "written for this corpus" if group[0].get("synthetic") else "public source + injected defect"
        lines.append(f'| {item} | {origin} | {model} | {condition} | {stats["passed"]}/{stats["attempted"]} | {fmt(stats["mean_rounds"])} |')
    lines.extend(["", f'Stop: {receipt["stop_reason"]}. Unattempted repetitions: {receipt["unattempted"]}.',
                  f'Reported cost: USD {receipt["total_cost"]:.6f}.',
                  "Rates include recorded errors in the attempted denominator. A range from incomplete repetitions is not a full-population estimate."])
    return "\n".join(lines) + "\n"


def exact_mcnemar(c1_only, c3_only):
    """Two-sided exact conditional test; ties are not discordant observations."""
    if any(type(n) is not int or n < 0 for n in (c1_only, c3_only)): raise ValueError("invalid pair count")
    n = c1_only + c3_only
    return min(1.0, 2 * sum(math.comb(n, k) for k in range(min(c1_only, c3_only) + 1)) / 2**n)


def paired_results(receipt):
    classes, selected = receipt["plan"]["classes"], {}
    for row in sorted(receipt["runs"], key=lambda r: r["index"]):
        selected[row["item"], row["condition"]] = row
    def usable(row):
        return row and row.get("status") in ("completed", "timeout", "step_limit") and not row.get("score_error") and not row.get("escapes")
    groups = {}
    for group in ("silent", "control", *sorted(set(classes.values()))):
        items = [i for i, c in classes.items() if c == group or
                 (group == "silent" and c in ("E3001", "E3010", "E3011")) or
                 (group == "control" and c in ("E3002", "E3004", "E3005"))]
        pairs = [(selected.get((i, "c1")), selected.get((i, "c3"))) for i in items]
        valid = [(a, b) for a, b in pairs if usable(a) and usable(b)]
        c1_only = sum(a["passed"] and not b["passed"] for a, b in valid)
        c3_only = sum(b["passed"] and not a["passed"] for a, b in valid)
        groups[group] = dict(planned=len(items), paired=len(valid), unknown=len(items)-len(valid),
                             c1=sum(a["passed"] for a, _ in valid), c3=sum(b["passed"] for _, b in valid),
                             c1_only=c1_only, c3_only=c3_only, p=exact_mcnemar(c1_only, c3_only))
    s, c = groups["silent"], groups["control"]
    groups["P1"] = s["paired"] > 0 and s["unknown"] == 0 and s["c3"] > s["c1"] and s["p"] < 0.05
    groups["P2"] = c["paired"] > 0 and c["unknown"] == 0 and abs(c["c3"]-c["c1"]) <= 1
    groups["descriptive"] = {}
    for condition in ("c1", "c3"):
        rows = [r for r in receipt["runs"] if r["condition"] == condition]
        groups["descriptive"][condition] = dict(attempts=len(rows), steps=sum(r.get("steps", 0) for r in rows),
            tokens=sum(r.get("tokens", {}).get("total", 0) for r in rows), checks=sum(r.get("check_runs", 0) for r in rows),
            seconds=round(sum(r.get("seconds", 0) for r in rows), 1), overloads=sum(r["status"] == "provider_overloaded" for r in rows))
    return groups


def exact_wilcoxon(differences):
    """One-sided C1 > C3 signed-rank test; enumerate all signs of nonzero item differences."""
    values = [d for d in differences if d != 0]
    ordered = sorted(abs(d) for d in values)
    ranks = [ordered.index(abs(d)) + len(ordered) - ordered[::-1].index(abs(d)) + 1 for d in values]
    observed = sum(r for r, d in zip(ranks, values) if d > 0)  # twice the midrank; integers preserve ties
    sums = [0]
    for rank in ranks: sums += [s + rank for s in sums]
    return dict(nonzero=len(values), positive_rank=observed / 2, p=sum(s >= observed for s in sums) / len(sums))


def effort_results(part_a, part_b):
    """Exclude the exploratory run. Average only complete C1/C3 repetition pairs within each item."""
    result, populations = dict(tables=[]), {}
    usable = lambda r: r and r.get("status") in ("completed", "timeout", "step_limit") and not r.get("score_error") and not r.get("escapes")
    repaired = lambda r: usable(r) and r["status"] == "completed" and r.get("passed") is True
    for part, receipt, allowed in (("A", part_a, {2, 3}), ("B", part_b, {1, 2})):
        classes, selected = receipt["plan"]["classes"], {}
        plan = {(r["item"], r["condition"], r["repetition"]) for r in receipt["plan"]["order"]}
        if any(r not in allowed for _, _, r in plan): raise ValueError("unregistered repetition")
        for row in sorted(receipt["runs"], key=lambda r: r["index"]):
            key = row["item"], row["condition"], row["repetition"]
            if key not in plan: raise ValueError("unregistered repetition")
            if key in selected and row.get("retry_of") != selected[key]["index"]: raise ValueError("duplicate repetition")
            selected[key] = row
        pairs, means = {}, {}
        for item in classes:
            reps = sorted({r for i, _, r in plan if i == item})
            pairs[item] = [(selected.get((item, "c1", r)), selected.get((item, "c3", r))) for r in reps]
            valid = [(a, b) for a, b in pairs[item] if usable(a) and usable(b)]
            if valid:
                means[item] = {field: [sum((Fraction(r[c][field] if field == "steps" else r[c]["tokens"]["total"]) for r in valid), Fraction()) / len(valid)
                                       for c in (0, 1)] for field in ("steps", "tokens")}
        silent = [i for i, c in classes.items() if c in ("E3001", "E3010", "E3011")]
        populations[part] = (means, silent, any(not usable(a) or not usable(b) for i in silent for a, b in pairs[i]))
        for group in ("all", "silent", "control", *sorted(set(classes.values()))):
            items = [i for i in classes if group == "all" or classes[i] == group or (group == "silent" and i in silent) or (group == "control" and i not in silent)]
            for condition in ("c1", "c3"):
                rows = [r for (i, c, _), r in selected.items() if i in items and c == condition]
                valid = [r for r in rows if usable(r)]
                result["tables"].append(dict(part=part, group=group, condition=condition, items=len(items), attempted=len(rows),
                    planned=sum(i in items and c == condition for i, c, _ in plan), unknown=len(rows)-len(valid),
                    passed=sum(bool(repaired(r)) for r in valid), repair_rate=sum(bool(repaired(r)) for r in valid)/len(valid) if valid else None,
                    censored=sum(r["status"] in ("timeout", "step_limit") for r in rows),
                    steps=sum(r["steps"] for r in valid), tokens=sum(r["tokens"]["total"] for r in valid)))
    for name, part, field in (("P1", "A", "steps"), ("P2", "A", "tokens"), ("P3", "B", "steps")):
        means, items, incomplete = populations[part]
        values = [means[i][field] for i in items if i in means]
        test = exact_wilcoxon([a-b for a, b in values])
        result[name] = dict(test, items=len(values), incomplete=incomplete,
                            c1=float(sum(a for a, _ in values)), c3=float(sum(b for _, b in values)), held=bool(values) and test["p"] < 0.05)
    savings = {p: 1-result[n]["c3"]/result[n]["c1"] if result[n]["c1"] else None for p, n in (("A", "P1"), ("B", "P3"))}
    result["P4"] = dict(savings, held=all(v is not None for v in savings.values()) and savings["B"] > savings["A"],
                        incomplete=result["P1"]["incomplete"] or result["P3"]["incomplete"], test="none; point comparison")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--part-b", type=Path, help="registered effort comparison; positional receipt is Part A")
    args = parser.parse_args()
    receipt = json.loads(args.receipt.read_text())
    if args.part_b: print(json.dumps(effort_results(receipt, json.loads(args.part_b.read_text())), indent=2)); return
    print(json.dumps(summarize(receipt), indent=2) if args.json else render(receipt), end="\n" if args.json else "")


if __name__ == "__main__":
    main()
