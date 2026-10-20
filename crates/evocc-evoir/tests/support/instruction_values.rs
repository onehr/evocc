//! Shared test data, not a producer/checker implementation dependency.
//! IDs identify individual field occurrences. Repeated edge operands stay
//! repeated, so the checks detect dropped occurrences as well as wrong values.
#![allow(dead_code)]

use evocc_evoir::{
    AccessLayout, CmpOp, IndexSignedness, Inst, OffsetDirection, TrapReason, UbReason, Value,
};
use evocc_types::{AddressSpace, EvoIRType, ParamMark, RecordShape, Region};
use std::collections::HashMap;

/// Independent field-role expectations, in `sentinel_instructions` order.
/// This table must not be computed by any traversal under test.
fn expected() -> Vec<(&'static [u32], &'static [u32])> {
    vec![
        (&[2010, 2011, 2012, 2013], &[2014, 2015, 2016]), // asm
        (
            &[2010, 2011, 2012, 2013, 2014, 2015, 2016, 2017, 2015, 2016],
            &[2014, 2015, 2016],
        ), // asm.goto
        (&[2020, 2021, 2022], &[2023, 2024]),             // va.start
        (&[2020, 2021, 2022], &[2025, 2023, 2024]),       // va.arg
        (&[2020, 2021, 2022, 2026], &[2023, 2024]),       // va.arg record
        (&[2020, 2021, 2022], &[2023, 2024]),             // va.end
        (&[2031], &[2030]),                               // bit operation
        (&[], &[2040]),                                   // block address
        (&[], &[2041]),                                   // return address
        (&[], &[2042]),                                   // frame address
        (&[], &[1970]),                                   // floating constant
        (&[1970], &[1971]),                               // floating conversion predicate
        (&[1971], &[]),                                   // floating obligation
        (&[1984, 1981, 1982, 1983], &[1980, 1985]),       // strict floating
        (&[1964, 1965, 1963, 1961, 1962], &[1960, 1966, 1967]), // atomic RMW
        (&[1941, 1942], &[1940, 1943, 1944]),             // TLS
        (&[1951, 1952], &[1950]),                         // vector
        (&[1922, 1923, 1921], &[1920, 1924, 1925]),       // atomic load
        (&[1932, 1933, 1931, 1930], &[1934, 1935]),       // atomic store
        (&[1431], &[1430]),                               // pointer cast
        (&[], &[1000]),                                   // constant
        (&[], &[1010]),                                   // uninitialized
        (&[1021, 1022], &[1020]),                         // add
        (&[1031, 1032], &[1030]),                         // sub
        (&[1041, 1042], &[1040]),                         // mul
        (&[1051, 1052], &[1050]),                         // signed remainder
        (&[1061, 1062], &[1060]),                         // unsigned remainder
        (&[1071, 1072], &[1070]),                         // shift left
        (&[1081, 1082], &[1080]),                         // arithmetic shift
        (&[1091, 1092], &[1090]),                         // logical shift
        (&[1101, 1102], &[1100]),                         // and
        (&[1111, 1112], &[1110]),                         // or
        (&[1121, 1122], &[1120]),                         // xor
        (&[1131, 1132], &[1130]),                         // signed add predicate
        (&[1141, 1142], &[1140]),                         // signed sub predicate
        (&[1151, 1152], &[1150]),                         // signed mul predicate
        (&[1161], &[1160]),                               // nonzero predicate
        (&[1171, 1172], &[1170]),                         // division overflow predicate
        (&[1181, 1182], &[1180]),                         // count predicate
        (&[1191, 1192], &[1190]),                         // signed shift predicate
        (&[1200], &[]),
        (&[1201], &[]),
        (&[1202], &[]),
        (&[1203], &[]),
        (&[1204], &[]),
        (&[1205], &[]),           // obligations
        (&[1211, 1212], &[1210]), // signed division
        (&[1221, 1222], &[1220]), // unsigned division
        (&[1231, 1232], &[1230]),
        (&[1234, 1235], &[1233]),
        (&[1237, 1238], &[1236]),
        (&[1240, 1241], &[1239]),
        (&[1243, 1244], &[1242]),
        (&[1246, 1247], &[1245]),         // comparisons
        (&[1250, 1251], &[]),             // branch
        (&[1252, 1253, 1254, 1255], &[]), // indirect branch
        (&[1260, 1261, 1262], &[]),       // conditional branch
        (&[1270, 1271, 1272], &[]),       // return
        (&[], &[]),
        (&[], &[]),
        (&[], &[]), // trap reasons
        (&[], &[1280]),
        (&[], &[1290]),
        (&[], &[1300]),                             // addresses
        (&[1311, 1312], &[1310, 1313, 1314]),       // allocation
        (&[1901, 1902, 1905], &[1900, 1903, 1904]), // dynamic allocation
        (&[1320, 1321, 1322], &[1323, 1324]),       // lifetime end
        (&[1330, 1331], &[1332]),                   // pointer guard
        (&[1340, 1341], &[1342]),                   // bounds guard
        (&[1351, 1352, 1353], &[1350, 1354, 1355]), // load
        (&[1360, 1361, 1362, 1363], &[1364, 1365]), // store
        (&[1366, 1367, 1368, 1369], &[1376, 1377]), // copy
        (&[1371, 1372], &[1370]),
        (&[1374, 1375], &[1373]),                               // offsets
        (&[1382, 1383, 1384, 1385], &[1380, 1381, 1386, 1387]), // call
        (&[1392, 1393, 1394, 1395, 1396], &[1390, 1391, 1397, 1398]), // indirect call
        (&[1401], &[1400]),
        (&[1411], &[1410]),
        (&[1421], &[1420]), // extensions/truncation
    ]
}

fn ids(values: Vec<&Value>) -> Vec<u32> {
    values.into_iter().map(|value| value.id).collect()
}

pub fn assert_use_remap(remap: fn(&mut Inst, &HashMap<u32, Value>)) {
    let instructions = sentinel_instructions();
    let roles = expected();
    assert_eq!(instructions.len(), roles.len(), "missing field-role row");
    for (mut inst, (uses, results)) in instructions.into_iter().zip(roles) {
        let before = format!("{inst:?}");
        assert_eq!(ids(evocc_evoir::inst_uses(&inst)), uses, "{before}");
        assert_eq!(ids(evocc_evoir::inst_results(&inst)), results, "{before}");
        let aliases = evocc_evoir::inst_uses(&inst)
            .into_iter()
            .map(|v| (v.id, value(v.id + 10_000, v.ty.clone())))
            .collect();
        remap(&mut inst, &aliases);
        assert_eq!(
            ids(evocc_evoir::inst_uses(&inst)),
            uses.iter().map(|id| id + 10_000).collect::<Vec<_>>(),
            "{before}"
        );
        assert_eq!(ids(evocc_evoir::inst_results(&inst)), results, "{before}");
    }
}

pub fn assert_all_values(visit: for<'a> fn(&'a mut Inst) -> Vec<&'a mut Value>) {
    let instructions = sentinel_instructions();
    let roles = expected();
    assert_eq!(instructions.len(), roles.len(), "missing field-role row");
    for (mut inst, (uses, results)) in instructions.into_iter().zip(roles) {
        let before = format!("{inst:?}");
        let values = visit(&mut inst);
        let mut actual: Vec<_> = values.iter().map(|v| v.id).collect();
        let mut wanted: Vec<_> = uses.iter().chain(results).copied().collect();
        actual.sort_unstable();
        wanted.sort_unstable();
        assert_eq!(actual, wanted, "{before}");
        for value in values {
            value.id += 10_000;
        }
        assert_eq!(
            ids(evocc_evoir::inst_uses(&inst)),
            uses.iter().map(|id| id + 10_000).collect::<Vec<_>>(),
            "{before}"
        );
        assert_eq!(
            ids(evocc_evoir::inst_results(&inst)),
            results.iter().map(|id| id + 10_000).collect::<Vec<_>>(),
            "{before}"
        );
    }
}

pub fn value(id: u32, ty: EvoIRType) -> Value {
    Value { id, ty }
}

pub fn bits(id: u32) -> Value {
    value(id, EvoIRType::Bits(37))
}

pub fn mem(id: u32) -> Value {
    value(id, EvoIRType::Mem(Region::All))
}

pub fn world(id: u32) -> Value {
    value(id, EvoIRType::World)
}

pub fn pointer(id: u32) -> Value {
    value(id, EvoIRType::Ptr(AddressSpace::Stack))
}

pub fn function_pointer(id: u32) -> Value {
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

pub fn sentinel_instructions() -> Vec<Inst> {
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

pub fn sentinel_assembly() -> evocc_evoir::InlineAssembly {
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

pub fn sentinel_varargs(op: evocc_evoir::VariadicOp) -> evocc_evoir::VariadicAccess {
    evocc_evoir::VariadicAccess {
        op,
        list: pointer(2020),
        mem_in: mem(2021),
        world_in: world(2022),
        mem_out: mem(2023),
        world_out: world(2024),
    }
}
