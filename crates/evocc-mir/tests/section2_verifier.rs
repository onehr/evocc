//! Section 2: block-argument SSA over virtual registers, and the `flags`
//! adjacency rule.
//!
//! "A `flags` value may be consumed only by the instruction that immediately
//! follows its producer in the same block, and nothing between them may write
//! flags. This is a legality rule the verifier of Section 9 checks, not a
//! hint."

use evocc_mir::{
    validate_module, BinOp, Cond, MBlock, MFunction, MInst, MModule, MTerm, RegClass, VReg, Width,
};

fn g(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn f(id: u32) -> VReg {
    VReg::new(id, RegClass::Flags)
}

fn one_block(args: Vec<VReg>, insts: Vec<MInst>, term: MTerm) -> MModule {
    MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![MBlock {
                id: 0,
                args,
                insts,
                term,
            }],
        }],
    }
}

#[test]
fn a_flags_value_consumed_by_the_next_instruction_is_legal() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
    );
    assert_eq!(validate_module(&module), Ok(()));
}

#[test]
fn a_flags_value_consumed_by_the_terminator_after_its_producer_is_legal() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![MInst::Cmp {
            dst: f(2),
            lhs: w(0),
            rhs: w(1),
            width: Width::W,
        }],
        MTerm::BCond {
            flags: f(2),
            cond: Cond::Lo,
            taken: 0,
            taken_args: vec![w(0), w(1)],
            untaken: 0,
            untaken_args: vec![w(0), w(1)],
        },
    );
    assert_eq!(validate_module(&module), Ok(()));
}

#[test]
fn a_flags_value_consumed_two_instructions_later_is_refused() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Binary {
                op: BinOp::Add,
                dst: w(4),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_second_flags_writer_between_producer_and_consumer_is_refused() {
    let module = one_block(
        vec![w(0), w(1)],
        vec![
            MInst::Cmp {
                dst: f(2),
                lhs: w(0),
                rhs: w(1),
                width: Width::W,
            },
            MInst::Cmp {
                dst: f(5),
                lhs: w(1),
                rhs: w(0),
                width: Width::W,
            },
            MInst::Cset {
                dst: w(3),
                flags: f(2),
                cond: Cond::Eq,
            },
        ],
        MTerm::Ret { value: Some(w(3)) },
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_flags_value_that_crosses_a_block_edge_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: vec![MInst::Cmp {
                        dst: f(1),
                        lhs: w(0),
                        rhs: w(0),
                        width: Width::W,
                    }],
                    term: MTerm::B {
                        target: 1,
                        args: vec![f(1)],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![f(2)],
                    insts: vec![MInst::Cset {
                        dst: w(3),
                        flags: f(2),
                        cond: Cond::Eq,
                    }],
                    term: MTerm::Ret { value: Some(w(3)) },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_virtual_register_defined_twice_is_refused() {
    let module = one_block(
        vec![w(0)],
        vec![
            MInst::MovImm { dst: w(1), imm: 1 },
            MInst::MovImm { dst: w(1), imm: 2 },
        ],
        MTerm::Ret { value: Some(w(1)) },
    );
    assert!(validate_module(&module).is_err());
}

#[test]
fn a_use_without_a_definition_is_refused() {
    let module = one_block(vec![w(0)], Vec::new(), MTerm::Ret { value: Some(w(9)) });
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_edge_whose_arity_does_not_match_the_target_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![w(0)],
                    insts: Vec::new(),
                    term: MTerm::B {
                        target: 1,
                        args: Vec::new(),
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![w(1)],
                    insts: Vec::new(),
                    term: MTerm::Ret { value: Some(w(1)) },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_edge_whose_class_does_not_match_the_target_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![g(0)],
                    insts: Vec::new(),
                    term: MTerm::B {
                        target: 1,
                        args: vec![g(0)],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![w(1)],
                    insts: Vec::new(),
                    term: MTerm::Ret { value: Some(w(1)) },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}

#[test]
fn an_unreachable_block_is_refused() {
    let module = MModule {
        globals: Vec::new(),
        functions: vec![MFunction {
            name: "f".to_owned(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Ret { value: None },
                },
                MBlock {
                    id: 1,
                    args: Vec::new(),
                    insts: Vec::new(),
                    term: MTerm::Ret { value: None },
                },
            ],
        }],
    };
    assert!(validate_module(&module).is_err());
}
