//! Lowering blocks.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn fresh_value(&mut self, ty: EvoIRType) -> Value {
        let value = Value {
            id: self.next_value,
            ty,
        };
        self.next_value += 1;
        value
    }

    pub(super) fn fresh_placeholder(&mut self) -> BlockId {
        let id = self.next_placeholder;
        self.next_placeholder += 1;
        id
    }

    pub(super) fn prune_unreachable_blocks(&mut self) -> Result<(), LowerError> {
        let mut reachable = BTreeSet::from([0]);
        reachable.extend(
            self.address_labels
                .keys()
                .filter_map(|name| self.labels.get(name).map(|label| label.id)),
        );
        let mut pending = reachable.iter().copied().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            let block = self.blocks.iter().find(|block| block.id == id).ok_or(
                LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "reachable edge targets an unknown draft block",
                },
            )?;
            let Some(terminator) = &block.terminator else {
                continue;
            };
            let targets = match terminator {
                Inst::IndirectBr { edges, .. } => edges.iter().map(|edge| edge.target).collect(),
                Inst::AsmGoto {
                    labels,
                    fallthrough,
                    ..
                } => labels
                    .iter()
                    .map(|label| label.edge.target)
                    .chain([fallthrough.target])
                    .collect(),
                Inst::Br(target, _) => vec![*target],
                Inst::CondBr(_, true_target, _, false_target, _) => {
                    vec![*true_target, *false_target]
                }
                _ => vec![],
            };
            for target in targets {
                if reachable.insert(target) {
                    pending.push(target);
                }
            }
        }
        self.blocks.retain(|block| reachable.contains(&block.id));
        Ok(())
    }

    pub(super) fn finish_indirect_branches(&mut self) -> Result<(), LowerError> {
        let targets = self
            .address_labels
            .iter()
            .map(|(name, placeholder)| {
                self.labels
                    .get(name)
                    .cloned()
                    .map(|target| (*placeholder, target))
                    .ok_or(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "indirect target label was not lowered",
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (mut state, target) in std::mem::take(&mut self.indirect_branches) {
            let needs_scope_actions = targets.iter().any(|(_, target)| {
                target.keys.iter().any(|key| !state.env.contains_key(key))
                    || state.env.keys().any(|key| {
                        matches!(
                            key,
                            EnvKey::Address(_) | EnvKey::Compound(_) | EnvKey::TemporaryAddress(_)
                        ) && !target.keys.contains(key)
                    })
            });
            let target_key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                target_key,
                BoundValue {
                    value: target.clone(),
                    integer: None,
                },
            );
            if needs_scope_actions {
                // Scope entry/exit is not a parallel copy. Route to the
                // selected adapter before allocating or ending any object.
                // The final indirect terminator also preserves the complete
                // CFG and diagnoses a value naming no valid label.
                for (placeholder, destination) in &targets {
                    let keys: Vec<_> = state.env.keys().copied().collect();
                    let chosen = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                    let next = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                    let label = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
                    self.emit(
                        &state,
                        Inst::BlockAddr {
                            result: label.clone(),
                            block: *placeholder,
                        },
                    )?;
                    let address = self.fresh_value(EvoIRType::Bits(64));
                    let expected = self.fresh_value(EvoIRType::Bits(64));
                    let predicate = self.fresh_value(EvoIRType::Bits(1));
                    self.emit(
                        &state,
                        Inst::PointerCast {
                            result: address.clone(),
                            operand: state.env[&target_key].value.clone(),
                        },
                    )?;
                    self.emit(
                        &state,
                        Inst::PointerCast {
                            result: expected.clone(),
                            operand: label,
                        },
                    )?;
                    self.emit(
                        &state,
                        Inst::ICmp(predicate.clone(), CmpOp::Eq, address, expected),
                    )?;
                    let arguments = self.edge_values(&state, &keys)?;
                    self.terminate(
                        &state,
                        Inst::CondBr(
                            predicate,
                            chosen.block,
                            arguments.clone(),
                            next.block,
                            arguments,
                        ),
                    )?;
                    self.branch(&chosen, destination.id, &destination.keys)?;
                    state = next;
                }
            }
            let mut edges = Vec::new();
            for (_, destination) in &targets {
                let mut arguments = Vec::new();
                let types = self
                    .block_mut(destination.id)?
                    .args
                    .iter()
                    .map(|(_, arg)| arg.value.ty.clone())
                    .collect::<Vec<_>>();
                for (key, ty) in destination.keys.iter().zip(types) {
                    if let Some(bound) = state.env.get(key) {
                        arguments.push(bound.value.clone());
                    } else {
                        // Only the invalid-target fall-through can reach
                        // this placeholder; it is never an object access.
                        let value = self.fresh_value(ty);
                        self.emit(&state, Inst::Uninitialized(value.clone()))?;
                        arguments.push(value);
                    }
                }
                edges.push(evocc_evoir::Edge {
                    target: destination.id,
                    arguments,
                });
            }
            edges.sort_by_key(|edge| edge.target);
            self.terminate(
                &state,
                Inst::IndirectBr {
                    target: state.env[&target_key].value.clone(),
                    edges,
                },
            )?;
        }
        Ok(())
    }

    pub(super) fn block_mut(&mut self, id: BlockId) -> Result<&mut DraftBlock, LowerError> {
        self.blocks
            .iter_mut()
            .find(|block| block.id == id)
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "unknown draft block",
            })
    }

    pub(super) fn emit(&mut self, state: &State, instruction: Inst) -> Result<(), LowerError> {
        if let Some(origins) = &mut self.source_values {
            for value in inst_results(&instruction) {
                origins.insert(value.id, self.source_span);
            }
        }
        let block = self.block_mut(state.block)?;
        if block.terminator.is_some() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "instruction emitted after a terminator",
            });
        }
        block.insts.push(instruction);
        Ok(())
    }

    pub(super) fn terminate(&mut self, state: &State, terminator: Inst) -> Result<(), LowerError> {
        let block = self.block_mut(state.block)?;
        if block.terminator.replace(terminator).is_some() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "block received more than one terminator",
            });
        }
        Ok(())
    }

    pub(super) fn branch(
        &mut self,
        state: &State,
        target: BlockId,
        keys: &[EnvKey],
    ) -> Result<(), LowerError> {
        if !state.env.keys().chain(keys.iter()).any(|key| {
            matches!(
                key,
                EnvKey::Compound(_) | EnvKey::Address(_) | EnvKey::TemporaryAddress(_)
            )
        }) {
            let arguments = self.edge_values(state, keys)?;
            return self.terminate(state, Inst::Br(target, arguments));
        }
        let mut state = state.clone();
        let leaving_storage = state
            .env
            .keys()
            .copied()
            .filter(|key| {
                matches!(key, EnvKey::Compound(_) | EnvKey::TemporaryAddress(_))
                    && !keys.contains(key)
            })
            .collect::<Vec<_>>();
        let leaving = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Address(symbol) if !keys.contains(key) => Some(*symbol),
                _ => None,
            })
            .collect::<Vec<_>>();
        for symbol in leaving.into_iter().rev() {
            self.end_object_lifetime(symbol, &mut state)?;
            state.env.remove(&EnvKey::Address(symbol));
        }
        self.end_compound_storage(&leaving_storage, &mut state)?;
        for key in keys {
            if let EnvKey::Address(symbol) = key {
                if !state.env.contains_key(key) {
                    let object = self.object_declarations.get(symbol).copied().ok_or(
                        LowerError::InvalidGeneratedSsa {
                            message: format!(
                                "object {} has no lexical storage declaration",
                                symbol.raw()
                            ),
                        },
                    )?;
                    self.allocate_object_storage(object, &mut state)?;
                }
            }
            if let EnvKey::Compound(identity) = key {
                if !state.env.contains_key(key) {
                    // A goto may enter below the lexical block's ordinary
                    // entry. Allocate on that edge, without its initializer.
                    let (ty, span) = self
                        .compound_scopes
                        .values()
                        .find_map(|sites| sites.get(identity))
                        .copied()
                        .ok_or(LowerError::InvalidGeneratedSsa {
                            message: "compound literal edge has no lexical storage site".to_owned(),
                        })?;
                    self.allocate_compound_storage(*identity, ty, span, &mut state)?;
                }
            }
        }
        let arguments = self.edge_values(&state, keys)?;
        self.terminate(&state, Inst::Br(target, arguments))
    }

    pub(super) fn keys_for(
        &self,
        state: &State,
        symbols: &BTreeSet<SymbolId>,
        preferred: &[SymbolId],
    ) -> Vec<EnvKey> {
        let mut keys = Vec::new();
        let mut seen = BTreeSet::new();
        let local_key = |symbol: SymbolId| {
            if self.memory_symbols.contains(&symbol) {
                let key = EnvKey::Address(symbol);
                // Lexical visibility includes later declarations, but their
                // allocation has not executed and has no address to transmit.
                state.env.contains_key(&key).then_some(key)
            } else {
                Some(EnvKey::Local(symbol))
            }
        };
        for symbol in preferred {
            let Some(key) = local_key(*symbol) else {
                continue;
            };
            if symbols.contains(symbol) && state.locals.contains(symbol) && seen.insert(key) {
                keys.push(key);
            }
        }
        let mut locals = state
            .env
            .iter()
            .filter_map(|(key, bound)| match key {
                EnvKey::Local(symbol) | EnvKey::Address(symbol) => {
                    Some((bound.value.id, *symbol, *key))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        locals.sort_by_key(|(value, _, _)| *value);
        for (_, symbol, key) in locals {
            // Structured successors do not describe a forward goto's uses.
            // Keep active scalar bindings until their lexical scope ends in
            // functions with labels targeted by jumps; otherwise a branch
            // containing only `goto` silently turns live values into poison.
            if (!self.goto_names.is_empty()
                || symbols.contains(&symbol)
                || self.size_symbols.contains(&symbol)
                || matches!(key, EnvKey::Address(_)))
                && seen.insert(key)
            {
                keys.push(key);
            }
        }
        for symbol in symbols {
            let Some(key) = local_key(*symbol) else {
                continue;
            };
            if seen.contains(&key) {
                continue;
            }
            let is_scalar_local = state.locals.contains(symbol)
                && self.unit.symbols().symbol(*symbol).is_ok_and(|symbol| {
                    matches!(symbol.kind(), SymbolKind::Object | SymbolKind::Parameter)
                });
            if is_scalar_local && seen.insert(key) {
                keys.push(key);
            }
        }
        let mut temporaries = state
            .env
            .iter()
            .filter_map(|(key, bound)| match key {
                EnvKey::Temporary(_) | EnvKey::TemporaryAddress(_) | EnvKey::Compound(_) => {
                    Some((bound.value.id, *key))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        temporaries.sort_by_key(|(value, _)| *value);
        for (_, key) in temporaries {
            if seen.insert(key) {
                keys.push(key);
            }
        }
        if state.env.contains_key(&EnvKey::ReturnDestination) {
            keys.push(EnvKey::ReturnDestination);
        }
        if self.info.participant {
            if state.env.contains_key(&EnvKey::Memory) {
                keys.push(EnvKey::Memory);
            }
            if state.env.contains_key(&EnvKey::World) {
                keys.push(EnvKey::World);
            }
        }
        keys
    }

    pub(super) fn expression_join_keys(
        &self,
        state: &State,
        arms: &[&CHirExpr<'_>],
    ) -> Vec<EnvKey> {
        let mut symbols = state
            .env
            .keys()
            .filter_map(|key| match key {
                EnvKey::Local(symbol) => Some(*symbol),
                _ => None,
            })
            .collect();
        // An arm may assign a declared local for the first time. Retain that
        // local even without a prior SSA binding; skipped arms carry poison.
        // Do not thread unrelated, not-yet-initialized declarations.
        for arm in arms {
            collect_symbols_expr(arm, &mut symbols);
        }
        self.keys_for(state, &symbols, &[])
    }

    pub(super) fn new_block_from(
        &mut self,
        template: &State,
        keys: &[EnvKey],
        role: BlockRole,
    ) -> Result<State, LowerError> {
        let id = self.next_block;
        self.next_block += 1;
        let mut args = Vec::new();
        let mut env = Environment::new();
        for (index, key) in keys.iter().copied().enumerate() {
            let (ty, integer) = if let Some(bound) = template.env.get(&key) {
                (bound.value.ty.clone(), bound.integer)
            } else if let EnvKey::Local(symbol) = key {
                let declaration =
                    self.unit
                        .symbols()
                        .symbol(symbol)
                        .map_err(|_| LowerError::UnknownSymbol {
                            symbol,
                            span: self.info.span,
                        })?;
                let (ty, integer) = self.c_value_type(declaration.ty(), self.info.span)?;
                (ty, Some(integer))
            } else if let EnvKey::Address(_) = key {
                (EvoIRType::Ptr(AddressSpace::Generic), None)
            } else {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "effect or temporary edge key is absent from its template environment",
                });
            };
            let value = self.fresh_value(ty);
            args.push((
                key,
                BlockArg {
                    name: format!("phi{index}"),
                    value: value.clone(),
                },
            ));
            env.insert(key, BoundValue { value, integer });
        }
        self.blocks.push(DraftBlock {
            id,
            args,
            insts: Vec::new(),
            terminator: None,
            role,
        });
        Ok(State {
            block: id,
            env,
            locals: template.locals.clone(),
            cleanups: template
                .cleanups
                .iter()
                .copied()
                .filter(|symbol| keys.contains(&EnvKey::Address(*symbol)))
                .collect(),
        })
    }

    pub(super) fn edge_values(
        &mut self,
        state: &State,
        keys: &[EnvKey],
    ) -> Result<Vec<Value>, LowerError> {
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(bound) = state.env.get(key) {
                values.push(bound.value.clone());
                continue;
            }
            let EnvKey::Local(symbol) = key else {
                return Err(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "effect or temporary edge uses an unavailable environment value",
                });
            };
            let declaration =
                self.unit
                    .symbols()
                    .symbol(*symbol)
                    .map_err(|_| LowerError::UnknownSymbol {
                        symbol: *symbol,
                        span: self.info.span,
                    })?;
            let (ty, _) = self.c_value_type(declaration.ty(), self.info.span)?;
            let value = self.fresh_value(ty);
            self.emit(state, Inst::Uninitialized(value.clone()))?;
            values.push(value);
        }
        Ok(values)
    }

    pub(super) fn merge_states(
        &mut self,
        states: Vec<State>,
        live: &BTreeSet<SymbolId>,
    ) -> Result<Option<State>, LowerError> {
        let mut states = states.into_iter();
        let Some(first) = states.next() else {
            return Ok(None);
        };
        let rest = states.collect::<Vec<_>>();
        if rest.is_empty() {
            return Ok(Some(first));
        }
        let keys = self.keys_for(&first, live, &[]);
        let target = self.new_block_from(&first, &keys, BlockRole::Ordinary)?;
        self.branch(&first, target.block, &keys)?;
        for state in rest {
            self.branch(&state, target.block, &keys)?;
        }
        Ok(Some(target))
    }

    pub(super) fn retain_temporary(&mut self, value: Value, state: &mut State) -> EnvKey {
        let key = EnvKey::Temporary(self.next_temporary);
        self.next_temporary += 1;
        state.env.insert(
            key,
            BoundValue {
                value,
                integer: None,
            },
        );
        key
    }

    pub(super) fn token(&self, state: &State, key: EnvKey) -> Result<BoundValue, LowerError> {
        state
            .env
            .get(&key)
            .cloned()
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "call participant has no current effect token",
            })
    }

    pub(super) fn replace_target(&mut self, old: BlockId, new: BlockId) {
        for block in &mut self.blocks {
            if let Some(terminator) = &mut block.terminator {
                replace_inst_target(terminator, old, new);
            }
        }
    }

    pub(super) fn move_block_before(
        &mut self,
        block_id: BlockId,
        before: usize,
    ) -> Result<(), LowerError> {
        let position = self
            .blocks
            .iter()
            .position(|block| block.id == block_id)
            .ok_or(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "loop step block disappeared",
            })?;
        if before < position {
            let block = self.blocks.remove(position);
            self.blocks.insert(before, block);
        }
        Ok(())
    }

    pub(super) fn eliminate_switch_forwarders(&mut self) -> Result<(), LowerError> {
        while let Some(index) = self.blocks.iter().position(|block| {
            if block.role != BlockRole::SwitchExit || !block.insts.is_empty() {
                return false;
            }
            let Some(Inst::Br(_, arguments)) = &block.terminator else {
                return false;
            };
            block.args.len() == arguments.len()
                && block
                    .args
                    .iter()
                    .zip(arguments)
                    .all(|((_, argument), value)| argument.value == *value)
        }) {
            let block = &self.blocks[index];
            let target = match &block.terminator {
                Some(Inst::Br(target, _)) => *target,
                _ => {
                    return Err(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "switch forwarding block changed during elimination",
                    });
                }
            };
            let old = block.id;
            for predecessor in &mut self.blocks {
                let Some(terminator) = &mut predecessor.terminator else {
                    continue;
                };
                replace_inst_target(terminator, old, target);
            }
            self.blocks.remove(index);
        }
        Ok(())
    }

    pub(super) fn finish_blocks(self) -> Result<Vec<Block>, LowerError> {
        self.blocks
            .into_iter()
            .map(|block| {
                Ok(Block {
                    id: block.id,
                    args: block
                        .args
                        .into_iter()
                        .map(|(_, argument)| argument)
                        .collect(),
                    insts: block.insts,
                    terminator: block.terminator.ok_or(LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "draft block lacks a terminator",
                    })?,
                })
            })
            .collect()
    }
}
