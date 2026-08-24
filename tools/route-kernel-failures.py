#!/usr/bin/env python3
"""Route each failed reconnaissance lane to the stage that owns its root cause.

Reads one reconnaissance run's results.jsonl, groups failures by the diagnostic
that stopped the lane, compares lane by lane against a baseline run, and emits a
routing table. Routing is explicit and conservative:

  preprocess-hardening   the lane failed in EvoCPP on a construct the roadmap
                         assigns to header and predefined-macro hardening
  gnu-extensions         the lane failed on a construct this stage implements
  unclassified           the first diagnostic does not identify the construct;
                         a reduction is required before routing

A first diagnostic truncates at the first error, so a parse failure is routed
only when a reduced reproducer or an unambiguous message names the construct.
Everything else stays unclassified rather than being guessed. This tool decides
nothing about priority; it makes the input to that decision inspectable.
"""

from __future__ import annotations

import argparse
import collections
import json
import sys
from pathlib import Path

# Messages that name their construct unambiguously. Kept short on purpose: a
# longer table would be guessing dressed as classification.
PREPROCESS_HARDENING = {
    ("E0104", 'error-directive: "Unknown compiler"'),
}
GNU_EXTENSION_MARKERS = ("__attribute__", "typeof", "__typeof__", "({", "statement expression")


def load(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def key(record: dict) -> tuple:
    d = record.get("diagnostic") or {}
    return (record["lane"], record.get("classification"), d.get("code"), d.get("message"))


def route(record: dict, reductions: dict[str, str]) -> str:
    if record["outcome"] != "failure":
        return "clean"
    d = record.get("diagnostic") or {}
    if (d.get("code"), d.get("message")) in PREPROCESS_HARDENING:
        return "preprocess-hardening"
    reduced = reductions.get(record["tuId"], "")
    if any(marker in reduced for marker in GNU_EXTENSION_MARKERS):
        return "gnu-extensions"
    return "unclassified"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run", type=Path, required=True, help="results.jsonl of the refreshed run")
    parser.add_argument("--baseline", type=Path, required=True, help="results.jsonl of the baseline run")
    parser.add_argument("--reductions", type=Path, help="JSON mapping tuId -> reduced source text")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    run = load(args.run)
    base = {(r["tuId"], r["lane"]): r for r in load(args.baseline)}
    reductions = json.loads(args.reductions.read_text()) if args.reductions else {}

    rows, regressions, groups = [], [], collections.Counter()
    for r in run:
        b = base.get((r["tuId"], r["lane"]))
        was_clean = b is not None and b["outcome"] != "failure"
        now_clean = r["outcome"] != "failure"
        if was_clean and not now_clean:
            regressions.append((r["tuId"], r["lane"]))
        rt = route(r, reductions)
        groups[(rt,) + key(r)] += 1
        d = r.get("diagnostic") or {}
        rows.append({
            "tuId": r["tuId"], "lane": r["lane"], "outcome": r["outcome"],
            "code": d.get("code"), "message": d.get("message"),
            "sourcePath": d.get("sourcePath"), "route": rt,
            "baselineOutcome": b["outcome"] if b else None,
        })

    summary = {
        "observations": len(run),
        "clean": sum(1 for r in rows if r["outcome"] != "failure"),
        "byRoute": dict(collections.Counter(r["route"] for r in rows)),
        "regressionsAgainstBaseline": regressions,
        "groups": [{"route": g[0], "lane": g[1], "classification": g[2], "code": g[3],
                    "message": g[4], "count": n} for g, n in groups.most_common()],
    }
    args.out.write_text(json.dumps({"summary": summary, "lanes": rows}, indent=2) + "\n")
    print(json.dumps(summary["byRoute"]), "| regressions:", len(regressions))
    for g in summary["groups"]:
        print(f"  {g['count']:3} {g['route']:22} {g['lane']:26} {g['code']} {str(g['message'])[:50]}")
    return 1 if regressions else 0


if __name__ == "__main__":
    sys.exit(main())
