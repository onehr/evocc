#!/usr/bin/env python3
"""Generate E3 obligations and qualify their pinned SMT translation.

The pinned decoder currently fails generation on undefined_bitvector. This
tool has no admitted SMT proof result; generation failure is not UNSAT.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import tempfile

from tools.isa_model_diff import instruction_sequences
from tools.ladder.eqsat_native import Capture, CommandExitError

ROOT = Path(__file__).resolve().parents[1]
QUERY = ("~(exist assertion) & all return & ~(exist match_failure) & "
         "~(exist overflow) & all assumption")
CONSTANTS = (0, 1, 255, 65535, 0xffffffff, 0xffffffffffffffff,
             0x123456789abcdef0, 0x8000000000000000)


def solver_verdict(exit_code, output):
    if exit_code in (124, 137):
        return "TIMEOUT"
    if exit_code != 0:
        return "SOLVER_ERROR"
    return {b"unsat": "UNSAT", b"sat": "SAT", b"unknown": "UNKNOWN"}.get(
        output.strip(), "SOLVER_ERROR")


def shape(name):
    match = re.fullmatch(r"model_(add|sub|mul|and|or|xor|shl|lshr|ashr|const)_(\d+)", name)
    if not match:
        raise ValueError("not a frozen model shape")
    op, width = match[1], int(match[2])
    if op == "const":
        if not 0 <= width < len(CONSTANTS):
            raise ValueError("constant index outside frozen corpus")
        return op, 64, f"0x{CONSTANTS[width]:016x}"
    if width not in (1, 2, 8, 16, 31, 32, 33, 63, 64):
        raise ValueError("width outside frozen corpus")
    expected = {
        "add": "av + bv", "sub": "av - bv",
        "mul": f"get_slice_int({width}, unsigned(av) * unsigned(bv), 0)",
        "and": "and_vec(av, bv)", "or": "or_vec(av, bv)", "xor": "xor_vec(av, bv)",
        "shl": "sail_shiftleft(av, unsigned(bv))",
        "lshr": "sail_shiftright(av, unsigned(bv))",
        "ashr": "sail_arith_shiftright(av, unsigned(bv))",
    }[op]
    return op, width, expected


def property_source(name, words, *, mutation=None, empty_domain=False):
    op, width, expected = shape(name)
    if not words or len(words) > 128 or any(not 0 <= word < 2**32 for word in words):
        raise ValueError("instruction sequence outside frozen bound")
    domain = f"unsigned(bv) < {width}" if op in ("shl", "lshr", "ashr") else "true"
    if empty_domain:
        domain = "false"
    bind = (f"    let av : bits({width}) = a[{width - 1}..0];\n"
            f"    let bv : bits({width}) = b[{width - 1}..0];\n")
    reset = "    R0 = a;\n    R1 = b;\n" + "".join(
        f"    R{r} = initial_gprs[{64*(r-1)-1}..{64*(r-2)}];\n" for r in range(2, 31))
    comparisons = (
        [f"R{r} == initial_gprs[{64*(r-1)-1}..{64*(r-2)}]" for r in range(2, 31)] +
        ["PSTATE == initial_pstate"])
    # A deep overloaded '&' expression caused exponential typechecking work
    # in the pinned Sail. Keep the identical conjunction explicitly Boolean.
    preserve = "".join(f"    let preserved_{i} : bool = {test};\n"
                       f"    correct = and_bool(correct, preserved_{i});\n"
                       for i, test in enumerate(comparisons))
    execute = "".join(f"    SEE = 0; __DecodeA64(0, 0x{word:08x});\n" for word in words)
    if mutation == "state":
        execute += "    R2 = not_vec(R2);\n"
    elif mutation is not None:
        raise ValueError("unknown negative control")
    return ("// Generated source obligation; instruction semantics are verbatim upstream.\n"
            f"$property :query {QUERY}\n"
            f"val prop_{name} : (bits(64), bits(64), bits(1856), ProcState) -> bool\n"
            f"function prop_{name}(a, b, initial_gprs, initial_pstate) = {{\n{bind}"
            f"    if {domain} then try {{\n{reset}    PSTATE = initial_pstate;\n{execute}"
            f"    let expected : bits({width}) = {expected};\n"
            f"    var correct : bool = R0 == sail_zero_extend(expected, 64);\n{preserve}"
            "    correct\n"
            "    } catch { _ => false } else true\n}\n\n"
            f"$property :query {QUERY}\n"
            f"val domain_{name} : (bits(64), bits(64)) -> bool\n"
            f"function domain_{name}(a, b) = {{\n{bind}    not_bool({domain})\n}}\n")


def control_source():
    header = f"$property :query {QUERY}\n"
    return (
        "// Deliberate translator-boundary failures, not compiler obligations.\n" + header +
        "val control_overflow : int -> bool\n"
        "function control_overflow(x) = { let y = x + 1; y > x }\n\n" + header +
        "val control_assumption : unit -> bool\n"
        "function control_assumption() = { sail_assume(false); true }\n\n" + header +
        "val control_loop : bits(5) -> bool\n"
        "function control_loop(x) = { var i = 0; while i < unsigned(x) do { i = i + 1 }; true }\n"
    )


def generate(directory):
    directory.mkdir(parents=True, exist_ok=True)
    previous = ROOT / ".evidence/codegen/n07-e2"
    sequences, _, _, _ = instruction_sequences((previous / "corpus-linux.o").read_bytes())
    recorded = json.loads((previous / "differential.json").read_text())["instruction_words"]
    if {name: [f"{w:08x}" for w in words] for name, words in sequences.items()} != recorded:
        raise ValueError("words differ from completed E2 differential")
    for name, words in sequences.items():
        (directory / (name + ".sail")).write_text(property_source(name, words))
    for name, words, options in (
        ("wrong-instruction", [0xcb010000], {}),
        ("wrong-state", [0x8b010000], {"mutation": "state"}),
        ("unknown-instruction", [0], {}),
        ("empty-domain", [0x8b010000], {"empty_domain": True}),
    ):
        (directory / (name + ".sail")).write_text(property_source("model_add_64", words, **options))
    (directory / "translator-controls.sail").write_text(control_source())
    print(f"Generated {len(sequences)} body obligations and five control files")


def run_qualification(base, container, *, constant_fold=False):
    """Reuse the existing bounded stdout/retention runner, not raw debug logs."""
    base = base.resolve()
    directory = Path(tempfile.mkdtemp(prefix="qualification-", dir=base))
    receipt = dict(status="STARTED", attempts=[], obligations=[], query=QUERY,
                   constant_fold=constant_fold)

    def save():
        (directory / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")

    resources = dict(timeout_seconds=110, output_bytes=4*1024**2,
                     min_free_bytes=25*1024**3, max_retained_bytes=500*1024**2)
    runner = Capture(directory, resources, os.environ.copy(), receipt, save,
                     watch_interval_seconds=1)
    prefix = [shutil.which("container"), "exec", container, "env",
              "PATH=/tools/bin:/usr/bin:/bin", "timeout"]
    cases = [("model_add_64", {"prop_model_add_64": "UNSAT", "domain_model_add_64": "SAT"})]
    cases += [(name, {"prop_model_add_64": "SAT", "domain_model_add_64": "SAT"})
              for name in ("wrong-instruction", "wrong-state", "unknown-instruction")]
    cases += [("empty-domain", {"prop_model_add_64": "UNSAT", "domain_model_add_64": "UNSAT"}),
              ("translator-controls", {"control_overflow": "SAT", "control_assumption": "SAT",
                                       "control_loop": "SAT"})]
    try:
        for name, expected in cases:
            print("qualify " + name, flush=True)
            stem = directory / name
            guest_stem = "/work/" + stem.relative_to(base).as_posix()
            row = runner.run("generate:" + name, [*prefix, "90", "/tools/bin/sail", "--smt",
                *(["--Oconstant-fold"] if constant_fold else []),
                "--smt-int-size", "256", "--smt-bits-size", "256",
                "--memo-z3-path", "/work/typecheck-cache", "--dprofile",
                "/model/integer.sail", "/work/properties/" + name + ".sail", "-o", guest_stem])
            for prop, verdict in expected.items():
                query = Path(str(stem) + "_" + prop + ".smt2")
                if not query.is_file():
                    raise ValueError("missing query: " + query.name)
                try:
                    row = runner.run("solve:" + name + ":" + prop,
                        [*prefix, "30", "/tools/bin/z3", "/work/" + query.relative_to(base).as_posix()])
                except CommandExitError:
                    row = receipt["attempts"][-1]
                actual = solver_verdict(row["exit_code"], (directory / row["stdout"]).read_bytes())
                receipt["obligations"].append(dict(case=name, property=prop, expected=verdict, actual=actual))
                save()
                if actual != verdict:
                    raise ValueError(f"{name}/{prop}: {actual}, expected {verdict}")
        receipt["status"] = "QUALIFIED"
    except (ValueError, OSError) as error:
        receipt.update(status="QUALIFICATION_FAILED", error=str(error))
    save()
    print(f"{receipt['status']}: {directory}", flush=True)
    return receipt["status"] == "QUALIFIED"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--generate", type=Path)
    mode.add_argument("--qualify", type=Path)
    parser.add_argument("--container", default="evocc-n07-e3-smt")
    parser.add_argument("--constant-fold", action="store_true")
    args = parser.parse_args()
    if args.generate:
        generate(args.generate)
    else:
        raise SystemExit(0 if run_qualification(args.qualify, args.container,
                                              constant_fold=args.constant_fold) else 1)
