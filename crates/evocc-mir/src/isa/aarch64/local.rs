//! Adjacent emission idioms. This never rewrites MIR or bypasses allocation
//! verification. Dead temporary contents are excluded from the live-state claim.
use std::collections::BTreeMap;
use std::fmt::Write;

use super::patterns::{LocalKind, LocalPattern, LOCAL_PATTERNS};
use crate::{
    inst_defs, inst_uses, term_uses, Allocated, BinOp, ExtOp, MInst, RegClass, VReg, Width,
};

#[derive(Debug, Default)]
pub struct Counts {
    pub selected: BTreeMap<&'static str, usize>,
    pub declined: BTreeMap<(&'static str, &'static str), usize>,
    pub removed_instructions: usize,
}

pub struct Selector<'a> {
    assignment: &'a BTreeMap<u32, u8>,
    occurrences: BTreeMap<u32, (usize, usize)>,
    pub counts: Counts,
}

#[derive(Debug)]
pub struct Selection {
    pub consumed: usize,
    pub pattern: &'static str,
    form: Form,
}

#[derive(Debug)]
enum Form {
    Alu {
        op: &'static str,
        dst: Option<VReg>,
        lhs: VReg,
        rhs: VReg,
        width: Width,
        rhs_class: RegClass,
        modifier: &'static str,
        amount: Option<u64>,
    },
    Multiply {
        op: &'static str,
        dst: VReg,
        lhs: VReg,
        rhs: VReg,
        addend: VReg,
        width: Width,
    },
    Memory {
        access: MInst,
        base: VReg,
        index: VReg,
        shift: u64,
    },
}

struct Consumer {
    op: &'static str,
    dst: Option<VReg>,
    other: VReg,
    width: Width,
}

fn consumer(inst: &MInst, temporary: VReg, compare: bool) -> Option<Consumer> {
    match inst {
        MInst::Cmp {
            lhs, rhs, width, ..
        } if compare && *rhs == temporary => Some(Consumer {
            op: "cmp",
            dst: None,
            other: *lhs,
            width: *width,
        }),
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } if !compare => {
            let other = if *rhs == temporary {
                *lhs
            } else if *lhs == temporary
                && matches!(op, BinOp::Add | BinOp::And | BinOp::Orr | BinOp::Eor)
            {
                *rhs
            } else {
                return None;
            };
            Some(Consumer {
                op: op.mnemonic(),
                dst: Some(*dst),
                other,
                width: *width,
            })
        }
        _ => None,
    }
}

impl<'a> Selector<'a> {
    pub fn new(function: &'a Allocated) -> Self {
        let mut occurrences: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
        for block in &function.blocks {
            for reg in block
                .args
                .iter()
                .copied()
                .chain(block.insts.iter().flat_map(inst_defs))
            {
                occurrences.entry(reg.id).or_default().0 += 1;
            }
            for reg in block
                .insts
                .iter()
                .flat_map(inst_uses)
                .chain(term_uses(&block.term))
            {
                occurrences.entry(reg.id).or_default().1 += 1;
            }
        }
        Self {
            assignment: &function.assignment,
            occurrences,
            counts: Counts::default(),
        }
    }

    fn single_use(&self, temporary: VReg) -> Result<(), &'static str> {
        if self.occurrences.get(&temporary.id) == Some(&(1, 1)) {
            Ok(())
        } else {
            Err("temporary-not-single-definition-and-use")
        }
    }

    // A source first read after an omitted write must not alias that write.
    // Sources read by the first instruction itself may legally alias its dst.
    fn input_survives(&self, input: VReg, removed: &[VReg]) -> Result<(), &'static str> {
        let Some(location) = self.assignment.get(&input.id) else {
            return Err("unallocated-input");
        };
        if removed
            .iter()
            .any(|reg| self.assignment.get(&reg.id) == Some(location))
        {
            Err("overwritten-surviving-input")
        } else {
            Ok(())
        }
    }

    pub fn select(&mut self, instructions: &[MInst], strict_alignment: bool) -> Option<Selection> {
        let bytes = super::costs::SCALAR_BYTES?;
        for pattern in LOCAL_PATTERNS {
            match self.candidate(pattern, instructions, strict_alignment) {
                Ok(Some(selection)) if selection.consumed as u32 * bytes > bytes => {
                    *self.counts.selected.entry(pattern.id).or_default() += 1;
                    self.counts.removed_instructions += selection.consumed - 1;
                    return Some(selection);
                }
                Err(reason) => {
                    *self
                        .counts
                        .declined
                        .entry((pattern.id, reason))
                        .or_default() += 1
                }
                _ => {}
            }
        }
        None
    }

    fn candidate(
        &self,
        pattern: &LocalPattern,
        instructions: &[MInst],
        strict_alignment: bool,
    ) -> Result<Option<Selection>, &'static str> {
        let [first, second, ..] = instructions else {
            return Ok(None);
        };
        if pattern.kind == LocalKind::IndexedMemory {
            return self.memory(pattern, instructions, strict_alignment);
        }
        let compare = matches!(
            pattern.kind,
            LocalKind::ShiftedCompare | LocalKind::ExtendedCompare
        );
        let (temporary, source, modifier, amount, width) = match (pattern.kind, first) {
            (
                LocalKind::ShiftedArithmetic | LocalKind::ShiftedCompare,
                MInst::BinaryImm {
                    op,
                    dst,
                    lhs,
                    imm,
                    width,
                },
            ) if pattern.shifts.contains(&op.mnemonic()) => {
                (*dst, *lhs, op.mnemonic(), Some(*imm), *width)
            }
            (
                LocalKind::ExtendedArithmetic | LocalKind::ExtendedCompare,
                MInst::Extend { op, dst, src },
            ) => {
                let Some(width) = Width::for_class(dst.class) else {
                    return Ok(None);
                };
                (*dst, *src, op.mnemonic(), None, width)
            }
            (
                LocalKind::MultiplyArithmetic,
                MInst::Binary {
                    op: BinOp::Mul,
                    dst,
                    lhs,
                    width,
                    ..
                },
            ) => (*dst, *lhs, "mul", None, *width),
            _ => return Ok(None),
        };
        let Some(tail) = consumer(second, temporary, compare) else {
            return Ok(None);
        };
        if !pattern.ops.contains(&tail.op) {
            return Ok(None);
        }
        if tail.width != width
            || !pattern.widths.contains(&u32::from(width.bits()))
            || temporary.class != width.class()
            || tail.other.class != width.class()
            || tail.dst.is_some_and(|dst| dst.class != width.class())
        {
            return Err("width-mismatch");
        }
        self.single_use(temporary)?;
        self.input_survives(tail.other, &[temporary])?;
        if let Some(count) = amount {
            if source.class != width.class() || !super::encodings::shift_immediate(count, width) {
                return Err("shift-domain");
            }
        } else if matches!(
            pattern.kind,
            LocalKind::ExtendedArithmetic | LocalKind::ExtendedCompare
        ) {
            if source.class != RegClass::Gpr32 {
                return Err("extension-source-class");
            }
            if matches!(
                first,
                MInst::Extend {
                    op: ExtOp::Sxtw,
                    ..
                }
            ) && width != Width::X
            {
                return Err("extension-width");
            }
        }
        let form = if let MInst::Binary {
            op: BinOp::Mul,
            rhs,
            ..
        } = first
        {
            if source.class != width.class() || rhs.class != width.class() {
                return Err("width-mismatch");
            }
            Form::Multiply {
                op: if tail.op == "sub" { "msub" } else { "madd" },
                dst: tail.dst.unwrap(),
                lhs: source,
                rhs: *rhs,
                addend: tail.other,
                width,
            }
        } else {
            Form::Alu {
                op: tail.op,
                dst: tail.dst,
                lhs: tail.other,
                rhs: source,
                width,
                rhs_class: if amount.is_some() {
                    width.class()
                } else {
                    RegClass::Gpr32
                },
                modifier,
                amount,
            }
        };
        Ok(Some(Selection {
            consumed: 2,
            pattern: pattern.id,
            form,
        }))
    }

    fn memory(
        &self,
        pattern: &LocalPattern,
        instructions: &[MInst],
        strict_alignment: bool,
    ) -> Result<Option<Selection>, &'static str> {
        let (add, access, shift) = match instructions {
            [MInst::BinaryImm {
                op: BinOp::Lsl,
                dst,
                lhs,
                imm,
                width: Width::X,
            }, add, access, ..] => (add, access, Some((*dst, *lhs, *imm))),
            [add, access, ..] => (add, access, None),
            _ => return Ok(None),
        };
        let MInst::Binary {
            op,
            dst: address,
            lhs,
            rhs,
            width: Width::X,
        } = add
        else {
            return Ok(None);
        };
        if !pattern.ops.contains(&op.mnemonic()) || !pattern.widths.contains(&64) {
            return Ok(None);
        }
        let (memory_base, size, align, offset, volatile, value, store) = match access {
            MInst::Load {
                base,
                kind,
                align,
                offset,
                volatile,
                dst,
                mask: _,
            } if matches!(
                kind,
                crate::LoadKind::B
                    | crate::LoadKind::H
                    | crate::LoadKind::W
                    | crate::LoadKind::X
                    | crate::LoadKind::P
                    | crate::LoadKind::Pf
                    | crate::LoadKind::Sb
                    | crate::LoadKind::Sh
                    | crate::LoadKind::Sw
            ) && !dst.class.is_simd() =>
            {
                (
                    *base,
                    kind.size_bytes(),
                    *align,
                    *offset,
                    *volatile,
                    *dst,
                    false,
                )
            }
            MInst::Store {
                base,
                kind,
                align,
                offset,
                volatile,
                src,
                mask: _,
            } if matches!(
                kind,
                crate::StoreKind::B
                    | crate::StoreKind::H
                    | crate::StoreKind::W
                    | crate::StoreKind::X
                    | crate::StoreKind::P
            ) && !src.class.is_simd() =>
            {
                (
                    *base,
                    kind.size_bytes(),
                    *align,
                    *offset,
                    *volatile,
                    *src,
                    true,
                )
            }
            _ => return Ok(None),
        };
        if memory_base != *address {
            return Ok(None);
        }
        if volatile || offset != 0 || size > 8 {
            return Err("memory-boundary");
        }
        if strict_alignment && align != 0 && u64::from(align) < size {
            return Err("strict-alignment-expansion");
        }
        if [*address, *lhs, *rhs]
            .iter()
            .any(|r| r.class != RegClass::Gpr64)
        {
            return Err("address-width");
        }
        self.single_use(*address)?;
        let (base, index, amount) = if let Some((temporary, source, amount)) = shift {
            let base = if *rhs == temporary {
                *lhs
            } else if *lhs == temporary {
                *rhs
            } else {
                return Ok(None);
            };
            self.single_use(temporary)?;
            self.input_survives(base, &[temporary])?;
            if source.class != RegClass::Gpr64
                || temporary.class != RegClass::Gpr64
                || !pattern.shifts.contains(&"lsl")
                || (amount != 0 && amount != u64::from(size.trailing_zeros()))
            {
                return Err("indexed-shift-domain");
            }
            if store {
                self.input_survives(value, &[temporary, *address])?;
            }
            (base, source, amount)
        } else {
            if store {
                self.input_survives(value, &[*address])?;
            }
            (*lhs, *rhs, 0)
        };
        Ok(Some(Selection {
            consumed: if shift.is_some() { 3 } else { 2 },
            pattern: pattern.id,
            form: Form::Memory {
                access: access.clone(),
                base,
                index,
                shift: amount,
            },
        }))
    }
}

impl Selection {
    pub fn emit(&self, text: &mut String, spell: &impl Fn(VReg, RegClass) -> String) {
        match &self.form {
            Form::Alu {
                op,
                dst,
                lhs,
                rhs,
                width,
                rhs_class,
                modifier,
                amount,
            } => {
                let destination = dst.map_or_else(String::new, |dst| {
                    format!("{}, ", spell(dst, width.class()))
                });
                let amount = amount.map_or_else(String::new, |n| format!(" #{n}"));
                let _ = writeln!(
                    text,
                    "    {op} {destination}{}, {}, {modifier}{amount}",
                    spell(*lhs, width.class()),
                    spell(*rhs, *rhs_class)
                );
            }
            Form::Multiply {
                op,
                dst,
                lhs,
                rhs,
                addend,
                width,
            } => {
                let _ = writeln!(
                    text,
                    "    {op} {}, {}, {}, {}",
                    spell(*dst, width.class()),
                    spell(*lhs, width.class()),
                    spell(*rhs, width.class()),
                    spell(*addend, width.class())
                );
            }
            Form::Memory {
                access,
                base,
                index,
                shift,
            } => {
                let (op, value, class) = match access {
                    MInst::Load { dst, kind, .. } => (
                        crate::emit::load_mnemonic(*kind),
                        *dst,
                        crate::emit::load_class(*kind, dst.class),
                    ),
                    MInst::Store { src, kind, .. } => (
                        crate::emit::store_mnemonic(*kind),
                        *src,
                        crate::emit::store_class(*kind),
                    ),
                    _ => unreachable!("admitted scalar memory form"),
                };
                let suffix = if *shift == 0 {
                    String::new()
                } else {
                    format!(", lsl #{shift}")
                };
                let _ = writeln!(
                    text,
                    "    {op} {}, [{}, {}{suffix}]",
                    spell(value, class),
                    spell(*base, RegClass::Gpr64),
                    spell(*index, RegClass::Gpr64)
                );
            }
        }
    }
}
