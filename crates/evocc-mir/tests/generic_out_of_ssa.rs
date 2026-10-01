//! N03a step 2b: run the actual shared SSA destruction on a test-only ISA.
//! This is not a compilation target or a replacement interpreter.

use std::collections::BTreeMap;

use evocc_mir::generic::machine::{MachInst, MachTerm, MachineBlock, MachineFunction};
use evocc_mir::generic::register::{RegisterClass, VirtualReg};
use evocc_mir::generic::ssa::{out_of_ssa_with_dce, SsaInst, SsaTerm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Small,
    Wide,
    Condition,
}

impl RegisterClass for Class {
    fn is_flags(self) -> bool {
        self == Self::Condition
    }
}

type Reg = VirtualReg<Class>;
type Block = MachineBlock<Inst, Term, Class>;
type Function = MachineFunction<Inst, Term, Class>;

fn r(id: u32) -> Reg {
    Reg::new(id, Class::Small)
}

fn flags(id: u32) -> Reg {
    Reg::new(id, Class::Condition)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Inst {
    Constant(Reg, u64),
    Copy(Reg, Reg),
    Binary(Reg, Reg, Reg),
    Load(Reg, Reg),
    Store(Reg, Reg),
    Flagged(Reg, Reg, Reg, Reg),
    CaptureCondition(Reg, Reg),
    Fixed(Reg),
}

impl MachInst for Inst {
    type Reg = Reg;

    fn defs(&self) -> Vec<Reg> {
        match self {
            Self::Constant(dst, _)
            | Self::Copy(dst, _)
            | Self::Binary(dst, _, _)
            | Self::Load(dst, _)
            | Self::CaptureCondition(dst, _) => vec![*dst],
            Self::Flagged(dst, condition, _, _) => vec![*dst, *condition],
            Self::Store(_, _) | Self::Fixed(_) => vec![],
        }
    }

    fn uses(&self) -> Vec<Reg> {
        match self {
            Self::Constant(_, _) => vec![],
            Self::Copy(_, src)
            | Self::Load(_, src)
            | Self::CaptureCondition(_, src)
            | Self::Fixed(src) => vec![*src],
            Self::Binary(_, lhs, rhs) | Self::Store(lhs, rhs) | Self::Flagged(_, _, lhs, rhs) => {
                vec![*lhs, *rhs]
            }
        }
    }

    fn operand_slots(&mut self, definitions: bool) -> Vec<&mut Reg> {
        if definitions {
            match self {
                Self::Constant(dst, _)
                | Self::Copy(dst, _)
                | Self::Binary(dst, _, _)
                | Self::Load(dst, _)
                | Self::CaptureCondition(dst, _) => vec![dst],
                Self::Flagged(dst, condition, _, _) => vec![dst, condition],
                Self::Store(_, _) | Self::Fixed(_) => vec![],
            }
        } else {
            match self {
                Self::Constant(_, _) => vec![],
                Self::Copy(_, src)
                | Self::Load(_, src)
                | Self::CaptureCondition(_, src)
                | Self::Fixed(src) => vec![src],
                Self::Binary(_, lhs, rhs)
                | Self::Store(lhs, rhs)
                | Self::Flagged(_, _, lhs, rhs) => vec![lhs, rhs],
            }
        }
    }

    fn writes_flags(&self) -> bool {
        matches!(self, Self::Flagged(..))
    }

    fn is_call(&self) -> bool {
        false
    }

    fn discardable(&self) -> bool {
        !matches!(self, Self::Load(..) | Self::Store(..) | Self::Fixed(..))
    }
}

impl SsaInst for Inst {
    fn copy(dst: Reg, src: Reg) -> Self {
        Self::Copy(dst, src)
    }

    fn has_fixed_binding(&self) -> bool {
        matches!(self, Self::Fixed(..))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Jump(u32, Vec<Reg>),
    Choose(Reg, [(u32, Vec<Reg>); 2]),
    Dispatch(Reg, Vec<(u32, Vec<Reg>)>),
    Return(Vec<Reg>),
}

impl MachTerm for Term {
    type Reg = Reg;

    fn uses(&self) -> Vec<Reg> {
        match self {
            Self::Jump(_, args) | Self::Return(args) => args.clone(),
            Self::Choose(src, edges) => std::iter::once(*src)
                .chain(edges.iter().flat_map(|(_, args)| args.iter().copied()))
                .collect(),
            Self::Dispatch(src, edges) => std::iter::once(*src)
                .chain(edges.iter().flat_map(|(_, args)| args.iter().copied()))
                .collect(),
        }
    }

    fn operand_slots(&mut self) -> Vec<&mut Reg> {
        match self {
            Self::Jump(..) => vec![],
            Self::Choose(src, _) => vec![src],
            Self::Dispatch(src, edges) => std::iter::once(src)
                .chain(edges.iter_mut().flat_map(|(_, args)| args.iter_mut()))
                .collect(),
            Self::Return(args) => args.iter_mut().collect(),
        }
    }

    fn edges(&self) -> Vec<(u32, &[Reg])> {
        match self {
            Self::Jump(target, args) => vec![(*target, args)],
            Self::Choose(_, edges) => edges
                .iter()
                .map(|(id, args)| (*id, args.as_slice()))
                .collect(),
            Self::Dispatch(_, edges) => edges
                .iter()
                .map(|(id, args)| (*id, args.as_slice()))
                .collect(),
            Self::Return(_) => vec![],
        }
    }

    fn is_exit(&self) -> bool {
        matches!(self, Self::Return(_))
    }
}

impl SsaTerm for Term {
    fn indirect_target(&self) -> Option<Reg> {
        if let Self::Dispatch(src, _) = self {
            Some(*src)
        } else {
            None
        }
    }

    fn set_indirect_target(&mut self, reg: Reg) {
        let Self::Dispatch(src, _) = self else {
            panic!("only a dispatch has an indirect target");
        };
        *src = reg;
    }

    fn clear_edge_args(&mut self) {
        match self {
            Self::Jump(_, args) => args.clear(),
            Self::Choose(_, edges) => {
                for (_, args) in edges {
                    args.clear();
                }
            }
            Self::Dispatch(_, edges) => {
                for (_, args) in edges {
                    args.clear();
                }
            }
            Self::Return(_) => {}
        }
    }

    fn retarget(&mut self, index: usize, target: u32) {
        let (destination, args) = match self {
            Self::Jump(destination, args) if index == 0 => (destination, args),
            Self::Choose(_, edges) => {
                let (destination, args) = &mut edges[index];
                (destination, args)
            }
            _ => panic!("computed labels and exits cannot be retargeted"),
        };
        *destination = target;
        args.clear();
    }

    fn jump(target: u32, args: Vec<Reg>) -> Self {
        Self::Jump(target, args)
    }

    fn prevents_forwarding(&self) -> bool {
        matches!(self, Self::Dispatch(..))
    }
}

fn straight() -> Function {
    Function {
        name: "toy".into(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![r(0), r(1)],
                insts: vec![Inst::Binary(r(2), r(0), r(1))],
                term: Term::Jump(1, vec![r(2)]),
            },
            Block {
                id: 1,
                args: vec![r(3)],
                insts: vec![],
                term: Term::Return(vec![r(3)]),
            },
        ],
    }
}

fn copies(function: &Function) -> Vec<(Reg, Reg)> {
    function
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .filter_map(|inst| {
            if let Inst::Copy(dst, src) = inst {
                Some((*dst, *src))
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn shared_containers_keep_the_public_compatibility_types() {
    // This checks type identity, not conversion through a second machine IR.
    let function: MachineFunction<evocc_mir::MInst, evocc_mir::MTerm, evocc_mir::RegClass> =
        evocc_mir::MFunction {
            name: "compatibility".into(),
            blocks: vec![],
        };
    assert_eq!(
        format!("{function:?}"),
        "MFunction { name: \"compatibility\", blocks: [] }"
    );
    let _: evocc_mir::MFunction = function;
    let reg: VirtualReg<evocc_mir::RegClass> = evocc_mir::VReg::new(3, evocc_mir::RegClass::Gpr64);
    assert_eq!(reg.id, 3);
    assert_eq!(format!("{reg:?}"), "VReg { id: 3, class: Gpr64 }");
    let block = evocc_mir::MBlock {
        id: 7,
        args: vec![],
        insts: vec![],
        term: evocc_mir::MTerm::Ret { values: vec![] },
    };
    assert_eq!(
        format!("{block:?}"),
        "MBlock { id: 7, args: [], insts: [], term: Ret { values: [] } }"
    );
}

#[test]
fn target_adapter_preserves_assembly_edge_order_and_forwarding_exclusions() {
    use evocc_mir::{AsmBody, MTerm, RegClass, VReg};
    let value = VReg::new(0, RegClass::Gpr64);
    let original = MTerm::AsmGoto {
        body: AsmBody {
            template: String::new(),
            volatile: true,
            outputs: vec![],
            inputs: vec![],
            clobbers: vec![],
        },
        labels: vec![(10, vec![value]), (11, vec![value])],
        fallthrough: 12,
        fallthrough_args: vec![value],
    };
    assert!(SsaTerm::prevents_forwarding(&original));
    for position in 0..3 {
        let mut changed = original.clone();
        SsaTerm::retarget(&mut changed, position, 99);
        for (index, (target, args)) in MachTerm::edges(&changed).into_iter().enumerate() {
            assert_eq!(
                target,
                if index == position {
                    99
                } else {
                    10 + index as u32
                }
            );
            assert_eq!(args.len(), usize::from(index != position));
        }
    }
    let indirect = MTerm::Indirect {
        target: value,
        edges: vec![(10, vec![value])],
    };
    assert!(SsaTerm::prevents_forwarding(&indirect));
    assert!(std::panic::catch_unwind(|| {
        let mut changed = indirect;
        SsaTerm::retarget(&mut changed, 0, 99);
    })
    .is_err());
}

#[test]
fn the_same_algorithm_forwards_without_a_target_algorithm_callback() {
    let before = straight();
    let after = out_of_ssa_with_dce(&before, true, true);
    assert_eq!(after.blocks[0].args, before.blocks[0].args);
    assert!(after.blocks[1].args.is_empty());
    assert_eq!(after.blocks[1].term, Term::Return(vec![r(2)]));
    assert!(copies(&after).is_empty());
    assert!(after
        .blocks
        .iter()
        .flat_map(|b| b.term.edges())
        .all(|(_, args)| args.is_empty()));
    assert_eq!(out_of_ssa_with_dce(&before, true, true), after);
}

#[test]
fn invalid_shape_admission_and_fixed_operands_keep_transport_copies() {
    let before = straight();
    let after = out_of_ssa_with_dce(&before, true, false);
    assert_eq!(copies(&after), vec![(r(3), r(2))]);
    let mut fixed = before;
    fixed.blocks[0].insts.insert(0, Inst::Fixed(r(0)));
    assert_eq!(
        copies(&out_of_ssa_with_dce(&fixed, true, true)),
        vec![(r(3), r(2))]
    );
}

#[test]
fn critical_copy_cycles_have_the_same_order_and_do_not_run_on_the_exit_edge() {
    let wide = |id| Reg::new(id, Class::Wide);
    let before = Function {
        name: "swap".into(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![wide(0), wide(1)],
                insts: vec![],
                term: Term::Jump(1, vec![wide(0), wide(1)]),
            },
            Block {
                id: 1,
                args: vec![wide(2), wide(3)],
                insts: vec![],
                term: Term::Choose(wide(2), [(1, vec![wide(3), wide(2)]), (2, vec![wide(2)])]),
            },
            Block {
                id: 2,
                args: vec![wide(4)],
                insts: vec![],
                term: Term::Return(vec![wide(4)]),
            },
        ],
    };
    let after = out_of_ssa_with_dce(&before, true, true);
    assert_eq!(after.blocks.len(), 4);
    assert_eq!(after.blocks[3].id, 3);
    assert_eq!(after.blocks[3].term, Term::Jump(1, vec![]));
    assert_eq!(
        after.blocks[1]
            .term
            .edges()
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert!(after.blocks[2].insts.is_empty());
    let cycle = &after.blocks[3].insts;
    assert_eq!(cycle.len(), 3);
    let Inst::Copy(scratch, source) = cycle[0] else {
        panic!("cycle must save a source");
    };
    assert_eq!(source, wide(3));
    assert_eq!(scratch.class, Class::Wide);
    let mut values = BTreeMap::from([(2, 101), (3, 202)]);
    for inst in cycle {
        let Inst::Copy(dst, src) = inst else {
            panic!("only transport moves");
        };
        values.insert(dst.id, values[&src.id]);
    }
    assert_eq!((values[&2], values[&3]), (202, 101));
    assert_eq!(copies(&after).len(), 5);
}

#[test]
fn computed_self_edges_capture_the_target_before_the_union_copy() {
    let before = Function {
        name: "dispatch".into(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![r(0), r(1)],
                insts: vec![],
                term: Term::Jump(1, vec![r(0), r(1)]),
            },
            Block {
                id: 1,
                args: vec![r(2), r(3)],
                insts: vec![],
                term: Term::Dispatch(r(2), vec![(1, vec![r(3), r(2)]), (2, vec![r(2), r(3)])]),
            },
            Block {
                id: 2,
                args: vec![r(4), r(5)],
                insts: vec![],
                term: Term::Return(vec![r(4), r(5)]),
            },
        ],
    };
    let after = out_of_ssa_with_dce(&before, true, true);
    assert_eq!(after.blocks.len(), 3);
    let dispatch = &after.blocks[1];
    let mut values = BTreeMap::from([(2, 101), (3, 202)]);
    for inst in &dispatch.insts {
        let Inst::Copy(dst, src) = inst else {
            panic!("only transport moves");
        };
        values.insert(dst.id, values[&src.id]);
    }
    let Term::Dispatch(target, edges) = &dispatch.term else {
        panic!("dispatch must survive");
    };
    assert_eq!(values[&target.id], 101);
    assert_eq!(
        (values[&2], values[&3], values[&4], values[&5]),
        (202, 101, 101, 202)
    );
    assert_eq!(
        edges.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert!(edges.iter().all(|(_, args)| args.is_empty()));
}

#[test]
fn baseline_dce_keeps_memory_effects_and_its_attribution_control() {
    let before = Function {
        name: "effects".into(),
        blocks: vec![Block {
            id: 0,
            args: vec![r(0), r(1)],
            insts: vec![
                Inst::Constant(r(2), 7),
                Inst::Binary(r(3), r(1), r(2)),
                Inst::Load(r(4), r(0)),
                Inst::Store(r(0), r(1)),
            ],
            term: Term::Return(vec![r(1)]),
        }],
    };
    let on = out_of_ssa_with_dce(&before, true, true);
    let off = out_of_ssa_with_dce(&before, false, true);
    assert_eq!(
        on.blocks[0].insts,
        vec![Inst::Load(r(4), r(0)), Inst::Store(r(0), r(1))]
    );
    assert_eq!(off.blocks[0].insts, before.blocks[0].insts);
}

#[test]
fn demand_keeps_the_flags_bundle_without_hardcoded_register_classes() {
    for live in [false, true] {
        let before = Function {
            name: "flags_bundle".into(),
            blocks: vec![Block {
                id: 0,
                args: vec![r(0), r(1)],
                insts: vec![
                    Inst::Flagged(r(2), flags(3), r(0), r(1)),
                    Inst::CaptureCondition(r(4), flags(3)),
                ],
                term: Term::Return(vec![if live { r(2) } else { r(0) }]),
            }],
        };
        let after = out_of_ssa_with_dce(&before, true, true);
        assert_eq!(after.blocks[0].insts.len(), if live { 2 } else { 0 });
    }
}
