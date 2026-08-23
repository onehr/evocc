#!/usr/bin/python3

"""Deterministic authority for the EvoCC Day-7 memory differential corpus."""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import json
import pathlib
import struct
import sys


DEFINED_COUNT = 128
INVALID_COUNT = 64
SCHEMA_VERSION = 1


@dataclasses.dataclass(frozen=True)
class DefinedCase:
    case_id: str
    category: str
    source: str
    value: int


@dataclasses.dataclass(frozen=True)
class InvalidCase:
    case_id: str
    category: str
    source: str
    diagnostic_code: str


def _defined_case(ordinal: int) -> DefinedCase:
    lane = ordinal // 16
    variant = ordinal % 16
    a = variant + 2
    b = variant * 2 + 3
    delta = variant % 5 + 1

    if lane == 0:
        values = (a, b, a + 3, b + 4)
        raw = sum(value + delta for value in values)
        source = (
            f"int main(void) {{ int a_{ordinal:03}[4] = "
            f"{{{values[0]}, {values[1]}, {values[2]}, {values[3]}}}; "
            f"int sum = 0; for (int i = 0; i < 4; i++) {{ "
            f"a_{ordinal:03}[i] += {delta}; sum += a_{ordinal:03}[i]; }} "
            "return sum % 251; }\n"
        )
        category = "array-mutation"
    elif lane == 1:
        raw = (a + delta) * 3 + b
        source = (
            f"int main(void) {{ int x_{ordinal:03} = {a}; int y = {b}; "
            f"int *p = &x_{ordinal:03}; int *q = p; *q += {delta}; "
            "return (*p * 3 + y) % 251; }\n"
        )
        category = "alias-read-write"
    elif lane == 2:
        raw = a + 2 * delta
        source = (
            f"int g_{ordinal:03} = {a}; "
            f"int bump_{ordinal:03}(void) {{ g_{ordinal:03} += {delta}; "
            f"return g_{ordinal:03}; }} "
            f"int main(void) {{ bump_{ordinal:03}(); bump_{ordinal:03}(); "
            f"return g_{ordinal:03} % 251; }}\n"
        )
        category = "global-across-calls"
    elif lane == 3:
        raw = (a + delta) * 7 + (b - 1)
        source = (
            f"struct Pair_{ordinal:03} {{ int left; int right; }}; "
            f"int main(void) {{ struct Pair_{ordinal:03} value; "
            f"struct Pair_{ordinal:03} *p = &value; p->left = {a}; "
            f"p->right = {b}; p->left += {delta}; p->right--; "
            "return (p->left * 7 + p->right) % 251; }\n"
        )
        category = "record-pointer"
    elif lane == 4:
        c0 = 65 + variant
        c1 = 90 - variant
        raw = c0 + c1 + 33
        source = (
            f"int main(void) {{ char src_{ordinal:03}[4] = "
            f"{{{c0}, {c1}, 33, 0}}; char dst[4] = {{0}}; "
            f"for (int i = 0; i < 4; i++) dst[i] = src_{ordinal:03}[i]; "
            "return (dst[0] + dst[1] + dst[2] + dst[3]) % 251; }\n"
        )
        category = "byte-copy"
    elif lane == 5:
        raw = a + b + delta
        source = (
            f"int main(void) {{ volatile int port_{ordinal:03}; "
            f"port_{ordinal:03} = {a}; int first = port_{ordinal:03}; "
            f"port_{ordinal:03} = {b + delta}; int second = port_{ordinal:03}; "
            "return (first + second) % 251; }\n"
        )
        category = "volatile-order"
    elif lane == 6:
        raw = a + b + delta
        source = (
            f"struct Node_{ordinal:03} {{ int value; struct Node_{ordinal:03} *next; }}; "
            f"int main(void) {{ struct Node_{ordinal:03} first; "
            f"struct Node_{ordinal:03} second; first.value = {a}; "
            f"second.value = {b + delta}; first.next = &second; second.next = 0; "
            f"struct Node_{ordinal:03} *p = &first; int sum = 0; "
            "while (p) { sum += p->value; p = p->next; } return sum % 251; }\n"
        )
        category = "linked-list"
    else:
        raw = b + delta
        source = (
            f"int main(void) {{ int a_{ordinal:03}[3] = {{{a}, {b}, {a + b}}}; "
            f"int *p = a_{ordinal:03}; p = p + 1; *p += {delta}; "
            "return *p % 251; }\n"
        )
        category = "pointer-arithmetic"

    return DefinedCase(
        case_id=f"day7.defined.{category}.{ordinal:03}",
        category=category,
        source=source,
        value=raw % 251,
    )


def build_defined_cases() -> list[DefinedCase]:
    return [_defined_case(ordinal) for ordinal in range(DEFINED_COUNT)]


def _invalid_case(ordinal: int) -> InvalidCase:
    lane = ordinal // 16
    salt = ordinal + 1
    if lane == 0:
        category = "uninitialized-read"
        code = "E3001"
        source = (
            f"int main(void) {{ int value_{ordinal:03}; int salt = {salt}; "
            f"int *p = &value_{ordinal:03}; return *p + salt; }}\n"
        )
    elif lane == 1:
        category = "null-dereference"
        code = "E3009"
        source = (
            f"int main(void) {{ int *p_{ordinal:03} = 0; int salt = {salt}; "
            f"return *p_{ordinal:03} + salt; }}\n"
        )
    elif lane == 2:
        category = "out-of-bounds"
        code = "E3011"
        source = (
            f"int main(void) {{ int a_{ordinal:03}[1] = {{{salt}}}; "
            f"return a_{ordinal:03}[1]; }}\n"
        )
    else:
        category = "dangling-pointer"
        code = "E3010"
        source = (
            f"int main(void) {{ int *p_{ordinal:03}; {{ int value = {salt}; "
            f"p_{ordinal:03} = &value; }} return *p_{ordinal:03}; }}\n"
        )
    return InvalidCase(
        case_id=f"day7.invalid.{category}.{ordinal:03}",
        category=category,
        source=source,
        diagnostic_code=code,
    )


def build_invalid_cases() -> list[InvalidCase]:
    return [_invalid_case(ordinal) for ordinal in range(INVALID_COUNT)]


def _sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def audit_corpus() -> dict[str, object]:
    defined = build_defined_cases()
    invalid = build_invalid_cases()
    all_cases = [*defined, *invalid]
    ids = [case.case_id for case in all_cases]
    sources = [case.source.encode("utf-8") for case in all_cases]
    if len(defined) != DEFINED_COUNT or len(invalid) != INVALID_COUNT:
        raise AssertionError("corpus denominator changed")
    if len(set(ids)) != len(ids):
        raise AssertionError("corpus IDs are not unique")
    if len(set(sources)) != len(sources):
        raise AssertionError("corpus source bytes are not unique")
    categories: dict[str, int] = {}
    for case in all_cases:
        categories[case.category] = categories.get(case.category, 0) + 1
    manifest = "".join(
        f"{case.case_id}\t{_sha256(case.source.encode('utf-8'))}\n"
        for case in all_cases
    ).encode("utf-8")
    return {
        "schemaVersion": SCHEMA_VERSION,
        "status": "PASS",
        "defined": len(defined),
        "invalid": len(invalid),
        "total": len(all_cases),
        "uniqueIds": len(set(ids)),
        "uniqueSourceBytes": len(set(sources)),
        "categoryCounts": dict(sorted(categories.items())),
        "manifestSha256": _sha256(manifest),
    }


def materialize_defined(output: pathlib.Path) -> dict[str, object]:
    audit = audit_corpus()
    output.mkdir(parents=False, exist_ok=False)
    rows = []
    for ordinal, case in enumerate(build_defined_cases()):
        filename = f"{ordinal:03}-{case.category}.c"
        path = output / filename
        path.write_text(case.source, encoding="utf-8")
        rows.append(
            f"{ordinal}\t{filename}\t{case.case_id}\t"
            f"{_sha256(case.source.encode('utf-8'))}\n"
        )
    manifest = "".join(rows)
    (output / "manifest.tsv").write_text(manifest, encoding="utf-8")
    return {
        **audit,
        "materializedDefined": DEFINED_COUNT,
        "definedManifestSha256": _sha256(manifest.encode("utf-8")),
    }


def _frame(payload: bytes) -> bytes:
    return struct.pack(">Q", len(payload)) + payload


def export_framed() -> None:
    audit = audit_corpus()
    cases: list[tuple[str, str, str, str]] = []
    for case in build_defined_cases():
        cases.append(("defined", case.case_id, case.source, str(case.value)))
    for case in build_invalid_cases():
        cases.append(
            ("invalid", case.case_id, case.source, case.diagnostic_code)
        )
    output = sys.stdout.buffer
    output.write(_frame(b"evocc-day7-memory-corpus-v1"))
    output.write(_frame(str(audit["manifestSha256"]).encode("ascii")))
    output.write(_frame(str(len(cases)).encode("ascii")))
    for record in cases:
        payload = b"".join(_frame(field.encode("utf-8")) for field in record)
        output.write(_frame(payload))


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("audit")
    materialize = subparsers.add_parser("materialize-defined")
    materialize.add_argument("output", type=pathlib.Path)
    subparsers.add_parser("export-framed")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    if args.command == "audit":
        result = audit_corpus()
    elif args.command == "materialize-defined":
        result = materialize_defined(args.output)
    elif args.command == "export-framed":
        export_framed()
        return 0
    else:
        raise AssertionError(args.command)
    print(json.dumps(result, indent=2, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
