//! The proof input must retain AST distinctions that ordinary MIR text hides.
use evocc_mir::core_machine_export::{export_core_machine, CoreMachineMode};
use evocc_mir::{Cond, MBlock, MFunction, MInst, MModule, MTerm, RegClass, VReg, Width};

fn reg(id: u32, class: RegClass) -> VReg {
    VReg { id, class }
}

fn module(term: MTerm) -> MModule {
    MModule {
        globals: vec![],
        functions: vec![MFunction {
            name: "named\"function\\\n".into(),
            blocks: vec![MBlock {
                id: 17,
                args: vec![reg(0, RegClass::Gpr32)],
                insts: vec![MInst::MovImm {
                    dst: reg(1, RegClass::Gpr64),
                    imm: u64::MAX,
                }],
                term,
            }],
        }],
    }
}

#[test]
fn conditional_branch_retains_explicit_flags() {
    let branch = |flags| MTerm::BCond {
        flags: reg(flags, RegClass::Flags),
        cond: Cond::Ne,
        taken: 2,
        taken_args: vec![reg(9, RegClass::Gpr64)],
        untaken: 3,
        untaken_args: vec![reg(8, RegClass::Gpr32)],
    };
    let first = export_core_machine(&module(branch(7)), CoreMachineMode::Ssa).unwrap();
    let second = export_core_machine(&module(branch(6)), CoreMachineMode::Ssa).unwrap();
    assert_ne!(first, second);
    assert!(first.contains("[\"b_cond\",[7,\"flags\"],\"ne\",2,[[9,\"gpr64\"]],3,[[8,\"gpr32\"]]]"));
    assert!(first.contains("18446744073709551615"));
    assert!(first.contains("named\\\"function\\\\\\u000a"));
}

#[test]
fn snapshot_preserves_width_classes_and_offsets() {
    let mut input = module(MTerm::Ret { values: vec![] });
    input.functions[0].blocks[0].insts = vec![
        MInst::Binary {
            op: evocc_mir::BinOp::Add,
            dst: reg(2, RegClass::Gpr64),
            lhs: reg(0, RegClass::Gpr32),
            rhs: reg(1, RegClass::Gpr64),
            width: Width::W,
        },
        MInst::Load {
            align: 1,
            dst: reg(3, RegClass::Gpr32),
            base: reg(4, RegClass::Gpr64),
            offset: i64::MIN,
            kind: evocc_mir::LoadKind::W,
            volatile: true,
            mask: None,
        },
    ];
    let text = export_core_machine(&input, CoreMachineMode::Ssa).unwrap();
    assert!(text.contains("[\"binary\",\"add\",[2,\"gpr64\"],[0,\"gpr32\"],[1,\"gpr64\"],\"w\"]"));
    assert!(
        text.contains("[\"load\",1,[3,\"gpr32\"],[4,\"gpr64\"],-9223372036854775808,\"w\",true]")
    );
    // Export preserves even an out-of-profile effect instead of silently
    // dropping it. The independent semantic checker must refuse volatility.
    assert_ne!(
        text,
        export_core_machine(&input, CoreMachineMode::PostSsa).unwrap()
    );
}

/// A mask comes only from a bit-field access, which `core-v1` excludes, so
/// the export refuses a masked load or store instead of dropping the mask.
#[test]
fn a_masked_access_is_outside_the_core_profile() {
    for inst in [
        MInst::Load {
            align: 0,
            dst: reg(3, RegClass::Gpr32),
            base: reg(4, RegClass::Gpr64),
            offset: 0,
            kind: evocc_mir::LoadKind::W,
            volatile: false,
            mask: Some(7),
        },
        MInst::Store {
            align: 0,
            src: reg(3, RegClass::Gpr32),
            base: reg(4, RegClass::Gpr64),
            offset: 0,
            kind: evocc_mir::StoreKind::W,
            volatile: false,
            mask: Some(0),
        },
    ] {
        let mut input = module(MTerm::Ret { values: vec![] });
        input.functions[0].blocks[0].insts = vec![inst];
        for mode in [CoreMachineMode::Ssa, CoreMachineMode::PostSsa] {
            assert!(export_core_machine(&input, mode).is_err());
        }
    }
}

#[test]
fn unsupported_instruction_is_not_erased() {
    let mut input = module(MTerm::Ret { values: vec![] });
    input.functions[0].blocks[0].insts.push(MInst::IncomingArg {
        dst: reg(2, RegClass::Gpr64),
        slot: 8,
    });
    assert!(export_core_machine(&input, CoreMachineMode::Ssa).is_err());
}
