#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{
    render_module, AccessLayout, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    GlobalKind, GlobalObject, GlobalVisibility, IndexSignedness, Inst, Module, OffsetDirection,
    TrapReason, UbReason, Value,
};
use evocc_types::{AddressSpace, EvoIRType, ParamMark, RecordShape, Region};

const MANIFEST: &str = include_str!("../../../tests/contracts/day8-section9-coverage.txt");

#[path = "support/instruction_values.rs"]
mod instruction_values;
use instruction_values::*;

#[test]
fn every_instruction_variant_is_actually_instantiated_by_the_field_manifest() {
    // An exhaustive match does not establish coverage if no fixture reaches
    // some arms. Read only the top-level variants of this deliberately simple
    // enum; fail closed if its declaration changes rather than silently treating
    // an empty inventory as complete.
    let declaration = include_str!("../src/lib.rs")
        .split_once("pub enum Inst {\n")
        .unwrap()
        .1
        .split_once("\n}")
        .unwrap()
        .0;
    let variants: BTreeSet<_> = declaration
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| line.starts_with(|c: char| c.is_ascii_uppercase()))
        .map(|line| line.split(['(', '{', ',']).next().unwrap().trim())
        .collect();
    assert!(
        variants.len() > 40,
        "instruction inventory was not recognized"
    );
    let instantiated: BTreeSet<_> = sentinel_instructions()
        .iter()
        .map(inst_schema)
        .map(|schema| schema.strip_prefix("Inst.").unwrap())
        .map(|schema| schema.split(['(', '.']).next().unwrap())
        .collect();
    assert_eq!(
        instantiated, variants,
        "unexercised instruction schema arms"
    );
}

fn sentinel_types() -> Vec<EvoIRType> {
    vec![
        EvoIRType::Bits(37),
        EvoIRType::F32,
        EvoIRType::F64,
        EvoIRType::Ptr(AddressSpace::Generic),
        EvoIRType::Ptr(AddressSpace::Global),
        EvoIRType::Ptr(AddressSpace::Stack),
        EvoIRType::Ptr(AddressSpace::ThreadLocal),
        EvoIRType::FnPtr {
            variadic: false,
            params: vec![
                EvoIRType::Ptr(AddressSpace::Generic),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
            results: vec![
                EvoIRType::Bits(17),
                EvoIRType::Mem(Region::Escaped),
                EvoIRType::World,
            ],
            // Revision 1.9.73: a `fnptr<Sig>` naming a signature with a record
            // return destination spells the mark.
            marks: vec![
                ParamMark::Sret(RecordShape {
                    size: 16,
                    align: 16,
                    vector: None,
                    floating: None,
                }),
                ParamMark::None,
                ParamMark::None,
            ],
        },
        EvoIRType::Vec(7, Box::new(EvoIRType::F32)),
        EvoIRType::Mem(Region::All),
        EvoIRType::Mem(Region::Unknown),
        EvoIRType::Mem(Region::Global),
        EvoIRType::Mem(Region::Stack),
        EvoIRType::Mem(Region::Heap),
        EvoIRType::Mem(Region::Escaped),
        EvoIRType::World,
        EvoIRType::Tuple(vec![EvoIRType::Bits(5), EvoIRType::F64]),
        EvoIRType::Void,
        EvoIRType::F128,
    ]
}

/// The half storage formats, which a declaration of the sentinel module
/// carries so that the function sentinels of earlier revisions keep their
/// bytes.
fn half_sentinel_types() -> Vec<EvoIRType> {
    vec![EvoIRType::F16, EvoIRType::BF16]
}

#[test]
fn every_instruction_preserves_uses_and_results_when_remapped() {
    instruction_values::assert_use_remap(evocc_evoir::remap_inst_uses);
}

fn sentinel_module() -> Module {
    let types = sentinel_types();
    let count = types.len();
    let params: Vec<FunctionParam> = types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| FunctionParam {
            name: format!("ignored_source_name_{index}"),
            ty,
        })
        .collect();
    // Revision 1.9.73: the third parameter is the `ptr<generic>` of the type
    // list, so it is the one a record argument can ride on.
    let mut param_marks = vec![ParamMark::None; count];
    param_marks[3] = ParamMark::ByVal(RecordShape {
        size: 24,
        align: 8,
        vector: Some(evocc_types::VectorAggregate {
            lanes: 2,
            bits: 32,
            count: 3,
        }),
        floating: None,
    });
    let mut module = Module {
        tls: vec![evocc_evoir::TlsSymbol {
            symbol: "sentinel_file".into(),
            model: evocc_evoir::TlsModel::InitialExec,
        }],
        lifecycle: vec![evocc_evoir::LifecycleRegistration {
            function: "sentinel_function".into(),
            phase: evocc_evoir::LifecyclePhase::Constructor,
            priority: 321,
        }],
        assembly: vec![".globl sentinel_asm\n".into()],
        aliases: vec![evocc_evoir::SymbolAlias {
            name: "sentinel_alias".into(),
            target: "sentinel_file".into(),
            visibility: GlobalVisibility::Protected,
            linkage: evocc_evoir::Linkage::Strong,
        }],
        extern_objects: vec![evocc_evoir::ExternalObject {
            visibility: GlobalVisibility::Hidden,
            linkage: evocc_evoir::Linkage::Weak,
            symbol: "sentinel_external".into(),
            size: Some(71),
            align: 32,
        }],
        declares: vec![
            evocc_evoir::FunctionDeclaration {
                visibility: GlobalVisibility::Protected,
                linkage: evocc_evoir::Linkage::Weak,
                name: "sentinel_decl".into(),
                params: vec![
                    EvoIRType::Ptr(AddressSpace::Generic),
                    EvoIRType::Mem(Region::All),
                    EvoIRType::World,
                ],
                param_marks: vec![
                    ParamMark::ByVal(RecordShape::new(24, 8)),
                    ParamMark::None,
                    ParamMark::None,
                ],
                variadic: true,
                ret_tys: vec![
                    EvoIRType::Bits(32),
                    EvoIRType::Mem(Region::All),
                    EvoIRType::World,
                ],
            },
            // Revision 2.4.0: a signed narrow parameter of the Darwin target
            // carries `sext`.
            evocc_evoir::FunctionDeclaration {
                visibility: GlobalVisibility::External,
                linkage: evocc_evoir::Linkage::Strong,
                name: "sentinel_sign".into(),
                params: vec![
                    EvoIRType::Bits(11),
                    EvoIRType::Mem(Region::All),
                    EvoIRType::World,
                ],
                param_marks: vec![ParamMark::SignExtend, ParamMark::None, ParamMark::None],
                variadic: false,
                ret_tys: vec![EvoIRType::Mem(Region::All), EvoIRType::World],
            },
            // Revision 2.22.0: the half storage formats are types.
            evocc_evoir::FunctionDeclaration {
                visibility: GlobalVisibility::External,
                linkage: evocc_evoir::Linkage::Strong,
                name: "sentinel_half".into(),
                params: half_sentinel_types()
                    .into_iter()
                    .chain([EvoIRType::Mem(Region::All), EvoIRType::World])
                    .collect(),
                param_marks: Vec::new(),
                variadic: false,
                ret_tys: vec![EvoIRType::Mem(Region::All), EvoIRType::World],
            },
        ],
        globals: vec![
            GlobalObject {
                read_only: false,
                section: Some(".data.sentinel".into()),
                relocs: Vec::new(),
                symbol: "sentinel_file".to_owned(),
                origin: 7001,
                kind: GlobalKind::File,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::External,
                layout: AccessLayout::new(3, 4),
                bytes: vec![0xa1, 0xb2, 0xc3],
                initialized: vec![true, false, true],
            },
            GlobalObject {
                read_only: false,
                section: None,
                relocs: Vec::new(),
                symbol: "__evocc.static.sentinel.7002".to_owned(),
                origin: 7002,
                kind: GlobalKind::FunctionStatic,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(2, 8),
                bytes: vec![0xd4, 0xe5],
                initialized: vec![false, true],
            },
            GlobalObject {
                read_only: false,
                section: None,
                relocs: Vec::new(),
                symbol: "__evocc.string.7003".to_owned(),
                origin: 7003,
                kind: GlobalKind::StringLiteral,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(1, 16),
                bytes: vec![0xf6],
                initialized: vec![true],
            },
            GlobalObject {
                read_only: true,
                section: None,
                symbol: "__evocc.compound.7004".into(),
                origin: 7004,
                kind: GlobalKind::CompoundLiteral,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(16, 8),
                bytes: vec![0; 16],
                initialized: vec![true; 16],
                relocs: vec![evocc_evoir::Relocation {
                    offset: 8,
                    symbol: "sentinel_file".into(),
                    addend: -3,
                    exposed: true,
                    difference: None,
                }],
            },
            GlobalObject {
                read_only: true,
                section: None,
                symbol: "__evocc.compound.7005".into(),
                origin: 7005,
                kind: GlobalKind::CompoundLiteral,
                linkage: evocc_evoir::Linkage::Strong,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(4, 4),
                bytes: vec![0; 4],
                initialized: vec![true; 4],
                relocs: vec![evocc_evoir::Relocation {
                    offset: 0,
                    symbol: "sentinel_function^b4242".into(),
                    addend: -13,
                    exposed: false,
                    difference: Some(evocc_evoir::BlockDifference {
                        base: "sentinel_function^entry".into(),
                        bits: 32,
                    }),
                }],
            },
        ],
        functions: vec![Function {
            section: Some(".text.sentinel".into()),
            patchable_entry: Some((4, 2)),
            stack_protect: true,
            alignment: 256,
            linkage: evocc_evoir::Linkage::Weak,
            visibility: evocc_evoir::GlobalVisibility::Hidden,
            variadic: false,
            name: "sentinel_function".to_owned(),
            params,
            ret_tys: vec![EvoIRType::F64, EvoIRType::Mem(Region::Escaped)],
            effects: EffectSummary::top(),
            blocks: vec![
                Block {
                    id: 4141,
                    args: vec![BlockArg {
                        name: "ignored_entry_arg_name".to_owned(),
                        value: value(900, EvoIRType::Bits(19)),
                    }],
                    insts: sentinel_instructions(),
                    terminator: Inst::Ret(vec![bits(1990), mem(1991), world(1992)]),
                },
                Block {
                    id: 4242,
                    args: vec![BlockArg {
                        name: "ignored_block_arg_name".to_owned(),
                        value: value(901, EvoIRType::Tuple(vec![EvoIRType::Bits(7)])),
                    }],
                    insts: vec![],
                    terminator: Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
                },
            ],
            param_marks,
        }],
    };
    let mut hfa = module.functions[0].clone();
    hfa.name = "sentinel_hfa".into();
    hfa.params = vec![FunctionParam {
        name: "hfa".into(),
        ty: EvoIRType::Ptr(AddressSpace::Generic),
    }];
    hfa.param_marks = vec![ParamMark::ByVal(RecordShape {
        size: 16,
        align: 16,
        vector: None,
        floating: Some(evocc_types::FloatAggregate {
            kind: evocc_types::FloatKind::F32,
            count: 4,
            abi_align: 16,
        }),
    })];
    module.functions.push(hfa);
    module
}

fn inst_schema(inst: &Inst) -> &'static str {
    match inst {
        Inst::TlsAddr(access) => {
            let _ = access;
            "Inst.TlsAddr(access)"
        }
        Inst::Vector(operation) => {
            let _ = operation;
            "Inst.Vector(operation)"
        }
        Inst::Floating(operation) => {
            let evocc_evoir::FloatingInst {
                result,
                op,
                arguments,
            } = operation;
            let _ = (result, op, arguments);
            "Inst.Floating(result,op,arguments)"
        }
        Inst::StrictFloating(operation) => {
            let evocc_evoir::StrictFloatingInst {
                instruction,
                world_in,
                world_out,
            } = operation;
            let evocc_evoir::FloatingInst {
                result,
                op,
                arguments,
            } = instruction;
            let _ = (result, op, arguments, world_in, world_out);
            "Inst.StrictFloating(result,op,arguments,world_in,world_out)"
        }
        Inst::IndirectBr { target, edges } => {
            let _ = (target, edges);
            "Inst.IndirectBr(target,edges)"
        }
        Inst::PointerCast { result, operand } => {
            let _ = (result, operand);
            "Inst.PointerCast(result,operand)"
        }
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            let _ = (result, operand, op);
            "Inst.BitOperation(result,operand,op)"
        }
        Inst::VaAccess(access) => {
            let evocc_evoir::VariadicAccess {
                op,
                list,
                mem_in,
                world_in,
                mem_out,
                world_out,
            } = access;
            let _ = (list, mem_in, world_in, mem_out, world_out);
            match op {
                evocc_evoir::VariadicOp::Start => {
                    "Inst.VaAccess.Start(list,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::VariadicOp::Arg(result) => {
                    let _ = result;
                    "Inst.VaAccess.Arg(result,list,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::VariadicOp::ArgRecord { destination, shape } => {
                    let _ = (destination, shape);
                    "Inst.VaAccess.ArgRecord(destination,shape,list,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::VariadicOp::End => {
                    "Inst.VaAccess.End(list,mem_in,world_in,mem_out,world_out)"
                }
            }
        }
        Inst::Asm(assembly) => {
            let _ = assembly;
            "Inst.Asm(assembly)"
        }
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let _ = (assembly, labels, fallthrough);
            "Inst.AsmGoto(assembly,labels,fallthrough)"
        }
        Inst::Const(result, immediate) => {
            let _ = (result, immediate);
            "Inst.Const(result,immediate)"
        }
        Inst::Uninitialized(result) => {
            let _ = result;
            "Inst.Uninitialized(result)"
        }
        Inst::Add(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Add(result,lhs,rhs)"
        }
        Inst::Sub(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Sub(result,lhs,rhs)"
        }
        Inst::Mul(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Mul(result,lhs,rhs)"
        }
        Inst::SRem(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.SRem(result,lhs,rhs)"
        }
        Inst::URem(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.URem(result,lhs,rhs)"
        }
        Inst::Shl(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Shl(result,lhs,rhs)"
        }
        Inst::AShr(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.AShr(result,lhs,rhs)"
        }
        Inst::LShr(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.LShr(result,lhs,rhs)"
        }
        Inst::And(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.And(result,lhs,rhs)"
        }
        Inst::Or(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Or(result,lhs,rhs)"
        }
        Inst::Xor(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.Xor(result,lhs,rhs)"
        }
        Inst::SignedAddDefined(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.SignedAddDefined(result,lhs,rhs)"
        }
        Inst::SignedSubDefined(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.SignedSubDefined(result,lhs,rhs)"
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.SignedMulDefined(result,lhs,rhs)"
        }
        Inst::DivisorNonZero(result, divisor) => {
            let _ = (result, divisor);
            "Inst.DivisorNonZero(result,divisor)"
        }
        Inst::SignedDivRemNoOverflow(result, dividend, divisor) => {
            let _ = (result, dividend, divisor);
            "Inst.SignedDivRemNoOverflow(result,dividend,divisor)"
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let _ = (result, shifted, amount, amount_signed);
            "Inst.ShiftCountDefined(result,shifted,amount,amount_signed)"
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let _ = (result, shifted, amount, amount_signed);
            "Inst.SignedShlDefined(result,shifted,amount,amount_signed)"
        }
        Inst::UbAssume(condition, reason) => {
            let _ = (condition, reason);
            "Inst.UbAssume(condition,reason)"
        }
        Inst::SDiv(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.SDiv(result,lhs,rhs)"
        }
        Inst::UDiv(result, lhs, rhs) => {
            let _ = (result, lhs, rhs);
            "Inst.UDiv(result,lhs,rhs)"
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            let _ = (result, operation, lhs, rhs);
            "Inst.ICmp(result,operation,lhs,rhs)"
        }
        Inst::Br(target, args) => {
            let _ = (target, args);
            "Inst.Br(target,args)"
        }
        Inst::CondBr(condition, true_target, true_args, false_target, false_args) => {
            let _ = (condition, true_target, true_args, false_target, false_args);
            "Inst.CondBr(condition,true_target,true_args,false_target,false_args)"
        }
        Inst::Ret(values) => {
            let _ = values;
            "Inst.Ret(values)"
        }
        Inst::Trap(reason) => {
            let _ = reason;
            "Inst.Trap(reason)"
        }
        Inst::PtrNull(result) => {
            let _ = result;
            "Inst.PtrNull(result)"
        }
        Inst::GlobalAddr { result, symbol } => {
            let _ = (result, symbol);
            "Inst.GlobalAddr(result,symbol)"
        }
        Inst::FuncAddr { result, symbol } => {
            let _ = (result, symbol);
            "Inst.FuncAddr(result,symbol)"
        }
        Inst::BlockAddr { result, block } => {
            let _ = (result, block);
            "Inst.BlockAddr(result,block)"
        }
        Inst::ReturnAddress { result } => {
            let _ = result;
            "Inst.ReturnAddress(result)"
        }
        Inst::FrameAddress { result, depth } => {
            let _ = (result, depth);
            "Inst.FrameAddress(result,depth)"
        }
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            layout,
        } => {
            let _ = (result, mem_in, world_in, mem_out, world_out, origin, layout);
            "Inst.Alloca(result,mem_in,world_in,mem_out,world_out,origin,layout)"
        }
        Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            origin,
            size,
            align,
        } => {
            let _ = (
                result, mem_in, world_in, mem_out, world_out, origin, size, align,
            );
            "Inst.DynamicAlloca(result,mem_in,world_in,mem_out,world_out,origin,size,align)"
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => {
            let _ = (mem_in, world_in, pointer, mem_out, world_out);
            "Inst.LifetimeEnd(mem_in,world_in,pointer,mem_out,world_out)"
        }
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        } => {
            let _ = (mem_in, pointer, mem_out);
            "Inst.PointerValid(mem_in,pointer,mem_out)"
        }
        Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            layout,
        } => {
            let _ = (mem_in, pointer, mem_out, layout);
            "Inst.AccessInBounds(mem_in,pointer,mem_out,layout)"
        }
        Inst::Atomic(access) => {
            let evocc_evoir::AtomicAccess {
                op,
                address,
                order,
                volatile,
                mem_in,
                world_in,
                mem_out,
                world_out,
            } = access;
            let _ = (
                address, order, volatile, mem_in, world_in, mem_out, world_out,
            );
            match op {
                evocc_evoir::AtomicOp::Rmw {
                    result,
                    operation,
                    value,
                    expected,
                } => {
                    let _ = (result, operation, value, expected);
                    "Inst.Atomic.Rmw(result,operation,value,expected,address,order,volatile,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::AtomicOp::Load(result) => {
                    let _ = result;
                    "Inst.Atomic.Load(result,address,order,volatile,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::AtomicOp::Store(value) => {
                    let _ = value;
                    "Inst.Atomic.Store(value,address,order,volatile,mem_in,world_in,mem_out,world_out)"
                }
                evocc_evoir::AtomicOp::Copy {
                    op,
                    buffers,
                    result,
                    layout,
                } => {
                    let _ = (op, buffers, result, layout);
                    "Inst.Atomic.Copy(op,buffers,result,layout,address,order,volatile,mem_in,world_in,mem_out,world_out)"
                }
            }
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
            mask,
        } => {
            let _ = (
                result, address, mem_in, world_in, mem_out, world_out, volatile, mask,
            );
            "Inst.Load(result,address,mem_in,world_in,mem_out,world_out,volatile,mask)"
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
            mask,
        } => {
            let _ = (
                address, value, mem_in, world_in, mem_out, world_out, volatile, mask,
            );
            "Inst.Store(address,value,mem_in,world_in,mem_out,world_out,volatile,mask)"
        }
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => {
            let _ = (
                destination,
                source,
                mem_in,
                world_in,
                mem_out,
                world_out,
                layout,
                volatile,
            );
            "Inst.Copy(destination,source,mem_in,world_in,mem_out,world_out,layout,volatile)"
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => {
            let _ = (result, base, index, direction, signedness, scale);
            "Inst.PtrOffset(result,base,index,direction,signedness,scale)"
        }
        Inst::Call {
            arg_marks,
            returns_twice,
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let _ = (
                results,
                callee,
                args,
                arg_marks,
                returns_twice,
                mem_in,
                world_in,
                mem_out,
                world_out,
            );
            "Inst.Call(results,callee,args,arg_marks,returns_twice,mem_in,world_in,mem_out,world_out)"
        }
        Inst::IndirectCall {
            arg_marks,
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let _ = (
                results, callee, args, arg_marks, mem_in, world_in, mem_out, world_out,
            );
            "Inst.IndirectCall(results,callee,args,arg_marks,mem_in,world_in,mem_out,world_out)"
        }
        Inst::SExt(result, source, target_ty) => {
            let _ = (result, source, target_ty);
            "Inst.SExt(result,source,target_ty)"
        }
        Inst::ZExt(result, source, target_ty) => {
            let _ = (result, source, target_ty);
            "Inst.ZExt(result,source,target_ty)"
        }
        Inst::Trunc(result, source, target_ty) => {
            let _ = (result, source, target_ty);
            "Inst.Trunc(result,source,target_ty)"
        }
    }
}

fn type_schema(ty: &EvoIRType) -> &'static str {
    match ty {
        EvoIRType::Bits(width) => {
            let _ = width;
            "EvoIRType.Bits(width)"
        }
        EvoIRType::F16 => "EvoIRType.F16",
        EvoIRType::BF16 => "EvoIRType.BF16",
        EvoIRType::F32 => "EvoIRType.F32",
        EvoIRType::F64 => "EvoIRType.F64",
        EvoIRType::F128 => "EvoIRType.F128",
        EvoIRType::Ptr(address_space) => {
            let _ = address_space;
            "EvoIRType.Ptr(address_space)"
        }
        EvoIRType::FnPtr {
            params,
            results,
            variadic,
            marks,
        } => {
            let _ = (params, results, variadic, marks);
            "EvoIRType.FnPtr(params,results,variadic,marks)"
        }
        EvoIRType::Vec(count, element) => {
            let _ = (count, element);
            "EvoIRType.Vec(count,element)"
        }
        EvoIRType::Mem(region) => {
            let _ = region;
            "EvoIRType.Mem(region)"
        }
        EvoIRType::World => "EvoIRType.World",
        EvoIRType::Tuple(elements) => {
            let _ = elements;
            "EvoIRType.Tuple(elements)"
        }
        EvoIRType::Void => "EvoIRType.Void",
    }
}

fn structural_schema(module: &Module) -> BTreeSet<&'static str> {
    let Module {
        tls,
        lifecycle,
        aliases,
        assembly,
        globals,
        extern_objects,
        declares,
        functions,
    } = module;
    let _ = assembly;
    let mut keys = BTreeSet::from([
        "Module(assembly,globals,extern_objects,declares,aliases,functions,tls,lifecycle)",
    ]);
    for evocc_evoir::ExternalObject {
        visibility,
        linkage,
        symbol,
        size,
        align,
    } in extern_objects
    {
        let _ = (visibility, linkage, symbol, size, align);
        keys.insert("ExternalObject(visibility,linkage,symbol,size,align)");
    }
    for evocc_evoir::FunctionDeclaration {
        visibility,
        linkage,
        name,
        params,
        param_marks,
        variadic,
        ret_tys,
    } in declares
    {
        let _ = (
            visibility,
            linkage,
            name,
            params,
            param_marks,
            variadic,
            ret_tys,
        );
        keys.insert(
            "FunctionDeclaration(visibility,linkage,name,params,param_marks,variadic,ret_tys)",
        );
    }
    for evocc_evoir::TlsSymbol { symbol, model } in tls {
        let _ = (symbol, model);
        keys.insert("TlsSymbol(symbol,model)");
    }
    for evocc_evoir::LifecycleRegistration {
        function,
        phase,
        priority,
    } in lifecycle
    {
        let _ = (function, phase, priority);
        keys.insert("LifecycleRegistration(function,phase,priority)");
    }
    for alias in aliases {
        let evocc_evoir::SymbolAlias {
            name,
            target,
            visibility,
            linkage,
        } = alias;
        let _ = (name, target, visibility, linkage);
        keys.insert("SymbolAlias(name,target,visibility,linkage)");
    }
    for global in globals {
        let GlobalObject {
            read_only,
            section,
            linkage,
            symbol,
            origin,
            kind,
            visibility,
            layout,
            bytes,
            initialized,
            relocs,
        } = global;
        let _ = (
            section,
            read_only,
            linkage,
            symbol,
            origin,
            kind,
            visibility,
            layout,
            bytes,
            initialized,
            relocs,
        );
        keys.insert(
            "GlobalObject(symbol,origin,kind,visibility,linkage,layout,bytes,initialized,relocs,section,read_only)",
        );
        keys.insert("GlobalObject.read_only");
        keys.insert("GlobalObject.section");
        for evocc_evoir::Relocation {
            offset,
            symbol,
            addend,
            exposed,
            difference,
        } in relocs
        {
            let _ = (offset, symbol, addend, exposed);
            keys.insert("Relocation(offset,symbol,addend,exposed,difference)");
            if let Some(evocc_evoir::BlockDifference { base, bits }) = difference {
                let _ = (base, bits);
                keys.insert("BlockDifference(base,bits)");
            }
        }
        let AccessLayout {
            size_bytes,
            align_bytes,
        } = layout;
        let _ = (size_bytes, align_bytes);
        keys.insert("AccessLayout(size_bytes,align_bytes)");
    }
    for function in functions {
        let Function {
            section,
            patchable_entry,
            stack_protect,
            alignment,
            linkage,
            name,
            params,
            param_marks,
            variadic,
            ret_tys,
            effects,
            visibility,
            blocks,
        } = function;
        let _ = (
            name,
            ret_tys,
            variadic,
            visibility,
            linkage,
            alignment,
            stack_protect,
            patchable_entry,
            section,
        );
        keys.insert(
            "Function(name,params,param_marks,variadic,ret_tys,effects,visibility,linkage,alignment,stack_protect,patchable_entry,section,blocks)",
        );
        keys.insert("Function.section");
        for mark in param_marks {
            if let Some(RecordShape {
                size,
                align,
                vector,
                floating,
            }) = mark.shape()
            {
                let _ = (size, align);
                keys.insert("RecordShape(size,align,vector,floating)");
                if let Some(evocc_types::FloatAggregate {
                    kind,
                    count,
                    abi_align,
                }) = floating
                {
                    let _ = (kind, count, abi_align);
                    keys.insert("FloatAggregate(kind,count,abi_align)");
                }
                if let Some(evocc_types::VectorAggregate { lanes, bits, count }) = vector {
                    let _ = (lanes, bits, count);
                    keys.insert("VectorAggregate(lanes,bits,count)");
                }
            }
        }
        for param in params {
            let FunctionParam { name, ty } = param;
            let _ = (name, ty);
            keys.insert("FunctionParam(name[omitted],ty)");
        }
        let EffectSummary {
            reads_all,
            writes_all,
            volatile,
            atomic,
            io,
            fenv,
            may_trap,
            may_unwind,
        } = effects;
        let _ = (
            reads_all, writes_all, volatile, atomic, io, fenv, may_trap, may_unwind,
        );
        keys.insert(
            "EffectSummary(reads_all,writes_all,volatile,atomic,io,fenv,may_trap,may_unwind)",
        );
        for block in blocks {
            let Block {
                id,
                args,
                insts,
                terminator,
            } = block;
            let _ = id;
            keys.insert("Block(id,args,insts,terminator)");
            for arg in args {
                let BlockArg { name, value } = arg;
                let _ = (name, value);
                keys.insert("BlockArg(name[omitted],value)");
            }
            for inst in insts.iter().chain(std::iter::once(terminator)) {
                keys.insert(inst_schema(inst));
                match inst {
                    Inst::Asm(assembly) => assembly_schema(assembly, &mut keys),
                    Inst::AsmGoto {
                        assembly,
                        labels,
                        fallthrough,
                    } => {
                        assembly_schema(assembly, &mut keys);
                        for evocc_evoir::AssemblyLabel { name, edge } in labels {
                            let _ = name;
                            keys.insert("AssemblyLabel(name,edge)");
                            assembly_edge_schema(edge, &mut keys);
                        }
                        assembly_edge_schema(fallthrough, &mut keys);
                    }
                    _ => {}
                }
            }
        }
    }
    keys
}

fn assembly_edge_schema(edge: &evocc_evoir::AssemblyEdge, keys: &mut BTreeSet<&'static str>) {
    let evocc_evoir::AssemblyEdge { target, arguments } = edge;
    let _ = (target, arguments);
    keys.insert("AssemblyEdge(target,arguments)");
}

fn assembly_schema(assembly: &evocc_evoir::InlineAssembly, keys: &mut BTreeSet<&'static str>) {
    let evocc_evoir::InlineAssembly {
        template,
        volatile,
        outputs,
        inputs,
        clobbers,
        results,
        mem_in,
        world_in,
        mem_out,
        world_out,
    } = assembly;
    let _ = (
        template, volatile, clobbers, results, mem_in, world_in, mem_out, world_out,
    );
    keys.insert("InlineAssembly(template,volatile,outputs,inputs,clobbers,results,mem_in,world_in,mem_out,world_out)");
    for evocc_evoir::AssemblyOutput {
        constraint,
        name,
        ty,
        incoming,
    } in outputs
    {
        let _ = (constraint, name, ty, incoming);
        keys.insert("AssemblyOutput(constraint,name,ty,incoming)");
    }
    for evocc_evoir::AssemblyInput {
        constraint,
        name,
        value,
    } in inputs
    {
        let _ = (constraint, name, value);
        keys.insert("AssemblyInput(constraint,name,value)");
    }
}

fn value_schema(value: &Value) -> &'static str {
    let Value { id, ty } = value;
    let _ = (id, ty);
    "Value(id,ty)"
}

fn enum_schema_keys() -> BTreeSet<&'static str> {
    let addresses = [
        AddressSpace::Generic,
        AddressSpace::Global,
        AddressSpace::Stack,
        AddressSpace::ThreadLocal,
    ];
    let regions = [
        Region::All,
        Region::Unknown,
        Region::Global,
        Region::Stack,
        Region::Heap,
        Region::Escaped,
    ];
    let comparisons = [
        CmpOp::Eq,
        CmpOp::Ne,
        CmpOp::SLt,
        CmpOp::SLe,
        CmpOp::ULt,
        CmpOp::ULe,
    ];
    let ub_reasons = [
        UbReason::BitBuiltinZeroOperand,
        UbReason::DivisionByZero,
        UbReason::RemainderByZero,
        UbReason::SignedOverflow,
        UbReason::InvalidShiftCount,
        UbReason::InvalidSignedLeftShift,
        UbReason::FloatingToInteger,
    ];
    let directions = [OffsetDirection::Add, OffsetDirection::Subtract];
    let signedness = [IndexSignedness::Signed, IndexSignedness::Unsigned];
    let kinds = [
        GlobalKind::File,
        GlobalKind::FunctionStatic,
        GlobalKind::StringLiteral,
        GlobalKind::CompoundLiteral,
    ];
    let visibility = [
        GlobalVisibility::External,
        GlobalVisibility::Internal,
        GlobalVisibility::Hidden,
        GlobalVisibility::Protected,
    ];
    let marks = [
        ParamMark::None,
        ParamMark::ByVal(RecordShape::new(24, 8)),
        ParamMark::Sret(RecordShape {
            size: 16,
            align: 16,
            vector: None,
            floating: None,
        }),
        ParamMark::SignExtend,
    ];

    let mut keys = BTreeSet::new();
    for value in addresses {
        keys.insert(match value {
            AddressSpace::Generic => "AddressSpace.Generic",
            AddressSpace::Global => "AddressSpace.Global",
            AddressSpace::Stack => "AddressSpace.Stack",
            AddressSpace::ThreadLocal => "AddressSpace.ThreadLocal",
        });
    }
    for value in regions {
        keys.insert(match value {
            Region::All => "Region.All",
            Region::Unknown => "Region.Unknown",
            Region::Global => "Region.Global",
            Region::Stack => "Region.Stack",
            Region::Heap => "Region.Heap",
            Region::Escaped => "Region.Escaped",
        });
    }
    for value in comparisons {
        keys.insert(match value {
            CmpOp::Eq => "CmpOp.Eq",
            CmpOp::Ne => "CmpOp.Ne",
            CmpOp::SLt => "CmpOp.SLt",
            CmpOp::SLe => "CmpOp.SLe",
            CmpOp::ULt => "CmpOp.ULt",
            CmpOp::ULe => "CmpOp.ULe",
        });
    }
    for value in ub_reasons {
        keys.insert(match value {
            UbReason::DivisionByZero => "UbReason.DivisionByZero",
            UbReason::BitBuiltinZeroOperand => "UbReason.BitBuiltinZeroOperand",
            UbReason::FloatingToInteger => "UbReason.FloatingToInteger",
            UbReason::RemainderByZero => "UbReason.RemainderByZero",
            UbReason::SignedOverflow => "UbReason.SignedOverflow",
            UbReason::InvalidShiftCount => "UbReason.InvalidShiftCount",
            UbReason::InvalidSignedLeftShift => "UbReason.InvalidSignedLeftShift",
        });
    }
    for value in [
        TrapReason::ScalarFunctionFellOffEnd,
        TrapReason::UnreachableReached,
        TrapReason::BuiltinTrap,
    ] {
        keys.insert(match value {
            TrapReason::ScalarFunctionFellOffEnd => "TrapReason.ScalarFunctionFellOffEnd",
            TrapReason::UnreachableReached => "TrapReason.UnreachableReached",
            TrapReason::BuiltinTrap => "TrapReason.BuiltinTrap",
        });
    }
    for value in directions {
        keys.insert(match value {
            OffsetDirection::Add => "OffsetDirection.Add",
            OffsetDirection::Subtract => "OffsetDirection.Subtract",
        });
    }
    for value in signedness {
        keys.insert(match value {
            IndexSignedness::Signed => "IndexSignedness.Signed",
            IndexSignedness::Unsigned => "IndexSignedness.Unsigned",
        });
    }
    for value in kinds {
        keys.insert(match value {
            GlobalKind::File => "GlobalKind.File",
            GlobalKind::FunctionStatic => "GlobalKind.FunctionStatic",
            GlobalKind::StringLiteral => "GlobalKind.StringLiteral",
            GlobalKind::CompoundLiteral => "GlobalKind.CompoundLiteral",
        });
    }
    for value in visibility {
        keys.insert(match value {
            GlobalVisibility::External => "GlobalVisibility.External",
            GlobalVisibility::Internal => "GlobalVisibility.Internal",
            GlobalVisibility::Hidden => "GlobalVisibility.Hidden",
            GlobalVisibility::Protected => "GlobalVisibility.Protected",
        });
    }
    for value in marks {
        keys.insert(match value {
            ParamMark::None => "ParamMark.None",
            ParamMark::ByVal(_) => "ParamMark.ByVal(shape)",
            ParamMark::Sret(_) => "ParamMark.Sret(shape)",
            ParamMark::SignExtend => "ParamMark.SignExtend",
        });
    }
    keys
}

fn parse_manifest() -> BTreeMap<&'static str, &'static str> {
    let mut entries = BTreeMap::new();
    for (line_number, line) in MANIFEST.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, sentinel) = line
            .split_once('|')
            .unwrap_or_else(|| panic!("manifest line {} has no separator", line_number + 1));
        assert!(
            !key.is_empty(),
            "manifest line {} has an empty key",
            line_number + 1
        );
        assert!(!sentinel.is_empty(), "manifest key {key} has no sentinel");
        assert!(
            entries.insert(key, sentinel).is_none(),
            "duplicate manifest key {key}"
        );
    }
    entries
}

#[test]
fn every_section9_field_has_an_independent_non_default_sentinel() {
    let module = sentinel_module();
    let rendered = render_module(&module);
    let manifest = parse_manifest();
    let mut covered = structural_schema(&module);
    covered.insert(value_schema(&value(1000, EvoIRType::Bits(37))));
    for ty in sentinel_types().into_iter().chain(half_sentinel_types()) {
        covered.insert(type_schema(&ty));
    }
    covered.extend(enum_schema_keys());

    let declared = manifest.keys().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        covered, declared,
        "manifest and exhaustive Rust schema disagree"
    );
    for (key, sentinel) in &manifest {
        assert!(
            rendered.contains(sentinel),
            "Section 9 sentinel for {key} is absent from rendered bytes: {sentinel:?}"
        );
    }

    eprintln!("section9FieldCoverage={}/{}", covered.len(), declared.len());
}
