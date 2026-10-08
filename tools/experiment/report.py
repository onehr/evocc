#!/usr/bin/env python3
"""Render measured repair outcomes without treating unattempted cells as failures."""
import argparse
import json
import statistics
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


def summarize(receipt):
    if receipt["schema"] not in ("agent-repair.v1", "agent-repair.v2"):
        raise ValueError("unsupported receipt schema")
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
              "Incomplete populations or missing measures do not establish a prediction. A tie is not a strict improvement. C0 preflight selects a C0-hard population; these results do not establish a general benefit of feedback."]
    return "\n".join(lines) + "\n"


def render(receipt):
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


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipt", type=Path)
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args()
    receipt = json.loads(args.receipt.read_text())
    print(json.dumps(summarize(receipt), indent=2) if args.json else render(receipt), end="\n" if args.json else "")


if __name__ == "__main__":
    main()
