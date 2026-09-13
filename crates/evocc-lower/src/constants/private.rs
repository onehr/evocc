//! Repeated tests of nonescaping local storage. A previous observed test can
//! exclude an edge only while the same bytes have not been written. Loads,
//! guards, calls and lifetime actions themselves are never removed.

use super::*;
use evocc_evoir::{IndexSignedness, OffsetDirection};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Point {
    Pending,
    Local(u32, i128),
    Unknown,
}

fn join(into: &mut Point, value: Point) -> bool {
    let next = match (*into, value) {
        (Point::Pending, value) => value,
        (old, Point::Pending) => old,
        (old, value) if old == value => old,
        _ => Point::Unknown,
    };
    let changed = next != *into;
    *into = next;
    changed
}

fn edges(block: &DraftBlock) -> Vec<(u32, &[Value])> {
    match &block.terminator {
        Some(Inst::Br(to, args)) => vec![(*to, args)],
        Some(Inst::CondBr(_, a, aa, b, ba)) => vec![(*a, aa), (*b, ba)],
        Some(Inst::IndirectBr { edges, .. }) => edges
            .iter()
            .map(|e| (e.target, e.arguments.as_slice()))
            .collect(),
        Some(Inst::AsmGoto {
            labels,
            fallthrough,
            ..
        }) => labels
            .iter()
            .map(|l| &l.edge)
            .chain([fallthrough])
            .map(|e| (e.target, e.arguments.as_slice()))
            .collect(),
        _ => Vec::new(),
    }
}

fn points(
    blocks: &[DraftBlock],
    facts: &[Fact],
    indices: &BTreeMap<u32, usize>,
    users: &[BTreeSet<usize>],
    reachable: &BTreeSet<usize>,
) -> Vec<Point> {
    let mut points = vec![Point::Pending; facts.len()];
    for (_, arg) in &blocks[0].args {
        points[arg.value.id as usize] = Point::Unknown;
    }
    let mut pending = reachable.clone();
    while let Some(index) = pending.pop_first() {
        if !reachable.contains(&index) {
            continue;
        }
        let block = &blocks[index];
        for inst in &block.insts {
            for result in inst_results(inst)
                .into_iter()
                .filter(|v| matches!(v.ty, EvoIRType::Ptr(_)))
            {
                let value = match inst {
                    Inst::Alloca { .. } => Point::Local(result.id, 0),
                    Inst::PointerCast { operand, .. } => points[operand.id as usize],
                    Inst::PtrOffset {
                        base,
                        index,
                        direction,
                        signedness,
                        scale,
                        ..
                    } => match points[base.id as usize] {
                        Point::Local(root, offset) => {
                            let displacement =
                                facts[index.id as usize].as_deref().and_then(|values| {
                                    let [raw] = values else { return None };
                                    let delta = match signedness {
                                        IndexSignedness::Signed => signed(*raw, width(index)?),
                                        IndexSignedness::Unsigned => i128::try_from(*raw).ok()?,
                                    }
                                    .checked_mul(i128::from(*scale))?;
                                    match direction {
                                        OffsetDirection::Add => offset.checked_add(delta),
                                        OffsetDirection::Subtract => offset.checked_sub(delta),
                                    }
                                });
                            displacement.map_or(Point::Unknown, |offset| Point::Local(root, offset))
                        }
                        other => other,
                    },
                    _ => Point::Unknown,
                };
                if join(&mut points[result.id as usize], value) {
                    pending.extend(&users[result.id as usize]);
                }
            }
        }
        for (target, args) in edges(block) {
            let Some(&next) = indices.get(&target) else {
                continue;
            };
            for (value, (_, param)) in args.iter().zip(&blocks[next].args) {
                if matches!(param.value.ty, EvoIRType::Ptr(_)) {
                    let incoming = points[value.id as usize];
                    if join(&mut points[param.value.id as usize], incoming) {
                        pending.insert(next);
                        pending.extend(&users[param.value.id as usize]);
                    }
                }
            }
        }
    }
    points
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Cell {
    object: u32,
    offset: i128,
    width: u16,
}

#[derive(Clone, Copy)]
struct Predicate {
    cell: Cell,
    value: u128,
    equal: bool,
}

struct BlockFacts {
    killed: BTreeSet<u32>,
    condition: Option<Predicate>,
}

pub(super) fn prune_guarded_reads(
    blocks: &mut [DraftBlock],
    facts: &[Fact],
    indices: &BTreeMap<u32, usize>,
    users: &[BTreeSet<usize>],
    reachable: &BTreeSet<usize>,
) {
    let points = points(blocks, facts, indices, users, reachable);
    let mut private: BTreeSet<_> = points
        .iter()
        .filter_map(|p| match p {
            Point::Local(root, _) => Some(*root),
            _ => None,
        })
        .collect();
    // An unknown offset/phi or any escape disqualifies the WHOLE object.
    // Otherwise an unknown pointer could alias a field we claim is private.
    for &index in reachable {
        let block = &blocks[index];
        for inst in &block.insts {
            for value in inst_uses(inst) {
                let Point::Local(root, _) = points[value.id as usize] else {
                    continue;
                };
                let safe = match inst {
                    Inst::PtrOffset { base, result, .. } => {
                        value == base && matches!(points[result.id as usize], Point::Local(_, _))
                    }
                    Inst::PointerCast { operand, result } => {
                        value == operand && matches!(points[result.id as usize], Point::Local(_, _))
                    }
                    Inst::PointerValid { pointer, .. }
                    | Inst::AccessInBounds { pointer, .. }
                    | Inst::LifetimeEnd { pointer, .. } => value == pointer,
                    Inst::Load {
                        address,
                        volatile: false,
                        ..
                    } => value == address,
                    Inst::Store {
                        address,
                        volatile: false,
                        value: stored,
                        ..
                    } => value == address && value != stored,
                    Inst::Copy {
                        destination,
                        source,
                        volatile: evocc_evoir::CopyVolatility::None,
                        ..
                    } => value == destination || value == source,
                    _ => false,
                };
                if !safe {
                    private.remove(&root);
                }
            }
        }
        let mut transported = BTreeSet::new();
        for (target, args) in edges(block) {
            let Some(&next) = indices.get(&target) else {
                continue;
            };
            for (value, (_, param)) in args.iter().zip(&blocks[next].args) {
                transported.insert(value.id);
                if let Point::Local(root, _) = points[value.id as usize] {
                    if !matches!(points[param.value.id as usize], Point::Local(_, _)) {
                        private.remove(&root);
                    }
                }
            }
        }
        // Only Br/CondBr are transparent transports here. An indirect/asm
        // control operand is not an approved way to expose private storage.
        if let Some(term) = &block.terminator {
            for value in inst_uses(term) {
                if !transported.contains(&value.id)
                    || !matches!(term, Inst::Br(..) | Inst::CondBr(..))
                {
                    if let Point::Local(root, _) = points[value.id as usize] {
                        private.remove(&root);
                    }
                }
            }
        }
    }
    if private.is_empty() {
        return;
    }
    let mut information = Vec::new();
    for block in blocks.iter() {
        let mut killed = BTreeSet::new();
        let mut loads = BTreeMap::<u32, Cell>::new();
        let mut predicates = BTreeMap::<u32, Predicate>::new();
        for inst in block.insts.iter().chain(block.terminator.iter()) {
            let writes = match inst {
                Inst::Alloca { result, .. } => Some(result),
                Inst::Store { address, .. } => Some(address),
                Inst::Copy { destination, .. } => Some(destination),
                Inst::LifetimeEnd { pointer, .. } => Some(pointer),
                _ => None,
            };
            if let Some(Point::Local(root, _)) = writes.map(|v| points[v.id as usize]) {
                killed.insert(root);
                loads.retain(|_, cell| cell.object != root);
                predicates.retain(|_, p| p.cell.object != root);
            }
            if matches!(inst, Inst::Asm(a) | Inst::AsmGoto { assembly: a, .. } if !a.template.trim().is_empty())
            {
                killed.extend(&private);
                loads.clear();
                predicates.clear();
            }
            match inst {
                Inst::Load {
                    result,
                    address,
                    volatile: false,
                    ..
                } => {
                    if let Point::Local(object, offset) = points[address.id as usize] {
                        if private.contains(&object) {
                            if let Some(width) = width(result) {
                                loads.insert(
                                    result.id,
                                    Cell {
                                        object,
                                        offset,
                                        width,
                                    },
                                );
                            }
                        }
                    }
                }
                Inst::ICmp(result, op @ (CmpOp::Eq | CmpOp::Ne), a, b) => {
                    for (input, constant) in [(a, b), (b, a)] {
                        let Some([value]) = facts[constant.id as usize].as_deref() else {
                            continue;
                        };
                        let predicate = loads
                            .get(&input.id)
                            .map(|&cell| Predicate {
                                cell,
                                value: *value,
                                equal: *op == CmpOp::Eq,
                            })
                            .or_else(|| {
                                predicates.get(&input.id).filter(|_| *value <= 1).map(|p| {
                                    Predicate {
                                        equal: p.equal == ((*value == 1) == (*op == CmpOp::Eq)),
                                        ..*p
                                    }
                                })
                            });
                        if let Some(p) = predicate {
                            predicates.insert(result.id, p);
                        }
                    }
                }
                Inst::ZExt(result, operand, _) | Inst::Trunc(result, operand, _) => {
                    if let Some(p) = predicates.get(&operand.id).copied() {
                        predicates.insert(result.id, p);
                    }
                }
                _ => {}
            }
        }
        let condition = match &block.terminator {
            Some(Inst::CondBr(value, ..)) => predicates.get(&value.id).copied(),
            _ => None,
        };
        information.push(BlockFacts { killed, condition });
    }
    let mut predecessors = vec![Vec::new(); blocks.len()];
    for (parent, block) in blocks.iter().enumerate() {
        // Lowering can leave unreachable continuation blocks after break or
        // return. Their synthetic edges are not possible execution paths.
        if !reachable.contains(&parent) {
            continue;
        }
        for (edge, (target, _)) in edges(block).into_iter().enumerate() {
            if let Some(&next) = indices.get(&target) {
                predecessors[next].push((parent, edge == 0));
            }
        }
    }
    // Demand-driven available-predicate query, not whole-function memory SSA.
    // Stop at every write, entry or conflicting observation. A work bound can
    // only decline pruning; it can never invent a result or remove a guard.
    let mut budget = 100_000usize;
    for (index, block) in blocks.iter_mut().enumerate() {
        if !reachable.contains(&index) {
            continue;
        }
        let Some(wanted) = information[index].condition else {
            continue;
        };
        let Some(answer) =
            prior_observation(index, wanted, &information, &predecessors, &mut budget)
        else {
            continue;
        };
        if let Some(Inst::CondBr(_, a, aa, b, ba)) = &block.terminator {
            block.terminator = Some(if answer {
                Inst::Br(*a, aa.clone())
            } else {
                Inst::Br(*b, ba.clone())
            });
        }
    }
}

fn prior_observation(
    index: usize,
    wanted: Predicate,
    info: &[BlockFacts],
    predecessors: &[Vec<(usize, bool)>],
    budget: &mut usize,
) -> Option<bool> {
    if index == 0 || info[index].killed.contains(&wanted.cell.object) {
        return None;
    }
    let mut pending = vec![index];
    let mut seen = BTreeSet::new();
    let mut answer = None;
    while let Some(index) = pending.pop() {
        if !seen.insert(index) {
            continue;
        }
        if predecessors[index].is_empty() {
            return None;
        }
        for &(parent, edge_true) in &predecessors[index] {
            *budget = budget.checked_sub(1)?;
            if let Some(p) = info[parent]
                .condition
                .filter(|p| p.cell == wanted.cell && p.value == wanted.value)
            {
                let observed = wanted.equal == (p.equal == edge_true);
                if answer.is_some_and(|previous| previous != observed) {
                    return None;
                }
                answer = Some(observed);
            } else {
                if parent == 0 || info[parent].killed.contains(&wanted.cell.object) {
                    return None;
                }
                pending.push(parent);
            }
        }
    }
    answer
}
