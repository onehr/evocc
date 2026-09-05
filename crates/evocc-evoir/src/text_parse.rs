//! Strict parser for canonical EvoIR Section 9 module bytes.

use std::collections::HashMap;

use evocc_memory::{AccessLayout, IndexSignedness, OffsetDirection};
use evocc_types::{AddressSpace, EvoIRType, Region};

use crate::{
    validate_module, validate_section9_bytes, Block, BlockArg, BlockId, CmpOp, EffectSummary,
    ExternalObject, Function, FunctionDeclaration, FunctionParam, GlobalKind, GlobalObject,
    GlobalVisibility, Inst, Module, TrapReason, UbReason, Value,
};

type BinaryConstructor = fn(Value, Value, Value) -> Inst;

#[derive(Clone)]
struct Signature {
    params: Vec<EvoIRType>,
    results: Vec<EvoIRType>,
    variadic: bool,
}

struct Header {
    name: String,
    convention: String,
    signature: Signature,
}

/// Parse a canonical Section 9 module and validate its complete SSA, symbol,
/// effect and memory contract.
///
/// Successful parsing is deliberately followed by canonical re-rendering.
/// This makes accepted bytes a producer-reachable spelling rather than a
/// permissive source language with aliases for the same module.
pub fn parse_section9_module(bytes: &[u8]) -> Result<Module, String> {
    validate_section9_bytes(bytes)?;
    if bytes.is_empty() {
        return Ok(Module::default());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "EvoIR text is not UTF-8".to_owned())?;
    let lines = text.lines().collect::<Vec<_>>();
    let mut signatures = HashMap::new();
    for line in &lines {
        if line.starts_with("func @") || line.starts_with("declare @") {
            let header = if line.starts_with("declare @") {
                parse_declaration_header(line)?
            } else {
                parse_function_header(line)?
            };
            if signatures
                .insert(header.name.clone(), header.signature)
                .is_some()
            {
                return Err(format!(
                    "E3041: symbol-declared-and-defined: '{}'",
                    header.name
                ));
            }
        }
    }

    let mut parser = ModuleParser {
        lines: &lines,
        position: 0,
        signatures: &signatures,
    };
    let module = parser.module()?;
    if parser.position != lines.len() {
        return Err(format!(
            "unexpected module text at line {}",
            parser.position + 1
        ));
    }
    validate_module(&module)?;
    if crate::render_module(&module).as_bytes() != bytes {
        return Err("EvoIR module is not in canonical Section 9 form".to_owned());
    }
    Ok(module)
}

struct ModuleParser<'a> {
    lines: &'a [&'a str],
    position: usize,
    signatures: &'a HashMap<String, Signature>,
}

impl ModuleParser<'_> {
    fn module(&mut self) -> Result<Module, String> {
        let mut globals = Vec::new();
        while self
            .lines
            .get(self.position)
            .is_some_and(|line| line.starts_with("global @"))
        {
            globals.push(parse_global(self.lines[self.position])?);
            self.position += 1;
        }
        if !globals.is_empty()
            && self
                .lines
                .get(self.position)
                .is_some_and(|line| line.is_empty())
        {
            self.position += 1;
        }

        let mut extern_objects = Vec::new();
        while self
            .lines
            .get(self.position)
            .is_some_and(|line| line.starts_with("extern @"))
        {
            let mut cursor = Cursor::new(self.lines[self.position]);
            cursor.expect("extern @")?;
            let symbol = cursor.ident()?.to_owned();
            cursor.expect(" size=")?;
            let size = if cursor.at("unknown") {
                cursor.expect("unknown")?;
                None
            } else {
                Some(cursor.index()?)
            };
            cursor.expect(" align=")?;
            let align =
                u32::try_from(cursor.index()?).map_err(|_| "external alignment exceeds u32")?;
            cursor.finish()?;
            extern_objects.push(ExternalObject {
                symbol,
                size,
                align,
            });
            self.position += 1;
        }
        if !extern_objects.is_empty() && self.lines.get(self.position) == Some(&"") {
            self.position += 1;
        }
        let mut declares = Vec::new();
        while self
            .lines
            .get(self.position)
            .is_some_and(|line| line.starts_with("declare @"))
        {
            let header = parse_declaration_header(self.lines[self.position])?;
            declares.push(FunctionDeclaration {
                variadic: header.signature.variadic,
                name: header.name,
                params: header.signature.params,
                ret_tys: header.signature.results,
            });
            self.position += 1;
        }
        if !declares.is_empty() && self.lines.get(self.position) == Some(&"") {
            self.position += 1;
        }
        let mut functions = Vec::new();
        while self.position < self.lines.len() {
            if !functions.is_empty() {
                if self.lines.get(self.position) != Some(&"") {
                    return Err(format!(
                        "functions must be separated by one blank line at line {}",
                        self.position + 1
                    ));
                }
                self.position += 1;
            }
            if self
                .lines
                .get(self.position)
                .is_some_and(|line| line.starts_with("asm "))
            {
                break;
            }
            functions.push(self.function()?);
        }
        let mut assembly = Vec::new();
        while let Some(line) = self.lines.get(self.position) {
            let mut cursor = Cursor::new(line);
            cursor.expect("asm ")?;
            assembly.push(cursor.quoted()?);
            cursor.finish()?;
            self.position += 1;
        }
        Ok(Module {
            assembly,
            globals,
            extern_objects,
            declares,
            functions,
        })
    }

    fn function(&mut self) -> Result<Function, String> {
        let line = self
            .lines
            .get(self.position)
            .ok_or_else(|| "missing function header".to_owned())?;
        let header = parse_function_header(line)?;
        self.position += 1;
        if header.convention != "target_default" {
            return Err(format!(
                "function '{}' uses unsupported calling convention '{}'",
                header.name, header.convention
            ));
        }
        let effects_line = self
            .lines
            .get(self.position)
            .ok_or_else(|| format!("function '{}' has no effects clause", header.name))?;
        let effects = parse_effects(effects_line)?;
        self.position += 1;
        if self.lines.get(self.position) != Some(&"{") {
            return Err(format!("function '{}' has no opening brace", header.name));
        }
        self.position += 1;

        let mut blocks = Vec::new();
        while self.lines.get(self.position) != Some(&"}") {
            if !blocks.is_empty() {
                if self.lines.get(self.position) != Some(&"") {
                    return Err(format!(
                        "blocks in '{}' must be separated by one blank line",
                        header.name
                    ));
                }
                self.position += 1;
            }
            blocks.push(self.block(blocks.is_empty())?);
        }
        self.position += 1;

        Ok(Function {
            variadic: header.signature.variadic,
            name: header.name,
            params: header
                .signature
                .params
                .into_iter()
                .enumerate()
                .map(|(index, ty)| FunctionParam {
                    name: format!("arg{index}"),
                    ty,
                })
                .collect(),
            ret_tys: header.signature.results,
            effects,
            blocks,
        })
    }

    fn block(&mut self, is_entry: bool) -> Result<Block, String> {
        let header = self
            .lines
            .get(self.position)
            .ok_or_else(|| "missing block header".to_owned())?;
        let (id, args) = parse_block_header(header, is_entry)?;
        self.position += 1;
        let mut values = args
            .iter()
            .map(|argument| (argument.value.id, argument.value.ty.clone()))
            .collect::<HashMap<_, _>>();
        if values.len() != args.len() {
            return Err(format!("duplicate block argument in block {id}"));
        }

        let mut insts = Vec::new();
        let terminator = loop {
            let line = self
                .lines
                .get(self.position)
                .ok_or_else(|| format!("block {id} has no terminator"))?;
            let body = line
                .strip_prefix("    ")
                .ok_or_else(|| format!("block {id} instruction is not indented"))?;
            if body
                .split_once(" = ")
                .is_some_and(|(_, rhs)| rhs.starts_with("asm.goto "))
            {
                let terminator = parse_tuple_instruction(body, &values, self.signatures)?;
                self.position += 1;
                break terminator;
            }
            if is_terminator_body(body) {
                let terminator = parse_terminator(body, &values)?;
                self.position += 1;
                break terminator;
            }
            let inst = parse_instruction(body, &values, self.signatures)?;
            for result in crate::inst_results(&inst) {
                if values.insert(result.id, result.ty.clone()).is_some() {
                    return Err(format!("duplicate value %v{} in block {id}", result.id));
                }
            }
            insts.push(inst);
            self.position += 1;
        };
        Ok(Block {
            id,
            args,
            insts,
            terminator,
        })
    }
}

fn parse_global(line: &str) -> Result<GlobalObject, String> {
    let fields = line.split(' ').collect::<Vec<_>>();
    if fields.len() != 9 || fields[0] != "global" {
        return Err("malformed module global".to_owned());
    }
    let symbol = parse_symbol(fields[1])?.to_owned();
    let origin = parse_named_u64(fields[2], "origin")?;
    let kind = match parse_named(fields[3], "kind")? {
        "file" => GlobalKind::File,
        "function_static" => GlobalKind::FunctionStatic,
        "string_literal" => GlobalKind::StringLiteral,
        _ => return Err("unknown module-global kind".to_owned()),
    };
    let visibility = match parse_named(fields[4], "visibility")? {
        "external" => GlobalVisibility::External,
        "internal" => GlobalVisibility::Internal,
        _ => return Err("unknown module-global visibility".to_owned()),
    };
    let size = parse_named_u64(fields[5], "size")?;
    let align = u32::try_from(parse_named_u64(fields[6], "align")?)
        .map_err(|_| "global alignment exceeds u32".to_owned())?;
    let byte_text = parse_named(fields[7], "bytes")?;
    if byte_text.is_empty() || byte_text.len() % 2 != 0 {
        return Err("global bytes must contain complete hexadecimal bytes".to_owned());
    }
    let mut bytes = Vec::with_capacity(byte_text.len() / 2);
    for pair in byte_text.as_bytes().as_chunks::<2>().0 {
        let text = std::str::from_utf8(pair).map_err(|_| "invalid global byte".to_owned())?;
        bytes.push(u8::from_str_radix(text, 16).map_err(|_| "invalid global byte".to_owned())?);
    }
    let initialized = parse_named(fields[8], "initialized")?
        .bytes()
        .map(|byte| match byte {
            b'0' => Ok(false),
            b'1' => Ok(true),
            _ => Err("global initialization bitmap is not binary".to_owned()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GlobalObject {
        symbol,
        origin,
        kind,
        visibility,
        layout: AccessLayout::new(size, align),
        bytes,
        initialized,
    })
}

fn parse_declaration_header(line: &str) -> Result<Header, String> {
    let mut cursor = Cursor::new(line);
    cursor.expect("declare @")?;
    let name = cursor.ident()?.to_owned();
    cursor.expect("(")?;
    let (params, variadic) = cursor.parameter_types()?;
    cursor.expect(") -> (")?;
    let results = cursor.type_list(")")?;
    cursor.expect(") cc = target_default")?;
    cursor.finish()?;
    Ok(Header {
        name,
        convention: "target_default".to_owned(),
        signature: Signature {
            params,
            results,
            variadic,
        },
    })
}

fn parse_function_header(line: &str) -> Result<Header, String> {
    let mut cursor = Cursor::new(line);
    cursor.expect("func @")?;
    let name = cursor.ident()?.to_owned();
    cursor.expect("(")?;
    let mut params = Vec::new();
    let mut index = 0_u64;
    let mut marker = None;
    if !cursor.at(")") {
        loop {
            if cursor.at("...") {
                if marker.replace(params.len()).is_some() {
                    return Err("duplicate variadic marker".into());
                }
                cursor.expect("..., ")?;
            }
            cursor.expect("%arg")?;
            let actual = cursor.index()?;
            if actual != index {
                return Err("function parameters must use positional %arg numbering".to_owned());
            }
            cursor.expect(": ")?;
            params.push(cursor.ty()?);
            index += 1;
            if cursor.at(")") {
                break;
            }
            cursor.expect(", ")?;
        }
    }
    cursor.expect(") -> (")?;
    if marker.is_some_and(|position| position.checked_add(2) != Some(params.len())) {
        return Err("variadic marker must precede mem<R>, world".into());
    }
    let results = cursor.type_list(")")?;
    cursor.expect(") cc = ")?;
    let convention = cursor.ident()?.to_owned();
    cursor.finish()?;
    Ok(Header {
        name,
        convention,
        signature: Signature {
            params,
            results,
            variadic: marker.is_some(),
        },
    })
}

fn parse_effects(line: &str) -> Result<EffectSummary, String> {
    let mut cursor = Cursor::new(line);
    cursor.expect("    effects { reads=")?;
    let reads_all = cursor.region_set()?;
    cursor.expect(", writes=")?;
    let writes_all = cursor.region_set()?;
    cursor.expect(", volatile=")?;
    let volatile = cursor.boolean()?;
    cursor.expect(", atomic=")?;
    let atomic = cursor.boolean()?;
    cursor.expect(", io=")?;
    let io = cursor.boolean()?;
    cursor.expect(", may_trap=")?;
    let may_trap = cursor.boolean()?;
    cursor.expect(", may_unwind=")?;
    let may_unwind = cursor.boolean()?;
    cursor.expect(" }")?;
    cursor.finish()?;
    Ok(EffectSummary {
        reads_all,
        writes_all,
        volatile,
        atomic,
        io,
        may_trap,
        may_unwind,
    })
}

fn parse_block_header(line: &str, is_entry: bool) -> Result<(BlockId, Vec<BlockArg>), String> {
    let mut cursor = Cursor::new(line);
    let id = if is_entry {
        cursor.expect("^entry")?;
        0
    } else {
        cursor.expect("^b")?;
        u32::try_from(cursor.index()?).map_err(|_| "block id exceeds u32".to_owned())?
    };
    cursor.expect("(")?;
    let mut args = Vec::new();
    if !cursor.at(")") {
        loop {
            let value_id = cursor.value_id()?;
            cursor.expect(": ")?;
            let ty = cursor.ty()?;
            args.push(BlockArg {
                name: format!("v{value_id}"),
                value: Value { id: value_id, ty },
            });
            if cursor.at(")") {
                break;
            }
            cursor.expect(", ")?;
        }
    }
    cursor.expect("):")?;
    cursor.finish()?;
    Ok((id, args))
}

fn parse_instruction(
    body: &str,
    values: &HashMap<u32, EvoIRType>,
    signatures: &HashMap<String, Signature>,
) -> Result<Inst, String> {
    if body.starts_with('(') {
        return parse_tuple_instruction(body, values, signatures);
    }
    if let Some(rest) = body.strip_prefix("ub.assume.") {
        let (reason, operand) = rest
            .split_once(' ')
            .ok_or_else(|| "malformed ub.assume".to_owned())?;
        return Ok(Inst::UbAssume(
            use_value(operand, values)?,
            parse_ub_reason(reason)?,
        ));
    }
    let (result_text, rhs) = body
        .split_once(" = ")
        .ok_or_else(|| "unknown Section 9 instruction form".to_owned())?;
    let result_id = parse_value_name(result_text)?;

    if let Some(rest) = rhs.strip_prefix("const.") {
        let mut cursor = Cursor::new(rest);
        let ty = cursor.ty()?;
        cursor.expect(" ")?;
        let immediate = cursor.index()?;
        cursor.finish()?;
        return Ok(Inst::Const(Value { id: result_id, ty }, immediate));
    }
    if let Some(rest) = rhs.strip_prefix("uninitialized.") {
        return Ok(Inst::Uninitialized(Value {
            id: result_id,
            ty: parse_type(rest)?,
        }));
    }
    for (name, constructor) in binary_constructors() {
        if let Some(rest) = rhs
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('.'))
        {
            let mut cursor = Cursor::new(rest);
            let ty = cursor.ty()?;
            cursor.expect(" ")?;
            let operands = cursor.remaining();
            let (left, right) = parse_two_values(operands, values)?;
            return Ok(constructor(Value { id: result_id, ty }, left, right));
        }
    }
    if let Some(rest) = rhs.strip_prefix("icmp.") {
        let (operation, operands) = rest
            .split_once(' ')
            .ok_or_else(|| "malformed comparison".to_owned())?;
        let (left, right) = parse_two_values(operands, values)?;
        return Ok(Inst::ICmp(
            Value {
                id: result_id,
                ty: EvoIRType::Bits(1),
            },
            parse_cmp(operation)?,
            left,
            right,
        ));
    }
    for (name, constructor) in predicate_constructors() {
        if let Some(operands) = rhs
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(' '))
        {
            let (left, right) = parse_two_values(operands, values)?;
            return Ok(constructor(
                Value {
                    id: result_id,
                    ty: EvoIRType::Bits(1),
                },
                left,
                right,
            ));
        }
    }
    if let Some(operand) = rhs.strip_prefix("divisor_nonzero ") {
        return Ok(Inst::DivisorNonZero(
            Value {
                id: result_id,
                ty: EvoIRType::Bits(1),
            },
            use_value(operand, values)?,
        ));
    }
    for (prefix, signed, signed_shl) in [
        ("shift_count_defined.signed ", true, false),
        ("shift_count_defined.unsigned ", false, false),
        ("signed_shl_defined.signed ", true, true),
        ("signed_shl_defined.unsigned ", false, true),
    ] {
        if let Some(operands) = rhs.strip_prefix(prefix) {
            let (shifted, amount) = parse_two_values(operands, values)?;
            let result = Value {
                id: result_id,
                ty: EvoIRType::Bits(1),
            };
            return Ok(if signed_shl {
                Inst::SignedShlDefined {
                    result,
                    shifted,
                    amount,
                    amount_signed: signed,
                }
            } else {
                Inst::ShiftCountDefined {
                    result,
                    shifted,
                    amount,
                    amount_signed: signed,
                }
            });
        }
    }
    for op in [
        crate::BitOp::LeadingZeros,
        crate::BitOp::TrailingZeros,
        crate::BitOp::Population,
        crate::BitOp::ByteSwap,
    ] {
        if let Some(rest) = rhs
            .strip_prefix(op.mnemonic())
            .and_then(|rest| rest.strip_prefix('.'))
        {
            let mut cursor = Cursor::new(rest);
            let ty = cursor.ty()?;
            cursor.expect(" ")?;
            let operand = use_value(cursor.remaining(), values)?;
            return Ok(Inst::BitOperation {
                result: Value { id: result_id, ty },
                operand,
                op,
            });
        }
    }
    for (name, constructor) in [
        ("sext", Inst::SExt as fn(Value, Value, EvoIRType) -> Inst),
        ("zext", Inst::ZExt as fn(Value, Value, EvoIRType) -> Inst),
        ("trunc", Inst::Trunc as fn(Value, Value, EvoIRType) -> Inst),
    ] {
        if let Some(rest) = rhs
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('.'))
        {
            let mut cursor = Cursor::new(rest);
            let ty = cursor.ty()?;
            cursor.expect(" ")?;
            let source = use_value(cursor.remaining(), values)?;
            return Ok(constructor(
                Value {
                    id: result_id,
                    ty: ty.clone(),
                },
                source,
                ty,
            ));
        }
    }
    if let Some(rest) = rhs.strip_prefix("ptr.null : ") {
        return Ok(Inst::PtrNull(Value {
            id: result_id,
            ty: parse_type(rest)?,
        }));
    }
    for (prefix, is_function) in [("global_addr ", false), ("func_addr ", true)] {
        if let Some(rest) = rhs.strip_prefix(prefix) {
            let (symbol, ty) = rest
                .split_once(" : ")
                .ok_or_else(|| "malformed address instruction".to_owned())?;
            let result = Value {
                id: result_id,
                ty: parse_type(ty)?,
            };
            return Ok(if is_function {
                Inst::FuncAddr {
                    result,
                    symbol: parse_symbol(symbol)?.to_owned(),
                }
            } else {
                Inst::GlobalAddr {
                    result,
                    symbol: parse_symbol(symbol)?.to_owned(),
                }
            });
        }
    }
    if let Some(rest) = rhs.strip_prefix("pointer_valid ") {
        let (memory, pointer) = parse_two_values(rest, values)?;
        return Ok(Inst::PointerValid {
            mem_out: Value {
                id: result_id,
                ty: memory.ty.clone(),
            },
            mem_in: memory,
            pointer,
        });
    }
    if let Some(rest) = rhs.strip_prefix("access_in_bounds ") {
        let (operands, attributes) = split_attributes(rest)?;
        let (memory, pointer) = parse_two_values(operands, values)?;
        return Ok(Inst::AccessInBounds {
            mem_out: Value {
                id: result_id,
                ty: memory.ty.clone(),
            },
            mem_in: memory,
            pointer,
            layout: parse_layout(attributes)?,
        });
    }
    if let Some(rest) = rhs.strip_prefix("ptr.offset.") {
        let (mode, tail) = rest
            .split_once(' ')
            .ok_or_else(|| "malformed ptr.offset".to_owned())?;
        let (direction, signedness) = mode
            .split_once('.')
            .ok_or_else(|| "malformed ptr.offset mode".to_owned())?;
        let (operands, attributes) = split_attributes(tail)?;
        let (base, index) = parse_two_values(operands, values)?;
        let scale = parse_named_u64(attributes, "scale")?;
        return Ok(Inst::PtrOffset {
            result: Value {
                id: result_id,
                ty: base.ty.clone(),
            },
            base,
            index,
            direction: match direction {
                "add" => OffsetDirection::Add,
                "sub" => OffsetDirection::Subtract,
                _ => return Err("unknown ptr.offset direction".to_owned()),
            },
            signedness: match signedness {
                "signed" => IndexSignedness::Signed,
                "unsigned" => IndexSignedness::Unsigned,
                _ => return Err("unknown ptr.offset signedness".to_owned()),
            },
            scale,
        });
    }
    Err("unknown Section 9 instruction form".to_owned())
}

fn parse_tuple_instruction(
    body: &str,
    values: &HashMap<u32, EvoIRType>,
    signatures: &HashMap<String, Signature>,
) -> Result<Inst, String> {
    let close = body
        .find(") = ")
        .ok_or_else(|| "malformed multi-result instruction".to_owned())?;
    let result_ids = parse_result_ids(&body[1..close])?;
    let rhs = &body[close + 4..];
    if rhs.starts_with("va_start ") || rhs.starts_with("va_end ") || rhs.starts_with("va_arg.") {
        let (opcode, operands) = rhs.split_once(' ').ok_or("malformed variadic access")?;
        let operands = parse_values(operands, values, false)?;
        require_count(&operands, 3, "variadic access operands")?;
        let result_type = opcode.strip_prefix("va_arg.").map(parse_type).transpose()?;
        let offset = usize::from(result_type.is_some());
        require_count(&result_ids, offset + 2, "variadic access results")?;
        let op = match result_type {
            Some(ty) => crate::VariadicOp::Arg(Value {
                id: result_ids[0],
                ty,
            }),
            None if opcode == "va_start" => crate::VariadicOp::Start,
            None => crate::VariadicOp::End,
        };
        return Ok(Inst::VaAccess(crate::VariadicAccess {
            op,
            list: operands[0].clone(),
            mem_in: operands[1].clone(),
            world_in: operands[2].clone(),
            mem_out: Value {
                id: result_ids[offset],
                ty: operands[1].ty.clone(),
            },
            world_out: Value {
                id: result_ids[offset + 1],
                ty: EvoIRType::World,
            },
        }));
    }
    if rhs.starts_with("asm ") || rhs.starts_with("asm.goto ") {
        return parse_assembly(rhs, &result_ids, values);
    }
    if let Some(rest) = rhs.strip_prefix("alloca ") {
        require_count(&result_ids, 3, "alloca results")?;
        let (operands, attributes) = split_attributes(rest)?;
        let (memory, world) = parse_two_values(operands, values)?;
        let (origin, layout) = parse_alloca_attributes(attributes)?;
        return Ok(Inst::Alloca {
            result: Value {
                id: result_ids[0],
                ty: EvoIRType::Ptr(AddressSpace::Generic),
            },
            mem_in: memory.clone(),
            world_in: world,
            mem_out: Value {
                id: result_ids[1],
                ty: memory.ty,
            },
            world_out: Value {
                id: result_ids[2],
                ty: EvoIRType::World,
            },
            origin,
            layout,
        });
    }
    if let Some(rest) = rhs.strip_prefix("lifetime.end ") {
        require_count(&result_ids, 2, "lifetime.end results")?;
        let operands = parse_values(rest, values, false)?;
        require_count(&operands, 3, "lifetime.end operands")?;
        return Ok(Inst::LifetimeEnd {
            mem_in: operands[0].clone(),
            world_in: operands[1].clone(),
            pointer: operands[2].clone(),
            mem_out: Value {
                id: result_ids[0],
                ty: operands[0].ty.clone(),
            },
            world_out: Value {
                id: result_ids[1],
                ty: EvoIRType::World,
            },
        });
    }
    for (prefix, volatile) in [("load volatile ", true), ("load ", false)] {
        if let Some(rest) = rhs.strip_prefix(prefix) {
            require_count(&result_ids, 3, "load results")?;
            let (operands, ty) = rest
                .rsplit_once(" : ")
                .ok_or_else(|| "malformed load type".to_owned())?;
            let operands = parse_values(operands, values, false)?;
            require_count(&operands, 3, "load operands")?;
            return Ok(Inst::Load {
                result: Value {
                    id: result_ids[0],
                    ty: parse_type(ty)?,
                },
                mem_in: operands[0].clone(),
                world_in: operands[1].clone(),
                address: operands[2].clone(),
                mem_out: Value {
                    id: result_ids[1],
                    ty: operands[0].ty.clone(),
                },
                world_out: Value {
                    id: result_ids[2],
                    ty: EvoIRType::World,
                },
                volatile,
            });
        }
    }
    for (prefix, volatile) in [("store volatile ", true), ("store ", false)] {
        if let Some(rest) = rhs.strip_prefix(prefix) {
            require_count(&result_ids, 2, "store results")?;
            let operands = parse_values(rest, values, false)?;
            require_count(&operands, 4, "store operands")?;
            return Ok(Inst::Store {
                mem_in: operands[0].clone(),
                world_in: operands[1].clone(),
                address: operands[2].clone(),
                value: operands[3].clone(),
                mem_out: Value {
                    id: result_ids[0],
                    ty: operands[0].ty.clone(),
                },
                world_out: Value {
                    id: result_ids[1],
                    ty: EvoIRType::World,
                },
                volatile,
            });
        }
    }
    if let Some(rest) = rhs.strip_prefix("copy ") {
        require_count(&result_ids, 2, "copy results")?;
        let (operands, attributes) = split_attributes(rest)?;
        let operands = parse_values(operands, values, false)?;
        require_count(&operands, 4, "copy operands")?;
        return Ok(Inst::Copy {
            mem_in: operands[0].clone(),
            world_in: operands[1].clone(),
            destination: operands[2].clone(),
            source: operands[3].clone(),
            mem_out: Value {
                id: result_ids[0],
                ty: operands[0].ty.clone(),
            },
            world_out: Value {
                id: result_ids[1],
                ty: EvoIRType::World,
            },
            layout: parse_layout(attributes)?,
        });
    }
    if let Some(rest) = rhs.strip_prefix("call ") {
        let open = rest.find('(').ok_or_else(|| "malformed call".to_owned())?;
        let callee_text = &rest[..open];
        let args_text = rest[open + 1..]
            .strip_suffix(')')
            .ok_or_else(|| "malformed call arguments".to_owned())?;
        let args = parse_values(args_text, values, true)?;
        if let Some(symbol) = callee_text.strip_prefix('@') {
            validate_ident(symbol)?;
            let signature = signatures
                .get(symbol)
                .ok_or_else(|| format!("call to missing function '{symbol}'"))?;
            return build_direct_call(result_ids, symbol, args, signature);
        }
        let callee = use_value(callee_text, values)?;
        let EvoIRType::FnPtr {
            params,
            results,
            variadic,
        } = &callee.ty
        else {
            return Err("indirect callee is not a function pointer".to_owned());
        };
        let signature = Signature {
            variadic: *variadic,
            params: params.clone(),
            results: results.clone(),
        };
        return build_indirect_call(result_ids, callee, args, &signature);
    }
    Err("unknown multi-result Section 9 instruction form".to_owned())
}

fn build_direct_call(
    result_ids: Vec<u32>,
    symbol: &str,
    args: Vec<Value>,
    signature: &Signature,
) -> Result<Inst, String> {
    let (ordinary_results, mem_out, world_out) = call_results(result_ids, signature)?;
    let (ordinary_args, mem_in, world_in) = call_args(args, signature)?;
    Ok(Inst::Call {
        results: ordinary_results,
        callee: symbol.to_owned(),
        args: ordinary_args,
        mem_in,
        world_in,
        mem_out,
        world_out,
    })
}

fn build_indirect_call(
    result_ids: Vec<u32>,
    callee: Value,
    args: Vec<Value>,
    signature: &Signature,
) -> Result<Inst, String> {
    let (ordinary_results, mem_out, world_out) = call_results(result_ids, signature)?;
    let (ordinary_args, mem_in, world_in) = call_args(args, signature)?;
    Ok(Inst::IndirectCall {
        results: ordinary_results,
        callee,
        args: ordinary_args,
        mem_in,
        world_in,
        mem_out,
        world_out,
    })
}

fn call_results(
    ids: Vec<u32>,
    signature: &Signature,
) -> Result<(Vec<Value>, Value, Value), String> {
    if signature.results.len() < 2 || ids.len() != signature.results.len() {
        return Err("call result count disagrees with callee signature".to_owned());
    }
    let values = ids
        .into_iter()
        .zip(&signature.results)
        .map(|(id, ty)| Value { id, ty: ty.clone() })
        .collect::<Vec<_>>();
    let split = values.len() - 2;
    Ok((
        values[..split].to_vec(),
        values[split].clone(),
        values[split + 1].clone(),
    ))
}

fn call_args(
    args: Vec<Value>,
    signature: &Signature,
) -> Result<(Vec<Value>, Value, Value), String> {
    if signature.params.len() < 2
        || args.len() < signature.params.len()
        || (!signature.variadic && args.len() != signature.params.len())
    {
        return Err("call argument count disagrees with callee signature".to_owned());
    }
    let split = args.len() - 2;
    Ok((
        args[..split].to_vec(),
        args[split].clone(),
        args[split + 1].clone(),
    ))
}

fn parse_assembly(
    rhs: &str,
    result_ids: &[u32],
    values: &HashMap<u32, EvoIRType>,
) -> Result<Inst, String> {
    use crate::{
        assembly_memory_constraint, AssemblyInput, AssemblyLabel, AssemblyOutput, InlineAssembly,
    };
    let mut cursor = Cursor::new(rhs);
    cursor.expect("asm")?;
    let is_goto = cursor.at(".goto");
    if is_goto {
        cursor.expect(".goto")?;
    }
    let volatile = cursor.at(" volatile");
    if volatile {
        cursor.expect(" volatile")?;
    }
    cursor.expect(" ")?;
    let template = cursor.quoted()?;
    cursor.expect(" ")?;
    let mem_in = cursor.value(values)?;
    cursor.expect(", ")?;
    let world_in = cursor.value(values)?;
    cursor.expect(" outputs [")?;
    let mut outputs = Vec::new();
    while !cursor.at("]") {
        if !outputs.is_empty() {
            cursor.expect(", ")?;
        }
        cursor.expect("(")?;
        let constraint = cursor.quoted()?;
        cursor.expect(", ")?;
        let name = cursor.quoted()?;
        cursor.expect(", ")?;
        let ty = cursor.ty()?;
        cursor.expect(", ")?;
        let incoming = if cursor.at("-") {
            cursor.expect("-")?;
            None
        } else {
            Some(cursor.value(values)?)
        };
        cursor.expect(")")?;
        outputs.push(AssemblyOutput {
            constraint,
            name,
            ty,
            incoming,
        });
    }
    cursor.expect("] inputs [")?;
    let mut inputs = Vec::new();
    while !cursor.at("]") {
        if !inputs.is_empty() {
            cursor.expect(", ")?;
        }
        cursor.expect("(")?;
        let constraint = cursor.quoted()?;
        cursor.expect(", ")?;
        let name = cursor.quoted()?;
        cursor.expect(", ")?;
        let value = cursor.value(values)?;
        cursor.expect(")")?;
        inputs.push(AssemblyInput {
            constraint,
            name,
            value,
        });
    }
    cursor.expect("] clobbers [")?;
    let mut clobbers = Vec::new();
    while !cursor.at("]") {
        if !clobbers.is_empty() {
            cursor.expect(", ")?;
        }
        clobbers.push(cursor.quoted()?);
    }
    cursor.expect("]")?;
    let types = outputs
        .iter()
        .filter(|output| !assembly_memory_constraint(&output.constraint))
        .map(|output| output.ty.clone())
        .collect::<Vec<_>>();
    require_count(result_ids, types.len() + 2, "assembly results")?;
    let results = result_ids
        .iter()
        .zip(types)
        .map(|(id, ty)| Value { id: *id, ty })
        .collect::<Vec<_>>();
    let assembly = InlineAssembly {
        template,
        volatile,
        outputs,
        inputs,
        clobbers,
        mem_in,
        world_in,
        mem_out: Value {
            id: result_ids[results.len()],
            ty: EvoIRType::Mem(Region::All),
        },
        world_out: Value {
            id: result_ids[results.len() + 1],
            ty: EvoIRType::World,
        },
        results,
    };
    if !is_goto {
        cursor.finish()?;
        return Ok(Inst::Asm(assembly));
    }
    let mut edge_values = values.clone();
    for result in assembly.results() {
        if edge_values.insert(result.id, result.ty.clone()).is_some() {
            return Err("duplicate assembly result".into());
        }
    }
    cursor.expect(" labels [")?;
    let mut labels = Vec::new();
    while !cursor.at("]") {
        if !labels.is_empty() {
            cursor.expect(", ")?;
        }
        cursor.expect("(")?;
        let name = cursor.quoted()?;
        cursor.expect(", ")?;
        let edge = cursor.assembly_edge(&edge_values)?;
        cursor.expect(")")?;
        labels.push(AssemblyLabel { name, edge });
    }
    cursor.expect("] fallthrough ")?;
    let fallthrough = cursor.assembly_edge(&edge_values)?;
    cursor.finish()?;
    Ok(Inst::AsmGoto {
        assembly,
        labels,
        fallthrough,
    })
}

fn parse_terminator(body: &str, values: &HashMap<u32, EvoIRType>) -> Result<Inst, String> {
    if let Some(target) = body.strip_prefix("br ") {
        let (block, args) = parse_branch_target(target, values)?;
        return Ok(Inst::Br(block, args));
    }
    if let Some(rest) = body.strip_prefix("cond_br ") {
        let (condition, targets) = rest
            .split_once(", ")
            .ok_or_else(|| "malformed cond_br".to_owned())?;
        let split = find_target_separator(targets)?;
        let (true_target, false_target) = targets.split_at(split);
        let (true_block, true_args) = parse_branch_target(true_target, values)?;
        let (false_block, false_args) = parse_branch_target(&false_target[2..], values)?;
        return Ok(Inst::CondBr(
            use_value(condition, values)?,
            true_block,
            true_args,
            false_block,
            false_args,
        ));
    }
    if body == "ret" {
        return Ok(Inst::Ret(Vec::new()));
    }
    if let Some(returned) = body.strip_prefix("ret ") {
        return Ok(Inst::Ret(parse_values(returned, values, false)?));
    }
    if let Some(reason) = body.strip_prefix("trap.") {
        return Ok(Inst::Trap(match reason {
            "scalar_function_fell_off_end" => TrapReason::ScalarFunctionFellOffEnd,
            "unreachable_reached" => TrapReason::UnreachableReached,
            _ => return Err("unknown trap reason".to_owned()),
        }));
    }
    Err("unknown terminator".to_owned())
}

fn is_terminator_body(body: &str) -> bool {
    body == "ret"
        || body.starts_with("ret ")
        || body.starts_with("br ")
        || body.starts_with("cond_br ")
        || body.starts_with("trap.")
}

fn parse_branch_target(
    text: &str,
    values: &HashMap<u32, EvoIRType>,
) -> Result<(BlockId, Vec<Value>), String> {
    let open = text
        .find('(')
        .ok_or_else(|| "malformed branch target".to_owned())?;
    let label = &text[..open];
    let args = text[open + 1..]
        .strip_suffix(')')
        .ok_or_else(|| "malformed branch target".to_owned())?;
    let block = if label == "^entry" {
        0
    } else {
        u32::try_from(parse_canonical_u64(
            label
                .strip_prefix("^b")
                .ok_or_else(|| "unknown block label".to_owned())?,
        )?)
        .map_err(|_| "block id exceeds u32".to_owned())?
    };
    Ok((block, parse_values(args, values, true)?))
}

fn find_target_separator(text: &str) -> Result<usize, String> {
    let mut depth = 0_u32;
    for (index, byte) in text.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| "unbalanced branch target".to_owned())?
            }
            b',' if depth == 0 && text.as_bytes().get(index + 1) == Some(&b' ') => {
                return Ok(index)
            }
            _ => {}
        }
    }
    Err("cond_br has no second target".to_owned())
}

fn binary_constructors() -> [(&'static str, BinaryConstructor); 13] {
    [
        ("add", Inst::Add),
        ("sub", Inst::Sub),
        ("mul", Inst::Mul),
        ("sdiv", Inst::SDiv),
        ("udiv", Inst::UDiv),
        ("srem", Inst::SRem),
        ("urem", Inst::URem),
        ("shl", Inst::Shl),
        ("ashr", Inst::AShr),
        ("lshr", Inst::LShr),
        ("and", Inst::And),
        ("or", Inst::Or),
        ("xor", Inst::Xor),
    ]
}

fn predicate_constructors() -> [(&'static str, BinaryConstructor); 4] {
    [
        ("signed_add_defined", Inst::SignedAddDefined),
        ("signed_sub_defined", Inst::SignedSubDefined),
        ("signed_mul_defined", Inst::SignedMulDefined),
        ("signed_div_rem_no_overflow", Inst::SignedDivRemNoOverflow),
    ]
}

fn parse_cmp(text: &str) -> Result<CmpOp, String> {
    match text {
        "eq" => Ok(CmpOp::Eq),
        "ne" => Ok(CmpOp::Ne),
        "slt" => Ok(CmpOp::SLt),
        "sle" => Ok(CmpOp::SLe),
        "ult" => Ok(CmpOp::ULt),
        "ule" => Ok(CmpOp::ULe),
        _ => Err("unknown comparison operation".to_owned()),
    }
}

fn parse_ub_reason(text: &str) -> Result<UbReason, String> {
    match text {
        "division_by_zero" => Ok(UbReason::DivisionByZero),
        "remainder_by_zero" => Ok(UbReason::RemainderByZero),
        "signed_overflow" => Ok(UbReason::SignedOverflow),
        "bit_builtin_zero_operand" => Ok(UbReason::BitBuiltinZeroOperand),
        "invalid_shift_count" => Ok(UbReason::InvalidShiftCount),
        "invalid_signed_left_shift" => Ok(UbReason::InvalidSignedLeftShift),
        _ => Err("unknown UB reason".to_owned()),
    }
}

fn parse_two_values(
    text: &str,
    values: &HashMap<u32, EvoIRType>,
) -> Result<(Value, Value), String> {
    let operands = parse_values(text, values, false)?;
    require_count(&operands, 2, "binary operands")?;
    Ok((operands[0].clone(), operands[1].clone()))
}

fn parse_values(
    text: &str,
    values: &HashMap<u32, EvoIRType>,
    allow_empty: bool,
) -> Result<Vec<Value>, String> {
    if text.is_empty() {
        return if allow_empty {
            Ok(Vec::new())
        } else {
            Err("empty value list".to_owned())
        };
    }
    text.split(", ")
        .map(|value| use_value(value, values))
        .collect()
}

fn use_value(text: &str, values: &HashMap<u32, EvoIRType>) -> Result<Value, String> {
    let id = parse_value_name(text)?;
    let ty = values
        .get(&id)
        .ok_or_else(|| format!("use of undefined local value %v{id}"))?
        .clone();
    Ok(Value { id, ty })
}

fn parse_result_ids(text: &str) -> Result<Vec<u32>, String> {
    if text.is_empty() {
        return Err("empty result list".to_owned());
    }
    text.split(", ").map(parse_value_name).collect()
}

fn parse_value_name(text: &str) -> Result<u32, String> {
    u32::try_from(parse_canonical_u64(
        text.strip_prefix("%v")
            .ok_or_else(|| "malformed value name".to_owned())?,
    )?)
    .map_err(|_| "value id exceeds u32".to_owned())
}

fn split_attributes(text: &str) -> Result<(&str, &str), String> {
    let (operands, attributes) = text
        .split_once(" [")
        .ok_or_else(|| "missing instruction attributes".to_owned())?;
    Ok((
        operands,
        attributes
            .strip_suffix(']')
            .ok_or_else(|| "unterminated instruction attributes".to_owned())?,
    ))
}

fn parse_layout(text: &str) -> Result<AccessLayout, String> {
    let (size, align) = text
        .split_once(", ")
        .ok_or_else(|| "malformed access layout".to_owned())?;
    Ok(AccessLayout::new(
        parse_named_u64(size, "size")?,
        u32::try_from(parse_named_u64(align, "align")?)
            .map_err(|_| "access alignment exceeds u32".to_owned())?,
    ))
}

fn parse_alloca_attributes(text: &str) -> Result<(u64, AccessLayout), String> {
    let fields = text.split(", ").collect::<Vec<_>>();
    require_count(&fields, 3, "alloca attributes")?;
    Ok((
        parse_named_u64(fields[0], "origin")?,
        AccessLayout::new(
            parse_named_u64(fields[1], "size")?,
            u32::try_from(parse_named_u64(fields[2], "align")?)
                .map_err(|_| "alloca alignment exceeds u32".to_owned())?,
        ),
    ))
}

fn parse_named_u64(text: &str, name: &str) -> Result<u64, String> {
    parse_canonical_u64(parse_named(text, name)?)
}

fn parse_named<'a>(text: &'a str, name: &str) -> Result<&'a str, String> {
    text.strip_prefix(name)
        .and_then(|text| text.strip_prefix('='))
        .ok_or_else(|| format!("missing {name} field"))
}

fn parse_canonical_u64(text: &str) -> Result<u64, String> {
    if text.is_empty()
        || (text.len() > 1 && text.starts_with('0'))
        || !text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("integer is not canonical unsigned decimal".to_owned());
    }
    text.parse().map_err(|_| "integer exceeds u64".to_owned())
}

fn parse_symbol(text: &str) -> Result<&str, String> {
    let ident = text
        .strip_prefix('@')
        .ok_or_else(|| "malformed symbol".to_owned())?;
    validate_ident(ident)?;
    Ok(ident)
}

fn validate_ident(ident: &str) -> Result<(), String> {
    let mut bytes = ident.bytes();
    let first = bytes.next().ok_or_else(|| "empty identifier".to_owned())?;
    if !first.is_ascii_alphabetic() && first != b'_' {
        return Err("identifier has invalid first byte".to_owned());
    }
    if !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.')) {
        return Err("identifier contains an invalid byte".to_owned());
    }
    Ok(())
}

fn parse_type(text: &str) -> Result<EvoIRType, String> {
    let mut cursor = Cursor::new(text);
    let ty = cursor.ty()?;
    cursor.finish()?;
    Ok(ty)
}

fn require_count<T>(values: &[T], expected: usize, description: &str) -> Result<(), String> {
    if values.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "{description} has {}, expected {expected}",
            values.len()
        ))
    }
}

struct Cursor<'a> {
    text: &'a str,
    position: usize,
}

impl<'a> Cursor<'a> {
    const fn new(text: &'a str) -> Self {
        Self { text, position: 0 }
    }

    fn at(&self, expected: &str) -> bool {
        self.text[self.position..].starts_with(expected)
    }

    fn expect(&mut self, expected: &str) -> Result<(), String> {
        if self.at(expected) {
            self.position += expected.len();
            Ok(())
        } else {
            Err(format!("expected {expected:?} at byte {}", self.position))
        }
    }

    fn finish(&self) -> Result<(), String> {
        if self.position == self.text.len() {
            Ok(())
        } else {
            Err(format!("unexpected text at byte {}", self.position))
        }
    }

    fn remaining(&self) -> &'a str {
        &self.text[self.position..]
    }

    fn ident(&mut self) -> Result<&'a str, String> {
        let start = self.position;
        while self
            .text
            .as_bytes()
            .get(self.position)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.'))
        {
            self.position += 1;
        }
        let ident = &self.text[start..self.position];
        validate_ident(ident)?;
        Ok(ident)
    }

    fn index(&mut self) -> Result<u64, String> {
        let start = self.position;
        while self
            .text
            .as_bytes()
            .get(self.position)
            .is_some_and(u8::is_ascii_digit)
        {
            self.position += 1;
        }
        parse_canonical_u64(&self.text[start..self.position])
    }

    fn value_id(&mut self) -> Result<u32, String> {
        self.expect("%v")?;
        u32::try_from(self.index()?).map_err(|_| "value id exceeds u32".to_owned())
    }

    fn value(&mut self, values: &HashMap<u32, EvoIRType>) -> Result<Value, String> {
        let id = self.value_id()?;
        let ty = values
            .get(&id)
            .ok_or_else(|| format!("value %v{id} is not a prior local definition"))?
            .clone();
        Ok(Value { id, ty })
    }

    fn assembly_edge(
        &mut self,
        values: &HashMap<u32, EvoIRType>,
    ) -> Result<crate::AssemblyEdge, String> {
        self.expect("^")?;
        let target = if self.at("entry") {
            self.expect("entry")?;
            0
        } else {
            self.expect("b")?;
            u32::try_from(self.index()?).map_err(|_| "block id exceeds u32")?
        };
        self.expect("(")?;
        let mut arguments = Vec::new();
        while !self.at(")") {
            if !arguments.is_empty() {
                self.expect(", ")?;
            }
            arguments.push(self.value(values)?);
        }
        self.expect(")")?;
        Ok(crate::AssemblyEdge { target, arguments })
    }

    fn quoted(&mut self) -> Result<String, String> {
        self.expect("\"")?;
        let mut bytes = Vec::new();
        loop {
            let byte = self
                .text
                .as_bytes()
                .get(self.position)
                .copied()
                .ok_or("unterminated assembly string")?;
            self.position += 1;
            match byte {
                b'"' => break,
                b'\\' => {
                    let escaped = self
                        .text
                        .as_bytes()
                        .get(self.position)
                        .copied()
                        .ok_or("unterminated assembly escape")?;
                    self.position += 1;
                    match escaped {
                        b'"' | b'\\' => bytes.push(escaped),
                        b'x' => {
                            let hex = self
                                .text
                                .get(self.position..self.position + 2)
                                .ok_or("truncated assembly byte escape")?;
                            bytes.push(
                                u8::from_str_radix(hex, 16)
                                    .map_err(|_| "invalid assembly byte escape")?,
                            );
                            self.position += 2;
                        }
                        _ => return Err("unknown assembly string escape".into()),
                    }
                }
                0x20..=0x7e => bytes.push(byte),
                _ => return Err("noncanonical assembly string byte".into()),
            }
        }
        String::from_utf8(bytes).map_err(|_| "assembly string is not UTF-8".into())
    }

    fn boolean(&mut self) -> Result<bool, String> {
        if self.at("true") {
            self.expect("true")?;
            Ok(true)
        } else if self.at("false") {
            self.expect("false")?;
            Ok(false)
        } else {
            Err("expected boolean".to_owned())
        }
    }

    fn region_set(&mut self) -> Result<bool, String> {
        if self.at("{}") {
            self.expect("{}")?;
            return Ok(false);
        }
        self.expect("{")?;
        let region = self.region()?;
        self.expect("}")?;
        if region == Region::All {
            Ok(true)
        } else {
            Err("Day 8 effect sets may contain only region all".to_owned())
        }
    }

    fn region(&mut self) -> Result<Region, String> {
        for (name, region) in [
            ("unknown", Region::Unknown),
            ("escaped", Region::Escaped),
            ("global", Region::Global),
            ("stack", Region::Stack),
            ("heap", Region::Heap),
            ("all", Region::All),
        ] {
            if self.at(name) {
                self.expect(name)?;
                return Ok(region);
            }
        }
        Err("unknown memory region".to_owned())
    }

    fn type_list(&mut self, end: &str) -> Result<Vec<EvoIRType>, String> {
        let mut types = Vec::new();
        if self.at(end) {
            return Ok(types);
        }
        loop {
            types.push(self.ty()?);
            if self.at(end) {
                return Ok(types);
            }
            self.expect(", ")?;
        }
    }

    fn parameter_types(&mut self) -> Result<(Vec<EvoIRType>, bool), String> {
        let mut types = Vec::new();
        let mut marker = None;
        while !self.at(")") {
            if self.at("...") {
                if marker.replace(types.len()).is_some() {
                    return Err("duplicate variadic marker".into());
                }
                self.expect("...")?;
            } else {
                types.push(self.ty()?);
            }
            if !self.at(")") {
                self.expect(", ")?;
            }
        }
        if marker.is_some_and(|position| position.checked_add(2) != Some(types.len())) {
            return Err("variadic marker must precede mem<R>, world".into());
        }
        Ok((types, marker.is_some()))
    }

    fn ty(&mut self) -> Result<EvoIRType, String> {
        if self.at("bits<") {
            self.expect("bits<")?;
            let width =
                u16::try_from(self.index()?).map_err(|_| "bit width exceeds u16".to_owned())?;
            if width == 0 {
                return Err("bit width must be positive".to_owned());
            }
            self.expect(">")?;
            return Ok(EvoIRType::Bits(width));
        }
        for (name, ty) in [
            ("f32", EvoIRType::F32),
            ("f64", EvoIRType::F64),
            ("world", EvoIRType::World),
            ("void", EvoIRType::Void),
        ] {
            if self.at(name) {
                self.expect(name)?;
                return Ok(ty);
            }
        }
        if self.at("ptr<") {
            self.expect("ptr<")?;
            let space = if self.at("generic") {
                self.expect("generic")?;
                AddressSpace::Generic
            } else if self.at("global") {
                self.expect("global")?;
                AddressSpace::Global
            } else if self.at("stack") {
                self.expect("stack")?;
                AddressSpace::Stack
            } else if self.at("threadlocal") {
                self.expect("threadlocal")?;
                AddressSpace::ThreadLocal
            } else {
                return Err("unknown pointer address space".to_owned());
            };
            self.expect(">")?;
            return Ok(EvoIRType::Ptr(space));
        }
        if self.at("mem<") {
            self.expect("mem<")?;
            let region = self.region()?;
            self.expect(">")?;
            return Ok(EvoIRType::Mem(region));
        }
        if self.at("vec<") {
            self.expect("vec<")?;
            let count =
                u16::try_from(self.index()?).map_err(|_| "vector count exceeds u16".to_owned())?;
            if count == 0 {
                return Err("vector count must be positive".to_owned());
            }
            self.expect(", ")?;
            let element = self.ty()?;
            self.expect(">")?;
            return Ok(EvoIRType::Vec(count, Box::new(element)));
        }
        if self.at("tuple<") {
            self.expect("tuple<")?;
            let elements = self.type_list(">")?;
            self.expect(">")?;
            return Ok(EvoIRType::Tuple(elements));
        }
        if self.at("fnptr<(") {
            self.expect("fnptr<(")?;
            let (params, variadic) = self.parameter_types()?;
            self.expect(")->(")?;
            let results = self.type_list(")")?;
            self.expect(")>")?;
            return Ok(EvoIRType::FnPtr {
                params,
                results,
                variadic,
            });
        }
        Err(format!("unknown EvoIR type at byte {}", self.position))
    }
}
