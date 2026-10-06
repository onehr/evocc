#!/usr/bin/env python3
"""Turn the reviewed registry block of the specification into Lean data.

The block this reads is normative text: Section 11.6 prints the rules and the
SHA-256 of their exact bytes, and Section 11.3 says the registry digest covers
those bytes including the newline after the last rule.  This tool

  1. extracts the block for one registry name and for the revision it
     revises,
  2. recomputes both digests and refuses to run when either differs from the
     digest the specification states beside its block,
  3. checks byte for byte that the earlier registry's block is a prefix of the
     later one's, which is what makes a certificate naming the earlier
     registry covered by the proof of the later one by construction,
  4. decodes each line into a rule value,
  5. renders the decoded values back to text and compares byte for byte,
  6. writes `lean/EvoIR/Registry.lean` containing nothing but data.

The Lean file holds rule values, never propositions: what a rule means is
fixed once by `RuleSound` in `EvoIR/Rule.lean`.  The round trip says the two
conversions agree with each other; it does not by itself say the decoding is
faithful, which is why the rendering is repeated inside Lean by `Rule.render`
and checked again by `tools/lean/audit.py`.
"""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass
from pathlib import Path

BIN_OPS = {"add", "sub", "mul", "and", "or", "xor"}
CMP_OPS = {"icmp.eq", "icmp.ne", "icmp.slt", "icmp.sle", "icmp.ult", "icmp.ule"}
SHIFT_OPS = {"shl", "lshr", "ashr"}
CAST_OPS = {"zext", "sext", "trunc"}

LEAN_BIN = {"add": "add", "sub": "sub", "mul": "mul", "and": "and", "or": "or", "xor": "xor"}
LEAN_CMP = {
    "icmp.eq": "eq",
    "icmp.ne": "ne",
    "icmp.slt": "slt",
    "icmp.sle": "sle",
    "icmp.ult": "ult",
    "icmp.ule": "ule",
}
LEAN_SHIFT = {"shl": "shl", "lshr": "lshr", "ashr": "ashr"}
LEAN_CAST = {"zext": "zext", "sext": "sext", "trunc": "trunc"}
LEAN_GUARD = {"signed_add_defined": "add", "signed_sub_defined": "sub",
              "signed_mul_defined": "mul", "signed_div_rem_no_overflow": "divRem"}


class RegistryError(Exception):
    """A refusal: the specification and this tool disagree."""


# -- the pattern AST -------------------------------------------------------


@dataclass(frozen=True)
class Var:
    name: str

    def render(self) -> str:
        return "?" + self.name

    def lean(self) -> str:
        return f'.var "{self.name}"'


@dataclass(frozen=True)
class ConstLike:
    name: str
    k: int

    def render(self) -> str:
        return f"const_like(?{self.name},{self.k})"

    def lean(self) -> str:
        return f'.constLike "{self.name}" {self.k}'


@dataclass(frozen=True)
class BoolLit:
    k: int

    def render(self) -> str:
        return f"bool({self.k})"

    def lean(self) -> str:
        return f".boolLit {self.k}"


@dataclass(frozen=True)
class Bin:
    op: str
    is_fold: bool
    a: object
    b: object

    def render(self) -> str:
        head = "fold." if self.is_fold else ""
        return f"{head}{self.op}({self.a.render()},{self.b.render()})"

    def lean(self) -> str:
        fold = "true" if self.is_fold else "false"
        return f".bin .{LEAN_BIN[self.op]} {fold} ({self.a.lean()}) ({self.b.lean()})"


@dataclass(frozen=True)
class Cmp:
    op: str
    is_fold: bool
    a: object
    b: object

    def render(self) -> str:
        head = "fold." if self.is_fold else ""
        return f"{head}{self.op}({self.a.render()},{self.b.render()})"

    def lean(self) -> str:
        fold = "true" if self.is_fold else "false"
        return f".cmp .{LEAN_CMP[self.op]} {fold} ({self.a.lean()}) ({self.b.lean()})"


@dataclass(frozen=True)
class Shift:
    op: str
    a: object
    k: object

    def render(self) -> str:
        return f"{self.op}({self.a.render()},{self.k.render()})"

    def lean(self) -> str:
        return f".shift .{LEAN_SHIFT[self.op]} ({self.a.lean()}) ({self.k.lean()})"


@dataclass(frozen=True)
class Cast:
    op: str
    a: object

    def render(self) -> str:
        return f"{self.op}({self.a.render()})"

    def lean(self) -> str:
        return f".cast .{LEAN_CAST[self.op]} ({self.a.lean()})"


@dataclass(frozen=True)
class KnownConst:
    def render(self) -> str:
        return "known_const"

    def lean(self) -> str:
        return ".knownConst"


@dataclass(frozen=True)
class Guard:
    op: str
    args: tuple

    def render(self) -> str:
        return self.op + "(" + ",".join(a.render() for a in self.args) + ")"

    def lean(self) -> str:
        values = " ".join("(" + a.lean() + ")" for a in self.args)
        if self.op == "divisor_nonzero":
            return ".nonzero " + values
        if self.op.startswith("shift_count_defined."):
            signed = "true" if self.op.endswith(".signed") else "false"
            return ".count " + signed + " " + values
        return ".guard ." + LEAN_GUARD[self.op] + " " + values


@dataclass(frozen=True)
class PatRhs:
    p: object

    def render(self) -> str:
        return self.p.render()

    def lean(self) -> str:
        return f".pat ({self.p.lean()})"


@dataclass(frozen=True)
class Cond:
    kind: str
    args: tuple

    def render(self) -> str:
        if self.kind == "true":
            return "true"
        if self.kind == "all_const":
            return "all_const"
        if self.kind == "fully_known":
            return "fully_known"
        if self.kind == "covers":
            return f"covers(?{self.args[0]},?{self.args[1]})"
        if self.kind == "known_zero":
            return f"known_zero(?{self.args[0]})"
        if self.kind == "width_min":
            return f"width_min(?{self.args[0]},{self.args[1]})"
        raise RegistryError(f"unknown condition kind {self.kind}")

    def lean(self) -> str:
        if self.kind == "true":
            return ".tt"
        if self.kind == "all_const":
            return ".allConst"
        if self.kind == "fully_known":
            return ".fullyKnown"
        if self.kind == "covers":
            return f'.covers "{self.args[0]}" "{self.args[1]}"'
        if self.kind == "known_zero":
            return f'.knownZero "{self.args[0]}"'
        if self.kind == "width_min":
            return f'.widthMin "{self.args[0]}" {self.args[1]}'
        raise RegistryError(f"unknown condition kind {self.kind}")


@dataclass(frozen=True)
class Rule:
    ident: str
    lhs: object
    rhs: object
    cond: Cond

    def render(self) -> str:
        return f"{self.ident} {self.lhs.render()} => {self.rhs.render()} when {self.cond.render()}"


# -- parsing ---------------------------------------------------------------


def split_args(text: str) -> list[str]:
    """Split the comma-separated arguments of one parenthesised list."""
    parts: list[str] = []
    depth = 0
    current = ""
    for ch in text:
        if ch == "(":
            depth += 1
            current += ch
        elif ch == ")":
            depth -= 1
            current += ch
        elif ch == "," and depth == 0:
            parts.append(current)
            current = ""
        else:
            current += ch
    parts.append(current)
    return parts


def parse_int(text: str) -> int:
    if not re.fullmatch(r"0|[1-9][0-9]*", text):
        raise RegistryError(f"not a canonical integer: {text!r}")
    return int(text)


def parse_var(text: str) -> str:
    if not re.fullmatch(r"\?[A-Za-z_][A-Za-z0-9_]*", text):
        raise RegistryError(f"not a variable: {text!r}")
    return text[1:]


def parse_pat(text: str):
    text = text.strip()
    if text.startswith("?"):
        return Var(parse_var(text))
    match = re.fullmatch(r"([A-Za-z_][A-Za-z0-9_.]*)\((.*)\)", text, re.DOTALL)
    if not match:
        raise RegistryError(f"not a pattern: {text!r}")
    head, inner = match.group(1), match.group(2)
    args = split_args(inner)
    if head == "const_like":
        if len(args) != 2:
            raise RegistryError(f"const_like takes two arguments: {text!r}")
        return ConstLike(parse_var(args[0].strip()), parse_int(args[1].strip()))
    if head == "bool":
        if len(args) != 1:
            raise RegistryError(f"bool takes one argument: {text!r}")
        return BoolLit(parse_int(args[0].strip()))
    is_fold = head.startswith("fold.")
    op = head[len("fold.") :] if is_fold else head
    if op in LEAN_GUARD or op in ("divisor_nonzero", "shift_count_defined.signed", "shift_count_defined.unsigned"):
        if is_fold or len(args) != (1 if op == "divisor_nonzero" else 2):
            raise RegistryError(f"not a typed g0 predicate: {text!r}")
        return Guard(op, tuple(parse_pat(a) for a in args))
    if op in BIN_OPS:
        if len(args) != 2:
            raise RegistryError(f"{op} takes two operands: {text!r}")
        return Bin(op, is_fold, parse_pat(args[0]), parse_pat(args[1]))
    if op in CMP_OPS:
        if len(args) != 2:
            raise RegistryError(f"{op} takes two operands: {text!r}")
        return Cmp(op, is_fold, parse_pat(args[0]), parse_pat(args[1]))
    if op in SHIFT_OPS:
        if is_fold:
            raise RegistryError(f"no fold builder for a shift: {text!r}")
        if len(args) != 2:
            raise RegistryError(f"{op} takes a value and an amount: {text!r}")
        return Shift(op, parse_pat(args[0]), parse_pat(args[1]))
    if op in CAST_OPS:
        if is_fold:
            raise RegistryError(f"no fold builder for a width change: {text!r}")
        if len(args) != 1:
            raise RegistryError(f"{op} takes one operand: {text!r}")
        return Cast(op, parse_pat(args[0]))
    raise RegistryError(f"operator outside the Section 8.5 allowlist: {text!r}")


def parse_rhs(text: str):
    text = text.strip()
    if text == "known_const":
        return KnownConst()
    return PatRhs(parse_pat(text))


def parse_cond(text: str) -> Cond:
    text = text.strip()
    if text in ("true", "all_const", "fully_known"):
        return Cond(text, ())
    match = re.fullmatch(r"([a-z_]+)\((.*)\)", text)
    if not match:
        raise RegistryError(f"not a side condition: {text!r}")
    head, inner = match.group(1), match.group(2)
    args = split_args(inner)
    if head == "covers":
        if len(args) != 2:
            raise RegistryError(f"covers takes two classes: {text!r}")
        return Cond("covers", (parse_var(args[0].strip()), parse_var(args[1].strip())))
    if head == "known_zero":
        if len(args) != 1:
            raise RegistryError(f"known_zero takes one class: {text!r}")
        return Cond("known_zero", (parse_var(args[0].strip()),))
    if head == "width_min":
        if len(args) != 2:
            raise RegistryError(f"width_min takes a class and a bound: {text!r}")
        return Cond("width_min", (parse_var(args[0].strip()), parse_int(args[1].strip())))
    raise RegistryError(f"unknown side condition: {text!r}")


def parse_rule(line: str) -> Rule:
    if " => " not in line or " when " not in line:
        raise RegistryError(f"not a rule line: {line!r}")
    left, rest = line.split(" => ", 1)
    rhs_text, cond_text = rest.rsplit(" when ", 1)
    ident, _, lhs_text = left.partition(" ")
    if not re.fullmatch(r"[a-z0-9_]+", ident):
        raise RegistryError(f"not a rule id: {ident!r}")
    if not lhs_text:
        raise RegistryError(f"no left-hand side: {line!r}")
    return Rule(ident, parse_pat(lhs_text), parse_rhs(rhs_text), parse_cond(cond_text))


# -- extraction ------------------------------------------------------------


def extract_block(spec: str, name: str) -> tuple[str, str]:
    """Return the registry block's exact text and the digest stated beside it."""
    marker = f"**Reviewed registry `{name}`.**"
    start = spec.find(marker)
    if start < 0:
        raise RegistryError(f"the specification prints no registry {name}")
    fence = spec.find("```text\n", start)
    if fence < 0:
        raise RegistryError(f"no rule block follows the {name} paragraph")
    digests = re.findall(r"`([0-9a-f]{64})`", spec[start:fence])
    if len(digests) != 1:
        raise RegistryError(
            f"expected exactly one digest between the {name} heading and its block, "
            f"found {len(digests)}"
        )
    body_start = fence + len("```text\n")
    end = spec.find("```", body_start)
    if end < 0:
        raise RegistryError(f"the {name} block is not closed")
    return spec[body_start:end], digests[0]


def lean_string(text: str) -> str:
    escaped = text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n")
    return '"' + escaped + '"'


def lean_ident(rule_id: str) -> str:
    return "rule_" + rule_id


HEADER = """\
-- Generated by tools/lean/registry_to_lean.py.  Do not edit.
--
-- Rule data decoded from the reviewed registry block of spec/evoir.md
-- section 11.6.  This file holds values, never propositions: what a rule
-- claims is fixed once by `RuleSound` in EvoIR/Rule.lean.
import EvoIR.Rule

namespace EvoIR
"""


def emit(
    rules: list[Rule],
    name: str,
    digest: str,
    block: str,
    prefix_name: str,
    prefix_digest: str,
    prefix_block: str,
    namespace: str = "EvoIR",
) -> str:
    prefix_count = len([line for line in prefix_block.split("\n") if line])
    out = [HEADER]
    out.append("/-- The registry name a certificate commits to. -/")
    out.append(f"def registryName : String := {lean_string(name)}\n")
    out.append("/-- The SHA-256 the specification states beside the block. -/")
    out.append(f"def registryDigest : String := {lean_string(digest)}\n")
    out.append(
        "/-- The exact bytes the digest covers: the first rule id through the\n"
        "newline after the last rule. -/"
    )
    out.append(f"def registryText : String := {lean_string(block)}\n")
    out.append(
        "/-- The revision this one revises.  Section 11.6: a revision includes\n"
        "every rule of the registry it revises, unchanged and in the same order,\n"
        "before its new rules, so a certificate naming the earlier registry\n"
        "fires only rules this file also holds. -/"
    )
    out.append(f"def registryPrefixName : String := {lean_string(prefix_name)}\n")
    out.append(f"def registryPrefixDigest : String := {lean_string(prefix_digest)}\n")
    out.append(f"def registryPrefixCount : Nat := {prefix_count}\n")
    out.append(f"def registryPrefixText : String := {lean_string(prefix_block)}\n")
    for rule in rules:
        out.append(f"def {lean_ident(rule.ident)} : Rule :=")
        out.append("  { id := " + lean_string(rule.ident))
        out.append("    lhs := " + rule.lhs.lean())
        out.append("    rhs := " + rule.rhs.lean())
        out.append("    cond := " + rule.cond.lean() + " }\n")
    out.append("/-- The registry, in the order the specification prints it. -/")
    out.append("def registry : List Rule :=")
    out.append("  [ " + "\n  , ".join(lean_ident(r.ident) for r in rules))
    out.append("  ]\n")
    out.append(
        "/-! The round trip, repeated inside Lean: rendering the decoded values\n"
        "reproduces the block the digest covers. -/"
    )
    out.append(
        "#guard (registry.map Rule.render).foldr (fun l acc => l ++ \"\\n\" ++ acc) \"\" "
        "== registryText\n"
    )
    out.append(
        "/-! The earlier revision is a prefix, in the rule values and in the\n"
        "bytes the earlier digest covers. -/"
    )
    out.append(
        "#guard ((registry.take registryPrefixCount).map Rule.render).foldr "
        "(fun l acc => l ++ \"\\n\" ++ acc) \"\" == registryPrefixText\n"
    )
    out.append("#guard registryText.startsWith registryPrefixText\n")
    out.append("end EvoIR")
    text = "\n".join(out) + "\n"
    return text.replace("namespace EvoIR\n", "namespace " + namespace + "\n").replace(
        "end EvoIR\n", "end " + namespace + "\n")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--spec", default="spec/evoir.md")
    parser.add_argument("--registry", default="p0.scalar.v4")
    parser.add_argument(
        "--prefix-registry",
        default="p0.scalar.v3",
        help="the revision the chosen registry revises; its block must be a "
        "byte prefix of the chosen one's",
    )
    parser.add_argument("--out", default="lean/EvoIR/Registry.lean")
    parser.add_argument("--namespace", default="EvoIR", choices=("EvoIR", "EvoIR.Guards"))
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the existing output instead of writing it",
    )
    args = parser.parse_args(argv)

    spec_path = Path(args.spec)
    spec = spec_path.read_text(encoding="utf-8")
    block, stated = extract_block(spec, args.registry)

    computed = hashlib.sha256(block.encode("utf-8")).hexdigest()
    if computed != stated:
        print(
            f"registry digest mismatch for {args.registry}:\n"
            f"  specification states {stated}\n"
            f"  bytes of the block   {computed}",
            file=sys.stderr,
        )
        return 2

    prefix_block, prefix_stated = extract_block(spec, args.prefix_registry)
    prefix_computed = hashlib.sha256(prefix_block.encode("utf-8")).hexdigest()
    if prefix_computed != prefix_stated:
        print(
            f"registry digest mismatch for {args.prefix_registry}:\n"
            f"  specification states {prefix_stated}\n"
            f"  bytes of the block   {prefix_computed}",
            file=sys.stderr,
        )
        return 2
    if not block.startswith(prefix_block):
        print(
            f"{args.prefix_registry} is not a byte prefix of {args.registry}; "
            "a certificate naming the earlier registry would not be covered",
            file=sys.stderr,
        )
        return 2

    lines = block.split("\n")
    if lines[-1] != "":
        print("the registry block does not end with a newline", file=sys.stderr)
        return 2
    lines = lines[:-1]

    rules = [parse_rule(line) for line in lines]

    seen: set[str] = set()
    for rule in rules:
        if rule.ident in seen:
            print(f"duplicate rule id {rule.ident}", file=sys.stderr)
            return 3
        seen.add(rule.ident)

    rendered = "".join(rule.render() + "\n" for rule in rules)
    if rendered.encode("utf-8") != block.encode("utf-8"):
        print("the decoded rules do not render back to the block", file=sys.stderr)
        for index, (left, right) in enumerate(zip(block.split("\n"), rendered.split("\n"))):
            if left != right:
                print(f"  line {index + 1}:", file=sys.stderr)
                print(f"    specification {left!r}", file=sys.stderr)
                print(f"    rendered      {right!r}", file=sys.stderr)
        return 3

    text = emit(
        rules, args.registry, stated, block, args.prefix_registry, prefix_stated, prefix_block,
        args.namespace
    )
    out_path = Path(args.out)
    if args.check:
        if not out_path.exists():
            print(f"{out_path} does not exist", file=sys.stderr)
            return 4
        if out_path.read_text(encoding="utf-8") != text:
            print(f"{out_path} is not what the generator produces", file=sys.stderr)
            return 4
        print(
            f"{out_path} matches the generator, {len(rules)} rules, digest {stated}; "
            f"{args.prefix_registry} ({prefix_stated}) is a byte prefix"
        )
        return 0

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(text, encoding="utf-8")
    print(
        f"wrote {out_path}: {len(rules)} rules, digest {stated}; "
        f"{args.prefix_registry} ({prefix_stated}) is a byte prefix"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
