#!/usr/bin/env python3
"""Generate register facts from the reviewed, canonical target description."""

from __future__ import annotations

import argparse
import hashlib
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path


class TargetDescriptionError(ValueError):
    """A malformed or inconsistent target fact, not an inferred repair."""


@dataclass(frozen=True)
class Bank:
    first: int
    count: int

    def contains(self, number: int) -> bool:
        return self.first <= number < self.first + self.count


@dataclass(frozen=True)
class View:
    bank: str
    bits: int
    prefix: str


@dataclass(frozen=True)
class Role:
    bank: str
    number: int


@dataclass
class Description:
    digest: str
    banks: dict[str, Bank] = field(default_factory=dict)
    views: dict[str, View] = field(default_factory=dict)
    aliases: list[tuple[str, str]] = field(default_factory=list)
    set_tokens: dict[str, tuple[str, ...]] = field(default_factory=dict)
    roles: dict[str, Role] = field(default_factory=dict)
    specials: dict[str, str] = field(default_factory=dict)
    clobber_aliases: dict[str, tuple[str, ...]] = field(default_factory=dict)
    asm_views: dict[str, tuple[str, ...]] = field(default_factory=dict)
    binding_prefixes: dict[str, str] = field(default_factory=dict)
    virtuals: dict[str, tuple[str, int]] = field(default_factory=dict)


def number(token: str) -> int:
    if not re.fullmatch(r"0|[1-9][0-9]*", token):
        raise TargetDescriptionError(f"noncanonical integer: {token}")
    value = int(token)
    if value > 255:
        raise TargetDescriptionError(f"register integer exceeds u8: {token}")
    return value


def identifier(token: str) -> str:
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", token):
        raise TargetDescriptionError(f"invalid identifier: {token}")
    return token


def insert(mapping: dict, key: str, value, kind: str):
    if key in mapping:
        raise TargetDescriptionError(f"duplicate {kind}: {key}")
    mapping[key] = value


def parse_description(source: bytes) -> Description:
    try:
        text = source.decode("ascii")
    except UnicodeDecodeError as error:
        raise TargetDescriptionError("description must be canonical ASCII") from error
    if not text.endswith("\n") or "\r" in text or "\t" in text:
        raise TargetDescriptionError("description requires canonical LF lines")
    lines = text.splitlines()
    if len(lines) < 3 or lines[:2] != ["version regfile.v1", "target aarch64"]:
        raise TargetDescriptionError("unsupported description version or target")
    digest_rows = [line for line in lines if line.startswith("sha256 ")]
    if len(digest_rows) != 1 or lines[2] != digest_rows[0]:
        raise TargetDescriptionError("expected one canonical digest row")
    stated = digest_rows[0].removeprefix("sha256 ")
    if not re.fullmatch(r"[0-9a-f]{64}", stated):
        raise TargetDescriptionError("malformed digest")
    payload = "".join(line + "\n" for line in lines if not line.startswith("sha256 "))
    if hashlib.sha256(payload.encode("ascii")).hexdigest() != stated:
        raise TargetDescriptionError("description digest mismatch")
    description = Description(stated)
    for line in lines[3:]:
        parts = line.split(" ")
        if not line or any(not part for part in parts):
            raise TargetDescriptionError("noncanonical fact spacing")
        kind, *args = parts
        if kind == "bank" and len(args) == 3:
            name, first, count = args
            insert(description.banks, identifier(name), Bank(number(first), number(count)), "bank")
        elif kind == "view" and len(args) == 4:
            name, bank, bits, prefix = args
            insert(description.views, identifier(name), View(identifier(bank), number(bits), prefix), "view")
        elif kind == "alias" and len(args) == 2:
            pair = tuple(identifier(arg) for arg in args)
            if pair in description.aliases:
                raise TargetDescriptionError("duplicate alias")
            description.aliases.append(pair)
        elif kind == "set" and len(args) >= 2:
            name, *members = args
            insert(description.set_tokens, identifier(name), tuple(members), "set")
        elif kind == "role" and len(args) == 3:
            name, bank, physical = args
            insert(description.roles, identifier(name), Role(identifier(bank), number(physical)), "role")
        elif kind == "special" and len(args) == 2:
            name, spelling = args
            insert(description.specials, identifier(name), identifier(spelling), "special")
        elif kind in ("clobber-alias", "asm-view") and len(args) >= 2:
            bank, *prefixes = args
            mapping = description.clobber_aliases if kind == "clobber-alias" else description.asm_views
            insert(mapping, identifier(bank), tuple(prefixes), kind)
        elif kind == "binding-prefix" and len(args) == 2:
            bank, prefix = args
            insert(description.binding_prefixes, identifier(bank), prefix, kind)
        elif kind == "virtual" and len(args) == 3:
            name, spelling, tag = args
            insert(description.virtuals, identifier(name), (identifier(spelling), number(tag)), "virtual")
        else:
            raise TargetDescriptionError(f"unknown fact or wrong arity: {line}")
    return description


def resolve_sets(description: Description) -> dict[str, tuple[int, ...]]:
    resolved: dict[str, tuple[int, ...]] = {}
    active: set[str] = set()

    def visit(name: str) -> tuple[int, ...]:
        if name in resolved:
            return resolved[name]
        if name in active:
            raise TargetDescriptionError(f"cyclic set reference: {name}")
        if name not in description.set_tokens:
            raise TargetDescriptionError(f"unknown set: {name}")
        active.add(name)
        tokens = description.set_tokens[name]
        values: list[int] = []
        if tokens != ("none",):
            for token in tokens:
                values.extend(visit(token[1:]) if token.startswith("@") else (number(token),))
        if len(values) != len(set(values)):
            raise TargetDescriptionError(f"duplicate register in set: {name}")
        active.remove(name)
        resolved[name] = tuple(values)
        return resolved[name]

    for name in description.set_tokens:
        visit(name)
    return {name: resolved[name] for name in description.set_tokens}


def validate(description: Description):
    physical: set[int] = set()
    for name, bank in description.banks.items():
        if bank.count == 0 or bank.first + bank.count > 256:
            raise TargetDescriptionError(f"invalid register bank extent: {name}")
        members = set(range(bank.first, bank.first + bank.count))
        if physical & members:
            raise TargetDescriptionError(f"physical register bank overlap: {name}")
        physical |= members
    for name, view in description.views.items():
        if view.bank not in description.banks:
            raise TargetDescriptionError(f"view {name} names an unknown bank")
        if view.bits == 0 or not re.fullmatch("[a-z]", view.prefix):
            raise TargetDescriptionError(f"invalid register view: {name}")
    for left, right in description.aliases:
        if left not in description.views or right not in description.views:
            raise TargetDescriptionError("alias names an unknown view")
        if description.views[left].bank != description.views[right].bank:
            raise TargetDescriptionError(f"alias crosses physical bank: {left} {right}")
    sets = resolve_sets(description)
    for name, members in sets.items():
        if not set(members) <= physical:
            raise TargetDescriptionError(f"set contains unknown physical register: {name}")
    required_banks = {"gpr", "simd"}
    required_views = {"gpr32", "gpr64", "simd32", "simd64", "simd128"}
    required_sets = {
        "CALLER_SAVED", "CALLEE_SAVED", "SIMD_CALLER_SAVED", "GPR_CALL_CLOBBERS",
        "SIMD_CALL_CLOBBERS", "SIMD_ACROSS_CALL", "SIMD_LOW64_PRESERVED", "RESERVED",
        "FIXED_ASM_GPR", "FIXED_ASM_SIMD", "FIXED_ASM",
    }
    if set(description.banks) != required_banks or set(description.views) != required_views:
        raise TargetDescriptionError("missing or unexpected register bank/view")
    if not required_sets <= sets.keys():
        raise TargetDescriptionError("missing required register set")
    for name in ("CALLER_SAVED", "CALLEE_SAVED", "SIMD_CALLER_SAVED", "SIMD_ACROSS_CALL"):
        if set(sets[name]) & set(sets["RESERVED"]):
            raise TargetDescriptionError(f"allocatable pool intersects reserved registers: {name}")
    for name, bank_name in (
        ("CALLER_SAVED", "gpr"), ("CALLEE_SAVED", "gpr"), ("GPR_CALL_CLOBBERS", "gpr"),
        ("FIXED_ASM_GPR", "gpr"), ("SIMD_CALLER_SAVED", "simd"),
        ("SIMD_ACROSS_CALL", "simd"), ("SIMD_CALL_CLOBBERS", "simd"),
        ("SIMD_LOW64_PRESERVED", "simd"), ("FIXED_ASM_SIMD", "simd"),
    ):
        if any(not description.banks[bank_name].contains(member) for member in sets[name]):
            raise TargetDescriptionError(f"set {name} crosses its physical bank")
    for name, role in description.roles.items():
        if role.bank not in description.banks or not description.banks[role.bank].contains(role.number):
            raise TargetDescriptionError(f"role names an incompatible register bank: {name}")
    for mapping in (description.clobber_aliases, description.asm_views):
        for bank, prefixes in mapping.items():
            if bank not in description.banks or len(prefixes) != len(set(prefixes)):
                raise TargetDescriptionError("invalid register prefix bank")
            if any(not re.fullmatch("[a-z]", prefix) for prefix in prefixes):
                raise TargetDescriptionError("invalid register prefix")
    for bank, prefix in description.binding_prefixes.items():
        if bank not in description.banks or not re.fullmatch("[a-z]", prefix):
            raise TargetDescriptionError("invalid binding prefix")
    if set(description.virtuals) != {"flags"}:
        raise TargetDescriptionError("missing or unexpected virtual register class")
    for mapping in (description.clobber_aliases, description.asm_views, description.binding_prefixes):
        if set(mapping) != required_banks:
            raise TargetDescriptionError("missing register prefix family")
    for required in ("SCRATCH", "SCRATCH_UNIT", "SIMD_SCRATCH", "SIMD_SCRATCH_UNIT", "FRAME_POINTER", "LINK_REGISTER", "PLATFORM_REGISTER"):
        if required not in description.roles:
            raise TargetDescriptionError(f"missing register role: {required}")


def constant(name: str, ty: str, expression: str, visibility: str = "pub") -> str:
    prefix = f"{visibility} const {name}: {ty} ="
    complete = f"{prefix} {expression};"
    return complete + "\n" if len(complete) <= 100 else prefix + "\n    " + expression + ";\n"


def array(name: str, ty: str, values, visibility: str = "pub") -> str:
    values = tuple(values)
    rendered = [repr(value) if isinstance(value, str) else str(value) for value in values]
    expression = "[" + ", ".join(rendered) + "]"
    declaration = f"{visibility} const {name}: [{ty}; {len(values)}] = "
    if len(expression) - 2 <= 60:
        return constant(name, f"[{ty}; {len(values)}]", expression, visibility)
    lines = [declaration + "["]
    pending = "    "
    for value in rendered:
        item = value + ","
        if len(pending) + len(item) > 100:
            lines.append(pending.rstrip())
            pending = "    "
        pending += item + " "
    if pending.strip():
        lines.append(pending.rstrip())
    lines.append("];")
    return "\n".join(lines) + "\n"


def header(description: Description) -> str:
    return (
        "// Generated by tools/generate-target.py; do not edit.\n"
        f"// Source: targets/aarch64/regfile; sha256 {description.digest}.\n\n"
    )


def render_mir(description: Description) -> str:
    sets = resolve_sets(description)
    out = [header(description), "use crate::generic::register::{RegisterBank, RegisterView};\n\n"]
    out.append(constant("DESCRIPTION_DIGEST", "&str", f'"{description.digest}"'))
    for name, bank in description.banks.items():
        out.append(f"\npub const {name.upper()}_BANK: RegisterBank = RegisterBank {{\n    first: {bank.first},\n    count: {bank.count},\n}};\n")
    for name, view in description.views.items():
        out.append(f'\npub const {name.upper()}_VIEW: RegisterView = RegisterView {{\n    bank: {view.bank.upper()}_BANK,\n    bits: {view.bits},\n    prefix: "{view.prefix}",\n}};\n')
    out.append("\n")
    for name, members in sets.items():
        out.append(array(name, "u8", members))
    simd = description.banks["simd"]
    out.append(array("SIMD_LOW64_PRESERVED_INDICES", "u8", (number - simd.first for number in sets["SIMD_LOW64_PRESERVED"])))
    out.append(constant("SIMD_BASE", "u8", str(simd.first)))
    for name, role in description.roles.items():
        out.append(constant(name, "u8", str(role.number)))
        bank = description.banks[role.bank]
        prefixes = [view.prefix for view in description.views.values() if view.bank == role.bank]
        prefixes.extend(description.asm_views[role.bank])
        for prefix in dict.fromkeys(prefixes):
            out.append(constant(f"{name}_{prefix.upper()}", "&str", f'"{prefix}{role.number - bank.first}"'))
    for name, spelling in description.specials.items():
        out.append(constant(name, "&str", f'"{spelling}"'))
    for name, (spelling, tag) in description.virtuals.items():
        out.append(constant(f"{name.upper()}_SPELLING", "&str", f'"{spelling}"'))
        out.append(constant(f"{name.upper()}_ASSIGNMENT_TAG", "u8", str(tag)))
    for bank, prefixes in description.clobber_aliases.items():
        out.append(array(f"{bank.upper()}_CLOBBER_PREFIXES", "char", prefixes))
    for bank, prefixes in description.asm_views.items():
        out.append(array(f"{bank.upper()}_ASM_VIEWS", "char", prefixes))
    for bank, prefix in description.binding_prefixes.items():
        out.append(constant(f"{bank.upper()}_BINDING_PREFIX", "char", repr(prefix)))
    return "".join(out)


def render_evoir_primitives(description: Description) -> str:
    sets = resolve_sets(description)
    out = [header(description)]
    visibility = "pub(super)"
    out.append(array("GPR_BINDINGS", "u8", sets["FIXED_ASM_GPR"], visibility))
    simd = description.banks["simd"]
    out.append(array("SIMD_BINDING_INDICES", "u8", (number - simd.first for number in sets["FIXED_ASM_SIMD"]), visibility))
    out.append(constant("SIMD_BASE", "u8", str(simd.first), visibility))
    for name, bank in description.banks.items():
        out.append(constant(f"{name.upper()}_LAST_INDEX", "u8", str(bank.count - 1), visibility))
    for bank, prefixes in description.clobber_aliases.items():
        out.append(array(f"{bank.upper()}_CLOBBER_PREFIXES", "char", prefixes, visibility))
    for bank, prefix in description.binding_prefixes.items():
        out.append(constant(f"{bank.upper()}_BINDING_PREFIX", "char", repr(prefix), visibility))
    return "".join(out)


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, default=root / "targets/aarch64/regfile")
    parser.add_argument("--output", type=Path, default=root / "crates/evocc-mir/src/isa/aarch64/regfile.rs")
    parser.add_argument("--evoir-output", type=Path, default=root / "crates/evocc-evoir/src/assembly_registers.rs")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        description = parse_description(args.input.read_bytes())
        validate(description)
        outputs = {
            args.output: render_mir(description).encode("utf-8"),
            args.evoir_output: render_evoir_primitives(description).encode("utf-8"),
        }
        if args.check:
            for path, expected in outputs.items():
                if not path.is_file() or path.read_bytes() != expected:
                    raise TargetDescriptionError(f"generated file differs: {path}")
        else:
            for path, contents in outputs.items():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(contents)
    except (OSError, TargetDescriptionError) as error:
        print(f"generate-target: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
