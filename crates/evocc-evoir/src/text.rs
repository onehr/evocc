//! Canonical EvoIR Section 9 text rendering.

use evocc_types::{AddressSpace, EvoIRType, Region};

use evocc_memory::{IndexSignedness, OffsetDirection};

use crate::{BlockId, CmpOp, Function, GlobalKind, GlobalVisibility, Inst, Module, Value};

/// Enforce the Section 9 byte envelope before semantic parsing.
///
/// The empty module is the sole zero-byte form. Non-empty modules are ASCII,
/// LF-terminated, contain no comments or trailing whitespace, and begin with a
/// module item. The parser/checker builds on this shared fail-safe lexical gate;
/// passing it alone is not a claim that an instruction stream is well formed.
pub fn validate_section9_bytes(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() {
        return Ok(());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| "EvoIR text is not UTF-8".to_owned())?;
    if !text.is_ascii() {
        return Err("EvoIR text is not ASCII".to_owned());
    }
    if !text.ends_with('\n') || text.ends_with("\n\n") {
        return Err("EvoIR text must end in exactly one newline".to_owned());
    }
    if text.contains(['\r', '\t', '\0']) {
        return Err("EvoIR text contains a forbidden byte".to_owned());
    }
    for line in text.lines() {
        let mut quoted = false;
        let mut escaped = false;
        for byte in line.bytes() {
            if escaped {
                escaped = false;
                continue;
            }
            if quoted && byte == b'\\' {
                escaped = true;
                continue;
            }
            if byte == b'"' {
                quoted = !quoted;
            }
            if byte == b';' && !quoted {
                return Err("EvoIR text contains a comment".into());
            }
        }
        if quoted {
            return Err("EvoIR text contains an unterminated string".into());
        }
        if line.ends_with(' ') {
            return Err("EvoIR text contains trailing whitespace".to_owned());
        }
    }
    let first = text.lines().next().unwrap_or_default();
    if !["global @", "extern @", "declare @", "func @"]
        .iter()
        .any(|prefix| first.starts_with(prefix))
    {
        return Err("EvoIR text does not begin with a module item".to_owned());
    }
    Ok(())
}

/// Render a module in the canonical human-readable syntax from specification
/// Section 9.
///
/// Functions and blocks retain their stored order. Value and block identities
/// are printed verbatim; the first block in each function is named `^entry`
/// regardless of its numeric identifier.
#[must_use]
pub fn render_module(module: &Module) -> String {
    let mut rendered = String::new();
    let mut globals = module.globals.iter().collect::<Vec<_>>();
    globals.sort_by_key(|global| global.origin);
    for global in &globals {
        let bytes = global
            .bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let initialized = global
            .initialized
            .iter()
            .map(|bit| if *bit { '1' } else { '0' })
            .collect::<String>();
        rendered.push_str(&format!(
            "global @{} origin={} kind={} visibility={} size={} align={} bytes={} initialized={}\n",
            global.symbol,
            global.origin,
            match global.kind {
                GlobalKind::File => "file",
                GlobalKind::FunctionStatic => "function_static",
                GlobalKind::StringLiteral => "string_literal",
            },
            match global.visibility {
                GlobalVisibility::External => "external",
                GlobalVisibility::Internal => "internal",
            },
            global.layout.size_bytes,
            global.layout.align_bytes,
            bytes,
            initialized
        ));
    }
    if !globals.is_empty()
        && (!module.extern_objects.is_empty()
            || !module.declares.is_empty()
            || !module.functions.is_empty())
    {
        rendered.push('\n');
    }

    for object in &module.extern_objects {
        rendered.push_str(&format!(
            "extern @{} size={} align={}\n",
            object.symbol,
            object
                .size
                .map_or_else(|| "unknown".to_owned(), |size| size.to_string()),
            object.align
        ));
    }
    if !module.extern_objects.is_empty()
        && (!module.declares.is_empty() || !module.functions.is_empty())
    {
        rendered.push('\n');
    }
    for declaration in &module.declares {
        rendered.push_str(&format!(
            "declare @{}({}) -> ({}) cc = target_default\n",
            declaration.name,
            render_parameter_types(&declaration.params, declaration.variadic),
            declaration
                .ret_tys
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !module.declares.is_empty() && !module.functions.is_empty() {
        rendered.push('\n');
    }

    for (index, function) in module.functions.iter().enumerate() {
        if index != 0 {
            rendered.push('\n');
        }
        rendered.push_str(&render_function(function));
    }
    rendered
}

fn render_type(ty: &EvoIRType) -> String {
    match ty {
        EvoIRType::Bits(width) => format!("bits<{width}>"),
        EvoIRType::F32 => "f32".to_owned(),
        EvoIRType::F64 => "f64".to_owned(),
        EvoIRType::Ptr(space) => format!(
            "ptr<{}>",
            match space {
                AddressSpace::Generic => "generic",
                AddressSpace::Global => "global",
                AddressSpace::Stack => "stack",
                AddressSpace::ThreadLocal => "threadlocal",
            }
        ),
        EvoIRType::FnPtr {
            params,
            results,
            variadic,
        } => format!(
            "fnptr<({})->({})>",
            render_parameter_types(params, *variadic),
            results
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EvoIRType::Vec(count, element) => format!("vec<{count}, {}>", render_type(element)),
        EvoIRType::Mem(region) => format!(
            "mem<{}>",
            match region {
                Region::All => "all",
                Region::Unknown => "unknown",
                Region::Global => "global",
                Region::Stack => "stack",
                Region::Heap => "heap",
                Region::Escaped => "escaped",
            }
        ),
        EvoIRType::World => "world".to_owned(),
        EvoIRType::Tuple(elements) => format!(
            "tuple<{}>",
            elements
                .iter()
                .map(render_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EvoIRType::Void => "void".to_owned(),
    }
}

fn quoted_string(text: &str) -> String {
    let mut rendered = String::from("\"");
    for byte in text.bytes() {
        match byte {
            b'"' => rendered.push_str("\\\""),
            b'\\' => rendered.push_str("\\\\"),
            0x20..=0x7e => rendered.push(char::from(byte)),
            _ => {
                use std::fmt::Write;
                write!(rendered, "\\x{byte:02x}").expect("writing to String");
            }
        }
    }
    rendered.push('"');
    rendered
}

fn render_value(value: &Value) -> String {
    format!("%v{}", value.id)
}

fn render_values(values: &[Value]) -> String {
    values
        .iter()
        .map(render_value)
        .collect::<Vec<_>>()
        .join(", ")
}

struct FunctionRenderer {
    entry: BlockId,
}

impl FunctionRenderer {
    fn assembly(&self, assembly: &crate::InlineAssembly, is_goto: bool) -> String {
        let results = assembly
            .results()
            .into_iter()
            .map(render_value)
            .collect::<Vec<_>>()
            .join(", ");
        let outputs = assembly
            .outputs
            .iter()
            .map(|output| {
                format!(
                    "({}, {}, {}, {})",
                    quoted_string(&output.constraint),
                    quoted_string(&output.name),
                    render_type(&output.ty),
                    output
                        .incoming
                        .as_ref()
                        .map_or_else(|| "-".to_owned(), render_value)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let inputs = assembly
            .inputs
            .iter()
            .map(|input| {
                format!(
                    "({}, {}, {})",
                    quoted_string(&input.constraint),
                    quoted_string(&input.name),
                    render_value(&input.value)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let clobbers = assembly
            .clobbers
            .iter()
            .map(|clobber| quoted_string(clobber))
            .collect::<Vec<_>>()
            .join(", ");
        format!("({results}) = asm{}{} {} {}, {} outputs [{outputs}] inputs [{inputs}] clobbers [{clobbers}]",
            if is_goto { ".goto" } else { "" }, if assembly.volatile { " volatile" } else { "" },
            quoted_string(&assembly.template), render_value(&assembly.mem_in), render_value(&assembly.world_in))
    }

    fn block_name(&self, id: BlockId) -> String {
        if id == self.entry {
            "^entry".to_owned()
        } else {
            format!("^b{id}")
        }
    }

    fn branch_target(&self, id: BlockId, args: &[Value]) -> String {
        format!("{}({})", self.block_name(id), render_values(args))
    }

    fn instruction(&self, inst: &Inst) -> String {
        match inst {
            Inst::VaAccess(access) => {
                let results = access
                    .results()
                    .into_iter()
                    .map(render_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                let list = render_value(&access.list);
                let mem = render_value(&access.mem_in);
                let world = render_value(&access.world_in);
                match &access.op {
                    crate::VariadicOp::Start => {
                        format!("({results}) = va_start {list}, {mem}, {world}")
                    }
                    crate::VariadicOp::End => {
                        format!("({results}) = va_end {list}, {mem}, {world}")
                    }
                    crate::VariadicOp::Arg(value) => format!(
                        "({results}) = va_arg.{} {list}, {mem}, {world}",
                        render_type(&value.ty)
                    ),
                }
            }
            Inst::Asm(assembly) => self.assembly(assembly, false),
            Inst::AsmGoto {
                assembly,
                labels,
                fallthrough,
            } => format!(
                "{} labels [{}] fallthrough {}",
                self.assembly(assembly, true),
                labels
                    .iter()
                    .map(|label| format!(
                        "({}, {})",
                        quoted_string(&label.name),
                        self.branch_target(label.edge.target, &label.edge.arguments)
                    ))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.branch_target(fallthrough.target, &fallthrough.arguments)
            ),
            Inst::Const(result, immediate) => format!(
                "{} = const.{} {immediate}",
                render_value(result),
                render_type(&result.ty)
            ),
            Inst::Uninitialized(result) => format!(
                "{} = uninitialized.{}",
                render_value(result),
                render_type(&result.ty)
            ),
            Inst::Add(result, left, right) => self.binary("add", result, left, right),
            Inst::Sub(result, left, right) => self.binary("sub", result, left, right),
            Inst::Mul(result, left, right) => self.binary("mul", result, left, right),
            Inst::SRem(result, left, right) => self.binary("srem", result, left, right),
            Inst::URem(result, left, right) => self.binary("urem", result, left, right),
            Inst::Shl(result, left, right) => self.binary("shl", result, left, right),
            Inst::AShr(result, left, right) => self.binary("ashr", result, left, right),
            Inst::LShr(result, left, right) => self.binary("lshr", result, left, right),
            Inst::And(result, left, right) => self.binary("and", result, left, right),
            Inst::Or(result, left, right) => self.binary("or", result, left, right),
            Inst::Xor(result, left, right) => self.binary("xor", result, left, right),
            Inst::SignedAddDefined(result, left, right) => {
                self.predicate_binary("signed_add_defined", result, left, right)
            }
            Inst::SignedSubDefined(result, left, right) => {
                self.predicate_binary("signed_sub_defined", result, left, right)
            }
            Inst::SignedMulDefined(result, left, right) => {
                self.predicate_binary("signed_mul_defined", result, left, right)
            }
            Inst::DivisorNonZero(result, divisor) => format!(
                "{} = divisor_nonzero {}",
                render_value(result),
                render_value(divisor)
            ),
            Inst::SignedDivRemNoOverflow(result, left, right) => {
                self.predicate_binary("signed_div_rem_no_overflow", result, left, right)
            }
            Inst::ShiftCountDefined {
                result,
                shifted,
                amount,
                amount_signed,
            } => format!(
                "{} = shift_count_defined.{} {}, {}",
                render_value(result),
                if *amount_signed { "signed" } else { "unsigned" },
                render_value(shifted),
                render_value(amount)
            ),
            Inst::SignedShlDefined {
                result,
                shifted,
                amount,
                amount_signed,
            } => format!(
                "{} = signed_shl_defined.{} {}, {}",
                render_value(result),
                if *amount_signed { "signed" } else { "unsigned" },
                render_value(shifted),
                render_value(amount)
            ),
            Inst::UbAssume(condition, reason) => format!(
                "ub.assume.{} {}",
                reason.mnemonic(),
                render_value(condition)
            ),
            Inst::SDiv(result, left, right) => self.binary("sdiv", result, left, right),
            Inst::UDiv(result, left, right) => self.binary("udiv", result, left, right),
            Inst::ICmp(result, operation, left, right) => format!(
                "{} = icmp.{} {}, {}",
                render_value(result),
                match operation {
                    CmpOp::Eq => "eq",
                    CmpOp::Ne => "ne",
                    CmpOp::SLt => "slt",
                    CmpOp::SLe => "sle",
                    CmpOp::ULt => "ult",
                    CmpOp::ULe => "ule",
                },
                render_value(left),
                render_value(right)
            ),
            Inst::Br(target, args) => format!("br {}", self.branch_target(*target, args)),
            Inst::CondBr(condition, true_target, true_args, false_target, false_args) => format!(
                "cond_br {}, {}, {}",
                render_value(condition),
                self.branch_target(*true_target, true_args),
                self.branch_target(*false_target, false_args)
            ),
            Inst::Ret(returned) if returned.is_empty() => "ret".to_owned(),
            Inst::Ret(returned) => format!("ret {}", render_values(returned)),
            Inst::Trap(reason) => format!("trap.{}", reason.mnemonic()),
            Inst::PtrNull(result) => format!(
                "{} = ptr.null : {}",
                render_value(result),
                render_type(&result.ty)
            ),
            Inst::GlobalAddr { result, symbol } => format!(
                "{} = global_addr @{symbol} : {}",
                render_value(result),
                render_type(&result.ty)
            ),
            Inst::FuncAddr { result, symbol } => format!(
                "{} = func_addr @{symbol} : {}",
                render_value(result),
                render_type(&result.ty)
            ),
            Inst::Alloca {
                result,
                mem_in,
                world_in,
                mem_out,
                world_out,
                origin,
                layout,
            } => format!(
                "({}, {}, {}) = alloca {}, {} [origin={}, size={}, align={}]",
                render_value(result),
                render_value(mem_out),
                render_value(world_out),
                render_value(mem_in),
                render_value(world_in),
                origin,
                layout.size_bytes,
                layout.align_bytes
            ),
            Inst::LifetimeEnd {
                mem_in,
                world_in,
                pointer,
                mem_out,
                world_out,
            } => format!(
                "({}, {}) = lifetime.end {}, {}, {}",
                render_value(mem_out),
                render_value(world_out),
                render_value(mem_in),
                render_value(world_in),
                render_value(pointer)
            ),
            Inst::PointerValid {
                mem_in,
                pointer,
                mem_out,
            } => format!(
                "{} = pointer_valid {}, {}",
                render_value(mem_out),
                render_value(mem_in),
                render_value(pointer)
            ),
            Inst::AccessInBounds {
                mem_in,
                pointer,
                mem_out,
                layout,
            } => format!(
                "{} = access_in_bounds {}, {} [size={}, align={}]",
                render_value(mem_out),
                render_value(mem_in),
                render_value(pointer),
                layout.size_bytes,
                layout.align_bytes
            ),
            Inst::Load {
                result,
                address,
                mem_in,
                world_in,
                mem_out,
                world_out,
                volatile,
            } => format!(
                "({}, {}, {}) = load{} {}, {}, {} : {}",
                render_value(result),
                render_value(mem_out),
                render_value(world_out),
                if *volatile { " volatile" } else { "" },
                render_value(mem_in),
                render_value(world_in),
                render_value(address),
                render_type(&result.ty)
            ),
            Inst::Store {
                address,
                value,
                mem_in,
                world_in,
                mem_out,
                world_out,
                volatile,
            } => format!(
                "({}, {}) = store{} {}, {}, {}, {}",
                render_value(mem_out),
                render_value(world_out),
                if *volatile { " volatile" } else { "" },
                render_value(mem_in),
                render_value(world_in),
                render_value(address),
                render_value(value)
            ),
            Inst::Copy {
                destination,
                source,
                mem_in,
                world_in,
                mem_out,
                world_out,
                layout,
            } => {
                let results = render_values(&[mem_out.clone(), world_out.clone()]);
                format!(
                    "({results}) = copy {}, {}, {}, {} [size={}, align={}]",
                    render_value(mem_in),
                    render_value(world_in),
                    render_value(destination),
                    render_value(source),
                    layout.size_bytes,
                    layout.align_bytes
                )
            }
            Inst::PtrOffset {
                result,
                base,
                index,
                direction,
                signedness,
                scale,
            } => format!(
                "{} = ptr.offset.{}.{} {}, {} [scale={}]",
                render_value(result),
                match direction {
                    OffsetDirection::Add => "add",
                    OffsetDirection::Subtract => "sub",
                },
                match signedness {
                    IndexSignedness::Signed => "signed",
                    IndexSignedness::Unsigned => "unsigned",
                },
                render_value(base),
                render_value(index),
                scale
            ),
            Inst::Call {
                results,
                callee,
                args,
                mem_in,
                world_in,
                mem_out,
                world_out,
            } => {
                let mut rendered_results = results.iter().map(render_value).collect::<Vec<_>>();
                rendered_results.push(render_value(mem_out));
                rendered_results.push(render_value(world_out));
                let mut rendered_args = args.iter().map(render_value).collect::<Vec<_>>();
                rendered_args.push(render_value(mem_in));
                rendered_args.push(render_value(world_in));
                format!(
                    "({}) = call @{callee}({})",
                    rendered_results.join(", "),
                    rendered_args.join(", ")
                )
            }
            Inst::IndirectCall {
                results,
                callee,
                args,
                mem_in,
                world_in,
                mem_out,
                world_out,
            } => {
                let mut rendered_results = results.iter().map(render_value).collect::<Vec<_>>();
                rendered_results.push(render_value(mem_out));
                rendered_results.push(render_value(world_out));
                let mut rendered_args = args.iter().map(render_value).collect::<Vec<_>>();
                rendered_args.push(render_value(mem_in));
                rendered_args.push(render_value(world_in));
                format!(
                    "({}) = call {}({})",
                    rendered_results.join(", "),
                    render_value(callee),
                    rendered_args.join(", ")
                )
            }
            Inst::SExt(result, source, ty) => self.conversion("sext", result, source, ty),
            Inst::ZExt(result, source, ty) => self.conversion("zext", result, source, ty),
            Inst::Trunc(result, source, ty) => self.conversion("trunc", result, source, ty),
        }
    }

    fn binary(&self, name: &str, result: &Value, left: &Value, right: &Value) -> String {
        format!(
            "{} = {name}.{} {}, {}",
            render_value(result),
            render_type(&result.ty),
            render_value(left),
            render_value(right)
        )
    }

    fn predicate_binary(&self, name: &str, result: &Value, left: &Value, right: &Value) -> String {
        format!(
            "{} = {name} {}, {}",
            render_value(result),
            render_value(left),
            render_value(right)
        )
    }

    fn conversion(&self, name: &str, result: &Value, source: &Value, ty: &EvoIRType) -> String {
        format!(
            "{} = {name}.{} {}",
            render_value(result),
            render_type(ty),
            render_value(source)
        )
    }
}

fn render_parameter_types(params: &[EvoIRType], variadic: bool) -> String {
    render_parameters(params.iter().map(render_type).collect(), variadic)
}

fn render_parameters(mut parameters: Vec<String>, variadic: bool) -> String {
    if variadic {
        parameters.insert(parameters.len().saturating_sub(2), "...".into());
    }
    parameters.join(", ")
}

fn render_function(function: &Function) -> String {
    let mut rendered = String::new();
    let calling_convention = "target_default";
    let parameters = render_parameters(
        function
            .params
            .iter()
            .enumerate()
            .map(|(index, parameter)| format!("%arg{index}: {}", render_type(&parameter.ty)))
            .collect::<Vec<_>>(),
        function.variadic,
    );
    let returns = function
        .ret_tys
        .iter()
        .map(render_type)
        .collect::<Vec<_>>()
        .join(", ");
    rendered.push_str(&format!(
        "func @{}({parameters}) -> ({returns}) cc = {calling_convention}\n",
        function.name,
    ));
    rendered.push_str(&format!(
        "    effects {{ reads={}, writes={}, volatile={}, atomic={}, io={}, may_trap={}, may_unwind={} }}\n{{\n",
        if function.effects.reads_all { "{all}" } else { "{}" },
        if function.effects.writes_all { "{all}" } else { "{}" },
        function.effects.volatile,
        function.effects.atomic,
        function.effects.io,
        function.effects.may_trap,
        function.effects.may_unwind,
    ));

    let entry = function.blocks.first().map_or(0, |block| block.id);
    let renderer = FunctionRenderer { entry };
    for (block_index, block) in function.blocks.iter().enumerate() {
        if block_index != 0 {
            rendered.push('\n');
        }
        let arguments = block
            .args
            .iter()
            .map(|argument| {
                format!(
                    "{}: {}",
                    render_value(&argument.value),
                    render_type(&argument.value.ty)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        rendered.push_str(&format!(
            "{}({arguments}):\n",
            renderer.block_name(block.id)
        ));
        for instruction in &block.insts {
            rendered.push_str("    ");
            rendered.push_str(&renderer.instruction(instruction));
            rendered.push('\n');
        }
        rendered.push_str("    ");
        rendered.push_str(&renderer.instruction(&block.terminator));
        rendered.push('\n');
    }
    rendered.push_str("}\n");
    rendered
}

#[cfg(test)]
mod tests {
    use evocc_types::{AddressSpace, EvoIRType, Region};

    use crate::{
        AccessLayout, Block, CmpOp, Function, GlobalKind, GlobalObject, GlobalVisibility, Inst,
        Module, TrapReason, UbReason, Value,
    };

    use super::{render_module, render_type, FunctionRenderer, IndexSignedness, OffsetDirection};

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    #[test]
    fn first_block_is_entry_and_trap_uses_section_9_mnemonic() {
        let module = Module {
            extern_objects: Vec::new(),
            declares: Vec::new(),
            globals: vec![],
            functions: vec![Function {
                variadic: false,
                name: "stopped".to_owned(),
                params: Vec::new(),
                ret_tys: vec![EvoIRType::Bits(32)],
                effects: crate::EffectSummary {
                    may_trap: true,
                    ..crate::EffectSummary::default()
                },
                blocks: vec![Block {
                    id: 17,
                    args: Vec::new(),
                    insts: Vec::new(),
                    terminator: Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
                }],
            }],
        };

        assert_eq!(
            render_module(&module),
            concat!(
                "func @stopped() -> (bits<32>) cc = target_default\n",
                "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }\n",
                "{\n",
                "^entry():\n",
                "    trap.scalar_function_fell_off_end\n",
                "}\n",
            )
        );
    }

    #[test]
    fn module_global_has_exact_section_9_text() {
        let module = Module {
            extern_objects: Vec::new(),
            declares: Vec::new(),
            globals: vec![GlobalObject {
                symbol: "__evocc.string.7".to_owned(),
                origin: 7,
                kind: GlobalKind::StringLiteral,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(3, 1),
                bytes: vec![0x45, 0x56, 0],
                initialized: vec![true, true, true],
            }],
            functions: vec![],
        };
        assert_eq!(
            render_module(&module),
            "global @__evocc.string.7 origin=7 kind=string_literal visibility=internal size=3 align=1 bytes=455600 initialized=111\n"
        );
    }

    #[test]
    fn every_type_and_instruction_variant_has_exact_section_9_text() {
        let type_cases = [
            (EvoIRType::Bits(17), "bits<17>"),
            (EvoIRType::F32, "f32"),
            (EvoIRType::F64, "f64"),
            (EvoIRType::Ptr(AddressSpace::Generic), "ptr<generic>"),
            (EvoIRType::Ptr(AddressSpace::Global), "ptr<global>"),
            (EvoIRType::Ptr(AddressSpace::Stack), "ptr<stack>"),
            (
                EvoIRType::Ptr(AddressSpace::ThreadLocal),
                "ptr<threadlocal>",
            ),
            (
                EvoIRType::FnPtr {
                    variadic: false,
                    params: vec![
                        EvoIRType::Bits(32),
                        EvoIRType::Mem(Region::All),
                        EvoIRType::World,
                    ],
                    results: vec![
                        EvoIRType::Bits(32),
                        EvoIRType::Mem(Region::All),
                        EvoIRType::World,
                    ],
                },
                "fnptr<(bits<32>, mem<all>, world)->(bits<32>, mem<all>, world)>",
            ),
            (
                EvoIRType::Vec(4, Box::new(EvoIRType::Bits(8))),
                "vec<4, bits<8>>",
            ),
            (EvoIRType::Mem(Region::All), "mem<all>"),
            (EvoIRType::Mem(Region::Unknown), "mem<unknown>"),
            (EvoIRType::Mem(Region::Global), "mem<global>"),
            (EvoIRType::Mem(Region::Stack), "mem<stack>"),
            (EvoIRType::Mem(Region::Heap), "mem<heap>"),
            (EvoIRType::Mem(Region::Escaped), "mem<escaped>"),
            (EvoIRType::World, "world"),
            (
                EvoIRType::Tuple(vec![EvoIRType::Bits(8), EvoIRType::World]),
                "tuple<bits<8>, world>",
            ),
            (EvoIRType::Void, "void"),
        ];
        for (ty, expected) in type_cases {
            assert_eq!(render_type(&ty), expected);
        }

        let bits = |id| value(id, EvoIRType::Bits(32));
        let pred = |id| value(id, EvoIRType::Bits(1));
        let pointer = |id| value(id, EvoIRType::Ptr(AddressSpace::Generic));
        let function_pointer = |id| {
            value(
                id,
                EvoIRType::FnPtr {
                    variadic: false,
                    params: vec![
                        EvoIRType::Bits(32),
                        EvoIRType::Mem(Region::All),
                        EvoIRType::World,
                    ],
                    results: vec![
                        EvoIRType::Bits(32),
                        EvoIRType::Mem(Region::All),
                        EvoIRType::World,
                    ],
                },
            )
        };
        let memory = |id| value(id, EvoIRType::Mem(Region::All));
        let world = |id| value(id, EvoIRType::World);
        let renderer = FunctionRenderer { entry: 17 };
        let cases = vec![
            (Inst::Const(bits(0), 7), "%v0 = const.bits<32> 7"),
            (Inst::Uninitialized(bits(0)), "%v0 = uninitialized.bits<32>"),
            (
                Inst::Add(bits(0), bits(1), bits(2)),
                "%v0 = add.bits<32> %v1, %v2",
            ),
            (
                Inst::Sub(bits(0), bits(1), bits(2)),
                "%v0 = sub.bits<32> %v1, %v2",
            ),
            (
                Inst::Mul(bits(0), bits(1), bits(2)),
                "%v0 = mul.bits<32> %v1, %v2",
            ),
            (
                Inst::SRem(bits(0), bits(1), bits(2)),
                "%v0 = srem.bits<32> %v1, %v2",
            ),
            (
                Inst::URem(bits(0), bits(1), bits(2)),
                "%v0 = urem.bits<32> %v1, %v2",
            ),
            (
                Inst::Shl(bits(0), bits(1), bits(2)),
                "%v0 = shl.bits<32> %v1, %v2",
            ),
            (
                Inst::AShr(bits(0), bits(1), bits(2)),
                "%v0 = ashr.bits<32> %v1, %v2",
            ),
            (
                Inst::LShr(bits(0), bits(1), bits(2)),
                "%v0 = lshr.bits<32> %v1, %v2",
            ),
            (
                Inst::And(bits(0), bits(1), bits(2)),
                "%v0 = and.bits<32> %v1, %v2",
            ),
            (
                Inst::Or(bits(0), bits(1), bits(2)),
                "%v0 = or.bits<32> %v1, %v2",
            ),
            (
                Inst::Xor(bits(0), bits(1), bits(2)),
                "%v0 = xor.bits<32> %v1, %v2",
            ),
            (
                Inst::SignedAddDefined(pred(0), bits(1), bits(2)),
                "%v0 = signed_add_defined %v1, %v2",
            ),
            (
                Inst::SignedSubDefined(pred(0), bits(1), bits(2)),
                "%v0 = signed_sub_defined %v1, %v2",
            ),
            (
                Inst::SignedMulDefined(pred(0), bits(1), bits(2)),
                "%v0 = signed_mul_defined %v1, %v2",
            ),
            (
                Inst::DivisorNonZero(pred(0), bits(2)),
                "%v0 = divisor_nonzero %v2",
            ),
            (
                Inst::SignedDivRemNoOverflow(pred(0), bits(1), bits(2)),
                "%v0 = signed_div_rem_no_overflow %v1, %v2",
            ),
            (
                Inst::ShiftCountDefined {
                    result: pred(0),
                    shifted: bits(1),
                    amount: bits(2),
                    amount_signed: true,
                },
                "%v0 = shift_count_defined.signed %v1, %v2",
            ),
            (
                Inst::ShiftCountDefined {
                    result: pred(0),
                    shifted: bits(1),
                    amount: bits(2),
                    amount_signed: false,
                },
                "%v0 = shift_count_defined.unsigned %v1, %v2",
            ),
            (
                Inst::SignedShlDefined {
                    result: pred(0),
                    shifted: bits(1),
                    amount: bits(2),
                    amount_signed: false,
                },
                "%v0 = signed_shl_defined.unsigned %v1, %v2",
            ),
            (
                Inst::SignedShlDefined {
                    result: pred(0),
                    shifted: bits(1),
                    amount: bits(2),
                    amount_signed: true,
                },
                "%v0 = signed_shl_defined.signed %v1, %v2",
            ),
            (
                Inst::UbAssume(pred(0), UbReason::DivisionByZero),
                "ub.assume.division_by_zero %v0",
            ),
            (
                Inst::SDiv(bits(0), bits(1), bits(2)),
                "%v0 = sdiv.bits<32> %v1, %v2",
            ),
            (
                Inst::UDiv(bits(0), bits(1), bits(2)),
                "%v0 = udiv.bits<32> %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::Eq, bits(1), bits(2)),
                "%v0 = icmp.eq %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::Ne, bits(1), bits(2)),
                "%v0 = icmp.ne %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::SLt, bits(1), bits(2)),
                "%v0 = icmp.slt %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::SLe, bits(1), bits(2)),
                "%v0 = icmp.sle %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::ULt, bits(1), bits(2)),
                "%v0 = icmp.ult %v1, %v2",
            ),
            (
                Inst::ICmp(pred(0), CmpOp::ULe, bits(1), bits(2)),
                "%v0 = icmp.ule %v1, %v2",
            ),
            (Inst::Br(17, vec![]), "br ^entry()"),
            (
                Inst::CondBr(pred(0), 17, vec![bits(1)], 3, vec![bits(2)]),
                "cond_br %v0, ^entry(%v1), ^b3(%v2)",
            ),
            (Inst::Ret(vec![]), "ret"),
            (Inst::Ret(vec![bits(0)]), "ret %v0"),
            (
                Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
                "trap.scalar_function_fell_off_end",
            ),
            (Inst::PtrNull(pointer(0)), "%v0 = ptr.null : ptr<generic>"),
            (
                Inst::GlobalAddr {
                    result: pointer(0),
                    symbol: "counter".to_owned(),
                },
                "%v0 = global_addr @counter : ptr<generic>",
            ),
            (
                Inst::FuncAddr {
                    result: function_pointer(0),
                    symbol: "callee".to_owned(),
                },
                "%v0 = func_addr @callee : fnptr<(bits<32>, mem<all>, world)->(bits<32>, mem<all>, world)>",
            ),
            (
                Inst::Alloca {
                    result: pointer(0),
                    mem_in: memory(1),
                    world_in: world(2),
                    mem_out: memory(3),
                    world_out: world(4),
                    origin: 9,
                    layout: AccessLayout::new(4, 4),
                },
                "(%v0, %v3, %v4) = alloca %v1, %v2 [origin=9, size=4, align=4]",
            ),
            (
                Inst::LifetimeEnd {
                    mem_in: memory(1),
                    world_in: world(2),
                    pointer: pointer(0),
                    mem_out: memory(3),
                    world_out: world(4),
                },
                "(%v3, %v4) = lifetime.end %v1, %v2, %v0",
            ),
            (
                Inst::PointerValid {
                    mem_in: memory(1),
                    pointer: pointer(0),
                    mem_out: memory(2),
                },
                "%v2 = pointer_valid %v1, %v0",
            ),
            (
                Inst::AccessInBounds {
                    mem_in: memory(1),
                    pointer: pointer(0),
                    mem_out: memory(2),
                    layout: AccessLayout::new(4, 4),
                },
                "%v2 = access_in_bounds %v1, %v0 [size=4, align=4]",
            ),
            (
                Inst::Load {
                    result: bits(0),
                    address: pointer(1),
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                    volatile: false,
                },
                "(%v0, %v4, %v5) = load %v2, %v3, %v1 : bits<32>",
            ),
            (
                Inst::Store {
                    address: pointer(1),
                    value: bits(0),
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                    volatile: false,
                },
                "(%v4, %v5) = store %v2, %v3, %v1, %v0",
            ),
            (
                Inst::Load {
                    result: bits(0),
                    address: pointer(1),
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                    volatile: true,
                },
                "(%v0, %v4, %v5) = load volatile %v2, %v3, %v1 : bits<32>",
            ),
            (
                Inst::Store {
                    address: pointer(1),
                    value: bits(0),
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                    volatile: true,
                },
                "(%v4, %v5) = store volatile %v2, %v3, %v1, %v0",
            ),
            (
                Inst::PtrOffset {
                    result: pointer(0),
                    base: pointer(1),
                    index: bits(2),
                    direction: OffsetDirection::Add,
                    signedness: IndexSignedness::Unsigned,
                    scale: 1,
                },
                "%v0 = ptr.offset.add.unsigned %v1, %v2 [scale=1]",
            ),
            (
                Inst::Call {
                    results: vec![bits(0)],
                    callee: "callee".to_owned(),
                    args: vec![bits(1)],
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                },
                "(%v0, %v4, %v5) = call @callee(%v1, %v2, %v3)",
            ),
            (
                Inst::IndirectCall {
                    results: vec![bits(0)],
                    callee: function_pointer(6),
                    args: vec![bits(1)],
                    mem_in: memory(2),
                    world_in: world(3),
                    mem_out: memory(4),
                    world_out: world(5),
                },
                "(%v0, %v4, %v5) = call %v6(%v1, %v2, %v3)",
            ),
            (
                Inst::SExt(bits(0), bits(1), EvoIRType::Bits(32)),
                "%v0 = sext.bits<32> %v1",
            ),
            (
                Inst::ZExt(bits(0), bits(1), EvoIRType::Bits(32)),
                "%v0 = zext.bits<32> %v1",
            ),
            (
                Inst::Trunc(bits(0), bits(1), EvoIRType::Bits(32)),
                "%v0 = trunc.bits<32> %v1",
            ),
        ];
        for (inst, expected) in cases {
            assert_eq!(renderer.instruction(&inst), expected);
        }
    }
}
