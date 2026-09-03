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

pub mod interp;
pub mod lane;
pub mod lower;
pub mod text;
pub mod text_parse;
pub mod validate;

use std::fmt;

pub use interp::{interpret_module, MirError, MirLimits, MirOutcome, MirValue};
pub use lower::{lower_module, Refusal};
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
    Mov { dst: VReg, src: VReg },
    /// `mov rd, #imm`: the `movz`/`movk`/`movn` sequence, spelled as one form.
    MovImm { dst: VReg, imm: u64 },
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
    /// `msub rd, rn, rm, ra`: `rd := ra - rn * rm`.
    Msub {
        dst: VReg,
        n: VReg,
        m: VReg,
        a: VReg,
        width: Width,
    },
    /// `sxtb`, `sxth`, `sxtw`, `uxtb`, `uxth`.
    Extend { op: ExtOp, dst: VReg, src: VReg },
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
    Cset { dst: VReg, flags: VReg, cond: Cond },
    /// `csel rd, rn, rm, cond`.
    Csel {
        dst: VReg,
        flags: VReg,
        lhs: VReg,
        rhs: VReg,
        cond: Cond,
        width: Width,
    },
    /// `bl sym`. The argument and result registers are explicit because
    /// Section 7 requires the allocator to see them as fixed operands and the
    /// interpreter to pass them; Section 4's bare `bl sym` names neither.
    Bl {
        symbol: String,
        args: Vec<VReg>,
        result: Option<VReg>,
    },
    /// `blr rn`, with the same explicit operands.
    Blr {
        target: VReg,
        args: Vec<VReg>,
        result: Option<VReg>,
    },
    /// The frame slot of an EvoIR `alloca`.
    ///
    /// Section 5 lowers `alloca` to `add rd, x29, #slot`, but EvoIR Section 3.7
    /// gives every dynamic execution of an `alloca` a fresh object identity and
    /// the next dynamic instance of its origin site, and EvoMIR Section 1 makes
    /// the EvoIR specification win where the two meet. A slot address computed
    /// once per activation cannot carry that identity, so the allocation stays
    /// an instruction at the `alloca`'s program point and the frame offset is
    /// assigned by Section 6 later.
    FrameSlot {
        dst: VReg,
        origin: u64,
        size: u64,
        align: u32,
    },
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
    /// `cbz`/`cbnz rn, label`.
    Cb {
        nonzero: bool,
        src: VReg,
        width: Width,
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
    /// `ret`.
    Ret { value: Option<VReg> },
    /// `brk #imm16`.
    Brk { imm: u16 },
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
        | MInst::Extend { dst, .. }
        | MInst::Bfx { dst, .. }
        | MInst::Cmp { dst, .. }
        | MInst::CmpImm { dst, .. }
        | MInst::Cset { dst, .. }
        | MInst::Csel { dst, .. }
        | MInst::FrameSlot { dst, .. } => vec![*dst],
        MInst::Store { .. } => Vec::new(),
        MInst::Bl { result, .. } | MInst::Blr { result, .. } => result.iter().copied().collect(),
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
        MInst::Binary { lhs, rhs, .. } | MInst::Cmp { lhs, rhs, .. } => vec![*lhs, *rhs],
        MInst::BinaryImm { lhs, .. } | MInst::CmpImm { lhs, .. } | MInst::Bfx { src: lhs, .. } => {
            vec![*lhs]
        }
        MInst::Msub { n, m, a, .. } => vec![*n, *m, *a],
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
        MTerm::Ret { value } => value.iter().copied().collect(),
        MTerm::Brk { .. } => Vec::new(),
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
    }
}

/// Whether an instruction writes the flags register.
#[must_use]
pub fn writes_flags(inst: &MInst) -> bool {
    matches!(inst, MInst::Cmp { .. } | MInst::CmpImm { .. })
}
