//! Canonical EvoIR Section 9 text rendering.

use evocc_types::{AddressSpace, EvoIRType, Region};

use evocc_memory::{IndexSignedness, OffsetDirection};

use crate::{BlockId, CmpOp, Function, GlobalStorage, Inst, Module, Value};

/// Render a module in the canonical human-readable syntax from specification
/// Section 9.
///
/// Functions and blocks retain their stored order. Value and block identities
/// are printed verbatim; the first block in each function is named `^entry`
/// regardless of its numeric identifier.
#[must_use]
pub fn render_module(module: &Module) -> String {
    let globals = module
        .globals
        .iter()
        .map(|global| {
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
            format!(
                "global @{} origin={} storage={} size={} align={} bytes={} initialized={}",
                global.symbol,
                global.origin,
                match global.storage {
                    GlobalStorage::Static => "static",
                    GlobalStorage::StringLiteral => "string_literal",
                },
                global.layout.size_bytes,
                global.layout.align_bytes,
                bytes,
                initialized
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    let functions = module
        .functions
        .iter()
        .map(render_function)
        .collect::<Vec<_>>()
        .join("\n\n");
    match (globals.is_empty(), functions.is_empty()) {
        (false, false) => format!("{globals}\n\n{functions}"),
        (false, true) => globals,
        (true, _) => functions,
    }
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

fn render_function(function: &Function) -> String {
    let mut rendered = String::new();
    let parameters = function
        .params
        .iter()
        .map(|parameter| format!("%{}: {}", parameter.name, render_type(&parameter.ty)))
        .collect::<Vec<_>>()
        .join(", ");
    let returns = function
        .ret_tys
        .iter()
        .map(render_type)
        .collect::<Vec<_>>()
        .join(", ");
    rendered.push_str(&format!(
        "func @{}({parameters}) -> ({returns}) cc = target_default {{\n",
        function.name
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
        if arguments.is_empty() {
            rendered.push_str(&renderer.block_name(block.id));
            rendered.push_str(":\n");
        } else {
            rendered.push_str(&format!(
                "{}({arguments}):\n",
                renderer.block_name(block.id)
            ));
        }
        for instruction in &block.insts {
            rendered.push_str("    ");
            rendered.push_str(&renderer.instruction(instruction));
            rendered.push('\n');
        }
        rendered.push_str("    ");
        rendered.push_str(&renderer.instruction(&block.terminator));
        rendered.push('\n');
    }
    rendered.push('}');
    rendered
}

#[cfg(test)]
mod tests {
    use evocc_types::{AddressSpace, EvoIRType, Region};

    use crate::{
        AccessLayout, Block, CmpOp, Function, GlobalObject, GlobalStorage, Inst, Module,
        TrapReason, UbReason, Value,
    };

    use super::{render_module, render_type, FunctionRenderer, IndexSignedness, OffsetDirection};

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    #[test]
    fn first_block_is_entry_and_trap_uses_section_9_mnemonic() {
        let module = Module {
            globals: vec![],
            functions: vec![Function {
                name: "stopped".to_owned(),
                params: Vec::new(),
                ret_tys: vec![EvoIRType::Bits(32)],
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
                "func @stopped() -> (bits<32>) cc = target_default {\n",
                "^entry:\n",
                "    trap.scalar_function_fell_off_end\n",
                "}",
            )
        );
    }

    #[test]
    fn module_global_has_exact_section_9_text() {
        let module = Module {
            globals: vec![GlobalObject {
                symbol: "bytes".to_owned(),
                origin: 7,
                storage: GlobalStorage::StringLiteral,
                layout: AccessLayout::new(3, 1),
                bytes: vec![0x45, 0x56, 0],
                initialized: vec![true, true, true],
            }],
            functions: vec![],
        };
        assert_eq!(
            render_module(&module),
            "global @bytes origin=7 storage=string_literal size=3 align=1 bytes=455600 initialized=111"
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
