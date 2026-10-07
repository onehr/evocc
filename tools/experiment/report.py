#!/usr/bin/env python3
"""Render measured repair outcomes without treating unattempted cells as failures."""
import argparse
import json
import statistics
from pathlib import Path


def statistics_for(rows):
    passed = [row for row in rows if row["passed"]]
    return {
        "attempted": len(rows),
        "passed": len(passed),
        "errors": sum(row["status"] in ("error", "stopped") for row in rows),
        "synthetic": sum(row.get("synthetic", False) for row in rows),
        "repair_rate": len(passed) / len(rows) if rows else None,
        "mean_rounds": statistics.mean(row["rounds"] for row in rows) if rows else None,
        "mean_success_rounds": statistics.mean(row["rounds"] for row in passed) if passed else None,
    }


def summarize(receipt):
    if receipt["schema"] != "agent-repair.v1":
        raise ValueError("unsupported receipt schema")
    plan, runs = receipt["plan"], receipt["runs"]
    keys = [(r["item"], r["condition"], r["model_sent"], r["repetition"]) for r in runs]
    if len(set(keys)) != len(keys):
        raise ValueError("duplicate repetition")
    for row in runs:
        if not 0 <= row["rounds"] <= plan["rounds"] or row["passed"] != (row["status"] == "passed"):
            raise ValueError("inconsistent outcome")
    result = []
    for model in plan["models"]:
        for condition in plan["conditions"]:
            for kind in ("all", "ub", "frontend", "logic"):
                group = [r for r in runs if r["model_sent"] == model and r["condition"] == condition
                         and (kind == "all" or r["class"] == kind)]
                stats = statistics_for(group)
                reps = [dict(repetition=n, **statistics_for([r for r in group if r["repetition"] == n]))
                        for n in range(1, plan["reps"] + 1)]
                planned = plan["items"] if kind == "all" else plan["class_counts"][kind]
                for rep in reps:
                    rep.update(planned=planned, complete=rep["attempted"] == planned)
                bounds = {}
                for field in ("repair_rate", "mean_rounds"):
                    observed = [r[field] for r in reps if r[field] is not None]
                    bounds[field] = [min(observed), max(observed)] if observed else None
                result.append(dict(model=model, condition=condition, group=kind,
                                   **stats, range=bounds, repetitions=reps))
    return result


def render(receipt):
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
