//! Join a straight-line continuation without changing any executed operation.
//! Source labels retain their identity, even if only a relocation names them.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use evocc_evoir::{remap_inst_uses, Inst};

use crate::{BlockId, DraftBlock, LowerError};

pub(super) fn join_linear_blocks(
    blocks: &mut Vec<DraftBlock>,
    labels: &BTreeSet<BlockId>,
) -> Result<(), LowerError> {
    let mut entries = BTreeMap::<BlockId, usize>::new();
    let indices: BTreeMap<_, _> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| (block.id, index))
        .collect();
    let mut protected = labels.clone();
    for block in blocks.iter() {
        for inst in &block.insts {
            if let Inst::BlockAddr { block, .. } = inst {
                protected.insert(*block);
            }
        }
        let targets = match &block.terminator {
            Some(Inst::Br(target, _)) => vec![*target],
            Some(Inst::CondBr(_, a, _, b, _)) => vec![*a, *b],
            Some(Inst::IndirectBr { edges, .. }) => edges.iter().map(|edge| edge.target).collect(),
            Some(Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            }) => labels
                .iter()
                .map(|label| label.edge.target)
                .chain([fallthrough.target])
                .collect(),
            _ => vec![],
        };
        // Count edges, not unique predecessor blocks: two alternatives into
        // one target still carry separate bindings.
        for target in targets {
            *entries.entry(target).or_default() += 1;
        }
    }
    let mut slots: Vec<_> = std::mem::take(blocks).into_iter().map(Some).collect();
    for source in 0..slots.len() {
        while let Some(head) = &slots[source] {
            let Some(Inst::Br(target, arguments)) = &head.terminator else {
                break;
            };
            if *target == head.id || protected.contains(target) || entries.get(target) != Some(&1) {
                break;
            }
            let Some(&destination) = indices.get(target) else {
                return Err(LowerError::InvalidGeneratedSsa {
                    message: "linear edge targets an unknown block".into(),
                });
            };
            if destination == 0 {
                break;
            }
            let tail =
                slots[destination]
                    .as_ref()
                    .ok_or_else(|| LowerError::InvalidGeneratedSsa {
                        message: "linear edge targets a removed block".into(),
                    })?;
            if arguments.len() != tail.args.len()
                || arguments
                    .iter()
                    .zip(&tail.args)
                    .any(|(value, (_, param))| value.ty != param.value.ty)
            {
                return Err(LowerError::InvalidGeneratedSsa {
                    message: "linear edge parameter types or arity disagree".into(),
                });
            }
            // The source definitions already exist. Simultaneous substitution
            // keeps token identity and does not read a transported object.
            let bindings: HashMap<_, _> = tail
                .args
                .iter()
                .zip(arguments)
                .map(|((_, param), value)| (param.value.id, value.clone()))
                .collect();
            let mut tail = slots[destination].take().unwrap();
            for inst in tail.insts.iter_mut().chain(tail.terminator.iter_mut()) {
                remap_inst_uses(inst, &bindings);
            }
            let head = slots[source].as_mut().unwrap();
            head.insts.extend(tail.insts);
            head.terminator = tail.terminator;
            // Every remaining outgoing edge is unchanged, so its entry count
            // stays valid. Only the removed continuation lost its sole edge.
        }
    }
    blocks.extend(slots.into_iter().flatten());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BlockRole, EnvKey};
    use evocc_evoir::{BlockArg, Edge, Value};
    use evocc_types::{AddressSpace, EvoIRType, Region};

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    fn block(id: u32, args: Vec<Value>, terminator: Inst) -> DraftBlock {
        DraftBlock {
            id,
            args: args
                .into_iter()
                .map(|value| {
                    (
                        EnvKey::Temporary(value.id),
                        BlockArg {
                            name: format!("arg{}", value.id),
                            value,
                        },
                    )
                })
                .collect(),
            insts: Vec::new(),
            terminator: Some(terminator),
            role: BlockRole::Ordinary,
        }
    }

    #[test]
    fn bindings_preserve_permutation_and_the_same_effect_state() {
        let types = [
            EvoIRType::Bits(32),
            EvoIRType::Bits(32),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ];
        let source: Vec<_> = types
            .iter()
            .enumerate()
            .map(|(i, ty)| value(i as u32, ty.clone()))
            .collect();
        let parameters: Vec<_> = types
            .iter()
            .enumerate()
            .map(|(i, ty)| value(10 + i as u32, ty.clone()))
            .collect();
        let mut incoming = source.clone();
        incoming.swap(0, 1);
        let mut blocks = vec![
            block(0, source, Inst::Br(1, incoming.clone())),
            block(1, parameters.clone(), Inst::Ret(parameters)),
        ];
        join_linear_blocks(&mut blocks, &BTreeSet::new()).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].terminator, Some(Inst::Ret(incoming)));
    }

    #[test]
    fn indirect_entries_and_address_only_labels_are_not_lost() {
        let address = value(0, EvoIRType::Ptr(AddressSpace::Generic));
        let mut blocks = vec![
            block(0, vec![], Inst::Br(2, vec![])),
            block(
                1,
                vec![],
                Inst::IndirectBr {
                    target: address,
                    edges: vec![Edge {
                        target: 2,
                        arguments: vec![],
                    }],
                },
            ),
            block(2, vec![], Inst::Ret(vec![])),
        ];
        join_linear_blocks(&mut blocks, &BTreeSet::new()).unwrap();
        assert_eq!(blocks.len(), 3);
        for protected in [BTreeSet::from([2]), BTreeSet::new()] {
            let mut blocks = vec![
                block(0, vec![], Inst::Br(2, vec![])),
                block(2, vec![], Inst::Ret(vec![])),
            ];
            if protected.is_empty() {
                blocks[0].insts.push(Inst::BlockAddr {
                    result: value(3, EvoIRType::Ptr(AddressSpace::Generic)),
                    block: 2,
                });
            }
            join_linear_blocks(&mut blocks, &protected).unwrap();
            assert_eq!(blocks.len(), 2);
        }
    }

    #[test]
    fn entry_self_edges_and_bad_bindings_are_not_silently_eliminated() {
        let mut loop_blocks = vec![block(0, vec![], Inst::Br(0, vec![]))];
        join_linear_blocks(&mut loop_blocks, &BTreeSet::new()).unwrap();
        assert_eq!(loop_blocks.len(), 1);
        assert_eq!(loop_blocks[0].terminator, Some(Inst::Br(0, vec![])));
        for args in [vec![], vec![value(0, EvoIRType::Bits(64))]] {
            let mut blocks = vec![
                block(0, vec![], Inst::Br(1, args)),
                block(1, vec![value(1, EvoIRType::Bits(32))], Inst::Ret(vec![])),
            ];
            assert!(join_linear_blocks(&mut blocks, &BTreeSet::new()).is_err());
        }
    }
}
