//! The canonical textual form of Appendix A: the renderer.
//!
//! Revision 0.2 carries a grammar. It fixes `func @name {`, block labels at
//! column zero with a parenthesised argument list, `%vN:class` on a definition
//! and `%vN` on a use, `.w`/`.x` width suffixes on the two-width forms,
//! `#imm` immediates, `[%vN, #off]` addresses, the `frame.slot` and `mem.copy`
//! attribute spellings, `bl @f(args) -> results`, two-successor conditionals,
//! `ret rs...`, the four symbol relocation names, the ` volatile` marker, and
//! the EvoIR Section 9.4 byte rules (four-space indent, `", "` between
//! operands, one blank line between blocks and between functions, one final
//! newline).
//!
//! The grammar writes a conditional terminator without naming the condition
//! register (`b.eq, ^t(...), ^e(...)`), so the flags value a terminator reads
//! is positional; Section 2's adjacency rule makes the producer unambiguous
//! and the parser recovers it from the instruction that precedes the
//! terminator.

use std::fmt::Write as _;

use evocc_evoir::{GlobalKind, GlobalObject, GlobalVisibility};

use crate::{MBlock, MFunction, MInst, MModule, MTerm, SymbolKind, VReg};

/// Render one module to its canonical bytes.
#[must_use]
pub fn render_module(module: &MModule) -> String {
    let mut text = String::new();
    let mut globals: Vec<&GlobalObject> = module.globals.iter().collect();
    globals.sort_by_key(|global| global.origin);
    for global in globals {
        render_global(&mut text, global);
    }
    if !module.globals.is_empty() && !module.functions.is_empty() {
        text.push('\n');
    }
    for (index, function) in module.functions.iter().enumerate() {
        if index != 0 {
            text.push('\n');
        }
        render_function(&mut text, function);
    }
    text
}

/// The Section 8 data line, in the EvoIR Section 9 spelling: "globals as data
/// ... with their EvoIR initial bytes".
fn render_global(text: &mut String, global: &GlobalObject) {
    let bytes: String = global
        .bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let initialized: String = global
        .initialized
        .iter()
        .map(|bit| if *bit { '1' } else { '0' })
        .collect();
    let kind = match global.kind {
        GlobalKind::File => "file",
        GlobalKind::FunctionStatic => "function_static",
        GlobalKind::StringLiteral => "string_literal",
    };
    let visibility = match global.visibility {
        GlobalVisibility::External => "external",
        GlobalVisibility::Internal => "internal",
    };
    let _ = writeln!(
        text,
        "global @{} origin={} kind={kind} visibility={visibility} size={} align={} bytes={bytes} initialized={initialized}",
        global.symbol,
        global.origin,
        global.layout.size_bytes,
        global.layout.align_bytes,
    );
}

fn render_function(text: &mut String, function: &MFunction) {
    let _ = writeln!(text, "func @{} {{", function.name);
    for (index, block) in function.blocks.iter().enumerate() {
        if index != 0 {
            text.push('\n');
        }
        render_block(text, function, index == 0, block);
    }
    text.push_str("}\n");
}

/// The label of a block: the first block of a function is `^entry`, every
/// other block is `^b` followed by its identifier (EvoIR Section 9.2).
fn label(function: &MFunction, id: u32) -> String {
    if function.blocks.first().is_some_and(|block| block.id == id) {
        "^entry".to_owned()
    } else {
        format!("^b{id}")
    }
}

fn render_block(text: &mut String, function: &MFunction, entry: bool, block: &MBlock) {
    let name = if entry {
        "^entry".to_owned()
    } else {
        format!("^b{}", block.id)
    };
    let _ = writeln!(text, "{name}({}):", definitions(&block.args));
    for inst in &block.insts {
        let _ = writeln!(text, "    {}", render_inst(inst));
    }
    let _ = writeln!(text, "    {}", render_term(function, &block.term));
}

/// A definition or block argument carries its class; a use does not, so that
/// one module has one rendering.
fn definitions(regs: &[VReg]) -> String {
    regs.iter()
        .map(VReg::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn uses(regs: &[VReg]) -> String {
    regs.iter()
        .map(|reg| use_of(*reg))
        .collect::<Vec<_>>()
        .join(", ")
}

fn use_of(reg: VReg) -> String {
    format!("%v{}", reg.id)
}

fn defined(reg: VReg) -> String {
    format!("{reg} = ")
}

/// A call's result list: definitions, so each carries its class.
fn results(regs: &[VReg]) -> String {
    if regs.is_empty() {
        String::new()
    } else {
        format!(" -> {}", definitions(regs))
    }
}

/// Which symbol table an `adrp`/`add` pair names.
///
/// Section 5 lowers both `global_addr` and `func_addr` to the same pair and
/// Section 8 gives both symbols the same platform prefix, so the printed pair
/// alone cannot say which table to resolve. EvoIR keeps globals and functions
/// in two tables and forbids a duplicate only within each, so the two can
/// collide; the relocation name carries the distinction rather than leaving
/// the resolution order to decide it. Revision 0.1 names no spelling: recorded
/// gap.
fn page_prefix(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Global => "",
        SymbolKind::Function => "FUNC",
    }
}

fn volatile_marker(volatile: bool) -> &'static str {
    if volatile {
        " volatile"
    } else {
        ""
    }
}

/// Render one instruction without its indent.
#[must_use]
pub fn render_inst(inst: &MInst) -> String {
    match inst {
        MInst::Mov { dst, src } => format!("{}mov {}", defined(*dst), use_of(*src)),
        MInst::MovImm { dst, imm } => format!("{}mov #{imm}", defined(*dst)),
        MInst::Adrp { dst, symbol, kind } => {
            format!("{}adrp @{symbol}@{}PAGE", defined(*dst), page_prefix(*kind))
        }
        MInst::AddPageOff {
            dst,
            src,
            symbol,
            kind,
        } => format!(
            "{}add {}, @{symbol}@{}PAGEOFF",
            defined(*dst),
            use_of(*src),
            page_prefix(*kind)
        ),
        MInst::Load {
            dst,
            base,
            offset,
            kind,
            volatile,
        } => format!(
            "{}{}{} [{}, #{offset}]",
            defined(*dst),
            kind.mnemonic(),
            volatile_marker(*volatile),
            use_of(*base)
        ),
        MInst::Store {
            src,
            base,
            offset,
            kind,
            volatile,
        } => format!(
            "{}{} {}, [{}, #{offset}]",
            kind.mnemonic(),
            volatile_marker(*volatile),
            use_of(*src),
            use_of(*base)
        ),
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } => format!(
            "{}{}.{} {}, {}",
            defined(*dst),
            op.mnemonic(),
            width.mnemonic(),
            use_of(*lhs),
            use_of(*rhs)
        ),
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        } => format!(
            "{}{}.{} {}, #{imm}",
            defined(*dst),
            op.mnemonic(),
            width.mnemonic(),
            use_of(*lhs)
        ),
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width,
        } => format!(
            "{}msub.{} {}, {}, {}",
            defined(*dst),
            width.mnemonic(),
            use_of(*n),
            use_of(*m),
            use_of(*a)
        ),
        MInst::Extend { op, dst, src } => {
            format!("{}{} {}", defined(*dst), op.mnemonic(), use_of(*src))
        }
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            width,
        } => format!(
            "{}{}.{} {}, #{lsb}, #{bits}",
            defined(*dst),
            if *signed { "sbfx" } else { "ubfx" },
            width.mnemonic(),
            use_of(*src)
        ),
        MInst::Cmp {
            dst,
            lhs,
            rhs,
            width,
        } => format!(
            "{}cmp.{} {}, {}",
            defined(*dst),
            width.mnemonic(),
            use_of(*lhs),
            use_of(*rhs)
        ),
        MInst::CmpImm {
            dst,
            lhs,
            imm,
            width,
        } => format!(
            "{}cmp.{} {}, #{imm}",
            defined(*dst),
            width.mnemonic(),
            use_of(*lhs)
        ),
        MInst::Cset { dst, flags, cond } => format!(
            "{}cset {}, {}",
            defined(*dst),
            use_of(*flags),
            cond.mnemonic()
        ),
        MInst::Csel {
            dst,
            flags,
            lhs,
            rhs,
            cond,
            width,
        } => format!(
            "{}csel.{} {}, {}, {}, {}",
            defined(*dst),
            width.mnemonic(),
            use_of(*flags),
            use_of(*lhs),
            use_of(*rhs),
            cond.mnemonic()
        ),
        MInst::Bl {
            symbol,
            args,
            results: outputs,
        } => format!("bl @{symbol}({}){}", uses(args), results(outputs)),
        MInst::Blr {
            target,
            args,
            results: outputs,
        } => format!(
            "blr {}({}){}",
            use_of(*target),
            uses(args),
            results(outputs)
        ),
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
        } => format!(
            "mem.copy [{}], [{}], #{size}, #{align}",
            use_of(*dst),
            use_of(*src)
        ),
        MInst::FlagBinary {
            op,
            dst,
            flags,
            lhs,
            rhs,
            width,
        } => format!(
            "{dst}, {flags} = {}.{} {}, {}",
            op.mnemonic(),
            width.mnemonic(),
            use_of(*lhs),
            use_of(*rhs)
        ),
        MInst::WideMultiply { op, dst, lhs, rhs } => format!(
            "{}{} {}, {}",
            defined(*dst),
            op.mnemonic(),
            use_of(*lhs),
            use_of(*rhs)
        ),
        MInst::Ldp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
        }
        | MInst::Stp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
        } => {
            let mnemonic = if matches!(inst, MInst::Ldp { .. }) {
                "ldp"
            } else {
                "stp"
            };
            let bang = if *writeback { "!" } else { "" };
            let head = if matches!(inst, MInst::Ldp { .. }) {
                format!("{}, {} = ", *rt1, *rt2)
            } else {
                String::new()
            };
            let operands = if matches!(inst, MInst::Ldp { .. }) {
                String::new()
            } else {
                format!("{}, {}, ", use_of(*rt1), use_of(*rt2))
            };
            format!(
                "{head}{mnemonic} {operands}[{}, #{offset}]{bang}",
                use_of(*base)
            )
        }
        MInst::SpillStore { src, slot } => {
            format!("spill.store {}, [slot #{slot}]", use_of(*src))
        }
        MInst::SpillReload { dst, slot } => {
            format!("{}spill.reload [slot #{slot}]", defined(*dst))
        }
        MInst::FrameSlot {
            dst,
            origin,
            size,
            align,
        } => format!(
            "{}frame.slot [origin={origin}, size={size}, align={align}]",
            defined(*dst)
        ),
    }
}

fn edge(function: &MFunction, target: u32, args: &[VReg]) -> String {
    format!("{}({})", label(function, target), uses(args))
}

/// Render one terminator without its indent.
#[must_use]
pub fn render_term(function: &MFunction, term: &MTerm) -> String {
    match term {
        MTerm::B { target, args } => format!("b {}", edge(function, *target, args)),
        MTerm::BCond {
            cond,
            taken,
            taken_args,
            untaken,
            untaken_args,
            ..
        } => format!(
            "b.{}, {}, {}",
            cond.mnemonic(),
            edge(function, *taken, taken_args),
            edge(function, *untaken, untaken_args)
        ),
        MTerm::Cb {
            nonzero,
            src,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => format!(
            "{} {}, {}, {}",
            if *nonzero { "cbnz" } else { "cbz" },
            use_of(*src),
            edge(function, *taken, taken_args),
            edge(function, *untaken, untaken_args)
        ),
        MTerm::Tb {
            nonzero,
            src,
            bit,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => format!(
            "{} {}, #{bit}, {}, {}",
            if *nonzero { "tbnz" } else { "tbz" },
            use_of(*src),
            edge(function, *taken, taken_args),
            edge(function, *untaken, untaken_args)
        ),
        MTerm::Ret { values } if values.is_empty() => "ret".to_owned(),
        MTerm::Ret { values } => format!("ret {}", uses(values)),
        MTerm::Brk { imm } => format!("brk #{imm}"),
    }
}
