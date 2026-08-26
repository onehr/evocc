#!/usr/bin/env python3
"""Generate and verify the frozen Day-5 AArch64 C layout corpus."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Any


EXPECTED_COUNTS = {
    "scalar": 16,
    "pointer": 8,
    "array": 16,
    "enum": 8,
    "struct": 28,
    "union": 16,
    "flexible-array": 8,
    "bit-field": 28,
}
EXPECTED_TOTAL = 128

# A second, independent case set for GNU layout attributes. Selected by
# environment so the frozen 128-case Day 5 manifest is never touched: the two
# sets are different contracts with different denominators, and a reader of
# either receipt should be able to tell which one produced a number.
ATTRIBUTED_COUNTS = {"attributed": 22}
ATTRIBUTED_TOTAL = 22
CASE_SET = os.environ.get("C30_LAYOUT_CASE_SET", "day5")
if CASE_SET == "attributed":
    EXPECTED_COUNTS = ATTRIBUTED_COUNTS
    EXPECTED_TOTAL = ATTRIBUTED_TOTAL
elif CASE_SET != "day5":
    raise SystemExit(f"unknown C30_LAYOUT_CASE_SET: {CASE_SET}")
BINDING_POLICY_VERSION = "apple-container-run-unique-alias-v1"

ORACLES = {
    "imageId": "sha256:2c23684c9cc5ef5899073b59844fa47cc3c1d894f2316e21f25d5d85be5a10e2",
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
}

COMPILER_FLAGS = [
    "-std=gnu11",
    "-O0",
    "-g0",
    "-pipe",
    "-fno-short-enums",
    "-fno-common",
    "-Wall",
    "-Wextra",
    "-Werror",
]


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_cases() -> list[dict[str, Any]]:
    if CASE_SET == "attributed":
        return build_attributed_cases()
    return build_day5_cases()


def build_attributed_cases() -> list[dict[str, Any]]:
    """GNU `aligned` and `packed`, one case per rule of specification 10.1.9.

    Every expectation the implementation's tests carry for these constructs is
    a claim until this set has been measured against GCC-14 and Clang-19; the
    measured consensus, not the test, is the authority if they disagree.
    """
    cases: list[dict[str, Any]] = []

    def add(
        case_id: str,
        declaration: str,
        offsets: tuple[tuple[str, str], ...] = (),
        assignments: tuple[str, ...] = (),
    ) -> None:
        ordinal = len(cases) + 1
        number = f"{ordinal:03d}"
        type_name = f"case_{number}_t"
        source = declaration.replace("$T", type_name).strip() + "\n"
        cases.append(
            {
                "ordinal": ordinal,
                "id": case_id,
                "category": "attributed",
                "typeName": type_name,
                "source": source,
                "sourceSha256": sha256_bytes(source.encode("utf-8")),
                "offsets": [
                    {"label": label, "expression": expression.replace("$T", type_name)}
                    for label, expression in offsets
                ],
                "storageAssignments": list(assignments),
            }
        )

    add("attr.aligned-int128-16", "typedef __int128 $T __attribute__((aligned(16)));")
    add("attr.aligned-char-4", "typedef char $T __attribute__((aligned(4)));")
    add("attr.aligned-int-1-no-lower", "typedef int $T __attribute__((aligned(1)));")
    add("attr.aligned-bare", "typedef char $T __attribute__((aligned));")
    add("attr.aligned-typedef-of-struct", "struct attr_s5 { char c; }; typedef struct attr_s5 $T __attribute__((aligned(8)));")
    add("attr.packed-struct", "typedef struct { char c; int i; } __attribute__((packed)) $T;", (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")))
    add("attr.packed-aligned-4", "typedef struct { char c; int i; } __attribute__((packed, aligned(4))) $T;", (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")))
    add("attr.packed-member-alone", "typedef struct { char c; int i __attribute__((packed)); short s; } $T;", (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)"), ("s", "offsetof($T, s)")))
    add("attr.aligned-member", "typedef struct { int a __attribute__((aligned(8))); char b; } $T;", (("a", "offsetof($T, a)"), ("b", "offsetof($T, b)")))
    add(
        "attr.packed-bit-fields",
        "typedef struct { unsigned a : 3; unsigned b : 6; unsigned c : 7; } __attribute__((packed)) $T;",
        assignments=("value.a = 5u;", "value.b = 33u;", "value.c = 100u;"),
    )
    add("attr.packed-union", "typedef union { char c; int i; } __attribute__((packed)) $T;", (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")))
    add("attr.aligned-after-keyword", "typedef struct __attribute__((aligned(16))) { char c; } $T;", (("c", "offsetof($T, c)"),))
    add("attr.packed-after-keyword", "typedef struct __attribute__((packed)) { short s; char c; int i; } $T;", (("s", "offsetof($T, s)"), ("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")))
    add("attr.aligned-struct-tag-rounds-size", "typedef struct { char c; } __attribute__((aligned(4))) $T;", (("c", "offsetof($T, c)"),))
    add(
        "pragma.pack-set-1",
        "#pragma pack(1)\ntypedef struct { char c; int i; } $T;\n#pragma pack()",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    add(
        "pragma.pack-reset-default",
        "#pragma pack(1)\nstruct $T_seed { char c; int i; };\n#pragma pack()\ntypedef struct { char c; int i; } $T;",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    add(
        "pragma.pack-set-2",
        "#pragma pack(2)\ntypedef struct { char c; int i; long l; } $T;\n#pragma pack()",
        (
            ("c", "offsetof($T, c)"),
            ("i", "offsetof($T, i)"),
            ("l", "offsetof($T, l)"),
        ),
    )
    add(
        "pragma.pack-push-1",
        "#pragma pack(push, 1)\ntypedef struct { char c; int i; } $T;\n#pragma pack(pop)",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    add(
        "pragma.pack-nested-push-4",
        "#pragma pack(push, 1)\n#pragma pack(push, 4)\ntypedef struct { char c; long l; } $T;\n#pragma pack(pop)\n#pragma pack(pop)",
        (("c", "offsetof($T, c)"), ("l", "offsetof($T, l)")),
    )
    add(
        "pragma.pack-pop-restores-1",
        "#pragma pack(push, 1)\n#pragma pack(push, 4)\nstruct $T_seed { char c; long l; };\n#pragma pack(pop)\ntypedef struct { char c; int i; } $T;\n#pragma pack(pop)",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    add(
        "pragma.pack-pop-restores-default",
        "#pragma pack(push, 1)\nstruct $T_seed { char c; int i; };\n#pragma pack(pop)\ntypedef struct { char c; int i; } $T;",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    add(
        "pragma.pack-caps-aligned-member",
        "#pragma pack(1)\ntypedef struct { char c; int __attribute__((aligned(4))) i; } $T;\n#pragma pack()",
        (("c", "offsetof($T, c)"), ("i", "offsetof($T, i)")),
    )
    return cases


def build_day5_cases() -> list[dict[str, Any]]:
    cases: list[dict[str, Any]] = []

    def add(
        case_id: str,
        category: str,
        declaration: str,
        offsets: tuple[tuple[str, str], ...] = (),
        assignments: tuple[str, ...] = (),
    ) -> None:
        ordinal = len(cases) + 1
        number = f"{ordinal:03d}"
        type_name = f"case_{number}_t"
        source = declaration.replace("$T", type_name).replace("$N", number).strip() + "\n"
        normalized_offsets = [
            {
                "label": label,
                "expression": expression.replace("$T", type_name).replace("$N", number),
            }
            for label, expression in offsets
        ]
        normalized_assignments = [
            assignment.replace("$T", type_name).replace("$N", number)
            for assignment in assignments
        ]
        cases.append(
            {
                "ordinal": ordinal,
                "id": case_id,
                "category": category,
                "typeName": type_name,
                "source": source,
                "sourceSha256": sha256_bytes(source.encode("utf-8")),
                "offsets": normalized_offsets,
                "storageAssignments": normalized_assignments,
            }
        )

    scalar_types = [
        ("scalar.bool", "_Bool"),
        ("scalar.char", "char"),
        ("scalar.signed-char", "signed char"),
        ("scalar.unsigned-char", "unsigned char"),
        ("scalar.short", "short"),
        ("scalar.unsigned-short", "unsigned short"),
        ("scalar.int", "int"),
        ("scalar.unsigned-int", "unsigned int"),
        ("scalar.long", "long"),
        ("scalar.unsigned-long", "unsigned long"),
        ("scalar.long-long", "long long"),
        ("scalar.unsigned-long-long", "unsigned long long"),
        ("scalar.float", "float"),
        ("scalar.double", "double"),
        ("scalar.long-double", "long double"),
        ("scalar.double-complex", "double _Complex"),
    ]
    for case_id, spelling in scalar_types:
        add(case_id, "scalar", f"typedef {spelling} $T;")

    pointer_declarations = [
        ("pointer.void", "typedef void *$T;"),
        ("pointer.char", "typedef char *$T;"),
        ("pointer.const-int", "typedef const int *$T;"),
        ("pointer.volatile-ulong", "typedef volatile unsigned long *$T;"),
        ("pointer.pointer", "typedef int **$T;"),
        ("pointer.array", "typedef int (*$T)[7];"),
        ("pointer.function", "typedef int (*$T)(long, int);"),
        ("pointer.incomplete-record", "struct opaque_$N; typedef struct opaque_$N *$T;"),
    ]
    for case_id, declaration in pointer_declarations:
        add(case_id, "pointer", declaration)

    array_declarations = [
        ("array.char-1", "typedef char $T[1];"),
        ("array.char-7", "typedef char $T[7];"),
        ("array.short-3", "typedef short $T[3];"),
        ("array.int-3", "typedef int $T[3];"),
        ("array.long-3", "typedef long $T[3];"),
        ("array.double-5", "typedef double $T[5];"),
        ("array.long-double-2", "typedef long double $T[2];"),
        ("array.pointer-4", "typedef void *$T[4];"),
        ("array.int-2x3", "typedef int $T[2][3];"),
        ("array.char-3x5", "typedef char $T[3][5];"),
        ("array.padded-struct-2", "typedef struct { char a; int b; } $T[2];"),
        ("array.union-3", "typedef union { char c; long l; } $T[3];"),
        ("array.enum-6", "typedef enum { E_$N_A = -1, E_$N_B = 4 } $T[6];"),
        ("array.uchar-17", "typedef unsigned char $T[17];"),
        ("array.int-pointer-5", "typedef int *$T[5];"),
        ("array.double-complex-3", "typedef double _Complex $T[3];"),
    ]
    for case_id, declaration in array_declarations:
        add(case_id, "array", declaration)

    enum_declarations = [
        ("enum.zero", "typedef enum { E_$N_ZERO = 0 } $T;"),
        ("enum.negative", "typedef enum { E_$N_NEG = -7, E_$N_POS = 3 } $T;"),
        ("enum.u7-bound", "typedef enum { E_$N_LO = 0, E_$N_HI = 127 } $T;"),
        ("enum.u8-bound", "typedef enum { E_$N_LO = 0, E_$N_HI = 255 } $T;"),
        ("enum.i16-bound", "typedef enum { E_$N_LO = -32768, E_$N_HI = 32767 } $T;"),
        ("enum.u16-bound", "typedef enum { E_$N_LO = 0, E_$N_HI = 65535 } $T;"),
        ("enum.million", "typedef enum { E_$N_LO = -1, E_$N_HI = 1048576 } $T;"),
        ("enum.int-max", "typedef enum { E_$N_LO = -1, E_$N_HI = 2147483647 } $T;"),
    ]
    for case_id, declaration in enum_declarations:
        add(case_id, "enum", declaration)

    struct_specs = [
        ("struct.char-int", "char a; int b;", ("a", "b")),
        ("struct.int-char", "int a; char b;", ("a", "b")),
        ("struct.char-long", "char a; long b;", ("a", "b")),
        ("struct.long-char", "long a; char b;", ("a", "b")),
        ("struct.short-char-int", "short a; char b; int c;", ("a", "b", "c")),
        ("struct.char-short-char", "char a; short b; char c;", ("a", "b", "c")),
        ("struct.char-double-char", "char a; double b; char c;", ("a", "b", "c")),
        ("struct.bool-long-short", "_Bool a; long b; short c;", ("a", "b", "c")),
        ("struct.char3-int", "char a[3]; int b;", ("a", "b")),
        ("struct.char5-long", "char a[5]; long b;", ("a", "b")),
        ("struct.int-char5", "int a; char b[5];", ("a", "b")),
        ("struct.pointer-char", "void *a; char b;", ("a", "b")),
        ("struct.char-pointer", "char a; void *b;", ("a", "b")),
        (
            "struct.nested-then-char",
            "struct { char a; int b; } inner; char tail;",
            ("inner", "inner.a", "inner.b", "tail"),
        ),
        (
            "struct.char-then-nested",
            "char head; struct { char a; int b; } inner;",
            ("head", "inner", "inner.a", "inner.b"),
        ),
        (
            "struct.nested-short-then-int",
            "struct { short a; char b; } inner; int tail;",
            ("inner", "inner.a", "inner.b", "tail"),
        ),
        (
            "struct.nested-union-then-char",
            "union { char c; long l; } inner; char tail;",
            ("inner", "inner.c", "inner.l", "tail"),
        ),
        (
            "struct.char-then-nested-union",
            "char head; union { int i; long l; } inner;",
            ("head", "inner", "inner.i", "inner.l"),
        ),
        (
            "struct.nested-array-then-char",
            "struct { char a; short b; } pair[2]; char tail;",
            ("pair", "pair[0].a", "pair[0].b", "pair[1].a", "tail"),
        ),
        ("struct.char-then-int3", "char head; int values[3];", ("head", "values")),
        (
            "struct.enum-then-char",
            "enum { E_$N_A = -1, E_$N_B = 8 } mode; char tail;",
            ("mode", "tail"),
        ),
        ("struct.ascending-align", "char a; short b; int c; long d;", ("a", "b", "c", "d")),
        ("struct.descending-align", "long a; int b; short c; char d;", ("a", "b", "c", "d")),
        ("struct.three-char", "char a; char b; char c;", ("a", "b", "c")),
        ("struct.three-short", "short a; short b; short c;", ("a", "b", "c")),
        ("struct.three-int", "int a; int b; int c;", ("a", "b", "c")),
        ("struct.three-long", "long a; long b; long c;", ("a", "b", "c")),
        (
            "struct.deep-tail-padding",
            "char prefix[3]; struct { double x; char y; } nested; short tail;",
            ("prefix", "nested", "nested.x", "nested.y", "tail"),
        ),
    ]
    for case_id, members, field_names in struct_specs:
        add(
            case_id,
            "struct",
            f"typedef struct {{ {members} }} $T;",
            tuple((field, f"offsetof($T, {field})") for field in field_names),
        )

    union_specs = [
        ("union.char-int", "char a; int b;", ("a", "b")),
        ("union.char-long", "char a; long b;", ("a", "b")),
        ("union.char9-long", "char a[9]; long b;", ("a", "b")),
        ("union.int3-long", "int a[3]; long b;", ("a", "b")),
        ("union.padded-struct-long", "struct { char a; int b; } s; long l;", ("s", "s.a", "s.b", "l")),
        (
            "union.large-struct-double",
            "struct { char a; double b; char c; } s; double d;",
            ("s", "s.a", "s.b", "s.c", "d"),
        ),
        ("union.pointer-double", "void *p; double d;", ("p", "d")),
        ("union.long-double-char17", "long double ld; char bytes[17];", ("ld", "bytes")),
        (
            "union.enum-int",
            "enum { E_$N_A = -3, E_$N_B = 9 } mode; int word;",
            ("mode", "word"),
        ),
        ("union.nested-union-char9", "union { short s; long l; } inner; char bytes[9];", ("inner", "inner.s", "inner.l", "bytes")),
        ("union.short5-int", "short values[5]; int word;", ("values", "word")),
        ("union.double2-long-double", "double values[2]; long double ld;", ("values", "ld")),
        ("union.long-char-struct-char15", "struct { long l; char c; } s; char bytes[15];", ("s", "s.l", "s.c", "bytes")),
        ("union.pointer2-long-double", "void *pointers[2]; long double ld;", ("pointers", "ld")),
        ("union.complex-long3", "double _Complex z; long words[3];", ("z", "words")),
        (
            "union.nested-records",
            "struct { short a; int b; } s; union { char c[7]; long l; } u;",
            ("s", "s.a", "s.b", "u", "u.c", "u.l"),
        ),
    ]
    for case_id, members, field_names in union_specs:
        add(
            case_id,
            "union",
            f"typedef union {{ {members} }} $T;",
            tuple((field, f"offsetof($T, {field})") for field in field_names),
        )

    flexible_specs = [
        ("flex.int-char", "int count; char data[];", ("count", "data")),
        ("flex.char-int", "char tag; int data[];", ("tag", "data")),
        ("flex.long-char", "long count; char data[];", ("count", "data")),
        ("flex.char-long", "char tag; long data[];", ("tag", "data")),
        ("flex.short-int", "short count; int data[];", ("count", "data")),
        (
            "flex.nested-header-char",
            "struct { char tag; int count; } header; char data[];",
            ("header", "header.tag", "header.count", "data"),
        ),
        ("flex.char3-double", "char prefix[3]; double data[];", ("prefix", "data")),
        ("flex.pointer-ushort", "void *owner; unsigned short data[];", ("owner", "data")),
    ]
    for case_id, members, field_names in flexible_specs:
        add(
            case_id,
            "flexible-array",
            f"typedef struct {{ {members} }} $T;",
            tuple((field, f"offsetof($T, {field})") for field in field_names),
        )

    bitfield_specs = [
        ("bit.named-u32-1", "unsigned int a : 1;", (), ("value.a = 1u;",)),
        ("bit.named-u32-3", "unsigned int a : 3;", (), ("value.a = 5u;",)),
        ("bit.named-u32-7", "unsigned int a : 7;", (), ("value.a = 0x55u;",)),
        ("bit.named-u32-16", "unsigned int a : 16;", (), ("value.a = 0xabcdu;",)),
        ("bit.named-u32-31", "unsigned int a : 31;", (), ("value.a = 0x55555555u;",)),
        ("bit.named-uchar-3", "unsigned char a : 3;", (), ("value.a = 5u;",)),
        ("bit.named-ushort-9", "unsigned short a : 9;", (), ("value.a = 0x101u;",)),
        ("bit.named-ulong-40", "unsigned long a : 40;", (), ("value.a = 0xabcde12345ul;",)),
        ("bit.pack-u32-3-5", "unsigned int a : 3; unsigned int b : 5;", (), ("value.a = 5u;", "value.b = 17u;")),
        ("bit.pack-u32-5-12", "unsigned int a : 5; unsigned int b : 12;", (), ("value.a = 17u;", "value.b = 0xabcu;")),
        ("bit.pack-u32-16-16", "unsigned int a : 16; unsigned int b : 16;", (), ("value.a = 0x1234u;", "value.b = 0xabcdu;")),
        ("bit.pack-u32-31-1", "unsigned int a : 31; unsigned int b : 1;", (), ("value.a = 0x55555555u;", "value.b = 1u;")),
        ("bit.cross-u32-31-2", "unsigned int a : 31; unsigned int b : 2;", (), ("value.a = 0x55555555u;", "value.b = 3u;")),
        ("bit.cross-u32-30-3", "unsigned int a : 30; unsigned int b : 3;", (), ("value.a = 0x15555555u;", "value.b = 5u;")),
        ("bit.cross-uchar-7-2", "unsigned char a : 7; unsigned char b : 2;", (), ("value.a = 0x55u;", "value.b = 3u;")),
        ("bit.cross-ushort-15-2", "unsigned short a : 15; unsigned short b : 2;", (), ("value.a = 0x5555u;", "value.b = 3u;")),
        ("bit.cross-ulong-63-2", "unsigned long a : 63; unsigned long b : 2;", (), ("value.a = 0x5555555555555555ul;", "value.b = 3u;")),
        ("bit.unnamed-u32-gap", "unsigned int a : 3; unsigned int : 5; unsigned int b : 4;", (), ("value.a = 5u;", "value.b = 9u;")),
        ("bit.unnamed-uchar-gap", "unsigned char a : 2; unsigned char : 3; unsigned char b : 3;", (), ("value.a = 3u;", "value.b = 5u;")),
        ("bit.zero-u32-barrier", "unsigned int a : 3; unsigned int : 0; unsigned int b : 4;", (), ("value.a = 5u;", "value.b = 9u;")),
        ("bit.zero-u32-after-16", "unsigned int a : 16; unsigned int : 0; unsigned int b : 16;", (), ("value.a = 0x1234u;", "value.b = 0xabcdu;")),
        ("bit.zero-u32-leading", "unsigned int : 0; unsigned int b : 5;", (), ("value.b = 17u;",)),
        (
            "bit.char-sentinels",
            "char head; unsigned int a : 3; unsigned int b : 5; char tail;",
            ("head", "tail"),
            ("value.head = 0x12;", "value.a = 5u;", "value.b = 17u;", "value.tail = 0x34;"),
        ),
        (
            "bit.u32-then-int-tail",
            "unsigned int a : 7; int tail;",
            ("tail",),
            ("value.a = 0x55u;", "value.tail = 0x12345678;"),
        ),
        (
            "bit.nested-then-char",
            "struct { unsigned int a : 5; unsigned int b : 6; } inner; char tail;",
            ("inner", "tail"),
            ("value.inner.a = 17u;", "value.inner.b = 0x2au;", "value.tail = 0x33;"),
        ),
        (
            "bit.union-u32",
            "union { struct { unsigned int a : 5; } bits; unsigned int word; } value;",
            ("value", "value.bits", "value.word"),
            ("value.value.bits.a = 17u;",),
        ),
        (
            "bit.union-ulong",
            "union { struct { unsigned long a : 5; } bits; unsigned char bytes[8]; } value;",
            ("value", "value.bits", "value.bytes"),
            ("value.value.bits.a = 17u;",),
        ),
        (
            "bit.mixed-base-units",
            "unsigned int a : 4; unsigned long b : 5; unsigned int c : 6;",
            (),
            ("value.a = 9u;", "value.b = 17u;", "value.c = 0x2au;"),
        ),
    ]
    for case_id, members, field_names, assignments in bitfield_specs:
        add(
            case_id,
            "bit-field",
            f"typedef struct {{ {members} }} $T;",
            tuple((field, f"offsetof($T, {field})") for field in field_names),
            assignments,
        )

    return cases


def parse_observations(path: Path, cases: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    raw = path.read_bytes()
    if b"\r" in raw:
        raise ValueError(f"{path}: CR bytes are forbidden")
    lines = raw.decode("ascii").splitlines()
    if len(lines) != EXPECTED_TOTAL:
        raise ValueError(f"{path}: expected {EXPECTED_TOTAL} lines, got {len(lines)}")

    observations: dict[str, dict[str, Any]] = {}
    for case, line in zip(cases, lines):
        fields = line.split("\t")
        if len(fields) != 5:
            raise ValueError(f"{path}: {case['id']}: expected five TSV fields")
        case_id, size_text, align_text, offsets_text, storage_text = fields
        if case_id != case["id"]:
            raise ValueError(f"{path}: expected {case['id']}, got {case_id}")
        if not size_text.isdecimal() or not align_text.isdecimal():
            raise ValueError(f"{path}: {case_id}: non-decimal size/alignment")
        size = int(size_text)
        align = int(align_text)
        if size <= 0 or align <= 0 or align & (align - 1):
            raise ValueError(f"{path}: {case_id}: invalid size/alignment {size}/{align}")

        if case["offsets"]:
            if not re.fullmatch(r"[0-9]+(?:,[0-9]+)*", offsets_text):
                raise ValueError(f"{path}: {case_id}: malformed offsets")
            offsets = [int(value) for value in offsets_text.split(",")]
            if len(offsets) != len(case["offsets"]):
                raise ValueError(f"{path}: {case_id}: wrong offset count")
            if any(offset > size for offset in offsets):
                raise ValueError(f"{path}: {case_id}: offset exceeds object size")
        elif offsets_text == "-":
            offsets = []
        else:
            raise ValueError(f"{path}: {case_id}: unexpected offsets")

        if case["storageAssignments"]:
            if not re.fullmatch(r"[0-9a-f]+", storage_text):
                raise ValueError(f"{path}: {case_id}: malformed storage bytes")
            if len(storage_text) != size * 2:
                raise ValueError(f"{path}: {case_id}: storage byte count differs from sizeof")
            storage: str | None = storage_text
        elif storage_text == "-":
            storage = None
        else:
            raise ValueError(f"{path}: {case_id}: unexpected storage observation")

        observations[case_id] = {
            "size": size,
            "align": align,
            "offsets": offsets,
            "storageHex": storage,
        }
    return observations


def make_manifest(consensus_path: Path | None) -> dict[str, Any]:
    cases = build_cases()
    consensus = parse_observations(consensus_path, cases) if consensus_path else {}
    manifest_cases = []
    for case in cases:
        recorded = dict(case)
        recorded["oracleConsensus"] = consensus.get(case["id"])
        manifest_cases.append(recorded)
    manifest = {
        "schemaVersion": 1,
        "contractState": "frozen" if consensus_path else "unmeasured",
        "target": "aarch64-unknown-linux-gnu",
        "dialect": "gnu11",
        "caseCount": EXPECTED_TOTAL,
        "categoryCounts": EXPECTED_COUNTS,
        "observationSchema": ["id", "sizeof", "_Alignof", "offsetof[]", "storageHex"],
        "compilerFlags": COMPILER_FLAGS,
        "oracles": ORACLES,
        "oracleEvidence": None,
        "cases": manifest_cases,
    }
    if consensus_path:
        manifest["oracleEvidence"] = expected_oracle_evidence(manifest)
    return manifest


def validate_manifest(path: Path, require_consensus: bool = True) -> dict[str, Any]:
    manifest = json.loads(path.read_text(encoding="utf-8"))
    expected_cases = build_cases()
    if manifest.get("schemaVersion") != 1:
        raise ValueError("manifest schemaVersion differs")
    expected_state = "frozen" if require_consensus else manifest.get("contractState")
    if manifest.get("contractState") != expected_state or expected_state not in {
        "frozen",
        "unmeasured",
    }:
        raise ValueError("manifest contractState differs")
    if manifest.get("target") != "aarch64-unknown-linux-gnu":
        raise ValueError("manifest target differs")
    if manifest.get("dialect") != "gnu11":
        raise ValueError("manifest dialect differs")
    if manifest.get("observationSchema") != [
        "id",
        "sizeof",
        "_Alignof",
        "offsetof[]",
        "storageHex",
    ]:
        raise ValueError("manifest observation schema differs")
    if manifest.get("compilerFlags") != COMPILER_FLAGS:
        raise ValueError("compiler flags differ")
    if manifest.get("oracles") != ORACLES:
        raise ValueError("oracle identities differ")
    if manifest.get("caseCount") != EXPECTED_TOTAL:
        raise ValueError("manifest caseCount differs")
    if manifest.get("categoryCounts") != EXPECTED_COUNTS:
        raise ValueError("manifest categoryCounts differ")
    actual_cases = manifest.get("cases")
    if not isinstance(actual_cases, list) or len(actual_cases) != EXPECTED_TOTAL:
        raise ValueError("manifest does not contain exactly 128 cases")

    ids: set[str] = set()
    sources: set[bytes] = set()
    hashes: set[str] = set()
    for expected, actual in zip(expected_cases, actual_cases):
        for field in (
            "ordinal",
            "id",
            "category",
            "typeName",
            "source",
            "sourceSha256",
            "offsets",
            "storageAssignments",
        ):
            if actual.get(field) != expected[field]:
                raise ValueError(f"{expected['id']}: manifest field {field} differs")
        source = actual["source"].encode("utf-8")
        if actual["id"] in ids:
            raise ValueError(f"duplicate case id {actual['id']}")
        ids.add(actual["id"])
        if source in sources:
            raise ValueError(f"duplicate source bytes for {actual['id']}")
        sources.add(source)
        if actual["sourceSha256"] in hashes:
            raise ValueError(f"duplicate source hash for {actual['id']}")
        hashes.add(actual["sourceSha256"])
        if sha256_bytes(source) != actual["sourceSha256"]:
            raise ValueError(f"source hash differs for {actual['id']}")
        if require_consensus and actual.get("oracleConsensus") is None:
            raise ValueError(f"missing frozen oracle observation for {actual['id']}")

    counts = {
        category: sum(case["category"] == category for case in actual_cases)
        for category in EXPECTED_COUNTS
    }
    if counts != EXPECTED_COUNTS:
        raise ValueError(f"actual category counts differ: {counts}")
    has_consensus = all(case.get("oracleConsensus") is not None for case in actual_cases)
    expected_evidence = expected_oracle_evidence(manifest) if has_consensus else None
    if manifest.get("oracleEvidence") != expected_evidence:
        raise ValueError("frozen oracle evidence differs")
    return manifest


def render_c(manifest: dict[str, Any]) -> str:
    cases = manifest["cases"]
    lines = [
        "#include <stddef.h>",
        "#include <stdio.h>",
        "#include <string.h>",
        "",
    ]
    for case in cases:
        lines.append(f"/* {case['ordinal']:03d}: {case['id']} */")
        lines.append(case["source"].rstrip("\n"))
    lines.extend(
        [
            "",
            "static void dump_bytes(const void *object, size_t size)",
            "{",
            "    const unsigned char *bytes = object;",
            "    for (size_t index = 0; index < size; ++index) {",
            '        printf("%02x", (unsigned int)bytes[index]);',
            "    }",
            "}",
            "",
        ]
    )
    for case in cases:
        ordinal = case["ordinal"]
        type_name = case["typeName"]
        lines.extend(
            [
                f"static void probe_{ordinal:03d}(void)",
                "{",
                f'    printf("{case["id"]}\\t%zu\\t%zu\\t", sizeof({type_name}), _Alignof({type_name}));',
            ]
        )
        if case["offsets"]:
            for index, offset in enumerate(case["offsets"]):
                prefix = "" if index == 0 else ","
                lines.append(
                    f'    printf("{prefix}%zu", (size_t)({offset["expression"]}));'
                )
        else:
            lines.append("    putchar('-');")
        lines.append("    putchar('\\t');")
        if case["storageAssignments"]:
            lines.extend(
                [
                    f"    {type_name} value;",
                    "    memset(&value, 0, sizeof(value));",
                ]
            )
            lines.extend(f"    {assignment}" for assignment in case["storageAssignments"])
            lines.append("    dump_bytes(&value, sizeof(value));")
        else:
            lines.append("    putchar('-');")
        lines.extend(["    putchar('\\n');", "}", ""])
    lines.extend(["int main(void)", "{"])
    lines.extend(f"    probe_{case['ordinal']:03d}();" for case in cases)
    lines.extend(["    return 0;", "}", ""])
    return "\n".join(lines)


def observation_line(case: dict[str, Any], observation: dict[str, Any]) -> str:
    offsets = ",".join(str(value) for value in observation["offsets"]) or "-"
    storage = observation["storageHex"] or "-"
    return (
        f"{case['id']}\t{observation['size']}\t{observation['align']}\t"
        f"{offsets}\t{storage}"
    )


def expected_oracle_evidence(manifest: dict[str, Any]) -> dict[str, Any]:
    consensus = "".join(
        observation_line(case, case["oracleConsensus"]) + "\n"
        for case in manifest["cases"]
    ).encode("ascii")
    observer = render_c(manifest).encode("utf-8")
    return {
        "observerSourceSha256": sha256_bytes(observer),
        "consensusSha256": sha256_bytes(consensus),
        "imageId": ORACLES["imageId"],
        "gcc": ORACLES["gcc"],
        "clang": ORACLES["clang"],
        "bindingPolicyVersion": BINDING_POLICY_VERSION,
        "digestAddressableLocalExecution": False,
    }


def oracle_conflicts(
    cases: list[dict[str, Any]],
    gcc: dict[str, dict[str, Any]],
    clang: dict[str, dict[str, Any]],
) -> list[dict[str, Any]]:
    return [
        {
            "id": case["id"],
            "gcc": gcc[case["id"]],
            "clang": clang[case["id"]],
        }
        for case in cases
        if gcc[case["id"]] != clang[case["id"]]
    ]


def compare_oracles(
    manifest_path: Path,
    gcc_path: Path,
    clang_path: Path,
    allow_unmeasured: bool = False,
) -> int:
    manifest = validate_manifest(manifest_path, not allow_unmeasured)
    cases = manifest["cases"]
    gcc = parse_observations(gcc_path, cases)
    clang = parse_observations(clang_path, cases)
    conflicts = oracle_conflicts(cases, gcc, clang)
    if conflicts:
        print(
            json.dumps(
                {
                    "status": "ORACLE_CONFLICT",
                    "total": EXPECTED_TOTAL,
                    "conflictCount": len(conflicts),
                    "conflicts": conflicts,
                },
                sort_keys=True,
            )
        )
        return 1

    if allow_unmeasured:
        print(
            json.dumps(
                {
                    "status": "ORACLE_CONSENSUS_UNFROZEN",
                    "total": EXPECTED_TOTAL,
                    "conflictCount": 0,
                },
                sort_keys=True,
            )
        )
        return 0

    drift = [
        {
            "id": case["id"],
            "frozen": case["oracleConsensus"],
            "observed": gcc[case["id"]],
        }
        for case in cases
        if case["oracleConsensus"] != gcc[case["id"]]
    ]
    status = "ORACLE_DRIFT" if drift else "ORACLE_CONSENSUS"
    print(
        json.dumps(
            {
                "status": status,
                "total": EXPECTED_TOTAL,
                "conflictCount": 0,
                "driftCount": len(drift),
                "drift": drift,
            },
            sort_keys=True,
        )
    )
    return 1 if drift else 0


def self_test(manifest_path: Path) -> int:
    manifest = validate_manifest(manifest_path)
    cases = manifest["cases"]
    gcc = {case["id"]: dict(case["oracleConsensus"]) for case in cases}
    clang = {case_id: dict(observation) for case_id, observation in gcc.items()}
    clang[cases[0]["id"]]["size"] += 1
    conflicts = oracle_conflicts(cases, gcc, clang)
    if len(conflicts) != 1 or conflicts[0]["id"] != cases[0]["id"]:
        raise ValueError("oracle disagreement did not classify exactly one conflict")
    print(
        json.dumps(
            {
                "status": "ORACLE_CONFLICT",
                "conflictCount": 1,
                "majorityVoting": "FORBIDDEN",
                "mutatedCase": cases[0]["id"],
            },
            sort_keys=True,
        )
    )
    return 0


def compare_subject(manifest_path: Path, subject_path: Path) -> int:
    manifest = validate_manifest(manifest_path)
    cases = manifest["cases"]
    subject = parse_observations(subject_path, cases)
    mismatches = [
        {
            "id": case["id"],
            "expected": case["oracleConsensus"],
            "subject": subject[case["id"]],
        }
        for case in cases
        if case["oracleConsensus"] != subject[case["id"]]
    ]
    print(
        json.dumps(
            {
                "status": "MISMATCH" if mismatches else "PASS",
                "total": EXPECTED_TOTAL,
                "mismatchCount": len(mismatches),
                "mismatches": mismatches,
            },
            sort_keys=True,
        )
    )
    return 1 if mismatches else 0


def main() -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)

    emit_manifest = subparsers.add_parser("emit-manifest")
    emit_manifest.add_argument("--consensus", type=Path)

    validate = subparsers.add_parser("validate-manifest")
    validate.add_argument("manifest", type=Path)
    validate.add_argument("--allow-unmeasured", action="store_true")

    emit_c = subparsers.add_parser("emit-c")
    emit_c.add_argument("manifest", type=Path)
    emit_c.add_argument("--allow-unmeasured", action="store_true")

    verify = subparsers.add_parser("verify-observations")
    verify.add_argument("manifest", type=Path)
    verify.add_argument("observations", type=Path)
    verify.add_argument("--allow-unmeasured", action="store_true")

    compare = subparsers.add_parser("compare-oracles")
    compare.add_argument("manifest", type=Path)
    compare.add_argument("gcc", type=Path)
    compare.add_argument("clang", type=Path)
    compare.add_argument("--allow-unmeasured", action="store_true")

    subject = subparsers.add_parser("compare-subject")
    subject.add_argument("manifest", type=Path)
    subject.add_argument("observations", type=Path)

    selftest = subparsers.add_parser("self-test")
    selftest.add_argument("manifest", type=Path)

    args = parser.parse_args()
    try:
        if args.command == "emit-manifest":
            print(json.dumps(make_manifest(args.consensus), indent=2, ensure_ascii=True))
        elif args.command == "validate-manifest":
            manifest = validate_manifest(args.manifest, not args.allow_unmeasured)
            print(
                json.dumps(
                    {
                        "status": "PASS",
                        "caseCount": len(manifest["cases"]),
                        "categoryCounts": manifest["categoryCounts"],
                    },
                    sort_keys=True,
                )
            )
        elif args.command == "emit-c":
            print(
                render_c(validate_manifest(args.manifest, not args.allow_unmeasured)),
                end="",
            )
        elif args.command == "verify-observations":
            manifest = validate_manifest(args.manifest, not args.allow_unmeasured)
            observations = parse_observations(args.observations, manifest["cases"])
            print(json.dumps({"status": "PASS", "caseCount": len(observations)}))
        elif args.command == "compare-oracles":
            return compare_oracles(
                args.manifest,
                args.gcc,
                args.clang,
                args.allow_unmeasured,
            )
        elif args.command == "compare-subject":
            return compare_subject(args.manifest, args.observations)
        elif args.command == "self-test":
            return self_test(args.manifest)
        else:
            raise AssertionError(args.command)
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError) as error:
        print(f"day5-layout-corpus: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
