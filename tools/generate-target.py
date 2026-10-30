#!/usr/bin/env python3
"""Generate register and instruction facts from canonical target descriptions."""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path


_description_spec = importlib.util.spec_from_file_location(
    "evocc_target_description", Path(__file__).with_name("target_description.py"))
_description_module = importlib.util.module_from_spec(_description_spec)
sys.modules[_description_spec.name] = _description_module
_description_spec.loader.exec_module(_description_module)
# The import resolves through sys.modules, so tools/ need not be on sys.path.
from evocc_target_description import (  # noqa: E402
    TargetDescriptionError, constant, identifier, insert, parse_abi, parse_description,
    parse_envelope, parse_profile, quantity, render_abi, render_evoir_primitives, render_mir,
    validate, validate_abi,
)
from evocc_target_description import abi_values, validate_profile  # noqa: E402,F401  for the tests


# The structural shapes below mirror only the fields present in the current
# enums. Operand roles and ordering live in insts, not in these field types.
INSTRUCTION_SHAPES = {
    "Floating": "dst:r args:v",
    "Aes": "dst:r args:v",
    "AtomicRmw": "dst:r base:r value:r expected:o",
    "Vector": "dst:r args:v",
    "Atomic": "load:b value:r base:r",
    "Mov": "dst:r src:r",
    "MovImm": "dst:r",
    "Adrp": "dst:r",
    "AddPageOff": "dst:r src:r",
    "Adr": "dst:r",
    "Load": "dst:r base:r",
    "Store": "src:r base:r",
    "Binary": "dst:r lhs:r rhs:r",
    "BinaryImm": "dst:r lhs:r",
    "FlagBinary": "dst:r flags:r lhs:r rhs:r",
    "WideMultiply": "dst:r lhs:r rhs:r",
    "Msub": "dst:r n:r m:r a:r",
    "Madd": "dst:r n:r m:r a:r",
    "CarryBinary": "dst:r flags:r lhs:r rhs:r",
    "AddCarryFlags": "dst:r flags_in:r lhs:r rhs:r",
    "SubCarryFlags": "dst:r flags_in:r lhs:r rhs:r",
    "TstImm": "dst:r lhs:r",
    "Unary": "dst:r src:r",
    "Extend": "dst:r src:r",
    "Bfx": "dst:r src:r",
    "Cmp": "dst:r lhs:r rhs:r",
    "CmpImm": "dst:r lhs:r",
    "Cset": "dst:r flags:r",
    "Csel": "dst:r flags:r lhs:r rhs:r",
    "Bl": "args:v results:v",
    "Blr": "target:r args:v results:v",
    "Ldp": "rt1:r rt2:r base:r",
    "Stp": "rt1:r rt2:r base:r",
    "SpillStore": "src:r slot:s",
    "SpillReload": "dst:r slot:s",
    "IncomingArg": "dst:r slot:s",
    "OutgoingArg": "src:r slot:s",
    "MemCopy": "dst:r src:r",
    "FrameSlot": "dst:r",
    "StackAlloc": "dst:r size:r",
    "StackRelease": "address:r",
    "FrameAddress": "dst:r",
    "IndirectResult": "",
    "FpEnvCheck": "",
    "VaSaveArea": "",
    "Asm": "body:a",
}
TERMINATOR_SHAPES = {
    "Indirect": "target:r edges:e",
    "B": "target:t args:v",
    "BCond": "flags:r taken:t taken_args:v untaken:t untaken_args:v",
    "Cb": "src:r taken:t taken_args:v untaken:t untaken_args:v",
    "Tb": "src:r taken:t taken_args:v untaken:t untaken_args:v",
    "Ret": "values:v",
    "Brk": "",
    "AsmGoto": "body:a labels:e fallthrough:t fallthrough_args:v",
}
INST_COLUMNS = ("defs", "uses", "rewrite_defs", "rewrite_uses", "flags", "discardable", "call", "copy", "fixed", "call_operands", "assembly", "exclusive", "indirect_result", "spill", "incoming", "outgoing")
TERM_COLUMNS = ("uses", "rewrite_uses", "edges", "exit", "indirect", "forwarding", "jump", "assembly", "returns", "spill_bridge")
ASSEMBLY_FILTERS = {
    "reg": None,
    "nonmemory": "!matches!(operand.constraint, AsmConstraint::Memory { .. })",
    "read": (
        "matches!(operand.constraint, AsmConstraint::Memory { .. })"
        " || operand.constraint.direction() == AsmDirection::ReadWrite"
    ),
    "definition": (
        "!matches!(operand.constraint, AsmConstraint::Memory { .. })"
        " && operand.constraint.direction().writes()"
    ),
    "nondefinition": (
        "matches!(operand.constraint, AsmConstraint::Memory { .. })"
        " || !operand.constraint.direction().writes()"
    ),
}


@dataclass
class InstructionDescription:
    digest: str
    instructions: dict[str, dict[str, str]] = field(default_factory=dict)
    terminators: dict[str, dict[str, str]] = field(default_factory=dict)


def parse_instructions(source: bytes, target: str = "aarch64") -> InstructionDescription:
    # Keep the same canonical byte envelope as the register component.
    try:
        text = source.decode("ascii")
    except UnicodeDecodeError as error:
        raise TargetDescriptionError("description must be canonical ASCII") from error
    lines = text.splitlines()
    if not text.endswith("\n") or "\r" in text or "\t" in text:
        raise TargetDescriptionError("description requires canonical LF lines")
    if len(lines) < 3 or lines[:2] != ["version insts.v1", f"target {identifier(target)}"]:
        raise TargetDescriptionError("unsupported instruction version or target")
    digest_rows = [line for line in lines if line.startswith("sha256 ")]
    if len(digest_rows) != 1 or lines[2] != digest_rows[0]:
        raise TargetDescriptionError("expected one canonical digest row")
    digest = digest_rows[0].removeprefix("sha256 ")
    payload = "".join(line + "\n" for line in lines if not line.startswith("sha256 "))
    if not re.fullmatch(r"[0-9a-f]{64}", digest) or hashlib.sha256(payload.encode()).hexdigest() != digest:
        raise TargetDescriptionError("instruction description digest mismatch")
    description = InstructionDescription(digest)
    for line in lines[3:]:
        parts = line.split(" ")
        if any(not part for part in parts) or len(parts) < 3:
            raise TargetDescriptionError("noncanonical instruction fact")
        kind, name, *values = parts
        if kind == "inst":
            columns, destination = INST_COLUMNS, description.instructions
        elif kind == "term":
            columns, destination = TERM_COLUMNS, description.terminators
        else:
            raise TargetDescriptionError(f"unknown instruction fact: {kind}")
        if len(values) != len(columns):
            raise TargetDescriptionError(f"wrong descriptor arity: {name}")
        fields = {}
        for column, token in zip(columns, values):
            key, equals, value = token.partition("=")
            if key != column or equals != "=" or not value:
                raise TargetDescriptionError(f"expected descriptor column {column}: {name}")
            fields[column] = value
        insert(destination, identifier(name), fields, "instruction family")
    return description


def operand_parts(selector: str) -> list[str]:
    return [] if selector == "-" else selector.split(",")


def validate_operand(selector: str, shape: dict[str, str]):
    for item in operand_parts(selector):
        if item.startswith("body."):
            pieces = item.split(".")
            if (len(pieces) != 3 or shape.get("body") != "a"
                    or pieces[1] not in ("outputs", "inputs") or pieces[2] not in ASSEMBLY_FILTERS):
                raise TargetDescriptionError(f"invalid operand selector: {item}")
        elif item.endswith(".args*"):
            if shape.get(item.removesuffix(".args*")) != "e":
                raise TargetDescriptionError(f"invalid operand selector shape: {item}")
        elif "@" in item:
            field_name, condition = item.split("@", 1)
            if shape.get(field_name) != "r" or shape.get("load") != "b" or condition not in ("load", "store"):
                raise TargetDescriptionError(f"invalid conditional operand selector: {item}")
        else:
            suffix = item[-1:] if item[-1:] in ("*", "?") else ""
            field_name = item[:-1] if suffix else item
            expected = {"": "r", "*": "v", "?": "o"}[suffix]
            if shape.get(field_name) != expected:
                raise TargetDescriptionError(f"invalid operand field or selector shape: {item}")


def validate_edges(selector: str, shape: dict[str, str]):
    for item in operand_parts(selector):
        if item.endswith("*"):
            valid = shape.get(item[:-1]) == "e"
        else:
            parts = item.split(":")
            valid = len(parts) == 2 and shape.get(parts[0]) == "t" and shape.get(parts[1]) == "v"
        if not valid:
            raise TargetDescriptionError(f"invalid edge selector: {item}")


def validate_instructions(description: InstructionDescription):
    for rows, shapes in (
        (description.instructions, INSTRUCTION_SHAPES),
        (description.terminators, TERMINATOR_SHAPES),
    ):
        if set(rows) != set(shapes):
            raise TargetDescriptionError("missing or unexpected instruction family")
        if list(rows) != list(shapes):
            raise TargetDescriptionError("noncanonical instruction family order")
        for name, row in rows.items():
            shape = dict(token.split(":") for token in shapes[name].split())
            for key in ("defs", "uses", "rewrite_defs", "rewrite_uses"):
                if key in row:
                    validate_operand(row[key], shape)
            for key in ("discardable", "call", "exit", "indirect_result", "spill_bridge"):
                if key in row and row[key] not in ("true", "false"):
                    raise TargetDescriptionError(f"invalid {key} fact: {name}")
            if "forwarding" in row and row["forwarding"] not in ("true", "false"):
                if row["forwarding"] != "body.fixed" or shape.get("body") != "a":
                    raise TargetDescriptionError(f"invalid forwarding fact: {name}")
            if "flags" in row and row["flags"] not in ("true", "false"):
                if row["flags"] != "body.flags" or shape.get("body") != "a":
                    raise TargetDescriptionError(f"invalid flags fact: {name}")
            if "edges" in row:
                validate_edges(row["edges"], shape)
            if "fixed" in row and row["fixed"] != "false":
                if row["fixed"] != "body.fixed" or shape.get("body") != "a":
                    raise TargetDescriptionError(f"invalid fixed binding fact: {name}")
            if "indirect" in row and row["indirect"] != "-":
                if shape.get(row["indirect"]) != "r":
                    raise TargetDescriptionError(f"invalid indirect target: {name}")
            for key, kind in (("assembly", "a"), ("exclusive", "r"), ("returns", "v")):
                if key in row and row[key] != "-" and shape.get(row[key]) != kind:
                    raise TargetDescriptionError(f"invalid {key} view: {name}")
            for key, kinds in (("call_operands", ("v", "v")), ("incoming", ("r", "s")), ("outgoing", ("r", "s"))):
                if key in row and row[key] != "-":
                    fields = row[key].split(":")
                    if len(fields) != 2 or tuple(shape.get(field) for field in fields) != kinds:
                        raise TargetDescriptionError(f"invalid {key} view: {name}")
            if "spill" in row and row["spill"] != "-":
                fields = row["spill"].split(":")
                if (len(fields) != 3 or fields[0] not in ("store", "reload")
                        or shape.get(fields[1]) != "r" or shape.get(fields[2]) != "s"):
                    raise TargetDescriptionError(f"invalid spill view: {name}")
            for key, kinds in (("copy", ("r", "r")), ("jump", ("t", "v"))):
                if key in row and row[key] != "-":
                    fields = row[key].split(":")
                    if (len(fields) != 2 or len(set(fields)) != 2
                            or tuple(shape.get(field) for field in fields) != kinds
                            or set(fields) != set(shape)):
                        raise TargetDescriptionError(f"invalid {key} constructor: {name}")
    for rows, key in ((description.instructions, "copy"), (description.terminators, "jump")):
        if sum(row[key] != "-" for row in rows.values()) != 1:
            raise TargetDescriptionError(f"expected exactly one {key} constructor")


def bound_fields(selector: str) -> set[str]:
    fields = set()
    for item in operand_parts(selector):
        if item.startswith("body."):
            fields.add("body")
        elif "@" in item:
            fields.update((item.split("@", 1)[0], "load"))
        elif ":" in item:
            fields.update(item.split(":"))
        else:
            fields.add(item.split(".", 1)[0].rstrip("*?"))
    return fields


def inst_pattern(name: str, fields: set[str], enum: str = "Self") -> str:
    if name == "Asm":
        return f"{enum}::Asm({'body' if fields else '_'})"
    if name == "IndirectResult":
        return f"{enum}::IndirectResult"
    members = "".join(name + ", " for name in sorted(fields))
    return f"{enum}::{name} {{ {members}.. }}"


def operand_iterator(item: str, mutable: bool) -> str:
    if item.startswith("body."):
        _, collection, mode = item.split(".")
        iterator = f"body.{collection}.iter" + ("_mut()" if mutable else "()")
        predicate = ASSEMBLY_FILTERS[mode]
        if predicate:
            iterator += f".filter(|operand| {predicate})"
        if mutable:
            return iterator + ".filter_map(|operand| match &mut operand.value { AsmValue::Reg(reg) => Some(reg), _ => None })"
        return iterator + ".filter_map(AsmOperand::register)"
    if item.endswith(".args*"):
        field_name = item.removesuffix(".args*")
        if mutable:
            return f"{field_name}.iter_mut().flat_map(|(_, args)| args.iter_mut())"
        return f"{field_name}.iter().flat_map(|(_, args)| args.iter().copied())"
    if "@" in item:
        field_name, condition = item.split("@", 1)
        test = "*load" if condition == "load" else "!*load"
        value = field_name if mutable else "*" + field_name
        return f"({test}).then_some({value})"
    if item.endswith(("*", "?")):
        field_name = item[:-1]
        return f"{field_name}.iter_mut()" if mutable else f"{field_name}.iter().copied()"
    return f"std::iter::once({item if mutable else '*' + item})"


def collection_name(stem: str, selector: str) -> str:
    """Keep generated temporaries disjoint from the matched enum fields."""
    fields = bound_fields(selector)
    while stem in fields:
        stem += "_"
    return stem


def operand_expression(selector: str, mutable: bool, indent: str = "            ") -> str:
    parts = operand_parts(selector)
    if not parts:
        return "Vec::new()"
    if all(re.fullmatch("[a-z][a-z0-9_]*", part) for part in parts):
        return "vec![" + ", ".join(part if mutable else "*" + part for part in parts) + "]"
    # Stable generated formatting avoids invoking a host formatter as part of
    # generation. The item carrying this expression is rustfmt::skip.
    inner = indent + "    "
    collected = collection_name("collected_operands", selector)
    lines = ["{", inner + f"let mut {collected} = Vec::new();"]
    for item in parts:
        lines.append(inner + f"{collected}.extend({operand_iterator(item, mutable)});")
    lines.extend((inner + collected, indent + "}"))
    return "\n".join(lines)


def render_operand_method(name: str, column: str, rows: dict, mutable: bool, enum: str) -> str:
    receiver = "&mut self" if mutable else "&self"
    ty = "&mut Self::Reg" if mutable else "Self::Reg"
    out = [f"    fn {name}({receiver}) -> Vec<{ty}> {{\n        match self {{\n"]
    for family, row in rows.items():
        selector = row[column]
        pattern = inst_pattern(family, bound_fields(selector), enum)
        if family == "Asm":
            helper = {"defs": "assembly_defs", "uses": "assembly_uses",
                      "rewrite_defs": "assembly_rewrite_defs", "rewrite_uses": "assembly_rewrite_uses"}[column]
            expression = f"{helper}(body)"
        else:
            expression = operand_expression(selector, mutable)
        out.append(f"            {pattern} => {expression},\n")
    out.append("        }\n    }\n")
    return "".join(out)


def render_slot_method(rows: dict) -> str:
    out = ["    fn operand_slots(&mut self, definitions: bool) -> Vec<&mut Self::Reg> {\n"
           "        if definitions {\n            match self {\n"]
    for column in ("rewrite_defs", "rewrite_uses"):
        for family, row in rows.items():
            selector = row[column]
            pattern = inst_pattern(family, bound_fields(selector))
            helper = "assembly_rewrite_defs" if column == "rewrite_defs" else "assembly_rewrite_uses"
            expression = f"{helper}(body)" if family == "Asm" else operand_expression(selector, True, "                ")
            out.append(f"                {pattern} => {expression},\n")
        out.append("            }\n")
        if column == "rewrite_defs":
            out.append("        } else {\n            match self {\n")
    out.append("        }\n    }\n")
    return "".join(out)


def render_bool_method(name: str, column: str, rows: dict) -> str:
    if all(row[column] in ("true", "false") for row in rows.values()):
        variants = [inst_pattern(family, set()) for family, row in rows.items() if row[column] == "true"]
        expression = "matches!(self, " + " | ".join(variants) + ")" if variants else "false"
        return f"    fn {name}(&self) -> bool {{\n        {expression}\n    }}\n"
    out = [f"    fn {name}(&self) -> bool {{\n        match self {{\n"]
    for family, row in rows.items():
        value = row[column]
        pattern = inst_pattern(family, {"body"} if value in ("body.flags", "body.fixed") else set())
        expression = {"body.flags": "assembly_writes_flags(body)",
                      "body.fixed": "body.numbered().iter().any(|operand| operand.fixed_register.is_some())"}.get(value, value)
        out.append(f"            {pattern} => {expression},\n")
    out.append("        }\n    }\n")
    return "".join(out)


def render_constructor(name: str, column: str, rows: dict, parameters: str, values: tuple[str, str]) -> str:
    family, row = next((family, row) for family, row in rows.items() if row[column] != "-")
    fields = row[column].split(":")
    members = ", ".join(field if field == value else f"{field}: {value}" for field, value in zip(fields, values))
    return f"    fn {name}({parameters}) -> Self {{\n        Self::{family} {{ {members} }}\n    }}\n"


def render_edge_mutation(rows: dict, retarget: bool) -> str:
    if retarget:
        out = ["    fn retarget(&mut self, from: usize, to: u32) {\n"
               "        let mut index = 0;\n"
               "        let mut visit = |target: &mut u32, args: &mut Vec<VReg>| {\n"
               "            if index == from {\n                *target = to;\n                args.clear();\n            }\n"
               "            index += 1;\n        };\n        match self {\n"]
    else:
        out = ["    fn clear_edge_args(&mut self) {\n        match self {\n"]
    for family, row in rows.items():
        parts = operand_parts(row["edges"])
        if retarget and row["indirect"] != "-":
            pattern = inst_pattern(family, set())
            expression = 'unreachable!("computed edges cannot be retargeted without address routing")'
        else:
            fields = set()
            statements = []
            for part in parts:
                if part.endswith("*"):
                    field_name = part[:-1]
                    fields.add(field_name)
                    action = "visit(target, args);" if retarget else "args.clear();"
                    target = "target" if retarget else "_"
                    statements.append(f"for ({target}, args) in {field_name} {{ {action} }}")
                else:
                    target, args = part.split(":")
                    fields.add(args)
                    if retarget:
                        fields.add(target)
                        statements.append(f"visit({target}, {args});")
                    else:
                        statements.append(f"{args}.clear();")
            pattern = inst_pattern(family, fields)
            expression = "{ " + " ".join(statements) + " }" if statements else "{}"
        out.append(f"            {pattern} => {expression},\n")
    out.append("        }\n    }\n")
    return "".join(out)


def render_ssa(description: InstructionDescription) -> str:
    out = ["\n#[rustfmt::skip]\nimpl SsaInst for MInst {\n"]
    out.append(render_constructor("copy", "copy", description.instructions, "dst: Self::Reg, src: Self::Reg", ("dst", "src")))
    out.append("\n    fn has_fixed_binding(&self) -> bool {\n        match self {\n")
    for family, row in description.instructions.items():
        fixed = row["fixed"] == "body.fixed"
        pattern = inst_pattern(family, {"body"} if fixed else set())
        expression = "body.numbered().iter().any(|operand| operand.fixed_register.is_some())" if fixed else "false"
        out.append(f"            {pattern} => {expression},\n")
    out.append("        }\n    }\n}\n")
    out.append("\n#[rustfmt::skip]\nimpl SsaTerm for MTerm {\n")
    out.append("    fn indirect_target(&self) -> Option<Self::Reg> {\n        match self {\n")
    for family, row in description.terminators.items():
        field_name = row["indirect"]
        fields = {field_name} if field_name != "-" else set()
        expression = f"Some(*{field_name})" if fields else "None"
        out.append(f"            {inst_pattern(family, fields)} => {expression},\n")
    out.append("        }\n    }\n")
    out.append("\n    fn set_indirect_target(&mut self, reg: Self::Reg) {\n        match self {\n")
    for family, row in description.terminators.items():
        field_name = row["indirect"]
        fields = {field_name} if field_name != "-" else set()
        expression = f"*{field_name} = reg" if fields else "{}"
        out.append(f"            {inst_pattern(family, fields)} => {expression},\n")
    out.append("        }\n    }\n")
    out.append("\n" + render_edge_mutation(description.terminators, False))
    out.append("\n" + render_edge_mutation(description.terminators, True))
    out.append("\n" + render_constructor("jump", "jump", description.terminators, "target: u32, args: Vec<Self::Reg>", ("target", "args")))
    out.append("\n" + render_bool_method("prevents_forwarding", "forwarding", description.terminators))
    out.append("}\n")
    return "".join(out)


def render_allocation_view(name: str, column: str, rows: dict, result: str, expression, mutable: bool = False, selected_fields=None) -> str:
    receiver = "&mut self" if mutable else "&self"
    out = [f"    fn {name}({receiver}) -> Option<{result}> {{\n        match self {{\n"]
    for family, row in rows.items():
        selector = row[column]
        parts = selector.split(":") if selector != "-" else []
        fields = set(selected_fields(parts) if selected_fields is not None and parts
                     else parts[1:] if column == "spill" else parts)
        pattern = inst_pattern(family, fields)
        value = f"Some({expression(parts)})" if parts else "None"
        out.append(f"            {pattern} => {value},\n")
    out.append("        }\n    }\n")
    return "".join(out)


def render_allocation(description: InstructionDescription) -> str:
    rows = description.instructions
    out = ["\n#[rustfmt::skip]\nimpl AllocInst for MInst {\n    type Body = AsmBody;\n\n"]
    out.append(render_allocation_view("copy_operands", "copy", rows, "(Self::Reg, Self::Reg)", lambda p: f"(*{p[0]}, *{p[1]})"))
    out.append(render_allocation_view("call_operands", "call_operands", rows, "CallOperands<'_, Self::Reg>", lambda p: f"({p[0]}.as_slice(), {p[1]}.as_slice())"))
    out.append(render_allocation_view("call_operands_mut", "call_operands", rows, "CallOperandsMut<'_, Self::Reg>", lambda p: f"({p[0]}, {p[1]})", True))
    out.append(render_allocation_view("assembly", "assembly", rows, "&Self::Body", lambda p: p[0]))
    out.append(render_allocation_view("assembly_mut", "assembly", rows, "&mut Self::Body", lambda p: p[0], True))
    out.append(render_bool_method("is_indirect_result", "indirect_result", rows))
    out.append(render_allocation_view("exclusive_result", "exclusive", rows, "Self::Reg", lambda p: "*" + p[0]))
    out.append(render_allocation_view("spill", "spill", rows, "SpillView<Self::Reg>", lambda p: f"SpillView {{ kind: SpillKind::{p[0].title()}, reg: *{p[1]}, slot: *{p[2]} }}"))
    arguments = {name: {**row, "stack_argument":
                 ("reload:" + row["incoming"] if row["incoming"] != "-" else
                  "store:" + row["outgoing"] if row["outgoing"] != "-" else "-")}
                 for name, row in rows.items()}
    out.append(render_allocation_view("stack_argument", "stack_argument", arguments, "SpillView<Self::Reg>", lambda p: f"SpillView {{ kind: SpillKind::{p[0].title()}, reg: *{p[1]}, slot: *{p[2]} }}", selected_fields=lambda p: p[1:]))
    out.append(render_allocation_view("spill_slot_mut", "spill", rows, "&mut u32", lambda p: p[2], True,
                                      selected_fields=lambda p: (p[2],)))
    for kind, name, value in (("store", "spill_store", "src"), ("reload", "spill_reload", "dst")):
        family, row = next((family, row) for family, row in rows.items() if row["spill"].startswith(kind + ":"))
        _, reg, slot = row["spill"].split(":")
        members = ", ".join(field if field == source else f"{field}: {source}"
                            for field, source in ((reg, value), (slot, "slot")))
        out.append(f"    fn {name}({value}: Self::Reg, slot: u32) -> Self {{\n        Self::{family} {{ {members} }}\n    }}\n")
    out.append(render_constructor("incoming_arg", "incoming", rows, "dst: Self::Reg, slot: u32", ("dst", "slot")))
    out.append(render_constructor("outgoing_arg", "outgoing", rows, "src: Self::Reg, slot: u32", ("src", "slot")))
    out.append("}\n\n#[rustfmt::skip]\nimpl AllocTerm for MTerm {\n    type Body = AsmBody;\n\n")
    rows = description.terminators
    out.append(render_allocation_view("return_values", "returns", rows, "&[Self::Reg]", lambda p: p[0] + ".as_slice()"))
    out.append(render_allocation_view("return_values_mut", "returns", rows, "&mut Vec<Self::Reg>", lambda p: p[0], True))
    out.append(render_allocation_view("assembly", "assembly", rows, "&Self::Body", lambda p: p[0]))
    out.append(render_allocation_view("assembly_mut", "assembly", rows, "&mut Self::Body", lambda p: p[0], True))
    out.append("    fn defs(&self) -> Vec<Self::Reg> {\n        match self {\n")
    for family, row in rows.items():
        fields = {row["assembly"]} if row["assembly"] != "-" else set()
        expression = f"assembly_defs({row['assembly']})" if fields else "Vec::new()"
        out.append(f"            {inst_pattern(family, fields)} => {expression},\n")
    out.append("        }\n    }\n")
    out.append(render_bool_method("allows_edge_spill_bridge", "spill_bridge", rows))
    out.append("}\n")
    return "".join(out)


def render_instructions(description: InstructionDescription) -> str:
    out = [
        "// Generated by tools/generate-target.py; do not edit.\n",
        f"// Source: targets/aarch64/insts; sha256 {description.digest}.\n\n",
        "use crate::generic::allocation::{\n    AllocInst, AllocTerm, CallOperands, CallOperandsMut, SpillKind, SpillView,\n};\n",
        "use crate::generic::machine::{MachInst, MachTerm};\n",
        "use crate::generic::ssa::{SsaInst, SsaTerm};\n",
        "use crate::{\n    AsmBody, AsmClobber, AsmConstraint, AsmDirection, AsmOperand, AsmValue, MInst, MTerm, VReg,\n};\n\n",
        constant("DESCRIPTION_DIGEST", "&str", f'"{description.digest}"'),
    ]
    for name, rows in (("INST_FAMILIES", description.instructions), ("TERM_FAMILIES", description.terminators)):
        out.append(f'\n#[rustfmt::skip]\npub const {name}: &[&str] = &[\n')
        out.extend(f'    "{family}",\n' for family in rows)
        out.append("];\n")
    assembly = description.instructions["Asm"]
    for helper, column, mutable in (
        ("assembly_defs", "defs", False), ("assembly_uses", "uses", False),
        ("assembly_rewrite_defs", "rewrite_defs", True), ("assembly_rewrite_uses", "rewrite_uses", True),
    ):
        receiver = "&mut AsmBody" if mutable else "&AsmBody"
        ty = "&mut VReg" if mutable else "VReg"
        expression = operand_expression(assembly[column], mutable, "    ")
        out.append(f"\n#[rustfmt::skip]\npub(crate) fn {helper}(body: {receiver}) -> Vec<{ty}> {{\n    {expression}\n}}\n")
    flags = assembly["flags"]
    flags_expr = "body.clobbers.contains(&AsmClobber::Flags)" if flags == "body.flags" else flags
    flags_parameter = "body" if flags == "body.flags" else "_body"
    out.append(f"\npub(crate) fn assembly_writes_flags({flags_parameter}: &AsmBody) -> bool {{\n    {flags_expr}\n}}\n")
    out.append("\n#[rustfmt::skip]\nimpl MachInst for MInst {\n    type Reg = VReg;\n\n")
    out.append(render_operand_method("defs", "defs", description.instructions, False, "Self"))
    out.append("\n" + render_operand_method("uses", "uses", description.instructions, False, "Self"))
    out.append("\n" + render_slot_method(description.instructions))
    for name, column in (("writes_flags", "flags"), ("is_call", "call"), ("discardable", "discardable")):
        out.append("\n" + render_bool_method(name, column, description.instructions))
    out.append("}\n")
    out.append("\n#[rustfmt::skip]\nimpl MachTerm for MTerm {\n    type Reg = VReg;\n\n")
    out.append(render_operand_method("uses", "uses", description.terminators, False, "Self"))
    out.append("\n" + render_operand_method("operand_slots", "rewrite_uses", description.terminators, True, "Self"))
    out.append("\n    fn edges(&self) -> Vec<(u32, &[Self::Reg])> {\n        match self {\n")
    for family, row in description.terminators.items():
        selector = row["edges"]
        pattern = inst_pattern(family, bound_fields(selector))
        parts = operand_parts(selector)
        if not parts:
            expression = "Vec::new()"
        elif all(":" in part for part in parts):
            values = [f"(*{target}, {args}.as_slice())" for target, args in (part.split(":") for part in parts)]
            expression = "vec![" + ", ".join(values) + "]"
        else:
            collected = collection_name("collected_edges", selector)
            lines = ["{", f"                let mut {collected} = Vec::new();"]
            for item in parts:
                if item.endswith("*"):
                    lines.append(f"                {collected}.extend({item[:-1]}.iter().map(|(target, args)| (*target, args.as_slice())));")
                else:
                    target, args = item.split(":")
                    lines.append(f"                {collected}.push((*{target}, {args}.as_slice()));")
            lines.extend((f"                {collected}", "            }"))
            expression = "\n".join(lines)
        out.append(f"            {pattern} => {expression},\n")
    out.append("        }\n    }\n\n    fn is_exit(&self) -> bool {\n        is_exit(self)\n    }\n}\n")
    exits = [inst_pattern(family, set(), "MTerm") for family, row in description.terminators.items() if row["exit"] == "true"]
    expression = "matches!(term, " + " | ".join(exits) + ")" if exits else "false"
    out.append(f"\n#[rustfmt::skip]\npub const fn is_exit(term: &MTerm) -> bool {{\n    {expression}\n}}\n")
    out.append(render_ssa(description))
    out.append(render_allocation(description))
    return "".join(out)

@dataclass
class PatternDescription:
    digest: str
    rows: list[dict[str, str]]


def parse_patterns(source: bytes, target: str = "aarch64") -> PatternDescription:
    digest, lines = parse_envelope(source, "patterns.v1", target)
    rows, seen = [], set()
    families = {
        "const_movz": ("movz_movk", "any"),
        "const_movn": ("movn_movk", "any"),
        "const_orr": ("orr_zero", "logical_mask"),
    }
    extensions = {
        "reuse_integer_extend": ("extend", "reuse_extend"),
        "reuse_low_extract": ("low_extract", "reuse_low_extract"),
        "reuse_low_mask": ("low_mask", "reuse_low_mask"),
    }
    controls = {
        "control_boolean_branch": ("cmp_cset_branch", "compare_branch", "adjacent_private_boolean_flags", "control"),
        "control_constant_interval": ("compare_interval", "sub_compare_set", "adjacent_private_ordered_imm12", "dst"),
    }
    local = {
        "local_indexed_memory": ("address_memory", "64", "indexed_memory", "access_result", "preserve", "same_access", "add", "lsl"),
        "local_shift_arithmetic": ("shift_alu", "32,64", "shifted_arithmetic", "dst", "preserve", "none", "add,sub,and,orr,eor", "lsl,lsr,asr"),
        "local_shift_compare": ("shift_cmp", "32,64", "shifted_compare", "flags", "original_cmp", "none", "cmp", "lsl,lsr,asr"),
        "local_extend_arithmetic": ("extend_alu", "32,64", "extended_arithmetic", "dst", "preserve", "none", "add,sub", "none"),
        "local_extend_compare": ("extend_cmp", "32,64", "extended_compare", "flags", "original_cmp", "none", "cmp", "none"),
        "local_multiply_arithmetic": ("mul_alu", "32,64", "multiply_arithmetic", "dst", "preserve", "none", "add,sub", "none"),
    }
    for line in lines:
        parts = line.split(" ")
        if len(parts) not in {10, 12} or parts[0] != "pattern":
            raise TargetDescriptionError("malformed selection pattern")
        name = parts[1]
        if name not in families and name not in local and name not in extensions and name not in controls or name in seen:
            raise TargetDescriptionError("unknown or duplicate selection pattern")
        seen.add(name)
        row = {"id": name}
        for token in parts[2:]:
            key, separator, value = token.partition("=")
            if not separator:
                raise TargetDescriptionError("malformed pattern field")
            insert(row, key, value, "pattern field")
        if name in families:
            kind, guard = families[name]
            expected = {
                "root": "mov_imm", "widths": "32,64", "sequence": kind,
                "guard": guard, "writes": "dst", "flags": "preserve",
                "memory": "none", "claim": "exact_bits_zero_upper_w",
            }
        elif name in extensions:
            root, kind = extensions[name]
            expected = {
                "root": root, "widths": "32,64", "sequence": kind,
                "guard": "block_local_unique_exact", "writes": "none",
                "flags": "preserve", "memory": "none", "claim": "exact_value_reuse",
            }
        elif name in controls:
            root, kind, guard, writes = controls[name]
            expected = {
                "root": root, "widths": "32,64", "sequence": kind,
                "guard": guard, "writes": writes, "flags": "adjacent_consumer",
                "memory": "none", "claim": "equal_live_state_edges",
            }
        else:
            root, widths, kind, writes, flags, memory, ops, shifts = local[name]
            expected = {
                "root": root, "widths": widths, "sequence": kind,
                "guard": "adjacent_single_use_physical_safe", "writes": writes,
                "flags": flags, "memory": memory, "claim": "equal_live_state",
                "ops": ops, "shifts": shifts,
            }
        if {key: value for key, value in row.items() if key != "id"} != expected:
            raise TargetDescriptionError("unsupported selection pattern contract")
        rows.append(row)
    if not rows:
        raise TargetDescriptionError("empty constant pattern table")
    return PatternDescription(digest, rows)


def render_patterns(description: PatternDescription) -> str:
    kinds = {"movz_movk": "MovzMovk", "movn_movk": "MovnMovk", "orr_zero": "OrrZero"}
    out = [
        "// Generated by tools/generate-target.py from targets/aarch64/patterns. Do not edit.\n",
        f'pub const DIGEST: &str = "{description.digest}";\n',
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n",
        "pub enum ConstantKind {\n    MovzMovk,\n    MovnMovk,\n    OrrZero,\n}\n",
        "#[derive(Debug, Clone, Copy)]\n",
        "pub struct ConstantPattern {\n    pub id: &'static str,\n",
        "    pub kind: ConstantKind,\n    pub widths: &'static [u32],\n}\n",
        "#[rustfmt::skip]\npub const CONSTANT_PATTERNS: &[ConstantPattern] = &[\n",
    ]
    for row in description.rows:
        if row["root"] != "mov_imm":
            continue
        out.append(f'    ConstantPattern {{ id: "{row["id"]}", kind: ConstantKind::{kinds[row["sequence"]]}, widths: &[32, 64] }},\n')
    out.append("];\n")
    local_kinds = {
        "indexed_memory": "IndexedMemory", "shifted_arithmetic": "ShiftedArithmetic",
        "shifted_compare": "ShiftedCompare", "extended_arithmetic": "ExtendedArithmetic",
        "extended_compare": "ExtendedCompare", "multiply_arithmetic": "MultiplyArithmetic",
    }
    out.extend([
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum LocalKind {\n",
        "".join(f"    {name},\n" for name in local_kinds.values()), "}\n",
        "#[derive(Debug, Clone, Copy)]\npub struct LocalPattern {\n    pub id: &'static str,\n    pub kind: LocalKind,\n    pub widths: &'static [u32],\n    pub ops: &'static [&'static str],\n    pub shifts: &'static [&'static str],\n}\n",
        "#[rustfmt::skip]\npub const LOCAL_PATTERNS: &[LocalPattern] = &[\n",
    ])
    for row in description.rows:
        if row["sequence"] not in local_kinds:
            continue
        ops = ", ".join(f'"{op}"' for op in row["ops"].split(","))
        shifts = ", ".join(f'"{op}"' for op in row["shifts"].split(","))
        out.append(f'    LocalPattern {{ id: "{row["id"]}", kind: LocalKind::{local_kinds[row["sequence"]]}, widths: &[{row["widths"]}], ops: &[{ops}], shifts: &[{shifts}] }},\n')
    out.append("];\n")
    extension_kinds = {
        "reuse_extend": "Extend", "reuse_low_extract": "LowExtract",
        "reuse_low_mask": "LowMask",
    }
    out.extend([
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum ExtensionKind {\n",
        "".join(f"    {name},\n" for name in extension_kinds.values()), "}\n",
        "#[derive(Debug, Clone, Copy)]\npub struct ExtensionPattern {\n    pub id: &'static str,\n    pub kind: ExtensionKind,\n    pub widths: &'static [u32],\n}\n",
        "#[rustfmt::skip]\npub const EXTENSION_PATTERNS: &[ExtensionPattern] = &[\n",
    ])
    for row in description.rows:
        if row["sequence"] not in extension_kinds:
            continue
        out.append(f'    ExtensionPattern {{ id: "{row["id"]}", kind: ExtensionKind::{extension_kinds[row["sequence"]]}, widths: &[{row["widths"]}] }},\n')
    out.append("];\n")
    control_kinds = {"compare_branch": "BooleanBranch", "sub_compare_set": "ConstantInterval"}
    out.extend([
        "#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum ControlKind {\n",
        "".join(f"    {name},\n" for name in control_kinds.values()), "}\n",
        "#[derive(Debug, Clone, Copy)]\npub struct ControlPattern {\n    pub id: &'static str,\n    pub kind: ControlKind,\n    pub widths: &'static [u32],\n}\n",
        "#[rustfmt::skip]\npub const CONTROL_PATTERNS: &[ControlPattern] = &[\n",
    ])
    for row in description.rows:
        if row["sequence"] in control_kinds:
            out.append(f'    ControlPattern {{ id: "{row["id"]}", kind: ControlKind::{control_kinds[row["sequence"]]}, widths: &[{row["widths"]}] }},\n')
    out.append("];\n")
    return "".join(out)


def render_pattern_docs(description: PatternDescription) -> str:
    out = [
        "<!-- Generated by tools/generate-target.py; edit targets/aarch64/patterns. -->\n",
        f"Pattern digest: {description.digest}.\n\n",
        "| Pattern | Root / widths | Sequence | Guard | State claim |\n",
        "| --- | --- | --- | --- | --- |\n",
    ]
    for row in description.rows:
        out.append(f'| {row["id"]} | {row["root"]} / {row["widths"]} | {row["sequence"]} | {row["guard"]} | {row["claim"]}; writes {row["writes"]}; flags {row["flags"]}; memory {row["memory"]} |\n')
    return "".join(out)


# E1 describes the existing immediate forms, not a lowering language.
ENCODING_FIELDS = {
    "add_sub": "unsigned", "memory_unscaled": "signed", "memory_scaled": "unsigned",
    "pair": "signed", "mov_wide": "unsigned", "tbz": "signed", "adr": "signed",
}
ENCODING_BINARIES = {
    "add_sub": {"add": "Add", "sub": "Sub"},
    "logical": {"and": "And", "orr": "Orr", "eor": "Eor"},
    "shift": {"lsl": "Lsl", "lsr": "Lsr", "asr": "Asr"},
}


@dataclass
class EncodingDescription:
    digest: str
    fields: dict[str, int] = field(default_factory=dict)
    shifts: dict[str, int] = field(default_factory=dict)
    scales: dict[str, int] = field(default_factory=dict)
    sets: dict[str, tuple[int, ...]] = field(default_factory=dict)
    binaries: dict[str, tuple[str, ...]] = field(default_factory=dict)
    instruction: dict[str, int] = field(default_factory=dict)


def parse_encodings(source: bytes, target: str = "aarch64") -> EncodingDescription:
    digest, lines = parse_envelope(source, "encodings.v1", target)
    result = EncodingDescription(digest)
    for line in lines:
        parts = line.split(" ")
        if len(parts) < 2:
            raise TargetDescriptionError("encoding row lacks a name")
        kind, name, *args = parts
        if kind == "field" and len(args) == 2:
            sign, bits = args
            if ENCODING_FIELDS.get(name) != sign or not 1 <= quantity(bits) <= 32:
                raise TargetDescriptionError("invalid encoding field kind or width")
            insert(result.fields, name, quantity(bits), "encoding field")
        elif kind in ("shift", "scale", "instruction") and len(args) == 1:
            mapping = {"shift": result.shifts, "scale": result.scales,
                       "instruction": result.instruction}[kind]
            insert(mapping, name, quantity(args[0]), kind)
        elif kind == "set" and len(args) == 1:
            values = tuple(quantity(value) for value in args[0].split(","))
            if (not values or tuple(sorted(set(values))) != values or
                    any(value == 0 or value > 64 or value & (value-1) for value in values)):
                raise TargetDescriptionError("encoding sizes must be distinct ascending powers of two")
            insert(result.sets, name, values, "encoding set")
        elif kind == "binary" and len(args) == 1:
            values = tuple(args[0].split(","))
            if (name not in ENCODING_BINARIES or len(set(values)) != len(values) or
                    set(values) != set(ENCODING_BINARIES[name])):
                raise TargetDescriptionError("unsupported binary immediate family")
            insert(result.binaries, name, values, "binary family")
        else:
            raise TargetDescriptionError("unknown encoding row or wrong arity")
    for mapping, required in [
        (result.fields, set(ENCODING_FIELDS)),
        (result.shifts, {"add_sub", "mov_wide"}),
        (result.scales, {"tbz", "adr"}),
        (result.sets, {"memory_bytes", "pair_bytes", "logical_elements"}),
        (result.binaries, set(ENCODING_BINARIES)),
        (result.instruction, {"bytes"}),
    ]:
        if set(mapping) != required:
            raise TargetDescriptionError("missing or unknown encoding fact")
    if (not 1 <= result.shifts["add_sub"] < 32 or
            result.fields["add_sub"] + result.shifts["add_sub"] >= 64 or
            not 1 <= result.fields["mov_wide"] <= 16 or
            result.shifts["mov_wide"] != result.fields["mov_wide"] or
            32 % result.shifts["mov_wide"]):
        raise TargetDescriptionError("invalid immediate chunk widths")
    if any(value == 0 or value > 16 or value & (value-1)
           for value in [*result.scales.values(), *result.instruction.values()]):
        raise TargetDescriptionError("encoding scales must be bounded powers of two")
    if (not set(result.sets["pair_bytes"]) <= set(result.sets["memory_bytes"]) or
            result.sets["memory_bytes"][0] != 1 or result.sets["memory_bytes"][-1] > 16 or
            result.sets["logical_elements"][0] < 2):
        raise TargetDescriptionError("invalid addressing sizes or logical elements")
    return result


def render_encodings(description: EncodingDescription) -> str:
    out = [
        "// Generated by tools/generate-target.py from targets/aarch64/encodings. Do not edit.\n",
        f'pub const DIGEST: &str = "{description.digest}";\n\n',
        "use crate::{BinOp, Width};\n\n",
    ]
    for name, bits in description.fields.items():
        if ENCODING_FIELDS[name] == "unsigned":
            out.append(f"pub const {name.upper()}_MAX: u64 = {(1 << bits)-1};\n")
        else:
            out.append(f"pub const {name.upper()}_MIN: i64 = {-(1 << (bits-1))};\n")
            out.append(f"pub const {name.upper()}_MAX: i64 = {(1 << (bits-1))-1};\n")
    for name, amount in description.shifts.items():
        out.append(f"pub const {name.upper()}_SHIFT: u32 = {amount};\n")
    for name, amount in description.scales.items():
        out.append(f"pub const {name.upper()}_SCALE: i64 = {amount};\n")
    out.append(f"pub const INSTRUCTION_BYTES: usize = {description.instruction['bytes']};\n")
    for name, values in description.sets.items():
        out.append(f"pub const {name.upper()}: &[u64] = &{list(values)};\n")
    out.append("""
pub const ADD_SUB_DOUBLE_MAX: u64 = (ADD_SUB_MAX << ADD_SUB_SHIFT) | ADD_SUB_MAX;

pub const fn add_sub_immediate(value: u64) -> bool {
    value <= ADD_SUB_MAX
}

pub const fn shift_immediate(value: u64, width: Width) -> bool {
    value < width.bits() as u64
}

pub fn scaled_memory_offset(offset: i64, bytes: u64) -> bool {
    MEMORY_BYTES.contains(&bytes)
        && offset >= 0
        && (offset as u64).is_multiple_of(bytes)
        && offset as u64 / bytes <= MEMORY_SCALED_MAX
}

pub fn memory_offset(offset: i64, bytes: u64) -> bool {
    MEMORY_BYTES.contains(&bytes)
        && ((MEMORY_UNSCALED_MIN..=MEMORY_UNSCALED_MAX).contains(&offset)
            || scaled_memory_offset(offset, bytes))
}

pub fn memory_byte_expansion(offset: i64, bytes: u64) -> bool {
    MEMORY_BYTES.contains(&bytes)
        && offset >= MEMORY_UNSCALED_MIN
        && offset
            .checked_add(bytes as i64 - 1)
            .is_some_and(|end| end <= MEMORY_SCALED_MAX as i64)
}

pub fn pair_offset(offset: i64, bytes: u64) -> bool {
    PAIR_BYTES.contains(&bytes)
        && offset % bytes as i64 == 0
        && (PAIR_MIN..=PAIR_MAX).contains(&(offset / bytes as i64))
}

pub const fn wide_immediate(value: u64, shift: u32, width: Width) -> bool {
    value <= MOV_WIDE_MAX && shift < width.bits() as u32 && shift.is_multiple_of(MOV_WIDE_SHIFT)
}

const fn relative_offset(offset: i64, scale: i64, minimum: i64, maximum: i64) -> bool {
    offset % scale == 0 && offset / scale >= minimum && offset / scale <= maximum
}

pub const fn tbz_offset(offset: i64) -> bool {
    relative_offset(offset, TBZ_SCALE, TBZ_MIN, TBZ_MAX)
}

pub const fn adr_offset(offset: i64) -> bool {
    relative_offset(offset, ADR_SCALE, ADR_MIN, ADR_MAX)
}

/// A repeated power-of-two element with one nonempty, nonfull cyclic run.
pub fn logical_mask(value: u64, width: Width) -> bool {
    if value & !width.mask() != 0 || value == 0 || value == width.mask() {
        return false;
    }
    for &element_bits in LOGICAL_ELEMENTS {
        let size = element_bits as u16;
        if size > width.bits() {
            break;
        }
        let mask = u64::MAX >> (64 - size);
        let element = value & mask;
        let rotated = ((element << 1) & mask) | (element >> (size - 1));
        if (element ^ rotated).count_ones() != 2 {
            continue;
        }
        let mut repeated = element;
        let mut filled = size;
        while filled < width.bits() {
            repeated |= repeated << filled;
            filled *= 2;
        }
        if repeated == value {
            return true;
        }
    }
    false
}
""")
    out.append("\n#[rustfmt::skip]\npub const fn has_immediate_form(op: BinOp) -> bool {\n    matches!(op,\n")
    variants = ["BinOp::" + ENCODING_BINARIES[family][name]
                for family, names in description.binaries.items() for name in names]
    out.append("        " + " | ".join(variants) + "\n    )\n}\n")
    out.append("\npub fn binary_immediate(op: BinOp, value: u64, width: Width) -> bool {\n    match op {\n")
    expressions = {"add_sub": "add_sub_immediate(value)", "logical": "logical_mask(value, width)",
                   "shift": "shift_immediate(value, width)"}
    for family, names in description.binaries.items():
        variants = " | ".join("BinOp::" + ENCODING_BINARIES[family][name] for name in names)
        out.append(f"        {variants} => {expressions[family]},\n")
    out.append("        _ => false,\n    }\n}\n")
    return "".join(out)


def render_encoding_docs(description: EncodingDescription) -> str:
    out = [
        "Generated from targets/aarch64/encodings by tools/generate-target.py.\n\n",
        f"Description digest: {description.digest}.\n\n",
        "| Fact | Encoding constraint |\n| --- | --- |\n",
    ]
    for name, bits in description.fields.items():
        sign = ENCODING_FIELDS[name]
        lo, hi = (0, (1 << bits)-1) if sign == "unsigned" else (-(1 << (bits-1)), (1 << (bits-1))-1)
        out.append(f"| {name} field | {sign} {bits} bits; encoded units {lo}..{hi} |\n")
    for name, shift in description.shifts.items():
        out.append(f"| {name} shift | {shift} bits |\n")
    for name, scale in description.scales.items():
        out.append(f"| {name} displacement | {scale} bytes per encoded unit |\n")
    out.append(f"| Instruction | {description.instruction['bytes']} bytes |\n")
    for name, values in description.sets.items():
        out.append(f"| {name} | {', '.join(map(str, values))} |\n")
    for family, names in description.binaries.items():
        out.append(f"| Binary {family} | {', '.join(names)} |\n")
    return "".join(out) + "\n"


def parse_costs(source: bytes, target: str = "aarch64") -> tuple[str, dict[str, int]]:
    digest, lines = parse_envelope(source, "costs.v1", target)
    costs = {}
    for line in lines:
        parts = line.split(" ")
        if len(parts) != 3 or parts[0] != "bytes":
            raise TargetDescriptionError("malformed instruction byte cost")
        insert(costs, parts[1], quantity(parts[2]), "instruction byte cost")
    if costs not in ({"mov_wide": 4, "logical_immediate": 4},
                     {"mov_wide": 4, "logical_immediate": 4, "scalar_instruction": 4}):
        raise TargetDescriptionError("C1 costs must count the admitted four-byte A64 forms")
    return digest, costs


def render_costs(description: tuple[str, dict[str, int]]) -> str:
    digest, costs = description
    return (
        "// Generated by tools/generate-target.py from targets/aarch64/costs. Do not edit.\n"
        f'pub const DIGEST: &str = "{digest}";\n'
        f'pub const WIDE_BYTES: u32 = {costs["mov_wide"]};\n'
        f'pub const LOGICAL_BYTES: u32 = {costs["logical_immediate"]};\n'
        f'pub const SCALAR_BYTES: Option<u32> = {"Some(4)" if "scalar_instruction" in costs else "None"};\n'
    )


def target_outputs(directory: Path, args) -> dict[Path, bytes]:
    root, target = directory.parent.parent, identifier(directory.name)
    machine = root / "crates/evocc-mir/src/isa" / target
    def path(option, default):
        return getattr(args, option) or default
    description = parse_description(path("input", directory / "regfile").read_bytes(), target)
    validate(description)
    abis = [parse_abi(item.read_bytes(), target) for item in
            (args.abi_input if args.abi_input is not None else sorted(directory.glob("abi-*")))]
    for abi in abis:
        validate_abi(abi, description)
    if len({abi.facts["abi"] for abi in abis}) != len(abis):
        raise TargetDescriptionError("duplicate ABI description")
    profiles = [parse_profile(item.read_bytes(), target) for item in
                (args.profile_input if args.profile_input is not None else sorted(directory.glob("profile-*")))]
    evoir = root / "crates/evocc-evoir/src" / (
        "assembly_registers.rs" if target == "aarch64" else f"assembly_{target}_registers.rs")
    outputs = {
        path("output", machine / "regfile.rs"): render_mir(description).encode(),
        path("evoir_output", evoir): render_evoir_primitives(description).encode(),
    }
    if abis or profiles or args.abi_output:
        outputs[path("abi_output", machine / "abi_tables.rs")] = render_abi(description, abis, profiles).encode()
    instructions = path("insts_input", directory / "insts")
    if instructions.is_file():
        decoded = parse_instructions(instructions.read_bytes(), target)
        validate_instructions(decoded)
        rendered = render_instructions(decoded).replace("targets/aarch64/insts", f"targets/{target}/insts")
        outputs[path("insts_output", machine / "insts.rs")] = rendered.encode()
    docs_path = path("pattern_docs", root / "spec/evomir.md")
    docs = None
    for kind, parse, render, render_docs in [
        ("patterns", parse_patterns, render_patterns, render_pattern_docs),
        ("encodings", parse_encodings, render_encodings, render_encoding_docs),
        ("costs", parse_costs, render_costs, None),
    ]:
        source = path(kind+"_input", directory/kind)
        if not source.is_file():
            continue
        decoded = parse(source.read_bytes(), target)
        rendered = render(decoded).replace(f"targets/aarch64/{kind}", f"targets/{target}/{kind}")
        outputs[path(kind+"_output", machine/(kind+".rs"))] = rendered.encode()
        if render_docs is not None:
            if docs is None:
                docs = docs_path.read_text(encoding="utf-8")
            start, end = f"<!-- {target}-{kind}:start -->", f"<!-- {target}-{kind}:end -->"
            if docs.count(start) != 1 or docs.count(end) != 1:
                raise TargetDescriptionError(f"missing or duplicate {kind} document markers")
            before, rest = docs.split(start)
            _, after = rest.split(end)
            docs = before+start+"\n"+render_docs(decoded).replace("targets/aarch64/", f"targets/{target}/")+end+after
    if docs is not None:
        outputs[docs_path] = docs.encode()
    return outputs


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target-dir", type=Path)
    for name in ["input", "output", "evoir-output", "insts-input", "insts-output", "abi-output",
                 "patterns-input", "patterns-output", "costs-input", "costs-output",
                 "encodings-input", "encodings-output", "pattern-docs"]:
        parser.add_argument("--"+name, type=Path)
    parser.add_argument("--abi-input", type=Path, action="append")
    parser.add_argument("--profile-input", type=Path, action="append")
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    directories = [args.target_dir.resolve()] if args.target_dir else [
        item for item in sorted((root/"targets").iterdir()) if item.is_dir() and (item/"regfile").is_file()]
    try:
        outputs = {}
        for directory in directories:
            for destination, contents in target_outputs(directory, args).items():
                if destination in outputs and outputs[destination] != contents:
                    raise TargetDescriptionError(f"targets disagree on output: {destination}")
                outputs[destination] = contents
        if not outputs:
            raise TargetDescriptionError("no target descriptions")
        for destination, contents in outputs.items():
            changed = not destination.is_file() or destination.read_bytes() != contents
            if args.check and changed:
                raise TargetDescriptionError(f"generated file differs: {destination}")
            if not args.check and changed:
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(contents)
    except (OSError, TargetDescriptionError) as error:
        print(f"generate-target: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
