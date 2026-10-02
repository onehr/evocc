//! AArch64 ABI adapters over the generated Darwin, Linux and kernel-profile
//! tables (`targets/aarch64/abi-*`, `profile-kernel`).
//!
//! The shared facts drive the generic cursor, allocator boundary, verifier and
//! frame layout. The remaining fields serve the one AArch64 aggregate, HFA/HVA
//! and variadic classifier in lowering; there is no second classifier.

use crate::generic::abi::{AbiFacts, AbiSpec, ArgumentCursor};
use crate::generic::allocation::AllocClass;
use crate::{RegClass, Target};

pub use super::abi_tables::{DARWIN, DARWIN_DIGEST, KERNEL, KERNEL_DIGEST, LINUX, LINUX_DIGEST};

/// Field offsets of the AArch64 `va_list` object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VaListLayout {
    pub stack: i64,
    pub general_top: i64,
    pub vector_top: i64,
    pub general_offset: i64,
    pub vector_offset: i64,
}

/// One operating-system ABI: the shared facts and the AArch64 classifier's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aarch64Abi {
    pub facts: AbiFacts,
    /// Section 6: "a `byval` record of at most 16 bytes is loaded from the
    /// copy into the next one or two general registers as 8-byte chunks ... a
    /// `byval` record above 16 bytes stays a pointer to the caller's copy";
    /// a record result follows the same limit.
    pub record_register_bytes: u64,
    /// Whether a 16-byte-aligned record starts at an even general register.
    pub aligned_record_pair_rounding: bool,
    pub va_list: VaListLayout,
}

impl AbiSpec for Aarch64Abi {
    fn facts(&self) -> &AbiFacts {
        &self.facts
    }
}

/// The allocator bank number of a register class.
#[must_use]
pub fn bank_of(class: RegClass) -> usize {
    usize::from(AllocClass::bank(class))
}

/// The bank of general registers and of vector registers.
#[must_use]
pub fn general_bank() -> usize {
    bank_of(RegClass::Gpr64)
}

#[must_use]
pub fn vector_bank() -> usize {
    bank_of(RegClass::Simd128)
}

impl Aarch64Abi {
    /// Argument registers of the general bank.
    #[must_use]
    pub fn general_arguments(&self) -> usize {
        usize::from(self.facts.argument_banks[general_bank()].count)
    }

    #[must_use]
    pub fn vector_arguments(&self) -> usize {
        usize::from(self.facts.argument_banks[vector_bank()].count)
    }

    /// Ordinary results a function or call may have.
    #[must_use]
    pub fn general_results(&self) -> usize {
        usize::from(self.facts.result_banks[general_bank()].count)
    }

    #[must_use]
    pub fn argument_cursor(&self) -> ArgumentCursor<'static> {
        ArgumentCursor::new(self.facts.argument_banks)
    }

    /// The variadic save area of the general bank, then of the vector bank.
    #[must_use]
    pub fn general_save_area_bytes(&self) -> u64 {
        self.facts.variadic_save_area_bytes(general_bank())
    }

    #[must_use]
    pub fn vector_save_area_bytes(&self) -> u64 {
        self.facts.variadic_save_area_bytes(vector_bank())
    }

    /// Bytes one saved argument register of a bank occupies.
    #[must_use]
    pub fn general_save_bytes(&self) -> u64 {
        self.facts.variadic_save_bytes[general_bank()]
    }

    #[must_use]
    pub fn vector_save_bytes(&self) -> u64 {
        self.facts.variadic_save_bytes[vector_bank()]
    }
}

/// Place one piece of `class` in the shared cursor.
pub fn place(cursor: &mut ArgumentCursor<'_>, class: RegClass) -> Result<u8, u32> {
    cursor.place(bank_of(class), class.spill_words())
}

/// The machine form is validated before a target is chosen, so it admits
/// only the results every AArch64 ABI can return in one bank.
#[must_use]
pub fn machine_result_registers(bank: usize) -> usize {
    usize::from(
        DARWIN.facts.result_banks[bank]
            .count
            .min(LINUX.facts.result_banks[bank].count),
    )
}

impl Target {
    /// The generated ABI table of this operating system.
    #[must_use]
    pub const fn abi(self) -> &'static Aarch64Abi {
        match self {
            Self::Darwin => &DARWIN,
            Self::Linux => &LINUX,
        }
    }
}

/// A feature a profile admits only on its host ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileFeature {
    FunctionSections,
    DataSections,
    ExplicitSections,
    Tls,
    PatchableEntry,
    ProtectedVisibility,
}

impl ProfileFeature {
    #[must_use]
    pub const fn spelling(self) -> &'static str {
        match self {
            Self::FunctionSections => "function-sections",
            Self::DataSections => "data-sections",
            Self::ExplicitSections => "explicit-sections",
            Self::Tls => "tls",
            Self::PatchableEntry => "patchable-entry",
            Self::ProtectedVisibility => "protected-visibility",
        }
    }
}

/// Project choices of one build profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    pub name: &'static str,
    /// The ABI that hosts the profile's host-only features.
    pub host_abi: &'static str,
    pub host_only: &'static [&'static str],
    /// Bytes the stack-protector canary slot adds to a frame.
    pub stack_guard_slot_bytes: u64,
}

impl Profile {
    /// Whether `target` admits `feature` under this profile.
    #[must_use]
    pub fn admits(&self, target: Target, feature: ProfileFeature) -> bool {
        !self.host_only.contains(&feature.spelling()) || target.abi().facts.name == self.host_abi
    }
}
