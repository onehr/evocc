"""N05 compact measurements over the existing S1 and interpreter receipts.

This module is a trend consumer, not a certificate checker or a second bench
runner. It keeps incomplete observations visible and never averages away a
single translation unit's regression. Native timing is intentionally absent.
"""
from __future__ import annotations

import argparse
import fcntl
import json
from pathlib import Path
import re
import statistics
import subprocess

from .eqsat_chain import digest, keys, natural, require, sha256_identity, unique
from .eqsat_elf import inspect_object_size, require_identical_objects


UNITS = (
    "p0:tests/fixtures/eqsat-performance/integer-mix.c",
    "zlib:deflate.c", "zlib:inflate.c",
    "zstd:lib/compress/zstd_compress.c", "zstd:lib/decompress/zstd_decompress.c",
    "cjson:cJSON.c",
)
PROFILES = ("checked", "unchecked")
LADDER_CASES = ("integer-mix", "zlib-compress", "zlib-decompress",
                "zstd-compress", "zstd-decompress", "cjson-roundtrip")
SIZE_FIELDS = ("text_bytes", "instruction_bytes", "static_instruction_slots",
               "padding_bytes", "literal_pool_or_intext_data_bytes",
               "unclassified_text_bytes", "rodata_bytes")
OUTCOMES = {"PASS", "FAIL", "NOT_RUN", "UNKNOWN", "TIMEOUT"}


def object_row(unit, profile, first, second, *, gcc_o2_text_bytes, compile_wall_ns=None):
    """Measure real object twins with S1, then discard their payloads upstream."""
    require_identical_objects(first, second)
    metrics = inspect_object_size(first)
    return dict(unit=unit, profile=profile, object_sha256=digest(first),
                twin_sha256=digest(second), gcc_o2_text_bytes=gcc_o2_text_bytes,
                compile_wall_ns=compile_wall_ns,
                **{field: metrics[field] for field in SIZE_FIELDS})


def steps_from_reports(reports, plan_sha256, expected_cases, aliases=None):
    """Consume N02 telemetry, without executing or changing either interpreter.

    Diagnostic parity is correctness evidence but has no successful step count.
    Missing passes/cases remain missing, rather than becoming zero-step results.
    """
    aliases = aliases or {}
    cases = {}
    for report in reports:
        if report.get("schema") == "evocc-pass-corpus-v1":
            passes = [dict(executions=report["cases"])]
        else:
            require(report.get("schema") == "evocc-pass-report-v1", "unsupported N02 report")
            passes = report["passes"]
        for pipeline_pass in passes:
            for execution in pipeline_pass["executions"]:
                name = aliases.get(execution["input_id"], execution["input_id"])
                require(name in expected_cases, "unplanned N02 execution")
                row = dict(case=name, input_sha256=execution["input_sha256"],
                           before_output_sha256=execution["before_outcome_sha256"],
                           after_output_sha256=execution["after_outcome_sha256"],
                           before_steps=execution["before_steps"], after_steps=execution["after_steps"],
                           status="PASS" if execution["outcome"] == "DEFINED_MATCH" else
                           "DIAGNOSTIC_MATCH" if execution["outcome"].startswith("DIAGNOSTIC_MATCH") else "UNKNOWN")
                row['validation'] = execution.get('validation', 'NOT_REPORTED')
                row['source_bridge'] = execution.get('source_bridge', 'NOT_REPORTED')
                detail = report.get('source_bridge_detail')
                row['source_bridge_detail'] = detail if detail and detail['input_id'] == execution['input_id'] else None
                if name in cases:
                    prior = cases[name]
                    require(prior["input_sha256"] == row["input_sha256"] and
                            prior["after_output_sha256"] == row["before_output_sha256"] and
                            prior["after_steps"] == row["before_steps"], "discontinuous N02 pass execution")
                    row["before_output_sha256"] = prior["before_output_sha256"]
                    row["before_steps"] = prior["before_steps"]
                    if prior["status"] != row["status"]:
                        row["status"] = "UNKNOWN"
                    for field in ('validation', 'source_bridge'):
                        if row[field] == 'NOT_REPORTED':
                            row[field] = prior[field]
                        elif prior[field] not in ('NOT_REPORTED', row[field]):
                            row[field] = 'MIXED'
                    if prior['source_bridge_detail'] is not None:
                        require(row['source_bridge_detail'] in (None, prior['source_bridge_detail']),
                                'different source-bridge diagnostics for the same input')
                        row['source_bridge_detail'] = prior['source_bridge_detail']
                cases[name] = row
    return dict(plan_sha256=plan_sha256, expected_cases=list(expected_cases),
                cases=[cases[name] for name in expected_cases if name in cases])


def _commit(value):
    require(isinstance(value, str) and re.fullmatch(r"[0-9a-f]{40}", value),
            "invalid source commit")


def _named_rows(rows, field):
    require(type(rows) is list, "rows must be a list")
    seen = set()
    for row in rows:
        require(type(row) is dict and isinstance(row.get(field), str), "invalid named row")
        require(row[field] not in seen, "duplicate " + field)
        seen.add(row[field])
    return seen


def validate_observation(record):
    """Check the compact data shape; no producer result is independently certified."""
    keys(record, "schema source_commit compiler_sha256 plan_sha256 inputs_sha256 "
         "capture_sha256 variant units ladder steps proof")
    require(record["schema"] == "evocc-codegen-trend-v1", "unsupported trend schema")
    _commit(record["source_commit"])
    for name in ("compiler_sha256", "plan_sha256", "inputs_sha256", "capture_sha256"):
        sha256_identity(record[name])
    require(isinstance(record["variant"], str) and
            re.fullmatch(r"[A-Za-z0-9_.+-]{1,80}", record["variant"]), "invalid variant")
    require(type(record["units"]) is list, "units must be a list")
    seen = set()
    for row in record["units"]:
        keys(row, "unit profile object_sha256 twin_sha256 gcc_o2_text_bytes compile_wall_ns "
             + " ".join(SIZE_FIELDS))
        test_unit = isinstance(row["unit"], str) and re.fullmatch(r"test:[A-Za-z0-9_.-]{1,80}", row["unit"])
        require((row["unit"] in UNITS or test_unit) and row["profile"] in PROFILES, "unknown unit/profile")
        key = row["unit"], row["profile"]
        require(key not in seen, "duplicate unit/profile")
        seen.add(key)
        for name in ("object_sha256", "twin_sha256"):
            sha256_identity(row[name])
        require(row["object_sha256"] == row["twin_sha256"], "nonidentical S1 twins")
        for name in SIZE_FIELDS:
            if name == "static_instruction_slots" and row[name] is None:
                continue
            natural(row[name])
        if row["gcc_o2_text_bytes"] is not None:
            natural(row["gcc_o2_text_bytes"])
            require(row["gcc_o2_text_bytes"] > 0, "empty GCC reference text")
        require(row["text_bytes"] == sum(row[name] for name in (
            "instruction_bytes", "padding_bytes", "literal_pool_or_intext_data_bytes",
            "unclassified_text_bytes")), "inconsistent S1 text partition")
        if row["static_instruction_slots"] is not None:
            require(row["instruction_bytes"] == 4 * row["static_instruction_slots"],
                    "inconsistent AArch64 instruction slots")
        samples = row["compile_wall_ns"]
        if samples is not None:
            require(type(samples) is list and len(samples) == 3, "need three compile observations")
            for value in samples:
                natural(value)
                require(value > 0, "zero compile duration")
    _named_rows(record["ladder"], "case")
    for row in record["ladder"]:
        keys(row, "case status result_sha256")
        require(row["case"] in LADDER_CASES and row["status"] in OUTCOMES,
                "unknown ladder case/status")
        if row["result_sha256"] is not None:
            sha256_identity(row["result_sha256"])
        require(row["status"] != "PASS" or row["result_sha256"] is not None,
                "missing ladder result hash")
    steps = record["steps"]
    if steps is not None:
        keys(steps, "plan_sha256 expected_cases cases")
        sha256_identity(steps["plan_sha256"])
        require(type(steps["expected_cases"]) is list and steps["expected_cases"] and
                all(isinstance(name, str) and re.fullmatch(r"[A-Za-z0-9_.:+/-]{1,160}", name)
                    for name in steps["expected_cases"]) and
                len(set(steps["expected_cases"])) == len(steps["expected_cases"]),
                "invalid expected steps population")
        _named_rows(steps["cases"], "case")
        for row in steps["cases"]:
            optional = ' validation source_bridge source_bridge_detail' if 'source_bridge' in row else ''
            keys(row, "case input_sha256 before_output_sha256 after_output_sha256 "
                 "before_steps after_steps status" + optional)
            if optional:
                for field in ('validation', 'source_bridge'):
                    require(type(row[field]) is str and re.fullmatch(r'[A-Z_]{1,80}', row[field]),
                            'invalid interpreter observation label')
                detail = row['source_bridge_detail']
                if detail is not None:
                    keys(detail, 'input_id chir_events evoir_events chir_return evoir_return first_difference')
                    require(type(detail['input_id']) is str, 'invalid source-bridge input')
                    for field in ('chir_events', 'evoir_events'):
                        natural(detail[field])
                    for field in ('chir_return', 'evoir_return'):
                        require(type(detail[field]) is int and 0 <= detail[field] < 1 << 128,
                                'invalid source-bridge return value')
                    if detail['first_difference'] is not None:
                        natural(detail['first_difference'])
            require(re.fullmatch(r"[A-Za-z0-9_.:+/-]{1,160}", row["case"]), "invalid steps case")
            require(row["case"] in steps["expected_cases"], "unplanned steps case")
            sha256_identity(row["input_sha256"])
            require(row["status"] in OUTCOMES | {"DIAGNOSTIC_MATCH"}, "unknown steps status")
            for name in ("before_output_sha256", "after_output_sha256"):
                if row[name] is not None:
                    sha256_identity(row[name])
                require(row["status"] != "PASS" or row[name] is not None,
                        "missing steps output hash")
            for name in ("before_steps", "after_steps"):
                if row[name] is not None:
                    natural(row[name])
                require(row["status"] != "PASS" or row[name] is not None,
                        "missing successful step count")
            if row["status"] == "DIAGNOSTIC_MATCH":
                require(row["before_output_sha256"] is not None and row["after_output_sha256"] is not None
                        and row["before_steps"] is None and row["after_steps"] is None,
                        "diagnostic parity has no successful step count")
    if record["proof"] is not None:
        keys(record["proof"], "completed_rejections bundle_failures")
        for value in record["proof"].values():
            natural(value)
    return record


def coverage(record):
    validate_observation(record)
    missing, failures = [], []
    present = {(row["unit"], row["profile"]) for row in record["units"]}
    for unit in UNITS:
        for profile in PROFILES:
            if (unit, profile) not in present:
                missing.append("size:" + unit + ":" + profile)
    for row in record["units"]:
        if row["gcc_o2_text_bytes"] is None:
            missing.append("GCC-O2:" + row["unit"])
        if row["compile_wall_ns"] is None:
            missing.append("compile:" + row["unit"] + ":" + row["profile"])
    ladder = {row["case"]: row for row in record["ladder"]}
    for name in LADDER_CASES:
        state = ladder.get(name, {}).get("status", "NOT_RUN")
        if state == "FAIL":
            failures.append("ladder:" + name)
        elif state != "PASS":
            missing.append("ladder:" + name + ":" + state)
    steps = record["steps"]
    if steps is None or not any(row["case"] == "integer-mix" for row in steps["cases"]):
        missing.append("steps:integer-mix")
    if steps:
        present_steps = {row["case"] for row in steps["cases"]}
        missing.extend("steps:" + name for name in steps["expected_cases"] if name not in present_steps)
        for row in steps["cases"]:
            if row["status"] == "FAIL" or (row["status"] in {"PASS", "DIAGNOSTIC_MATCH"} and
                    row["before_output_sha256"] != row["after_output_sha256"]):
                failures.append("steps:" + row["case"])
            elif row["status"] not in {"PASS", "DIAGNOSTIC_MATCH"}:
                missing.append("steps:" + row["case"] + ":" + row["status"])
    if record["proof"] is None:
        missing.append("proof")
    elif any(record["proof"].values()):
        failures.append("proof")
    return dict(status="FAIL" if failures else "NOT_RUN" if missing else "OBSERVED",
                missing=missing, failures=failures)


def _load_lines(text):
    records = []
    for number, line in enumerate(text.splitlines(), 1):
        require(line.strip(), "empty trend line " + str(number))
        try:
            records.append(validate_observation(json.loads(line, object_pairs_hook=unique)))
        except (ValueError, TypeError) as error:
            raise ValueError("invalid trend line " + str(number) + ": " + str(error)) from error
    return records


def read_observations(path):
    path = Path(path)
    return _load_lines(path.read_text()) if path.exists() else []


def append_observation(path, record):
    validate_observation(record)
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("a+", encoding="utf-8") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        stream.seek(0)
        records = _load_lines(stream.read())
        key = record["source_commit"], record["variant"], record["capture_sha256"]
        for existing in records:
            if (existing["source_commit"], existing["variant"], existing["capture_sha256"]) == key:
                require(existing == record, "conflicting observation; history is immutable")
                return False
        stream.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
        stream.flush()
    return True


def _identity(record):
    return record["plan_sha256"], record["inputs_sha256"], record["variant"]


def _ratio(after, before):
    return after / before if before else (1.0 if after == 0 else None)


def compare(before, after):
    """Per-unit changes with separately visible correctness/coverage and bounds."""
    validate_observation(before)
    validate_observation(after)
    require(_identity(before) == _identity(after), "incomparable trend population or variant")
    old = {(r["unit"], r["profile"]): r for r in before["units"]}
    new = {(r["unit"], r["profile"]): r for r in after["units"]}
    rows, steps = [], []
    for key in sorted(old.keys() | new.keys(), key=lambda item:
                      (UNITS.index(item[0]) if item[0] in UNITS else len(UNITS), item[0], PROFILES.index(item[1]))):
        if key not in old or key not in new:
            rows.append(dict(unit=key[0], profile=key[1], status="NOT_RUN"))
            continue
        a, b = old[key], new[key]
        require(a["gcc_o2_text_bytes"] == b["gcc_o2_text_bytes"], "incomparable GCC reference")
        compile_ratio = None
        if a["compile_wall_ns"] is not None and b["compile_wall_ns"] is not None:
            compile_ratio = statistics.median(b["compile_wall_ns"]) / statistics.median(a["compile_wall_ns"])
        regressions = []
        if b["text_bytes"] > a["text_bytes"]:
            regressions.append("text_bytes")
        if b["text_bytes"] + b["rodata_bytes"] > a["text_bytes"] + a["rodata_bytes"]:
            regressions.append("text_plus_rodata_bytes")
        if compile_ratio is not None and compile_ratio > 1.25:
            regressions.append("compile_time")
        rows.append(dict(unit=key[0], profile=key[1], status="REGRESSION" if regressions else "NO_REGRESSION",
                         text_before=a["text_bytes"], text_after=b["text_bytes"],
                         text_delta=b["text_bytes"] - a["text_bytes"],
                         rodata_delta=b["rodata_bytes"] - a["rodata_bytes"],
                         gcc_o2_ratio=b["text_bytes"] / b["gcc_o2_text_bytes"] if b["gcc_o2_text_bytes"] else None,
                         compile_ratio=compile_ratio, regressions=regressions))
    if before["steps"] and after["steps"]:
        require(before["steps"]["plan_sha256"] == after["steps"]["plan_sha256"],
                "incomparable steps plan")
        prior = {row["case"]: row for row in before["steps"]["cases"]}
        current = {row["case"]: row for row in after["steps"]["cases"]}
        for name in sorted(prior.keys() | current.keys()):
            a, b = prior.get(name), current.get(name)
            if not a or not b or a["status"] not in {"PASS", "DIAGNOSTIC_MATCH"} or b["status"] not in {"PASS", "DIAGNOSTIC_MATCH"}:
                steps.append(dict(case=name, status="NOT_RUN"))
                continue
            require(a["input_sha256"] == b["input_sha256"], "incomparable step input")
            if a["after_output_sha256"] != b["after_output_sha256"] or a["status"] != b["status"]:
                steps.append(dict(case=name, status="FAIL"))
                continue
            if a["status"] == "DIAGNOSTIC_MATCH":
                steps.append(dict(case=name, status="DIAGNOSTIC_MATCH_NO_STEP_METRIC"))
                continue
            ratio = _ratio(b["after_steps"], a["after_steps"])
            steps.append(dict(case=name, ratio=ratio,
                              status="TRADEOFF" if ratio is None or ratio > 1.05 else "WITHIN_BOUND"))
    before_coverage, after_coverage = coverage(before), coverage(after)
    regressed = any(row["status"] == "REGRESSION" for row in rows)
    tradeoff = any(row["status"] == "TRADEOFF" for row in steps)
    incomplete = any(r["status"] != "OBSERVED" for r in (before_coverage, after_coverage))
    failed = any(r["status"] == "FAIL" for r in (before_coverage, after_coverage)) or any(r["status"] == "FAIL" for r in steps)
    return dict(status="FAIL" if failed else "REGRESSION" if regressed else "TRADEOFF" if tradeoff else
                "INCOMPLETE" if incomplete else "NO_REGRESSION", units=rows, steps=steps,
                before_coverage=before_coverage, after_coverage=after_coverage)


def _metric(record, unit, profile, metric):
    if metric == "steps":
        if record["steps"]:
            return next((r["after_steps"] for r in record["steps"]["cases"]
                         if r["case"] == unit and r["status"] == "PASS" and
                         r["before_output_sha256"] == r["after_output_sha256"]), None)
        return None
    row = next((r for r in record["units"] if (r["unit"], r["profile"]) == (unit, profile)), None)
    if row is None:
        return None
    if metric == "compile_time":
        return statistics.median(row["compile_wall_ns"]) if row["compile_wall_ns"] else None
    require(metric == "text_bytes", "unsupported bisect metric")
    return row["text_bytes"]


def locate_regression(commits, records, unit, profile, metric="text_bytes", variant="O2-p0"):
    """Find the earliest evidenced bad boundary without assuming monotonic history.

    ``commits`` must come from the caller's actual Git first-parent traversal.
    Unknown early commits cannot be skipped merely because a later receipt is
    good. Missing points after an already proven first bad commit need no build.
    """
    require(len(commits) >= 2 and len(set(commits)) == len(commits), "need distinct good/bad ancestry")
    require(metric in {"text_bytes", "compile_time", "steps"}, "unsupported bisect metric")
    require(profile in PROFILES, "unsupported bisect profile")
    require(metric == "steps" or unit in UNITS or re.fullmatch(r"test:[A-Za-z0-9_.-]{1,80}", unit),
            "unknown bisect unit")
    for commit in commits:
        _commit(commit)
    for row in records:
        validate_observation(row)
    candidates = [r for r in records if r["source_commit"] == commits[0] and r["variant"] == variant]
    identities = {_identity(r) for r in candidates}
    require(len(identities) <= 1, "ambiguous good-commit population")
    identity = next(iter(identities), None)
    step_population = None
    if metric == "steps":
        signatures = {(r["steps"]["plan_sha256"], tuple(r["steps"]["expected_cases"]),
                       row["input_sha256"]) for r in candidates if r["steps"]
                      for row in r["steps"]["cases"] if row["case"] == unit}
        require(len(signatures) <= 1, "ambiguous good-commit steps population")
        step_population = next(iter(signatures), None)
    values, missing, ambiguous = {}, [], []
    for commit in commits:
        matching = [r for r in records if r["source_commit"] == commit and
                    r["variant"] == variant and _identity(r) == identity]
        if metric == "steps":
            matching = [r for r in matching if r["steps"] and any(
                (r["steps"]["plan_sha256"], tuple(r["steps"]["expected_cases"]), row["input_sha256"])
                == step_population for row in r["steps"]["cases"] if row["case"] == unit)]
        observed = {_metric(r, unit, profile, metric) for r in matching}
        observed.discard(None)
        if len(observed) > 1:
            ambiguous.append(commit)
        elif len(observed) == 1:
            values[commit] = observed.pop()
        else:
            missing.append(commit)
    base = values.get(commits[0])
    threshold = 1.25 if metric == "compile_time" else 1.05 if metric == "steps" else 1
    first_bad = None
    if base is not None:
        first_bad = next((c for c in commits[1:] if c in values and values[c] > base * threshold), None)
    boundary = commits.index(first_bad) if first_bad else len(commits) - 1
    unresolved = [c for c in commits[:boundary + 1] if c in missing or c in ambiguous]
    proven = first_bad is not None and not unresolved
    return dict(status="REGRESSION_FOUND" if proven else "INCOMPLETE" if unresolved else "NO_REGRESSION",
                first_bad=first_bad if proven else None,
                first_observed_bad=first_bad,
                last_evidenced_good=next((c for c in reversed(commits[:boundary]) if c in values and
                                         base is not None and values[c] <= base * threshold), None),
                rebuild_commits=[c for c in unresolved if c not in ambiguous],
                ambiguous_commits=[c for c in unresolved if c in ambiguous],
                metric=metric, unit=unit, profile=profile, baseline=base,
                observed=[dict(commit=c, value=values[c]) for c in commits if c in values])


def git_commits(root, good, bad):
    """Read history only; never checkout, reset or manufacture a receipt."""
    def git(*args):
        return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()
    good = git("rev-parse", "--verify", good + "^{commit}")
    bad = git("rev-parse", "--verify", bad + "^{commit}")
    _commit(good)
    _commit(bad)
    history = git("rev-list", "--first-parent", bad).splitlines()
    require(good in history, "good is not on bad's first-parent ancestry")
    return list(reversed(history[:history.index(good) + 1]))


def render_trend(records):
    if not records:
        return "Codegen trend: NOT_RUN (no observations)\n"
    for row in records:
        validate_observation(row)
    current = records[-1]
    compatible = [row for row in records if _identity(row) == _identity(current)]
    baseline = compatible[0]
    result = compare(baseline, current)
    lines = ["Codegen trend (read-only; latency NOT_MEASURED)",
             "  baseline=" + baseline["source_commit"][:12] + " current=" + current["source_commit"][:12],
             "  variant=" + current["variant"] + " coverage=" + coverage(current)["status"],
             "  TU / profile | baseline -> current | delta | GCC-O2 | compile bound"]
    for row in result["units"]:
        if row["status"] == "NOT_RUN":
            lines.append("  " + row["unit"] + " / " + row["profile"] + " | NOT_RUN")
            continue
        compile_text = "NOT_RUN" if row["compile_ratio"] is None else f'{row["compile_ratio"]:.3f}x / 1.250x'
        reference_text = "NOT_RUN" if row["gcc_o2_ratio"] is None else f'{row["gcc_o2_ratio"]:.3f}x'
        lines.append(f'  {row["unit"]} / {row["profile"]} | {row["text_before"]} -> {row["text_after"]}'
                     f' | {row["text_delta"]:+d} | {reference_text} | {compile_text}')
    step_counts = {}
    for row in result['steps']:
        step_counts[row['status']] = step_counts.get(row['status'], 0) + 1
    lines.append('  steps: ' + (', '.join(f'{status}={count}' for status, count in sorted(step_counts.items())) or 'NOT_RUN'))
    if current['steps']:
        bridge = [row['case'] + '=' + row['source_bridge'] for row in current['steps']['cases']
                  if row.get('source_bridge') not in (None, 'NOT_REPORTED', 'NOT_RUN')]
        lines.append('  source bridge (not TV): ' + (', '.join(bridge) or 'NOT_RUN'))
    lines.append("  ladder: " + (", ".join(r["case"] + "=" + r["status"] for r in current["ladder"]) or "NOT_RUN"))
    lines.append("  comparison=" + result["status"] + "; target remains <=1.400x GCC-14 -O2 per unchecked TU")
    lines.append("  Prior observations retained: " + str(len(records)) + "; no cross-family average.")
    return "\n".join(lines) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--history", type=Path, required=True)
    parser.add_argument("--append", type=Path, help="append a compact observation, never raw capture")
    args = parser.parse_args(argv)
    if args.append:
        append_observation(args.history, json.loads(args.append.read_text(), object_pairs_hook=unique))
    print(render_trend(read_observations(args.history)), end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
