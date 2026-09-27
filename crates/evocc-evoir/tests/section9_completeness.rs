#![allow(clippy::too_many_lines)]

use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{
    render_module, AccessLayout, Block, BlockArg, CmpOp, EffectSummary, Function, FunctionParam,
    GlobalKind, GlobalObject, GlobalVisibility, IndexSignedness, Inst, Module, OffsetDirection,
    TrapReason, UbReason, Value,
};
use evocc_types::{AddressSpace, EvoIRType, ParamMark, RecordShape, Region};

const MANIFEST: &str = include_str!("../../../tests/contracts/day8-section9-coverage.txt");

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
            marks: Vec::new(),
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

fn sentinel_instructions() -> Vec<Inst> {
    vec![
        Inst::Asm(sentinel_assembly()),
        Inst::AsmGoto {
            assembly: sentinel_assembly(),
            labels: vec![evocc_evoir::AssemblyLabel {
                name: "sentinel_label".into(),
                edge: evocc_evoir::AssemblyEdge {
                    target: 4242,
                    arguments: vec![bits(2014), mem(2015), world(2016)],
                },
            }],
            fallthrough: evocc_evoir::AssemblyEdge {
                target: 4343,
                arguments: vec![bits(2017), mem(2015), world(2016)],
            },
        },
        Inst::VaAccess(sentinel_varargs(evocc_evoir::VariadicOp::Start)),
        Inst::VaAccess(sentinel_varargs(evocc_evoir::VariadicOp::Arg(value(
            2025,
            EvoIRType::F128,
        )))),
        Inst::VaAccess(sentinel_varargs(evocc_evoir::VariadicOp::ArgRecord {
            destination: pointer(2026),
            shape: RecordShape {
                size: 32,
                align: 16,
                vector: None,
                floating: Some(evocc_types::FloatAggregate {
                    kind: evocc_types::FloatKind::F64,
                    count: 4,
                    abi_align: 16,
                }),
            },
        })),
        Inst::VaAccess(sentinel_varargs(evocc_evoir::VariadicOp::End)),
        Inst::BitOperation {
            result: bits(2030),
            operand: bits(2031),
            op: evocc_evoir::BitOp::ByteSwap,
        },
        Inst::BlockAddr {
            result: pointer(2040),
            block: 4242,
        },
        Inst::ReturnAddress {
            result: pointer(2041),
        },
        Inst::FrameAddress {
            result: pointer(2042),
            depth: 3,
        },
        Inst::Floating(evocc_evoir::FloatingInst {
            result: value(1970, EvoIRType::F64),
            op: evocc_evoir::FloatingOp::Constant(0xfff8000000000123),
            arguments: vec![],
        }),
        Inst::Floating(evocc_evoir::FloatingInst {
            result: value(1971, EvoIRType::Bits(1)),
            op: evocc_evoir::FloatingOp::ToIntDefined {
                width: 73,
                signed: true,
            },
            arguments: vec![value(1970, EvoIRType::F64)],
        }),
        Inst::UbAssume(value(1971, EvoIRType::Bits(1)), UbReason::FloatingToInteger),
        Inst::StrictFloating(evocc_evoir::StrictFloatingInst {
            instruction: evocc_evoir::FloatingInst {
                result: value(1980, EvoIRType::F64),
                op: evocc_evoir::FloatingOp::Fma,
                arguments: vec![
                    value(1981, EvoIRType::F64),
                    value(1982, EvoIRType::F64),
                    value(1983, EvoIRType::F64),
                ],
            },
            world_in: value(1984, EvoIRType::World),
            world_out: value(1985, EvoIRType::World),
        }),
        Inst::Atomic(evocc_evoir::AtomicAccess {
            op: evocc_evoir::AtomicOp::Rmw {
                result: value(1960, EvoIRType::Bits(16)),
                operation: evocc_evoir::AtomicRmwOp::CompareExchange,
                value: value(1961, EvoIRType::Bits(16)),
                expected: Some(value(1962, EvoIRType::Bits(16))),
            },
            address: pointer(1963),
            order: evocc_evoir::AtomicOrder::AcqRel,
            volatile: true,
            mem_in: mem(1964),
            world_in: world(1965),
            mem_out: mem(1966),
            world_out: world(1967),
        }),
        Inst::TlsAddr(evocc_evoir::TlsAddress {
            result: pointer(1940),
            symbol: "sentinel_file".into(),
            mem_in: mem(1941),
            world_in: world(1942),
            mem_out: mem(1943),
            world_out: world(1944),
        }),
        Inst::Vector(evocc_evoir::VectorInst {
            result: value(1950, EvoIRType::Vec(2, Box::new(EvoIRType::Bits(32)))),
            op: evocc_evoir::VectorOp::Pack,
            arguments: vec![
                value(1951, EvoIRType::Bits(32)),
                value(1952, EvoIRType::Bits(32)),
            ],
        }),
        Inst::Atomic(evocc_evoir::AtomicAccess {
            op: evocc_evoir::AtomicOp::Load(value(1920, EvoIRType::Bits(32))),
            address: pointer(1921),
            order: evocc_evoir::AtomicOrder::Consume,
            volatile: true,
            mem_in: mem(1922),
            world_in: world(1923),
            mem_out: mem(1924),
            world_out: world(1925),
        }),
        Inst::Atomic(evocc_evoir::AtomicAccess {
            op: evocc_evoir::AtomicOp::Store(value(1930, EvoIRType::Bits(64))),
            address: pointer(1931),
            order: evocc_evoir::AtomicOrder::SeqCst,
            volatile: false,
            mem_in: mem(1932),
            world_in: world(1933),
            mem_out: mem(1934),
            world_out: world(1935),
        }),
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
        Inst::UbAssume(bits(1205), UbReason::BitBuiltinZeroOperand),
        Inst::SDiv(bits(1210), bits(1211), bits(1212)),
        Inst::UDiv(bits(1220), bits(1221), bits(1222)),
        Inst::ICmp(bits(1230), CmpOp::Eq, bits(1231), bits(1232)),
        Inst::ICmp(bits(1233), CmpOp::Ne, bits(1234), bits(1235)),
        Inst::ICmp(bits(1236), CmpOp::SLt, bits(1237), bits(1238)),
        Inst::ICmp(bits(1239), CmpOp::SLe, bits(1240), bits(1241)),
        Inst::ICmp(bits(1242), CmpOp::ULt, bits(1243), bits(1244)),
        Inst::ICmp(bits(1245), CmpOp::ULe, bits(1246), bits(1247)),
        Inst::Br(4242, vec![bits(1250), bits(1251)]),
        Inst::IndirectBr {
            target: pointer(1252),
            edges: vec![evocc_evoir::Edge {
                target: 4242,
                arguments: vec![pointer(1253), mem(1254), world(1255)],
            }],
        },
        Inst::CondBr(bits(1260), 4242, vec![bits(1261)], 4343, vec![bits(1262)]),
        Inst::Ret(vec![bits(1270), mem(1271), world(1272)]),
        Inst::Trap(TrapReason::ScalarFunctionFellOffEnd),
        Inst::Trap(TrapReason::UnreachableReached),
        Inst::Trap(TrapReason::BuiltinTrap),
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
        Inst::DynamicAlloca {
            result: pointer(1900),
            mem_in: mem(1901),
            world_in: world(1902),
            mem_out: mem(1903),
            world_out: world(1904),
            origin: 1_901_901,
            size: value(1905, EvoIRType::Bits(64)),
            align: 32,
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
            layout: AccessLayout::new(8, 16),
            volatile: evocc_evoir::CopyVolatility::ReadWrite,
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
            arg_marks: vec![ParamMark::None, ParamMark::ByVal(RecordShape::new(24, 8))],
            results: vec![bits(1380), bits(1381)],
            callee: "sentinel_direct_callee".to_owned(),
            args: vec![bits(1382), bits(1383)],
            mem_in: mem(1384),
            world_in: world(1385),
            mem_out: mem(1386),
            world_out: world(1387),
        },
        Inst::IndirectCall {
            arg_marks: vec![ParamMark::None, ParamMark::ByVal(RecordShape::new(16, 16))],
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

fn sentinel_assembly() -> evocc_evoir::InlineAssembly {
    evocc_evoir::InlineAssembly {
        template: "sentinel %0, %[input]".into(),
        volatile: true,
        outputs: vec![evocc_evoir::AssemblyOutput {
            constraint: "+&r@x7".into(),
            name: "output".into(),
            ty: EvoIRType::Bits(37),
            incoming: Some(bits(2010)),
        }],
        inputs: vec![evocc_evoir::AssemblyInput {
            constraint: "r@x9".into(),
            name: "input".into(),
            value: bits(2011),
        }],
        clobbers: vec!["memory".into(), "cc".into()],
        results: vec![bits(2014)],
        mem_in: mem(2012),
        world_in: world(2013),
        mem_out: mem(2015),
        world_out: world(2016),
    }
}

fn sentinel_varargs(op: evocc_evoir::VariadicOp) -> evocc_evoir::VariadicAccess {
    evocc_evoir::VariadicAccess {
        op,
        list: pointer(2020),
        mem_in: mem(2021),
        world_in: world(2022),
        mem_out: mem(2023),
        world_out: world(2024),
    }
}

#[test]
fn newly_covered_instructions_keep_every_operand_when_remapped() {
    // These expectations are independent of both traversal implementations.
    // In particular asm.goto repeats its output tokens on mutually exclusive
    // edges; occurrence/order must not be reduced to a set of Value IDs.
    let cases: &[(&[u32], &[u32])] = &[
        (&[2010, 2011, 2012, 2013], &[2014, 2015, 2016]),
        (
            &[2010, 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2015, 2016],
            &[2014, 2015, 2016],
        ),
        (&[2020, 2021, 2022], &[2023, 2024]),
        (&[2020, 2021, 2022], &[2025, 2023, 2024]),
        (&[2020, 2021, 2022, 2026], &[2023, 2024]),
        (&[2020, 2021, 2022], &[2023, 2024]),
        (&[2031], &[2030]),
        (&[], &[2040]),
        (&[], &[2041]),
        (&[], &[2042]),
    ];
    for (mut instruction, &(uses, results)) in sentinel_instructions().into_iter().zip(cases) {
        let ids = |values: Vec<&Value>| values.into_iter().map(|v| v.id).collect::<Vec<_>>();
        assert_eq!(ids(evocc_evoir::inst_uses(&instruction)), uses);
        assert_eq!(ids(evocc_evoir::inst_results(&instruction)), results);
        let aliases = evocc_evoir::inst_uses(&instruction)
            .into_iter()
            .map(|v| (v.id, value(v.id + 10_000, v.ty.clone())))
            .collect();
        evocc_evoir::remap_inst_uses(&mut instruction, &aliases);
        assert_eq!(
            ids(evocc_evoir::inst_uses(&instruction)),
            uses.iter().map(|id| id + 10_000).collect::<Vec<_>>()
        );
        assert_eq!(ids(evocc_evoir::inst_results(&instruction)), results);
    }
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
        declares: vec![evocc_evoir::FunctionDeclaration {
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
        }],
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
            "Inst.Call(results,callee,args,arg_marks,mem_in,world_in,mem_out,world_out)"
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
