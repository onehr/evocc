//! Bounded integer facts over the completed lowering CFG. Only conditional
//! edges change: original evaluations, poison, UB checks and effects remain.

use std::collections::{BTreeMap, BTreeSet};

use evocc_evoir::{inst_results, inst_uses, BitOp, CmpOp, Inst, Value};
use evocc_types::EvoIRType;

use super::DraftBlock;

// Empty is the unreachable/bottom fact, None is unknown/top. A union larger
// than eight values widens to top, bounding both storage and convergence.
type Fact = Option<Vec<u128>>;

fn union(into: &mut Fact, other: &Fact) -> bool {
    if into.is_none() || other.as_ref().is_some_and(Vec::is_empty) {
        return false;
    }
    let old = into.clone();
    match other {
        None => *into = None,
        Some(values) => {
            let current = into.as_mut().unwrap();
            current.extend(values);
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

pub(super) fn prune_constant_edges(blocks: &mut [DraftBlock], address_roots: &BTreeSet<u32>) {
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
            }
        }
    }
    while let Some(index) = pending.pop_first() {
        if !reachable.contains(&index) {
            continue;
        }
        let block = &blocks[index];
        for inst in block.insts.iter().chain(block.terminator.iter()) {
            let value = transfer(inst, &facts);
            for result in inst_results(inst) {
                if union(&mut facts[result.id as usize], &value) {
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
            }
        }
    }
    for block in blocks {
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
}

/// Follow actual lowered uses after constant edges are removed. Linkage,
/// relocations (including block addresses), aliases and assembly remain roots.
pub(super) fn remove_unreferenced_functions(
    module: &mut evocc_evoir::Module,
    preserved: BTreeSet<String>,
) {
    let functions: BTreeMap<_, _> = module
        .functions
        .iter()
        .map(|f| (f.name.as_str(), f))
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
            .flat_map(|g| &g.relocs)
            .map(|r| r.symbol.split('^').next().unwrap().to_owned()),
    );
    let assembly_uses = |assembly: &str, pending: &mut BTreeSet<String>| {
        for name in
            assembly.split(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$')))
        {
            if functions.contains_key(name) {
                pending.insert(name.to_owned());
            }
            if let Some(name) = name
                .strip_prefix('_')
                .filter(|name| functions.contains_key(name))
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
        let Some(function) = functions.get(name.as_str()) else {
            continue;
        };
        if !retained.insert(name) {
            continue;
        }
        for instruction in function.blocks.iter().flat_map(|b| &b.insts) {
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
}
