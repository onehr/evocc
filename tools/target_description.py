from __future__ import annotations

import hashlib
import re
from dataclasses import dataclass, field


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
    names: tuple[str, ...] = ()


@dataclass(frozen=True)
class Role:
    bank: str
    number: int


@dataclass
class Description:
    digest: str
    target: str = "aarch64"
    banks: dict[str, Bank] = field(default_factory=dict)
    views: dict[str, View] = field(default_factory=dict)
    aliases: list[tuple[str, str]] = field(default_factory=list)
    set_tokens: dict[str, tuple[str, ...]] = field(default_factory=dict)
    roles: dict[str, Role | None] = field(default_factory=dict)
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


def parse_description(source: bytes, target: str = "aarch64") -> Description:
    try:
        text = source.decode("ascii")
    except UnicodeDecodeError as error:
        raise TargetDescriptionError("description must be canonical ASCII") from error
    if not text.endswith("\n") or "\r" in text or "\t" in text:
        raise TargetDescriptionError("description requires canonical LF lines")
    lines = text.splitlines()
    if len(lines) < 3 or lines[:2] != ["version regfile.v1", f"target {identifier(target)}"]:
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
    description = Description(stated, target=target)
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
        elif kind == "view-names" and len(args) >= 4:
            name, bank, bits, *names = args
            insert(description.views, identifier(name), View(identifier(bank), number(bits), "", tuple(names)), "view")
        elif kind == "role" and len(args) == 2 and args[1] == "none":
            insert(description.roles, identifier(args[0]), None, "role")
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
        if view.bits == 0:
            raise TargetDescriptionError(f"invalid register view: {name}")
        if view.names:
            if len(view.names) != description.banks[view.bank].count or len(set(view.names)) != len(view.names):
                raise TargetDescriptionError(f"invalid named register view: {name}")
            for spelling in view.names:
                identifier(spelling)
        elif not re.fullmatch("[a-z]", view.prefix):
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
    if set(description.banks) != required_banks or not required_views <= description.views.keys():
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
        if role is None:
            continue
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
    required_roles = ("SCRATCH", "SCRATCH_UNIT", "SIMD_SCRATCH", "SIMD_SCRATCH_UNIT", "FRAME_POINTER")
    if description.target == "aarch64":
        required_roles += ("LINK_REGISTER", "PLATFORM_REGISTER")
    for required in required_roles:
        if description.roles.get(required) is None:
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
        f"// Source: targets/{description.target}/regfile; sha256 {description.digest}.\n\n"
    )


def render_mir(description: Description) -> str:
    sets = resolve_sets(description)
    types = "RegisterBank, RegisterView"
    if any(view.names for view in description.views.values()):
        types += ", NamedRegisterView"
    out = [header(description), f"use crate::generic::register::{{{types}}};\n\n"]
    out.append(constant("DESCRIPTION_DIGEST", "&str", f'"{description.digest}"'))
    for name, bank in description.banks.items():
        out.append(f"\npub const {name.upper()}_BANK: RegisterBank = RegisterBank {{\n    first: {bank.first},\n    count: {bank.count},\n}};\n")
    for name, view in description.views.items():
        if view.names:
            names = ", ".join(f'"{spelling}"' for spelling in view.names)
            out.append(f'\n#[rustfmt::skip]\npub const {name.upper()}_VIEW: NamedRegisterView = NamedRegisterView {{\n    bank: {view.bank.upper()}_BANK,\n    bits: {view.bits},\n    names: &[{names}],\n}};\n')
            continue
        out.append(f'\npub const {name.upper()}_VIEW: RegisterView = RegisterView {{\n    bank: {view.bank.upper()}_BANK,\n    bits: {view.bits},\n    prefix: "{view.prefix}",\n}};\n')
    out.append("\n")
    for name, members in sets.items():
        out.append(array(name, "u8", members))
    out.append(array("GPR_ALLOCATABLE", "u8", (*sets["CALLER_SAVED"], *sets["CALLEE_SAVED"])))
    out.append(array("CALL_CLOBBERS", "u8", (*sets["GPR_CALL_CLOBBERS"], *sets["SIMD_CALL_CLOBBERS"])))
    simd = description.banks["simd"]
    out.append(array("SIMD_LOW64_PRESERVED_INDICES", "u8", (number - simd.first for number in sets["SIMD_LOW64_PRESERVED"])))
    out.append(constant("SIMD_BASE", "u8", str(simd.first)))
    for name, role in description.roles.items():
        if role is None:
            out.append(constant(name, "Option<u8>", "None"))
            continue
        out.append(constant(name, "u8", str(role.number)))
        bank = description.banks[role.bank]
        for view_name, view in description.views.items():
            if view.bank == role.bank and view.names:
                spelling = view.names[role.number - bank.first]
                out.append(constant(f"{name}_{view_name.upper()}", "&str", f'"{spelling}"'))
        prefixes = [view.prefix for view in description.views.values() if view.bank == role.bank and not view.names]
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



def parse_envelope(source: bytes, version: str, target: str = "aarch64") -> tuple[str, list[str]]:
    """The canonical byte envelope shared by every target component."""
    try:
        text = source.decode("ascii")
    except UnicodeDecodeError as error:
        raise TargetDescriptionError("description must be canonical ASCII") from error
    if not text.endswith("\n") or "\r" in text or "\t" in text:
        raise TargetDescriptionError("description requires canonical LF lines")
    lines = text.splitlines()
    if len(lines) < 3 or lines[:2] != [f"version {version}", f"target {identifier(target)}"]:
        raise TargetDescriptionError(f"unsupported {version} version or target")
    digest_rows = [line for line in lines if line.startswith("sha256 ")]
    if len(digest_rows) != 1 or lines[2] != digest_rows[0]:
        raise TargetDescriptionError("expected one canonical digest row")
    digest = digest_rows[0].removeprefix("sha256 ")
    payload = "".join(line + "\n" for line in lines if not line.startswith("sha256 "))
    if not re.fullmatch(r"[0-9a-f]{64}", digest) or hashlib.sha256(payload.encode()).hexdigest() != digest:
        raise TargetDescriptionError(f"{version} description digest mismatch")
    for line in lines[3:]:
        if not line or any(not part for part in line.split(" ")):
            raise TargetDescriptionError("noncanonical fact spacing")
    return digest, lines[3:]


def quantity(token: str) -> int:
    if not re.fullmatch(r"0|[1-9][0-9]*", token) or int(token) >= 2**32:
        raise TargetDescriptionError(f"noncanonical quantity: {token}")
    return int(token)


ABI_SINGLE = (
    "abi", "regfile", "indirect-result", "result-overflow", "stack-slot-bytes",
    "stack-alignment-bytes", "stack-arguments", "callee-saved", "call-clobbers",
    "partially-preserved", "frame-record-bytes", "save-slot-bytes", "variadic-save-alignment",
    "record-register-bytes", "aligned-record-pair-rounding",
)
ABI_PER_BANK = ("arguments", "results", "variadic-save")
VA_LIST_FIELDS = ("stack", "general-top", "vector-top", "general-offset", "vector-offset")
PROFILE_FEATURES = (
    "function-sections", "data-sections", "explicit-sections", "tls", "patchable-entry",
    "protected-visibility",
)


@dataclass
class AbiDescription:
    digest: str
    facts: dict = field(default_factory=dict)


def parse_abi(source: bytes, target: str = "aarch64") -> AbiDescription:
    digest, lines = parse_envelope(source, "abi.v1", target)
    description = AbiDescription(digest)
    facts = description.facts
    for line in lines:
        kind, *args = line.split(" ")
        if kind in ABI_PER_BANK or kind == "va-list":
            if len(args) < 2 or (kind in ("variadic-save", "va-list") and len(args) != 2):
                raise TargetDescriptionError(f"wrong ABI fact arity: {line}")
            if not re.fullmatch(r"[a-z][a-z0-9-]*", args[0]):
                raise TargetDescriptionError(f"invalid ABI fact key: {line}")
            rows = facts.setdefault(kind, {})
            if args[0] in rows:
                raise TargetDescriptionError(f"duplicate ABI fact: {kind} {args[0]}")
            if kind in ("arguments", "results") and args[1] == "list":
                rows[args[0]] = tuple(number(value) for value in args[2:])
            else:
                if len(args) != 2:
                    raise TargetDescriptionError(f"wrong ABI fact arity: {line}")
                rows[args[0]] = quantity(args[1])
        elif kind in ABI_SINGLE:
            if not args:
                raise TargetDescriptionError(f"wrong ABI fact arity: {line}")
            if kind in facts:
                raise TargetDescriptionError(f"duplicate ABI fact: {kind}")
            facts[kind] = tuple(args)
        else:
            raise TargetDescriptionError(f"unknown ABI fact: {line}")
    for kind in (*ABI_SINGLE, *ABI_PER_BANK, "va-list"):
        if kind not in facts:
            raise TargetDescriptionError(f"missing ABI fact: {kind}")
    for name in VA_LIST_FIELDS:
        if name not in facts["va-list"]:
            raise TargetDescriptionError(f"missing va-list field: {name}")
    if set(facts["va-list"]) != set(VA_LIST_FIELDS):
        raise TargetDescriptionError("unexpected va-list field")
    return description


def single(abi: AbiDescription, kind: str, arity: int = 1) -> tuple[str, ...]:
    values = abi.facts[kind]
    if len(values) != arity:
        raise TargetDescriptionError(f"wrong ABI fact arity: {kind}")
    return values


def register_set(tokens: tuple[str, ...], sets: dict[str, tuple[int, ...]]) -> tuple[int, ...]:
    values: list[int] = []
    for token in tokens:
        if token.startswith("@"):
            if token[1:] not in sets:
                raise TargetDescriptionError(f"unknown register set: {token[1:]}")
            values.extend(sets[token[1:]])
        else:
            values.append(number(token))
    if len(values) != len(set(values)):
        raise TargetDescriptionError("duplicate register in ABI set")
    return tuple(values)


def abi_register(abi: AbiDescription, kind: str, regfile: Description, reserved: set[int]) -> int | None:
    if abi.facts[kind] == ("none",):
        return None
    bank_name, index = single(abi, kind, 2)
    if bank_name not in regfile.banks:
        raise TargetDescriptionError(f"{kind} names an unknown bank")
    bank = regfile.banks[bank_name]
    if number(index) >= bank.count:
        raise TargetDescriptionError(f"{kind} register exceeds its bank")
    physical = bank.first + number(index)
    if physical in reserved:
        raise TargetDescriptionError(f"{kind} register {physical} is reserved")
    return physical


def abi_values(abi: AbiDescription, regfile: Description) -> dict:
    """Validated, resolved values in regfile bank order."""
    facts = abi.facts
    sets = resolve_sets(regfile)
    reserved = set(sets["RESERVED"])
    if single(abi, "regfile") != (regfile.digest,):
        raise TargetDescriptionError("ABI names a different register description revision")
    name = single(abi, "abi")[0]
    identifier(name)
    banks = list(regfile.banks)
    values = {"name": name, "argument_banks": [], "result_banks": [], "variadic_save_bytes": []}
    for kind in ABI_PER_BANK:
        if set(facts[kind]) != set(banks):
            raise TargetDescriptionError(f"{kind} must name every register bank")
    for bank_name in banks:
        bank = regfile.banks[bank_name]
        for kind, key in (("arguments", "argument_banks"), ("results", "result_banks")):
            members = facts[kind][bank_name]
            if isinstance(members, int):
                if members == 0 or members > bank.count:
                    raise TargetDescriptionError(f"{kind} registers exceed their bank: {bank_name}")
                members = tuple(range(bank.first, bank.first + members))
            if len(set(members)) != len(members):
                raise TargetDescriptionError(f"duplicate {kind} register: {bank_name}")
            if any(not bank.contains(member) for member in members):
                raise TargetDescriptionError(f"{kind} registers exceed their bank: {bank_name}")
            if reserved & set(members):
                raise TargetDescriptionError(f"{kind} registers are reserved: {bank_name}")
            values[key].append(members)
        if facts["variadic-save"][bank_name] == 0:
            raise TargetDescriptionError("variadic save width must be positive")
        values["variadic_save_bytes"].append(facts["variadic-save"][bank_name])
    values["indirect_result"] = abi_register(abi, "indirect-result", regfile, reserved)
    values["result_overflow"] = abi_register(abi, "result-overflow", regfile, reserved)
    for kind in ("stack-slot-bytes", "stack-alignment-bytes", "frame-record-bytes", "save-slot-bytes",
                 "variadic-save-alignment", "record-register-bytes"):
        values[kind.replace("-", "_")] = quantity(single(abi, kind)[0])
        if values[kind.replace("-", "_")] == 0:
            raise TargetDescriptionError(f"{kind} must be positive")
    if values["stack_alignment_bytes"] % values["stack_slot_bytes"]:
        raise TargetDescriptionError("stack alignment is not a whole number of slots")
    for kind, choices in (("stack-arguments", ("admitted", "refused")),
                          ("aligned-record-pair-rounding", ("true", "false"))):
        if single(abi, kind)[0] not in choices:
            raise TargetDescriptionError(f"invalid {kind} fact")
    values["stack_arguments"] = single(abi, "stack-arguments")[0] == "admitted"
    values["aligned_record_pair_rounding"] = single(abi, "aligned-record-pair-rounding")[0] == "true"
    values["callee_saved"] = register_set(facts["callee-saved"], sets)
    values["call_clobbers"] = register_set(facts["call-clobbers"], sets)
    partial = facts["partially-preserved"]
    if len(partial) < 2:
        raise TargetDescriptionError("wrong ABI fact arity: partially-preserved")
    values["partially_preserved"] = register_set(partial[:-1], sets)
    values["partially_preserved_bytes"] = quantity(partial[-1])
    if set(values["callee_saved"]) & set(values["call_clobbers"]):
        raise TargetDescriptionError("callee-saved registers cannot be call clobbered")
    if set(values["partially_preserved"]) & (set(values["call_clobbers"]) | set(values["callee_saved"])):
        raise TargetDescriptionError("partially preserved registers overlap another preservation class")
    values["va_list"] = [facts["va-list"][name] for name in VA_LIST_FIELDS]
    return values


def validate_abi(abi: AbiDescription, regfile: Description):
    abi_values(abi, regfile)


@dataclass
class ProfileDescription:
    digest: str
    name: str = ""
    host_abi: tuple[str, str] = ("", "")
    host_only: list[str] = field(default_factory=list)
    stack_guard_slot_bytes: int = 0


def parse_profile(source: bytes, target: str = "aarch64") -> ProfileDescription:
    digest, lines = parse_envelope(source, "profile.v1", target)
    profile = ProfileDescription(digest)
    seen: set[str] = set()
    for line in lines:
        kind, *args = line.split(" ")
        if kind in ("profile", "abi", "stack-guard-slot-bytes") and kind in seen:
            raise TargetDescriptionError(f"duplicate profile fact: {kind}")
        seen.add(kind)
        if kind == "profile" and len(args) == 1:
            profile.name = identifier(args[0])
        elif kind == "abi" and len(args) == 2:
            profile.host_abi = (identifier(args[0]), args[1])
        elif kind == "host-only" and len(args) == 1:
            if args[0] in profile.host_only:
                raise TargetDescriptionError(f"duplicate profile feature: {args[0]}")
            profile.host_only.append(args[0])
        elif kind == "stack-guard-slot-bytes" and len(args) == 1:
            profile.stack_guard_slot_bytes = quantity(args[0])
        else:
            raise TargetDescriptionError(f"unknown profile fact or wrong arity: {line}")
    for kind in ("profile", "abi", "stack-guard-slot-bytes"):
        if kind not in seen:
            raise TargetDescriptionError(f"missing profile fact: {kind}")
    return profile


def validate_profile(profile: ProfileDescription, abis: dict):
    name, revision = profile.host_abi
    if name not in abis:
        raise TargetDescriptionError(f"profile names an unknown ABI: {name}")
    if abis[name].digest != revision:
        raise TargetDescriptionError("profile names a different ABI revision")
    for feature in profile.host_only:
        if feature not in PROFILE_FEATURES:
            raise TargetDescriptionError(f"unknown profile feature: {feature}")
    alignment = quantity(single(abis[name], "stack-alignment-bytes")[0])
    if profile.stack_guard_slot_bytes == 0 or profile.stack_guard_slot_bytes % alignment:
        raise TargetDescriptionError("stack guard slot must preserve the stack alignment")


def abi_field_expressions(prefix: str, values: dict) -> list[tuple[str, str]]:
    fields = [("name", '"'+values["name"]+'"')]
    for key in ("argument_banks", "result_banks"):
        fields.append((key, "&"+prefix+"_"+key.upper()))
    for key in ("indirect_result", "result_overflow"):
        value = values[key]
        fields.append((key, "None" if value is None else f"Some({value})"))
    for key in ("stack_slot_bytes", "stack_alignment_bytes", "stack_arguments",
                "callee_saved", "call_clobbers", "partially_preserved", "partially_preserved_bytes",
                "frame_record_bytes", "save_slot_bytes", "variadic_save_bytes", "variadic_save_alignment"):
        value = values[key]
        if key in ("callee_saved", "call_clobbers", "partially_preserved"):
            value = "&"+prefix+"_"+key.upper()
        elif key == "variadic_save_bytes":
            value = "&["+", ".join(map(str, value))+"]"
        elif isinstance(value, bool):
            value = str(value).lower()
        fields.append((key, str(value)))
    return fields


def render_abi(regfile: Description, abis: list[AbiDescription], profiles: list[ProfileDescription]) -> str:
    by_name = {abi.facts["abi"][0]: abi for abi in abis}
    for profile in profiles:
        validate_profile(profile, by_name)
    wrapped = regfile.target == "aarch64"
    out = ["// Generated by tools/generate-target.py; do not edit.\n"]
    for abi in abis:
        out.append(f"// Source: targets/{regfile.target}/abi-{abi.facts['abi'][0]}; sha256 {abi.digest}.\n")
    for profile in profiles:
        out.append(f"// Source: targets/{regfile.target}/profile-{profile.name}; sha256 {profile.digest}.\n")
    if wrapped:
        out.append("\nuse super::abi::{Aarch64Abi, Profile, VaListLayout};\n")
    elif profiles:
        out.append("\nuse super::abi::Profile;\n")
    out.append("use crate::generic::abi::AbiFacts;\nuse crate::generic::register::RegisterList;\n\n")
    for abi in abis:
        out.append(constant(f"{abi.facts['abi'][0].upper()}_DIGEST", "&str", f'"{abi.digest}"'))
    for profile in profiles:
        out.append(constant(f"{profile.name.upper()}_DIGEST", "&str", f'"{profile.digest}"'))
    for abi in abis:
        values = abi_values(abi, regfile)
        prefix = values["name"].upper()
        for key in ("argument_banks", "result_banks"):
            out.append(f"\n#[rustfmt::skip]\nconst {prefix}_{key.upper()}: [RegisterList; {len(values[key])}] = [\n")
            for members in values[key]:
                out.append("    RegisterList(&[" + ", ".join(map(str, members)) + "]),\n")
            out.append("];\n")
        out.append("\n")
        for key in ("callee_saved", "call_clobbers", "partially_preserved"):
            out.append(array(f"{prefix}_{key.upper()}", "u8", values[key], "pub(super)"))
        ty = "Aarch64Abi" if wrapped else "AbiFacts"
        out.append(f"\n#[rustfmt::skip]\npub const {prefix}: {ty} = {ty} {{\n")
        if wrapped:
            out.append("    facts: AbiFacts {\n")
        indent = "        " if wrapped else "    "
        for key, value in abi_field_expressions(prefix, values):
            out.append(f"{indent}{key}: {value},\n")
        if wrapped:
            out.append("    },\n")
            out.append(f"    record_register_bytes: {values['record_register_bytes']},\n")
            out.append(f"    aligned_record_pair_rounding: {str(values['aligned_record_pair_rounding']).lower()},\n")
            out.append("    va_list: VaListLayout {\n")
            for name, value in zip(VA_LIST_FIELDS, values["va_list"]):
                out.append(f"        {name.replace('-', '_')}: {value},\n")
            out.append("    },\n")
        out.append("};\n")
    for profile in profiles:
        features = "".join(f"        \"{feature}\",\n" for feature in profile.host_only)
        out.append(
            f"\n#[rustfmt::skip]\npub const {profile.name.upper()}: Profile = Profile {{\n"
            f"    name: \"{profile.name}\",\n"
            f"    host_abi: \"{profile.host_abi[0]}\",\n"
            f"    host_only: &[\n{features}    ],\n"
            f"    stack_guard_slot_bytes: {profile.stack_guard_slot_bytes},\n"
            "};\n"
        )
    return "".join(out)
