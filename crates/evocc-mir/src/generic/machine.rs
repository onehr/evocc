//! Ordered machine operand facts, independent of instruction and register types.
//!
//! Semantic reads and mutable occurrences are distinct: a read-write operand
//! can read and define a value while still occupying only one mutable slot.

use super::register::VirtualReg;

/// Blocks retain source order, explicit arguments and one target terminator.
#[derive(Clone, PartialEq, Eq)]
pub struct MachineBlock<I, T, C> {
    pub id: u32,
    pub args: Vec<VirtualReg<C>>,
    pub insts: Vec<I>,
    pub term: T,
}

#[derive(Clone, PartialEq, Eq)]
pub struct MachineFunction<I, T, C> {
    pub name: String,
    pub blocks: Vec<MachineBlock<I, T, C>>,
}

impl<I: std::fmt::Debug, T: std::fmt::Debug, C: std::fmt::Debug> std::fmt::Debug
    for MachineBlock<I, T, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MBlock")
            .field("id", &self.id)
            .field("args", &self.args)
            .field("insts", &self.insts)
            .field("term", &self.term)
            .finish()
    }
}

impl<I: std::fmt::Debug, T: std::fmt::Debug, C: std::fmt::Debug> std::fmt::Debug
    for MachineFunction<I, T, C>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MFunction")
            .field("name", &self.name)
            .field("blocks", &self.blocks)
            .finish()
    }
}

pub trait MachInst {
    type Reg: Copy;

    fn uses(&self) -> Vec<Self::Reg>;
    fn defs(&self) -> Vec<Self::Reg>;
    fn operand_slots(&mut self, definitions: bool) -> Vec<&mut Self::Reg>;
    fn writes_flags(&self) -> bool;
    fn is_call(&self) -> bool;
    fn discardable(&self) -> bool;
}

pub trait MachTerm {
    type Reg: Copy;

    fn uses(&self) -> Vec<Self::Reg>;
    /// Existing post-copy coalescing occurrences, not every semantic edge use.
    fn operand_slots(&mut self) -> Vec<&mut Self::Reg>;
    fn edges(&self) -> Vec<(u32, &[Self::Reg])>;
    fn is_exit(&self) -> bool;
}
