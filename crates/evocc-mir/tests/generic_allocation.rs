//! N03a step 2c: the actual allocator and independent verifier on a small ISA.
//! No target interpreter, allocation callback or native toolchain is used here.

use std::collections::BTreeMap;

use evocc_mir::generic::allocation::{
    allocate, AllocClass, AllocInst, AllocTerm, Allocated, AllocationTarget, ConstraintBody,
    ConstraintOperand, ConstraintValue, SpillKind, SpillView,
};
use evocc_mir::generic::allocverify::verify_allocation;
use evocc_mir::generic::machine::{MachInst, MachTerm, MachineBlock, MachineFunction};
use evocc_mir::generic::register::{RegisterBank, RegisterClass, VirtualReg};
use evocc_mir::generic::ssa::{SsaInst, SsaTerm};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Word,
    Pair,
    Condition,
}

impl RegisterClass for Class {
    fn is_flags(self) -> bool {
        self == Self::Condition
    }
}

impl AllocClass for Class {
    fn bank(self) -> u8 {
        u8::from(self == Self::Pair)
    }
    fn spill_words(self) -> u32 {
        if self == Self::Pair {
            2
        } else {
            1
        }
    }
}

type Reg = VirtualReg<Class>;
type Function = MachineFunction<Inst, Term, Class>;
type Block = MachineBlock<Inst, Term, Class>;

fn r(id: u32) -> Reg {
    Reg::new(id, Class::Word)
}
fn q(id: u32) -> Reg {
    Reg::new(id, Class::Pair)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Operand {
    reg: Reg,
    fixed: Option<u8>,
    tied: Option<u8>,
    early: bool,
    read_write: bool,
    memory: bool,
}

fn operand(reg: Reg) -> Operand {
    Operand {
        reg,
        fixed: None,
        tied: None,
        early: false,
        read_write: false,
        memory: false,
    }
}

impl ConstraintOperand for Operand {
    type Reg = Reg;
    fn value(&self) -> ConstraintValue<Reg> {
        ConstraintValue::Reg(self.reg)
    }
    fn set_register(&mut self, reg: Reg) {
        self.reg = reg;
    }
    fn fixed_register(&self) -> Option<u8> {
        self.fixed
    }
    fn set_fixed_register(&mut self, number: Option<u8>) {
        self.fixed = number;
    }
    fn tied_output(&self) -> Option<u8> {
        self.tied
    }
    fn early_clobber(&self) -> bool {
        self.early
    }
    fn read_write(&self) -> bool {
        self.read_write
    }
    fn memory(&self) -> bool {
        self.memory
    }
    fn needs_register(&self) -> bool {
        true
    }
    fn bank(&self) -> u8 {
        self.reg.class.bank()
    }
    fn accepts_zero(&self) -> bool {
        false
    }
    fn accepts_symbol(&self) -> bool {
        false
    }
    fn spelling(&self) -> String {
        "slot".into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Body {
    outputs: Vec<Operand>,
    inputs: Vec<Operand>,
    clobbers: Vec<u8>,
}

impl ConstraintBody for Body {
    type Reg = Reg;
    type Operand = Operand;
    fn outputs(&self) -> &[Operand] {
        &self.outputs
    }
    fn inputs(&self) -> &[Operand] {
        &self.inputs
    }
    fn outputs_mut(&mut self) -> &mut [Operand] {
        &mut self.outputs
    }
    fn inputs_mut(&mut self) -> &mut [Operand] {
        &mut self.inputs
    }
    fn defined(&self) -> Vec<Reg> {
        self.outputs
            .iter()
            .filter(|o| !o.memory)
            .map(|o| o.reg)
            .collect()
    }
    fn clobbered_registers(&self) -> Vec<u8> {
        self.clobbers.clone()
    }
}

impl Body {
    fn uses(&self) -> Vec<Reg> {
        self.inputs
            .iter()
            .chain(self.outputs.iter().filter(|o| o.memory || o.read_write))
            .map(|o| o.reg)
            .collect()
    }
    fn slots(&mut self, definitions: bool) -> Vec<&mut Reg> {
        if definitions {
            self.outputs
                .iter_mut()
                .filter(|o| !o.memory)
                .map(|o| &mut o.reg)
                .collect()
        } else {
            self.outputs
                .iter_mut()
                .filter(|o| o.memory)
                .chain(self.inputs.iter_mut())
                .map(|o| &mut o.reg)
                .collect()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Inst {
    Value(Reg, u64),
    Copy(Reg, Reg),
    Add(Reg, Reg, Reg),
    Early(Reg, Reg, Reg),
    Call {
        args: Vec<Reg>,
        results: Vec<Reg>,
        opaque: u64,
    },
    Constraint(Body),
    Save(Reg, u32),
    Restore(Reg, u32),
    Incoming(Reg, u32),
    Outgoing(Reg, u32),
    IndirectResult,
}

impl MachInst for Inst {
    type Reg = Reg;
    fn uses(&self) -> Vec<Reg> {
        match self {
            Self::Copy(_, src) | Self::Save(src, _) | Self::Outgoing(src, _) => vec![*src],
            Self::Add(_, lhs, rhs) | Self::Early(_, lhs, rhs) => vec![*lhs, *rhs],
            Self::Call { args, .. } => args.clone(),
            Self::Constraint(body) => body.uses(),
            _ => vec![],
        }
    }
    fn defs(&self) -> Vec<Reg> {
        match self {
            Self::Value(dst, _)
            | Self::Copy(dst, _)
            | Self::Add(dst, ..)
            | Self::Early(dst, ..)
            | Self::Restore(dst, _)
            | Self::Incoming(dst, _) => vec![*dst],
            Self::Call { results, .. } => results.clone(),
            Self::Constraint(body) => body.defined(),
            _ => vec![],
        }
    }
    fn operand_slots(&mut self, definitions: bool) -> Vec<&mut Reg> {
        if let Self::Constraint(body) = self {
            return body.slots(definitions);
        }
        if definitions {
            match self {
                Self::Value(dst, _)
                | Self::Copy(dst, _)
                | Self::Add(dst, ..)
                | Self::Early(dst, ..)
                | Self::Restore(dst, _)
                | Self::Incoming(dst, _) => vec![dst],
                Self::Call { results, .. } => results.iter_mut().collect(),
                _ => vec![],
            }
        } else {
            match self {
                Self::Copy(_, src) | Self::Save(src, _) | Self::Outgoing(src, _) => vec![src],
                Self::Add(_, lhs, rhs) | Self::Early(_, lhs, rhs) => vec![lhs, rhs],
                Self::Call { args, .. } => args.iter_mut().collect(),
                _ => vec![],
            }
        }
    }
    fn writes_flags(&self) -> bool {
        false
    }
    fn is_call(&self) -> bool {
        matches!(self, Self::Call { .. })
    }
    fn discardable(&self) -> bool {
        matches!(self, Self::Value(..) | Self::Copy(..) | Self::Add(..))
    }
}

impl SsaInst for Inst {
    fn copy(dst: Reg, src: Reg) -> Self {
        Self::Copy(dst, src)
    }
    fn has_fixed_binding(&self) -> bool {
        matches!(self, Self::Constraint(body) if body.outputs.iter().chain(&body.inputs).any(|o| o.fixed.is_some()))
    }
}

impl AllocInst for Inst {
    type Body = Body;
    fn copy_operands(&self) -> Option<(Reg, Reg)> {
        if let Self::Copy(dst, src) = self {
            Some((*dst, *src))
        } else {
            None
        }
    }
    fn call_operands(&self) -> Option<(&[Reg], &[Reg])> {
        if let Self::Call { args, results, .. } = self {
            Some((args, results))
        } else {
            None
        }
    }
    fn call_operands_mut(&mut self) -> Option<(&mut Vec<Reg>, &mut Vec<Reg>)> {
        if let Self::Call { args, results, .. } = self {
            Some((args, results))
        } else {
            None
        }
    }
    fn assembly(&self) -> Option<&Body> {
        if let Self::Constraint(body) = self {
            Some(body)
        } else {
            None
        }
    }
    fn assembly_mut(&mut self) -> Option<&mut Body> {
        if let Self::Constraint(body) = self {
            Some(body)
        } else {
            None
        }
    }
    fn is_indirect_result(&self) -> bool {
        matches!(self, Self::IndirectResult)
    }
    fn exclusive_result(&self) -> Option<Reg> {
        if let Self::Early(dst, ..) = self {
            Some(*dst)
        } else {
            None
        }
    }
    fn spill(&self) -> Option<SpillView<Reg>> {
        match self {
            Self::Save(reg, slot) => Some(SpillView {
                kind: SpillKind::Store,
                reg: *reg,
                slot: *slot,
            }),
            Self::Restore(reg, slot) => Some(SpillView {
                kind: SpillKind::Reload,
                reg: *reg,
                slot: *slot,
            }),
            _ => None,
        }
    }
    fn spill_slot_mut(&mut self) -> Option<&mut u32> {
        match self {
            Self::Save(_, slot) | Self::Restore(_, slot) => Some(slot),
            _ => None,
        }
    }
    fn spill_store(src: Reg, slot: u32) -> Self {
        Self::Save(src, slot)
    }
    fn spill_reload(dst: Reg, slot: u32) -> Self {
        Self::Restore(dst, slot)
    }
    fn incoming_arg(dst: Reg, slot: u32) -> Self {
        Self::Incoming(dst, slot)
    }
    fn outgoing_arg(src: Reg, slot: u32) -> Self {
        Self::Outgoing(src, slot)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Term {
    Jump(u32, Vec<Reg>),
    Return(Vec<Reg>),
    Constraint {
        body: Body,
        edges: Vec<(u32, Vec<Reg>)>,
    },
}

impl MachTerm for Term {
    type Reg = Reg;
    fn uses(&self) -> Vec<Reg> {
        match self {
            Self::Jump(_, args) | Self::Return(args) => args.clone(),
            Self::Constraint { body, edges } => body
                .uses()
                .into_iter()
                .chain(edges.iter().flat_map(|(_, args)| args.iter().copied()))
                .collect(),
        }
    }
    fn operand_slots(&mut self) -> Vec<&mut Reg> {
        match self {
            Self::Jump(..) => vec![],
            Self::Return(args) => args.iter_mut().collect(),
            Self::Constraint { body, .. } => body
                .outputs
                .iter_mut()
                .chain(&mut body.inputs)
                .map(|o| &mut o.reg)
                .collect(),
        }
    }
    fn edges(&self) -> Vec<(u32, &[Reg])> {
        match self {
            Self::Jump(target, args) => vec![(*target, args)],
            Self::Return(_) => vec![],
            Self::Constraint { edges, .. } => edges
                .iter()
                .map(|(id, args)| (*id, args.as_slice()))
                .collect(),
        }
    }
    fn is_exit(&self) -> bool {
        matches!(self, Self::Return(_))
    }
}

impl SsaTerm for Term {
    fn indirect_target(&self) -> Option<Reg> {
        None
    }
    fn set_indirect_target(&mut self, _: Reg) {
        panic!("no computed terminator in this fixture")
    }
    fn clear_edge_args(&mut self) {
        match self {
            Self::Jump(_, args) => args.clear(),
            Self::Constraint { edges, .. } => edges.iter_mut().for_each(|(_, args)| args.clear()),
            Self::Return(_) => {}
        }
    }
    fn retarget(&mut self, index: usize, target: u32) {
        match self {
            Self::Jump(old, args) if index == 0 => {
                *old = target;
                args.clear();
            }
            Self::Constraint { edges, .. } => {
                if let Some((old, args)) = edges.get_mut(index) {
                    *old = target;
                    args.clear();
                }
            }
            _ => {}
        }
    }
    fn jump(target: u32, args: Vec<Reg>) -> Self {
        Self::Jump(target, args)
    }
    fn prevents_forwarding(&self) -> bool {
        matches!(self, Self::Constraint { .. })
    }
}

impl AllocTerm for Term {
    type Body = Body;
    fn return_values(&self) -> Option<&[Reg]> {
        if let Self::Return(values) = self {
            Some(values)
        } else {
            None
        }
    }
    fn return_values_mut(&mut self) -> Option<&mut Vec<Reg>> {
        if let Self::Return(values) = self {
            Some(values)
        } else {
            None
        }
    }
    fn assembly(&self) -> Option<&Body> {
        if let Self::Constraint { body, .. } = self {
            Some(body)
        } else {
            None
        }
    }
    fn assembly_mut(&mut self) -> Option<&mut Body> {
        if let Self::Constraint { body, .. } = self {
            Some(body)
        } else {
            None
        }
    }
    fn defs(&self) -> Vec<Reg> {
        self.assembly()
            .map_or_else(Vec::new, ConstraintBody::defined)
    }
    fn allows_edge_spill_bridge(&self) -> bool {
        matches!(self, Self::Constraint { .. })
    }
}

struct TinyTarget;

impl AllocationTarget<Class> for TinyTarget {
    fn bank(&self, class: Class) -> RegisterBank {
        RegisterBank {
            first: if class == Class::Pair { 16 } else { 0 },
            count: 8,
        }
    }
    fn candidates(&self, class: Class, across_call: bool) -> &'static [u8] {
        match (class, across_call) {
            (Class::Pair, false) => &[16, 17],
            (Class::Pair, true) => &[],
            (_, false) => &[0, 1, 2],
            (_, true) => &[2],
        }
    }
    fn callee_saved(&self) -> &'static [u8] {
        &[2]
    }
    fn call_clobbers(&self) -> &'static [u8] {
        &[0, 1, 6, 7, 16, 17, 18, 19, 20, 21, 22, 23]
    }
    fn cycle_scratch(&self, class: Class) -> u8 {
        if class == Class::Pair {
            22
        } else {
            6
        }
    }
    fn flags_assignment_tag(&self) -> u8 {
        0
    }
    fn fixed_assembly_registers(&self) -> &'static [u8] {
        &[0, 1, 2, 3, 4, 5, 6, 7, 16, 17, 18, 19, 20, 21, 22, 23]
    }
    fn clobber_classes(&self) -> &'static [Class] {
        &[Class::Word]
    }
    fn argument_bank(&self, class: Class) -> RegisterBank {
        RegisterBank {
            first: if class == Class::Pair { 16 } else { 0 },
            count: 2,
        }
    }
    fn argument_banks(&self) -> &'static [RegisterBank] {
        &[
            RegisterBank { first: 0, count: 2 },
            RegisterBank {
                first: 16,
                count: 2,
            },
        ]
    }
    fn indirect_result_register(&self) -> u8 {
        7
    }
    fn result_overflow_register(&self) -> u8 {
        2
    }
    fn stack_slot_bytes(&self) -> u64 {
        8
    }
    fn stack_alignment_words(&self) -> u32 {
        2
    }
    fn fixed_binding_spelling(&self, number: u8) -> String {
        format!("p{number}")
    }
}

fn single(args: Vec<Reg>, insts: Vec<Inst>, values: Vec<Reg>) -> Function {
    Function {
        name: "synthetic".into(),
        blocks: vec![Block {
            id: 0,
            args,
            insts,
            term: Term::Return(values),
        }],
    }
}

/// A bounded test-local trace over physical registers and spill words. This
/// checks actual transported values, separately from the verifier's dataflow;
/// it is not another compilation target or a replacement project interpreter.
fn straight_result(
    allocated: &Allocated<Inst, Term, Class>,
    mut registers: BTreeMap<u8, u128>,
) -> Vec<u128> {
    assert_eq!(allocated.blocks.len(), 1);
    let number = |reg: Reg| allocated.assignment[&reg.id];
    let mut slots = BTreeMap::<u32, u64>::new();
    for inst in &allocated.blocks[0].insts {
        match inst {
            Inst::Value(dst, value) => {
                registers.insert(number(*dst), u128::from(*value));
            }
            Inst::Copy(dst, src) => {
                let value = registers[&number(*src)];
                registers.insert(number(*dst), value);
            }
            Inst::Add(dst, lhs, rhs) => {
                let value = registers[&number(*lhs)].wrapping_add(registers[&number(*rhs)]);
                let value = if dst.class == Class::Pair {
                    value
                } else {
                    u128::from(value as u64)
                };
                registers.insert(number(*dst), value);
            }
            Inst::Save(src, slot) => {
                let value = registers[&number(*src)];
                for word in 0..src.class.spill_words() {
                    slots.insert(slot + word, (value >> (word * 64)) as u64);
                }
            }
            Inst::Restore(dst, slot) => {
                let value = (0..dst.class.spill_words()).fold(0_u128, |value, word| {
                    value | (u128::from(slots[&(slot + word)]) << (word * 64))
                });
                registers.insert(number(*dst), value);
            }
            other => panic!("outside this straight-line transport fixture: {other:?}"),
        }
    }
    let Term::Return(values) = &allocated.blocks[0].term else {
        panic!("straight-line fixture must return");
    };
    values.iter().map(|reg| registers[&number(*reg)]).collect()
}

#[test]
fn ordered_pools_and_boundary_copies_are_the_actual_shared_algorithm() {
    let source = single(
        vec![r(0)],
        vec![Inst::Value(r(1), 3), Inst::Add(r(2), r(0), r(1))],
        vec![r(2)],
    );
    let first = allocate(&TinyTarget, &source).unwrap();
    let second = allocate(&TinyTarget, &source).unwrap();
    assert_eq!(first, second);
    verify_allocation(&TinyTarget, &first).unwrap();
    assert_eq!(first.spilled, 0);
    assert_eq!(first.frame_slots, 0);
    let incoming = first.blocks[0]
        .insts
        .iter()
        .find_map(|inst| {
            if let Inst::Add(_, lhs, _) = inst {
                Some(*lhs)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(first.assignment[&incoming.id], 0);
    assert_eq!(first.assignment[&1], 1);
    assert_eq!(first.parameters, 1);
    assert!(first.blocks.iter().all(|block| block.args.is_empty()));
    assert_eq!(straight_result(&first, BTreeMap::from([(0, 5)])), vec![8]);
}

#[test]
fn indirect_result_uses_the_target_boundary_register_not_an_argument_slot() {
    let source = single(
        vec![r(0), r(1)],
        vec![Inst::IndirectResult, Inst::Add(r(2), r(0), r(1))],
        vec![r(2)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    assert!(allocated.indirect_result);
    assert_eq!(allocated.parameters, 2);
    assert!(allocated.assignment.values().any(|number| *number == 7));

    let mut corrupted = allocated.clone();
    for number in corrupted.assignment.values_mut() {
        if *number == 7 {
            *number = 6;
        }
    }
    assert!(verify_allocation(&TinyTarget, &corrupted).is_err());
}

#[test]
fn pressure_spills_and_a_removed_reload_is_independently_rejected() {
    let mut insts: Vec<_> = (0..10)
        .map(|id| Inst::Value(r(id), u64::from(id)))
        .collect();
    insts.push(Inst::Add(r(20), r(0), r(1)));
    for id in 2..10 {
        insts.push(Inst::Add(r(19 + id), r(18 + id), r(id)));
    }
    let source = single(vec![], insts, vec![r(28)]);
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    assert!(allocated.spilled > 0);
    assert!(allocated.frame_slots > 0);
    assert!(allocated
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, Inst::Save(..))));
    assert!(allocated
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, Inst::Restore(..))));
    assert_eq!(straight_result(&allocated, BTreeMap::new()), vec![45]);
    let mut missing = allocated.clone();
    for block in &mut missing.blocks {
        block.insts.retain(|i| !matches!(i, Inst::Restore(..)));
    }
    assert!(verify_allocation(&TinyTarget, &missing).is_err());
}

#[test]
fn call_keeps_opaque_payload_and_full_width_values_spill_across_it() {
    let source = single(
        vec![],
        vec![
            Inst::Value(q(0), 9),
            Inst::Call {
                args: vec![],
                results: vec![r(1)],
                opaque: 0xfeed_cafe,
            },
            Inst::Add(q(2), q(0), q(0)),
        ],
        vec![q(2)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    assert!(allocated.spilled > 0);
    assert!(allocated.frame_slots >= 2);
    let insts = &allocated.blocks[0].insts;
    let call = insts
        .iter()
        .position(|i| matches!(i, Inst::Call { .. }))
        .unwrap();
    assert!(matches!(
        insts[call],
        Inst::Call {
            opaque: 0xfeed_cafe,
            ..
        }
    ));
    assert!(insts[..call]
        .iter()
        .any(|i| matches!(i, Inst::Save(reg, slot) if reg.class == Class::Pair && slot % 2 == 0)));
    assert!(insts[call + 1..].iter().any(
        |i| matches!(i, Inst::Restore(reg, slot) if reg.class == Class::Pair && slot % 2 == 0)
    ));
}

#[test]
fn a_live_scalar_uses_the_callee_pool_and_call_clobber_forgery_is_rejected() {
    let source = single(
        vec![],
        vec![
            Inst::Value(r(0), 9),
            Inst::Call {
                args: vec![],
                results: vec![],
                opaque: 5,
            },
            Inst::Add(r(1), r(0), r(0)),
        ],
        vec![r(1)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    assert_eq!(allocated.assignment[&0], 2);
    assert_eq!(allocated.used_callee_saved, vec![2]);
    let mut forged = allocated.clone();
    forged.assignment.insert(0, 0);
    assert!(verify_allocation(&TinyTarget, &forged).is_err());
}

#[test]
fn repeated_fixed_inputs_split_occurrences_not_the_source_value() {
    let mut left = operand(r(0));
    left.fixed = Some(3);
    let mut right = operand(r(0));
    right.fixed = Some(4);
    let source = single(
        vec![r(0)],
        vec![Inst::Constraint(Body {
            inputs: vec![left, right],
            ..Body::default()
        })],
        vec![r(0)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    let body = allocated.blocks[0]
        .insts
        .iter()
        .find_map(AllocInst::assembly)
        .unwrap();
    assert_ne!(body.inputs[0].reg.id, body.inputs[1].reg.id);
    assert_eq!(allocated.assignment[&body.inputs[0].reg.id], 3);
    assert_eq!(allocated.assignment[&body.inputs[1].reg.id], 4);
}

#[test]
fn tied_early_constraints_and_out_of_range_tie_are_checked_independently() {
    let mut output = operand(r(2));
    output.early = true;
    let mut input = operand(r(0));
    input.tied = Some(0);
    let source = single(
        vec![r(0), r(1)],
        vec![Inst::Constraint(Body {
            outputs: vec![output],
            inputs: vec![input, operand(r(1))],
            clobbers: vec![],
        })],
        vec![r(2)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    let body = allocated.blocks[0]
        .insts
        .iter()
        .find_map(AllocInst::assembly)
        .unwrap();
    let result = body.outputs[0].reg;
    let tied = body.inputs[0].reg;
    let other = body.inputs[1].reg;
    assert_eq!(
        allocated.assignment[&result.id],
        allocated.assignment[&tied.id]
    );
    assert_ne!(
        allocated.assignment[&result.id],
        allocated.assignment[&other.id]
    );
    let mut forged = allocated.clone();
    let body = forged.blocks[0]
        .insts
        .iter_mut()
        .find_map(AllocInst::assembly_mut)
        .unwrap();
    body.inputs[0].tied = Some(9);
    assert!(verify_allocation(&TinyTarget, &forged).is_err());
    let mut forged = allocated.clone();
    forged
        .assignment
        .insert(result.id, allocated.assignment[&other.id]);
    assert!(verify_allocation(&TinyTarget, &forged).is_err());
}

#[test]
fn verifier_checks_bank_membership_not_allocator_pool_membership() {
    let source = single(
        vec![],
        vec![
            Inst::Value(r(0), 3),
            Inst::Constraint(Body {
                inputs: vec![operand(r(0))],
                ..Body::default()
            }),
        ],
        vec![],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    let mut in_bank = allocated.clone();
    in_bank.assignment.insert(0, 5); // Admitted physical bank, not an allocation candidate.
    verify_allocation(&TinyTarget, &in_bank).unwrap();
    in_bank.assignment.insert(0, 16); // Other bank, even though it is a valid candidate there.
    assert!(verify_allocation(&TinyTarget, &in_bank).is_err());
}

#[test]
fn exclusive_result_cannot_overlap_any_input() {
    let source = single(
        vec![r(0), r(1)],
        vec![Inst::Early(r(2), r(0), r(1))],
        vec![r(2)],
    );
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    let (dst, src) = allocated.blocks[0]
        .insts
        .iter()
        .find_map(|i| {
            if let Inst::Early(dst, src, _) = i {
                Some((*dst, *src))
            } else {
                None
            }
        })
        .unwrap();
    assert_ne!(allocated.assignment[&dst.id], allocated.assignment[&src.id]);
    let mut forged = allocated.clone();
    forged
        .assignment
        .insert(dst.id, allocated.assignment[&src.id]);
    assert!(verify_allocation(&TinyTarget, &forged).is_err());
}

#[test]
fn constraint_output_copies_live_on_selected_edges_not_in_shared_successors() {
    let mut output = operand(r(1));
    output.fixed = Some(3);
    let source = Function {
        name: "selected_edges".into(),
        blocks: vec![
            Block {
                id: 0,
                args: vec![r(0)],
                insts: vec![],
                term: Term::Constraint {
                    body: Body {
                        outputs: vec![output],
                        inputs: vec![operand(r(0))],
                        clobbers: vec![],
                    },
                    edges: vec![(1, vec![]), (2, vec![])],
                },
            },
            Block {
                id: 1,
                args: vec![],
                insts: vec![],
                term: Term::Return(vec![r(1)]),
            },
            Block {
                id: 2,
                args: vec![],
                insts: vec![],
                term: Term::Return(vec![r(1)]),
            },
        ],
    };
    let allocated = allocate(&TinyTarget, &source).unwrap();
    verify_allocation(&TinyTarget, &allocated).unwrap();
    let selected = allocated.blocks[0].term.edges();
    assert_eq!(selected.len(), 2);
    assert_ne!(selected[0].0, selected[1].0);
    for (index, (bridge, args)) in selected.iter().enumerate() {
        assert!(*bridge > 2);
        assert!(args.is_empty());
        let block = allocated.blocks.iter().find(|b| b.id == *bridge).unwrap();
        assert!(block.insts.iter().any(|i| matches!(i, Inst::Copy(..))));
        assert_eq!(block.term.edges()[0].0, index as u32 + 1);
    }
    // The fixed output copy has no execution site before the selected branch,
    // and neither original shared successor receives that boundary copy.
    for id in [0, 1, 2] {
        let block = allocated.blocks.iter().find(|b| b.id == id).unwrap();
        assert!(!block.insts.iter().any(
            |i| matches!(i, Inst::Copy(_, src) if allocated.assignment.get(&src.id) == Some(&3))
        ));
    }
}
