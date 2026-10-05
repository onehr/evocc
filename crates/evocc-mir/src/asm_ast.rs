//! The restricted assembly AST of the checked core-v1 path.
//!
//! This builder writes one AST line for each line that `emit.rs` writes for an
//! allocated function, with every optional emission variant off. It covers
//! only the core-v1 envelope and refuses every other form. The builder is not
//! trusted: the Lean printer prints the AST and compares the text with the
//! actual output of the same compilation, byte for byte.
//!
//! `export_core_asm` runs the native pipeline with default options on one
//! EvoIR module. The binary `evo-asm-ast` and the proof-required driver call
//! it. Export is not proof admission: the Lean print check and the Lean
//! checker decide.

use std::fmt::Write as _;

use evocc_evoir::{GlobalVisibility, Linkage, Module};

use crate::emit::Frame;
use crate::isa::aarch64::encodings;
use crate::{
    Allocated, BinOp, Cond, ExtOp, FlagOp, LoadKind, MInst, MTerm, RegClass, StoreKind, VReg,
    WideMul, Width,
};

pub type Result<T> = std::result::Result<T, String>;

/// Export the assembly AST with the frame hints, and the assembly text, of one
/// checked Linux AArch64 compilation with default options. The text is the
/// text that `compile_module_with_options` writes. The function refuses a
/// module outside the core assembly profile, and refuses if the allocated
/// functions do not reproduce the compiler text.
pub fn export_core_asm(module: &Module) -> Result<(String, String)> {
    if !module.globals.is_empty()
        || !module.aliases.is_empty()
        || !module.assembly.is_empty()
        || !module.lifecycle.is_empty()
        || !module.tls.is_empty()
        || !module.extern_objects.is_empty()
        || !module.declares.is_empty()
    {
        return Err("module outside the core assembly profile".into());
    }
    for function in &module.functions {
        if function.alignment != 4
            || function.section.is_some()
            || function.patchable_entry.is_some()
            || function.stack_protect
            || function.linkage == Linkage::Weak
            || !matches!(
                function.visibility,
                GlobalVisibility::External | GlobalVisibility::Internal
            )
        {
            return Err(format!(
                "function {} outside the core assembly profile",
                function.name
            ));
        }
    }
    let target = crate::Target::Linux;
    let profile = crate::UbCheckProfile::Checked;
    let keep = crate::emitted_functions(module);
    let lowered =
        crate::lower_module_for_profile(target, module, &|name| !keep.contains(name), profile)
            .map_err(|e| format!("{e:?}"))?;
    let mut functions = Vec::new();
    for function in &lowered.functions {
        if !keep.contains(&function.name) {
            continue;
        }
        let destroyed = crate::out_of_ssa(function);
        let mut allocated =
            crate::allocate_for_with_options(target, &destroyed, Default::default())
                .map_err(|detail| format!("@{}: {detail}", function.name))?;
        allocated.internal = module
            .functions
            .iter()
            .find(|source| source.name == function.name)
            .is_some_and(|source| source.visibility == GlobalVisibility::Internal);
        crate::verify_allocation_for(target, &allocated)
            .map_err(|refusal| format!("@{}: {}", function.name, refusal.detail))?;
        functions.push(allocated);
    }
    let unit = crate::emit_module_unit(target, &functions, &[], &[]);
    let text = format!("// evocc-native-ub-checks={}\n{unit}", profile.name());
    let actual = crate::compile_module_with_options(
        target,
        module,
        &|_| true,
        crate::RelocationModel::PositionIndependent,
        &crate::emit::CodegenOptions::default(),
    )
    .map_err(|e| format!("{e:?}"))?;
    if actual != text {
        return Err("allocated functions do not reproduce the compiler text".into());
    }
    let mut nodes = Vec::new();
    for allocated in &functions {
        let frame = crate::frame_of_for(target, allocated);
        nodes.push(function(allocated, &frame)?);
    }
    let root = Node::Array(vec![
        Node::Text("core-asm-v1".into()),
        Node::Text("aarch64-unknown-linux-gnu".into()),
        Node::Text("checked".into()),
        Node::Text(format!(
            "EvoCC {} aarch64-unknown-linux-gnu",
            env!("CARGO_PKG_VERSION")
        )),
        Node::Array(nodes),
    ]);
    let mut output = String::new();
    root.render(&mut output);
    output.push('\n');
    Ok((output, actual))
}

/// A JSON value of the wire format.
enum Node {
    Text(String),
    Natural(u64),
    Integer(i64),
    Flag(bool),
    Array(Vec<Node>),
}

impl Node {
    fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    fn tagged(tag: &str, fields: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(std::iter::once(Self::text(tag)).chain(fields).collect())
    }

    fn render(&self, out: &mut String) {
        match self {
            Self::Text(value) => {
                out.push('"');
                for character in value.chars() {
                    match character {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\u{0}'..='\u{1f}' => {
                            let _ = write!(out, "\\u{:04x}", u32::from(character));
                        }
                        other => out.push(other),
                    }
                }
                out.push('"');
            }
            Self::Natural(value) => {
                let _ = write!(out, "{value}");
            }
            Self::Integer(value) => {
                let _ = write!(out, "{value}");
            }
            Self::Flag(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    value.render(out);
                }
                out.push(']');
            }
        }
    }
}

/// The frame pointer, the link register and the 64-bit view of a register.
const FRAME_POINTER: u64 = 29;
const LINK_REGISTER: u64 = 30;

/// The largest immediate of one `add`/`sub` instruction. A larger frame or
/// slot offset needs a longer sequence, which this profile refuses.
const IMM12: u64 = encodings::ADD_SUB_MAX;

/// The largest offset of a scaled 32-bit transfer from `sp`. A larger offset
/// needs a scratch register, which this profile refuses.
const SCALED_LIMIT: u64 = encodings::MEMORY_SCALED_MAX * 4;

/// At or above this instruction count the emitter relaxes conditional
/// branches. This profile refuses relaxed branches.
const RELAXATION_THRESHOLD: usize = (encodings::TBZ_MAX as usize + 1)
    * encodings::TBZ_SCALE as usize
    / encodings::INSTRUCTION_BYTES
    / 2;

fn view(class: RegClass) -> Result<Node> {
    match class {
        RegClass::Gpr32 => Ok(Node::text("w")),
        RegClass::Gpr64 => Ok(Node::text("x")),
        _ => Err("register class outside the core assembly profile".into()),
    }
}

fn width(value: Width) -> Node {
    Node::text(value.mnemonic())
}

fn cond(value: Cond) -> Node {
    Node::text(value.mnemonic())
}

struct Builder<'a> {
    allocated: &'a Allocated,
    frame: &'a Frame,
    lines: Vec<Node>,
    instructions: usize,
}

impl Builder<'_> {
    /// The physical register of a virtual register; an unassigned register
    /// is the zero register, as in `emit.rs`.
    fn reg(&self, value: VReg) -> Result<Node> {
        match self.allocated.assignment.get(&value.id) {
            None => Ok(Node::text("zr")),
            Some(number) if *number <= 30 => Ok(Node::Natural(u64::from(*number))),
            Some(_) => Err("register number outside x0..x30".into()),
        }
    }

    fn base(&self, value: VReg) -> Result<Node> {
        match self.allocated.assignment.get(&value.id) {
            Some(number) if *number <= 30 => Ok(Node::Natural(u64::from(*number))),
            _ => Err("memory base without a general register".into()),
        }
    }

    fn inst(&mut self, tag: &str, fields: Vec<Node>) {
        self.lines.push(Node::tagged(tag, fields));
        self.instructions += 1;
    }

    fn label(&mut self, block: u32) {
        self.lines
            .push(Node::tagged("label", [Node::Natural(u64::from(block))]));
    }

    /// `emit_frame_access` for an offset that a scaled immediate reaches.
    fn frame_access(&mut self, store: bool, class: RegClass, reg: Node, offset: u64) -> Result<()> {
        if offset > SCALED_LIMIT {
            return Err("frame offset outside the scaled immediate range".into());
        }
        self.inst(
            if store { "str" } else { "ldr" },
            vec![
                view(class)?,
                reg,
                Node::text("sp"),
                Node::Integer(offset as i64),
            ],
        );
        Ok(())
    }

    /// `emit_immediate`: `movz` of the low half, then `movk` of each
    /// nonzero higher half.
    fn immediate(&mut self, class: RegClass, dst: VReg, value: u64) -> Result<()> {
        let bits = match class {
            RegClass::Gpr64 => 64,
            RegClass::Gpr32 => 32,
            _ => return Err("immediate outside the general registers".into()),
        };
        let chunks = bits / 16;
        self.inst(
            "movz",
            vec![view(class)?, self.reg(dst)?, Node::Natural(value & 0xffff)],
        );
        for index in 1..chunks {
            let shift = 16 * index;
            let part = (value >> shift) & 0xffff;
            if part != 0 {
                self.inst(
                    "movk",
                    vec![
                        view(class)?,
                        self.reg(dst)?,
                        Node::Natural(part),
                        Node::Natural(shift),
                    ],
                );
            }
        }
        Ok(())
    }

    fn prologue(&mut self) -> Result<()> {
        let frame = self.frame;
        self.inst(
            "stp_pre",
            vec![
                Node::Natural(FRAME_POINTER),
                Node::Natural(LINK_REGISTER),
                Node::Integer(-16),
            ],
        );
        self.inst("mov_from_sp", vec![Node::Natural(FRAME_POINTER)]);
        if frame.size != 0 {
            if frame.size > IMM12 {
                return Err("frame size outside the single sub immediate".into());
            }
            self.inst("sub_sp", vec![Node::Natural(frame.size)]);
        }
        for (number, offset) in &frame.saved {
            self.frame_access(
                true,
                RegClass::Gpr64,
                Node::Natural(u64::from(*number)),
                *offset,
            )?;
        }
        Ok(())
    }

    fn epilogue(&mut self) -> Result<()> {
        let frame = self.frame;
        for (number, offset) in &frame.saved {
            self.frame_access(
                false,
                RegClass::Gpr64,
                Node::Natural(u64::from(*number)),
                *offset,
            )?;
        }
        if frame.size != 0 {
            self.inst("add_sp", vec![Node::Natural(frame.size)]);
        }
        self.inst(
            "ldp_post",
            vec![
                Node::Natural(FRAME_POINTER),
                Node::Natural(LINK_REGISTER),
                Node::Integer(16),
            ],
        );
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn instruction(&mut self, inst: &MInst) -> Result<()> {
        let at = |builder: &Self, reg: VReg| builder.reg(reg);
        match inst {
            MInst::Mov { dst, src } => {
                if dst.class == src.class {
                    self.inst(
                        "mov",
                        vec![view(dst.class)?, at(self, *dst)?, at(self, *src)?],
                    );
                } else {
                    view(dst.class)?;
                    view(src.class)?;
                    self.inst(
                        "mov",
                        vec![Node::text("w"), at(self, *dst)?, at(self, *src)?],
                    );
                }
            }
            MInst::MovImm { dst, imm } => self.immediate(dst.class, *dst, *imm)?,
            MInst::Load {
                dst,
                base,
                offset,
                kind,
                volatile: false,
                ..
            } => {
                let w = match kind {
                    LoadKind::W => "w",
                    LoadKind::X => "x",
                    _ => return Err("load kind outside the core assembly profile".into()),
                };
                self.inst(
                    "ldr",
                    vec![
                        Node::text(w),
                        at(self, *dst)?,
                        self.base(*base)?,
                        Node::Integer(*offset),
                    ],
                );
            }
            MInst::Store {
                src,
                base,
                offset,
                kind,
                volatile: false,
                ..
            } => {
                let w = match kind {
                    StoreKind::W => "w",
                    StoreKind::X => "x",
                    _ => return Err("store kind outside the core assembly profile".into()),
                };
                self.inst(
                    "str",
                    vec![
                        Node::text(w),
                        at(self, *src)?,
                        self.base(*base)?,
                        Node::Integer(*offset),
                    ],
                );
            }
            MInst::Binary {
                op,
                dst,
                lhs,
                rhs,
                width: w,
            } => {
                let op = binop(*op);
                self.inst(
                    "alu",
                    vec![
                        Node::text(op),
                        width(*w),
                        at(self, *dst)?,
                        at(self, *lhs)?,
                        at(self, *rhs)?,
                    ],
                );
            }
            MInst::BinaryImm {
                op,
                dst,
                lhs,
                imm,
                width: w,
            } => {
                let op = binop(*op);
                self.inst(
                    "alu_imm",
                    vec![
                        Node::text(op),
                        width(*w),
                        at(self, *dst)?,
                        at(self, *lhs)?,
                        Node::Natural(*imm),
                    ],
                );
            }
            MInst::Msub {
                dst,
                n,
                m,
                a,
                width: w,
            }
            | MInst::Madd {
                dst,
                n,
                m,
                a,
                width: w,
            } => {
                let tag = if matches!(inst, MInst::Msub { .. }) {
                    "msub"
                } else {
                    "madd"
                };
                self.inst(
                    tag,
                    vec![
                        width(*w),
                        at(self, *dst)?,
                        at(self, *n)?,
                        at(self, *m)?,
                        at(self, *a)?,
                    ],
                );
            }
            MInst::FlagBinary {
                op,
                dst,
                lhs,
                rhs,
                width: w,
                ..
            } => {
                let op = match op {
                    FlagOp::Adds => "adds",
                    FlagOp::Subs => "subs",
                };
                self.inst(
                    "flag_alu",
                    vec![
                        Node::text(op),
                        width(*w),
                        at(self, *dst)?,
                        at(self, *lhs)?,
                        at(self, *rhs)?,
                    ],
                );
            }
            MInst::WideMultiply { op, dst, lhs, rhs } => {
                let op = match op {
                    WideMul::Smull => "smull",
                    WideMul::Smulh => "smulh",
                    WideMul::Umulh => "umulh",
                };
                self.inst(
                    "wide_mul",
                    vec![
                        Node::text(op),
                        at(self, *dst)?,
                        at(self, *lhs)?,
                        at(self, *rhs)?,
                    ],
                );
            }
            MInst::Extend { op, dst, src } => {
                let op = match op {
                    ExtOp::Sxtb => "sxtb",
                    ExtOp::Sxth => "sxth",
                    ExtOp::Sxtw => "sxtw",
                    ExtOp::Uxtb => "uxtb",
                    ExtOp::Uxth => "uxth",
                };
                self.inst(
                    "extend",
                    vec![
                        Node::text(op),
                        view(dst.class)?,
                        at(self, *dst)?,
                        at(self, *src)?,
                    ],
                );
            }
            MInst::Bfx {
                signed,
                dst,
                src,
                lsb,
                bits,
                width: w,
            } => self.inst(
                "bfx",
                vec![
                    Node::Flag(*signed),
                    width(*w),
                    at(self, *dst)?,
                    at(self, *src)?,
                    Node::Natural(u64::from(*lsb)),
                    Node::Natural(u64::from(*bits)),
                ],
            ),
            MInst::Cmp {
                lhs, rhs, width: w, ..
            } => self.inst("cmp", vec![width(*w), at(self, *lhs)?, at(self, *rhs)?]),
            MInst::CmpImm {
                lhs, imm, width: w, ..
            } => self.inst(
                "cmp_imm",
                vec![width(*w), at(self, *lhs)?, Node::Natural(*imm)],
            ),
            MInst::TstImm {
                lhs, imm, width: w, ..
            } => self.inst(
                "tst_imm",
                vec![width(*w), at(self, *lhs)?, Node::Natural(*imm)],
            ),
            MInst::Cset { dst, cond: c, .. } => {
                self.inst("cset", vec![view(dst.class)?, at(self, *dst)?, cond(*c)]);
            }
            MInst::Csel {
                dst,
                lhs,
                rhs,
                cond: c,
                width: w,
                ..
            } => self.inst(
                "csel",
                vec![
                    width(*w),
                    at(self, *dst)?,
                    at(self, *lhs)?,
                    at(self, *rhs)?,
                    cond(*c),
                ],
            ),
            MInst::Bl {
                symbol,
                fp_mode: None,
                ..
            } => self.inst("bl", vec![Node::text(symbol)]),
            MInst::FrameSlot { dst, origin, .. } => {
                if dst.class != RegClass::Gpr64 {
                    return Err("frame slot address outside a 64-bit register".into());
                }
                let Some(offset) = self.frame.slots.get(origin).copied() else {
                    return Err("frame slot without a layout offset".into());
                };
                if offset > IMM12 {
                    return Err("frame slot offset outside the add immediate".into());
                }
                self.inst("add_from_sp", vec![at(self, *dst)?, Node::Natural(offset)]);
            }
            MInst::SpillStore { src, slot } | MInst::SpillReload { dst: src, slot } => {
                let Some(offset) = self.frame.spills.get(slot).copied() else {
                    return Err("spill slot without a layout offset".into());
                };
                let store = matches!(inst, MInst::SpillStore { .. });
                let reg = at(self, *src)?;
                self.frame_access(store, src.class, reg, offset)?;
            }
            MInst::IncomingArg { dst, slot } => {
                let offset = 16 + 8 * u64::from(*slot);
                self.inst(
                    "ldr",
                    vec![
                        view(dst.class)?,
                        at(self, *dst)?,
                        Node::Natural(FRAME_POINTER),
                        Node::Integer(offset as i64),
                    ],
                );
            }
            MInst::OutgoingArg { src, slot } => {
                let offset = 8 * u64::from(*slot);
                self.inst(
                    "str",
                    vec![
                        view(src.class)?,
                        at(self, *src)?,
                        Node::text("sp"),
                        Node::Integer(offset as i64),
                    ],
                );
            }
            _ => return Err("machine instruction outside the core assembly profile".into()),
        }
        Ok(())
    }

    fn branch(&mut self, next: Option<u32>, target: u32) {
        if next != Some(target) {
            self.inst("b", vec![Node::Natural(u64::from(target))]);
        }
    }

    fn terminator(&mut self, term: &MTerm, next: Option<u32>) -> Result<()> {
        let layout = |taken: u32, untaken: u32| {
            if next == Some(taken) && taken != untaken {
                (true, untaken, taken)
            } else {
                (false, taken, untaken)
            }
        };
        match term {
            MTerm::B { target, .. } => self.branch(next, *target),
            MTerm::BCond {
                cond: c,
                taken,
                untaken,
                ..
            } => {
                let (invert, taken, untaken) = layout(*taken, *untaken);
                let c = if invert { c.inverted() } else { *c };
                self.inst("b_cond", vec![cond(c), Node::Natural(u64::from(taken))]);
                self.branch(next, untaken);
            }
            MTerm::Cb {
                nonzero,
                src,
                taken,
                untaken,
                ..
            } => {
                let (invert, taken, untaken) = layout(*taken, *untaken);
                let fields = vec![
                    Node::Flag(*nonzero ^ invert),
                    view(src.class)?,
                    self.reg(*src)?,
                    Node::Natural(u64::from(taken)),
                ];
                self.inst("cb", fields);
                self.branch(next, untaken);
            }
            MTerm::Tb {
                nonzero,
                src,
                bit,
                taken,
                untaken,
                ..
            } => {
                let (invert, taken, untaken) = layout(*taken, *untaken);
                let fields = vec![
                    Node::Flag(*nonzero ^ invert),
                    view(src.class)?,
                    self.reg(*src)?,
                    Node::Natural(u64::from(*bit)),
                    Node::Natural(u64::from(taken)),
                ];
                self.inst("tb", fields);
                self.branch(next, untaken);
            }
            MTerm::Ret { .. } => {
                self.epilogue()?;
                self.inst("ret", vec![]);
            }
            MTerm::Brk { imm } => self.inst("brk", vec![Node::Natural(u64::from(*imm))]),
            _ => return Err("machine terminator outside the core assembly profile".into()),
        }
        Ok(())
    }
}

fn binop(op: BinOp) -> &'static str {
    op.mnemonic()
}

/// The frame layout as an untrusted hint for the Lean checker.
fn frame_hint(frame: &Frame) -> Node {
    let pairs = |entries: Vec<(u64, u64)>| {
        Node::Array(
            entries
                .into_iter()
                .map(|(a, b)| Node::Array(vec![Node::Natural(a), Node::Natural(b)]))
                .collect(),
        )
    };
    Node::Array(vec![
        Node::Natural(frame.size),
        pairs(
            frame
                .saved
                .iter()
                .map(|(r, o)| (u64::from(*r), *o))
                .collect(),
        ),
        pairs(
            frame
                .spills
                .iter()
                .map(|(s, o)| (u64::from(*s), *o))
                .collect(),
        ),
        pairs(frame.slots.iter().map(|(s, o)| (*s, *o)).collect()),
        Node::Natural(frame.outgoing),
    ])
}

/// One function: `[name, global, lines, frame]`.
fn function(allocated: &Allocated, frame: &Frame) -> Result<Node> {
    if frame.alignment != 16
        || frame.dynamic
        || frame.register_save.is_some()
        || !frame.saved_vectors.is_empty()
        || frame.stack_guard.is_some()
        || frame.frameless
        || frame.paired_saves
        || frame.shared_epilogue
        || frame.unwind_tables
        || frame.sign_returns
        || frame.strict_alignment
        || frame.compact_constants
        || frame.local_selection
    {
        return Err("frame outside the core assembly profile".into());
    }
    let mut builder = Builder {
        allocated,
        frame,
        lines: Vec::new(),
        instructions: 0,
    };
    builder.prologue()?;
    let order: Vec<u32> = allocated.blocks.iter().map(|block| block.id).collect();
    for (index, block) in allocated.blocks.iter().enumerate() {
        builder.label(block.id);
        for inst in &block.insts {
            builder.instruction(inst)?;
        }
        builder.terminator(&block.term, order.get(index + 1).copied())?;
    }
    if builder.instructions >= RELAXATION_THRESHOLD {
        return Err("function long enough for branch relaxation".into());
    }
    Ok(Node::Array(vec![
        Node::text(&allocated.name),
        Node::Flag(!allocated.internal),
        Node::Array(builder.lines),
        frame_hint(frame),
    ]))
}
