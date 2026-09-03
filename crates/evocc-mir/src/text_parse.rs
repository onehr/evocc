//! The canonical textual form of Appendix A: the parser.
//!
//! A parser rejects rather than repairs, for the reason EvoIR Section 9.5
//! gives: a parser that accepts more than the renderer emits can read a file
//! no producer could have written.
//!
//! A use is spelled `%vN` without its class, because a definition already
//! carries one and two spellings of the same register would break the
//! "one rendering per function" rule of Appendix A. The class of a use is
//! therefore recovered from its definition, which Section 2 places either in
//! the same block before it or in that block's argument list.

use std::collections::HashMap;

use evocc_evoir::{AccessLayout, GlobalKind, GlobalObject, GlobalVisibility};

use crate::{
    BinOp, Cond, ExtOp, FlagOp, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, RegClass,
    StoreKind, SymbolKind, VReg, WideMul, Width,
};

/// Parse canonical Appendix A bytes back into a module.
pub fn parse_module(text: &str) -> Result<MModule, String> {
    if text.contains('\t') {
        return Err("a tab is never canonical".to_owned());
    }
    if text.contains('\r') {
        return Err("a carriage return is never canonical".to_owned());
    }
    if text.is_empty() {
        return Ok(MModule::default());
    }
    if !text.ends_with('\n') {
        return Err("the final byte is a newline".to_owned());
    }
    let mut lines: Vec<&str> = text.split('\n').collect();
    lines.pop();
    for line in &lines {
        if line != &line.trim_end() {
            return Err(format!("trailing whitespace on line '{line}'"));
        }
    }

    let mut cursor = 0;
    let mut globals = Vec::new();
    while cursor < lines.len() && lines[cursor].starts_with("global ") {
        globals.push(parse_global(lines[cursor])?);
        cursor += 1;
    }
    if !globals.is_empty() && cursor < lines.len() {
        if !lines[cursor].is_empty() {
            return Err("globals and functions are separated by one blank line".to_owned());
        }
        cursor += 1;
    }

    let mut functions = Vec::new();
    while cursor < lines.len() {
        if !functions.is_empty() {
            if !lines[cursor].is_empty() {
                return Err("functions are separated by one blank line".to_owned());
            }
            cursor += 1;
        }
        functions.push(parse_function(&lines, &mut cursor)?);
    }
    Ok(MModule { globals, functions })
}

fn parse_global(line: &str) -> Result<GlobalObject, String> {
    let rest = line
        .strip_prefix("global @")
        .ok_or_else(|| format!("not a global line: '{line}'"))?;
    let mut fields = rest.split(' ');
    let symbol = fields
        .next()
        .ok_or_else(|| "global has no symbol".to_owned())?
        .to_owned();
    let mut attribute = |key: &str| -> Result<String, String> {
        let field = fields
            .next()
            .ok_or_else(|| format!("global @{symbol} has no {key}"))?;
        field
            .strip_prefix(&format!("{key}="))
            .map(str::to_owned)
            .ok_or_else(|| format!("global @{symbol} expected {key}=, found '{field}'"))
    };
    let origin: u64 = attribute("origin")?
        .parse()
        .map_err(|_| "global origin is an unsigned decimal".to_owned())?;
    let kind = match attribute("kind")?.as_str() {
        "file" => GlobalKind::File,
        "function_static" => GlobalKind::FunctionStatic,
        "string_literal" => GlobalKind::StringLiteral,
        other => return Err(format!("unknown global kind '{other}'")),
    };
    let visibility = match attribute("visibility")?.as_str() {
        "external" => GlobalVisibility::External,
        "internal" => GlobalVisibility::Internal,
        other => return Err(format!("unknown visibility '{other}'")),
    };
    let size: u64 = attribute("size")?
        .parse()
        .map_err(|_| "global size is an unsigned decimal".to_owned())?;
    let align: u32 = attribute("align")?
        .parse()
        .map_err(|_| "global align is an unsigned decimal".to_owned())?;
    let bytes_text = attribute("bytes")?;
    let initialized_text = attribute("initialized")?;
    if fields.next().is_some() {
        return Err("a global line carries exactly its enumerated fields".to_owned());
    }
    if bytes_text.len() as u64 != size * 2 {
        return Err("a bytes run is exactly two hex digits per byte".to_owned());
    }
    let mut bytes = Vec::new();
    for pair in bytes_text.as_bytes().chunks(2) {
        let text = std::str::from_utf8(pair).map_err(|_| "invalid hex byte".to_owned())?;
        if !text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(format!("hex bytes are lowercase: '{text}'"));
        }
        bytes.push(u8::from_str_radix(text, 16).map_err(|_| "invalid hex byte".to_owned())?);
    }
    if initialized_text.len() as u64 != size {
        return Err("an initialized run is exactly one bit per byte".to_owned());
    }
    let initialized = initialized_text
        .chars()
        .map(|bit| match bit {
            '0' => Ok(false),
            '1' => Ok(true),
            other => Err(format!("an initialized bit is 0 or 1, found '{other}'")),
        })
        .collect::<Result<Vec<bool>, String>>()?;
    Ok(GlobalObject {
        symbol,
        origin,
        kind,
        visibility,
        layout: AccessLayout::new(size, align),
        bytes,
        initialized,
    })
}

fn parse_function(lines: &[&str], cursor: &mut usize) -> Result<MFunction, String> {
    let header = lines
        .get(*cursor)
        .ok_or_else(|| "expected a function header".to_owned())?;
    let name = header
        .strip_prefix("func @")
        .and_then(|rest| rest.strip_suffix(" {"))
        .ok_or_else(|| format!("not a function header: '{header}'"))?
        .to_owned();
    *cursor += 1;

    let mut classes: HashMap<u32, RegClass> = HashMap::new();
    let mut blocks: Vec<MBlock> = Vec::new();
    loop {
        let line = lines
            .get(*cursor)
            .ok_or_else(|| format!("function @{name} is unterminated"))?;
        if *line == "}" {
            *cursor += 1;
            break;
        }
        if !blocks.is_empty() {
            if !line.is_empty() {
                return Err("blocks are separated by one blank line".to_owned());
            }
            *cursor += 1;
        }
        blocks.push(parse_block(lines, cursor, blocks.is_empty(), &mut classes)?);
    }
    Ok(MFunction { name, blocks })
}

fn parse_block(
    lines: &[&str],
    cursor: &mut usize,
    entry: bool,
    classes: &mut HashMap<u32, RegClass>,
) -> Result<MBlock, String> {
    let header = lines
        .get(*cursor)
        .ok_or_else(|| "expected a block label".to_owned())?;
    let (label, rest) = header
        .split_once('(')
        .ok_or_else(|| format!("not a block label: '{header}'"))?;
    let id = if entry {
        if label != "^entry" {
            return Err("the first block of a function is ^entry".to_owned());
        }
        0
    } else {
        label
            .strip_prefix("^b")
            .and_then(|digits| digits.parse::<u32>().ok())
            .ok_or_else(|| format!("not a block label: '{label}'"))?
    };
    let args_text = rest
        .strip_suffix("):")
        .ok_or_else(|| format!("a block label ends with '):': '{header}'"))?;
    let mut args = Vec::new();
    if !args_text.is_empty() {
        for field in args_text.split(", ") {
            let reg = parse_definition(field)?;
            classes.insert(reg.id, reg.class);
            args.push(reg);
        }
    }
    *cursor += 1;

    let mut insts = Vec::new();
    let mut term = None;
    while term.is_none() {
        let line = lines
            .get(*cursor)
            .ok_or_else(|| "a block ends in a terminator".to_owned())?;
        let body = line
            .strip_prefix("    ")
            .ok_or_else(|| format!("the instruction indent is four spaces: '{line}'"))?;
        if body.starts_with(' ') {
            return Err(format!("the instruction indent is four spaces: '{line}'"));
        }
        *cursor += 1;
        match parse_body(body, classes)? {
            Parsed::Inst(inst) => {
                for reg in crate::inst_defs(&inst) {
                    classes.insert(reg.id, reg.class);
                }
                insts.push(inst);
            }
            // The grammar writes `b.<cond>` with no condition register, so the
            // flags value it reads is the one Section 2's adjacency rule
            // requires: the flags register the preceding instruction defines.
            Parsed::Term(MTerm::BCond {
                cond,
                taken,
                taken_args,
                untaken,
                untaken_args,
                ..
            }) => {
                let flags = insts
                    .last()
                    .and_then(|inst| {
                        crate::inst_defs(inst)
                            .into_iter()
                            .find(|reg| reg.class == RegClass::Flags)
                    })
                    .ok_or_else(|| {
                        "a conditional branch follows the instruction that sets its flags"
                            .to_owned()
                    })?;
                term = Some(MTerm::BCond {
                    flags,
                    cond,
                    taken,
                    taken_args,
                    untaken,
                    untaken_args,
                });
            }
            Parsed::Term(parsed) => term = Some(parsed),
        }
    }
    Ok(MBlock {
        id,
        args,
        insts,
        term: term.expect("the loop exits only with a terminator"),
    })
}

fn parse_definition(text: &str) -> Result<VReg, String> {
    let (name, class) = text
        .split_once(':')
        .ok_or_else(|| format!("a definition names its class: '{text}'"))?;
    let id = name
        .strip_prefix("%v")
        .and_then(|digits| digits.parse::<u32>().ok())
        .ok_or_else(|| format!("not a virtual register: '{name}'"))?;
    let class = match class {
        "gpr32" => RegClass::Gpr32,
        "gpr64" => RegClass::Gpr64,
        "flags" => RegClass::Flags,
        other => return Err(format!("unknown register class '{other}'")),
    };
    Ok(VReg::new(id, class))
}

enum Parsed {
    Inst(MInst),
    Term(MTerm),
}

/// A cursor over one instruction body.
struct Scan<'a> {
    text: &'a str,
    classes: &'a HashMap<u32, RegClass>,
}

impl<'a> Scan<'a> {
    fn new(text: &'a str, classes: &'a HashMap<u32, RegClass>) -> Self {
        Self { text, classes }
    }

    fn expect(&mut self, literal: &str) -> Result<(), String> {
        self.text = self
            .text
            .strip_prefix(literal)
            .ok_or_else(|| format!("expected '{literal}' at '{}'", self.text))?;
        Ok(())
    }

    fn take_while(&mut self, accept: impl Fn(char) -> bool) -> &'a str {
        let end = self
            .text
            .find(|character| !accept(character))
            .unwrap_or(self.text.len());
        let (head, tail) = self.text.split_at(end);
        self.text = tail;
        head
    }

    fn use_of(&mut self) -> Result<VReg, String> {
        self.expect("%v")?;
        let digits = self.take_while(|character| character.is_ascii_digit());
        let id: u32 = digits
            .parse()
            .map_err(|_| format!("not a virtual register index: '{digits}'"))?;
        let class = self
            .classes
            .get(&id)
            .copied()
            .ok_or_else(|| format!("%v{id} is used without a definition"))?;
        Ok(VReg::new(id, class))
    }

    fn immediate(&mut self) -> Result<u64, String> {
        self.expect("#")?;
        let digits = self.take_while(|character| character.is_ascii_digit());
        digits
            .parse()
            .map_err(|_| format!("not an unsigned decimal immediate: '{digits}'"))
    }

    fn signed_immediate(&mut self) -> Result<i64, String> {
        self.expect("#")?;
        let text = self.take_while(|character| character.is_ascii_digit() || character == '-');
        text.parse()
            .map_err(|_| format!("not a decimal offset: '{text}'"))
    }

    fn condition(&mut self) -> Result<Cond, String> {
        let text = self.take_while(|character| character.is_ascii_alphabetic());
        Cond::from_mnemonic(text).ok_or_else(|| format!("unknown condition '{text}'"))
    }

    fn symbol(&mut self) -> Result<String, String> {
        self.expect("@")?;
        Ok(self
            .take_while(|character| {
                character.is_ascii_alphanumeric() || character == '_' || character == '.'
            })
            .to_owned())
    }

    /// The relocation name after a symbol, which also says which symbol table
    /// the pair resolves in.
    fn page_kind(&mut self, base: &str) -> Result<SymbolKind, String> {
        self.expect("@")?;
        if self.text.starts_with("FUNC") {
            self.expect("FUNC")?;
            self.expect(base)?;
            Ok(SymbolKind::Function)
        } else {
            self.expect(base)?;
            Ok(SymbolKind::Global)
        }
    }

    fn use_list(&mut self) -> Result<Vec<VReg>, String> {
        self.expect("(")?;
        let mut regs = Vec::new();
        if self.text.starts_with(')') {
            self.expect(")")?;
            return Ok(regs);
        }
        loop {
            regs.push(self.use_of()?);
            if self.text.starts_with(", ") {
                self.expect(", ")?;
                continue;
            }
            self.expect(")")?;
            return Ok(regs);
        }
    }

    /// The optional `-> results` of a call: definitions, so each names its
    /// class.
    fn result_list(&mut self) -> Result<Vec<VReg>, String> {
        if !self.text.starts_with(" -> ") {
            return Ok(Vec::new());
        }
        self.expect(" -> ")?;
        let mut regs = Vec::new();
        loop {
            let name = self
                .take_while(|character| character != ',' && character != ' ' && character != ')');
            regs.push(parse_definition(name)?);
            if self.text.starts_with(", ") {
                self.expect(", ")?;
                continue;
            }
            return Ok(regs);
        }
    }

    fn edge(&mut self) -> Result<(u32, Vec<VReg>), String> {
        self.expect("^")?;
        let label = self.take_while(|character| character.is_ascii_alphanumeric());
        let target = if label == "entry" {
            0
        } else {
            label
                .strip_prefix('b')
                .and_then(|digits| digits.parse::<u32>().ok())
                .ok_or_else(|| format!("not a block label: '^{label}'"))?
        };
        Ok((target, self.use_list()?))
    }

    fn finish(&self) -> Result<(), String> {
        if self.text.is_empty() {
            Ok(())
        } else {
            Err(format!("trailing text '{}'", self.text))
        }
    }
}

fn split_width(mnemonic: &str) -> Option<(&str, Width)> {
    if let Some(head) = mnemonic.strip_suffix(".w") {
        Some((head, Width::W))
    } else {
        mnemonic.strip_suffix(".x").map(|head| (head, Width::X))
    }
}

fn parse_body(body: &str, classes: &HashMap<u32, RegClass>) -> Result<Parsed, String> {
    // `adds`/`subs` define a register and the flags. The Appendix A grammar has
    // no two-destination production, so both are written before the `=` and
    // recognised before the single-destination forms.
    if let Some(inst) = parse_flag_binary(body, classes)? {
        return Ok(Parsed::Inst(inst));
    }
    let (dst, rest) = match body.split_once(" = ") {
        Some((head, tail)) if head.starts_with("%v") => (Some(parse_definition(head)?), tail),
        _ => (None, body),
    };
    let (mnemonic, arguments) = match rest.split_once(' ') {
        Some(split) => split,
        None => (rest, ""),
    };
    let mut scan = Scan::new(arguments, classes);

    // Terminators first: none of them defines a register.
    if dst.is_none() {
        match mnemonic {
            "ret" => {
                let mut values = Vec::new();
                if !arguments.is_empty() {
                    loop {
                        values.push(scan.use_of()?);
                        if scan.text.starts_with(", ") {
                            scan.expect(", ")?;
                            continue;
                        }
                        break;
                    }
                }
                scan.finish()?;
                return Ok(Parsed::Term(MTerm::Ret { values }));
            }
            "brk" => {
                let imm = scan.immediate()?;
                scan.finish()?;
                let imm =
                    u16::try_from(imm).map_err(|_| "a brk immediate is 16 bits".to_owned())?;
                return Ok(Parsed::Term(MTerm::Brk { imm }));
            }
            "b" => {
                let (target, args) = scan.edge()?;
                scan.finish()?;
                return Ok(Parsed::Term(MTerm::B { target, args }));
            }
            "cbz" | "cbnz" => {
                let src = scan.use_of()?;
                scan.expect(", ")?;
                let (taken, taken_args) = scan.edge()?;
                scan.expect(", ")?;
                let (untaken, untaken_args) = scan.edge()?;
                scan.finish()?;
                return Ok(Parsed::Term(MTerm::Cb {
                    nonzero: mnemonic == "cbnz",
                    src,
                    taken,
                    taken_args,
                    untaken,
                    untaken_args,
                }));
            }
            "tbz" | "tbnz" => {
                let src = scan.use_of()?;
                scan.expect(", ")?;
                let bit = scan.immediate()?;
                scan.expect(", ")?;
                let (taken, taken_args) = scan.edge()?;
                scan.expect(", ")?;
                let (untaken, untaken_args) = scan.edge()?;
                scan.finish()?;
                return Ok(Parsed::Term(MTerm::Tb {
                    nonzero: mnemonic == "tbnz",
                    src,
                    bit: u16::try_from(bit).map_err(|_| "a bit index is small".to_owned())?,
                    taken,
                    taken_args,
                    untaken,
                    untaken_args,
                }));
            }
            _ => {}
        }
        if let Some(name) = mnemonic.strip_prefix("b.") {
            let name = name
                .strip_suffix(',')
                .ok_or_else(|| format!("a conditional branch names two successors: '{body}'"))?;
            let cond =
                Cond::from_mnemonic(name).ok_or_else(|| format!("unknown condition '{name}'"))?;
            let (taken, taken_args) = scan.edge()?;
            scan.expect(", ")?;
            let (untaken, untaken_args) = scan.edge()?;
            scan.finish()?;
            // The flags operand is filled in by the caller from the preceding
            // instruction; a placeholder cannot escape, because `parse_block`
            // replaces it before the block is built.
            return Ok(Parsed::Term(MTerm::BCond {
                flags: VReg::new(u32::MAX, RegClass::Flags),
                cond,
                taken,
                taken_args,
                untaken,
                untaken_args,
            }));
        }
        // A store defines nothing either.
        let volatile = arguments.starts_with("volatile ");
        if let Some(kind) = StoreKind::from_mnemonic(mnemonic) {
            if volatile {
                scan.expect("volatile ")?;
            }
            let src = scan.use_of()?;
            scan.expect(", [")?;
            let base = scan.use_of()?;
            scan.expect(", ")?;
            let offset = scan.signed_immediate()?;
            scan.expect("]")?;
            scan.finish()?;
            return Ok(Parsed::Inst(MInst::Store {
                src,
                base,
                offset,
                kind,
                volatile,
            }));
        }
        if mnemonic == "bl" {
            let symbol = scan.symbol()?;
            let args = scan.use_list()?;
            let results = scan.result_list()?;
            scan.finish()?;
            return Ok(Parsed::Inst(MInst::Bl {
                symbol,
                args,
                results,
            }));
        }
        if mnemonic == "blr" {
            let target = scan.use_of()?;
            let args = scan.use_list()?;
            let results = scan.result_list()?;
            scan.finish()?;
            return Ok(Parsed::Inst(MInst::Blr {
                target,
                args,
                results,
            }));
        }
        if mnemonic == "mem.copy" {
            scan.expect("[")?;
            let dst = scan.use_of()?;
            scan.expect("], [")?;
            let src = scan.use_of()?;
            scan.expect("], ")?;
            let size = scan.immediate()?;
            scan.expect(", ")?;
            let align = scan.immediate()?;
            scan.finish()?;
            return Ok(Parsed::Inst(MInst::MemCopy {
                dst,
                src,
                size,
                align: u32::try_from(align).map_err(|_| "an alignment is small".to_owned())?,
            }));
        }
        return Err(format!("unknown form '{body}'"));
    }

    let dst = dst.expect("checked above");
    parse_defining(dst, mnemonic, arguments, &mut scan).map(Parsed::Inst)
}

fn parse_defining(
    dst: VReg,
    mnemonic: &str,
    arguments: &str,
    scan: &mut Scan<'_>,
) -> Result<MInst, String> {
    if mnemonic == "mov" {
        if arguments.starts_with('#') {
            let imm = scan.immediate()?;
            scan.finish()?;
            return Ok(MInst::MovImm { dst, imm });
        }
        let src = scan.use_of()?;
        scan.finish()?;
        return Ok(MInst::Mov { dst, src });
    }
    if mnemonic == "adrp" {
        let symbol = scan.symbol()?;
        let kind = scan.page_kind("PAGE")?;
        scan.finish()?;
        return Ok(MInst::Adrp { dst, symbol, kind });
    }
    if mnemonic == "add" {
        let src = scan.use_of()?;
        scan.expect(", ")?;
        let symbol = scan.symbol()?;
        let kind = scan.page_kind("PAGEOFF")?;
        scan.finish()?;
        return Ok(MInst::AddPageOff {
            dst,
            src,
            symbol,
            kind,
        });
    }
    if mnemonic == "frame.slot" {
        scan.expect("[origin=")?;
        let origin: u64 = scan
            .take_while(|character| character.is_ascii_digit())
            .parse()
            .map_err(|_| "an origin is an unsigned decimal".to_owned())?;
        scan.expect(", size=")?;
        let size: u64 = scan
            .take_while(|character| character.is_ascii_digit())
            .parse()
            .map_err(|_| "a size is an unsigned decimal".to_owned())?;
        scan.expect(", align=")?;
        let align: u32 = scan
            .take_while(|character| character.is_ascii_digit())
            .parse()
            .map_err(|_| "an alignment is an unsigned decimal".to_owned())?;
        scan.expect("]")?;
        scan.finish()?;
        return Ok(MInst::FrameSlot {
            dst,
            origin,
            size,
            align,
        });
    }
    if mnemonic == "cset" {
        let flags = scan.use_of()?;
        scan.expect(", ")?;
        let cond = scan.condition()?;
        scan.finish()?;
        return Ok(MInst::Cset { dst, flags, cond });
    }
    if let Some(op) = WideMul::from_mnemonic(mnemonic) {
        let lhs = scan.use_of()?;
        scan.expect(", ")?;
        let rhs = scan.use_of()?;
        scan.finish()?;
        return Ok(MInst::WideMultiply { op, dst, lhs, rhs });
    }
    if let Some(op) = ExtOp::from_mnemonic(mnemonic) {
        let src = scan.use_of()?;
        scan.finish()?;
        return Ok(MInst::Extend { op, dst, src });
    }
    let volatile = arguments.starts_with("volatile ");
    if let Some(kind) = LoadKind::from_mnemonic(mnemonic) {
        if volatile {
            scan.expect("volatile ")?;
        }
        scan.expect("[")?;
        let base = scan.use_of()?;
        scan.expect(", ")?;
        let offset = scan.signed_immediate()?;
        scan.expect("]")?;
        scan.finish()?;
        return Ok(MInst::Load {
            dst,
            base,
            offset,
            kind,
            volatile,
        });
    }

    let (head, width) =
        split_width(mnemonic).ok_or_else(|| format!("unknown form '{mnemonic}'"))?;
    match head {
        "msub" => {
            let n = scan.use_of()?;
            scan.expect(", ")?;
            let m = scan.use_of()?;
            scan.expect(", ")?;
            let a = scan.use_of()?;
            scan.finish()?;
            Ok(MInst::Msub {
                dst,
                n,
                m,
                a,
                width,
            })
        }
        "sbfx" | "ubfx" => {
            let src = scan.use_of()?;
            scan.expect(", ")?;
            let lsb = scan.immediate()?;
            scan.expect(", ")?;
            let bits = scan.immediate()?;
            scan.finish()?;
            Ok(MInst::Bfx {
                signed: head == "sbfx",
                dst,
                src,
                lsb: u16::try_from(lsb).map_err(|_| "a bit index is small".to_owned())?,
                bits: u16::try_from(bits).map_err(|_| "a bit width is small".to_owned())?,
                width,
            })
        }
        "cmp" => {
            let lhs = scan.use_of()?;
            scan.expect(", ")?;
            if scan.text.starts_with('#') {
                let imm = scan.immediate()?;
                scan.finish()?;
                Ok(MInst::CmpImm {
                    dst,
                    lhs,
                    imm,
                    width,
                })
            } else {
                let rhs = scan.use_of()?;
                scan.finish()?;
                Ok(MInst::Cmp {
                    dst,
                    lhs,
                    rhs,
                    width,
                })
            }
        }
        "csel" => {
            let flags = scan.use_of()?;
            scan.expect(", ")?;
            let lhs = scan.use_of()?;
            scan.expect(", ")?;
            let rhs = scan.use_of()?;
            scan.expect(", ")?;
            let cond = scan.condition()?;
            scan.finish()?;
            Ok(MInst::Csel {
                dst,
                flags,
                lhs,
                rhs,
                cond,
                width,
            })
        }
        _ => {
            let op =
                BinOp::from_mnemonic(head).ok_or_else(|| format!("unknown form '{mnemonic}'"))?;
            let lhs = scan.use_of()?;
            scan.expect(", ")?;
            if scan.text.starts_with('#') {
                let imm = scan.immediate()?;
                scan.finish()?;
                Ok(MInst::BinaryImm {
                    op,
                    dst,
                    lhs,
                    imm,
                    width,
                })
            } else {
                let rhs = scan.use_of()?;
                scan.finish()?;
                Ok(MInst::Binary {
                    op,
                    dst,
                    lhs,
                    rhs,
                    width,
                })
            }
        }
    }
}

/// The two-destination `adds`/`subs` form, or `None` when `body` is not one.
fn parse_flag_binary(
    body: &str,
    classes: &HashMap<u32, RegClass>,
) -> Result<Option<MInst>, String> {
    let Some((head, tail)) = body.split_once(" = ") else {
        return Ok(None);
    };
    let Some((left, right)) = head.split_once(", ") else {
        return Ok(None);
    };
    if !left.starts_with("%v") || !right.starts_with("%v") {
        return Ok(None);
    }
    let dst = parse_definition(left)?;
    let flags = parse_definition(right)?;
    let (mnemonic, arguments) = tail
        .split_once(' ')
        .ok_or_else(|| format!("unknown form '{body}'"))?;
    let (name, width) =
        split_width(mnemonic).ok_or_else(|| format!("unknown form '{mnemonic}'"))?;
    let op = FlagOp::from_mnemonic(name).ok_or_else(|| format!("unknown form '{mnemonic}'"))?;
    let mut scan = Scan::new(arguments, classes);
    let lhs = scan.use_of()?;
    scan.expect(", ")?;
    let rhs = scan.use_of()?;
    scan.finish()?;
    Ok(Some(MInst::FlagBinary {
        op,
        dst,
        flags,
        lhs,
        rhs,
        width,
    }))
}
