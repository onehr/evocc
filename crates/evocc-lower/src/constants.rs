//! Bounded integer facts over the completed lowering CFG. Only conditional
//! edges change: original evaluations, poison, UB checks and effects remain.

use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{
    inst_results, inst_uses, BitOp, CmpOp, GlobalObject, IndexSignedness, Inst, OffsetDirection,
    Value,
};
use evocc_types::{ByteOrder, EvoIRType};

use super::DraftBlock;

mod private;

// Empty is the unreachable/bottom fact, None is unknown/top. A union larger
// than eight values widens to top, bounding both storage and convergence.
type Fact<T = u128> = Option<Vec<T>>;

fn union<T: Clone + Ord>(into: &mut Fact<T>, other: &Fact<T>) -> bool {
    if into.is_none() || other.as_ref().is_some_and(Vec::is_empty) {
        return false;
    }
    let old = into.clone();
    match other {
        None => *into = None,
        Some(values) => {
            let current = into.as_mut().unwrap();
            current.extend_from_slice(values);
            current.sort_unstable();
            current.dedup();
            if current.len() > 8 {
                *into = None;
            }
        }
    }
    *into != old
}

fn width(value: &Value) -> Option<u16> {
    match value.ty {
        EvoIRType::Bits(n @ 1..=128) => Some(n),
        _ => None,
    }
}

fn mask(n: u16) -> u128 {
    u128::MAX >> (128 - n)
}

fn signed(raw: u128, n: u16) -> i128 {
    ((raw << (128 - n)) as i128) >> (128 - n)
}

fn transfer(inst: &Inst, facts: &[Fact]) -> Fact {
    // These are facts on successful completion of the ORIGINAL instruction,
    // not permission to replace it. Even x & 0 must still consume x (and
    // report poison), and every evaluation/guard/effect stays in the CFG.
    let absorbing = |a: &Value, b: &Value, value| {
        ![a, b]
            .iter()
            .any(|v| facts[v.id as usize].as_ref().is_some_and(Vec::is_empty))
            && [a, b]
                .iter()
                .any(|v| facts[v.id as usize].as_deref() == Some(&[value]))
    };
    match inst {
        Inst::And(_, a, b) | Inst::Mul(_, a, b) if absorbing(a, b, 0) => {
            return Some(vec![0]);
        }
        Inst::Or(v, a, b) if absorbing(a, b, mask(width(v)?)) => {
            return Some(vec![mask(width(v)?)]);
        }
        Inst::Sub(_, a, b) | Inst::Xor(_, a, b) if a == b => {
            return Some(
                if facts[a.id as usize].as_ref().is_some_and(Vec::is_empty) {
                    Vec::new()
                } else {
                    vec![0]
                },
            );
        }
        _ => {}
    }
    let unary = |a: &Value, f: &dyn Fn(u128) -> Option<u128>| -> Fact {
        facts[a.id as usize]
            .as_ref()?
            .iter()
            .map(|&v| f(v))
            .collect()
    };
    let binary = |a: &Value, b: &Value, f: &dyn Fn(u128, u128) -> Option<u128>| -> Fact {
        // A bottom operand means no execution has supplied this operation.
        if [a, b]
            .iter()
            .any(|v| facts[v.id as usize].as_ref().is_some_and(Vec::is_empty))
        {
            return Some(Vec::new());
        }
        let mut out = Vec::new();
        for &x in facts[a.id as usize].as_ref()? {
            for &y in facts[b.id as usize].as_ref()? {
                out.push(f(x, y)?);
            }
        }
        out.sort_unstable();
        out.dedup();
        (out.len() <= 8).then_some(out)
    };
    match inst {
        Inst::Const(v, raw) => Some(vec![u128::from(*raw) & mask(width(v)?)]),
        Inst::Add(v, a, b) => binary(a, b, &|x, y| Some(x.wrapping_add(y) & mask(width(v)?))),
        Inst::Sub(v, a, b) => binary(a, b, &|x, y| Some(x.wrapping_sub(y) & mask(width(v)?))),
        Inst::Mul(v, a, b) => binary(a, b, &|x, y| Some(x.wrapping_mul(y) & mask(width(v)?))),
        Inst::UDiv(_, a, b) => binary(a, b, &|x, y| x.checked_div(y)),
        Inst::URem(_, a, b) => binary(a, b, &|x, y| x.checked_rem(y)),
        Inst::SDiv(v, a, b) => binary(a, b, &|x, y| {
            let n = width(v)?;
            Some(signed(x, n).checked_div(signed(y, n))? as u128 & mask(n))
        }),
        Inst::SRem(v, a, b) => binary(a, b, &|x, y| {
            let n = width(v)?;
            Some(signed(x, n).checked_rem(signed(y, n))? as u128 & mask(n))
        }),
        Inst::And(_, a, b) => binary(a, b, &|x, y| Some(x & y)),
        Inst::Or(_, a, b) => binary(a, b, &|x, y| Some(x | y)),
        Inst::Xor(_, a, b) => binary(a, b, &|x, y| Some(x ^ y)),
        Inst::Shl(v, a, b) => binary(a, b, &|x, y| {
            (y < u128::from(width(v)?)).then(|| (x << y) & mask(width(v).unwrap()))
        }),
        Inst::LShr(v, a, b) => binary(a, b, &|x, y| (y < u128::from(width(v)?)).then(|| x >> y)),
        Inst::AShr(v, a, b) => binary(a, b, &|x, y| {
            let n = width(v)?;
            (y < u128::from(n)).then(|| (signed(x, n) >> y) as u128 & mask(n))
        }),
        Inst::ZExt(v, a, _) | Inst::Trunc(v, a, _) => unary(a, &|x| Some(x & mask(width(v)?))),
        Inst::SExt(v, a, _) => unary(a, &|x| Some(signed(x, width(a)?) as u128 & mask(width(v)?))),
        Inst::ICmp(_, op, a, b) if width(a).is_some() && width(a) == width(b) => {
            binary(a, b, &|x, y| {
                Some(u128::from(match op {
                    CmpOp::Eq => x == y,
                    CmpOp::Ne => x != y,
                    CmpOp::ULt => x < y,
                    CmpOp::ULe => x <= y,
                    CmpOp::SLt => signed(x, width(a)?) < signed(y, width(b)?),
                    CmpOp::SLe => signed(x, width(a)?) <= signed(y, width(b)?),
                }))
            })
        }
        Inst::BitOperation {
            result,
            operand,
            op,
        } => unary(operand, &|x| {
            let n = width(result)?;
            Some(match op {
                BitOp::LeadingZeros => u128::from(x.leading_zeros() - (128 - u32::from(n))),
                BitOp::TrailingZeros => u128::from(x.trailing_zeros().min(u32::from(n))),
                BitOp::Population => u128::from(x.count_ones()),
                BitOp::ByteSwap => x.swap_bytes() >> (128 - n),
            })
        }),
        // Loads, calls, poison and proofs never acquire facts by assumption.
        _ => None,
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Address {
    symbol: String,
    offset: u64,
}

fn address_transfer(
    inst: &Inst,
    facts: &[Fact],
    addresses: &[Fact<Address>],
    globals: &BTreeMap<String, GlobalObject>,
) -> Fact<Address> {
    match inst {
        Inst::GlobalAddr { symbol, .. } if globals.contains_key(symbol) => Some(vec![Address {
            symbol: symbol.clone(),
            offset: 0,
        }]),
        Inst::PointerCast { operand, .. } => addresses[operand.id as usize].clone(),
        Inst::PtrOffset {
            base,
            index,
            scale,
            signedness,
            direction,
            ..
        } => {
            let bases = &addresses[base.id as usize];
            let offsets = &facts[index.id as usize];
            if bases.as_ref().is_some_and(Vec::is_empty)
                || offsets.as_ref().is_some_and(Vec::is_empty)
            {
                return Some(Vec::new());
            }
            let mut result = Vec::new();
            for base in bases.as_ref()? {
                for &raw in offsets.as_ref()? {
                    let delta = match signedness {
                        IndexSignedness::Signed => signed(raw, width(index)?),
                        IndexSignedness::Unsigned => i128::try_from(raw).ok()?,
                    }
                    .checked_mul(i128::from(*scale))?;
                    let offset = match direction {
                        OffsetDirection::Add => i128::from(base.offset).checked_add(delta)?,
                        OffsetDirection::Subtract => i128::from(base.offset).checked_sub(delta)?,
                    };
                    result.push(Address {
                        symbol: base.symbol.clone(),
                        offset: u64::try_from(offset).ok()?,
                    });
                }
            }
            result.sort_unstable();
            result.dedup();
            (result.len() <= 8).then_some(result)
        }
        _ => None,
    }
}

// A successful original load from an immutable source definition has these
// bits. Do not remove the load, its bounds/alignment guard or its effects;
// in particular pointer relocation bytes cannot become numeric constants.
fn immutable_load(
    inst: &Inst,
    addresses: &[Fact<Address>],
    globals: &BTreeMap<String, GlobalObject>,
    order: ByteOrder,
) -> Fact {
    let Inst::Load {
        result,
        address,
        volatile: false,
        ..
    } = inst
    else {
        return None;
    };
    let bits = width(result)?;
    let size = usize::from(bits).div_ceil(8);
    let mut values = Vec::new();
    for address in addresses[address.id as usize].as_ref()? {
        let global = globals.get(&address.symbol)?;
        let start = usize::try_from(address.offset).ok()?;
        let end = start.checked_add(size)?;
        if !global.initialized.get(start..end)?.iter().all(|&bit| bit)
            || global.relocs.iter().any(|reloc| {
                reloc.offset < end as u64 && reloc.offset.saturating_add(8) > address.offset
            })
        {
            return None;
        }
        let mut raw = 0u128;
        for (i, &byte) in global.bytes.get(start..end)?.iter().enumerate() {
            let shift = if order == ByteOrder::Little {
                i
            } else {
                size - 1 - i
            };
            raw |= u128::from(byte) << (shift * 8);
        }
        values.push(raw & mask(bits));
    }
    values.sort_unstable();
    values.dedup();
    Some(values)
}

pub(super) fn prune_constant_edges(
    blocks: &mut [DraftBlock],
    address_roots: &BTreeSet<u32>,
    globals: &BTreeMap<String, GlobalObject>,
    order: ByteOrder,
) {
    // Address identities retain their explicit CFG reachability under §2.5.
    // Do not prune their entry paths using this scalar-only analysis.
    if blocks.is_empty() || !address_roots.is_empty() {
        return;
    }
    let indices: BTreeMap<_, _> = blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect();
    let count = blocks
        .iter()
        .flat_map(|b| {
            b.args.iter().map(|(_, a)| a.value.id).chain(
                b.insts
                    .iter()
                    .chain(b.terminator.iter())
                    .flat_map(inst_results)
                    .map(|v| v.id),
            )
        })
        .max()
        .map_or(0, |n| n as usize + 1);
    let mut facts = vec![Some(Vec::new()); count];
    let mut addresses = vec![Some(Vec::new()); count];
    let mut users = vec![BTreeSet::new(); count];
    for (index, block) in blocks.iter().enumerate() {
        for value in block
            .insts
            .iter()
            .chain(block.terminator.iter())
            .flat_map(inst_uses)
        {
            users[value.id as usize].insert(index);
        }
    }
    let mut reachable = BTreeSet::new();
    let mut pending = BTreeSet::new();
    for (i, block) in blocks.iter().enumerate() {
        if i == 0 || address_roots.contains(&block.id) {
            reachable.insert(i);
            pending.insert(i);
            for (_, arg) in &block.args {
                facts[arg.value.id as usize] = None;
                addresses[arg.value.id as usize] = None;
            }
        }
    }
    while let Some(index) = pending.pop_first() {
        if !reachable.contains(&index) {
            continue;
        }
        let block = &blocks[index];
        for inst in block.insts.iter().chain(block.terminator.iter()) {
            let value = if matches!(inst, Inst::Load { .. }) {
                immutable_load(inst, &addresses, globals, order)
            } else {
                transfer(inst, &facts)
            };
            let address = address_transfer(inst, &facts, &addresses, globals);
            for result in inst_results(inst) {
                let bits = if width(result).is_some() {
                    &value
                } else {
                    &None
                };
                if union(&mut facts[result.id as usize], bits) {
                    pending.extend(&users[result.id as usize]);
                }
                if union(&mut addresses[result.id as usize], &address) {
                    pending.extend(&users[result.id as usize]);
                }
            }
        }
        let mut edges = Vec::new();
        match &block.terminator {
            Some(Inst::Br(target, args)) => edges.push((*target, args)),
            Some(Inst::CondBr(condition, a, aa, b, ba)) => {
                let value = &facts[condition.id as usize];
                if value.as_ref().is_none_or(|v| v.iter().any(|&n| n != 0)) {
                    edges.push((*a, aa));
                }
                if value.as_ref().is_none_or(|v| v.contains(&0)) {
                    edges.push((*b, ba));
                }
            }
            Some(Inst::IndirectBr {
                edges: successors, ..
            }) => {
                edges.extend(successors.iter().map(|e| (e.target, &e.arguments)));
            }
            Some(Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            }) => {
                edges.extend(labels.iter().map(|l| (l.edge.target, &l.edge.arguments)));
                edges.push((fallthrough.target, &fallthrough.arguments));
            }
            _ => {}
        }
        for (target, args) in edges {
            let Some(&next) = indices.get(&target) else {
                continue;
            };
            if reachable.insert(next) {
                pending.insert(next);
            }
            for (value, (_, parameter)) in args.iter().zip(&blocks[next].args) {
                let incoming = facts[value.id as usize].clone();
                if union(&mut facts[parameter.value.id as usize], &incoming) {
                    pending.insert(next);
                    pending.extend(&users[parameter.value.id as usize]);
                }
                let incoming = addresses[value.id as usize].clone();
                if union(&mut addresses[parameter.value.id as usize], &incoming) {
                    pending.insert(next);
                    pending.extend(&users[parameter.value.id as usize]);
                }
            }
        }
    }
    for block in blocks.iter_mut() {
        if let Some(Inst::CondBr(condition, a, aa, b, ba)) = &block.terminator {
            if let Some(values) = &facts[condition.id as usize] {
                if !values.is_empty() && values.iter().all(|&v| v == 0) {
                    block.terminator = Some(Inst::Br(*b, ba.clone()));
                } else if !values.is_empty() && values.iter().all(|&v| v != 0) {
                    block.terminator = Some(Inst::Br(*a, aa.clone()));
                }
            }
        }
    }
    private::prune_guarded_reads(blocks, &facts, &indices, &users, &reachable);
}

// Delete only unused address construction and parameter transport. Every
// evaluation, poison-consuming operation, guard and effect stays in place.
fn discard_unused_addresses(function: &mut evocc_evoir::Function) {
    fn edges(inst: &mut Inst, mut visit: impl FnMut(u32, &mut Vec<Value>)) {
        match inst {
            Inst::Br(target, args) => visit(*target, args),
            Inst::CondBr(_, a, aa, b, ba) => {
                visit(*a, aa);
                visit(*b, ba);
            }
            Inst::IndirectBr { edges, .. } => {
                for edge in edges {
                    visit(edge.target, &mut edge.arguments);
                }
            }
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                for label in labels {
                    visit(label.edge.target, &mut label.edge.arguments);
                }
                visit(fallthrough.target, &mut fallthrough.arguments);
            }
            _ => {}
        }
    }
    let parameters: BTreeMap<_, _> = function
        .blocks
        .iter()
        .map(|b| {
            (
                b.id,
                b.args.iter().map(|a| a.value.clone()).collect::<Vec<_>>(),
            )
        })
        .collect();
    let mut incoming = BTreeMap::<u32, Vec<u32>>::new();
    let mut needed = BTreeSet::new();
    for block in &function.blocks {
        // Effect state must cross every edge, even when it only circulates
        // around a non-returning loop. Absence of a later read or return is
        // not permission to drop the current linear token.
        needed.extend(block.args.iter().filter_map(|arg| {
            matches!(arg.value.ty, EvoIRType::Mem(_) | EvoIRType::World).then_some(arg.value.id)
        }));
        for inst in block.insts.iter().chain(std::iter::once(&block.terminator)) {
            let mut without_edges = inst.clone();
            edges(&mut without_edges, |target, args| {
                for (param, arg) in parameters[&target].iter().zip(args.iter()) {
                    incoming.entry(param.id).or_default().push(arg.id);
                }
                args.clear();
            });
            needed.extend(inst_uses(&without_edges).iter().map(|v| v.id));
        }
    }
    // The entry signature is an ABI boundary, including an entry backedge.
    if let Some(entry) = function.blocks.first() {
        needed.extend(entry.args.iter().map(|a| a.value.id));
    }
    let mut pending: Vec<_> = needed.iter().copied().collect();
    while let Some(value) = pending.pop() {
        for &arg in incoming.get(&value).into_iter().flatten() {
            if needed.insert(arg) {
                pending.push(arg);
            }
        }
    }
    for block in &mut function.blocks {
        block.args.retain(|a| needed.contains(&a.value.id));
        for inst in block
            .insts
            .iter_mut()
            .chain(std::iter::once(&mut block.terminator))
        {
            edges(inst, |target, args| {
                let mut index = 0;
                args.retain(|_| {
                    let keep = needed.contains(&parameters[&target][index].id);
                    index += 1;
                    keep
                });
            });
        }
        block.insts.retain(|inst| match inst {
            Inst::GlobalAddr { result, .. } | Inst::FuncAddr { result, .. } => {
                needed.contains(&result.id)
            }
            _ => true,
        });
    }
}

/// Follow actual lowered uses after constant edges are removed. External
/// symbols, explicit sections, preservation attributes, aliases and assembly
/// are roots; an otherwise unused private table is not a root for its callbacks.
pub(super) fn remove_unreferenced_functions(
    module: &mut evocc_evoir::Module,
    preserved: BTreeSet<String>,
) {
    for function in &mut module.functions {
        discard_unused_addresses(function);
    }
    let functions: BTreeMap<_, _> = module
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
        .collect();
    let globals: BTreeMap<_, _> = module
        .globals
        .iter()
        .map(|g| (g.symbol.as_str(), g))
        .collect();
    let mut pending = preserved;
    pending.extend(
        module
            .functions
            .iter()
            .filter(|f| f.visibility != evocc_evoir::GlobalVisibility::Internal)
            .map(|f| f.name.clone()),
    );
    pending.extend(module.aliases.iter().map(|a| a.target.clone()));
    pending.extend(
        module
            .globals
            .iter()
            .filter(|g| {
                g.visibility != evocc_evoir::GlobalVisibility::Internal || g.section.is_some()
            })
            .map(|g| g.symbol.clone()),
    );
    let assembly_uses = |assembly: &str, pending: &mut BTreeSet<String>| {
        for name in
            assembly.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')))
        {
            if functions.contains_key(name) || globals.contains_key(name) {
                pending.insert(name.to_owned());
            }
            if let Some(name) = name
                .strip_prefix('_')
                .filter(|name| functions.contains_key(name) || globals.contains_key(name))
            {
                pending.insert(name.to_owned());
            }
        }
    };
    for assembly in &module.assembly {
        assembly_uses(assembly, &mut pending);
    }
    let mut retained = BTreeSet::new();
    while let Some(name) = pending.pop_first() {
        if !retained.insert(name.clone()) {
            continue;
        }
        if let Some(global) = globals.get(name.as_str()) {
            pending.extend(
                global
                    .relocs
                    .iter()
                    .map(|r| r.symbol.split('^').next().unwrap().to_owned()),
            );
        }
        let Some(function) = functions.get(name.as_str()) else {
            continue;
        };
        for instruction in function
            .blocks
            .iter()
            .flat_map(|b| b.insts.iter().chain(std::iter::once(&b.terminator)))
        {
            match instruction {
                Inst::Call { callee, .. } => {
                    pending.insert(callee.clone());
                }
                Inst::FuncAddr { symbol, .. } | Inst::GlobalAddr { symbol, .. } => {
                    pending.insert(symbol.clone());
                }
                Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => {
                    assembly_uses(&assembly.template, &mut pending)
                }
                _ => {}
            }
        }
    }
    module.functions.retain(|f| retained.contains(&f.name));
    module.globals.retain(|g| retained.contains(&g.symbol));
}
