//! AArch64 facts and borrowed constraint views for the shared allocator.

use super::regfile;
use crate::generic::abi::AbiFacts;
use crate::generic::allocation::{
    AllocClass, AllocationTarget, ConstraintBody, ConstraintOperand, ConstraintValue, SecondReturn,
};
use crate::generic::register::RegisterBank;
use crate::{
    AsmBody, AsmConstraint, AsmDirection, AsmImmediate, AsmOperand, AsmValue, MInst, RegClass,
    Target, VReg,
};

/// The register pools and constraint views of AArch64 under one OS ABI.
pub struct Aarch64Allocation {
    abi: &'static AbiFacts,
}

impl Aarch64Allocation {
    #[must_use]
    pub const fn for_target(target: Target) -> Self {
        Self {
            abi: target.abi_facts(),
        }
    }
}

/// EvoMIR Section 4.5: only `bl.returns_twice` resumes a second time.
impl SecondReturn for MInst {
    fn returns_twice(&self) -> bool {
        matches!(
            self,
            Self::Bl {
                returns_twice: true,
                ..
            }
        )
    }
}

impl AllocClass for RegClass {
    fn value_bits(self) -> u16 {
        match self {
            Self::Gpr32 | Self::Simd32 => 32,
            Self::Gpr64 | Self::Simd64 => 64,
            Self::Simd128 => 128,
            Self::Flags => 4,
        }
    }
    fn bank(self) -> u8 {
        u8::from(self.is_simd())
    }
    fn spill_words(self) -> u32 {
        if self == Self::Simd128 {
            2
        } else {
            1
        }
    }
}

impl AllocationTarget<RegClass> for Aarch64Allocation {
    fn bank(&self, class: RegClass) -> RegisterBank {
        if class.is_simd() {
            regfile::SIMD_BANK
        } else {
            regfile::GPR_BANK
        }
    }
    fn candidates(&self, class: RegClass, across_call: bool) -> &'static [u8] {
        if class.is_simd() {
            if across_call {
                &regfile::SIMD_ACROSS_CALL
            } else {
                &regfile::SIMD_CALLER_SAVED
            }
        } else if across_call {
            &regfile::CALLEE_SAVED
        } else {
            &regfile::GPR_ALLOCATABLE
        }
    }
    fn abi(&self) -> &AbiFacts {
        self.abi
    }
    fn cycle_scratch(&self, class: RegClass) -> u8 {
        if class.is_simd() {
            regfile::SIMD_SCRATCH
        } else {
            regfile::SCRATCH
        }
    }
    fn flags_assignment_tag(&self) -> u8 {
        regfile::FLAGS_ASSIGNMENT_TAG
    }
    fn fixed_assembly_registers(&self) -> &'static [u8] {
        &regfile::FIXED_ASM
    }
    fn clobber_classes(&self) -> &'static [RegClass] {
        &[RegClass::Gpr32, RegClass::Gpr64]
    }
    fn fixed_binding_spelling(&self, number: u8) -> String {
        format!("x{number}")
    }
}

impl ConstraintOperand for AsmOperand {
    type Reg = VReg;
    fn value(&self) -> ConstraintValue<VReg> {
        match self.value {
            AsmValue::Reg(reg) => ConstraintValue::Reg(reg),
            AsmValue::Zero(_) => ConstraintValue::Zero,
            AsmValue::Imm(_) => ConstraintValue::Immediate,
            AsmValue::Symbol { .. } => ConstraintValue::Symbol,
        }
    }
    fn set_register(&mut self, reg: VReg) {
        self.value = AsmValue::Reg(reg);
    }
    fn fixed_register(&self) -> Option<u8> {
        self.fixed_register
    }
    fn set_fixed_register(&mut self, number: Option<u8>) {
        self.fixed_register = number;
    }
    fn tied_output(&self) -> Option<u8> {
        if let AsmConstraint::Tied(digit) = self.constraint {
            Some(digit)
        } else {
            None
        }
    }
    fn early_clobber(&self) -> bool {
        self.constraint.early_clobber()
    }
    fn read_write(&self) -> bool {
        self.constraint.direction() == AsmDirection::ReadWrite
    }
    fn memory(&self) -> bool {
        matches!(self.constraint, AsmConstraint::Memory { .. })
    }
    fn needs_register(&self) -> bool {
        self.constraint.needs_register()
    }
    fn bank(&self) -> u8 {
        u8::from(self.constraint.simd())
    }
    fn accepts_zero(&self) -> bool {
        matches!(self.constraint, AsmConstraint::RegisterOrZero)
    }
    fn accepts_symbol(&self) -> bool {
        matches!(
            self.constraint,
            AsmConstraint::Immediate(AsmImmediate::Integer)
        )
    }
    fn spelling(&self) -> String {
        self.constraint.spelling()
    }
}

impl ConstraintBody for AsmBody {
    type Reg = VReg;
    type Operand = AsmOperand;
    fn outputs(&self) -> &[AsmOperand] {
        &self.outputs
    }
    fn inputs(&self) -> &[AsmOperand] {
        &self.inputs
    }
    fn outputs_mut(&mut self) -> &mut [AsmOperand] {
        &mut self.outputs
    }
    fn inputs_mut(&mut self) -> &mut [AsmOperand] {
        &mut self.inputs
    }
    fn defined(&self) -> Vec<VReg> {
        self.defined()
    }
    fn clobbered_registers(&self) -> Vec<u8> {
        self.clobbered_registers()
    }
}
