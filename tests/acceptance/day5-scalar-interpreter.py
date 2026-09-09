#!/usr/bin/env python3
"""Frozen Day-5 scalar C-HIR interpreter corpus and comparator.

The 256 defined programs are checked against this independent, bounded model.
The historical 64 EXPECTED_REJECT programs were runtime-error witnesses in
the original profile. Revision 1.9.88 defines four signed shifts; their source
bytes and frozen manifest remain unchanged, but current execution checks their
measured return values. This file never accepts observations from a TSV or similar external
subject substitute; ``behavior`` executes the binary path supplied by the
authoritative shell join directly.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import pathlib
import subprocess
import sys
from typing import Any, Callable, Sequence


SCHEMA = "c30.day5.scalar-interpreter.v1"
TARGET = "aarch64-unknown-linux-gnu"
DEFINED_ROWS = 16
DEFINED_COLUMNS = 16
DEFINED_COUNT = DEFINED_ROWS * DEFINED_COLUMNS
REJECT_FAMILIES = 8
REJECT_VARIANTS = 8
REJECT_COUNT = REJECT_FAMILIES * REJECT_VARIANTS
WATCHDOG_SECONDS = 2.0
CONTRACT = pathlib.Path(__file__).resolve().parents[1] / "contracts/day5-scalar-interpreter.json"
# Current GNU-profile results, not a mutation of the historical corpus.
SIGNED_SHIFT_RESULTS = {
    "reject.signed-edge.v04": -2,
    "reject.signed-edge.v05": -2147483648,
    "reject.signed-edge.v06": -2147483648,
    "reject.signed-edge.v07": -1073741824,
}


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("ascii")).hexdigest()


@dataclasses.dataclass(frozen=True)
class ExpressionAxis:
    key: str
    expression: str
    value: int


@dataclasses.dataclass(frozen=True)
class ControlAxis:
    key: str
    delta: int
    render: Callable[[str, int, str], str]


@dataclasses.dataclass(frozen=True)
class DefinedCase:
    ordinal: int
    case_id: str
    expression_axis: str
    control_axis: str
    source: str
    value: int

    def manifest(self) -> dict[str, Any]:
        return {
            "ordinal": self.ordinal,
            "id": self.case_id,
            "expressionAxis": self.expression_axis,
            "controlAxis": self.control_axis,
            "sourceSha256": sha256_text(self.source),
            "expected": {
                "kind": "RETURN",
                "value": self.value,
                "nativeExitCode": self.value,
            },
        }


@dataclasses.dataclass(frozen=True)
class Diagnostic:
    code: str
    message: str
    byte_start: int
    byte_end: int
    source_slice: str

    @property
    def line(self) -> int:
        return 1

    @property
    def column(self) -> int:
        return self.byte_start + 1

    def manifest(self) -> dict[str, Any]:
        return {
            "file": "/dev/stdin",
            "fileId": 1,
            "line": self.line,
            "column": self.column,
            "byteStart": self.byte_start,
            "byteEnd": self.byte_end,
            "sourceSlice": self.source_slice,
            "code": self.code,
            "message": self.message,
        }


@dataclasses.dataclass(frozen=True)
class RejectCase:
    ordinal: int
    case_id: str
    family: str
    variant: int
    source: str
    diagnostic: Diagnostic

    def manifest(self) -> dict[str, Any]:
        return {
            "ordinal": self.ordinal,
            "id": self.case_id,
            "classification": "EXPECTED_REJECT",
            "family": self.family,
            "variant": self.variant,
            "sourceSha256": sha256_text(self.source),
            "diagnostic": self.diagnostic.manifest(),
        }


def expression_axes() -> tuple[ExpressionAxis, ...]:
    return (
        ExpressionAxis("literal", "7", 7),
        ExpressionAxis("signed-negative", "(-9 + 20)", 11),
        ExpressionAxis("unary-plus-minus", "+(-(-8))", 8),
        ExpressionAxis("logical-not", "(!0 + !!9)", 2),
        ExpressionAxis("bitwise-not", "~(-10)", 9),
        ExpressionAxis("add-subtract", "(20 + 5 - 13)", 12),
        ExpressionAxis("multiply-divide", "(6 * 7 / 2)", 21),
        ExpressionAxis("remainder", "(53 % 10)", 3),
        ExpressionAxis("shifts", "((3 << 3) + (32 >> 3))", 28),
        ExpressionAxis("bitwise", "((29 & 15) | (8 ^ 3))", 15),
        ExpressionAxis("comparisons", "((4 < 9) + (9 >= 9) + (3 != 4))", 3),
        ExpressionAxis("short-circuit", "((0 && (1 / 0)) + (1 || (1 / 0)))", 1),
        ExpressionAxis("conditional-dead-arm", "(1 ? 31 : (1 / 0))", 31),
        ExpressionAxis("comma", "(2, 17)", 17),
        ExpressionAxis(
            "integer-casts",
            "((int)(unsigned char)260 + (int)(signed char)3)",
            7,
        ),
        ExpressionAxis(
            "sizeof-alignof",
            "((int)sizeof(int) + (int)_Alignof(long))",
            12,
        ),
    )


def control_axes() -> tuple[ControlAxis, ...]:
    return (
        ControlAxis("return", 0, lambda v, _x, _s: f"return {v};"),
        ControlAxis(
            "nested-block",
            1,
            lambda v, _x, s: f"{{ int w_{s} = {v}; {v} = w_{s} + 1; }} return {v};",
        ),
        ControlAxis(
            "if-true",
            2,
            lambda v, x, _s: f"if ({v} == {x}) return {v} + 2; return 99;",
        ),
        ControlAxis(
            "if-else",
            3,
            lambda v, x, _s: f"if ({v} != {x}) return 99; else return {v} + 3;",
        ),
        ControlAxis(
            "while",
            3,
            lambda v, _x, s: (
                f"int n_{s} = 3; while (n_{s}) {{ {v} += 1; n_{s} -= 1; }} return {v};"
            ),
        ),
        ControlAxis(
            "while-continue",
            4,
            lambda v, _x, s: (
                f"int i_{s} = 0; while (i_{s} < 5) {{ i_{s}++; "
                f"if (i_{s} == 3) continue; {v}++; }} return {v};"
            ),
        ),
        ControlAxis(
            "do-while",
            4,
            lambda v, _x, s: (
                f"int n_{s} = 2; do {{ {v} += 2; n_{s}--; }} while (n_{s}); return {v};"
            ),
        ),
        ControlAxis(
            "for-expression-init",
            6,
            lambda v, _x, s: (
                f"int i_{s}; for (i_{s} = 0; i_{s} < 3; i_{s}++) "
                f"{{ {v} = {v} + 2; }} return {v};"
            ),
        ),
        ControlAxis(
            "for-declaration-init",
            8,
            lambda v, _x, s: (
                f"for (int i_{s} = 0; i_{s} < 4; i_{s} = i_{s} + 1) "
                f"{{ {v} += 2; }} return {v};"
            ),
        ),
        ControlAxis(
            "break",
            3,
            lambda v, _x, s: (
                f"int i_{s} = 0; while (1) {{ if (i_{s} == 3) break; "
                f"i_{s}++; {v}++; }} return {v};"
            ),
        ),
        ControlAxis(
            "switch-match",
            10,
            lambda v, x, _s: (
                f"switch ({v}) {{ case {x}: return {v} + 10; default: return 99; }}"
            ),
        ),
        ControlAxis(
            "switch-fallthrough",
            5,
            lambda v, _x, _s: (
                f"switch (2) {{ case 1: {v} += 1; case 2: {v} += 2; "
                f"case 3: {v} += 3; break; default: {v} = 99; }} return {v};"
            ),
        ),
        ControlAxis(
            "switch-default",
            12,
            lambda v, x, _s: (
                f"switch ({v}) {{ case {x + 100}: return 99; default: {v} += 12; }} "
                f"return {v};"
            ),
        ),
        ControlAxis(
            "goto-forward",
            13,
            lambda v, _x, s: (
                f"{v} += 1; goto done_{s}; {v} = 99; done_{s}: return {v} + 12;"
            ),
        ),
        ControlAxis(
            "prefix-postfix-update",
            14,
            lambda v, _x, _s: f"{v}++; ++{v}; {v}--; --{v}; return {v} + 14;",
        ),
        ControlAxis(
            "block-enum",
            15,
            lambda v, _x, s: f"enum E_{s} {{ K_{s} = 15 }}; {v} += K_{s}; return {v};",
        ),
    )


def build_defined_cases() -> tuple[DefinedCase, ...]:
    cases: list[DefinedCase] = []
    for row, expression in enumerate(expression_axes()):
        for column, control in enumerate(control_axes()):
            suffix = f"e{row:02d}_c{column:02d}"
            variable = f"v_{suffix}"
            body = control.render(variable, expression.value, suffix)
            source = f"int main(void) {{ int {variable} = {expression.expression}; {body} }}\n"
            cases.append(
                DefinedCase(
                    ordinal=len(cases) + 1,
                    case_id=f"defined.{suffix}.{expression.key}.{control.key}",
                    expression_axis=expression.key,
                    control_axis=control.key,
                    source=source,
                    value=expression.value + control.delta,
                )
            )
    return tuple(cases)


def anchored_diagnostic(source: str, anchor: str, code: str, message: str) -> Diagnostic:
    start = source.rindex(anchor)
    return Diagnostic(code, message, start, start + len(anchor), anchor)


def reject_case(
    cases: list[RejectCase],
    family: str,
    variant: int,
    source: str,
    anchor: str,
    code: str,
    message: str,
) -> None:
    cases.append(
        RejectCase(
            ordinal=len(cases) + 1,
            case_id=f"reject.{family}.v{variant:02d}",
            family=family,
            variant=variant,
            source=source,
            diagnostic=anchored_diagnostic(source, anchor, code, message),
        )
    )


def binary_source(family: str, variant: int, left: int, op: str, right: int) -> tuple[str, str]:
    suffix = f"{family.replace('-', '_')}_{variant:02d}"
    lhs = f"a_{suffix}"
    rhs = f"b_{suffix}"
    expression = f"{lhs} {op} {rhs}"
    source = (
        f"int main(void) {{ int {lhs} = {left}; int {rhs} = {right}; "
        f"return {expression}; }}\n"
    )
    return source, expression


def build_reject_cases() -> tuple[RejectCase, ...]:
    cases: list[RejectCase] = []

    for variant, numerator in enumerate((1, -1, 7, -7, 99, -99, 2147483647, -2147483647)):
        source, anchor = binary_source("division-by-zero", variant, numerator, "/", 0)
        reject_case(cases, "division-by-zero", variant, source, anchor, "E3002", "integer-division-by-zero")

    for variant, numerator in enumerate((1, -1, 8, -8, 101, -101, 2147483647, -2147483647)):
        source, anchor = binary_source("remainder-by-zero", variant, numerator, "%", 0)
        reject_case(cases, "remainder-by-zero", variant, source, anchor, "E3003", "integer-remainder-by-zero")

    add_pairs = (
        (2147483647, 1),
        (2147483647, 2),
        (2147483646, 2),
        (2000000000, 2000000000),
        (-2147483647 - 1, -1),
        (-2147483647 - 1, -2),
        (-2147483647, -2),
        (-2000000000, -2000000000),
    )
    for variant, (left, right) in enumerate(add_pairs):
        source, anchor = binary_source("signed-add-overflow", variant, left, "+", right)
        reject_case(cases, "signed-add-overflow", variant, source, anchor, "E3004", "signed-integer-overflow")

    subtract_pairs = (
        (-2147483647 - 1, 1),
        (-2147483647 - 1, 2),
        (-2147483647, 2),
        (-2000000000, 2000000000),
        (2147483647, -1),
        (2147483647, -2),
        (2147483646, -2),
        (2000000000, -2000000000),
    )
    for variant, (left, right) in enumerate(subtract_pairs):
        source, anchor = binary_source("signed-subtract-overflow", variant, left, "-", right)
        reject_case(cases, "signed-subtract-overflow", variant, source, anchor, "E3004", "signed-integer-overflow")

    multiply_pairs = (
        (1073741824, 2),
        (715827883, 3),
        (536870912, 4),
        (100000, 100000),
        (-1073741824, 3),
        (-715827883, 4),
        (-100000, 100000),
        (-2147483647 - 1, -1),
    )
    for variant, (left, right) in enumerate(multiply_pairs):
        source, anchor = binary_source("signed-multiply-overflow", variant, left, "*", right)
        reject_case(cases, "signed-multiply-overflow", variant, source, anchor, "E3004", "signed-integer-overflow")

    shift_pairs = ((1, -1), (1, 32), (1, 33), (7, 63), (-1, -2), (-1, 64), (99, 100), (0, 2147483647))
    for variant, (left, right) in enumerate(shift_pairs):
        source, anchor = binary_source("invalid-shift-count", variant, left, "<<" if variant % 2 == 0 else ">>", right)
        reject_case(cases, "invalid-shift-count", variant, source, anchor, "E3005", "invalid-shift-count")

    edge_specs = (
        ("int a_signed_edge_00 = -2147483647 - 1; return -a_signed_edge_00;", "-a_signed_edge_00", "E3004", "signed-integer-overflow"),
        ("int a_signed_edge_01 = -2147483647 - 1; return +(-a_signed_edge_01);", "-a_signed_edge_01", "E3004", "signed-integer-overflow"),
        ("int a_signed_edge_02 = -2147483647 - 1; int b_signed_edge_02 = -1; return a_signed_edge_02 / b_signed_edge_02;", "a_signed_edge_02 / b_signed_edge_02", "E3004", "signed-integer-overflow"),
        ("int a_signed_edge_03 = -2147483647 - 1; int b_signed_edge_03 = -1; return a_signed_edge_03 % b_signed_edge_03;", "a_signed_edge_03 % b_signed_edge_03", "E3004", "signed-integer-overflow"),
        ("int a_signed_edge_04 = -1; int b_signed_edge_04 = 1; return a_signed_edge_04 << b_signed_edge_04;", "a_signed_edge_04 << b_signed_edge_04", "E3006", "invalid-signed-left-shift"),
        ("int a_signed_edge_05 = 1; int b_signed_edge_05 = 31; return a_signed_edge_05 << b_signed_edge_05;", "a_signed_edge_05 << b_signed_edge_05", "E3006", "invalid-signed-left-shift"),
        ("int a_signed_edge_06 = 1073741824; int b_signed_edge_06 = 1; return a_signed_edge_06 << b_signed_edge_06;", "a_signed_edge_06 << b_signed_edge_06", "E3006", "invalid-signed-left-shift"),
        ("int a_signed_edge_07 = 3; int b_signed_edge_07 = 30; return a_signed_edge_07 << b_signed_edge_07;", "a_signed_edge_07 << b_signed_edge_07", "E3006", "invalid-signed-left-shift"),
    )
    for variant, (body, anchor, code, message) in enumerate(edge_specs):
        source = f"int main(void) {{ {body} }}\n"
        reject_case(cases, "signed-edge", variant, source, anchor, code, message)

    for variant in range(REJECT_VARIANTS):
        name = f"uninit_{variant:02d}"
        source = (
            f"int main(void) {{ int decoy_{variant:02d} = {variant + 1}; int {name}; "
            f"return {name}; }}\n"
        )
        reject_case(
            cases,
            "uninitialized-read",
            variant,
            source,
            name,
            "E3001",
            f"read-of-uninitialized-object: {name}",
        )

    return tuple(cases)


def fuel_control() -> tuple[str, Diagnostic]:
    source = "int main(void) { for (;;) { } }\n"
    return source, anchored_diagnostic(
        source,
        "for (;;) { }",
        "E3008",
        "interpreter-fuel-exhausted",
    )


def build_manifest() -> dict[str, Any]:
    defined = build_defined_cases()
    rejected = build_reject_cases()
    fuel_source, fuel_diagnostic = fuel_control()
    return {
        "schema": SCHEMA,
        "target": TARGET,
        "cli": {
            "argv": [f"--target={TARGET}", "--interpret-c-hir", "/dev/stdin"],
            "success": {"exitCode": 0, "stdout": "<signed-decimal>\\n", "stderr": ""},
            "error": {
                "exitCode": 1,
                "stdout": "",
                "stderr": "/dev/stdin:<line>:<column>: error[<code>]: <message>\\n",
            },
            "returnValueIsNotProcessExitCode": True,
        },
        "denominators": {
            "definedRows": DEFINED_ROWS,
            "definedColumns": DEFINED_COLUMNS,
            "defined": DEFINED_COUNT,
            "expectedRejectFamilies": REJECT_FAMILIES,
            "expectedRejectVariants": REJECT_VARIANTS,
            "expectedReject": REJECT_COUNT,
        },
        "modelPolicy": {
            "defined": "independent exact integer/control-flow model",
            "expectedReject": "hand-audited runtime-error witnesses",
            "undefinedBehaviorOracleClaim": False,
            "nestedMutation": "excluded",
            "memoryAndPointerOperations": "excluded",
            "entry": "zero-parameter int main(void)",
        },
        "definedOracleAuthority": {
            "scope": "definedCasesOnly",
            "requiredConsensus": "GCC_AND_CLANG_NATIVE_EXIT_CODE",
            "evidenceStatus": "NOT_MEASURED_IN_THIS_FREEZE",
            "imageId": "sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2",
            "compilerFlags": ["-std=gnu11", "-O0", "-g0", "-pipe"],
            "gcc": {
                "path": "/usr/bin/gcc-14",
                "version": "gcc-14 (Debian 14.2.0-19) 14.2.0",
                "sha256": "fb051a6c58961fb6aec758acf50d7d0375eecff740d0706388092829e3fb7a41",
            },
            "clang": {
                "path": "/usr/bin/clang-19",
                "version": "Debian clang version 19.1.7 (3+b1)",
                "sha256": "8bba5d733f54ada0cc50ba7646a87ddc798d5de73f67e397cdefada07297bb65",
            },
            "expectedRejectNativeExecution": "PROHIBITED",
        },
        "expressionAxes": [
            {"index": index, "id": axis.key, "expression": axis.expression, "value": axis.value}
            for index, axis in enumerate(expression_axes())
        ],
        "controlAxes": [
            {"index": index, "id": axis.key, "resultDelta": axis.delta}
            for index, axis in enumerate(control_axes())
        ],
        "definedCases": [case.manifest() for case in defined],
        "expectedRejectCases": [case.manifest() for case in rejected],
        "runtimeControls": [
            {
                "id": "control.fuel-exhaustion",
                "sourceSha256": sha256_text(fuel_source),
                "diagnostic": fuel_diagnostic.manifest(),
            }
        ],
    }


def validate_model() -> None:
    expressions = expression_axes()
    controls = control_axes()
    defined = build_defined_cases()
    rejected = build_reject_cases()
    assert len(expressions) == DEFINED_ROWS
    assert len(controls) == DEFINED_COLUMNS
    assert len(defined) == DEFINED_COUNT
    assert len(rejected) == REJECT_COUNT
    assert {case.family for case in rejected} == {
        "division-by-zero",
        "remainder-by-zero",
        "signed-add-overflow",
        "signed-subtract-overflow",
        "signed-multiply-overflow",
        "invalid-shift-count",
        "signed-edge",
        "uninitialized-read",
    }
    for family in {case.family for case in rejected}:
        variants = [case.variant for case in rejected if case.family == family]
        assert variants == list(range(REJECT_VARIANTS)), (family, variants)

    all_cases: Sequence[DefinedCase | RejectCase] = (*defined, *rejected)
    ids = [case.case_id for case in all_cases]
    sources = [case.source for case in all_cases]
    hashes = [sha256_text(source) for source in sources]
    assert len(ids) == len(set(ids)), "case IDs are not unique"
    assert len(sources) == len(set(sources)), "source bytes are not unique"
    assert len(hashes) == len(set(hashes)), "source hashes are not unique"
    assert all(source.startswith("int main(void) {") for source in sources)
    assert all(source.endswith("}\n") for source in sources)
    assert all("main(" not in source.removeprefix("int main(void)") for source in sources)
    assert all(0 <= case.value <= 255 for case in defined), "native exit status is not exact"

    for case in rejected:
        diagnostic = case.diagnostic
        encoded = case.source.encode("ascii")
        assert 0 <= diagnostic.byte_start < diagnostic.byte_end <= len(encoded)
        assert diagnostic.line == 1
        assert diagnostic.column == diagnostic.byte_start + 1
        assert (
            case.source[diagnostic.byte_start : diagnostic.byte_end]
            == diagnostic.source_slice
        )


def audit_contract() -> dict[str, Any]:
    validate_model()
    expected = build_manifest()
    observed = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if observed != expected:
        raise AssertionError("checked-in Day-5 scalar-interpreter contract differs from model")
    contract_bytes = CONTRACT.read_bytes()
    if contract_bytes != (json.dumps(expected, indent=2, sort_keys=True) + "\n").encode("utf-8"):
        raise AssertionError("contract JSON is not in canonical formatting")
    return {
        "status": "PASS",
        "schema": SCHEMA,
        "defined": DEFINED_COUNT,
        "expectedReject": REJECT_COUNT,
        "contractSha256": hashlib.sha256(contract_bytes).hexdigest(),
    }


def expected_observations() -> list[dict[str, Any]]:
    observations: list[dict[str, Any]] = []
    for case in build_defined_cases():
        observations.append(
            {
                "id": case.case_id,
                "sourceSha256": sha256_text(case.source),
                "rc": 0,
                "stdout": f"{case.value}\n",
                "stderr": "",
                "timedOut": False,
            }
        )
    for case in build_reject_cases():
        diagnostic = case.diagnostic
        if case.case_id in SIGNED_SHIFT_RESULTS:
            observations.append(
                {
                    "id": case.case_id,
                    "sourceSha256": sha256_text(case.source),
                    "rc": 0,
                    "stdout": f"{SIGNED_SHIFT_RESULTS[case.case_id]}\n",
                    "stderr": "",
                    "timedOut": False,
                }
            )
            continue
        observations.append(
            {
                "id": case.case_id,
                "sourceSha256": sha256_text(case.source),
                "rc": 1,
                "stdout": "",
                "stderr": (
                    f"/dev/stdin:{diagnostic.line}:{diagnostic.column}: "
                    f"error[{diagnostic.code}]: {diagnostic.message}\n"
                ),
                "timedOut": False,
            }
        )
    return observations


def compare_observations(observed: Sequence[dict[str, Any]]) -> list[str]:
    expected = expected_observations()
    mismatches: list[str] = []
    if len(observed) != len(expected):
        mismatches.append(f"denominator: expected {len(expected)}, observed {len(observed)}")
    for index, expected_case in enumerate(expected):
        if index >= len(observed):
            mismatches.append(f"{expected_case['id']}: missing observation")
            continue
        actual_case = observed[index]
        if actual_case != expected_case:
            differing = sorted(
                key
                for key in set(expected_case) | set(actual_case)
                if expected_case.get(key) != actual_case.get(key)
            )
            mismatches.append(f"{expected_case['id']}: differing fields {','.join(differing)}")
    for actual_case in observed[len(expected) :]:
        mismatches.append(f"unexpected observation: {actual_case.get('id', '<missing-id>')}")
    return mismatches


def comparator_self_test() -> dict[str, Any]:
    baseline = expected_observations()
    assert not compare_observations(baseline)
    mutants: list[tuple[str, list[dict[str, Any]]]] = []

    wrong_value = [dict(value) for value in baseline]
    wrong_value[0]["stdout"] = "8\n"
    mutants.append(("defined-result", wrong_value))

    wrong_hash = [dict(value) for value in baseline]
    wrong_hash[DEFINED_COUNT // 2]["sourceSha256"] = "0" * 64
    mutants.append(("source-hash", wrong_hash))

    wrong_diagnostic = [dict(value) for value in baseline]
    wrong_diagnostic[DEFINED_COUNT]["stderr"] = wrong_diagnostic[DEFINED_COUNT]["stderr"].replace(
        "E3002", "E3003"
    )
    mutants.append(("runtime-diagnostic", wrong_diagnostic))

    missing = [dict(value) for value in baseline[:-1]]
    mutants.append(("denominator", missing))

    for name, mutant in mutants:
        if not compare_observations(mutant):
            raise AssertionError(f"comparator accepted {name} mutant")
    return {"status": "PASS", "mutantsRejected": [name for name, _ in mutants]}


def execute_case(binary: pathlib.Path, case_id: str, source: str) -> dict[str, Any]:
    try:
        completed = subprocess.run(
            [str(binary), f"--target={TARGET}", "--interpret-c-hir", "/dev/stdin"],
            input=source,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=WATCHDOG_SECONDS,
            check=False,
        )
        rc: int | None = completed.returncode
        stdout = completed.stdout
        stderr = completed.stderr
        timed_out = False
    except subprocess.TimeoutExpired as error:
        rc = None
        stdout = error.stdout.decode("utf-8", "replace") if isinstance(error.stdout, bytes) else (error.stdout or "")
        stderr = error.stderr.decode("utf-8", "replace") if isinstance(error.stderr, bytes) else (error.stderr or "")
        timed_out = True
    if "panicked at" in stderr or "stack backtrace" in stderr:
        stderr += "<HARNESS:PANIC-OR-BACKTRACE>\n"
    return {
        "id": case_id,
        "sourceSha256": sha256_text(source),
        "rc": rc,
        "stdout": stdout,
        "stderr": stderr,
        "timedOut": timed_out,
    }


def verify_behavior(binary_name: str) -> int:
    audit_contract()
    binary = pathlib.Path(binary_name)
    if not binary.is_file() or binary.is_symlink():
        raise AssertionError(f"production binary is missing, non-regular, or symbolic: {binary}")

    observations: list[dict[str, Any]] = []
    for case in build_defined_cases():
        observations.append(execute_case(binary, case.case_id, case.source))
    for case in build_reject_cases():
        observations.append(execute_case(binary, case.case_id, case.source))
    mismatches = compare_observations(observations)

    fuel_source, fuel_diagnostic = fuel_control()
    fuel_observation = execute_case(binary, "control.fuel-exhaustion", fuel_source)
    fuel_expected = {
        "id": "control.fuel-exhaustion",
        "sourceSha256": sha256_text(fuel_source),
        "rc": 1,
        "stdout": "",
        "stderr": (
            f"/dev/stdin:{fuel_diagnostic.line}:{fuel_diagnostic.column}: "
            f"error[{fuel_diagnostic.code}]: {fuel_diagnostic.message}\n"
        ),
        "timedOut": False,
    }
    if fuel_observation != fuel_expected:
        differing = sorted(
            key
            for key in set(fuel_expected) | set(fuel_observation)
            if fuel_expected.get(key) != fuel_observation.get(key)
        )
        mismatches.append(f"control.fuel-exhaustion: differing fields {','.join(differing)}")

    summary = {
        "status": "PASS" if not mismatches else "FAIL",
        "defined": DEFINED_COUNT + len(SIGNED_SHIFT_RESULTS),
        "expectedReject": REJECT_COUNT - len(SIGNED_SHIFT_RESULTS),
        "historicalDefined": DEFINED_COUNT,
        "historicalExpectedReject": REJECT_COUNT,
        "signedShiftProfileRevision": "1.9.88",
        "fuelControls": 1,
        "mismatchCount": len(mismatches),
        "mismatchSample": mismatches[:32],
    }
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if not mismatches else 1


def emit_jsonl() -> None:
    for case in build_defined_cases():
        print(json.dumps({**case.manifest(), "source": case.source}, sort_keys=True))
    for case in build_reject_cases():
        print(json.dumps({**case.manifest(), "source": case.source}, sort_keys=True))


def write_contract() -> None:
    CONTRACT.write_text(json.dumps(build_manifest(), indent=2, sort_keys=True) + "\n", encoding="utf-8")


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("audit")
    subparsers.add_parser("self-test")
    subparsers.add_parser("emit-jsonl")
    subparsers.add_parser("write-contract")
    behavior = subparsers.add_parser("behavior")
    behavior.add_argument("binary")
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    args = parse_args(argv)
    if args.command == "audit":
        print(json.dumps(audit_contract(), indent=2, sort_keys=True))
        return 0
    if args.command == "self-test":
        validate_model()
        print(json.dumps(comparator_self_test(), indent=2, sort_keys=True))
        return 0
    if args.command == "emit-jsonl":
        validate_model()
        emit_jsonl()
        return 0
    if args.command == "write-contract":
        validate_model()
        write_contract()
        return 0
    if args.command == "behavior":
        return verify_behavior(args.binary)
    raise AssertionError(args.command)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
