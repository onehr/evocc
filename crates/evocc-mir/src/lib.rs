//! EvoMIR: the machine-level intermediate representation of EvoCC.
//!
//! This crate implements Sections 2 to 5 and Appendix A of the EvoMIR
//! specification (`docs/evomir-spec.md`, revision 0.1) for the pre-allocation
//! form: the data model, the canonical textual form, the verifier of the
//! Section 2 `flags` rule, the executable semantics of Section 4, and the
//! Section 5 lowering from extracted EvoIR.
//!
//! Register allocation (Section 7), frame layout (Section 6) and emission
//! (Section 8) are deliberately absent; nothing here assigns a physical
//! register or a frame offset.
//!
//! Where revision 0.1 is silent on a rule this crate needs, the gap is
//! recorded in `GAPS.md`-style prose in the module that needs it and the
//! choice is spelled out at the point of use rather than left implicit.

#![forbid(unsafe_code)]

pub mod allocverify;
pub mod emit;
pub mod interp;
pub mod lane;
pub mod linscan;
pub mod lower;
pub mod native;
pub mod regalloc;
pub mod text;
pub mod text_parse;
pub mod validate;

use std::fmt;

pub use allocverify::{verify_allocation, VerifierRefusal};
pub use emit::{emit_module, emit_module_for, frame_of, Frame, Target};
pub use interp::{interpret_module, MirError, MirLimits, MirOutcome, MirValue};
pub use linscan::{
    allocate, allocate_for, render_allocated, Allocated, CALLEE_SAVED, CALLER_SAVED,
};
pub use lower::{lower_module, lower_module_for, lower_module_with, Refusal};
pub use native::{
    compile_module, compile_module_for, compile_module_with, native_corpus_entry, Driver,
    NativeStop, Toolchain,
};
pub use regalloc::out_of_ssa;
pub use text::render_module;
pub use text_parse::parse_module;
pub use validate::validate_module;

/// A register class of EvoMIR Section 2.
///
/// `gpr32` and `gpr64` name the same physical register file viewed at two
/// widths; `flags` is the single condition-code register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegClass {
    Gpr32,
    Gpr64,
    Flags,
}

impl RegClass {
    /// The canonical spelling of Appendix A.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Gpr32 => "gpr32",
            Self::Gpr64 => "gpr64",
            Self::Flags => "flags",
        }
    }

    /// The register width in bits, for the two general classes.
    #[must_use]
    pub const fn bit_width(self) -> Option<u16> {
        match self {
            Self::Gpr32 => Some(32),
            Self::Gpr64 => Some(64),
            Self::Flags => None,
        }
    }
}

/// A virtual register: the pre-allocation operand form of Section 2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VReg {
    pub id: u32,
    pub class: RegClass,
}

impl VReg {
    #[must_use]
    pub const fn new(id: u32, class: RegClass) -> Self {
        Self { id, class }
    }
}

impl fmt::Display for VReg {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "%v{}:{}", self.id, self.class.mnemonic())
    }
}

/// The operand width an arithmetic or logic form is spelled at: the `w` and
/// `x` spellings Section 4 says every such form has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Width {
    W,
    X,
}

impl Width {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::W => "w",
            Self::X => "x",
        }
    }

    #[must_use]
    pub const fn bits(self) -> u16 {
        match self {
            Self::W => 32,
            Self::X => 64,
        }
    }

    #[must_use]
    pub const fn mask(self) -> u64 {
        match self {
            Self::W => 0xffff_ffff,
            Self::X => u64::MAX,
        }
    }

    /// The register class this width spells.
    ///
    /// Appendix A puts a width suffix on every two-width form, and Section 4's
    /// semantics are the suffix's: an instruction reads and writes its own
    /// width, whatever class its operands were declared in. Emission spells
    /// every operand of such a form here, so that the assembly says what the
    /// interpreter does.
    #[must_use]
    pub const fn class(self) -> RegClass {
        match self {
            Self::W => RegClass::Gpr32,
            Self::X => RegClass::Gpr64,
        }
    }

    /// The width that holds a value of `class`.
    #[must_use]
    pub const fn for_class(class: RegClass) -> Option<Self> {
        match class {
            RegClass::Gpr32 => Some(Self::W),
            RegClass::Gpr64 => Some(Self::X),
            RegClass::Flags => None,
        }
    }
}

/// The condition codes Section 4 admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cond {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Lo,
    Ls,
    Hi,
    Hs,
    Vs,
    Vc,
    Cs,
    Cc,
}

impl Cond {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Eq => "eq",
            Self::Ne => "ne",
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Gt => "gt",
            Self::Ge => "ge",
            Self::Lo => "lo",
            Self::Ls => "ls",
            Self::Hi => "hi",
            Self::Hs => "hs",
            Self::Vs => "vs",
            Self::Vc => "vc",
            Self::Cs => "cs",
            Self::Cc => "cc",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "eq" => Self::Eq,
            "ne" => Self::Ne,
            "lt" => Self::Lt,
            "le" => Self::Le,
            "gt" => Self::Gt,
            "ge" => Self::Ge,
            "lo" => Self::Lo,
            "ls" => Self::Ls,
            "hi" => Self::Hi,
            "hs" => Self::Hs,
            "vs" => Self::Vs,
            "vc" => Self::Vc,
            "cs" => Self::Cs,
            "cc" => Self::Cc,
            _ => return None,
        })
    }

    /// True when this condition reads the flags as signed quantities.
    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(self, Self::Lt | Self::Le | Self::Gt | Self::Ge)
    }
}

/// The register-register arithmetic and logic forms of Section 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    SDiv,
    UDiv,
    And,
    Orr,
    Eor,
    Lsl,
    Lsr,
    Asr,
}

impl BinOp {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::SDiv => "sdiv",
            Self::UDiv => "udiv",
            Self::And => "and",
            Self::Orr => "orr",
            Self::Eor => "eor",
            Self::Lsl => "lsl",
            Self::Lsr => "lsr",
            Self::Asr => "asr",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "add" => Self::Add,
            "sub" => Self::Sub,
            "mul" => Self::Mul,
            "sdiv" => Self::SDiv,
            "udiv" => Self::UDiv,
            "and" => Self::And,
            "orr" => Self::Orr,
            "eor" => Self::Eor,
            "lsl" => Self::Lsl,
            "lsr" => Self::Lsr,
            "asr" => Self::Asr,
            _ => return None,
        })
    }

    /// Whether Section 4 gives this form an immediate spelling.
    #[must_use]
    pub const fn has_immediate_form(self) -> bool {
        matches!(
            self,
            Self::Add
                | Self::Sub
                | Self::And
                | Self::Orr
                | Self::Eor
                | Self::Lsl
                | Self::Lsr
                | Self::Asr
        )
    }
}

/// The flag-setting arithmetic forms of Section 4: "`adds`, `subs rd, rn, rm`
/// | as `add`/`sub`, and write `nzcv`".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FlagOp {
    Adds,
    Subs,
}

/// The two carry-propagating arithmetic forms of Section 5's `bits<128>` row.
///
/// They differ from [`FlagOp`] in reading `nzcv` as well: `adc` adds the carry
/// the preceding `adds` left, and `sbc` subtracts the borrow the preceding
/// `subs` left. Both are what carries a sum or a difference from the low half
/// of a pair into its high half (measured: GCC-14 `add128`, `sub128`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarryOp {
    Adc,
    Sbc,
}

impl CarryOp {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Adc => "adc",
            Self::Sbc => "sbc",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        match text {
            "adc" => Some(Self::Adc),
            "sbc" => Some(Self::Sbc),
            _ => None,
        }
    }
}

impl FlagOp {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Adds => "adds",
            Self::Subs => "subs",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "adds" => Self::Adds,
            "subs" => Self::Subs,
            _ => return None,
        })
    }
}

/// The two wide signed multiplies of Section 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WideMul {
    /// `smull xd, wn, wm`: the signed 32x32 to 64 product.
    Smull,
    /// `smulh xd, xn, xm`: the high 64 bits of a signed 64x64 product.
    Smulh,
    /// `umulh xd, xn, xm`: the high 64 bits of an unsigned 64x64 product.
    ///
    /// Section 5's `bits<128>` row (revision 0.10) needs it for the product's
    /// high half, and the kernel cohort's whole use of the width is that one
    /// product read as two halves.
    Umulh,
}

impl WideMul {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Smull => "smull",
            Self::Smulh => "smulh",
            Self::Umulh => "umulh",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "smull" => Self::Smull,
            "smulh" => Self::Smulh,
            "umulh" => Self::Umulh,
            _ => return None,
        })
    }
}

/// The fixed-width extension forms of Section 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExtOp {
    Sxtb,
    Sxth,
    Sxtw,
    Uxtb,
    Uxth,
}

impl ExtOp {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Sxtb => "sxtb",
            Self::Sxth => "sxth",
            Self::Sxtw => "sxtw",
            Self::Uxtb => "uxtb",
            Self::Uxth => "uxth",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "sxtb" => Self::Sxtb,
            "sxth" => Self::Sxth,
            "sxtw" => Self::Sxtw,
            "uxtb" => Self::Uxtb,
            "uxth" => Self::Uxth,
            _ => return None,
        })
    }

    /// The source bit width the form reads.
    #[must_use]
    pub const fn source_bits(self) -> u16 {
        match self {
            Self::Sxtb | Self::Uxtb => 8,
            Self::Sxth | Self::Uxth => 16,
            Self::Sxtw => 32,
        }
    }

    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(self, Self::Sxtb | Self::Sxth | Self::Sxtw)
    }
}

/// The load forms of Section 4.
///
/// `P` is the pointer-width load that reconstructs the out-of-band provenance
/// of EvoIR Section 5.3. Section 4 spells only the byte widths, so the
/// distinction between an eight-byte integer load and a pointer load is a
/// recorded gap; without it an EvoMIR interpreter cannot agree with the EvoIR
/// interpreter on a pointer load, which Section 9.1 requires of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LoadKind {
    B,
    H,
    W,
    X,
    Sb,
    Sh,
    Sw,
    P,
    Pf,
}

impl LoadKind {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::B => "ldrb",
            Self::H => "ldrh",
            Self::W => "ldr.w",
            Self::X => "ldr.x",
            Self::Sb => "ldrsb",
            Self::Sh => "ldrsh",
            Self::Sw => "ldrsw",
            Self::P => "ldr.p",
            Self::Pf => "ldr.pf",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "ldrb" => Self::B,
            "ldrh" => Self::H,
            "ldr.w" => Self::W,
            "ldr.x" => Self::X,
            "ldrsb" => Self::Sb,
            "ldrsh" => Self::Sh,
            "ldrsw" => Self::Sw,
            "ldr.p" => Self::P,
            "ldr.pf" => Self::Pf,
            _ => return None,
        })
    }

    /// Accessed bytes.
    #[must_use]
    pub const fn size_bytes(self) -> u64 {
        match self {
            Self::B | Self::Sb => 1,
            Self::H | Self::Sh => 2,
            Self::W | Self::Sw => 4,
            Self::X | Self::P | Self::Pf => 8,
        }
    }

    #[must_use]
    pub const fn is_signed(self) -> bool {
        matches!(self, Self::Sb | Self::Sh | Self::Sw)
    }
}

/// The store forms of Section 4, with the same pointer distinction as
/// [`LoadKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StoreKind {
    B,
    H,
    W,
    X,
    P,
}

impl StoreKind {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::B => "strb",
            Self::H => "strh",
            Self::W => "str.w",
            Self::X => "str.x",
            Self::P => "str.p",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "strb" => Self::B,
            "strh" => Self::H,
            "str.w" => Self::W,
            "str.x" => Self::X,
            "str.p" => Self::P,
            _ => return None,
        })
    }

    #[must_use]
    pub const fn size_bytes(self) -> u64 {
        match self {
            Self::B => 1,
            Self::H => 2,
            Self::W => 4,
            Self::X | Self::P => 8,
        }
    }
}

/// Which symbol table an `adrp`/`add` pair names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    Global,
    Function,
}

/// One EvoMIR instruction: a Section 4 form, its destination class and its
/// operands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MInst {
    /// `mov rd, rn`.
    Mov {
        dst: VReg,
        src: VReg,
    },
    /// `mov rd, #imm`: the `movz`/`movk`/`movn` sequence, spelled as one form.
    MovImm {
        dst: VReg,
        imm: u64,
    },
    /// `adrp rd, sym@PAGE`.
    Adrp {
        dst: VReg,
        symbol: String,
        kind: SymbolKind,
    },
    /// `add rd, rn, sym@PAGEOFF`.
    AddPageOff {
        dst: VReg,
        src: VReg,
        symbol: String,
        kind: SymbolKind,
    },
    /// `ldr rd, [rn, #off]` in all of its widths.
    Load {
        dst: VReg,
        base: VReg,
        offset: i64,
        kind: LoadKind,
        volatile: bool,
    },
    /// `str rd, [rn, #off]` in all of its widths.
    Store {
        src: VReg,
        base: VReg,
        offset: i64,
        kind: StoreKind,
        volatile: bool,
    },
    /// A register-register arithmetic or logic form.
    Binary {
        op: BinOp,
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    },
    /// The immediate spelling of the same.
    BinaryImm {
        op: BinOp,
        dst: VReg,
        lhs: VReg,
        imm: u64,
        width: Width,
    },
    /// `adds`/`subs rd, rn, rm`: the arithmetic result and `nzcv`.
    FlagBinary {
        op: FlagOp,
        dst: VReg,
        flags: VReg,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    },
    /// `smull`/`smulh`: the two wide signed multiplies, whose operand and
    /// result widths Section 4 fixes, so neither carries a width suffix.
    WideMultiply {
        op: WideMul,
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
    },
    /// `msub rd, rn, rm, ra`: `rd := ra - rn * rm`.
    Msub {
        dst: VReg,
        n: VReg,
        m: VReg,
        a: VReg,
        width: Width,
    },
    /// `madd rd, rn, rm, ra`: `rd := ra + rn * rm`.
    ///
    /// Section 5's `bits<128>` product accumulates the two cross terms into
    /// the high half with it.
    Madd {
        dst: VReg,
        n: VReg,
        m: VReg,
        a: VReg,
        width: Width,
    },
    /// `adc`/`sbc rd, rn, rm`: the arithmetic form that reads the carry.
    ///
    /// `flags` is the `nzcv` value the preceding `adds` or `subs` produced,
    /// which the Section 2 adjacency rule already requires to be the
    /// immediately preceding instruction. The form writes no flags of its own.
    CarryBinary {
        op: CarryOp,
        dst: VReg,
        flags: VReg,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    },
    /// `sbcs xzr, rn, rm`: the flag-setting subtract-with-borrow whose result
    /// is discarded.
    ///
    /// Section 5 states the ordered 128-bit comparison as "`cmp alo, blo` then
    /// `sbcs xzr, ahi, bhi` and `cset` with the 64-bit form's condition". Only
    /// the flags are wanted, so like [`MInst::Cmp`] the destination names the
    /// `flags` value this instruction defines, and `flags_in` names the one it
    /// reads.
    SubCarryFlags {
        dst: VReg,
        flags_in: VReg,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    },
    /// `tst rn, #imm`: the flags of `rn & imm`, the result discarded.
    ///
    /// Section 5's branch-free variable shift selects "on `tst s, #64`".
    TstImm {
        dst: VReg,
        lhs: VReg,
        imm: u64,
        width: Width,
    },
    /// `sxtb`, `sxth`, `sxtw`, `uxtb`, `uxth`.
    Extend {
        op: ExtOp,
        dst: VReg,
        src: VReg,
    },
    /// `sbfx`/`ubfx rd, rn, #lsb, #width`.
    Bfx {
        signed: bool,
        dst: VReg,
        src: VReg,
        lsb: u16,
        bits: u16,
        width: Width,
    },
    /// `cmp rn, rm`: sets the named `flags` value from `rn - rm`.
    Cmp {
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    },
    /// `cmp rn, #imm12`.
    CmpImm {
        dst: VReg,
        lhs: VReg,
        imm: u64,
        width: Width,
    },
    /// `cset rd, cond`.
    Cset {
        dst: VReg,
        flags: VReg,
        cond: Cond,
    },
    /// `csel rd, rn, rm, cond`.
    Csel {
        dst: VReg,
        flags: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: Cond,
        width: Width,
    },
    /// `bl @f(args) -> results`. Section 4 makes the argument and result
    /// registers explicit operands so the allocator sees them as fixed.
    Bl {
        symbol: String,
        args: Vec<VReg>,
        results: Vec<VReg>,
    },
    /// `blr rn(args) -> results`.
    Blr {
        target: VReg,
        args: Vec<VReg>,
        results: Vec<VReg>,
    },
    /// `ldp`/`stp rt1, rt2, [rn, #off]`: Section 4 admits them "prologue and
    /// epilogue only (Section 6)". `writeback` is the pre-index the prologue's
    /// `stp x29, x30, [sp, #-16]!` uses.
    /// `volatile` marks the pair that carries a `bits<128>` volatile access;
    /// the prologue's and epilogue's own pairs are never volatile.
    Ldp {
        rt1: VReg,
        rt2: VReg,
        base: VReg,
        offset: i64,
        writeback: bool,
        volatile: bool,
    },
    Stp {
        rt1: VReg,
        rt2: VReg,
        base: VReg,
        offset: i64,
        writeback: bool,
        volatile: bool,
    },
    /// A spilled value written to its frame slot, and read back from it.
    ///
    /// Section 7 spills "to a frame slot" and Section 6 lays out "one 8-byte
    /// slot per spilled virtual register", but neither gives EvoMIR a form for
    /// the two accesses. They are pseudo-forms here, on the pattern Section 4
    /// already uses for `frame.slot` and `mem.copy`; emission expands each to a
    /// `str` or `ldr` at the slot's frame offset. Recorded gap.
    SpillStore {
        src: VReg,
        slot: u32,
    },
    SpillReload {
        dst: VReg,
        slot: u32,
    },
    /// One incoming stack argument (Section 6 "Stack arguments", Linux,
    /// revision 0.10): "the callee reads slot `i` at `x29 + 16 + 8 * i`".
    ///
    /// A pseudo-form on the pattern of the spill accesses: it exists only
    /// after allocation, because which arguments find no register is what the
    /// allocation decides, and emission expands it against the frame.
    IncomingArg {
        dst: VReg,
        slot: u32,
    },
    /// One outgoing stack argument, written into the area the caller reserves
    /// "below its frame", at increasing addresses from the caller's `sp`.
    OutgoingArg {
        src: VReg,
        slot: u32,
    },
    /// The `mem.copy` pseudo-form of Section 4: `size` bytes from `src` to
    /// `dst`, "transferring per-byte initializedness and pointer fragments
    /// exactly as EvoIR Section 3.8's `copy` does".
    MemCopy {
        dst: VReg,
        src: VReg,
        size: u64,
        align: u32,
    },
    /// The `frame.slot` pseudo-form of Section 4: "the address of a fresh
    /// object of S bytes at alignment A, one per dynamic execution of the
    /// instruction, as EvoIR Section 3.7 requires of `alloca`".
    FrameSlot {
        dst: VReg,
        origin: u64,
        size: u64,
        align: u32,
    },
    /// The `asm` of EvoIR Section 3.11, "carried through unchanged: the
    /// template, its operands with their constraints, its clobbers".
    ///
    /// The interpreter does not execute it; emission writes the template with
    /// each operand reference replaced by the operand's assigned register,
    /// memory address or immediate.
    Asm(AsmBody),
}

// ---------------------------------------------------------------------------
// Inline assembly (EvoIR Section 3.11, EvoMIR Section 4)
// ---------------------------------------------------------------------------

/// Which way an assembly operand moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmDirection {
    /// `r`, `Q`: read by the template.
    Read,
    /// `=r`, `=Q`: written by it.
    Write,
    /// `+r`, `+Q`: both.
    ReadWrite,
}

impl AsmDirection {
    /// The prefix EvoIR Section 3.11 spells this direction with.
    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Read => "",
            Self::Write => "=",
            Self::ReadWrite => "+",
        }
    }

    #[must_use]
    pub const fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    #[must_use]
    pub const fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

/// The immediate letters of EvoIR Section 3.11's table.
///
/// Which values each letter admits is the assembler's business, not this
/// crate's: the lowering has already refused a non-constant, and the emitter
/// writes the constant literally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmImmediate {
    /// `I`: an immediate the add and subtract forms accept.
    I,
    /// `J`: its negation.
    J,
    /// `K`: an immediate the 32-bit logical forms accept.
    K,
    /// `L`: the 64-bit ones.
    L,
    /// `i`: an integer constant.
    Integer,
    /// `n`: an integer constant.
    N,
}

impl AsmImmediate {
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::I => "I",
            Self::J => "J",
            Self::K => "K",
            Self::L => "L",
            Self::Integer => "i",
            Self::N => "n",
        }
    }

    #[must_use]
    pub fn from_mnemonic(text: &str) -> Option<Self> {
        Some(match text {
            "I" => Self::I,
            "J" => Self::J,
            "K" => Self::K,
            "L" => Self::L,
            "i" => Self::Integer,
            "n" => Self::N,
            _ => return None,
        })
    }
}

/// One row of EvoIR Section 3.11's constraint table.
///
/// The vocabulary is closed: it is the one the configured kernel uses, and a
/// constraint outside it is refused before EvoMIR sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmConstraint {
    /// `r`, `=r`, `+r`: any register of the operand's class.
    Register(AsmDirection),
    /// `=&r`, `+&r`: a register no input operand of the same `asm` holds.
    EarlyClobber(AsmDirection),
    /// `rZ`: as `r`, except that a constant zero is the zero register.
    RegisterOrZero,
    /// `Q`, `=Q`, `+Q`, `Qo`: a register holding the operand's address.
    Memory {
        direction: AsmDirection,
        /// `Qo` rather than `Q`.
        offsettable: bool,
    },
    /// A digit: the same register as the output that digit names.
    Tied(u8),
    /// `I`, `J`, `K`, `L`, `i`, `n`: an immediate written literally.
    Immediate(AsmImmediate),
    /// `p`: a register holding the address.
    Address,
}

impl AsmConstraint {
    /// The constraint string EvoIR Section 3.11 spells this row with.
    #[must_use]
    pub fn spelling(self) -> String {
        match self {
            Self::Register(direction) => format!("{}r", direction.prefix()),
            Self::EarlyClobber(direction) => format!("{}&r", direction.prefix()),
            Self::RegisterOrZero => "rZ".to_owned(),
            Self::Memory {
                direction,
                offsettable,
            } => format!(
                "{}Q{}",
                direction.prefix(),
                if offsettable { "o" } else { "" }
            ),
            Self::Tied(digit) => digit.to_string(),
            Self::Immediate(letter) => letter.mnemonic().to_owned(),
            Self::Address => "p".to_owned(),
        }
    }

    /// Read one constraint string, or nothing when it is outside the table.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() == 1 {
            if let Some(digit) = text.chars().next().and_then(|c| c.to_digit(10)) {
                return Some(Self::Tied(u8::try_from(digit).ok()?));
            }
        }
        if text == "rZ" {
            return Some(Self::RegisterOrZero);
        }
        if text == "p" {
            return Some(Self::Address);
        }
        if let Some(letter) = AsmImmediate::from_mnemonic(text) {
            return Some(Self::Immediate(letter));
        }
        let (direction, rest) = match text.as_bytes().first() {
            Some(b'=') => (AsmDirection::Write, &text[1..]),
            Some(b'+') => (AsmDirection::ReadWrite, &text[1..]),
            _ => (AsmDirection::Read, text),
        };
        Some(match rest {
            "r" => Self::Register(direction),
            // The table has `=&r` and `+&r` and no read-only early clobber:
            // an operand the template only reads cannot be clobbered early.
            "&r" if direction != AsmDirection::Read => Self::EarlyClobber(direction),
            "Q" => Self::Memory {
                direction,
                offsettable: false,
            },
            "Qo" => Self::Memory {
                direction,
                offsettable: true,
            },
            _ => return None,
        })
    }

    /// Whether the operand occupies a register the allocator must assign.
    #[must_use]
    pub const fn needs_register(self) -> bool {
        !matches!(self, Self::Immediate(_))
    }

    /// Which way the operand moves, for the rows that carry a direction.
    #[must_use]
    pub const fn direction(self) -> AsmDirection {
        match self {
            Self::Register(direction)
            | Self::EarlyClobber(direction)
            | Self::Memory { direction, .. } => direction,
            // A `rZ`, an address, an immediate and a tied operand are read.
            Self::RegisterOrZero | Self::Address | Self::Immediate(_) | Self::Tied(_) => {
                AsmDirection::Read
            }
        }
    }
}

/// What one operand of an `asm` is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsmValue {
    /// A virtual register the allocator assigns.
    Reg(VReg),
    /// An immediate the emitter writes literally.
    Imm(i64),
    /// EvoIR Section 3.11's `rZ` when the operand is the constant zero: the
    /// zero register, which "consumes no allocation".
    Zero(RegClass),
}

/// One `asm` operand: its constraint, the name a `%[name]` reference uses,
/// and the value it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmOperand {
    pub constraint: AsmConstraint,
    pub name: Option<String>,
    pub value: AsmValue,
}

impl AsmOperand {
    /// A plain register operand with no name.
    #[must_use]
    pub const fn reg(constraint: AsmConstraint, reg: VReg) -> Self {
        Self {
            constraint,
            name: None,
            value: AsmValue::Reg(reg),
        }
    }

    /// An immediate operand with no name.
    #[must_use]
    pub const fn imm(letter: AsmImmediate, value: i64) -> Self {
        Self {
            constraint: AsmConstraint::Immediate(letter),
            name: None,
            value: AsmValue::Imm(value),
        }
    }

    /// The register this operand occupies, if it occupies one.
    #[must_use]
    pub const fn register(&self) -> Option<VReg> {
        match self.value {
            AsmValue::Reg(reg) => Some(reg),
            AsmValue::Imm(_) | AsmValue::Zero(_) => None,
        }
    }
}

/// One entry of an `asm`'s clobber list.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AsmClobber {
    /// A named physical register the `asm` writes.
    Register(u8),
    /// `cc`: the flags.
    Flags,
    /// `memory`: inert here, since the EvoIR instruction's maximal effects
    /// already order it against every other access.
    Memory,
}

impl AsmClobber {
    /// The name EvoIR Section 3.11's clobber list spells this with.
    #[must_use]
    pub fn spelling(&self) -> String {
        match self {
            Self::Register(number) => format!("x{number}"),
            Self::Flags => "cc".to_owned(),
            Self::Memory => "memory".to_owned(),
        }
    }

    /// Read one clobber name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "cc" => Some(Self::Flags),
            "memory" => Some(Self::Memory),
            _ => text
                .strip_prefix('x')
                .or_else(|| text.strip_prefix('w'))
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|number| *number <= 30)
                .map(Self::Register),
        }
    }
}

/// Everything an `asm` and an `asm.goto` share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsmBody {
    /// The template, copied verbatim and never parsed.
    pub template: String,
    /// EvoIR keeps `volatile` "for the assembler" and it changes nothing else.
    pub volatile: bool,
    pub outputs: Vec<AsmOperand>,
    pub inputs: Vec<AsmOperand>,
    pub clobbers: Vec<AsmClobber>,
}

impl AsmBody {
    /// The operands in the order a template reference numbers them: outputs
    /// first, then inputs, as the constraint table's digits require.
    #[must_use]
    pub fn numbered(&self) -> Vec<&AsmOperand> {
        self.outputs.iter().chain(self.inputs.iter()).collect()
    }

    /// The registers the instruction defines: its register outputs.
    ///
    /// A memory output is not one: what it writes is the memory the address
    /// names, and the register itself only carries the address.
    #[must_use]
    pub fn defined(&self) -> Vec<VReg> {
        self.outputs
            .iter()
            .filter(|operand| !matches!(operand.constraint, AsmConstraint::Memory { .. }))
            .filter_map(AsmOperand::register)
            .collect()
    }

    /// The registers the instruction reads: every input that occupies one, the
    /// address register of a memory output, which the template reads even when
    /// it writes through it, and a read-write register output.
    #[must_use]
    pub fn read(&self) -> Vec<VReg> {
        let mut registers: Vec<VReg> = self
            .inputs
            .iter()
            .filter_map(AsmOperand::register)
            .collect();
        for operand in &self.outputs {
            if matches!(operand.constraint, AsmConstraint::Memory { .. })
                || operand.constraint.direction() == AsmDirection::ReadWrite
            {
                registers.extend(operand.register());
            }
        }
        registers
    }

    /// The physical registers the clobber list names.
    #[must_use]
    pub fn clobbered_registers(&self) -> Vec<u8> {
        self.clobbers
            .iter()
            .filter_map(|clobber| match clobber {
                AsmClobber::Register(number) => Some(*number),
                AsmClobber::Flags | AsmClobber::Memory => None,
            })
            .collect()
    }

    /// Whether the clobber list names the flags.
    #[must_use]
    pub fn clobbers_flags(&self) -> bool {
        self.clobbers.contains(&AsmClobber::Flags)
    }
}
/// One EvoMIR terminator.
///
/// Every conditional form names both successors and the arguments each edge
/// passes, because the pre-allocation form of Section 2 is block-argument SSA
/// and has no fall-through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MTerm {
    /// `b label`.
    B { target: u32, args: Vec<VReg> },
    /// `b.cond label`.
    BCond {
        flags: VReg,
        cond: Cond,
        taken: u32,
        taken_args: Vec<VReg>,
        untaken: u32,
        untaken_args: Vec<VReg>,
    },
    /// `cbz`/`cbnz rn, ^then(args), ^else(args)`. The tested width is the
    /// operand's class, so Section 4 gives the form no width suffix.
    Cb {
        nonzero: bool,
        src: VReg,
        taken: u32,
        taken_args: Vec<VReg>,
        untaken: u32,
        untaken_args: Vec<VReg>,
    },
    /// `tbz`/`tbnz rn, #bit, label`.
    Tb {
        nonzero: bool,
        src: VReg,
        bit: u16,
        taken: u32,
        taken_args: Vec<VReg>,
        untaken: u32,
        untaken_args: Vec<VReg>,
    },
    /// `ret rs...`: Section 6 returns up to eight results in `x0` to `x7`.
    Ret { values: Vec<VReg> },
    /// `brk #imm16`.
    Brk { imm: u16 },
    /// The `asm.goto` of EvoIR Section 3.11: a terminator "whose successors
    /// are its labels and its fall-through".
    AsmGoto {
        body: AsmBody,
        /// Each `%l[label]` reference, in declaration order, with the values
        /// its edge passes.
        labels: Vec<(u32, Vec<VReg>)>,
        fallthrough: u32,
        fallthrough_args: Vec<VReg>,
    },
}

/// A basic block: label, block arguments, instructions and one terminator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MBlock {
    pub id: u32,
    pub args: Vec<VReg>,
    pub insts: Vec<MInst>,
    pub term: MTerm,
}

/// A function: a list of basic blocks in the order of its EvoIR source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MFunction {
    pub name: String,
    pub blocks: Vec<MBlock>,
}

/// A module: the global objects of Section 8 and the functions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MModule {
    pub globals: Vec<evocc_evoir::GlobalObject>,
    pub functions: Vec<MFunction>,
}

/// Every virtual register an instruction defines.
#[must_use]
pub fn inst_defs(inst: &MInst) -> Vec<VReg> {
    match inst {
        MInst::Mov { dst, .. }
        | MInst::MovImm { dst, .. }
        | MInst::Adrp { dst, .. }
        | MInst::AddPageOff { dst, .. }
        | MInst::Load { dst, .. }
        | MInst::Binary { dst, .. }
        | MInst::BinaryImm { dst, .. }
        | MInst::Msub { dst, .. }
        | MInst::Madd { dst, .. }
        | MInst::CarryBinary { dst, .. }
        | MInst::SubCarryFlags { dst, .. }
        | MInst::TstImm { dst, .. }
        | MInst::Extend { dst, .. }
        | MInst::Bfx { dst, .. }
        | MInst::Cmp { dst, .. }
        | MInst::CmpImm { dst, .. }
        | MInst::Cset { dst, .. }
        | MInst::Csel { dst, .. }
        | MInst::FrameSlot { dst, .. }
        | MInst::SpillReload { dst, .. }
        | MInst::IncomingArg { dst, .. }
        | MInst::WideMultiply { dst, .. } => vec![*dst],
        MInst::Ldp { rt1, rt2, .. } => vec![*rt1, *rt2],
        MInst::FlagBinary { dst, flags, .. } => vec![*dst, *flags],
        MInst::Store { .. }
        | MInst::MemCopy { .. }
        | MInst::SpillStore { .. }
        | MInst::OutgoingArg { .. }
        | MInst::Stp { .. } => Vec::new(),
        MInst::Bl { results, .. } | MInst::Blr { results, .. } => results.clone(),
        MInst::Asm(body) => body.defined(),
    }
}

/// Every virtual register an instruction reads, in operand order.
#[must_use]
pub fn inst_uses(inst: &MInst) -> Vec<VReg> {
    match inst {
        MInst::Mov { src, .. } | MInst::AddPageOff { src, .. } | MInst::Extend { src, .. } => {
            vec![*src]
        }
        MInst::MovImm { .. } | MInst::Adrp { .. } | MInst::FrameSlot { .. } => Vec::new(),
        MInst::Load { base, .. } => vec![*base],
        MInst::Store { src, base, .. } => vec![*src, *base],
        MInst::Binary { lhs, rhs, .. }
        | MInst::Cmp { lhs, rhs, .. }
        | MInst::FlagBinary { lhs, rhs, .. }
        | MInst::WideMultiply { lhs, rhs, .. } => vec![*lhs, *rhs],
        MInst::MemCopy { dst, src, .. } => vec![*dst, *src],
        MInst::SpillStore { src, .. } | MInst::OutgoingArg { src, .. } => vec![*src],
        MInst::SpillReload { .. } | MInst::IncomingArg { .. } => Vec::new(),
        MInst::Ldp { base, .. } => vec![*base],
        MInst::Stp { rt1, rt2, base, .. } => vec![*rt1, *rt2, *base],
        MInst::BinaryImm { lhs, .. } | MInst::CmpImm { lhs, .. } | MInst::Bfx { src: lhs, .. } => {
            vec![*lhs]
        }
        MInst::Msub { n, m, a, .. } | MInst::Madd { n, m, a, .. } => vec![*n, *m, *a],
        MInst::CarryBinary {
            flags, lhs, rhs, ..
        }
        | MInst::SubCarryFlags {
            flags_in: flags,
            lhs,
            rhs,
            ..
        } => vec![*flags, *lhs, *rhs],
        MInst::TstImm { lhs, .. } => vec![*lhs],
        MInst::Cset { flags, .. } => vec![*flags],
        MInst::Csel {
            flags, lhs, rhs, ..
        } => vec![*flags, *lhs, *rhs],
        MInst::Bl { args, .. } => args.clone(),
        MInst::Blr { target, args, .. } => {
            let mut uses = vec![*target];
            uses.extend(args.iter().copied());
            uses
        }
        MInst::Asm(body) => body.read(),
    }
}

/// Every virtual register a terminator reads, in operand order.
#[must_use]
pub fn term_uses(term: &MTerm) -> Vec<VReg> {
    match term {
        MTerm::B { args, .. } => args.clone(),
        MTerm::BCond {
            flags,
            taken_args,
            untaken_args,
            ..
        } => {
            let mut uses = vec![*flags];
            uses.extend(taken_args.iter().copied());
            uses.extend(untaken_args.iter().copied());
            uses
        }
        MTerm::Cb {
            src,
            taken_args,
            untaken_args,
            ..
        }
        | MTerm::Tb {
            src,
            taken_args,
            untaken_args,
            ..
        } => {
            let mut uses = vec![*src];
            uses.extend(taken_args.iter().copied());
            uses.extend(untaken_args.iter().copied());
            uses
        }
        MTerm::Ret { values } => values.clone(),
        MTerm::Brk { .. } => Vec::new(),
        MTerm::AsmGoto {
            body,
            labels,
            fallthrough_args,
            ..
        } => {
            let mut uses = body.read();
            for (_, args) in labels {
                uses.extend(args.iter().copied());
            }
            uses.extend(fallthrough_args.iter().copied());
            uses
        }
    }
}

/// The outgoing edges of a terminator: target block and the arguments passed.
#[must_use]
pub fn term_edges(term: &MTerm) -> Vec<(u32, &[VReg])> {
    match term {
        MTerm::B { target, args } => vec![(*target, args.as_slice())],
        MTerm::BCond {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        }
        | MTerm::Cb {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        }
        | MTerm::Tb {
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        } => vec![
            (*taken, taken_args.as_slice()),
            (*untaken, untaken_args.as_slice()),
        ],
        MTerm::Ret { .. } | MTerm::Brk { .. } => Vec::new(),
        MTerm::AsmGoto {
            labels,
            fallthrough,
            fallthrough_args,
            ..
        } => {
            let mut edges: Vec<(u32, &[VReg])> = labels
                .iter()
                .map(|(target, args)| (*target, args.as_slice()))
                .collect();
            edges.push((*fallthrough, fallthrough_args.as_slice()));
            edges
        }
    }
}

/// Whether an instruction writes the flags register.
#[must_use]
pub fn writes_flags(inst: &MInst) -> bool {
    match inst {
        MInst::Cmp { .. }
        | MInst::CmpImm { .. }
        | MInst::FlagBinary { .. }
        | MInst::SubCarryFlags { .. }
        | MInst::TstImm { .. } => true,
        // Section 7: "`cc` clobbers the flags, which ends any `flags` value's
        // adjacency".
        MInst::Asm(body) => body.clobbers_flags(),
        _ => false,
    }
}
