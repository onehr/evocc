//! Ordered machine operand facts, independent of instruction and register types.
//!
//! Semantic reads and mutable occurrences are distinct: a read-write operand
//! can read and define a value while still occupying only one mutable slot.

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
