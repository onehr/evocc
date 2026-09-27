#!/usr/bin/env python3
"""Render receipt-backed roadmap status without launching tests.

Usage: python3 tools/render-status.py           # update docs/status.md
       python3 tools/render-status.py --ascii   # read-only terminal overview
       python3 tools/render-status.py --check   # compare with docs/status.md
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys
import textwrap
import unicodedata

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "status.md"

APPLICATIONS = (
    ("E0a", "torture"), ("E0b", "ctest"), ("E0c", "csmith"),
    ("E1", "zlib"), ("E2", "zstd"), ("E3", "cjson"),
    ("E4", "mbedtls"), ("E5", "tinycc"), ("E6", "toybox"),
    ("E7", "musl"), ("E8", "polybench"), ("E9", "llvm-test-suite"),
    ("E10", "jq"), ("E11", "lua"), ("E12", "sqlite"),
)
RECEIPTS = (
    "bench-receipt.json", "kernel-full-receipt.json",
    "kernel-bpf-full-receipt.json", "kernel-selftest-receipt.json",
    "kernel-selftest-full-receipt.json", "kernel-kunit-receipt.json",
    "kernel-expanded-kunit-receipt.json", "semantic-gate-contract.json",
    "kernel-kunit-bench-receipt.json",
    "lean-p0-receipt.json", "lean-correspondence-receipt.json",
)
OUTCOMES = ("PASS", "FAIL", "SKIP", "TIMEOUT", "INVALID_OUTPUT", "INCOMPLETE", "NOT_RUN")


def cells(text: str) -> int:
    """Display width, also used by the existing renderer regression tests."""
    return sum(2 if unicodedata.east_asian_width(ch) in "WF" else 1 for ch in text)


def record_count(value) -> int:
    """Receipts may carry either records or their already-computed count."""
    return value if isinstance(value, int) else len(value or [])


def revision(path: Path) -> str:
    found = re.search(r"\*\*Revision ([0-9.]+)\*\*", path.read_text())
    return found.group(1) if found else "?"


def contract(name: str) -> dict:
    path = ROOT / "tests" / "contracts" / name
    return json.loads(path.read_text()) if path.exists() else {}


def load_receipts() -> dict[str, dict]:
    # Read each checkpoint once, especially the live paired outcome summaries.
    receipts = {name: contract(name) for name in RECEIPTS}
    for name in ("kernel-selftest-receipt.json", "kernel-selftest-full-receipt.json"):
        path = receipts[name].get("kernelReceiptPath")
        if path and path not in receipts:
            key = Path(path).name
            if path == f"tests/contracts/{key}" and key in receipts:
                receipts[path] = receipts[key]
            else:
                source = ROOT / path
                receipts[path] = json.loads(source.read_text()) if source.exists() else {}
    return receipts


def ratio(done, total) -> str:
    if not isinstance(total, int) or total <= 0 or not isinstance(done, int):
        return "unmeasured"
    return f"{done:,}/{total:,}"


def validated(row: dict):
    # Raw executions can include programs later found outside the profile.
    return (row.get("dispositions") or {}).get("PASS", row.get("passed"))


def lane_status(row: dict) -> str:
    state = row.get("status", "NOT_RUN")
    if state in ("PASS", "PROFILE_CLEAN"):
        passed, total = validated(row), row.get("total")
        if (not isinstance(total, int) or total <= 0 or not isinstance(passed, int)
                or not 0 <= passed <= total or (state == "PASS" and passed != total)):
            return "UNVERIFIED"
        if (row.get("dispositions") or {}).get("compiler-failure", 0):
            return "UNVERIFIED"
    return state


def measurement(row: dict) -> str:
    return (row.get("measurement") or {}).get("run") or row.get("run") or "unmeasured"


def run_id(receipt: dict) -> str:
    return receipt.get("runId") or receipt.get("run") or "unmeasured"


def kernel_verdict(receipt: dict) -> tuple[str, str, str]:
    """A boot PASS also needs the complete link and the same Image."""
    total = receipt.get("requestedUnits")
    compiled = receipt.get("counts", {}).get("substituted")
    linked = (isinstance(total, int) and total > 0 and compiled == total
              and receipt.get("completeComposition") is True
              and receipt.get("relinkExitCode") == 0)
    boot = receipt.get("boot", {})
    booted = (linked and boot.get("verdict") == "PASS"
              and bool(receipt.get("imageSha256"))
              and boot.get("imageSha256") == receipt["imageSha256"])
    state = receipt.get("status", "NOT_RUN")
    if state == "PASS" and not booted:
        state = "UNVERIFIED"
    return state, "PASS" if linked else "NOT_VERIFIED", "PASS" if booted else "NOT_VERIFIED"


def kernel_selftest_status(receipt: dict, full_kernel: dict,
                           named_kernels: dict[str, dict] | None = None) -> str:
    if not receipt:
        return "NOT_RUN"
    kernel = full_kernel
    if path := receipt.get("kernelReceiptPath"):
        if named_kernels is not None:
            key = Path(path).name
            fallback = named_kernels.get(key, {}) if path == f"tests/contracts/{key}" else {}
            kernel = named_kernels.get(path, fallback)
        else:
            source = ROOT / path
            kernel = json.loads(source.read_text()) if source.exists() else {}
    expected = kernel.get("imageSha256") or kernel.get("builtInComposition", {}).get("imageSha256")
    if not expected or receipt.get("kernelImageSha256") != expected:
        return "STALE_IMAGE"
    return receipt.get("status", "NOT_RUN")


def kunit_progress(receipt: dict) -> str:
    parts = []
    for name in ("reference", "subject"):
        entry = receipt.get(name, {})
        counts = entry.get("outcomes")
        measured = (f"{counts.get('PASS', 0)} PASS / {counts.get('FAIL', 0)} FAIL / "
                    f"{counts.get('SKIP', 0)} SKIP") if counts is not None else entry.get("status", "NOT_RUN")
        parts.append(f"{name} {measured}")
        modules = entry.get("modules")
        if modules is not None:
            counts = modules.get("outcomes")
            measured = (f"{modules.get('filesLoaded', 0)} files loaded, "
                        f"{counts.get('PASS', 0)} PASS / {counts.get('FAIL', 0)} FAIL / "
                        f"{counts.get('SKIP', 0)} SKIP") if counts is not None else modules.get("status", "NOT_RUN")
            parts.append(f"{name} modules {measured}")
    for followup in receipt.get("targetedFollowups", []):
        counts = followup.get("outcomes")
        measured = (f"{counts.get('PASS', 0)} PASS / {counts.get('FAIL', 0)} FAIL / "
                    f"{counts.get('SKIP', 0)} SKIP") if counts is not None else "NOT_RUN"
        parts.append(f"follow-up {followup.get('suite', 'unnamed')} "
                     f"({followup.get('tests', '?')} tests): {measured}; "
                     "filtered; not a full KUnit rerun")
    records = receipt.get("config", {}).get("targetCRecords")
    label = f"KUnit ({records:,} C records)" if records is not None else "KUnit"
    return label + ":\n  " + "\n  ".join(parts)


def selftest_progress(receipt: dict) -> list[str]:
    lines = []
    for name in ("reference", "subject"):
        summary = receipt.get("execution", {}).get(name, {}).get("summary", {})
        counts = summary.get("outcomes")
        if not counts:
            lines.append(f"  {name}: NOT_RUN (no program outcomes)")
            continue
        total = summary.get("programs")
        recorded = sum(n for outcome, n in counts.items() if outcome not in ("NOT_RUN", "RUNNING"))
        width = 16
        filled = int(width * min(recorded / total, 1)) if total else 0
        lines.append(f"  {name:9} [{'#' * filled}{'-' * (width - filled)}] "
                     f"{ratio(recorded, total)} recorded (not PASS)")
        keys = [*OUTCOMES, *sorted(set(counts) - set(OUTCOMES))]
        detail = " / ".join(f"{counts.get(key, 0)} {key}" for key in keys)
        lines.extend(textwrap.wrap(detail, width=105, initial_indent="    ", subsequent_indent="    "))
    return lines


def render_ascii(receipts: dict[str, dict], *, evoir: str, evomir: str) -> str:
    bench = receipts.get("bench-receipt.json", {})
    rows = bench.get("benchmarks", {})
    states = Counter(lane_status(rows.get(name, {})) for _, name in APPLICATIONS)
    totals = " | ".join(f"{count} {state}" for state, count in sorted(states.items()))
    rule = "=" * 105
    lines = [
        f"EvoCC roadmap checkpoint | EvoIR {evoir} | EvoMIR {evomir}", rule,
        "APPLICATIONS - each lane keeps its own frozen measurement", f"  {totals}",
        f"  {'Rung':4} {'Benchmark':16} {'Status':15} {'Validated':20} Measurement (UTC run ID)",
        "  " + "-" * 103,
    ]
    for rung, name in APPLICATIONS:
        row = rows.get(name, {})
        kind = "TU" if row.get("kind") == "units" else row.get("kind", "")
        amount = f"{ratio(validated(row), row.get('total'))} {kind}".rstrip()
        lines.append(f"  {rung:4} {name:16} {lane_status(row):15} {amount:20} {measurement(row)}")
    lines += [
        "  PROFILE_CLEAN excludes evidenced deferrals/inapplicability; it is not all-program PASS.",
        "  TU = translation unit. Mixed-link coverage and whole-program PASS are different claims.",
        "", "KERNEL - configurations and runtime populations remain separate",
    ]
    # An adapter can stop before replacing its last completed lane receipt.
    # Show the selected attempt independently, so that an old K6 PASS cannot
    # conceal the new attempt's RUNNING/ERROR/INTERRUPTED state. A receipt is
    # still a checkpoint, not proof that its recorded process remains alive.
    pending_kernel = False
    for name in bench.get('selected', []):
        if name == 'kernel' or name.startswith('kernel-'):
            state = rows.get(name, {}).get('status', 'NOT_RUN')
            lines.append(f"  Attempt {name}: {state}; run={run_id(bench)}")
            pending_kernel |= state != 'PASS'
    if pending_kernel:
        lines.append('    Retained compositions below are not this attempt\'s acceptance.')
    full = receipts.get("kernel-full-receipt.json", {})
    bpf = receipts.get("kernel-bpf-full-receipt.json", {})
    for label, receipt in (("locked", full), ("bpf", bpf)):
        state, link, boot = kernel_verdict(receipt)
        amount = ratio(receipt.get("counts", {}).get("substituted"), receipt.get("requestedUnits"))
        lines.append(f"  K6 {label:6} {state:11} {amount} target-C; link={link}; boot={boot}")
        lines.append(f"    run={run_id(receipt)}")
        if receipt.get("notASingleCompilerSurvey"):
            repair = receipt.get("incrementalRepair", {})
            lines.append(f"    incremental: {receipt.get('freshCompilations', '?')} rebuilt / "
                         f"{repair.get('unchangedObjects', '?')} unchanged; not a fresh survey")
    modules = bpf.get("moduleLink", {})
    count = record_count(modules.get("modules"))
    module_state = modules.get("status", "NOT_RUN")
    if module_state == "PASS" and (count <= 0 or modules.get("linkExitCode") != 0):
        module_state = "UNVERIFIED"
    lines.append(f"  BPF module link/BTF: {module_state}; {count} modules (not a load-every-driver claim)")

    selected = receipts.get("kernel-selftest-receipt.json", {})
    selected_state = kernel_selftest_status(selected, full, receipts)
    cases = selected.get("subject", {}).get("subtests")
    if selected_state == "PASS" and (not cases or cases.get("PASS", 0) <= 0
                                     or any(value for key, value in cases.items() if key != "PASS")):
        selected_state = "UNVERIFIED"
    keys = ["PASS", "FAIL", "SKIP", *sorted(set(cases or {}) - {"PASS", "FAIL", "SKIP"})]
    result = " / ".join(f"{cases.get(k, 0)} {k}" for k in keys) if cases else "unmeasured"
    lines.append(f"  K7 selected {selected_state}: {result}; run={run_id(selected)}")
    live = receipts.get("kernel-selftest-full-receipt.json", {})
    selection = live.get('selection')
    label = 'targeted replay' if selection else 'all programs'
    lines.append(f"  K7 {label} {kernel_selftest_status(live, full, receipts)}; run={run_id(live)}")
    if selection:
        lines.append(f"    {len(selection.get('programs', []))} selected; full inventory retained; "
                     "not a new full campaign")
    lines.extend(selftest_progress(live))
    installation = live.get("moduleInstallations", {}).get("subject", {})
    if installation:
        state = installation.get("status", "NOT_RUN")
        if not live.get("kernelImageSha256") or installation.get("imageSha256") != live["kernelImageSha256"]:
            state = "STALE_IMAGE"
        elif state == "PASS" and record_count(installation.get("modules")) <= 0:
            state = "UNVERIFIED"
        lines.append(f"  Module installation: {state}; {record_count(installation.get('modules'))} subject modules")
        lines.append("    Signing/installation is not execution; helper compiler identities remain in the receipt.")

    kunit = receipts.get("kernel-expanded-kunit-receipt.json", {})
    if kunit:
        lines.append(f"  Expanded KUnit: {kunit.get('status', 'NOT_RUN')}; "
                     f"executionComplete={kunit.get('executionComplete', False)}")
    else:
        kunit = receipts.get("kernel-kunit-receipt.json", {})
        lines.append(f"  Earlier KUnit configuration: {kunit.get('status', 'NOT_RUN')}")
    lines.extend("  " + line for line in kunit_progress(kunit).splitlines())
    unified = receipts.get('kernel-kunit-bench-receipt.json', {})
    lines.append(f"  Unified KUnit bench: {unified.get('status', 'NOT_RUN')}; "
                 f"executionComplete={unified.get('executionComplete', False)}; run={run_id(unified)}")
    if unified:
        lines.extend('    ' + line for line in kunit_progress(unified).splitlines())

    gates = receipts.get("semantic-gate-contract.json", {}).get("receipt", {})
    semantic, machine = gates.get("campaign", {}), gates.get("b1", {}).get("campaign", {})
    lines += ["", "ARCHITECTURE / REMAINING ROADMAP GATES (bounded evidence, not a completion percentage)"]
    if semantic:
        lines.append(f"  Scalar semantic gate: {semantic.get('programs', '?')} random IR programs; "
                     f"{semantic.get('semanticMismatches', '?')} mismatches")
    else:
        lines.append("  Scalar semantic gate: NOT_RUN")
    if machine:
        lines.append(f"  MIR three-lane gate: {ratio(machine.get('lowered'), machine.get('programs'))}; "
                     f"{machine.get('threeLaneMismatches', '?')} mismatches")
    else:
        lines.append("  MIR three-lane gate: NOT_RUN")
    for target, key, field in (("Darwin", "b3", "cCorpus"), ("Linux", "b5", "linuxCorpus")):
        corpus = gates.get(key, {}).get(field, {})
        lines.append(f"  AArch64 {target:6} C corpus: {ratio(corpus.get('agreeing'), corpus.get('programs'))}")
    lean = receipts.get("lean-p0-receipt.json", {})
    if lean:
        n = len(lean.get("segments", {}).get("A", {}).get("targets", []))
        lines.append(f"  Lean scalar rules: {ratio(n, lean.get('binding', {}).get('registry_rules'))}; "
                     "not a proof of the entire compiler")
    correspondence = receipts.get("lean-correspondence-receipt.json", {}).get("total", {})
    if correspondence:
        lines.append(f"  Rust/Lean model differential: {correspondence.get('compared', '?')} vectors; "
                     f"{correspondence.get('differences', '?')} differences")
    lines += [
        "  FP: f32/f64 native + strict fenv; f128 interpreters; quad native/musl/FP complex/vector open",
        "  G2 EqSat whole-kernel reproducibility + every optimized TU proof: NOT_ACCEPTED",
        "  Q2 GCC -O2 same-task performance comparison: NOT_ACCEPTED",
        "  Q3 measured self-improvement benefit: NOT_ACCEPTED",
        "  H/x86 userspace and full release/research payload: NOT_ACCEPTED",
        "  These open gates have no completion receipt here; baseline K6 does not close them.",
        rule,
    ]
    return "\n".join(lines).encode("ascii", "backslashreplace").decode("ascii") + "\n"


def render_document(receipts: dict[str, dict], *, evoir: str, evomir: str) -> str:
    bench = receipts.get("bench-receipt.json", {})
    text = (
        "# EvoCC roadmap status\n\n"
        "Generated by `python3 tools/render-status.py`; "
        "read-only terminal view: `python3 tools/render-status.py --ascii`. "
        "This is a receipt checkpoint, not a live process monitor.\n\n"
        f"Benchmark orchestrator run: `{bench.get('run', 'unmeasured')}`; "
        f"checkout `{bench.get('subject', 'unknown')[:12]}`; "
        f"dirty tree: `{bench.get('dirty', 'unknown')}`. "
        "Each application row retains its own measurement and compiler identity; "
        "the current kernel run does not remeasure applications.\n\n"
        "```text\n" + render_ascii(receipts, evoir=evoir, evomir=evomir) + "```\n"
    )
    rows = bench.get("benchmarks", {})
    text += "\n## Application evidence and limits\n\n"
    text += "PASS is limited to the lane's recorded configuration, installed objects and original test selection. "
    text += "It does not mean every upstream configuration or dependency was compiled by EvoCC.\n\n"
    text += "| Benchmark | Classification / scope | Compiler SHA-256 (prefix) |\n|---|---|---|\n"
    for _, name in APPLICATIONS:
        row = rows.get(name, {})
        disposition = row.get("dispositions") or {}
        detail = "; ".join(f"{key}: {value}" for key, value in disposition.items())
        if not detail:
            outcomes = "; ".join(f"{key}: {value}" for key, value in row.get("outcomes", {}).items())
            detail = (outcomes + "; " if outcomes else "") + row.get("note", "No measurement receipt.")
        digest = (row.get("measurement") or {}).get("driverSha256") or "unrecorded"
        text += f"| {name} | {detail.replace('|', '/')} | `{digest[:12]}` |\n"
    text += (
        "\n- Scalar f32/f64 execution, HFA and variadic ABI are implemented under the fixed-environment "
        "profile. Dynamic rounding and lexical non-trapping FENV_ACCESS have targeted source, "
        "interpreter and native checks. Binary128 source/IR has targeted interpreter checks; "
        "original musl closure, binary128 native/ABI and floating "
        "complex/vector execution remain separate open work; "
        "see the [FP scope in the roadmap](roadmap.md#milestone-ladder-for-end-to-end-evaluation-added-2026-09-06).\n"
        "- PolyBench's current lane is MINI_DATASET correctness, not an optimization-speed result. "
        "LLVM's full-campaign count is not increased by later tailored repairs; "
        "see the [dated original-program and performance results](notebook/benchmark-evaluation-20260915.md).\n"
        "- F3/F4 surveys and the old mixed-kernel `kernel` bench row are historical measurements "
        "of different populations. They are not the current compiler's lowering ceiling. "
        "Current full compositions are reported separately above.\n"
        "- A Rust-built compiler is not C self-hosting. Kernel target-C coverage does not establish "
        "complete HOSTCC coverage. Doom has no accepted benchmark receipt yet.\n"
    )
    expanded = receipts.get("kernel-expanded-kunit-receipt.json", {})
    text += "\n## Kernel evidence and remaining work\n\n"
    if expanded.get("compositionNote"):
        text += f"Expanded KUnit composition: {expanded['compositionNote']}\n\n"
    if expanded.get("remaining"):
        text += f"Expanded KUnit remaining: {expanded['remaining']}\n\n"
    text += (
        "Full kselftest counts are program-level outer verdicts, not nested assertion counts. "
        "SKIP, TIMEOUT, INVALID_OUTPUT, INCOMPLETE and NOT_RUN are not PASS. "
        "Reference failures do not excuse a subject regression. A RUNNING receipt means the last "
        "recorded campaign state; use its capture/process to confirm liveness.\n\n"
        "Sources: [benchmark receipt](../tests/contracts/bench-receipt.json), "
        "[locked K6](../tests/contracts/kernel-full-receipt.json), "
        "[BPF/module K6](../tests/contracts/kernel-bpf-full-receipt.json), "
        "[selected K7](../tests/contracts/kernel-selftest-receipt.json), "
        "[full K7 checkpoint](../tests/contracts/kernel-selftest-full-receipt.json), "
        "[expanded KUnit](../tests/contracts/kernel-expanded-kunit-receipt.json).\n\n"
        "Open gate definitions: [launch logic and G2](roadmap.md#launch-logic-re-plan-of-2026-08-25). "
        "Runtime diagnoses: [K7 issue ledger](notebook/kernel-selftest-gaps-20260914.md). "
        "Historical failures remain in their original receipts; rendering does not rewrite them.\n"
    )
    return text


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--ascii", action="store_true", help="print ASCII only; do not write any file")
    modes.add_argument("--check", action="store_true", help="compare generated Markdown with docs/status.md")
    args = parser.parse_args(argv)
    receipts = load_receipts()
    versions = {"evoir": revision(ROOT / "docs/evoir-spec.md"), "evomir": revision(ROOT / "docs/evomir-spec.md")}
    if args.ascii:
        print(render_ascii(receipts, **versions), end="")
        return 0
    text = render_document(receipts, **versions)
    print(text, end="")
    if args.check:
        if (OUT.read_text() if OUT.exists() else "") != text:
            print("docs/status.md differs from the receipt checkpoint; rerun tools/render-status.py", file=sys.stderr)
            return 1
        return 0
    OUT.write_text(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
