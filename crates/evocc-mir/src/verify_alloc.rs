//! Export of the actual allocator input and output for the independent Lean
//! allocation checker (work package V03).
//!
//! The export has three parts. The first part is the post-SSA program that
//! the allocator receives, in the `core-mir-v1` format of
//! `core_machine_export`. The second part is the allocated program. Each
//! virtual register operand is written as its assigned location. The export
//! does not drop, merge or reorder instructions. The third part is one
//! proposed abstract state for each block entry. These hints are untrusted:
//! the Lean checker checks them on every edge. The export is not a proof.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::core_machine_export::{export_core_machine, CoreMachineMode};
use crate::generic::allocation::AllocationOptions;
use crate::{
    allocate_for_with_options, inst_defs, inst_uses, term_uses, verify_allocation_for, Allocated,
    LoadKind, MFunction, MInst, MModule, MTerm, RegClass, StoreKind, Target, VReg, Width,
};

type Result<T> = std::result::Result<T, String>;

/// A storage place of one activation, as in `lean/EvoIR/Alloc/Syntax.lean`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Place {
    Gpr(u8),
    Flags,
    Spill(u32),
    Incoming(u32),
    Outgoing(u32),
}

/// An operand location. `view` is `None` only for the flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Loc {
    place: Place,
    view: Option<Width>,
}

impl Loc {
    fn width(self) -> Width {
        self.view.unwrap_or(Width::X)
    }

    fn at(place: Place, class: RegClass) -> Option<Self> {
        let view = match class {
            RegClass::Gpr32 => Some(Width::W),
            RegClass::Gpr64 => Some(Width::X),
            RegClass::Flags => None,
            _ => return None,
        };
        match (place, view) {
            (Place::Flags, None) => Some(Self { place, view }),
            (Place::Flags, Some(_)) | (_, None) => None,
            _ => Some(Self { place, view }),
        }
    }

    fn fits(self, class: RegClass) -> bool {
        Self::at(self.place, class) == Some(self)
    }
}

fn place_node(place: Place) -> String {
    match place {
        Place::Gpr(number) => format!("[\"gpr\",{number}]"),
        Place::Flags => "[\"flags\"]".to_owned(),
        Place::Spill(slot) => format!("[\"spill\",{slot}]"),
        Place::Incoming(slot) => format!("[\"incoming\",{slot}]"),
        Place::Outgoing(slot) => format!("[\"outgoing\",{slot}]"),
    }
}

fn loc_node(loc: Loc) -> String {
    let view = loc.width().mnemonic();
    match loc.place {
        Place::Gpr(number) => format!("[\"gpr\",{number},\"{view}\"]"),
        Place::Flags => "[\"flags\"]".to_owned(),
        Place::Spill(slot) => format!("[\"spill\",{slot},\"{view}\"]"),
        Place::Incoming(slot) => format!("[\"incoming\",{slot},\"{view}\"]"),
        Place::Outgoing(slot) => format!("[\"outgoing\",{slot},\"{view}\"]"),
    }
}

fn class_name(class: RegClass) -> Result<&'static str> {
    Ok(match class {
        RegClass::Gpr32 => "gpr32",
        RegClass::Gpr64 => "gpr64",
        RegClass::Flags => "flags",
        _ => return Err("allocation snapshot contains a class outside the Core profile".into()),
    })
}

fn reg_node(reg: VReg) -> Result<String> {
    Ok(format!("[{},\"{}\"]", reg.id, class_name(reg.class)?))
}

fn text(value: &str) -> String {
    let mut out = String::from("\"");
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
    out
}

/// The location of an allocated operand.
struct Locations<'a> {
    allocated: &'a Allocated,
}

impl Locations<'_> {
    fn of(&self, reg: VReg) -> Result<Loc> {
        let view = match reg.class {
            RegClass::Gpr32 => Width::W,
            RegClass::Gpr64 => Width::X,
            RegClass::Flags => {
                return Ok(Loc {
                    place: Place::Flags,
                    view: None,
                })
            }
            _ => return Err("allocated operand outside the Core profile".into()),
        };
        let number = self
            .allocated
            .assignment
            .get(&reg.id)
            .copied()
            .ok_or_else(|| format!("allocated operand %v{} has no register", reg.id))?;
        Ok(Loc {
            place: Place::Gpr(number),
            view: Some(view),
        })
    }

    fn node(&self, reg: VReg) -> Result<String> {
        Ok(loc_node(self.of(reg)?))
    }

    fn nodes(&self, regs: &[VReg]) -> Result<String> {
        let parts = regs
            .iter()
            .map(|reg| self.node(*reg))
            .collect::<Result<Vec<_>>>()?;
        Ok(format!("[{}]", parts.join(",")))
    }
}

fn slot_loc(place: Place, view: Width) -> Loc {
    Loc {
        place,
        view: Some(view),
    }
}

/// The copy that an inserted instruction performs: (destination, source).
fn copy_of(inst: &MInst, locs: &Locations<'_>) -> Result<Option<(Loc, Loc)>> {
    Ok(match inst {
        MInst::Mov { dst, src } => Some((locs.of(*dst)?, locs.of(*src)?)),
        MInst::SpillStore { src, slot } => {
            let src = locs.of(*src)?;
            Some((slot_loc(Place::Spill(*slot), src.width()), src))
        }
        MInst::SpillReload { dst, slot } => {
            let dst = locs.of(*dst)?;
            Some((dst, slot_loc(Place::Spill(*slot), dst.width())))
        }
        MInst::IncomingArg { dst, slot } => {
            let dst = locs.of(*dst)?;
            Some((dst, slot_loc(Place::Incoming(*slot), dst.width())))
        }
        MInst::OutgoingArg { src, slot } => {
            let src = locs.of(*src)?;
            Some((slot_loc(Place::Outgoing(*slot), src.width()), src))
        }
        _ => None,
    })
}

fn load_kind(kind: LoadKind) -> Result<&'static str> {
    match kind {
        LoadKind::W => Ok("w"),
        LoadKind::X => Ok("x"),
        _ => Err("allocated load outside the Core profile".into()),
    }
}

fn store_kind(kind: StoreKind) -> Result<&'static str> {
    match kind {
        StoreKind::W => Ok("w"),
        StoreKind::X => Ok("x"),
        _ => Err("allocated store outside the Core profile".into()),
    }
}

fn instruction(inst: &MInst, locs: &Locations<'_>) -> Result<String> {
    let l = |reg: &VReg| locs.node(*reg);
    Ok(match inst {
        MInst::Mov { dst, src } => format!("[\"mov\",{},{}]", l(dst)?, l(src)?),
        MInst::MovImm { dst, imm } => format!("[\"mov_imm\",{},{imm}]", l(dst)?),
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        } => format!(
            "[\"binary\",\"{}\",{},{},{},\"{}\"]",
            op.mnemonic(),
            l(dst)?,
            l(lhs)?,
            l(rhs)?,
            width.mnemonic()
        ),
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        } => format!(
            "[\"binary_imm\",\"{}\",{},{},{imm},\"{}\"]",
            op.mnemonic(),
            l(dst)?,
            l(lhs)?,
            width.mnemonic()
        ),
        MInst::FlagBinary {
            op,
            dst,
            flags,
            lhs,
            rhs,
            width,
        } => format!(
            "[\"flag_binary\",\"{}\",{},{},{},{},\"{}\"]",
            op.mnemonic(),
            l(dst)?,
            l(flags)?,
            l(lhs)?,
            l(rhs)?,
            width.mnemonic()
        ),
        MInst::WideMultiply { op, dst, lhs, rhs } => format!(
            "[\"wide_multiply\",\"{}\",{},{},{}]",
            op.mnemonic(),
            l(dst)?,
            l(lhs)?,
            l(rhs)?
        ),
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width,
        }
        | MInst::Madd {
            dst,
            n,
            m,
            a,
            width,
        } => format!(
            "[\"{}\",{},{},{},{},\"{}\"]",
            if matches!(inst, MInst::Msub { .. }) {
                "msub"
            } else {
                "madd"
            },
            l(dst)?,
            l(n)?,
            l(m)?,
            l(a)?,
            width.mnemonic()
        ),
        MInst::Extend { op, dst, src } => {
            format!("[\"extend\",\"{}\",{},{}]", op.mnemonic(), l(dst)?, l(src)?)
        }
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            width,
        } => format!(
            "[\"bfx\",{signed},{},{},{lsb},{bits},\"{}\"]",
            l(dst)?,
            l(src)?,
            width.mnemonic()
        ),
        MInst::Cmp {
            dst,
            lhs,
            rhs,
            width,
        } => format!(
            "[\"cmp\",{},{},{},\"{}\"]",
            l(dst)?,
            l(lhs)?,
            l(rhs)?,
            width.mnemonic()
        ),
        MInst::CmpImm {
            dst,
            lhs,
            imm,
            width,
        } => format!(
            "[\"cmp_imm\",{},{},{imm},\"{}\"]",
            l(dst)?,
            l(lhs)?,
            width.mnemonic()
        ),
        MInst::TstImm {
            dst,
            lhs,
            imm,
            width,
        } => format!(
            "[\"tst_imm\",{},{},{imm},\"{}\"]",
            l(dst)?,
            l(lhs)?,
            width.mnemonic()
        ),
        MInst::Cset { dst, flags, cond } => format!(
            "[\"cset\",{},{},\"{}\"]",
            l(dst)?,
            l(flags)?,
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
            "[\"csel\",{},{},{},{},\"{}\",\"{}\"]",
            l(dst)?,
            l(flags)?,
            l(lhs)?,
            l(rhs)?,
            cond.mnemonic(),
            width.mnemonic()
        ),
        MInst::Load {
            align,
            dst,
            base,
            offset,
            kind,
            volatile,
        } => format!(
            "[\"load\",{align},{},{},{offset},\"{}\",{volatile}]",
            l(dst)?,
            l(base)?,
            load_kind(*kind)?
        ),
        MInst::Store {
            align,
            src,
            base,
            offset,
            kind,
            volatile,
        } => format!(
            "[\"store\",{align},{},{},{offset},\"{}\",{volatile}]",
            l(src)?,
            l(base)?,
            store_kind(*kind)?
        ),
        MInst::FrameSlot {
            dst,
            origin,
            size,
            align,
        } => format!("[\"frame_slot\",{},{origin},{size},{align}]", l(dst)?),
        MInst::StackRelease { address } => format!("[\"stack_release\",{}]", l(address)?),
        MInst::Bl {
            symbol,
            args,
            results,
            fp_mode: None,
        } => format!(
            "[\"bl\",{},{},{}]",
            text(symbol),
            locs.nodes(args)?,
            locs.nodes(results)?
        ),
        MInst::SpillStore { src, slot } => format!("[\"spill_store\",{slot},{}]", l(src)?),
        MInst::SpillReload { dst, slot } => format!("[\"spill_reload\",{},{slot}]", l(dst)?),
        MInst::IncomingArg { dst, slot } => format!("[\"incoming_arg\",{},{slot}]", l(dst)?),
        MInst::OutgoingArg { src, slot } => format!("[\"outgoing_arg\",{slot},{}]", l(src)?),
        _ => return Err("allocated instruction outside the Core profile".into()),
    })
}

fn no_edge_arguments(args: &[&[VReg]]) -> Result<()> {
    if args.iter().any(|args| !args.is_empty()) {
        return Err("allocated edge has arguments".into());
    }
    Ok(())
}

fn terminator(term: &MTerm, locs: &Locations<'_>) -> Result<String> {
    Ok(match term {
        MTerm::B { target, args } => {
            no_edge_arguments(&[args])?;
            format!("[\"b\",{target}]")
        }
        MTerm::BCond {
            flags,
            cond,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => {
            no_edge_arguments(&[taken_args, untaken_args])?;
            format!(
                "[\"b_cond\",{},\"{}\",{taken},{untaken}]",
                locs.node(*flags)?,
                cond.mnemonic()
            )
        }
        MTerm::Cb {
            nonzero,
            src,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => {
            no_edge_arguments(&[taken_args, untaken_args])?;
            format!("[\"cb\",{nonzero},{},{taken},{untaken}]", locs.node(*src)?)
        }
        MTerm::Tb {
            nonzero,
            src,
            bit,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => {
            no_edge_arguments(&[taken_args, untaken_args])?;
            format!(
                "[\"tb\",{nonzero},{},{bit},{taken},{untaken}]",
                locs.node(*src)?
            )
        }
        MTerm::Ret { values } => format!("[\"ret\",{}]", locs.nodes(values)?),
        MTerm::Brk { imm } => format!("[\"brk\",{imm}]"),
        _ => return Err("allocated terminator outside the Core profile".into()),
    })
}

fn term_targets(term: &MTerm) -> Vec<u32> {
    match term {
        MTerm::B { target, .. } => vec![*target],
        MTerm::BCond { taken, untaken, .. }
        | MTerm::Cb { taken, untaken, .. }
        | MTerm::Tb { taken, untaken, .. } => vec![*taken, *untaken],
        _ => Vec::new(),
    }
}

/// A proposed abstract state: each pair says that a place holds the value
/// of a virtual register of the program before allocation.
type Equations = BTreeSet<(Place, VReg)>;

fn kill(state: &mut Equations, place: Place) {
    state.retain(|(held, _)| *held != place);
}

fn forget(state: &mut Equations, reg: VReg) {
    state.retain(|(_, value)| value.id != reg.id);
}

fn define(state: &mut Equations, loc: Loc, reg: VReg) {
    kill(state, loc.place);
    forget(state, reg);
    state.insert((loc.place, reg));
}

fn copy(state: &mut Equations, dst: Loc, src: Loc) {
    let moved: Vec<_> = state
        .iter()
        .filter(|(place, reg)| *place == src.place && src.fits(reg.class) && dst.fits(reg.class))
        .map(|(_, reg)| (dst.place, *reg))
        .collect();
    kill(state, dst.place);
    state.extend(moved);
}

fn rename(state: &mut Equations, dst: VReg, src: VReg) {
    forget(state, dst);
    let joined: Vec<_> = state
        .iter()
        .filter(|(_, reg)| *reg == src)
        .map(|(place, _)| (*place, dst))
        .collect();
    state.extend(joined);
}

fn clobbered(place: Place) -> bool {
    match place {
        Place::Gpr(number) => number < 18,
        Place::Flags | Place::Outgoing(_) => true,
        Place::Spill(_) | Place::Incoming(_) => false,
    }
}

fn incoming_place(index: usize) -> Place {
    if index < 8 {
        Place::Gpr(u8::try_from(index).unwrap_or(u8::MAX))
    } else {
        Place::Incoming(u32::try_from(index - 8).unwrap_or(u32::MAX))
    }
}

/// The transfer of one block, in the order of the Lean `walk`. It is a
/// proposal aid only; the Lean checker repeats every step and check.
fn walk(pre: &[MInst], post: &[MInst], locs: &Locations<'_>, state: &mut Equations) -> Result<()> {
    let (mut i, mut j) = (0, 0);
    loop {
        if let Some(inst) = post.get(j) {
            if !matches!(inst, MInst::Mov { .. }) {
                if let Some((dst, src)) = copy_of(inst, locs)? {
                    copy(state, dst, src);
                    j += 1;
                    continue;
                }
            }
            if let MInst::Mov { dst: ld, src: ls } = inst {
                if let Some(MInst::Mov { dst: d, src: s }) = pre.get(i) {
                    let (ld, ls) = (locs.of(*ld)?, locs.of(*ls)?);
                    if d.class != s.class
                        && d.class != RegClass::Flags
                        && state.contains(&(ls.place, *s))
                        && ls.fits(s.class)
                        && ld.fits(d.class)
                    {
                        define(state, ld, *d);
                        i += 1;
                        j += 1;
                        continue;
                    }
                }
                let (dst, src) = copy_of(inst, locs)?.expect("move copies");
                copy(state, dst, src);
                j += 1;
                continue;
            }
        }
        if let Some(MInst::Mov { dst, src }) = pre.get(i) {
            if dst.class == src.class {
                rename(state, *dst, *src);
                i += 1;
                continue;
            }
        }
        match (pre.get(i), post.get(j)) {
            (None, None) => return Ok(()),
            (
                Some(MInst::Bl { results, .. }),
                Some(MInst::Bl {
                    results: allocated, ..
                }),
            ) => {
                state.retain(|(place, _)| !clobbered(*place));
                for reg in results {
                    forget(state, *reg);
                }
                for (reg, loc) in results.iter().zip(allocated) {
                    state.insert((locs.of(*loc)?.place, *reg));
                }
            }
            (Some(original), Some(inst)) => {
                for (reg, loc) in inst_defs(original).iter().zip(inst_defs(inst)) {
                    define(state, locs.of(loc)?, *reg);
                }
            }
            _ => return Err("allocated block does not align with its input".into()),
        }
        i += 1;
        j += 1;
    }
}

/// Live-in virtual register identities of each block before allocation.
fn live_in(function: &MFunction) -> BTreeMap<u32, BTreeSet<u32>> {
    let mut reads = BTreeMap::new();
    let mut writes = BTreeMap::new();
    for block in &function.blocks {
        let mut read = BTreeSet::new();
        let mut written = BTreeSet::new();
        for inst in &block.insts {
            for reg in inst_uses(inst) {
                if !written.contains(&reg.id) {
                    read.insert(reg.id);
                }
            }
            for reg in inst_defs(inst) {
                written.insert(reg.id);
            }
        }
        for reg in term_uses(&block.term) {
            if !written.contains(&reg.id) {
                read.insert(reg.id);
            }
        }
        reads.insert(block.id, read);
        writes.insert(block.id, written);
    }
    let mut live: BTreeMap<u32, BTreeSet<u32>> = function
        .blocks
        .iter()
        .map(|block| (block.id, BTreeSet::new()))
        .collect();
    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let mut entering = reads[&block.id].clone();
            for target in term_targets(&block.term) {
                for value in live.get(&target).into_iter().flatten() {
                    if !writes[&block.id].contains(value) {
                        entering.insert(*value);
                    }
                }
            }
            if live[&block.id] != entering {
                live.insert(block.id, entering);
                changed = true;
            }
        }
        if !changed {
            return live;
        }
    }
}

/// Propose one state for each block entry: the meet of all paths, kept only
/// for values that are live at the entry.
fn hints(pre: &MFunction, allocated: &Allocated) -> Result<BTreeMap<u32, Equations>> {
    let locs = Locations { allocated };
    let allocated_blocks: BTreeMap<u32, &[MInst]> = allocated
        .blocks
        .iter()
        .map(|block| (block.id, block.insts.as_slice()))
        .collect();
    let mut states: BTreeMap<u32, Equations> = BTreeMap::new();
    if let Some(entry) = pre.blocks.iter().find(|block| block.id == 0) {
        let mut state = Equations::new();
        for (index, reg) in entry.args.iter().enumerate() {
            state.insert((incoming_place(index), *reg));
        }
        states.insert(0, state);
    }
    for _ in 0..(pre.blocks.len() + 1) * 8 {
        let mut changed = false;
        for block in &pre.blocks {
            let Some(mut state) = states.get(&block.id).cloned() else {
                continue;
            };
            let post = allocated_blocks
                .get(&block.id)
                .ok_or("allocated block missing")?;
            walk(&block.insts, post, &locs, &mut state)?;
            for target in term_targets(&block.term) {
                let merged = match states.get(&target) {
                    Some(existing) => existing.intersection(&state).copied().collect(),
                    None => state.clone(),
                };
                if states.get(&target) != Some(&merged) {
                    states.insert(target, merged);
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let live = live_in(pre);
    Ok(states
        .into_iter()
        .map(|(block, state)| {
            let alive = live.get(&block).cloned().unwrap_or_default();
            let state = state
                .into_iter()
                .filter(|(_, reg)| alive.contains(&reg.id))
                .collect();
            (block, state)
        })
        .collect())
}

fn allocated_function(pre: &MFunction, allocated: &Allocated) -> Result<String> {
    let locs = Locations { allocated };
    let mut blocks = Vec::new();
    for block in &allocated.blocks {
        let insts = block
            .insts
            .iter()
            .map(|inst| instruction(inst, &locs))
            .collect::<Result<Vec<_>>>()?;
        blocks.push(format!(
            "[{},[{}],{}]",
            block.id,
            insts.join(","),
            terminator(&block.term, &locs)?
        ));
    }
    let mut proposed = Vec::new();
    for (block, state) in hints(pre, allocated)? {
        let pairs = state
            .iter()
            .map(|(place, reg)| Ok(format!("[{},{}]", place_node(*place), reg_node(*reg)?)))
            .collect::<Result<Vec<_>>>()?;
        proposed.push(format!("[{block},[{}]]", pairs.join(",")));
    }
    Ok(format!(
        "[{},[{}],[{}]]",
        text(&allocated.name),
        blocks.join(","),
        proposed.join(",")
    ))
}

/// Allocate every function of the supplied post-SSA module with the default
/// Linux options of the native path, run the existing allocation verifier,
/// and export the input, the output and the proposed block states.
pub fn export_core_allocation(post_ssa: &MModule) -> Result<String> {
    let pre = export_core_machine(post_ssa, CoreMachineMode::PostSsa)?;
    let mut functions = Vec::new();
    for function in &post_ssa.functions {
        let allocated =
            allocate_for_with_options(Target::Linux, function, AllocationOptions::default())
                .map_err(|detail| format!("@{}: {detail}", function.name))?;
        verify_allocation_for(Target::Linux, &allocated)
            .map_err(|refusal| format!("@{}: {}", function.name, refusal.detail))?;
        functions.push(allocated_function(function, &allocated)?);
    }
    Ok(format!(
        "[\"core-alloc-pair-v1\",{},[\"core-alloc-v1\",\"aarch64-unknown-linux-gnu\",\"checked\",[{}]]]\n",
        pre.trim_end(),
        functions.join(",")
    ))
}
