//! Shared argument placement over an operating-system ABI description.
//!
//! The target still classifies each source value into register-sized pieces;
//! this module only places those pieces. Every register bank has its own
//! argument counter and all banks share one stack cursor counted in slots.

use super::register::RegisterList;

/// The generated facts of one ABI. Banks are indexed by the allocator's
/// register-bank number, so a bank's argument registers and its variadic save
/// width share an index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiFacts {
    pub name: &'static str,
    pub argument_banks: &'static [RegisterList],
    pub result_banks: &'static [RegisterList],
    /// Where a caller passes the destination of a memory-returned result.
    pub indirect_result: Option<u8>,
    /// The existing location of a result beyond its bank's result registers.
    pub result_overflow: Option<u8>,
    pub stack_slot_bytes: u64,
    pub stack_alignment_bytes: u64,
    /// Whether arguments beyond the general registers are lowered at all.
    pub stack_arguments: bool,
    pub callee_saved: &'static [u8],
    pub call_clobbers: &'static [u8],
    /// Registers of which a callee preserves only the low bytes.
    pub partially_preserved: &'static [u8],
    pub partially_preserved_bytes: u64,
    /// Bytes between the frame pointer and the first incoming stack slot.
    pub frame_record_bytes: u64,
    pub save_slot_bytes: u64,
    /// Bytes one argument register occupies in a variadic save area, per bank.
    pub variadic_save_bytes: &'static [u64],
    pub variadic_save_alignment: u64,
}

impl AbiFacts {
    pub fn require_indirect_result(&self) -> Result<u8, String> {
        self.indirect_result
            .ok_or_else(|| "ABI has no indirect-result register".into())
    }

    pub fn result_register(&self, location: Result<u8, u32>) -> Result<u8, String> {
        location
            .ok()
            .or(self.result_overflow)
            .ok_or_else(|| "ABI has no overflow-result register".into())
    }

    #[must_use]
    pub const fn stack_alignment_words(&self) -> u64 {
        self.stack_alignment_bytes / self.stack_slot_bytes
    }

    /// One bank's variadic register save area.
    #[must_use]
    pub fn variadic_save_area_bytes(&self, bank: usize) -> u64 {
        self.argument_banks[bank].count() as u64 * self.variadic_save_bytes[bank]
    }

    /// The adjacent save areas of every bank, in bank order.
    #[must_use]
    pub fn variadic_save_total_bytes(&self) -> u64 {
        (0..self.argument_banks.len())
            .map(|bank| self.variadic_save_area_bytes(bank))
            .sum()
    }

    /// The offset of incoming stack slot `slot` from the frame pointer.
    #[must_use]
    pub fn incoming_argument_offset(&self, slot: u32) -> u64 {
        self.frame_record_bytes + u64::from(slot) * self.stack_slot_bytes
    }

    /// The offset of outgoing stack slot `slot` from the caller's stack pointer.
    #[must_use]
    pub fn outgoing_argument_offset(&self, slot: u32) -> u64 {
        u64::from(slot) * self.stack_slot_bytes
    }

    /// The outgoing area for `slots` words, rounded to the stack alignment.
    #[must_use]
    pub fn outgoing_area_bytes(&self, slots: u32) -> u64 {
        u64::from(slots).next_multiple_of(self.stack_alignment_words()) * self.stack_slot_bytes
    }
}

/// An operating-system ABI as the shared algorithms see it. The target's own
/// classifier facts stay with its implementation.
pub trait AbiSpec {
    fn facts(&self) -> &AbiFacts;
}

impl AbiSpec for AbiFacts {
    fn facts(&self) -> &AbiFacts {
        self
    }
}

/// Independent per-bank register counters over one shared stack cursor.
#[derive(Debug, Clone)]
pub struct ArgumentCursor<'a> {
    banks: &'a [RegisterList],
    used: Vec<usize>,
    stack: u32,
}

impl<'a> ArgumentCursor<'a> {
    #[must_use]
    pub fn new(banks: &'a [RegisterList]) -> Self {
        Self {
            banks,
            used: vec![0; banks.len()],
            stack: 0,
        }
    }

    /// Place one piece of `bank`: its next register, or a stack slot aligned
    /// to the piece's own `words`. An exhausted bank never takes a register
    /// back once a piece has gone to the stack.
    pub fn place(&mut self, bank: usize, words: u32) -> Result<u8, u32> {
        let limit = self.banks[bank];
        let next = &mut self.used[bank];
        if let Some(number) = limit.nth(*next) {
            *next += 1;
            Ok(number)
        } else {
            self.stack = self.stack.next_multiple_of(words);
            let slot = self.stack;
            self.stack += words;
            Err(slot)
        }
    }

    /// Registers of `bank` consumed so far.
    #[must_use]
    pub fn used(&self, bank: usize) -> usize {
        self.used[bank]
    }

    #[must_use]
    pub const fn stack_words(&self) -> u32 {
        self.stack
    }

    #[must_use]
    pub fn stack_bytes(&self, abi: &impl AbiSpec) -> u64 {
        u64::from(self.stack) * abi.facts().stack_slot_bytes
    }

    /// Positions to skip so that a two-position piece of `bank` starts at an
    /// even register, or at an even stack slot once the bank is exhausted.
    #[must_use]
    pub fn pair_padding(&self, bank: usize) -> usize {
        if self.used[bank] < self.banks[bank].count() {
            self.used[bank] % 2
        } else {
            (self.stack % 2) as usize
        }
    }
}
