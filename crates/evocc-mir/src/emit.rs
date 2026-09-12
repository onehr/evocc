//! Sections 6 and 8: the frame layout, the prologue and epilogue, and the
//! Darwin arm64 assembly text.
//!
//! Section 8: "The assembly text is the platform assembler's syntax. Functions
//! are emitted in module order, each as `.globl`/`.p2align 2`/label/body;
//! globals as data ... with their EvoIR initial bytes; symbols get the
//! platform prefix (`_` on Darwin)."

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use evocc_evoir::{GlobalObject, GlobalVisibility, Linkage, Module, SymbolAlias};

use crate::{
    linscan::physical, Allocated, AsmBody, AsmConstraint, AsmOperand, AsmValue, LoadKind, MInst,
    MTerm, RegClass, StoreKind, SymbolKind, VReg, WideMul, Width,
};

/// Address materialization selected by the caller's build options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocationModel {
    PositionIndependent,
    Static,
}

/// Where every part of one frame lives, measured in bytes above `sp` after the
/// prologue has lowered it.
///
/// Section 6 lays the frame out "below the saved pair, in order: the
/// callee-saved save area, then one slot per `frame.slot` ..., then the spill
/// area", so the callee-saved words are highest and the spill area lowest.
///
/// Every offset here is taken from the aligned `sp`. Over-aligned frames
/// restore the entry stack pointer through `x29`, since their alignment
/// adjustment depends on the incoming address rather than only frame size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Runtime allocations move sp; fixed slots must then stay relative to x29.
    pub dynamic: bool,
    /// Total bytes the prologue subtracts, a multiple of sixteen.
    pub size: u64,
    /// Required alignment of `sp` after the prologue, at least sixteen.
    pub alignment: u64,
    /// Callee-saved register to its offset above `sp`.
    pub saved: BTreeMap<u8, u64>,
    /// Spill slot index to its offset above `sp`.
    pub spills: BTreeMap<u32, u64>,
    /// Each `frame.slot` instruction, in the order Section 6 lays them out, to
    /// its offset above `sp`.
    pub slots: BTreeMap<u64, u64>,
    /// Bytes of outgoing stack arguments, which Section 6 puts at the bottom
    /// of the frame so that a call writes them at `[sp, #8 * i]`.
    pub outgoing: u64,
    /// A variadic callee's register save area: the number of argument
    /// registers its named parameters consumed, and the area's offset above
    /// `sp`.
    ///
    /// The 64-byte general-register area is followed by the 128-byte vector
    /// area. Both sit above outgoing arguments and below every spill.
    pub register_save: Option<(u8, u64)>,
}

/// Section 6: the area is "64-byte, 16-byte-aligned".
const REGISTER_SAVE_BYTES: u64 = 64;
const VECTOR_SAVE_BYTES: u64 = 128;

const fn align_up(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        value
    } else {
        value.div_ceil(alignment) * alignment
    }
}

/// Compute the Section 6 layout of one allocated function.
#[must_use]
pub fn frame_of(allocated: &Allocated) -> Frame {
    let mut automatic: Vec<(u64, u64, u64)> = Vec::new();
    for block in &allocated.blocks {
        for inst in &block.insts {
            if let MInst::FrameSlot {
                origin,
                size,
                align,
                ..
            } = inst
            {
                automatic.push((*origin, *size, u64::from(*align)));
            }
        }
    }
    automatic.sort_unstable();
    automatic.dedup();
    let alignment = automatic
        .iter()
        .map(|(_, _, align)| *align)
        .max()
        .unwrap_or(16)
        .max(16);

    let spill_bytes = u64::from(allocated.frame_slots) * 8;

    // Section 6's frame layout puts "outgoing stack arguments (Linux only,
    // revision 0.10)" last, which is lowest, so every other slot sits above
    // the area and a call writes its arguments at `[sp, #8 * i]`.
    let outgoing = allocated.outgoing;

    // Section 6's two save areas are adjacent and sixteen-byte aligned.
    let named = allocated.blocks.iter().find_map(|block| {
        block.insts.iter().find_map(|inst| match inst {
            MInst::VaSaveArea { named } => Some(*named),
            _ => None,
        })
    });
    let register_save = named.map(|named| (named, align_up(outgoing, 16)));
    let above_save = register_save.map_or(outgoing, |(_, base)| {
        base + REGISTER_SAVE_BYTES + VECTOR_SAVE_BYTES
    });

    let mut spills = BTreeMap::new();
    for index in 0..allocated.frame_slots {
        spills.insert(index, above_save + u64::from(index) * 8);
    }

    // "one slot per `frame.slot` (its declared size, rounded to its declared
    // alignment, laid out in origin order from the top of the frame
    // downward)": walking the origins in reverse from the bottom of the area
    // upward puts the lowest origin highest, which is that order, and gives
    // each object an offset its own alignment divides. An object placed by
    // subtracting its rounded size from the top instead would land wherever
    // the object above it left the cursor, which is what the campaign's first
    // assembler refusal was.
    let mut slots = BTreeMap::new();
    let mut cursor = above_save + spill_bytes;
    for (origin, object, align) in automatic.iter().rev() {
        cursor = align_up(cursor, (*align).max(1));
        slots.insert(*origin, cursor);
        cursor += *object;
    }

    // Section 6 addresses "every slot (object, spill, save area) ... from `sp`
    // with a non-negative scaled offset", and an eight-byte transfer scales by
    // eight, so the save area starts on an eight-byte boundary whatever the
    // objects below it needed.
    let saved_base = align_up(cursor, 8);
    let saved_bytes = allocated.used_callee_saved.len() as u64 * 8;
    let size = align_up(saved_base + saved_bytes, alignment) + alignment - 16;

    let mut saved = BTreeMap::new();
    for (index, number) in allocated.used_callee_saved.iter().enumerate() {
        saved.insert(*number, saved_base + index as u64 * 8);
    }

    Frame {
        dynamic: allocated.blocks.iter().any(|block| {
            block
                .insts
                .iter()
                .any(|inst| matches!(inst, MInst::StackAlloc { .. }))
        }),
        size,
        alignment,
        saved,
        spills,
        slots,
        outgoing,
        register_save,
    }
}

/// The platform an emission targets.
///
/// Section 1: the AArch64 scalar subset is "first hosted as Darwin arm64 ...
/// and then as Linux AArch64 for the kernel. The two targets share every rule
/// below except the ones the calling convention and the symbol syntax name."
/// This is those ones: the symbol prefix, the page-relative spelling, the
/// section names and the local-label prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Target {
    #[default]
    Darwin,
    Linux,
}

impl Target {
    /// Section 8: "symbols get the platform prefix (`_` on Darwin)".
    #[must_use]
    pub const fn symbol_prefix(self) -> &'static str {
        match self {
            Self::Darwin => "_",
            Self::Linux => "",
        }
    }

    /// A label the assembler keeps out of the symbol table.
    const fn label_prefix(self) -> &'static str {
        match self {
            Self::Darwin => "L",
            Self::Linux => ".L",
        }
    }

    /// The producer note this backend leaves in the object.
    ///
    /// ELF keeps `.ident` strings in a `.comment` section that survives into
    /// a linked image, so an image can be asked which compiler produced the
    /// code inside it. Mach-O has no counterpart, so Darwin emits none.
    const fn producer_ident(self) -> Option<&'static str> {
        match self {
            Self::Darwin => None,
            Self::Linux => Some(concat!(
                ".ident \"EvoCC ",
                env!("CARGO_PKG_VERSION"),
                " aarch64-unknown-linux-gnu\""
            )),
        }
    }

    const fn text_section(self) -> &'static str {
        match self {
            Self::Darwin => ".section __TEXT,__text,regular,pure_instructions",
            Self::Linux => ".text",
        }
    }

    const fn data_section(self) -> &'static str {
        match self {
            Self::Darwin => ".section __DATA,__data",
            Self::Linux => ".data",
        }
    }

    /// Section 8's third data section: the one whose contents an object file
    /// does not store, only reserves.
    ///
    /// ELF spells it `.bss`. Mach-O has no directive of that name; the
    /// zero-filled section of `__DATA` is named the same way any other section
    /// is, with the `zerofill` attribute that tells the assembler to reserve
    /// the space instead of writing it.
    const fn bss_section(self) -> &'static str {
        match self {
            Self::Darwin => ".section __DATA,__bss,zerofill",
            Self::Linux => ".bss",
        }
    }

    /// The name a target is selected by.
    #[must_use]
    pub const fn mnemonic(self) -> &'static str {
        match self {
            Self::Darwin => "darwin-arm64",
            Self::Linux => "linux-aarch64",
        }
    }

    /// Read a target name.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "darwin-arm64" | "darwin" => Some(Self::Darwin),
            "linux-aarch64" | "linux" => Some(Self::Linux),
            _ => None,
        }
    }

    /// Section 4: the page-relative pair, "`@f@FUNCPAGE` / `@f@FUNCPAGEOFF`
    /// for a function symbol ... (Darwin spelling; Linux uses `:pg_hi21:` and
    /// `:lo12:`)".
    fn adrp_operand(self, symbol: &str, page: &str) -> String {
        match self {
            Self::Darwin => format!("{}{symbol}@{page}", self.symbol_prefix()),
            Self::Linux => format!(":pg_hi21:{symbol}"),
        }
    }

    fn pageoff_operand(self, symbol: &str) -> String {
        match self {
            Self::Darwin => format!("{}{symbol}@PAGEOFF", self.symbol_prefix()),
            Self::Linux => format!(":lo12:{symbol}"),
        }
    }
}

/// Emit one module as Darwin arm64 assembly.
#[must_use]
pub fn emit_module(functions: &[Allocated], globals: &[GlobalObject]) -> String {
    emit_module_for(Target::Darwin, functions, globals)
}

/// Emit one module for `target`, with no module assembly table.
#[must_use]
pub fn emit_module_for(
    target: Target,
    functions: &[Allocated],
    globals: &[GlobalObject],
) -> String {
    emit_module_unit(target, functions, globals, &[])
}

/// Emit one whole unit for `target`: its functions, its globals and its module
/// assembly table.
///
/// Section 8 (revision 0.14): "Every entry of the EvoIR module assembly table
/// ... is emitted verbatim, in table order, after the last function and the
/// last global of the unit. Emission does not parse an entry and substitutes
/// nothing into it; it precedes the first entry with a `.text` directive and
/// follows the last with one, so an entry that switches sections cannot change
/// where a later revision's trailing output lands." The directive written here
/// is the target's own text-section spelling from Section 8's table, which is
/// what returns a Mach-O assembly to `__TEXT,__text` after an entry has moved
/// it; a bare `.text` would name the wrong section there.
#[must_use]
pub fn emit_module_unit(
    target: Target,
    functions: &[Allocated],
    globals: &[GlobalObject],
    assembly: &[String],
) -> String {
    emit_unit(
        target,
        functions,
        globals,
        assembly,
        None,
        RelocationModel::PositionIndependent,
    )
}

pub(crate) fn emit_linked_module(
    target: Target,
    functions: &[Allocated],
    module: &Module,
    relocation: RelocationModel,
) -> String {
    emit_unit(
        target,
        functions,
        &module.globals,
        &module.assembly,
        Some(module),
        relocation,
    )
}

fn emit_unit(
    target: Target,
    functions: &[Allocated],
    globals: &[GlobalObject],
    assembly: &[String],
    module: Option<&Module>,
    relocation: RelocationModel,
) -> String {
    let prefix = target.symbol_prefix();
    let mut text = String::new();
    text.push_str(target.text_section());
    text.push('\n');
    let mut aliases: BTreeMap<&str, Vec<&SymbolAlias>> = BTreeMap::new();
    let mut weak_functions = BTreeSet::new();
    let mut indirect_addresses = BTreeSet::new();
    if let Some(module) = module {
        for name in module
            .functions
            .iter()
            .filter(|f| f.visibility == GlobalVisibility::Hidden)
            .map(|f| f.name.as_str())
            .chain(
                module
                    .declares
                    .iter()
                    .filter(|f| f.visibility == GlobalVisibility::Hidden)
                    .map(|f| f.name.as_str()),
            )
            .chain(
                module
                    .globals
                    .iter()
                    .filter(|g| g.visibility == GlobalVisibility::Hidden)
                    .map(|g| g.symbol.as_str()),
            )
            .chain(
                module
                    .extern_objects
                    .iter()
                    .filter(|g| g.visibility == GlobalVisibility::Hidden)
                    .map(|g| g.symbol.as_str()),
            )
            .chain(
                module
                    .aliases
                    .iter()
                    .filter(|a| a.visibility == GlobalVisibility::Hidden)
                    .map(|a| a.name.as_str()),
            )
        {
            let directive = if target == Target::Linux {
                ".hidden"
            } else {
                ".private_extern"
            };
            let _ = writeln!(text, "{directive} {prefix}{name}");
        }
        for alias in &module.aliases {
            aliases.entry(&alias.target).or_default().push(alias);
        }
        weak_functions.extend(
            module
                .functions
                .iter()
                .filter(|f| f.linkage == Linkage::Weak)
                .map(|f| f.name.as_str()),
        );
        for name in module
            .extern_objects
            .iter()
            .filter(|g| g.linkage == Linkage::Weak)
            .map(|g| g.symbol.as_str())
            .chain(
                module
                    .declares
                    .iter()
                    .filter(|f| f.linkage == Linkage::Weak)
                    .map(|f| f.name.as_str()),
            )
        {
            let directive = if target == Target::Linux {
                ".weak"
            } else {
                ".weak_reference"
            };
            let _ = writeln!(text, "{directive} {prefix}{name}");
        }
        indirect_addresses.extend(
            module
                .extern_objects
                .iter()
                .filter(|g| {
                    g.linkage == Linkage::Weak
                        || (relocation == RelocationModel::PositionIndependent
                            && g.visibility != GlobalVisibility::Hidden)
                })
                .map(|g| (SymbolKind::Global, g.symbol.as_str()))
                .chain(
                    module
                        .declares
                        .iter()
                        .filter(|f| {
                            f.linkage == Linkage::Weak
                                || (relocation == RelocationModel::PositionIndependent
                                    && f.visibility != GlobalVisibility::Hidden)
                        })
                        .map(|f| (SymbolKind::Function, f.name.as_str())),
                ),
        );
    }
    let mut static_blocks: BTreeMap<&str, BTreeSet<u32>> = BTreeMap::new();
    for relocation in globals.iter().flat_map(|global| &global.relocs) {
        if let Some((function, block)) = relocation_block(&relocation.symbol) {
            static_blocks.entry(function).or_default().insert(block);
        }
    }
    let alignments: BTreeMap<_, _> = module
        .into_iter()
        .flat_map(|module| &module.functions)
        .map(|function| (function.name.as_str(), function.alignment))
        .collect();
    for function in functions {
        let alignment = alignments.get(function.name.as_str()).copied().unwrap_or(4);
        emit_function(
            &mut text,
            target,
            function,
            static_blocks.get(function.name.as_str()),
            weak_functions.contains(function.name.as_str()),
            &indirect_addresses,
            alignment,
        );
        emit_aliases(
            &mut text,
            target,
            aliases.get(function.name.as_str()),
            "function",
        );
    }
    if !globals.is_empty() {
        let mut ordered: Vec<&GlobalObject> = globals.iter().collect();
        ordered.sort_by_key(|global| global.origin);
        // A global names its section only when the section changes, so a unit
        // whose globals all hold content emits the one `.data` it always did.
        let mut section = String::new();
        for global in ordered {
            let zero_filled = is_zero_filled(global);
            let wanted = if let Some(name) = &global.section {
                match target {
                    Target::Linux => format!(
                        ".section {name:?},\"aw\",%{}",
                        if global.has_bss_section() {
                            "nobits"
                        } else {
                            "progbits"
                        }
                    ),
                    Target::Darwin => match name.split_once(',') {
                        Some((segment, name)) => format!(".section {segment:?},{name:?}"),
                        None => format!(".section {name:?}"),
                    },
                }
            } else if zero_filled {
                target.bss_section().to_owned()
            } else {
                target.data_section().to_owned()
            };
            if wanted != section {
                let _ = writeln!(text, "\n{wanted}");
                section = wanted;
            }
            let _ = writeln!(
                text,
                ".p2align {}",
                global.layout.align_bytes.trailing_zeros()
            );
            // Section 8 (revision 0.16): an internal object's symbol is local
            // to the object file, so it is defined and never declared global.
            if global.visibility != GlobalVisibility::Internal {
                emit_definition_linkage(
                    &mut text,
                    target,
                    &global.symbol,
                    global.linkage == Linkage::Weak,
                );
            }
            if target == Target::Linux {
                let _ = writeln!(text, ".type {}, %object", global.symbol);
            }
            let _ = writeln!(text, "{prefix}{}:", global.symbol);
            emit_initializer(&mut text, target, global, zero_filled);
            if target == Target::Linux {
                let _ = writeln!(text, ".size {0}, .-{0}", global.symbol);
            }
            emit_aliases(
                &mut text,
                target,
                aliases.get(global.symbol.as_str()),
                "object",
            );
        }
        // Leave the assembler where it was before the globals: the trailing
        // output of Section 8 belongs to the text section, and a unit whose
        // last global was zero-filled would otherwise leave it in `.bss`.
        if section == target.bss_section() {
            let _ = writeln!(text, "\n{}", target.text_section());
        }
    }
    if !assembly.is_empty() {
        let _ = writeln!(text, "\n{}", target.text_section());
        for entry in assembly {
            text.push_str(entry);
            if !entry.ends_with('\n') {
                text.push('\n');
            }
        }
        let _ = writeln!(text, "{}", target.text_section());
    }
    if let Some(ident) = target.producer_ident() {
        let _ = writeln!(text, "\n{ident}");
    }
    text
}

fn emit_definition_linkage(text: &mut String, target: Target, name: &str, weak: bool) {
    let prefix = target.symbol_prefix();
    if weak && target == Target::Linux {
        let _ = writeln!(text, ".weak {name}");
    } else {
        let _ = writeln!(text, ".globl {prefix}{name}");
        if weak {
            let _ = writeln!(text, ".weak_definition {prefix}{name}");
        }
    }
}

fn emit_aliases(
    text: &mut String,
    target: Target,
    aliases: Option<&Vec<&SymbolAlias>>,
    kind: &str,
) {
    let prefix = target.symbol_prefix();
    for alias in aliases.into_iter().flatten() {
        if alias.visibility != GlobalVisibility::Internal {
            emit_definition_linkage(text, target, &alias.name, alias.linkage == Linkage::Weak);
        }
        if target == Target::Linux {
            let _ = writeln!(text, ".type {}, %{kind}", alias.name);
        }
        let _ = writeln!(
            text,
            ".set {prefix}{}, {prefix}{}",
            alias.name, alias.target
        );
    }
}

/// One global's initial bytes, with the relocations of EvoIR 2.1 written into
/// them.
///
/// Section 10, `E4005` withdrawn at revision 0.12: "a relocation `(offset,
/// @symbol, addend)` is emitted as `.xword symbol+addend` at its offset, the
/// surrounding bytes as before". EvoIR sorts the list by offset, forbids
/// overlap and keeps every entry inside the size, so one walk suffices.
///
/// The bytes around the relocations are a run-length encoding, because the
/// alternative is a line per byte and a benchmark with a megabyte of static
/// zeros then costs a megabyte of lines and tens of seconds of assembly. A run
/// of zeros at least a line long is one `.zero N`; every other byte joins a
/// `.byte` list of at most a line's worth. A byte whose `initialized=` bit is
/// clear is emitted as the zero EvoIR stores for it, as it was before: the
/// object file has no way to spell an uninitialized byte, and the EvoIR
/// interpreter, not the image, is what diagnoses a read of one.
fn emit_initializer(text: &mut String, target: Target, global: &GlobalObject, zero_filled: bool) {
    if global.bytes.is_empty() {
        let _ = writeln!(text, "    .space {}", global.layout.size_bytes);
        return;
    }
    if zero_filled {
        let _ = writeln!(text, "    .zero {}", global.bytes.len());
        return;
    }
    let mut pending: Vec<u8> = Vec::new();
    let mut offset = 0_usize;
    let mut relocations = global.relocs.iter().peekable();
    while offset < global.bytes.len() {
        if let Some(relocation) = relocations.peek() {
            if relocation.offset == offset as u64 {
                flush_bytes(text, &mut pending);
                let addend = relocation.addend;
                let sign = if addend < 0 { '-' } else { '+' };
                let magnitude = addend.unsigned_abs();
                let symbol = relocation_block(&relocation.symbol).map_or_else(
                    || format!("{}{}", target.symbol_prefix(), relocation.symbol),
                    |(function, block)| block_symbol(target, function, block),
                );
                if addend == 0 {
                    let _ = writeln!(text, "    .xword {symbol}");
                } else {
                    let _ = writeln!(text, "    .xword {symbol}{sign}{magnitude}");
                }
                offset += 8;
                relocations.next();
                continue;
            }
        }
        // A run of zeros may not swallow the offset of the next relocation:
        // the relocated word replaces its bytes and nothing else.
        let limit = relocations
            .peek()
            .map_or(global.bytes.len(), |relocation| {
                usize::try_from(relocation.offset).unwrap_or(usize::MAX)
            })
            .min(global.bytes.len())
            .max(offset);
        let run = global.bytes[offset..limit]
            .iter()
            .take_while(|byte| **byte == 0)
            .count();
        if run >= BYTES_PER_LINE {
            flush_bytes(text, &mut pending);
            let _ = writeln!(text, "    .zero {run}");
            offset += run;
            continue;
        }
        pending.push(global.bytes[offset]);
        offset += 1;
    }
    flush_bytes(text, &mut pending);
}

/// How many bytes one `.byte` directive carries.
///
/// It is also the shortest run of zeros worth collapsing: a run this long or
/// longer costs one line as `.zero N` and would have cost at least one line as
/// bytes, so the encoding never makes the text longer than the byte list it
/// replaces, and a run shorter than a line rides along inside one.
const BYTES_PER_LINE: usize = 16;

/// Write the bytes gathered so far as `.byte` lists, at most a line's worth
/// each, and empty the buffer.
fn flush_bytes(text: &mut String, pending: &mut Vec<u8>) {
    for chunk in pending.chunks(BYTES_PER_LINE) {
        let _ = write!(text, "    .byte {}", chunk[0]);
        for byte in &chunk[1..] {
            let _ = write!(text, ", {byte}");
        }
        text.push('\n');
    }
    pending.clear();
}

/// Whether a global reserves space rather than storing bytes.
///
/// Every byte is zero, EvoIR 2.1's `initialized=` bits are all set so no byte
/// is a poisoned value some later revision might have to spell differently,
/// and no relocation writes an address into the object. Such a global has the
/// same content wherever the loader's zero page puts it, so it belongs in the
/// section that reserves space without storing it. A global with no bytes at
/// all is the same case: it reserves its size and stores nothing.
fn is_zero_filled(global: &GlobalObject) -> bool {
    global.relocs.is_empty()
        && (global.bytes.is_empty()
            || (global.bytes.len() as u64 == global.layout.size_bytes
                && global.initialized.len() == global.bytes.len()
                && global.bytes.iter().all(|byte| *byte == 0)
                && global.initialized.iter().all(|bit| *bit)))
}

fn label(target: Target, function: &str, block: u32) -> String {
    format!("{}{function}_block{block}", target.label_prefix())
}

/// Section 8 leaves "the details of instruction selection" to emission, and a
/// branch it selects has to reach its target.
///
/// `tbz`/`tbnz` encode a fourteen-bit signed word displacement, which is 8,192
/// instructions either way, and `b.cond` a nineteen-bit one. A function longer
/// than the tighter reach can place a conditional terminator's target past it,
/// and the assembler answers `conditional branch out of range`. Emission
/// therefore counts the instructions a function emits with the short forms,
/// and, at or above this threshold, emits every conditional terminator as the
/// inverted short branch over an unconditional `b`, whose twenty-six-bit
/// displacement reaches 128 mebibytes.
///
/// The threshold is half the `tbz` reach. The margin covers the two ways the
/// count can be below the truth: an inline-assembly template line standing for
/// more than one instruction, and the growth relaxation itself adds.
const RELAXATION_THRESHOLD: usize = 4_096;

/// The local symbol a `block_addr` names.
///
/// Section 5, revision 0.15: "a static relocation against a block uses a local
/// symbol `.L<function>.<block>` the emitter places at the block's first
/// instruction". The prefix is the platform's own local one, `L` on Darwin and
/// `.L` on Linux, so the symbol stays out of the symbol table on both.
fn block_symbol(target: Target, function: &str, block: u32) -> String {
    format!("{}{function}.{block}", target.label_prefix())
}

/// The validator has already checked the canonical function/block target.
fn relocation_block(symbol: &str) -> Option<(&str, u32)> {
    let (function, block) = symbol.split_once("^b")?;
    Some((function, block.parse().ok()?))
}

/// The label the inverted short branch of a relaxed terminator jumps over.
fn relax_label(target: Target, function: &str, index: u32) -> String {
    format!("{}{function}_relax{index}", target.label_prefix())
}

/// The instructions in an emitted function body.
///
/// Every instruction emission writes is one line indented by four spaces;
/// labels and directives start at the first column.
fn instructions_in(body: &str) -> usize {
    body.lines().filter(|line| line.starts_with(' ')).count()
}

/// Undefined symbols need GOT indirection for PIE linking, including strong
/// function addresses and the null address of an absent weak definition.
/// The address slot is linker storage, not an abstract C memory operation.
struct SymbolEmission<'a> {
    target: Target,
    indirect_addresses: &'a BTreeSet<(SymbolKind, &'a str)>,
}

fn emit_function(
    text: &mut String,
    target: Target,
    allocated: &Allocated,
    static_blocks: Option<&BTreeSet<u32>>,
    weak: bool,
    indirect_addresses: &BTreeSet<(SymbolKind, &str)>,
    alignment: u32,
) {
    let frame = frame_of(allocated);
    let prefix = target.symbol_prefix();
    // Section 8 (revision 0.16): "An EvoIR function or global whose
    // `visibility` is `internal` is emitted without `.globl`, so its symbol is
    // local to the object." The blank line, the alignment and the label are
    // unchanged, so a module with no internal function emits the same bytes as
    // before the revision.
    if allocated.internal {
        text.push('\n');
    } else {
        text.push('\n');
        emit_definition_linkage(text, target, &allocated.name, weak);
    }
    let _ = writeln!(text, ".p2align {}", alignment.trailing_zeros());
    if target == Target::Linux {
        let _ = writeln!(text, ".type {}, %function", allocated.name);
    }
    let _ = writeln!(text, "{prefix}{}:", allocated.name);
    emit_prologue(text, &frame);

    // Appendix A's width suffix, not the operand's declared class, decides how
    // an operand of a two-width form is spelled: the assembler admits only one
    // width per instruction, and Section 4's semantics are the suffix's.
    let spell = |reg: VReg, class: RegClass| -> String {
        allocated.assignment.get(&reg.id).map_or_else(
            || {
                if class == RegClass::Gpr32 {
                    "wzr"
                } else {
                    "xzr"
                }
                .to_owned()
            },
            |number| physical(class, *number),
        )
    };

    // The short conditional forms are tried first, and their instruction count
    // decides whether they reach: a body at or above the threshold is emitted
    // again with every conditional terminator relaxed.
    let mut body = String::new();
    emit_blocks(
        &mut body,
        &SymbolEmission {
            target,
            indirect_addresses,
        },
        allocated,
        &frame,
        &spell,
        false,
        static_blocks,
    );
    if instructions_in(&body) >= RELAXATION_THRESHOLD {
        body.clear();
        emit_blocks(
            &mut body,
            &SymbolEmission {
                target,
                indirect_addresses,
            },
            allocated,
            &frame,
            &spell,
            true,
            static_blocks,
        );
    }
    text.push_str(&body);
    if target == Target::Linux {
        let _ = writeln!(text, ".size {0}, .-{0}", allocated.name);
    }
}

fn emit_blocks(
    text: &mut String,
    symbols: &SymbolEmission<'_>,
    allocated: &Allocated,
    frame: &Frame,
    spell: &impl Fn(VReg, RegClass) -> String,
    relax: bool,
    static_blocks: Option<&BTreeSet<u32>>,
) {
    let target = symbols.target;
    let order: Vec<u32> = allocated.blocks.iter().map(|block| block.id).collect();
    // An adr or static relocation can take a block's address. Both name the
    // same local symbol at the first instruction; unaddressed blocks retain
    // only their branch label.
    let addressed: BTreeSet<u32> = allocated
        .blocks
        .iter()
        .flat_map(|block| block.insts.iter())
        .filter_map(|inst| match inst {
            MInst::Adr { block, .. } => Some(*block),
            _ => None,
        })
        .collect();
    let mut copies = 0_u32;
    let mut relaxations = 0_u32;
    for (index, block) in allocated.blocks.iter().enumerate() {
        let _ = writeln!(text, "{}:", label(target, &allocated.name, block.id));
        if addressed.contains(&block.id)
            || static_blocks.is_some_and(|blocks| blocks.contains(&block.id))
        {
            let _ = writeln!(text, "{}:", block_symbol(target, &allocated.name, block.id));
        }
        for inst in &block.insts {
            emit_inst(
                text,
                symbols,
                inst,
                frame,
                spell,
                &allocated.name,
                &mut copies,
            );
        }
        let next = order.get(index + 1).copied();
        emit_term(
            text,
            target,
            &allocated.name,
            &block.term,
            frame,
            next,
            spell,
            relax,
            &mut relaxations,
        );
    }
}

fn emit_prologue(text: &mut String, frame: &Frame) {
    let _ = writeln!(text, "    stp x29, x30, [sp, #-16]!");
    let _ = writeln!(text, "    mov x29, sp");
    if frame.size != 0 {
        emit_add_immediate(text, "sub", "sp", "sp", frame.size);
    }
    if frame.alignment > 16 {
        let _ = writeln!(text, "    mov x16, sp");
        let _ = writeln!(text, "    and x16, x16, #-{}", frame.alignment);
        let _ = writeln!(text, "    mov sp, x16");
    }
    for (number, offset) in &frame.saved {
        emit_frame_access(text, "str", &format!("x{number}"), *offset, frame);
    }
    // Section 6: a variadic callee "stores `x_k` to `x7` into it at offsets
    // `8 * k` to `56` in the prologue". Nothing before this point writes an
    // argument register, so the incoming anonymous arguments are still there.
    if let Some((named, base)) = frame.register_save {
        for number in named..8 {
            emit_frame_access(
                text,
                "str",
                &format!("x{number}"),
                base + u64::from(number) * 8,
                frame,
            );
        }
        // Reference-built callers may pass anonymous values in q0-q7 even
        // when this function only forwards the list. Use the reserved
        // scratch register so large outgoing areas cannot overflow stp's
        // signed, scaled offset; no argument register is clobbered.
        emit_add_immediate(text, "add", "x16", "sp", base + REGISTER_SAVE_BYTES);
        for number in (0..8).step_by(2) {
            let _ = writeln!(
                text,
                "    stp q{number}, q{}, [x16, #{}]",
                number + 1,
                number * 16
            );
        }
    }
}

fn emit_epilogue(text: &mut String, frame: &Frame) {
    for (number, offset) in &frame.saved {
        emit_frame_access(text, "ldr", &format!("x{number}"), *offset, frame);
    }
    if frame.alignment > 16 || frame.dynamic {
        let _ = writeln!(text, "    mov sp, x29");
    } else if frame.size != 0 {
        emit_add_immediate(text, "add", "sp", "sp", frame.size);
    }
    let _ = writeln!(text, "    ldp x29, x30, [sp], #16");
}

/// The `movz`/`movk` sequence Section 4 names for `mov rd, #imm`.
fn emit_immediate(text: &mut String, register: &str, class: RegClass, value: u64) {
    let chunks = if class == RegClass::Gpr64 { 4 } else { 2 };
    let mut written = false;
    for index in 0..chunks {
        let part = (value >> (16 * index)) & 0xffff;
        if part == 0 && written {
            continue;
        }
        if written {
            let _ = writeln!(text, "    movk {register}, #{part}, lsl #{}", 16 * index);
        } else {
            if index == 0 {
                let _ = writeln!(text, "    movz {register}, #{part}");
            } else {
                let _ = writeln!(text, "    movz {register}, #{part}, lsl #{}", 16 * index);
            }
            written = true;
        }
    }
    if !written {
        let _ = writeln!(text, "    movz {register}, #0");
    }
}

/// The largest immediate an `add` or `sub` carries in one instruction, and the
/// largest one it carries in two: the second form shifts its twelve bits left
/// by twelve.
const IMM12: u64 = 4095;
const IMM12_SHIFTED: u64 = 4095 << 12 | 4095;

/// The largest frame offset a scaled transfer immediate reaches for a
/// thirty-two-bit access. Every frame offset this revision produces is a
/// multiple of eight, so one bound serves both widths.
const SCALED_LIMIT: u64 = 4095 * 4;

/// `dst := src + imm`, in as many instructions as the immediate needs.
///
/// Section 6 puts every slot "from `sp` with a non-negative scaled offset" and
/// keeps the frame inside "the unsigned twelve-bit immediate form", which a
/// frame with a large automatic object leaves. The platform's own answer is the
/// shifted form, and below it a materialized constant; the sequence is
/// emission's, as Section 4's pseudo-form expansions are.
fn emit_add_immediate(text: &mut String, op: &str, dst: &str, src: &str, imm: u64) {
    if imm <= IMM12 {
        let _ = writeln!(text, "    {op} {dst}, {src}, #{imm}");
    } else if imm <= IMM12_SHIFTED {
        let high = imm >> 12;
        let low = imm & IMM12;
        let _ = writeln!(text, "    {op} {dst}, {src}, #{high}, lsl #12");
        if low != 0 {
            let _ = writeln!(text, "    {op} {dst}, {dst}, #{low}");
        }
    } else {
        let scratch = physical(
            RegClass::Gpr64,
            if src == "x16" {
                SCRATCH_UNIT
            } else {
                SCRATCH_OFFSET
            },
        );
        emit_immediate(text, &scratch, RegClass::Gpr64, imm);
        let _ = writeln!(text, "    {op} {dst}, {src}, {scratch}");
    }
}

/// One transfer to or from a frame slot, at whatever distance from `sp`.
fn emit_frame_address(text: &mut String, destination: &str, offset: u64, frame: &Frame) {
    if !frame.dynamic {
        emit_add_immediate(text, "add", destination, "sp", offset);
    } else if frame.alignment == 16 {
        emit_add_immediate(text, "sub", destination, "x29", frame.size - offset);
    } else {
        emit_add_immediate(text, "sub", destination, "x29", frame.size);
        let _ = writeln!(
            text,
            "    and {destination}, {destination}, #-{}",
            frame.alignment
        );
        emit_add_immediate(text, "add", destination, destination, offset);
    }
}

fn emit_frame_access(
    text: &mut String,
    mnemonic: &str,
    register: &str,
    offset: u64,
    frame: &Frame,
) {
    if offset <= SCALED_LIMIT && !frame.dynamic {
        let _ = writeln!(text, "    {mnemonic} {register}, [sp, #{offset}]");
        return;
    }
    let scratch = physical(RegClass::Gpr64, SCRATCH_OFFSET);
    emit_frame_address(text, &scratch, offset, frame);
    let _ = writeln!(text, "    {mnemonic} {register}, [{scratch}, #0]");
}

/// Section 8: "An `asm` is emitted as its template with every operand
/// reference replaced: `%0` to `%9` and `%[name]` by the operand's register in
/// its natural width, `%w0` and `%x0` by the 32-bit and 64-bit spelling of that
/// register, a `Q` operand by `[reg]`, an immediate operand by its value,
/// `%l[label]` by the block label, and `%%` by one percent. The template's own
/// text is copied verbatim and never parsed."
fn emit_assembly(
    text: &mut String,
    target: Target,
    body: &AsmBody,
    labels: &[(u32, Vec<VReg>)],
    function: &str,
    spell: &impl Fn(VReg, RegClass) -> String,
) {
    let operands = body.numbered();
    // The reference is resolved against the operand it names, at the width the
    // modifier asks for or the operand's own.
    let render = |operand: &AsmOperand, width: Option<RegClass>, address: bool| -> String {
        match &operand.value {
            AsmValue::Imm(value) => format!("{value}"),
            AsmValue::Symbol { name, addend } => {
                let symbol = relocation_block(name).map_or_else(
                    || format!("{}{name}", target.symbol_prefix()),
                    |(function, block)| block_symbol(target, function, block),
                );
                if *addend == 0 {
                    symbol
                } else {
                    format!("{symbol}{addend:+}")
                }
            }
            AsmValue::Zero(class) => {
                let class = width.unwrap_or(*class);
                if class == RegClass::Gpr32 {
                    "wzr"
                } else {
                    "xzr"
                }
                .to_owned()
            }
            AsmValue::Reg(reg) => {
                let spelled = spell(*reg, width.unwrap_or(reg.class));
                if address || matches!(operand.constraint, AsmConstraint::Memory { .. }) {
                    format!("[{spelled}]")
                } else {
                    spelled
                }
            }
        }
    };

    let mut rendered = String::new();
    let characters: Vec<char> = body.template.chars().collect();
    let mut index = 0;
    while index < characters.len() {
        if characters[index] != '%' {
            rendered.push(characters[index]);
            index += 1;
            continue;
        }
        index += 1;
        if index >= characters.len() {
            rendered.push('%');
            break;
        }
        // `%%` is one percent.
        if characters[index] == '%' {
            rendered.push('%');
            index += 1;
            continue;
        }
        // `%l[label]` names one of the `asm.goto` successors, in the order the
        // labels were declared.
        if characters[index] == 'l' && characters.get(index + 1) == Some(&'[') {
            let close = characters[index + 2..]
                .iter()
                .position(|c| *c == ']')
                .map(|at| index + 2 + at);
            if let Some(close) = close {
                let name: String = characters[index + 2..close].iter().collect();
                // Revision 0.8: `%l[...]` holds the label's position in the
                // declared list, counting from zero.
                let successor = name
                    .parse::<usize>()
                    .ok()
                    .or_else(|| {
                        name.strip_prefix('b')
                            .and_then(|digits| digits.parse::<usize>().ok())
                    })
                    .and_then(|position| labels.get(position))
                    .map(|(block, _)| *block);
                if let Some(block) = successor {
                    rendered.push_str(&label(target, function, block));
                    index = close + 1;
                    continue;
                }
            }
        }
        // Address operands print a bracketed full-width register.
        let address = characters[index] == 'a';
        let width = match characters[index] {
            'w' => Some(RegClass::Gpr32),
            'x' | 'a' => Some(RegClass::Gpr64),
            _ => None,
        };
        let at = if width.is_some() || characters[index] == 'c' {
            index + 1
        } else {
            index
        };
        if at >= characters.len() {
            rendered.push('%');
            rendered.push(characters[index]);
            index += 1;
            continue;
        }
        // `%[name]`.
        if characters[at] == '[' {
            let close = characters[at + 1..]
                .iter()
                .position(|c| *c == ']')
                .map(|offset| at + 1 + offset);
            if let Some(close) = close {
                let name: String = characters[at + 1..close].iter().collect();
                if let Some(operand) = operands
                    .iter()
                    .find(|operand| operand.name.as_deref() == Some(name.as_str()))
                {
                    rendered.push_str(&render(operand, width, address));
                    index = close + 1;
                    continue;
                }
            }
        }
        // `%0` to `%9`.
        if let Some(digit) = characters[at].to_digit(10) {
            if let Some(operand) = operands.get(digit as usize) {
                rendered.push_str(&render(operand, width, address));
                index = at + 1;
                continue;
            }
        }
        // Anything else is not a reference this revision admits; EvoIR has
        // already refused it, so the character is copied as it stands.
        rendered.push('%');
        rendered.push(characters[index]);
        index += 1;
    }

    for line in rendered.lines() {
        let _ = writeln!(text, "    {line}");
    }
}

fn scratch(class: RegClass) -> String {
    physical(class, SCRATCH_OFFSET)
}

/// Section 2's two intra-procedure-call scratch registers, "used only by
/// emitted sequences". The unrolled copy needs one; the counted loop needs the
/// second for its counter.
const SCRATCH_OFFSET: u8 = 16;
const SCRATCH_UNIT: u8 = 17;

/// Section 4: a `mem.copy` is "unrolled up to 64 bytes and a counted loop
/// above".
const UNROLL_CEILING: u64 = 64;

fn load_mnemonic(kind: LoadKind) -> &'static str {
    match kind {
        LoadKind::B => "ldrb",
        LoadKind::H => "ldrh",
        LoadKind::W | LoadKind::X | LoadKind::P | LoadKind::Pf => "ldr",
        LoadKind::Sb => "ldrsb",
        LoadKind::Sh => "ldrsh",
        LoadKind::Sw => "ldrsw",
    }
}

fn store_mnemonic(kind: StoreKind) -> &'static str {
    match kind {
        StoreKind::B => "strb",
        StoreKind::H => "strh",
        StoreKind::W | StoreKind::X | StoreKind::P => "str",
    }
}

/// The class a load's destination is spelled in.
///
/// The transfer size is the mnemonic's, not the register's: `ldrb` and `ldrh`
/// write a thirty-two-bit register, `ldrsw` a sixty-four-bit one, and the two
/// sign-extending byte and halfword forms take whichever the value's class
/// asks for.
const fn load_class(kind: LoadKind, declared: RegClass) -> RegClass {
    match kind {
        LoadKind::B | LoadKind::H | LoadKind::W => RegClass::Gpr32,
        LoadKind::X | LoadKind::P | LoadKind::Pf | LoadKind::Sw => RegClass::Gpr64,
        LoadKind::Sb | LoadKind::Sh => declared,
    }
}

/// The class a store's source is spelled in: the same rule without the
/// sign-extending forms, which do not store.
const fn store_class(kind: StoreKind) -> RegClass {
    match kind {
        StoreKind::B | StoreKind::H | StoreKind::W => RegClass::Gpr32,
        StoreKind::X | StoreKind::P => RegClass::Gpr64,
    }
}

#[allow(clippy::too_many_lines)]
fn emit_inst(
    text: &mut String,
    symbols: &SymbolEmission<'_>,
    inst: &MInst,
    frame: &Frame,
    spell: &impl Fn(VReg, RegClass) -> String,
    function: &str,
    copies: &mut u32,
) {
    let target = symbols.target;
    let name = |reg: VReg| spell(reg, reg.class);
    // Every operand of a two-width form takes the form's own width.
    let at = |reg: VReg, width: Width| spell(reg, width.class());
    match inst {
        MInst::Mov { dst, src } => {
            // The two classes are one register file at two widths, so a move
            // between them is the thirty-two-bit form: it truncates when it
            // reads the low half and zero-extends when it writes it.
            if dst.class == src.class {
                let _ = writeln!(text, "    mov {}, {}", name(*dst), name(*src));
            } else {
                let _ = writeln!(
                    text,
                    "    mov {}, {}",
                    at(*dst, Width::W),
                    at(*src, Width::W)
                );
            }
        }
        MInst::MovImm { dst, imm } => emit_immediate(text, &name(*dst), dst.class, *imm),
        MInst::Adr { dst, block } => {
            let _ = writeln!(
                text,
                "    adr {}, {}",
                name(*dst),
                block_symbol(target, function, *block)
            );
        }
        MInst::Adrp { dst, symbol, kind } => {
            if symbols
                .indirect_addresses
                .contains(&(*kind, symbol.as_str()))
            {
                let operand = match target {
                    Target::Linux => format!(":got:{symbol}"),
                    Target::Darwin => format!("_{symbol}@GOTPAGE"),
                };
                let _ = writeln!(text, "    adrp {}, {operand}", name(*dst));
                return;
            }
            let page = match kind {
                SymbolKind::Global | SymbolKind::Function => "PAGE",
            };
            let _ = writeln!(
                text,
                "    adrp {}, {}",
                name(*dst),
                target.adrp_operand(symbol, page)
            );
        }
        MInst::AddPageOff {
            dst,
            src,
            symbol,
            kind,
        } => {
            if symbols
                .indirect_addresses
                .contains(&(*kind, symbol.as_str()))
            {
                let operand = match target {
                    Target::Linux => format!("#:got_lo12:{symbol}"),
                    Target::Darwin => format!("_{symbol}@GOTPAGEOFF"),
                };
                let _ = writeln!(text, "    ldr {}, [{}, {operand}]", name(*dst), name(*src));
                return;
            }
            let _ = writeln!(
                text,
                "    add {}, {}, {}",
                name(*dst),
                name(*src),
                target.pageoff_operand(symbol)
            );
        }
        MInst::Load {
            dst,
            base,
            offset,
            kind,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                load_mnemonic(*kind),
                spell(*dst, load_class(*kind, dst.class)),
                spell(*base, RegClass::Gpr64)
            );
        }
        MInst::Store {
            src,
            base,
            offset,
            kind,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, [{}, #{offset}]",
                store_mnemonic(*kind),
                spell(*src, store_class(*kind)),
                spell(*base, RegClass::Gpr64)
            );
        }
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{imm}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width)
            );
        }
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let _ = writeln!(
                text,
                "    msub {}, {}, {}, {}",
                at(*dst, *width),
                at(*n, *width),
                at(*m, *width),
                at(*a, *width)
            );
        }
        MInst::Madd {
            dst,
            n,
            m,
            a,
            width,
        } => {
            let _ = writeln!(
                text,
                "    madd {}, {}, {}, {}",
                at(*dst, *width),
                at(*n, *width),
                at(*m, *width),
                at(*a, *width)
            );
        }
        MInst::CarryBinary {
            op,
            dst,
            lhs,
            rhs,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        // The final carry chain keeps flags, not its arithmetic result.
        MInst::AddCarryFlags {
            lhs, rhs, width, ..
        }
        | MInst::SubCarryFlags {
            lhs, rhs, width, ..
        } => {
            let zero = if matches!(width, Width::X) {
                "xzr"
            } else {
                "wzr"
            };
            let _ = writeln!(
                text,
                "    {} {zero}, {}, {}",
                if matches!(inst, MInst::AddCarryFlags { .. }) {
                    "adcs"
                } else {
                    "sbcs"
                },
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        MInst::TstImm {
            lhs, imm, width, ..
        } => {
            let _ = writeln!(text, "    tst {}, #{imm}", at(*lhs, *width));
        }
        MInst::FlagBinary {
            op,
            dst,
            lhs,
            rhs,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width)
            );
        }
        // Section 4 fixes both widths of each wide multiply: `smull xd, wn,
        // wm` and `smulh xd, xn, xm`.
        MInst::WideMultiply { op, dst, lhs, rhs } => {
            let operands = match op {
                WideMul::Smull => RegClass::Gpr32,
                WideMul::Smulh | WideMul::Umulh => RegClass::Gpr64,
            };
            let _ = writeln!(
                text,
                "    {} {}, {}, {}",
                op.mnemonic(),
                spell(*dst, RegClass::Gpr64),
                spell(*lhs, operands),
                spell(*rhs, operands)
            );
        }
        // `sxtb`/`sxth`/`uxtb`/`uxth`/`sxtw` all read a thirty-two-bit source.
        MInst::Extend { op, dst, src } => {
            let _ = writeln!(
                text,
                "    {} {}, {}",
                op.mnemonic(),
                name(*dst),
                spell(*src, RegClass::Gpr32)
            );
        }
        MInst::Unary {
            op,
            dst,
            src,
            width,
        } => {
            let operands = width.class();
            let _ = writeln!(
                text,
                "    {} {}, {}",
                op.mnemonic(),
                spell(*dst, operands),
                spell(*src, operands)
            );
        }
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            width,
        } => {
            let _ = writeln!(
                text,
                "    {} {}, {}, #{lsb}, #{bits}",
                if *signed { "sbfx" } else { "ubfx" },
                at(*dst, *width),
                at(*src, *width)
            );
        }
        MInst::Cmp {
            lhs, rhs, width, ..
        } => {
            let _ = writeln!(text, "    cmp {}, {}", at(*lhs, *width), at(*rhs, *width));
        }
        MInst::CmpImm {
            lhs, imm, width, ..
        } => {
            let _ = writeln!(text, "    cmp {}, #{imm}", at(*lhs, *width));
        }
        MInst::Cset { dst, cond, .. } => {
            let _ = writeln!(text, "    cset {}, {}", name(*dst), cond.mnemonic());
        }
        MInst::Csel {
            dst,
            lhs,
            rhs,
            cond,
            width,
            ..
        } => {
            let _ = writeln!(
                text,
                "    csel {}, {}, {}, {}",
                at(*dst, *width),
                at(*lhs, *width),
                at(*rhs, *width),
                cond.mnemonic()
            );
        }
        MInst::Bl { symbol, .. } => {
            let _ = writeln!(text, "    bl {}{symbol}", target.symbol_prefix());
        }
        MInst::Blr { target, .. } => {
            let _ = writeln!(text, "    blr {}", name(*target));
        }
        // Section 6's variadic places: `stack` is "`x29 + 16` (the first
        // incoming stack slot)" and `gr_top` is "save area + 64".
        MInst::FrameAddress { dst, place } => match place {
            crate::FramePlace::IncomingStack => {
                emit_add_immediate(text, "add", &name(*dst), "x29", 16);
            }
            crate::FramePlace::RegisterSaveTop => {
                let top = frame
                    .register_save
                    .map_or(0, |(_, base)| base + REGISTER_SAVE_BYTES);
                emit_frame_address(text, &name(*dst), top, frame);
            }
            // Section 5's activation-address rows (revision 0.17). The
            // prologue's `stp x29, x30, [sp, #-16]!` puts the saved link
            // register at `x29 + 8`, and the frame pointer is `x29` itself.
            crate::FramePlace::SavedLinkRegister => {
                let _ = writeln!(text, "    ldr {}, [x29, #8]", name(*dst));
            }
            crate::FramePlace::FramePointer => {
                let _ = writeln!(text, "    mov {}, x29", name(*dst));
            }
            crate::FramePlace::AncestorFrame(depth) => {
                let destination = name(*dst);
                let _ = writeln!(text, "    mov {destination}, x29");
                if depth.get() <= 2 {
                    for _ in 0..depth.get() {
                        let _ = writeln!(text, "    ldr {destination}, [{destination}]");
                    }
                } else {
                    // x16 is reserved from allocation. A non-flag-setting sub
                    // and cbnz preserve live NZCV across this pseudo-form.
                    let label = format!("{}{function}_frame{}", target.label_prefix(), *copies);
                    *copies += 1;
                    emit_immediate(text, "x16", RegClass::Gpr64, u64::from(depth.get()));
                    let _ = writeln!(text, "{label}:\n    ldr {destination}, [{destination}]\n    sub x16, x16, #1\n    cbnz x16, {label}");
                }
            }
        },
        // The area itself is reserved by the frame layout and filled by the
        // prologue, so the marker emits nothing of its own.
        MInst::VaSaveArea { .. } => {}
        // Section 6 (revision 0.18): allocation has already pinned the
        // register the marker names to `x8`, so emission writes nothing.
        MInst::IndirectResult => {}
        // Section 6: "one slot per `frame.slot` (its declared size, rounded to
        // its declared alignment ...)".
        MInst::FrameSlot { dst, origin, .. } => {
            let offset = frame.slots.get(origin).copied().unwrap_or(0);
            emit_frame_address(text, &name(*dst), offset, frame);
        }
        MInst::StackAlloc {
            dst, size, align, ..
        } => {
            // Keep a fresh outgoing-argument area below each allocation. It
            // must never overlap either the new object or a previous one.
            let _ = writeln!(text, "    sub x16, sp, {}", name(*size));
            let _ = writeln!(text, "    and x16, x16, #-{}", (*align).max(16));
            let _ = writeln!(text, "    mov {}, x16", name(*dst));
            emit_add_immediate(text, "sub", "sp", "x16", align_up(frame.outgoing, 16));
        }
        MInst::SpillStore { src, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            emit_frame_access(text, "str", &name(*src), offset, frame);
        }
        MInst::SpillReload { dst, slot } => {
            let offset = frame.spills.get(slot).copied().unwrap_or(0);
            emit_frame_access(text, "ldr", &name(*dst), offset, frame);
        }
        // Section 4: emission expands the copy "to `ldr`/`str` pairs of the
        // largest aligned width, unrolled up to 64 bytes and a counted loop
        // above". The two intra-procedure scratch registers of Section 2 are
        // what the expansion may use: one carries each unit, and above the
        // ceiling the other is the counter.
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
            ..
        } => {
            let unit = [8_u64, 4, 2, 1]
                .into_iter()
                .find(|unit| u64::from(*align) >= *unit && size.is_multiple_of(*unit))
                .unwrap_or(1);
            let (load, store, class) = match unit {
                8 => ("ldr", "str", RegClass::Gpr64),
                4 => ("ldr", "str", RegClass::Gpr32),
                2 => ("ldrh", "strh", RegClass::Gpr32),
                _ => ("ldrb", "strb", RegClass::Gpr32),
            };
            let source = spell(*src, RegClass::Gpr64);
            let destination = spell(*dst, RegClass::Gpr64);
            if *size <= UNROLL_CEILING {
                let temporary = scratch(class);
                let mut offset = 0;
                while offset < *size {
                    let _ = writeln!(text, "    {load} {temporary}, [{source}, #{offset}]");
                    let _ = writeln!(text, "    {store} {temporary}, [{destination}, #{offset}]");
                    offset += unit;
                }
            } else {
                // The counter runs down from `size` to zero in units, so the
                // loop needs no comparison and leaves `nzcv` alone; the offset
                // register is the second scratch, and the two addresses are
                // read but never written, since the allocator may still have
                // uses for them after the copy.
                let counter = physical(RegClass::Gpr64, SCRATCH_OFFSET);
                let temporary = physical(class, SCRATCH_UNIT);
                let body = format!("L{function}_copy{copies}");
                *copies += 1;
                emit_immediate(text, &counter, RegClass::Gpr64, *size);
                let _ = writeln!(text, "{body}:");
                let _ = writeln!(text, "    sub {counter}, {counter}, #{unit}");
                let _ = writeln!(text, "    {load} {temporary}, [{source}, {counter}]");
                let _ = writeln!(text, "    {store} {temporary}, [{destination}, {counter}]");
                let _ = writeln!(text, "    cbnz {counter}, {body}");
            }
        }
        // Section 6: "the callee reads slot `i` at `x29 + 16 + 8 * i`, since
        // the prologue saves the pair first", and the caller writes its own
        // at increasing addresses from the `sp` it calls with.
        MInst::IncomingArg { dst, slot } => {
            let offset = 16 + u64::from(*slot) * 8;
            let _ = writeln!(text, "    ldr {}, [x29, #{offset}]", name(*dst));
        }
        MInst::OutgoingArg { src, slot } => {
            let offset = u64::from(*slot) * 8;
            let _ = writeln!(text, "    str {}, [sp, #{offset}]", name(*src));
        }
        MInst::Asm(body) => emit_assembly(text, target, body, &[], function, spell),
        MInst::Ldp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
            ..
        } => {
            let bang = if *writeback { "!" } else { "" };
            let _ = writeln!(
                text,
                "    ldp {}, {}, [{}, #{offset}]{bang}",
                name(*rt1),
                name(*rt2),
                name(*base)
            );
        }
        MInst::Stp {
            rt1,
            rt2,
            base,
            offset,
            writeback,
            ..
        } => {
            let bang = if *writeback { "!" } else { "" };
            let _ = writeln!(
                text,
                "    stp {}, {}, [{}, #{offset}]{bang}",
                name(*rt1),
                name(*rt2),
                name(*base)
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_term(
    text: &mut String,
    target: Target,
    function: &str,
    term: &MTerm,
    frame: &Frame,
    next: Option<u32>,
    spell: &impl Fn(VReg, RegClass) -> String,
    relax: bool,
    relaxations: &mut u32,
) {
    let name = |reg: VReg| spell(reg, reg.class);
    let platform = target;
    let branch = |text: &mut String, target: u32| {
        if next != Some(target) {
            let _ = writeln!(text, "    b {}", label(platform, function, target));
        }
    };
    // A relaxed conditional terminator is its own inverse jumping over an
    // unconditional `b` to the taken block: the short branch then spans two
    // instructions and always reaches, and the long one reaches the function.
    // `over` is the label it jumps to; `None` is the unrelaxed spelling, whose
    // operand is the taken block itself.
    let over = |relaxations: &mut u32| -> Option<String> {
        if !relax {
            return None;
        }
        let label = relax_label(platform, function, *relaxations);
        *relaxations += 1;
        Some(label)
    };
    let close = |text: &mut String, over: Option<String>, taken: u32| {
        if let Some(over) = over {
            let _ = writeln!(text, "    b {}", label(platform, function, taken));
            let _ = writeln!(text, "{over}:");
        }
    };
    match term {
        MTerm::Indirect { target, .. } => {
            let _ = writeln!(text, "    br {}", name(*target));
        }
        MTerm::B { target, .. } => branch(text, *target),
        MTerm::BCond {
            cond,
            taken,
            untaken,
            ..
        } => {
            let over = over(relaxations);
            let (cond, destination) = match &over {
                Some(over) => (cond.inverted(), over.clone()),
                None => (*cond, label(platform, function, *taken)),
            };
            let _ = writeln!(text, "    b.{} {destination}", cond.mnemonic());
            close(text, over, *taken);
            branch(text, *untaken);
        }
        MTerm::Cb {
            nonzero,
            src,
            taken,
            untaken,
            ..
        } => {
            let over = over(relaxations);
            let (nonzero, destination) = match &over {
                Some(over) => (!*nonzero, over.clone()),
                None => (*nonzero, label(platform, function, *taken)),
            };
            let _ = writeln!(
                text,
                "    {} {}, {destination}",
                if nonzero { "cbnz" } else { "cbz" },
                name(*src)
            );
            close(text, over, *taken);
            branch(text, *untaken);
        }
        MTerm::Tb {
            nonzero,
            src,
            bit,
            taken,
            untaken,
            ..
        } => {
            let over = over(relaxations);
            let (nonzero, destination) = match &over {
                Some(over) => (!*nonzero, over.clone()),
                None => (*nonzero, label(platform, function, *taken)),
            };
            let _ = writeln!(
                text,
                "    {} {}, #{bit}, {destination}",
                if nonzero { "tbnz" } else { "tbz" },
                name(*src)
            );
            close(text, over, *taken);
            branch(text, *untaken);
        }
        // Section 8: the template first, then the branch its `%l[label]`
        // references did not take. A successor the template jumps to is the
        // template's own business; the fall-through is emission's.
        MTerm::AsmGoto {
            body,
            labels,
            fallthrough,
            ..
        } => {
            emit_assembly(text, target, body, labels, function, spell);
            branch(text, *fallthrough);
        }
        MTerm::Ret { .. } => {
            emit_epilogue(text, frame);
            let _ = writeln!(text, "    ret");
        }
        MTerm::Brk { imm } => {
            let _ = writeln!(text, "    brk #{imm}");
        }
    }
}
