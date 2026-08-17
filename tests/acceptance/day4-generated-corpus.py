#!/usr/bin/env python3

"""Deterministic, independently modelled Day-4 parser corpus.

The default action is a production-independent model audit.  It proves the
denominators, Cartesian coverage, byte uniqueness, seed discipline, and the
checked-in SHA-256 freeze.  ``--behavior-cli`` is deliberately separate: it
runs every generated translation unit through an evocc binary and checks the
process contract.  Consumers which inspect C-HIR can use ``--emit-jsonl``;
the expected fingerprint and span facts come from this generator, not evocc.

The structural axes are fixed.  The seed is allowed to influence generated C
identifiers and integer literals only; the audit compares lexical skeletons
against a second seed to enforce that rule mechanically.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import dataclasses
import hashlib
import json
import pathlib
import re
import subprocess
import sys
from collections.abc import Callable, Iterable, Sequence
from typing import Optional, Union


SCHEMA = "c30.day4.generated-corpus.v1"
DEFAULT_SEED = "d4-2026-08-17"
ALTERNATE_AUDIT_SEED = "d4-seed-discipline-witness"
MODEL_FILE_ID = 73
DECLARATION_AXIS_COUNT = 25
STATEMENT_AXIS_COUNT = 40
VALID_COUNT = DECLARATION_AXIS_COUNT * STATEMENT_AXIS_COUNT
MALFORMED_FAMILY_COUNT = 20
MALFORMED_VARIANTS = 10
MALFORMED_COUNT = MALFORMED_FAMILY_COUNT * MALFORMED_VARIANTS
CONTRACT = pathlib.Path(__file__).resolve().parents[1] / "contracts/day4-generated-corpus.json"


KEYWORDS = frozenset(
    """_Alignof _Atomic _Bool auto break case char const continue default do
    double else enum extern float for goto if int long register restrict return
    short signed sizeof static struct switch typedef union unsigned void volatile
    while _Thread_local""".split()
)
TOKEN_RE = re.compile(r"[A-Za-z_][A-Za-z_0-9]*|[0-9]+")


@dataclasses.dataclass(frozen=True)
class SpanFact:
    label: str
    byte_start: int
    byte_end: int
    line: int
    column: int
    file_id: int = MODEL_FILE_ID

    def manifest(self) -> dict[str, object]:
        return dataclasses.asdict(self)


@dataclasses.dataclass(frozen=True)
class DeclModel:
    key: str
    parts: tuple[str, ...]
    fingerprint: str


@dataclasses.dataclass(frozen=True)
class StatementModel:
    key: str
    body: str
    fingerprint: str
    nodes: tuple[tuple[str, str, int], ...]


@dataclasses.dataclass(frozen=True)
class ValidCase:
    case_id: str
    declaration_axis: str
    statement_axis: str
    source: str
    expected_fingerprint: str
    span_facts: tuple[SpanFact, ...]

    def manifest(self) -> dict[str, object]:
        return {
            "kind": "valid",
            "id": self.case_id,
            "declaration_axis": self.declaration_axis,
            "statement_axis": self.statement_axis,
            "source_sha256": sha256_text(self.source),
            "expected_fingerprint": self.expected_fingerprint,
            "span_facts": [fact.manifest() for fact in self.span_facts],
        }

    def emitted(self) -> dict[str, object]:
        value = self.manifest()
        value["source"] = self.source
        return value


@dataclasses.dataclass(frozen=True)
class MalformedCase:
    case_id: str
    family: str
    variant: int
    source: str
    code: str
    message: str
    byte_start: int
    byte_end: int

    @property
    def line(self) -> int:
        return 1

    @property
    def column(self) -> int:
        return self.byte_start + 1

    def manifest(self) -> dict[str, object]:
        return {
            "kind": "malformed",
            "id": self.case_id,
            "family": self.family,
            "variant": self.variant,
            "source_sha256": sha256_text(self.source),
            "diagnostic": {
                "file_id": MODEL_FILE_ID,
                "line": self.line,
                "column": self.column,
                "byte_start": self.byte_start,
                "byte_end": self.byte_end,
                "code": self.code,
                "message": self.message,
            },
        }

    def emitted(self) -> dict[str, object]:
        value = self.manifest()
        value["source"] = self.source
        return value


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("ascii")).hexdigest()


def generated_identifier(seed: str, role: str, *coordinates: int) -> str:
    payload = ":".join([SCHEMA, seed, role, *(str(value) for value in coordinates)])
    suffix = hashlib.sha256(payload.encode("ascii")).hexdigest()[:10]
    return f"d4_{role}_{suffix}"


def generated_literal(seed: str, role: str, *coordinates: int) -> int:
    payload = ":".join([SCHEMA, seed, role, *(str(value) for value in coordinates)])
    # Stay in the parser's uncontroversial decimal-literal range and never 0.
    return 1 + int(hashlib.sha256(payload.encode("ascii")).hexdigest()[:8], 16) % 97


def declaration_models(seed: str, row: int) -> tuple[DeclModel, ...]:
    def name(role: str) -> str:
        return generated_identifier(seed, role, row)

    def lit(role: str) -> int:
        return generated_literal(seed, role, row)

    tag = name("tag")
    tag2 = name("tag_b")
    obj = name("object")
    field = name("field")
    field2 = name("field_b")
    enum_a = name("enum_a")
    enum_b = name("enum_b")
    proto = name("prototype")
    arg = name("argument")
    alias = name("alias")
    bound = lit("bound")
    enum_value = lit("enum_value")
    bit_width = 1 + lit("bit_width") % 7

    return (
        DeclModel(
            "struct.named_definition",
            (f"struct {tag} {{ int {field}; long {field2}; }};",),
            f"tag(struct#0({tag}),definition,[{field}:int,{field2}:long])",
        ),
        DeclModel(
            "struct.forward_pointer_object",
            (f"struct {tag};", f"struct {tag} *{obj};"),
            f"tag(struct#0({tag}),forward);object({obj},ptr(struct#0({tag})))",
        ),
        DeclModel(
            "struct.anonymous_object",
            (f"struct {{ int {field}; long {field2}; }} {obj};",),
            f"tag(struct#0(anon),definition,[{field}:int,{field2}:long]);object({obj},struct#0(anon))",
        ),
        DeclModel(
            "struct.nested_member",
            (
                f"struct {tag} {{ int {field}; }};",
                f"struct {tag2} {{ struct {tag} {field2}; }};",
            ),
            f"tag(struct#0({tag}),definition,[{field}:int]);tag(struct#1({tag2}),definition,[{field2}:struct#0({tag})])",
        ),
        DeclModel(
            "struct.self_pointer",
            (f"struct {tag} {{ int {field}; struct {tag} *{field2}; }};",),
            f"tag(struct#0({tag}),definition,[{field}:int,{field2}:ptr(struct#0({tag}))])",
        ),
        DeclModel(
            "union.named_definition",
            (f"union {tag} {{ int {field}; long {field2}; }};",),
            f"tag(union#0({tag}),definition,[{field}:int,{field2}:long])",
        ),
        DeclModel(
            "enum.named_values",
            (f"enum {tag} {{ {enum_a}, {enum_b} = {enum_value} }};",),
            f"tag(enum#0({tag}),definition,[{enum_a}=0,{enum_b}={enum_value}])",
        ),
        DeclModel(
            "function.struct_parameter",
            (f"struct {tag};", f"int {proto}(struct {tag} *{arg});"),
            f"tag(struct#0({tag}),forward);function({proto},[{arg}:ptr(struct#0({tag}))],int,prototype,variadic:false)",
        ),
        DeclModel(
            "function.unspecified",
            (f"int {proto}();",),
            f"function({proto},[],int,unspecified,variadic:false)",
        ),
        DeclModel(
            "function.void_prototype",
            (f"int {proto}(void);",),
            f"function({proto},[],int,prototype,variadic:false)",
        ),
        DeclModel(
            "function.named_parameter",
            (f"int {proto}(int {arg});",),
            f"function({proto},[{arg}:int],int,prototype,variadic:false)",
        ),
        DeclModel(
            "function.unnamed_parameter",
            (f"int {proto}(int);",),
            f"function({proto},[anon:int],int,prototype,variadic:false)",
        ),
        DeclModel(
            "function.variadic",
            (f"int {proto}(int {arg}, ...);",),
            f"function({proto},[{arg}:int],int,prototype,variadic:true)",
        ),
        DeclModel(
            "object.static",
            (f"static int {obj};",),
            f"object({obj},int,storage:static)",
        ),
        DeclModel(
            "object.extern_long",
            (f"extern long {obj};",),
            f"object({obj},long,storage:extern)",
        ),
        DeclModel(
            "object.const_pointer",
            (f"const int *{obj};",),
            f"object({obj},ptr(qual[const](int)))",
        ),
        DeclModel(
            "object.array",
            (f"int {obj}[{bound}];",),
            f"object({obj},array(Constant({bound}),int))",
        ),
        DeclModel(
            "object.function_pointer",
            (f"int (*{obj})(int {arg});",),
            f"object({obj},ptr(fn([int],prototype,variadic:false)->int))",
        ),
        DeclModel(
            "typedef.builtin",
            (f"typedef int {alias};",),
            f"typedef({alias},int)",
        ),
        DeclModel(
            "struct.bit_fields",
            (
                f"struct {tag} {{ unsigned {field} : {bit_width}; unsigned : 0; }};",
            ),
            f"tag(struct#0({tag}),definition,[{field}:uint:bits={bit_width},anon:uint:bits=0])",
        ),
        DeclModel(
            "union.anonymous_object",
            (f"union {{ int {field}; long {field2}; }} {obj};",),
            f"tag(union#0(anon),definition,[{field}:int,{field2}:long]);object({obj},union#0(anon))",
        ),
        DeclModel(
            "enum.anonymous_object",
            (f"enum {{ {enum_a}, {enum_b} = {enum_value} }} {obj};",),
            f"tag(enum#0(anon),definition,[{enum_a}=0,{enum_b}={enum_value}]);object({obj},enum#0(anon))",
        ),
        DeclModel(
            "struct.forward_in_holder",
            (
                f"struct {tag};",
                f"struct {tag2} {{ struct {tag} *{field}; }};",
            ),
            f"tag(struct#0({tag}),forward);tag(struct#1({tag2}),definition,[{field}:ptr(struct#0({tag}))])",
        ),
        DeclModel(
            "function.pointer_return",
            (f"int *{proto}(void);",),
            f"function({proto},[],ptr(int),prototype,variadic:false)",
        ),
        DeclModel(
            "function.array_parameter",
            (f"int {proto}(int {arg}[{bound}]);",),
            f"function({proto},[{arg}:ptr(int)],int,prototype,variadic:false,written:array({bound}))",
        ),
    )


def statement_models(seed: str, column: int) -> tuple[StatementModel, ...]:
    v = generated_identifier(seed, "value", column)
    local = generated_identifier(seed, "local", column)
    label = generated_identifier(seed, "label", column)
    callee = generated_identifier(seed, "callee", column)
    obj = generated_identifier(seed, "record", column)
    member = generated_identifier(seed, "member", column)
    a = generated_literal(seed, "literal_a", column)
    b = generated_literal(seed, "literal_b", column)
    c = generated_literal(seed, "literal_c", column)

    def stmt(
        key: str,
        body: str,
        fingerprint: str,
        *nodes: tuple[str, str, int],
    ) -> StatementModel:
        return StatementModel(key, body, fingerprint, tuple(nodes))

    return (
        stmt("null", "{ ; }", "compound[null]", ("stmt.null", ";", 0)),
        stmt("expression.ident", f"{{ {v}; }}", f"compound[expr(id({v}))]", ("stmt.expr", f"{v};", 0)),
        stmt("expression.assign", f"{{ {v} = {a}; }}", f"compound[expr(assign(id({v}),int({a})))]", ("stmt.expr", f"{v} = {a};", 0)),
        stmt("return.empty", "{ return; }", "compound[return()]", ("stmt.return", "return;", 0)),
        stmt("return.value", f"{{ return {a}; }}", f"compound[return(int({a}))]", ("stmt.return", f"return {a};", 0)),
        stmt("compound.nested", f"{{ {{ {v} = {a}; }} }}", f"compound[compound[expr(assign(id({v}),int({a})))]]", ("stmt.compound", f"{{ {v} = {a}; }}", 0), ("stmt.expr", f"{v} = {a};", 0)),
        stmt("if.then", f"{{ if ({v}) {v} = {a}; }}", f"compound[if(id({v}),expr(assign(id({v}),int({a}))),none)]", ("stmt.if", f"if ({v}) {v} = {a};", 0), ("stmt.then", f"{v} = {a};", 0)),
        stmt("if.else", f"{{ if ({v}) {v} = {a}; else {v} = {b}; }}", f"compound[if(id({v}),expr(assign(id({v}),int({a}))),expr(assign(id({v}),int({b}))))]", ("stmt.if", f"if ({v}) {v} = {a}; else {v} = {b};", 0), ("stmt.then", f"{v} = {a};", 0), ("stmt.else", f"{v} = {b};", 0)),
        stmt("if.compound_arms", f"{{ if ({v}) {{ return {a}; }} else {{ return {b}; }} }}", f"compound[if(id({v}),compound[return(int({a}))],compound[return(int({b}))])]", ("stmt.if", f"if ({v}) {{ return {a}; }} else {{ return {b}; }}", 0)),
        stmt("while.break", f"{{ while ({v}) {{ break; }} }}", f"compound[while(id({v}),compound[break])]", ("stmt.while", f"while ({v}) {{ break; }}", 0), ("stmt.break", "break;", 0)),
        stmt("while.continue", f"{{ while ({v}) {{ {v} = {v} - {a}; continue; }} }}", f"compound[while(id({v}),compound[expr(assign(id({v}),sub(id({v}),int({a})))),continue])]", ("stmt.while", f"while ({v}) {{ {v} = {v} - {a}; continue; }}", 0), ("stmt.continue", "continue;", 0)),
        stmt("do_while", f"{{ do {{ {v} = {v} - {a}; }} while ({v}); }}", f"compound[do(compound[expr(assign(id({v}),sub(id({v}),int({a}))))],id({v}))]", ("stmt.do", f"do {{ {v} = {v} - {a}; }} while ({v});", 0)),
        stmt("for.empty", "{ for (;;) { break; } }", "compound[for(empty,none,none,compound[break])]", ("stmt.for", "for (;;) { break; }", 0)),
        stmt("for.expression", f"{{ for ({v} = {a}; {v} < {b}; {v} = {v} + {c}) ; }}", f"compound[for(expr(assign(id({v}),int({a}))),lt(id({v}),int({b})),assign(id({v}),add(id({v}),int({c}))),null)]", ("stmt.for", f"for ({v} = {a}; {v} < {b}; {v} = {v} + {c}) ;", 0)),
        stmt("for.declaration", f"{{ for (int {local} = {a}; {local} < {b}; {local} = {local} + {c}) {v} = {v} + {local}; }}", f"compound[for(decl(object({local},int,init=int({a}))),lt(id({local}),int({b})),assign(id({local}),add(id({local}),int({c}))),expr(assign(id({v}),add(id({v}),id({local})))))]", ("stmt.for", f"for (int {local} = {a}; {local} < {b}; {local} = {local} + {c}) {v} = {v} + {local};", 0)),
        stmt("switch.case_default", f"{{ switch ({v}) {{ case {a}: break; default: break; }} }}", f"compound[switch(id({v}),compound[case(int({a}),break),default(break)])]", ("stmt.switch", f"switch ({v}) {{ case {a}: break; default: break; }}", 0), ("stmt.case", f"case {a}: break;", 0), ("stmt.default", "default: break;", 0)),
        stmt("switch.multiple_cases", f"{{ switch ({v}) {{ case {a}: {v} = {b}; break; case {c}: {v} = {a}; break; default: ; }} }}", f"compound[switch(id({v}),compound[case(int({a}),expr(assign(id({v}),int({b})))),break,case(int({c}),expr(assign(id({v}),int({a})))),break,default(null)])]", ("stmt.switch", f"switch ({v}) {{ case {a}: {v} = {b}; break; case {c}: {v} = {a}; break; default: ; }}", 0)),
        stmt("goto.forward_label", f"{{ goto {label}; {label}: return {v}; }}", f"compound[goto({label}),label({label},return(id({v})))]", ("stmt.goto", f"goto {label};", 0), ("stmt.label", f"{label}: return {v};", 0)),
        stmt("label.backward_goto", f"{{ {label}: ; goto {label}; }}", f"compound[label({label},null),goto({label})]", ("stmt.label", f"{label}: ;", 0), ("stmt.goto", f"goto {label};", 0)),
        stmt("block.declaration", f"{{ {{ int {local}; {local} = {v}; }} }}", f"compound[compound[decl(object({local},int)),expr(assign(id({local}),id({v})))]]", ("stmt.compound", f"{{ int {local}; {local} = {v}; }}", 0), ("decl.block", f"int {local};", 0)),
        stmt("block.initialized_declaration", f"{{ int {local} = {a}; return {local}; }}", f"compound[decl(object({local},int,init=int({a}))),return(id({local}))]", ("decl.block", f"int {local} = {a};", 0), ("stmt.return", f"return {local};", 0)),
        stmt("expression.comma", f"{{ ({v} = {a}, {v} = {b}); }}", f"compound[expr(comma(assign(id({v}),int({a})),assign(id({v}),int({b}))))]", ("stmt.expr", f"({v} = {a}, {v} = {b});", 0)),
        stmt("expression.call", f"{{ {callee}({v}, {a}); }}", f"compound[expr(call(id({callee}),[id({v}),int({a})]))]", ("stmt.expr", f"{callee}({v}, {a});", 0)),
        stmt("expression.member", f"{{ {obj}.{member}; }}", f"compound[expr(member(id({obj}),{member}))]", ("stmt.expr", f"{obj}.{member};", 0)),
        stmt("expression.pointer_member", f"{{ {obj}->{member}; }}", f"compound[expr(ptr_member(id({obj}),{member}))]", ("stmt.expr", f"{obj}->{member};", 0)),
        stmt("expression.index", f"{{ {obj}[{v}]; }}", f"compound[expr(index(id({obj}),id({v})))]", ("stmt.expr", f"{obj}[{v}];", 0)),
        stmt("expression.ternary", f"{{ {v} ? {a} : {b}; }}", f"compound[expr(ternary(id({v}),int({a}),int({b})))]", ("stmt.expr", f"{v} ? {a} : {b};", 0)),
        stmt("expression.sizeof_type", "{ sizeof(int); }", "compound[expr(sizeof(type:int))]", ("stmt.expr", "sizeof(int);", 0)),
        stmt("expression.sizeof_expr", f"{{ sizeof {v}; }}", f"compound[expr(sizeof_expr(id({v})))]", ("stmt.expr", f"sizeof {v};", 0)),
        stmt("expression.alignof", "{ _Alignof(int); }", "compound[expr(alignof(type:int))]", ("stmt.expr", "_Alignof(int);", 0)),
        stmt("expression.prefix", f"{{ ++{v}; --{v}; }}", f"compound[expr(pre_inc(id({v}))),expr(pre_dec(id({v})))]", ("stmt.pre_inc", f"++{v};", 0), ("stmt.pre_dec", f"--{v};", 0)),
        stmt("expression.postfix", f"{{ {v}++; {v}--; }}", f"compound[expr(post_inc(id({v}))),expr(post_dec(id({v})))]", ("stmt.post_inc", f"{v}++;", 0), ("stmt.post_dec", f"{v}--;", 0)),
        stmt("for.omitted_step", f"{{ for (; {v};) {{ if ({v}) continue; }} }}", f"compound[for(empty,id({v}),none,compound[if(id({v}),continue,none)])]", ("stmt.for", f"for (; {v};) {{ if ({v}) continue; }}", 0)),
        stmt("label.loop", f"{{ {label}: while ({v}) {{ break; }} }}", f"compound[label({label},while(id({v}),compound[break]))]", ("stmt.label", f"{label}: while ({v}) {{ break; }}", 0)),
        stmt("while.switch", f"{{ while ({v}) {{ switch ({v}) {{ default: break; }} break; }} }}", f"compound[while(id({v}),compound[switch(id({v}),compound[default(break)]),break])]", ("stmt.while", f"while ({v}) {{ switch ({v}) {{ default: break; }} break; }}", 0)),
        stmt("do.null", f"{{ do ; while ({v}); }}", f"compound[do(null,id({v}))]", ("stmt.do", f"do ; while ({v});", 0)),
        stmt("if.block_declaration", f"{{ if ({v}) {{ int {local}; {local} = {a}; }} }}", f"compound[if(id({v}),compound[decl(object({local},int)),expr(assign(id({local}),int({a})))],none)]", ("stmt.if", f"if ({v}) {{ int {local}; {local} = {a}; }}", 0)),
        stmt("null.sequence", "{ ; ; ; }", "compound[null,null,null]", ("stmt.null.0", ";", 0), ("stmt.null.1", ";", 1), ("stmt.null.2", ";", 2)),
        stmt("return.comma", f"{{ return ({v}, {a}); }}", f"compound[return(comma(id({v}),int({a})))]", ("stmt.return", f"return ({v}, {a});", 0)),
        stmt("compound.deep", f"{{ {{ {{ {v} = {a}; }} }} }}", f"compound[compound[compound[expr(assign(id({v}),int({a})))]]]", ("stmt.compound.0", f"{{ {{ {v} = {a}; }} }}", 0), ("stmt.compound.1", f"{{ {v} = {a}; }}", 0)),
    )


def nth_span(source: str, fragment: str, occurrence: int) -> tuple[int, int]:
    start = -1
    cursor = 0
    for _ in range(occurrence + 1):
        start = source.find(fragment, cursor)
        if start < 0:
            raise AssertionError(f"missing span fragment {fragment!r} occurrence {occurrence}")
        cursor = start + 1
    return start, start + len(fragment)


def make_span(label: str, start: int, end: int) -> SpanFact:
    if not (0 <= start <= end):
        raise AssertionError((label, start, end))
    return SpanFact(label, start, end, 1, start + 1)


# ---------------------------------------------------------------------------
# Independent statement/expression model
# ---------------------------------------------------------------------------


C_TOKEN_RE = re.compile(
    r"\s+|\.\.\.|\+\+|--|->|<=|>=|==|!=|&&|\|\||<<|>>|"
    r"[A-Za-z_][A-Za-z_0-9]*|[0-9]+|[][{}();,:?+*/%<>=.!&|^~-]"
)


@dataclasses.dataclass(frozen=True)
class ModelToken:
    text: str
    start: int
    end: int


@dataclasses.dataclass(frozen=True)
class ModelExpr:
    kind: str
    start: int
    end: int
    value: Optional[str] = None
    children: tuple["ModelExpr", ...] = ()

    def fingerprint(self) -> str:
        if self.kind == "int":
            return f"int({self.value})"
        if self.kind == "id":
            return f"id({self.value})"
        if self.kind in {
            "neg",
            "not",
            "bit_not",
            "addr_of",
            "deref",
            "pre_inc",
            "pre_dec",
            "post_inc",
            "post_dec",
            "plus",
            "sizeof_expr",
        }:
            return f"{self.kind}({self.children[0].fingerprint()})"
        if self.kind in {
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "lt",
            "le",
            "gt",
            "ge",
            "eq",
            "ne",
            "and",
            "or",
            "bit_and",
            "bit_or",
            "bit_xor",
            "shl",
            "shr",
            "assign",
            "comma",
            "index",
        }:
            return f"{self.kind}({self.children[0].fingerprint()},{self.children[1].fingerprint()})"
        if self.kind == "ternary":
            return "ternary({},{},{})".format(*(child.fingerprint() for child in self.children))
        if self.kind == "call":
            return f"call({self.children[0].fingerprint()},[{','.join(child.fingerprint() for child in self.children[1:])}])"
        if self.kind in {"member", "ptr_member"}:
            return f"{self.kind}({self.children[0].fingerprint()},{self.value})"
        if self.kind in {"sizeof", "alignof"}:
            return f"{self.kind}(type:{self.value})"
        raise AssertionError(f"unhandled model expression {self.kind}")

    def spans(self, path: str) -> list[SpanFact]:
        facts = [make_span(path, self.start, self.end)]
        if self.kind in {
            "neg",
            "not",
            "bit_not",
            "addr_of",
            "deref",
            "pre_inc",
            "pre_dec",
            "post_inc",
            "post_dec",
            "plus",
            "sizeof_expr",
        }:
            facts.extend(self.children[0].spans(f"{path}.operand"))
        elif self.kind in {
            "add",
            "sub",
            "mul",
            "div",
            "mod",
            "lt",
            "le",
            "gt",
            "ge",
            "eq",
            "ne",
            "and",
            "or",
            "bit_and",
            "bit_or",
            "bit_xor",
            "shl",
            "shr",
            "assign",
            "comma",
        }:
            facts.extend(self.children[0].spans(f"{path}.left"))
            facts.extend(self.children[1].spans(f"{path}.right"))
        elif self.kind == "ternary":
            facts.extend(self.children[0].spans(f"{path}.condition"))
            facts.extend(self.children[1].spans(f"{path}.then"))
            facts.extend(self.children[2].spans(f"{path}.else"))
        elif self.kind == "call":
            facts.extend(self.children[0].spans(f"{path}.callee"))
            for index, argument in enumerate(self.children[1:]):
                facts.extend(argument.spans(f"{path}.argument[{index}]"))
        elif self.kind in {"member", "ptr_member"}:
            facts.extend(self.children[0].spans(f"{path}.object"))
        elif self.kind == "index":
            facts.extend(self.children[0].spans(f"{path}.object"))
            facts.extend(self.children[1].spans(f"{path}.index"))
        return facts


@dataclasses.dataclass(frozen=True)
class ModelLocalDecl:
    start: int
    end: int
    specifier_start: int
    specifier_end: int
    name: str
    name_start: int
    name_end: int
    initializer: Optional[ModelExpr]

    def fingerprint(self) -> str:
        initializer = "" if self.initializer is None else f",init={self.initializer.fingerprint()}"
        return f"object({self.name},int{initializer})"

    def spans(self, path: str) -> list[SpanFact]:
        facts = [
            make_span(path, self.start, self.end),
            make_span(f"{path}.name", self.name_start, self.name_end),
            make_span(f"{path}.specifier", self.specifier_start, self.specifier_end),
            make_span(f"{path}.written", self.name_start, self.name_end),
        ]
        if self.initializer is not None:
            facts.extend(self.initializer.spans(f"{path}.initializer"))
        return facts


@dataclasses.dataclass(frozen=True)
class ModelStmt:
    kind: str
    start: int
    end: int
    expressions: tuple[ModelExpr, ...] = ()
    statements: tuple["ModelStmt", ...] = ()
    items: tuple[Union["ModelStmt", ModelLocalDecl], ...] = ()
    declaration: Optional[ModelLocalDecl] = None
    name: Optional[str] = None
    name_start: Optional[int] = None
    name_end: Optional[int] = None

    def fingerprint(self) -> str:
        if self.kind == "expr":
            return f"expr({self.expressions[0].fingerprint()})"
        if self.kind == "if":
            otherwise = "none" if len(self.statements) == 1 else self.statements[1].fingerprint()
            return f"if({self.expressions[0].fingerprint()},{self.statements[0].fingerprint()},{otherwise})"
        if self.kind in {"switch", "while"}:
            return f"{self.kind}({self.expressions[0].fingerprint()},{self.statements[0].fingerprint()})"
        if self.kind == "do":
            return f"do({self.statements[0].fingerprint()},{self.expressions[0].fingerprint()})"
        if self.kind == "for":
            if self.declaration is not None:
                initializer = f"decl({self.declaration.fingerprint()})"
            elif self.expressions and self.expressions[0].kind == "for_init_marker":
                initializer = "empty"
            else:
                initializer = f"expr({self.expressions[0].fingerprint()})"
            expression_offset = 1
            if self.expressions and self.expressions[0].kind == "for_init_marker":
                expression_offset = 1
            condition = self.expressions[expression_offset] if len(self.expressions) > expression_offset else None
            step = self.expressions[expression_offset + 1] if len(self.expressions) > expression_offset + 1 else None
            return (
                f"for({initializer},{'none' if condition is None or condition.kind == 'none_marker' else condition.fingerprint()},"
                f"{'none' if step is None or step.kind == 'none_marker' else step.fingerprint()},{self.statements[0].fingerprint()})"
            )
        if self.kind == "return":
            value = "" if not self.expressions else self.expressions[0].fingerprint()
            return f"return({value})"
        if self.kind == "compound":
            values = [
                f"decl({item.fingerprint()})" if isinstance(item, ModelLocalDecl) else item.fingerprint()
                for item in self.items
            ]
            return f"compound[{','.join(values)}]"
        if self.kind in {"break", "continue", "null"}:
            return self.kind
        if self.kind == "goto":
            return f"goto({self.name})"
        if self.kind == "label":
            return f"label({self.name},{self.statements[0].fingerprint()})"
        if self.kind == "case":
            return f"case({self.expressions[0].fingerprint()},{self.statements[0].fingerprint()})"
        if self.kind == "default":
            return f"default({self.statements[0].fingerprint()})"
        raise AssertionError(f"unhandled model statement {self.kind}")

    def spans(self, path: str) -> list[SpanFact]:
        facts = [make_span(path, self.start, self.end)]
        if self.kind == "expr":
            facts.extend(self.expressions[0].spans(f"{path}.expression"))
        elif self.kind == "if":
            facts.extend(self.expressions[0].spans(f"{path}.condition"))
            facts.extend(self.statements[0].spans(f"{path}.then"))
            if len(self.statements) == 2:
                facts.extend(self.statements[1].spans(f"{path}.else"))
        elif self.kind == "switch":
            facts.extend(self.expressions[0].spans(f"{path}.expression"))
            facts.extend(self.statements[0].spans(f"{path}.body"))
        elif self.kind == "while":
            facts.extend(self.expressions[0].spans(f"{path}.condition"))
            facts.extend(self.statements[0].spans(f"{path}.body"))
        elif self.kind == "do":
            facts.extend(self.statements[0].spans(f"{path}.body"))
            facts.extend(self.expressions[0].spans(f"{path}.condition"))
        elif self.kind == "for":
            if self.declaration is not None:
                facts.extend(self.declaration.spans(f"{path}.initializer.declaration"))
            elif self.expressions[0].kind != "for_init_marker":
                facts.extend(self.expressions[0].spans(f"{path}.initializer.expression"))
            expression_offset = 1
            if len(self.expressions) > expression_offset and self.expressions[expression_offset].kind != "none_marker":
                facts.extend(self.expressions[expression_offset].spans(f"{path}.condition"))
            if len(self.expressions) > expression_offset + 1 and self.expressions[expression_offset + 1].kind != "none_marker":
                facts.extend(self.expressions[expression_offset + 1].spans(f"{path}.step"))
            facts.extend(self.statements[0].spans(f"{path}.body"))
        elif self.kind == "return" and self.expressions:
            facts.extend(self.expressions[0].spans(f"{path}.value"))
        elif self.kind == "compound":
            for index, item in enumerate(self.items):
                if isinstance(item, ModelLocalDecl):
                    facts.extend(item.spans(f"{path}.item[{index}].declaration"))
                else:
                    facts.extend(item.spans(f"{path}.item[{index}].statement"))
        elif self.kind in {"goto", "label"}:
            assert self.name_start is not None and self.name_end is not None
            facts.append(make_span(f"{path}.label", self.name_start, self.name_end))
            if self.kind == "label":
                facts.extend(self.statements[0].spans(f"{path}.statement"))
        elif self.kind == "case":
            facts.extend(self.expressions[0].spans(f"{path}.value"))
            facts.extend(self.statements[0].spans(f"{path}.statement"))
        elif self.kind == "default":
            facts.extend(self.statements[0].spans(f"{path}.statement"))
        return facts


class StatementModelParser:
    BINARY_PRECEDENCE = {
        "||": (1, "or"),
        "&&": (2, "and"),
        "|": (3, "bit_or"),
        "^": (4, "bit_xor"),
        "&": (5, "bit_and"),
        "==": (6, "eq"),
        "!=": (6, "ne"),
        "<": (7, "lt"),
        "<=": (7, "le"),
        ">": (7, "gt"),
        ">=": (7, "ge"),
        "<<": (8, "shl"),
        ">>": (8, "shr"),
        "+": (9, "add"),
        "-": (9, "sub"),
        "*": (10, "mul"),
        "/": (10, "div"),
        "%": (10, "mod"),
    }
    PREFIX = {
        "-": "neg",
        "+": "plus",
        "!": "not",
        "~": "bit_not",
        "&": "addr_of",
        "*": "deref",
        "++": "pre_inc",
        "--": "pre_dec",
    }

    def __init__(self, source: str, offset: int):
        self.source = source
        self.tokens = [
            ModelToken(match.group(0), offset + match.start(), offset + match.end())
            for match in C_TOKEN_RE.finditer(source)
            if not match.group(0).isspace()
        ]
        self.position = 0

    def peek(self, distance: int = 0) -> Optional[ModelToken]:
        index = self.position + distance
        return self.tokens[index] if index < len(self.tokens) else None

    def take(self, expected: Optional[str] = None) -> ModelToken:
        token = self.peek()
        if token is None:
            raise AssertionError(f"model parser reached EOF, expected {expected}")
        if expected is not None and token.text != expected:
            raise AssertionError(f"model parser expected {expected!r}, got {token.text!r}")
        self.position += 1
        return token

    def parse(self) -> ModelStmt:
        statement = self.parse_statement()
        if self.peek() is not None:
            raise AssertionError(f"model statement has trailing token {self.peek()}")
        return statement

    def parse_statement(self) -> ModelStmt:
        token = self.peek()
        if token is None:
            raise AssertionError("expected statement")
        if token.text == "{":
            return self.parse_compound()
        if token.text == ";":
            semi = self.take()
            return ModelStmt("null", semi.start, semi.end)
        if token.text == "if":
            start = self.take().start
            self.take("(")
            condition = self.parse_expression()
            self.take(")")
            then_branch = self.parse_statement()
            statements = [then_branch]
            end = then_branch.end
            if self.peek() is not None and self.peek().text == "else":
                self.take()
                else_branch = self.parse_statement()
                statements.append(else_branch)
                end = else_branch.end
            return ModelStmt("if", start, end, (condition,), tuple(statements))
        if token.text in {"switch", "while"}:
            keyword = self.take()
            self.take("(")
            expression = self.parse_expression()
            self.take(")")
            body = self.parse_statement()
            return ModelStmt(keyword.text, keyword.start, body.end, (expression,), (body,))
        if token.text == "do":
            start = self.take().start
            body = self.parse_statement()
            self.take("while")
            self.take("(")
            condition = self.parse_expression()
            self.take(")")
            semi = self.take(";")
            return ModelStmt("do", start, semi.end, (condition,), (body,))
        if token.text == "for":
            start = self.take().start
            self.take("(")
            declaration = None
            expressions: list[ModelExpr] = []
            if self.peek().text == ";":
                marker = self.take()
                expressions.append(ModelExpr("for_init_marker", marker.start, marker.start))
            elif self.peek().text == "int":
                declaration = self.parse_local_declaration()
                expressions.append(ModelExpr("for_init_marker", declaration.end, declaration.end))
            else:
                expressions.append(self.parse_expression())
                self.take(";")
            if self.peek().text == ";":
                marker = self.take()
                expressions.append(ModelExpr("none_marker", marker.start, marker.start))
            else:
                expressions.append(self.parse_expression())
                self.take(";")
            if self.peek().text == ")":
                marker = self.peek()
                expressions.append(ModelExpr("none_marker", marker.start, marker.start))
            else:
                expressions.append(self.parse_expression())
            self.take(")")
            body = self.parse_statement()
            return ModelStmt("for", start, body.end, tuple(expressions), (body,), declaration=declaration)
        if token.text == "return":
            start = self.take().start
            if self.peek().text == ";":
                semi = self.take()
                return ModelStmt("return", start, semi.end)
            expression = self.parse_expression()
            semi = self.take(";")
            return ModelStmt("return", start, semi.end, (expression,))
        if token.text in {"break", "continue"}:
            keyword = self.take()
            semi = self.take(";")
            return ModelStmt(keyword.text, keyword.start, semi.end)
        if token.text == "goto":
            start = self.take().start
            name = self.take()
            semi = self.take(";")
            return ModelStmt("goto", start, semi.end, name=name.text, name_start=name.start, name_end=name.end)
        if token.text == "case":
            start = self.take().start
            value = self.parse_assignment()
            self.take(":")
            nested = self.parse_statement()
            return ModelStmt("case", start, nested.end, (value,), (nested,))
        if token.text == "default":
            start = self.take().start
            self.take(":")
            nested = self.parse_statement()
            return ModelStmt("default", start, nested.end, statements=(nested,))
        if self.peek(1) is not None and self.peek(1).text == ":":
            name = self.take()
            self.take(":")
            nested = self.parse_statement()
            return ModelStmt(
                "label",
                name.start,
                nested.end,
                statements=(nested,),
                name=name.text,
                name_start=name.start,
                name_end=name.end,
            )
        expression = self.parse_expression()
        semi = self.take(";")
        return ModelStmt("expr", expression.start, semi.end, (expression,))

    def parse_compound(self) -> ModelStmt:
        open_brace = self.take("{")
        items: list[Union[ModelStmt, ModelLocalDecl]] = []
        while self.peek() is not None and self.peek().text != "}":
            if self.peek().text == "int":
                items.append(self.parse_local_declaration())
            else:
                items.append(self.parse_statement())
        close_brace = self.take("}")
        return ModelStmt("compound", open_brace.start, close_brace.end, items=tuple(items))

    def parse_local_declaration(self) -> ModelLocalDecl:
        specifier = self.take("int")
        name = self.take()
        initializer = None
        if self.peek().text == "=":
            self.take()
            initializer = self.parse_assignment()
        semi = self.take(";")
        return ModelLocalDecl(
            specifier.start,
            semi.end,
            specifier.start,
            specifier.end,
            name.text,
            name.start,
            name.end,
            initializer,
        )

    def parse_expression(self) -> ModelExpr:
        expression = self.parse_assignment()
        while self.peek() is not None and self.peek().text == ",":
            self.take()
            right = self.parse_assignment()
            expression = ModelExpr("comma", expression.start, right.end, children=(expression, right))
        return expression

    def parse_assignment(self) -> ModelExpr:
        left = self.parse_conditional()
        if self.peek() is not None and self.peek().text == "=":
            self.take()
            right = self.parse_assignment()
            return ModelExpr("assign", left.start, right.end, children=(left, right))
        return left

    def parse_conditional(self) -> ModelExpr:
        condition = self.parse_binary(1)
        if self.peek() is None or self.peek().text != "?":
            return condition
        self.take()
        when_true = self.parse_expression()
        self.take(":")
        when_false = self.parse_conditional()
        return ModelExpr("ternary", condition.start, when_false.end, children=(condition, when_true, when_false))

    def parse_binary(self, minimum: int) -> ModelExpr:
        left = self.parse_unary()
        while self.peek() is not None and self.peek().text in self.BINARY_PRECEDENCE:
            precedence, kind = self.BINARY_PRECEDENCE[self.peek().text]
            if precedence < minimum:
                break
            self.take()
            right = self.parse_binary(precedence + 1)
            left = ModelExpr(kind, left.start, right.end, children=(left, right))
        return left

    def parse_unary(self) -> ModelExpr:
        token = self.peek()
        if token.text in self.PREFIX:
            operator = self.take()
            operand = self.parse_unary()
            return ModelExpr(self.PREFIX[operator.text], operator.start, operand.end, children=(operand,))
        if token.text == "sizeof":
            keyword = self.take()
            if self.peek() is not None and self.peek().text == "(" and self.peek(1).text == "int":
                self.take("(")
                self.take("int")
                close = self.take(")")
                return ModelExpr("sizeof", keyword.start, close.end, value="int")
            operand = self.parse_unary()
            return ModelExpr("sizeof_expr", keyword.start, operand.end, children=(operand,))
        if token.text == "_Alignof":
            keyword = self.take()
            self.take("(")
            type_name = self.take("int")
            close = self.take(")")
            return ModelExpr("alignof", keyword.start, close.end, value=type_name.text)
        return self.parse_postfix()

    def parse_postfix(self) -> ModelExpr:
        expression = self.parse_primary()
        while self.peek() is not None:
            if self.peek().text == "(":
                self.take()
                arguments = []
                if self.peek().text != ")":
                    while True:
                        arguments.append(self.parse_assignment())
                        if self.peek().text != ",":
                            break
                        self.take()
                close = self.take(")")
                expression = ModelExpr("call", expression.start, close.end, children=(expression, *arguments))
            elif self.peek().text == "[":
                self.take()
                index = self.parse_expression()
                close = self.take("]")
                expression = ModelExpr("index", expression.start, close.end, children=(expression, index))
            elif self.peek().text in {".", "->"}:
                operator = self.take()
                member = self.take()
                kind = "member" if operator.text == "." else "ptr_member"
                expression = ModelExpr(kind, expression.start, member.end, value=member.text, children=(expression,))
            elif self.peek().text in {"++", "--"}:
                operator = self.take()
                kind = "post_inc" if operator.text == "++" else "post_dec"
                expression = ModelExpr(kind, expression.start, operator.end, children=(expression,))
            else:
                break
        return expression

    def parse_primary(self) -> ModelExpr:
        token = self.take()
        if token.text.isdigit():
            return ModelExpr("int", token.start, token.end, value=str(int(token.text, 10)))
        if re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", token.text):
            return ModelExpr("id", token.start, token.end, value=token.text)
        if token.text == "(":
            expression = self.parse_expression()
            close = self.take(")")
            return dataclasses.replace(expression, start=token.start, end=close.end)
        raise AssertionError(f"unexpected primary token {token.text!r}")


# ---------------------------------------------------------------------------
# Independent declaration/source-fact model
# ---------------------------------------------------------------------------


def _find_between(source: str, fragment: str, start: int, end: int, occurrence: int = 0) -> tuple[int, int]:
    cursor = start
    found = -1
    for _ in range(occurrence + 1):
        found = source.find(fragment, cursor, end)
        if found < 0:
            raise AssertionError(
                f"declaration model cannot find {fragment!r} occurrence {occurrence} in {source[start:end]!r}"
            )
        cursor = found + 1
    return found, found + len(fragment)


def _matching_brace_end(source: str, start: int, end: int) -> int:
    open_brace = source.find("{", start, end)
    if open_brace < 0:
        raise AssertionError("tag definition lacks opening brace")
    depth = 0
    for index in range(open_brace, end):
        if source[index] == "{":
            depth += 1
        elif source[index] == "}":
            depth -= 1
            if depth == 0:
                return index + 1
    raise AssertionError("tag definition lacks closing brace")


def _record_tag_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
    kind: str,
) -> list[SpanFact]:
    facts = [make_span(path, declaration_start, declaration_end)]
    specifier_end = _matching_brace_end(source, declaration_start, declaration_end)
    facts.append(make_span(f"{path}.specifier", declaration_start, specifier_end))
    header = re.match(
        rf"{kind}(?:\s+([A-Za-z_][A-Za-z_0-9]*))?\s*\{{",
        source[declaration_start:specifier_end],
    )
    if header is None:
        raise AssertionError(f"cannot model {kind} header")
    if header.group(1) is not None:
        relative_start, relative_end = header.span(1)
        facts.append(
            make_span(
                f"{path}.name",
                declaration_start + relative_start,
                declaration_start + relative_end,
            )
        )
    open_brace = source.find("{", declaration_start, specifier_end)
    member_start = open_brace + 1
    member_index = 0
    while member_start < specifier_end - 1:
        while member_start < specifier_end - 1 and source[member_start].isspace():
            member_start += 1
        if member_start >= specifier_end - 1:
            break
        semicolon = source.find(";", member_start, specifier_end)
        if semicolon < 0:
            raise AssertionError("record member lacks semicolon")
        member_end = semicolon + 1
        member_text = source[member_start:member_end]
        match = re.fullmatch(
            r"(?P<specifier>int|long|unsigned|struct\s+[A-Za-z_][A-Za-z_0-9]*)"
            r"\s+(?:(?P<pointer>\*)\s*)?(?P<name>[A-Za-z_][A-Za-z_0-9]*)?"
            r"(?:\s*:\s*(?P<bits>[0-9]+))?;",
            member_text,
        )
        if match is None:
            raise AssertionError(f"cannot model record member {member_text!r}")
        member_path = f"{path}.member[{member_index}]"
        facts.append(make_span(member_path, member_start, member_end))
        specifier_start, specifier_finish = match.span("specifier")
        specifier_start += member_start
        specifier_finish += member_start
        facts.append(make_span(f"{member_path}.specifier", specifier_start, specifier_finish))
        specifier_name = re.search(r"struct\s+([A-Za-z_][A-Za-z_0-9]*)", match.group("specifier"))
        if specifier_name is not None:
            name_start = specifier_start + specifier_name.start(1)
            name_end = specifier_start + specifier_name.end(1)
            facts.append(make_span(f"{member_path}.specifier.name", name_start, name_end))
        if match.group("name") is not None:
            name_start, name_end = match.span("name")
            name_start += member_start
            name_end += member_start
            facts.append(make_span(f"{member_path}.name", name_start, name_end))
            written_start = name_start
            if match.group("pointer") is not None:
                pointer_start, pointer_end = match.span("pointer")
                pointer_start += member_start
                pointer_end += member_start
                written_start = pointer_start
                facts.append(make_span(f"{member_path}.written.layer[0]", pointer_start, pointer_end))
            facts.append(make_span(f"{member_path}.written", written_start, name_end))
        else:
            # RecordMember::written falls back to the type specifier for an
            # unnamed bit-field.
            facts.append(make_span(f"{member_path}.written", specifier_start, specifier_finish))
        if match.group("bits") is not None:
            bits_start, bits_end = match.span("bits")
            facts.append(
                make_span(
                    f"{member_path}.bit_width",
                    member_start + bits_start,
                    member_start + bits_end,
                )
            )
        member_index += 1
        member_start = member_end
    return facts


def _enum_tag_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
) -> list[SpanFact]:
    facts = [make_span(path, declaration_start, declaration_end)]
    specifier_end = _matching_brace_end(source, declaration_start, declaration_end)
    facts.append(make_span(f"{path}.specifier", declaration_start, specifier_end))
    header = re.match(
        r"enum(?:\s+([A-Za-z_][A-Za-z_0-9]*))?\s*\{",
        source[declaration_start:specifier_end],
    )
    if header is None:
        raise AssertionError("cannot model enum header")
    if header.group(1) is not None:
        start, end = header.span(1)
        facts.append(make_span(f"{path}.name", declaration_start + start, declaration_start + end))
    open_brace = source.find("{", declaration_start, specifier_end)
    body = source[open_brace + 1 : specifier_end - 1]
    cursor = open_brace + 1
    for index, raw_item in enumerate(body.split(",")):
        stripped = raw_item.strip()
        item_start = source.find(stripped, cursor, specifier_end)
        item_end = item_start + len(stripped)
        cursor = item_end
        match = re.fullmatch(
            r"(?P<name>[A-Za-z_][A-Za-z_0-9]*)(?:\s*=\s*(?P<value>[0-9]+))?",
            stripped,
        )
        if match is None:
            raise AssertionError(f"cannot model enumerator {stripped!r}")
        item_path = f"{path}.enumerator[{index}]"
        facts.append(make_span(item_path, item_start, item_end))
        name_start, name_end = match.span("name")
        facts.append(make_span(f"{item_path}.name", item_start + name_start, item_start + name_end))
        if match.group("value") is not None:
            value_start, value_end = match.span("value")
            facts.append(
                make_span(
                    f"{item_path}.written_value",
                    item_start + value_start,
                    item_start + value_end,
                )
            )
    return facts


def _forward_tag_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
    kind: str,
) -> list[SpanFact]:
    match = re.fullmatch(
        rf"{kind}\s+([A-Za-z_][A-Za-z_0-9]*);",
        source[declaration_start:declaration_end],
    )
    if match is None:
        raise AssertionError("cannot model forward tag")
    name_start, name_end = match.span(1)
    absolute_name_start = declaration_start + name_start
    absolute_name_end = declaration_start + name_end
    return [
        make_span(path, declaration_start, declaration_end),
        make_span(f"{path}.name", absolute_name_start, absolute_name_end),
        make_span(f"{path}.specifier", declaration_start, absolute_name_end),
    ]


def _tag_specifier_name_fact(source: str, path: str, start: int, end: int) -> list[SpanFact]:
    match = re.fullmatch(r"(?:struct|union|enum)\s+([A-Za-z_][A-Za-z_0-9]*)", source[start:end])
    if match is None:
        return []
    relative_start, relative_end = match.span(1)
    return [make_span(f"{path}.name", start + relative_start, start + relative_end)]


def _object_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
    specifier_fragment: str,
    name: str,
    written_fragment: str,
    layer_fragments: Sequence[str] = (),
) -> list[SpanFact]:
    specifier_start, specifier_end = _find_between(
        source, specifier_fragment, declaration_start, declaration_end
    )
    written_start, written_end = _find_between(
        source, written_fragment, specifier_end, declaration_end
    )
    name_start, name_end = _find_between(source, name, written_start, written_end)
    facts = [
        make_span(path, declaration_start, declaration_end),
        make_span(f"{path}.name", name_start, name_end),
        make_span(f"{path}.specifier", specifier_start, specifier_end),
        make_span(f"{path}.written", written_start, written_end),
    ]
    facts.extend(_tag_specifier_name_fact(source, f"{path}.specifier", specifier_start, specifier_end))
    layer_cursor = written_start
    for index, fragment in enumerate(layer_fragments):
        layer_start, layer_end = _find_between(source, fragment, layer_cursor, written_end)
        facts.append(make_span(f"{path}.written.layer[{index}]", layer_start, layer_end))
        # Pointer and function layers may overlap declarator nesting, so do not
        # force subsequent searches to begin after the previous layer.
        layer_cursor = written_start
    return facts


def _function_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
    return_specifier: str,
    name: str,
    written_fragment: str,
    function_layer: str,
    parameters: Sequence[tuple[str, str, Optional[str], str, Sequence[str]]],
    extra_layers: Sequence[str] = (),
    body: Optional[ModelStmt] = None,
) -> list[SpanFact]:
    specifier_start, specifier_end = _find_between(
        source, return_specifier, declaration_start, declaration_end
    )
    written_start, written_end = _find_between(
        source, written_fragment, specifier_end, declaration_end
    )
    name_start, name_end = _find_between(source, name, written_start, written_end)
    facts = [
        make_span(path, declaration_start, declaration_end),
        make_span(f"{path}.name", name_start, name_end),
        make_span(f"{path}.return_specifier", specifier_start, specifier_end),
        make_span(f"{path}.written", written_start, written_end),
    ]
    layer_fragments = [function_layer, *extra_layers]
    # WrittenDeclarator layers are identifier-outward.  A leading pointer is
    # appended after suffix layers for ordinary `*f(...)`, but precedes the
    # function suffix for the parenthesized `(*p)(...)` object case (handled by
    # _object_paths).
    for index, fragment in enumerate(layer_fragments):
        layer_start, layer_end = _find_between(source, fragment, written_start, written_end)
        facts.append(make_span(f"{path}.written.layer[{index}]", layer_start, layer_end))
    for index, (parameter_fragment, parameter_specifier, parameter_name, parameter_written, layers) in enumerate(parameters):
        parameter_start, parameter_end = _find_between(
            source, parameter_fragment, written_start, written_end
        )
        parameter_path = f"{path}.parameter[{index}]"
        parameter_specifier_start, parameter_specifier_end = _find_between(
            source, parameter_specifier, parameter_start, parameter_end
        )
        parameter_written_start, parameter_written_end = _find_between(
            source, parameter_written, parameter_start, parameter_end
        )
        facts.extend(
            [
                make_span(parameter_path, parameter_start, parameter_end),
                make_span(
                    f"{parameter_path}.specifier",
                    parameter_specifier_start,
                    parameter_specifier_end,
                ),
                make_span(
                    f"{parameter_path}.written",
                    parameter_written_start,
                    parameter_written_end,
                ),
            ]
        )
        facts.extend(
            _tag_specifier_name_fact(
                source,
                f"{parameter_path}.specifier",
                parameter_specifier_start,
                parameter_specifier_end,
            )
        )
        if parameter_name is not None:
            parameter_name_start, parameter_name_end = _find_between(
                source, parameter_name, parameter_written_start, parameter_written_end
            )
            facts.append(make_span(f"{parameter_path}.name", parameter_name_start, parameter_name_end))
        for layer_index, layer in enumerate(layers):
            layer_start, layer_end = _find_between(
                source, layer, parameter_written_start, parameter_written_end
            )
            facts.append(
                make_span(
                    f"{parameter_path}.written.layer[{layer_index}]",
                    layer_start,
                    layer_end,
                )
            )
    if body is not None:
        facts.extend(body.spans(f"{path}.body"))
    return facts


def _typedef_paths(
    source: str,
    path: str,
    declaration_start: int,
    declaration_end: int,
    name: str,
) -> list[SpanFact]:
    specifier_start, specifier_end = _find_between(source, "int", declaration_start, declaration_end)
    name_start, name_end = _find_between(source, name, specifier_end, declaration_end)
    return [
        make_span(path, declaration_start, declaration_end),
        make_span(f"{path}.name", name_start, name_end),
        make_span(f"{path}.specifier", specifier_start, specifier_end),
        make_span(f"{path}.written", name_start, name_end),
    ]


def declaration_path_model(
    declaration_index: int,
    declaration: DeclModel,
    source: str,
) -> tuple[list[SpanFact], int]:
    part_ranges = []
    cursor = 0
    for part in declaration.parts:
        start = source.find(part, cursor)
        if start < 0:
            raise AssertionError(f"missing declaration part {part!r}")
        part_ranges.append((start, start + len(part)))
        cursor = start + len(part)
    facts: list[SpanFact] = []
    event = 0

    def path() -> str:
        return f"decl[{event}]"

    def add_record(part_index: int, kind: str, same_part_object: Optional[tuple[str, str]] = None) -> None:
        nonlocal event
        start, end = part_ranges[part_index]
        facts.extend(_record_tag_paths(source, path(), start, end, kind))
        event += 1
        if same_part_object is not None:
            name, specifier = same_part_object
            close = _matching_brace_end(source, start, end)
            written_start, written_end = _find_between(source, name, close, end)
            facts.extend(
                _object_paths(
                    source,
                    path(),
                    start,
                    end,
                    specifier,
                    name,
                    source[written_start:written_end],
                )
            )
            event += 1

    def add_enum(part_index: int, same_part_object: Optional[tuple[str, str]] = None) -> None:
        nonlocal event
        start, end = part_ranges[part_index]
        facts.extend(_enum_tag_paths(source, path(), start, end))
        event += 1
        if same_part_object is not None:
            name, specifier = same_part_object
            facts.extend(_object_paths(source, path(), start, end, specifier, name, name))
            event += 1

    # The axis index is the contract.  Each branch models source facts only;
    # semantic fingerprints remain independently authored in DeclModel.
    if declaration_index in {0, 4, 5, 19}:
        add_record(0, "union" if declaration_index == 5 else "struct")
    elif declaration_index == 1:
        start, end = part_ranges[0]
        facts.extend(_forward_tag_paths(source, path(), start, end, "struct"))
        event += 1
        part = declaration.parts[1]
        match = re.fullmatch(r"struct\s+(\w+)\s+\*(\w+);", part)
        assert match is not None
        start, end = part_ranges[1]
        facts.extend(
            _object_paths(
                source,
                path(),
                start,
                end,
                f"struct {match.group(1)}",
                match.group(2),
                f"*{match.group(2)}",
                ("*",),
            )
        )
        event += 1
    elif declaration_index == 2:
        part = declaration.parts[0]
        name = re.search(r"}\s+(\w+);", part).group(1)
        specifier = part[: part.rfind("}") + 1]
        add_record(0, "struct", (name, specifier))
    elif declaration_index == 3:
        add_record(0, "struct")
        add_record(1, "struct")
    elif declaration_index == 6:
        add_enum(0)
    elif declaration_index == 7:
        start, end = part_ranges[0]
        facts.extend(_forward_tag_paths(source, path(), start, end, "struct"))
        event += 1
        part = declaration.parts[1]
        match = re.fullmatch(r"int\s+(\w+)\(struct\s+(\w+)\s+\*(\w+)\);", part)
        assert match is not None
        fn, tag, argument = match.groups()
        start, end = part_ranges[1]
        facts.extend(
            _function_paths(
                source,
                path(),
                start,
                end,
                "int",
                fn,
                f"{fn}(struct {tag} *{argument})",
                f"(struct {tag} *{argument})",
                ((f"struct {tag} *{argument}", f"struct {tag}", argument, f"*{argument}", ("*",)),),
            )
        )
        event += 1
    elif declaration_index in {8, 9, 10, 11, 12, 23, 24}:
        part = declaration.parts[0]
        start, end = part_ranges[0]
        fn = re.search(r"(?:\*\s*)?([A-Za-z_][A-Za-z_0-9]*)\(", part).group(1)
        open_paren = part.find("(", part.find(fn) + len(fn))
        close_paren = part.rfind(")")
        arguments_text = part[open_paren + 1 : close_paren]
        parameters: list[tuple[str, str, Optional[str], str, Sequence[str]]] = []
        if declaration_index == 10:
            match = re.fullmatch(r"int\s+(\w+)", arguments_text)
            assert match is not None
            parameters.append((arguments_text, "int", match.group(1), match.group(1), ()))
        elif declaration_index == 11:
            # An unnamed parameter's WrittenDeclarator falls back to its
            # specifier span.
            parameters.append(("int", "int", None, "int", ()))
        elif declaration_index == 12:
            first = arguments_text.split(",", 1)[0]
            match = re.fullmatch(r"int\s+(\w+)", first)
            assert match is not None
            parameters.append((first, "int", match.group(1), match.group(1), ()))
        elif declaration_index == 24:
            match = re.fullmatch(r"int\s+(\w+)\[([0-9]+)\]", arguments_text)
            assert match is not None
            argument, bound = match.groups()
            parameters.append(
                (
                    arguments_text,
                    "int",
                    argument,
                    f"{argument}[{bound}]",
                    (f"[{bound}]",),
                )
            )
        pointer_prefix = "*" if declaration_index == 23 else ""
        written = f"{pointer_prefix}{fn}({arguments_text})"
        extra_layers = ("*",) if declaration_index == 23 else ()
        facts.extend(
            _function_paths(
                source,
                path(),
                start,
                end,
                "int",
                fn,
                written,
                f"({arguments_text})",
                tuple(parameters),
                extra_layers,
            )
        )
        event += 1
    elif declaration_index in {13, 14}:
        part = declaration.parts[0]
        match = re.fullmatch(r"(?:static|extern)\s+(int|long)\s+(\w+);", part)
        assert match is not None
        specifier, name = match.groups()
        start, end = part_ranges[0]
        facts.extend(_object_paths(source, path(), start, end, specifier, name, name))
        event += 1
    elif declaration_index == 15:
        part = declaration.parts[0]
        match = re.fullmatch(r"const int\s+\*(\w+);", part)
        assert match is not None
        name = match.group(1)
        start, end = part_ranges[0]
        facts.extend(
            _object_paths(source, path(), start, end, "const int", name, f"*{name}", ("*",))
        )
        event += 1
    elif declaration_index == 16:
        part = declaration.parts[0]
        match = re.fullmatch(r"int\s+(\w+)\[([0-9]+)\];", part)
        assert match is not None
        name, bound = match.groups()
        start, end = part_ranges[0]
        facts.extend(
            _object_paths(
                source,
                path(),
                start,
                end,
                "int",
                name,
                f"{name}[{bound}]",
                (f"[{bound}]",),
            )
        )
        event += 1
    elif declaration_index == 17:
        part = declaration.parts[0]
        match = re.fullmatch(r"int\s+\(\*(\w+)\)\(int\s+(\w+)\);", part)
        assert match is not None
        name, argument = match.groups()
        start, end = part_ranges[0]
        facts.extend(
            _object_paths(
                source,
                path(),
                start,
                end,
                "int",
                name,
                f"(*{name})(int {argument})",
                ("*", f"(int {argument})"),
            )
        )
        event += 1
    elif declaration_index == 18:
        name = re.fullmatch(r"typedef int\s+(\w+);", declaration.parts[0]).group(1)
        start, end = part_ranges[0]
        facts.extend(_typedef_paths(source, path(), start, end, name))
        event += 1
    elif declaration_index == 20:
        part = declaration.parts[0]
        name = re.search(r"}\s+(\w+);", part).group(1)
        specifier = part[: part.rfind("}") + 1]
        add_record(0, "union", (name, specifier))
    elif declaration_index == 21:
        part = declaration.parts[0]
        name = re.search(r"}\s+(\w+);", part).group(1)
        specifier = part[: part.rfind("}") + 1]
        add_enum(0, (name, specifier))
    elif declaration_index == 22:
        start, end = part_ranges[0]
        facts.extend(_forward_tag_paths(source, path(), start, end, "struct"))
        event += 1
        add_record(1, "struct")
    else:
        raise AssertionError(f"unmodelled declaration axis {declaration_index}")
    return facts, event


def valid_cases(seed: str = DEFAULT_SEED) -> list[ValidCase]:
    rows: list[ValidCase] = []
    # Models are regenerated per coordinate so every case's lexical payload is
    # seed-derived without letting the seed choose an axis.
    for declaration_index in range(DECLARATION_AXIS_COUNT):
        for statement_index in range(STATEMENT_AXIS_COUNT):
            declaration = declaration_models(seed, declaration_index)[declaration_index]
            statement = statement_models(seed, statement_index)[statement_index]
            function_name = generated_identifier(seed, "function", declaration_index, statement_index)
            parameter_name = generated_identifier(seed, "parameter", declaration_index, statement_index)
            prefix = " ".join(declaration.parts)
            function = f"int {function_name}(int {parameter_name}) {statement.body}"
            source = f"{prefix} {function}\n"
            function_start = len(prefix) + 1
            body_start = function_start + function.index(statement.body)
            facts, next_declaration = declaration_path_model(
                declaration_index, declaration, source
            )
            modeled_body = StatementModelParser(statement.body, body_start).parse()
            if modeled_body.fingerprint() != statement.fingerprint:
                raise AssertionError(
                    f"statement oracle disagreement for {statement.key}: "
                    f"authored={statement.fingerprint} parsed={modeled_body.fingerprint()}"
                )
            function_path = f"decl[{next_declaration}]"
            facts.extend(
                _function_paths(
                    source,
                    function_path,
                    function_start,
                    function_start + len(function),
                    "int",
                    function_name,
                    f"{function_name}(int {parameter_name})",
                    f"(int {parameter_name})",
                    ((f"int {parameter_name}", "int", parameter_name, parameter_name, ()),),
                    body=modeled_body,
                )
            )

            fingerprint = (
                f"unit[{declaration.fingerprint};"
                f"function({function_name},[{parameter_name}:int],int,prototype,variadic:false,"
                f"body={statement.fingerprint})]"
            )
            rows.append(
                ValidCase(
                    case_id=f"valid.d{declaration_index:02d}.s{statement_index:02d}",
                    declaration_axis=declaration.key,
                    statement_axis=statement.key,
                    source=source,
                    expected_fingerprint=fingerprint,
                    span_facts=tuple(facts),
                )
            )
    return rows


MalformedBuilder = Callable[[str, int], tuple[str, str, str, str, int]]


def _at(source: str, fragment: str, occurrence: int = 0) -> tuple[int, int]:
    return nth_span(source, fragment, occurrence)


def _at_eof(source: str) -> tuple[int, int]:
    return len(source), len(source)


def malformed_families() -> tuple[tuple[str, MalformedBuilder], ...]:
    def missing_external_semicolon(seed: str, variant: int):
        name = generated_identifier(seed, "bad_object", variant)
        source = f"int {name}"
        start, end = _at_eof(source)
        return source, "E1002", "expected-token: ;", "eof", start

    def pointer_missing_name(seed: str, variant: int):
        tag = generated_identifier(seed, "missing_pointer_name", variant)
        qualifier = "const " if variant % 2 else ""
        source = f"struct {tag} *{qualifier};"
        start, _ = _at(source, ";")
        return source, "E1001", "expected-declaration", ";", start

    def array_missing_rbracket(seed: str, variant: int):
        name = generated_identifier(seed, "bad_array", variant)
        value = generated_literal(seed, "bad_bound", variant)
        source = f"int {name}[{value};"
        start, _ = _at(source, ";")
        return source, "E1002", "expected-token: ]", ";", start

    def prototype_missing_rparen(seed: str, variant: int):
        fn = generated_identifier(seed, "bad_proto", variant)
        arg = generated_identifier(seed, "bad_arg", variant)
        source = f"int {fn}(int {arg};"
        start, _ = _at(source, ";")
        return source, "E1002", "expected-token: )", ";", start

    def duplicate_tag(seed: str, variant: int):
        tag = generated_identifier(seed, "duplicate_tag", variant)
        first = generated_identifier(seed, "first_field", variant)
        second = generated_identifier(seed, "second_field", variant)
        source = f"struct {tag} {{ int {first}; }}; struct {tag} {{ long {second}; }};"
        start, _ = _at(source, tag, 1)
        return source, "E1101", f"duplicate-tag-definition: {tag}", tag, start

    def conflicting_tag_kind(seed: str, variant: int):
        tag = generated_identifier(seed, "conflict_tag", variant)
        source = f"struct {tag}; union {tag};"
        start, _ = _at(source, tag, 1)
        return source, "E1102", f"conflicting-tag-kind: {tag}", tag, start

    def tag_missing_rbrace(seed: str, variant: int):
        tag = generated_identifier(seed, "open_tag", variant)
        field = generated_identifier(seed, "open_field", variant)
        source = f"struct {tag} {{ int {field};"
        start, _ = _at_eof(source)
        return source, "E1002", "expected-token: }", "eof", start

    def member_missing_semicolon(seed: str, variant: int):
        tag = generated_identifier(seed, "member_tag", variant)
        field = generated_identifier(seed, "member_field", variant)
        source = f"struct {tag} {{ int {field} }};"
        start, _ = _at(source, "}")
        return source, "E1002", "expected-token: ;", "}", start

    def enum_missing_value(seed: str, variant: int):
        tag = generated_identifier(seed, "enum_tag", variant)
        item = generated_identifier(seed, "enum_item", variant)
        source = f"enum {tag} {{ {item} = }};"
        start, _ = _at(source, "}")
        return source, "E1003", "expected-expression", "}", start

    def body_missing_rbrace(seed: str, variant: int):
        fn = generated_identifier(seed, "open_function", variant)
        value = generated_literal(seed, "return_value", variant)
        source = f"int {fn}(void) {{ return {value};"
        start, _ = _at_eof(source)
        return source, "E1002", "expected-token: }", "eof", start

    def return_missing_semicolon(seed: str, variant: int):
        fn = generated_identifier(seed, "return_function", variant)
        value = generated_literal(seed, "return_value", variant)
        source = f"int {fn}(void) {{ return {value} }}"
        start, _ = _at(source, "}")
        return source, "E1002", "expected-token: ;", "}", start

    def if_missing_lparen(seed: str, variant: int):
        fn = generated_identifier(seed, "if_function", variant)
        value = generated_identifier(seed, "if_value", variant)
        source = f"int {fn}(int {value}) {{ if {value}) return {value}; }}"
        start, _ = _at(source, value, 1)
        return source, "E1002", "expected-token: (", value, start

    def if_missing_rparen(seed: str, variant: int):
        fn = generated_identifier(seed, "if_close_function", variant)
        value = generated_identifier(seed, "if_close_value", variant)
        source = f"int {fn}(int {value}) {{ if ({value} return {value}; }}"
        start, _ = _at(source, "return")
        return source, "E1002", "expected-token: )", "return", start

    def while_missing_rparen(seed: str, variant: int):
        fn = generated_identifier(seed, "while_function", variant)
        value = generated_identifier(seed, "while_value", variant)
        source = f"int {fn}(int {value}) {{ while ({value} {{ break; }} }}"
        start, _ = _at(source, "{", 1)
        return source, "E1002", "expected-token: )", "{", start

    def do_missing_while(seed: str, variant: int):
        fn = generated_identifier(seed, "do_function", variant)
        value = generated_identifier(seed, "do_value", variant)
        source = f"int {fn}(int {value}) {{ do {{ ; }} ({value}); }}"
        start, _ = _at(source, "(", 1)
        return source, "E1002", "expected-token: while", "(", start

    def for_missing_semicolon(seed: str, variant: int):
        fn = generated_identifier(seed, "for_function", variant)
        value = generated_identifier(seed, "for_value", variant)
        limit = generated_literal(seed, "for_limit", variant)
        source = f"int {fn}(int {value}) {{ for ({value} = 0 {value} < {limit}; {value}++) ; }}"
        start, _ = _at(source, value, 2)
        return source, "E1002", "expected-token: ;", value, start

    def switch_missing_lparen(seed: str, variant: int):
        fn = generated_identifier(seed, "switch_function", variant)
        value = generated_identifier(seed, "switch_value", variant)
        source = f"int {fn}(int {value}) {{ switch {value}) {{ default: break; }} }}"
        start, _ = _at(source, value, 1)
        return source, "E1002", "expected-token: (", value, start

    def goto_missing_label(seed: str, variant: int):
        fn = generated_identifier(seed, "goto_function", variant)
        source = f"int {fn}(void) {{ goto ; /* {generated_identifier(seed, 'goto_witness', variant)} */ }}"
        start, _ = _at(source, ";")
        return source, "E1002", "expected-token: identifier", ";", start

    def case_missing_colon(seed: str, variant: int):
        fn = generated_identifier(seed, "case_function", variant)
        value = generated_identifier(seed, "case_value", variant)
        label = generated_literal(seed, "case_label", variant)
        source = f"int {fn}(int {value}) {{ switch ({value}) {{ case {label} break; }} }}"
        start, _ = _at(source, "break")
        return source, "E1002", "expected-token: :", "break", start

    def label_missing_statement(seed: str, variant: int):
        fn = generated_identifier(seed, "label_function", variant)
        label = generated_identifier(seed, "bad_label", variant)
        source = f"int {fn}(void) {{ {label}: }}"
        start, _ = _at(source, "}")
        return source, "E1003", "expected-expression", "}", start

    return (
        ("declaration.missing_semicolon", missing_external_semicolon),
        ("declarator.pointer_missing_name", pointer_missing_name),
        ("declarator.array_missing_rbracket", array_missing_rbracket),
        ("declarator.prototype_missing_rparen", prototype_missing_rparen),
        ("tag.duplicate_definition", duplicate_tag),
        ("tag.conflicting_kind", conflicting_tag_kind),
        ("tag.missing_rbrace", tag_missing_rbrace),
        ("tag.member_missing_semicolon", member_missing_semicolon),
        ("enum.missing_value", enum_missing_value),
        ("compound.missing_rbrace", body_missing_rbrace),
        ("return.missing_semicolon", return_missing_semicolon),
        ("if.missing_lparen", if_missing_lparen),
        ("if.missing_rparen", if_missing_rparen),
        ("while.missing_rparen", while_missing_rparen),
        ("do.missing_while", do_missing_while),
        ("for.missing_semicolon", for_missing_semicolon),
        ("switch.missing_lparen", switch_missing_lparen),
        ("goto.missing_label", goto_missing_label),
        ("case.missing_colon", case_missing_colon),
        ("label.missing_statement", label_missing_statement),
    )


def malformed_cases(seed: str = DEFAULT_SEED) -> list[MalformedCase]:
    rows: list[MalformedCase] = []
    for family_index, (family, builder) in enumerate(malformed_families()):
        for variant in range(MALFORMED_VARIANTS):
            source, code, message, anchor, byte_start = builder(seed, variant)
            byte_end = byte_start if anchor == "eof" else byte_start + len(anchor)
            rows.append(
                MalformedCase(
                    case_id=f"malformed.e{family_index:02d}.v{variant:02d}",
                    family=family,
                    variant=variant,
                    source=source,
                    code=code,
                    message=message,
                    byte_start=byte_start,
                    byte_end=byte_end,
                )
            )
    return rows


def lexical_skeleton(value: str) -> str:
    def replace(match: re.Match[str]) -> str:
        token = match.group(0)
        if token in KEYWORDS:
            return token
        return "$N" if token.isdigit() else "$ID"

    return TOKEN_RE.sub(replace, value)


CorpusCase = Union[ValidCase, MalformedCase]


def input_digest(kind: str, cases: Iterable[CorpusCase]) -> str:
    digest = hashlib.sha256()
    for case in cases:
        source = case.source.encode("ascii")
        case_id = case.case_id.encode("ascii")
        digest.update(kind.encode("ascii"))
        digest.update(len(case_id).to_bytes(4, "big"))
        digest.update(case_id)
        digest.update(len(source).to_bytes(8, "big"))
        digest.update(source)
    return digest.hexdigest()


def manifest_digest(cases: Iterable[CorpusCase]) -> str:
    digest = hashlib.sha256()
    for case in cases:
        row = json.dumps(case.manifest(), sort_keys=True, separators=(",", ":"), ensure_ascii=True)
        digest.update(row.encode("ascii"))
        digest.update(b"\n")
    return digest.hexdigest()


def observed_contract(valid: Sequence[ValidCase], malformed: Sequence[MalformedCase]) -> dict[str, object]:
    combined = [*valid, *malformed]
    return {
        "schema": SCHEMA,
        "seed": DEFAULT_SEED,
        "model_file_id": MODEL_FILE_ID,
        "declaration_axes": DECLARATION_AXIS_COUNT,
        "statement_axes": STATEMENT_AXIS_COUNT,
        "valid_count": VALID_COUNT,
        "malformed_families": MALFORMED_FAMILY_COUNT,
        "malformed_variants_per_family": MALFORMED_VARIANTS,
        "malformed_count": MALFORMED_COUNT,
        "valid_input_sha256": input_digest("valid", valid),
        "malformed_input_sha256": input_digest("malformed", malformed),
        "manifest_sha256": manifest_digest(combined),
    }


def assert_unique(label: str, values: Iterable[object], expected: int) -> None:
    materialized = list(values)
    if len(materialized) != expected:
        raise AssertionError(f"{label} denominator: expected {expected}, observed {len(materialized)}")
    if len(set(materialized)) != expected:
        raise AssertionError(f"{label} is not unique: {expected - len(set(materialized))} collisions")


def audit_model() -> dict[str, object]:
    valid = valid_cases()
    malformed = malformed_cases()
    alternate_valid = valid_cases(ALTERNATE_AUDIT_SEED)
    alternate_malformed = malformed_cases(ALTERNATE_AUDIT_SEED)
    declaration_axis_models = declaration_models(DEFAULT_SEED, 0)
    statement_axis_models = statement_models(DEFAULT_SEED, 0)

    assert len(declaration_axis_models) == DECLARATION_AXIS_COUNT
    assert len(statement_axis_models) == STATEMENT_AXIS_COUNT
    assert len(malformed_families()) == MALFORMED_FAMILY_COUNT
    assert len(valid) == VALID_COUNT
    assert len(malformed) == MALFORMED_COUNT
    assert len(alternate_valid) == VALID_COUNT
    assert len(alternate_malformed) == MALFORMED_COUNT

    assert_unique("valid ids", (case.case_id for case in valid), VALID_COUNT)
    assert_unique("valid source bytes", (case.source.encode("ascii") for case in valid), VALID_COUNT)
    assert_unique("valid expected fingerprints", (case.expected_fingerprint for case in valid), VALID_COUNT)
    assert_unique(
        "declaration-axis model fingerprints",
        (model.fingerprint for model in declaration_axis_models),
        DECLARATION_AXIS_COUNT,
    )
    assert_unique(
        "statement-axis model fingerprints",
        (model.fingerprint for model in statement_axis_models),
        STATEMENT_AXIS_COUNT,
    )
    assert_unique("malformed ids", (case.case_id for case in malformed), MALFORMED_COUNT)
    assert_unique("malformed source bytes", (case.source.encode("ascii") for case in malformed), MALFORMED_COUNT)
    if {case.source for case in valid} & {case.source for case in malformed}:
        raise AssertionError("valid and malformed corpora overlap")

    pairs = {(case.declaration_axis, case.statement_axis) for case in valid}
    declaration_axes = {case.declaration_axis for case in valid}
    statement_axes = {case.statement_axis for case in valid}
    if len(declaration_axes) != DECLARATION_AXIS_COUNT:
        raise AssertionError(f"declaration coverage changed: {len(declaration_axes)}")
    if len(statement_axes) != STATEMENT_AXIS_COUNT:
        raise AssertionError(f"statement coverage changed: {len(statement_axes)}")
    if len(pairs) != VALID_COUNT:
        raise AssertionError(f"Cartesian coverage changed: {len(pairs)}")
    family_counts = {family: 0 for family, _ in malformed_families()}
    for case in malformed:
        family_counts[case.family] += 1
    if set(family_counts.values()) != {MALFORMED_VARIANTS}:
        raise AssertionError(f"malformed family coverage changed: {family_counts}")

    for default, alternate in zip(valid, alternate_valid):
        if (default.case_id, default.declaration_axis, default.statement_axis) != (
            alternate.case_id,
            alternate.declaration_axis,
            alternate.statement_axis,
        ):
            raise AssertionError(f"seed selected a structural axis: {default.case_id}")
        if lexical_skeleton(default.source) != lexical_skeleton(alternate.source):
            raise AssertionError(f"seed changed source structure: {default.case_id}")
        if lexical_skeleton(default.expected_fingerprint) != lexical_skeleton(alternate.expected_fingerprint):
            raise AssertionError(f"seed changed expected structure: {default.case_id}")
        if default.source == alternate.source:
            raise AssertionError(f"seed failed to perturb lexical payload: {default.case_id}")
    for default, alternate in zip(malformed, alternate_malformed):
        if (default.case_id, default.family, default.variant, default.code) != (
            alternate.case_id,
            alternate.family,
            alternate.variant,
            alternate.code,
        ):
            raise AssertionError(f"seed selected malformed structure: {default.case_id}")
        if lexical_skeleton(default.source) != lexical_skeleton(alternate.source):
            raise AssertionError(f"seed changed malformed source structure: {default.case_id}")
        if default.source == alternate.source:
            raise AssertionError(f"seed failed to perturb malformed payload: {default.case_id}")

    for case in valid:
        if not case.source.isascii() or "\n" in case.source[:-1] or not case.source.endswith("\n"):
            raise AssertionError(f"valid source is not canonical one-line ASCII: {case.case_id}")
        for fact in case.span_facts:
            if fact.file_id != MODEL_FILE_ID or fact.line != 1 or fact.column != fact.byte_start + 1:
                raise AssertionError(f"invalid span coordinates: {case.case_id}: {fact}")
            if not (0 <= fact.byte_start <= fact.byte_end <= len(case.source)):
                raise AssertionError(f"span outside source: {case.case_id}: {fact}")
    for case in malformed:
        if not case.source.isascii() or "\n" in case.source:
            raise AssertionError(f"malformed source is not one-line ASCII: {case.case_id}")
        if not (0 <= case.byte_start <= case.byte_end <= len(case.source)):
            raise AssertionError(f"diagnostic outside source: {case.case_id}")

    observed = observed_contract(valid, malformed)
    expected = json.loads(CONTRACT.read_text(encoding="utf-8"))
    if observed != expected:
        raise AssertionError(
            "Day-4 generated corpus freeze mismatch\n"
            f"expected={json.dumps(expected, sort_keys=True)}\n"
            f"observed={json.dumps(observed, sort_keys=True)}"
        )
    return observed


def emit_jsonl(kind: str) -> None:
    if kind in ("valid", "all"):
        for case in valid_cases():
            print(json.dumps(case.emitted(), sort_keys=True, separators=(",", ":")))
    if kind in ("malformed", "all"):
        for case in malformed_cases():
            print(json.dumps(case.emitted(), sort_keys=True, separators=(",", ":")))


def run_one_cli(binary: str, case: CorpusCase) -> Optional[str]:
    try:
        completed = subprocess.run(
            [binary, "--parse-only", "/dev/stdin"],
            input=case.source.encode("ascii"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=2,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return f"{case.case_id}: TIMEOUT after 2 seconds"

    if isinstance(case, ValidCase):
        expected = (0, b"", b"")
    else:
        stderr = (
            f"/dev/stdin:{case.line}:{case.column}: "
            f"error[{case.code}]: {case.message}\n"
        ).encode("ascii")
        expected = (1, b"", stderr)
    actual = (completed.returncode, completed.stdout, completed.stderr)
    if actual != expected:
        return f"{case.case_id}: expected={expected!r} actual={actual!r}"
    return None


def run_cli(binary: str, jobs: int) -> None:
    # Run the model audit first so behavioral success can never mask a damaged
    # denominator, generator, or freeze contract.
    audit_model()
    cases: list[CorpusCase] = [*valid_cases(), *malformed_cases()]
    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as executor:
        failures = [
            failure
            for failure in executor.map(lambda case: run_one_cli(binary, case), cases)
            if failure is not None
        ]
    if failures:
        shown = "\n".join(failures[:20])
        suffix = "" if len(failures) <= 20 else f"\n... {len(failures) - 20} more"
        raise AssertionError(f"Day-4 CLI behavioral matrix failed ({len(failures)}):\n{shown}{suffix}")
    print(
        f"DAY4_GENERATED_BEHAVIOR valid={VALID_COUNT}/{VALID_COUNT} "
        f"malformed={MALFORMED_COUNT}/{MALFORMED_COUNT} timeout=0 panic=0"
    )


def decode_probe_spans(encoded: str) -> dict[str, SpanFact]:
    spans: dict[str, SpanFact] = {}
    if not encoded:
        return spans
    for record in encoded.split(","):
        fields = record.split(":")
        if len(fields) != 6:
            raise AssertionError(f"malformed probe span record {record!r}")
        path = bytes.fromhex(fields[0]).decode("ascii")
        fact = SpanFact(
            path,
            int(fields[2]),
            int(fields[3]),
            int(fields[4]),
            int(fields[5]),
            int(fields[1]),
        )
        if path in spans:
            raise AssertionError(f"probe emitted duplicate node path {path}")
        spans[path] = fact
    return spans


@dataclasses.dataclass(frozen=True)
class ProbeDiagnostic:
    case_id: str
    code: str
    message: str
    file_id: int
    byte_start: int
    byte_end: int
    line: int
    column: int


def expected_probe_diagnostic(case: MalformedCase) -> ProbeDiagnostic:
    return ProbeDiagnostic(
        case.case_id,
        case.code,
        case.message,
        MODEL_FILE_ID,
        case.byte_start,
        case.byte_end,
        case.line,
        case.column,
    )


def decode_probe_diagnostic(fields: Sequence[str]) -> ProbeDiagnostic:
    if len(fields) != 5 or fields[0] != "ERR":
        raise AssertionError(f"malformed probe diagnostic record {fields!r}")
    coordinates = fields[4].split(":")
    if len(coordinates) != 5:
        raise AssertionError(f"malformed probe diagnostic coordinates {fields[4]!r}")
    return ProbeDiagnostic(
        case_id=fields[1],
        code=fields[2],
        message=bytes.fromhex(fields[3]).decode("ascii"),
        file_id=int(coordinates[0]),
        byte_start=int(coordinates[1]),
        byte_end=int(coordinates[2]),
        line=int(coordinates[3]),
        column=int(coordinates[4]),
    )


def diagnostic_difference(expected: ProbeDiagnostic, actual: ProbeDiagnostic) -> Optional[str]:
    if actual == expected:
        return None
    changed = [
        field.name
        for field in dataclasses.fields(ProbeDiagnostic)
        if getattr(expected, field.name) != getattr(actual, field.name)
    ]
    return f"changed={changed!r} expected={expected!r} actual={actual!r}"


def run_diagnostic_probe(binary: str) -> None:
    audit_model()
    cases = malformed_cases()
    protocol = "".join(
        f"{case.case_id}\t{case.source.encode('ascii').hex()}\n" for case in cases
    ).encode("ascii")
    try:
        completed = subprocess.run(
            [binary],
            input=protocol,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=15,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError(
            "Day-4 structured-diagnostic probe exceeded 15-second corpus watchdog"
        ) from error
    if completed.returncode != 0:
        raise AssertionError(
            f"Day-4 structured-diagnostic probe exited {completed.returncode}; "
            f"stderr={completed.stderr.decode('utf-8', errors='replace')!r}"
        )
    if completed.stderr:
        raise AssertionError(
            "Day-4 structured-diagnostic probe wrote stderr: "
            f"{completed.stderr.decode('utf-8', errors='replace')!r}"
        )

    lines = completed.stdout.decode("ascii").splitlines()
    if len(lines) != MALFORMED_COUNT:
        raise AssertionError(
            f"structured-diagnostic denominator: expected {MALFORMED_COUNT}, observed {len(lines)}"
        )
    expected_by_id = {case.case_id: expected_probe_diagnostic(case) for case in cases}
    observed_by_id: dict[str, ProbeDiagnostic] = {}
    failures: list[str] = []
    for line in lines:
        fields = line.split("\t")
        try:
            actual = decode_probe_diagnostic(fields)
        except (AssertionError, UnicodeDecodeError, ValueError) as error:
            failures.append(str(error))
            continue
        expected = expected_by_id.get(actual.case_id)
        if expected is None:
            failures.append(f"unexpected structured-diagnostic case {actual.case_id!r}")
            continue
        if actual.case_id in observed_by_id:
            failures.append(f"duplicate structured-diagnostic case {actual.case_id}")
            continue
        observed_by_id[actual.case_id] = actual
        difference = diagnostic_difference(expected, actual)
        if difference is not None:
            failures.append(f"{actual.case_id}: {difference}")
    missing = sorted(set(expected_by_id) - set(observed_by_id))
    if missing:
        failures.append(f"structured-diagnostic probe omitted cases {missing[:20]!r}")
    if failures:
        shown = "\n".join(failures[:20])
        suffix = "" if len(failures) <= 20 else f"\n... {len(failures) - 20} more"
        raise AssertionError(
            f"Day-4 structured-diagnostic join failed ({len(failures)}):\n{shown}{suffix}"
        )

    # Sensitivity controls: independently corrupt each observed field across
    # the full denominator.  A comparator that forgets either field cannot
    # pass this gate merely because production happened to emit the right one.
    file_id_mutants_killed = 0
    byte_end_mutants_killed = 0
    for case_id, expected in expected_by_id.items():
        actual = observed_by_id[case_id]
        file_id_mutant = dataclasses.replace(actual, file_id=actual.file_id + 1)
        byte_end_mutant = dataclasses.replace(actual, byte_end=actual.byte_end + 1)
        file_id_mutants_killed += diagnostic_difference(expected, file_id_mutant) is not None
        byte_end_mutants_killed += diagnostic_difference(expected, byte_end_mutant) is not None
    if file_id_mutants_killed != MALFORMED_COUNT:
        raise AssertionError(
            f"FileId fault control: killed {file_id_mutants_killed}/{MALFORMED_COUNT}"
        )
    if byte_end_mutants_killed != MALFORMED_COUNT:
        raise AssertionError(
            f"byte_end fault control: killed {byte_end_mutants_killed}/{MALFORMED_COUNT}"
        )
    print(
        f"DAY4_GENERATED_DIAGNOSTICS malformed={MALFORMED_COUNT}/{MALFORMED_COUNT} "
        f"exact_structured={MALFORMED_COUNT}/{MALFORMED_COUNT} "
        f"file_id_mutants={file_id_mutants_killed}/{MALFORMED_COUNT} "
        f"byte_end_mutants={byte_end_mutants_killed}/{MALFORMED_COUNT}"
    )


def run_chir_probe(binary: str) -> None:
    # The probe is one streaming process so the 1000-way join is fast.  A
    # bounded whole-corpus timeout still fails closed if any parser case hangs.
    observed = audit_model()
    cases = valid_cases()
    protocol = "".join(
        f"{case.case_id}\t{case.source.encode('ascii').hex()}\n" for case in cases
    ).encode("ascii")
    try:
        completed = subprocess.run(
            [binary],
            input=protocol,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=30,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise AssertionError("Day-4 C-HIR probe exceeded 30-second corpus watchdog") from error
    if completed.returncode != 0:
        raise AssertionError(
            f"Day-4 C-HIR probe exited {completed.returncode}; "
            f"stderr={completed.stderr.decode('utf-8', errors='replace')!r}"
        )
    if completed.stderr:
        raise AssertionError(
            f"Day-4 C-HIR probe wrote stderr: {completed.stderr.decode('utf-8', errors='replace')!r}"
        )
    lines = completed.stdout.decode("ascii").splitlines()
    if len(lines) != VALID_COUNT:
        raise AssertionError(
            f"C-HIR probe denominator: expected {VALID_COUNT}, observed {len(lines)}"
        )
    expected_by_id = {case.case_id: case for case in cases}
    seen: set[str] = set()
    failures: list[str] = []
    for line in lines:
        fields = line.split("\t")
        if len(fields) < 2:
            failures.append(f"malformed probe output {line!r}")
            continue
        status, case_id = fields[:2]
        case = expected_by_id.get(case_id)
        if case is None:
            failures.append(f"unexpected probe case {case_id!r}")
            continue
        if case_id in seen:
            failures.append(f"duplicate probe case {case_id}")
            continue
        seen.add(case_id)
        if status != "OK":
            failures.append(f"{case_id}: parser rejected generated-valid source: {fields!r}")
            continue
        if len(fields) != 4:
            failures.append(f"{case_id}: malformed OK record {fields!r}")
            continue
        actual_fingerprint = bytes.fromhex(fields[2]).decode("ascii")
        if actual_fingerprint != case.expected_fingerprint:
            failures.append(
                f"{case_id}: fingerprint\n"
                f"  expected {case.expected_fingerprint}\n"
                f"  actual   {actual_fingerprint}"
            )
        try:
            actual_spans = decode_probe_spans(fields[3])
        except (AssertionError, UnicodeDecodeError, ValueError) as error:
            failures.append(f"{case_id}: {error}")
            continue
        expected_spans: dict[str, SpanFact] = {}
        for fact in case.span_facts:
            if fact.label in expected_spans:
                failures.append(f"{case_id}: model has duplicate node path {fact.label}")
            expected_spans[fact.label] = fact
        if actual_spans != expected_spans:
            missing = sorted(set(expected_spans) - set(actual_spans))
            extra = sorted(set(actual_spans) - set(expected_spans))
            changed = sorted(
                path
                for path in set(actual_spans) & set(expected_spans)
                if actual_spans[path] != expected_spans[path]
            )
            detail = []
            if missing:
                detail.append(f"missing={missing[:8]!r}")
            if extra:
                detail.append(f"extra={extra[:8]!r}")
            if changed:
                examples = [
                    f"{path}: expected={expected_spans[path]} actual={actual_spans[path]}"
                    for path in changed[:5]
                ]
                detail.append("changed=" + " | ".join(examples))
            failures.append(f"{case_id}: node spans {'; '.join(detail)}")
    missing_cases = sorted(set(expected_by_id) - seen)
    if missing_cases:
        failures.append(f"probe omitted cases {missing_cases[:20]!r}")
    if failures:
        shown = "\n".join(failures[:20])
        suffix = "" if len(failures) <= 20 else f"\n... {len(failures) - 20} more"
        raise AssertionError(f"Day-4 C-HIR behavioral join failed ({len(failures)}):\n{shown}{suffix}")
    print(
        f"DAY4_GENERATED_CHIR valid={VALID_COUNT}/{VALID_COUNT} "
        f"fingerprints={VALID_COUNT}/{VALID_COUNT} exact_node_spans={VALID_COUNT}/{VALID_COUNT} "
        f"manifest_sha256={observed['manifest_sha256']}"
    )


def parse_args(argv: Sequence[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--emit-jsonl", choices=("valid", "malformed", "all"))
    mode.add_argument("--behavior-cli", metavar="EVOCC")
    mode.add_argument("--behavior-chir", metavar="DAY4_CHIR_PROBE")
    mode.add_argument("--behavior-diagnostics", metavar="DAY4_CHIR_PROBE")
    parser.add_argument("--jobs", type=int, default=8)
    return parser.parse_args(argv)


def main(argv: Sequence[str]) -> int:
    arguments = parse_args(argv)
    if arguments.jobs < 1 or arguments.jobs > 64:
        raise ValueError("--jobs must be between 1 and 64")
    if arguments.emit_jsonl:
        audit_model()
        emit_jsonl(arguments.emit_jsonl)
        return 0
    if arguments.behavior_cli:
        run_cli(arguments.behavior_cli, arguments.jobs)
        return 0
    if arguments.behavior_chir:
        run_chir_probe(arguments.behavior_chir)
        return 0
    if arguments.behavior_diagnostics:
        run_diagnostic_probe(arguments.behavior_diagnostics)
        return 0
    observed = audit_model()
    print(
        f"DAY4_GENERATED_MODEL valid={VALID_COUNT} "
        f"declarations={DECLARATION_AXIS_COUNT} statements={STATEMENT_AXIS_COUNT} "
        f"declaration_fingerprints={DECLARATION_AXIS_COUNT} "
        f"statement_fingerprints={STATEMENT_AXIS_COUNT} "
        f"malformed={MALFORMED_COUNT} families={MALFORMED_FAMILY_COUNT} "
        f"manifest_sha256={observed['manifest_sha256']}"
    )
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv[1:]))
    except (AssertionError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"day4 generated corpus failed: {error}", file=sys.stderr)
        raise SystemExit(1)
