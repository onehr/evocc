#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{
    render_module, AccessLayout, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    GlobalKind, GlobalObject, GlobalVisibility, IndexSignedness, Inst, Module, OffsetDirection,
    TrapReason, UbReason, Value,
};
use evocc_types::{AddressSpace, EvoIRType, Region};

const MANIFEST: &str = include_str!("../../../tests/contracts/day8-section9-coverage.txt");

fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

fn bits(id: u32) -> Value {
    value(id, EvoIRType::Bits(37))
}

fn mem(id: u32) -> Value {
    value(id, EvoIRType::Mem(Region::All))
}

fn world(id: u32) -> Value {
    value(id, EvoIRType::World)
}

fn pointer(id: u32) -> Value {
    value(id, EvoIRType::Ptr(AddressSpace::Stack))
}

fn function_pointer(id: u32) -> Value {
    value(
        id,
        EvoIRType::FnPtr {
            variadic: false,
            params: vec![
                EvoIRType::Bits(37),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
            results: vec![
                EvoIRType::Bits(37),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
        },
    )
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
                EvoIRType::Bits(13),
                EvoIRType::Mem(Region::All),
                EvoIRType::World,
            ],
            results: vec![
                EvoIRType::Bits(17),
                EvoIRType::Mem(Region::Escaped),
                EvoIRType::World,
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
    ]
}

fn sentinel_instructions() -> Vec<Inst> {
    vec![
        Inst::PointerCast {
            result: value(1430, EvoIRType::Bits(64)),
            operand: pointer(1431),
        },
        Inst::Const(value(1000, EvoIRType::Bits(37)), 987_654_321),
        Inst::Uninitialized(value(1010, EvoIRType::Bits(29))),
        Inst::Add(bits(1020), bits(1021), bits(1022)),
        Inst::Sub(bits(1030), bits(1031), bits(1032)),
        Inst::Mul(bits(1040), bits(1041), bits(1042)),
        Inst::SRem(bits(1050), bits(1051), bits(1052)),
        Inst::URem(bits(1060), bits(1061), bits(1062)),
        Inst::Shl(bits(1070), bits(1071), bits(1072)),
        Inst::AShr(bits(1080), bits(1081), bits(1082)),
        Inst::LShr(bits(1090), bits(1091), bits(1092)),
        Inst::And(bits(1100), bits(1101), bits(1102)),
        Inst::Or(bits(1110), bits(1111), bits(1112)),
        Inst::Xor(bits(1120), bits(1121), bits(1122)),
        Inst::SignedAddDefined(bits(1130), bits(1131), bits(1132)),
        Inst::SignedSubDefined(bits(1140), bits(1141), bits(1142)),
        Inst::SignedMulDefined(bits(1150), bits(1151), bits(1152)),
        Inst::DivisorNonZero(bits(1160), bits(1161)),
        Inst::SignedDivRemNoOverflow(bits(1170), bits(1171), bits(1172)),
        Inst::ShiftCountDefined {
            result: bits(1180),
            shifted: bits(1181),
            amount: bits(1182),
            amount_signed: true,
        },
        Inst::SignedShlDefined {
            result: bits(1190),
            shifted: bits(1191),
            amount: bits(1192),
            amount_signed: true,
        },
        Inst::UbAssume(bits(1200), UbReason::InvalidSignedLeftShift),
        Inst::UbAssume(bits(1201), UbReason::DivisionByZero),
        Inst::UbAssume(bits(1202), UbReason::RemainderByZero),
        Inst::UbAssume(bits(1203), UbReason::SignedOverflow),
        Inst::UbAssume(bits(1204), UbReason::InvalidShiftCount),
        Inst::SDiv(bits(1210), bits(1211), bits(1212)),
        Inst::UDiv(bits(1220), bits(1221), bits(1222)),
        Inst::ICmp(bits(1230), CmpOp::Eq, bits(1231), bits(1232)),
        Inst::ICmp(bits(1233), CmpOp::Ne, bits(1234), bits(1235)),
        Inst::ICmp(bits(1236), CmpOp::SLt, bits(1237), bits(1238)),
        Inst::ICmp(bits(1239), CmpOp::SLe, bits(1240), bits(1241)),
        Inst::ICmp(bits(1242), CmpOp::ULt, bits(1243), bits(1244)),
        Inst::ICmp(bits(1245), CmpOp::ULe, bits(1246), bits(1247)),
        Inst::Br(4242, vec![bits(1250), bits(1251)]),
        Inst::CondBr(bits(1260), 4242, vec![bits(1261)], 4343, vec![bits(1262)]),
        Inst::Ret(vec![bits(1270), mem(1271), world(1272)]),
        Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
        Inst::Trap(TrapReason::UnreachableReached),
        Inst::PtrNull(pointer(1280)),
        Inst::GlobalAddr {
            result: value(1290, EvoIRType::Ptr(AddressSpace::Global)),
            symbol: "sentinel_global_target".to_owned(),
        },
        Inst::FuncAddr {
            result: function_pointer(1300),
            symbol: "sentinel_function_target".to_owned(),
        },
        Inst::Alloca {
            result: pointer(1310),
            mem_in: mem(1311),
            world_in: world(1312),
            mem_out: mem(1313),
            world_out: world(1314),
            origin: 1_313_131,
            layout: AccessLayout::new(53, 16),
        },
        Inst::LifetimeEnd {
            mem_in: mem(1320),
            world_in: world(1321),
            pointer: pointer(1322),
            mem_out: mem(1323),
            world_out: world(1324),
        },
        Inst::PointerValid {
            mem_in: mem(1330),
            pointer: pointer(1331),
            mem_out: mem(1332),
        },
        Inst::AccessInBounds {
            mem_in: mem(1340),
            pointer: pointer(1341),
            mem_out: mem(1342),
            layout: AccessLayout::new(59, 32),
        },
        Inst::Load {
            result: bits(1350),
            address: pointer(1351),
            mem_in: mem(1352),
            world_in: world(1353),
            mem_out: mem(1354),
            world_out: world(1355),
            volatile: true,
        },
        Inst::Store {
            address: pointer(1360),
            value: bits(1361),
            mem_in: mem(1362),
            world_in: world(1363),
            mem_out: mem(1364),
            world_out: world(1365),
            volatile: true,
        },
        Inst::Copy {
            destination: pointer(1366),
            source: pointer(1367),
            mem_in: mem(1368),
            world_in: world(1369),
            mem_out: mem(1376),
            world_out: world(1377),
            layout: AccessLayout::new(64, 16),
        },
        Inst::PtrOffset {
            result: pointer(1370),
            base: pointer(1371),
            index: bits(1372),
            direction: OffsetDirection::Subtract,
            signedness: IndexSignedness::Signed,
            scale: 61,
        },
        Inst::PtrOffset {
            result: pointer(1373),
            base: pointer(1374),
            index: bits(1375),
            direction: OffsetDirection::Add,
            signedness: IndexSignedness::Unsigned,
            scale: 67,
        },
        Inst::Call {
            results: vec![bits(1380), bits(1381)],
            callee: "sentinel_direct_callee".to_owned(),
            args: vec![bits(1382), bits(1383)],
            mem_in: mem(1384),
            world_in: world(1385),
            mem_out: mem(1386),
            world_out: world(1387),
        },
        Inst::IndirectCall {
            results: vec![bits(1390), bits(1391)],
            callee: function_pointer(1392),
            args: vec![bits(1393), bits(1394)],
            mem_in: mem(1395),
            world_in: world(1396),
            mem_out: mem(1397),
            world_out: world(1398),
        },
        Inst::SExt(
            bits(1400),
            value(1401, EvoIRType::Bits(11)),
            EvoIRType::Bits(37),
        ),
        Inst::ZExt(
            bits(1410),
            value(1411, EvoIRType::Bits(12)),
            EvoIRType::Bits(37),
        ),
        Inst::Trunc(
            value(1420, EvoIRType::Bits(9)),
            bits(1421),
            EvoIRType::Bits(9),
        ),
    ]
}

fn sentinel_module() -> Module {
    let types = sentinel_types();
    let params = types
        .into_iter()
        .enumerate()
        .map(|(index, ty)| FunctionParam {
            name: format!("ignored_source_name_{index}"),
            ty,
        })
        .collect();
    Module {
        assembly: vec![".globl sentinel_asm\n".into()],
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: vec![
            GlobalObject {
                relocs: Vec::new(),
                symbol: "sentinel_file".to_owned(),
                origin: 7001,
                kind: GlobalKind::File,
                visibility: GlobalVisibility::External,
                layout: AccessLayout::new(3, 4),
                bytes: vec![0xa1, 0xb2, 0xc3],
                initialized: vec![true, false, true],
            },
            GlobalObject {
                relocs: Vec::new(),
                symbol: "__evocc.static.sentinel.7002".to_owned(),
                origin: 7002,
                kind: GlobalKind::FunctionStatic,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(2, 8),
                bytes: vec![0xd4, 0xe5],
                initialized: vec![false, true],
            },
            GlobalObject {
                relocs: Vec::new(),
                symbol: "__evocc.string.7003".to_owned(),
                origin: 7003,
                kind: GlobalKind::StringLiteral,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(1, 16),
                bytes: vec![0xf6],
                initialized: vec![true],
            },
            GlobalObject {
                symbol: "__evocc.compound.7004".into(),
                origin: 7004,
                kind: GlobalKind::CompoundLiteral,
                visibility: GlobalVisibility::Internal,
                layout: AccessLayout::new(16, 8),
                bytes: vec![0; 16],
                initialized: vec![true; 16],
                relocs: vec![evocc_evoir::Relocation {
                    offset: 8,
                    symbol: "sentinel_file".into(),
                    addend: -3,
                }],
            },
        ],
        functions: vec![Function {
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
        }],
    }
}

fn inst_schema(inst: &Inst) -> &'static str {
    match inst {
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
            let _ = access;
            "Inst.VaAccess(access)"
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
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
        } => {
            let _ = (
                result, address, mem_in, world_in, mem_out, world_out, volatile,
            );
            "Inst.Load(result,address,mem_in,world_in,mem_out,world_out,volatile)"
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile,
        } => {
            let _ = (
                address, value, mem_in, world_in, mem_out, world_out, volatile,
            );
            "Inst.Store(address,value,mem_in,world_in,mem_out,world_out,volatile)"
        }
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
        } => {
            let _ = (
                destination,
                source,
                mem_in,
                world_in,
                mem_out,
                world_out,
                layout,
            );
            "Inst.Copy(destination,source,mem_in,world_in,mem_out,world_out,layout)"
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
            results,
            callee,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let _ = (results, callee, args, mem_in, world_in, mem_out, world_out);
            "Inst.Call(results,callee,args,mem_in,world_in,mem_out,world_out)"
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
            let _ = (results, callee, args, mem_in, world_in, mem_out, world_out);
            "Inst.IndirectCall(results,callee,args,mem_in,world_in,mem_out,world_out)"
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
        EvoIRType::F32 => "EvoIRType.F32",
        EvoIRType::F64 => "EvoIRType.F64",
        EvoIRType::Ptr(address_space) => {
            let _ = address_space;
            "EvoIRType.Ptr(address_space)"
        }
        EvoIRType::FnPtr {
            params,
            results,
            variadic,
        } => {
            let _ = (params, results, variadic);
            "EvoIRType.FnPtr(params,results,variadic)"
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
        assembly,
        globals,
        extern_objects,
        declares,
        functions,
    } = module;
    let _ = (assembly, extern_objects, declares);
    let mut keys = BTreeSet::from(["Module(assembly,globals,extern_objects,declares,functions)"]);
    for global in globals {
        let GlobalObject {
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
            symbol,
            origin,
            kind,
            visibility,
            layout,
            bytes,
            initialized,
            relocs,
        );
        keys.insert("GlobalObject(symbol,origin,kind,visibility,layout,bytes,initialized,relocs)");
        let AccessLayout {
            size_bytes,
            align_bytes,
        } = layout;
        let _ = (size_bytes, align_bytes);
        keys.insert("AccessLayout(size_bytes,align_bytes)");
    }
    for function in functions {
        let Function {
            name,
            params,
            variadic,
            ret_tys,
            effects,
            blocks,
        } = function;
        let _ = (name, ret_tys, variadic);
        keys.insert("Function(name,params,variadic,ret_tys,effects,blocks)");
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
            may_trap,
            may_unwind,
        } = effects;
        let _ = (
            reads_all, writes_all, volatile, atomic, io, may_trap, may_unwind,
        );
        keys.insert("EffectSummary(reads_all,writes_all,volatile,atomic,io,may_trap,may_unwind)");
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
            }
        }
    }
    keys
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
        UbReason::DivisionByZero,
        UbReason::RemainderByZero,
        UbReason::SignedOverflow,
        UbReason::InvalidShiftCount,
        UbReason::InvalidSignedLeftShift,
    ];
    let directions = [OffsetDirection::Add, OffsetDirection::Subtract];
    let signedness = [IndexSignedness::Signed, IndexSignedness::Unsigned];
    let kinds = [
        GlobalKind::File,
        GlobalKind::FunctionStatic,
        GlobalKind::StringLiteral,
        GlobalKind::CompoundLiteral,
    ];
    let visibility = [GlobalVisibility::External, GlobalVisibility::Internal];

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
            UbReason::RemainderByZero => "UbReason.RemainderByZero",
            UbReason::SignedOverflow => "UbReason.SignedOverflow",
            UbReason::InvalidShiftCount => "UbReason.InvalidShiftCount",
            UbReason::InvalidSignedLeftShift => "UbReason.InvalidSignedLeftShift",
        });
    }
    for value in [
        TrapReason::ScalarFunctionFellOffEnd,
        TrapReason::UnreachableReached,
    ] {
        keys.insert(match value {
            TrapReason::ScalarFunctionFellOffEnd => "TrapReason.ScalarFunctionFellOffEnd",
            TrapReason::UnreachableReached => "TrapReason.UnreachableReached",
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
    for ty in sentinel_types() {
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
