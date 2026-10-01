#!/usr/bin/env python3
"""Locate a per-unit codegen regression using retained measurements first.

This read-only command never switches a checkout or creates a worktree. An
incomplete interval prints exact revisions requiring the existing bench entry
point; a missing receipt is not a passing midpoint.
"""
from __future__ import annotations

# Direct execution puts tools/ on sys.path. Keep the standard-library bisect
# interface available when statistics imports this same-named entry point.
from _bisect import bisect_left, bisect_right, insort_left, insort_right

bisect = bisect_right
insort = insort_right

import argparse
import importlib
import json
from pathlib import Path
import subprocess
import sys


def main(argv=None):
    package = "ladder" if __name__ == "__main__" else "tools.ladder"
    trend = importlib.import_module(package + ".codegen_trend")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--history", type=Path, required=True)
    parser.add_argument("--good", required=True)
    parser.add_argument("--bad", required=True)
    parser.add_argument("--unit", required=True, help="S5 unit ID, or input case ID for steps")
    parser.add_argument("--profile", choices=trend.PROFILES, default="unchecked")
    parser.add_argument("--variant", default="O2-p0")
    parser.add_argument("--metric", choices=("text_bytes", "compile_time", "steps"), default="text_bytes")
    args = parser.parse_args(argv)
    try:
        result = trend.locate_regression(trend.git_commits(args.root, args.good, args.bad),
                                         trend.read_observations(args.history), args.unit,
                                         args.profile, args.metric, args.variant)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print("codegen bisect: " + str(error), file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True, indent=2))
    return 2 if result["status"] == "INCOMPLETE" else 0


if __name__ == "__main__":
    raise SystemExit(main())
