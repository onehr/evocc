//! Lowering lifetimes.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_compound_literal_address(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<Value, LowerError> {
        let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
            return Err(LowerError::UnsupportedExpression {
                span: expression.span(),
                feature: "compound-literal allocation requested for another expression",
            });
        };
        if initializer.plan().target_type() != *ty
            || initializer.plan().validate(self.unit.types()).is_err()
        {
            return Err(LowerError::UnsupportedExpression {
                span: *span,
                feature: "compound-literal initializer plan is invalid",
            });
        }
        let identity = expression as *const CHirExpr<'a> as usize;
        let (origin, static_storage) = self.compound_origins.get(&identity).copied().ok_or(
            LowerError::InvalidGeneratedSsa {
                message: "compound literal has no allocation origin".to_owned(),
            },
        )?;
        if static_storage {
            let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            self.emit(
                state,
                Inst::GlobalAddr {
                    result: address.clone(),
                    symbol: format!("__evocc.compound.{origin}"),
                },
            )?;
            return Ok(address);
        }
        let address = self.token(state, EnvKey::Compound(identity))?.value;
        if self.unit.zero_initialize_compound_padding(initializer) {
            self.lower_constructor_padding(address.clone(), initializer, *span, state)?;
        }
        self.lower_initializer_at(address, *ty, initializer, *span, state)
    }

    /// Storage belongs to the block activation, while the initializer runs
    /// on every evaluation. Hoist only allocation, never initializer effects.
    pub(super) fn begin_compound_storage(
        &mut self,
        scope: usize,
        state: &mut State,
    ) -> Result<Vec<EnvKey>, LowerError> {
        let sites = self
            .compound_scopes
            .get(&scope)
            .cloned()
            .unwrap_or_default();
        let mut keys = Vec::new();
        // Allocation order follows manifest ordinals, not host AST addresses.
        let mut sites = sites.into_iter().collect::<Vec<_>>();
        sites.sort_by_key(|(identity, _)| {
            self.compound_origins
                .get(identity)
                .map(|(origin, _)| *origin)
        });
        for (identity, (ty, span)) in sites {
            let Some(&(_, false)) = self.compound_origins.get(&identity) else {
                continue;
            };
            self.allocate_compound_storage(identity, ty, span, state)?;
            keys.push(EnvKey::Compound(identity));
        }
        Ok(keys)
    }

    pub(super) fn allocate_compound_storage(
        &mut self,
        identity: usize,
        ty: TypeId,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let (origin, _) = self.compound_origins[&identity];
        let address = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::Alloca {
                result: address.clone(),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
                origin,
                layout: self.access_layout(ty, span)?,
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        state.env.insert(
            EnvKey::Compound(identity),
            BoundValue {
                value: address,
                integer: None,
            },
        );
        Ok(())
    }

    pub(super) fn end_compound_storage(
        &mut self,
        keys: &[EnvKey],
        state: &mut State,
    ) -> Result<(), LowerError> {
        let mut keys = keys.to_vec();
        // EnvKey includes a host AST address only as an internal lookup key.
        // Observable IR ordering must depend on stable IR value identities.
        keys.sort_by_key(|key| state.env.get(key).map(|binding| binding.value.id));
        for key in keys.iter().rev() {
            if let Some(binding) = state.env.remove(key) {
                self.end_pointer_lifetime(binding.value, state)?;
            }
        }
        Ok(())
    }

    pub(super) fn end_object_lifetime(
        &mut self,
        symbol: SymbolId,
        state: &mut State,
    ) -> Result<(), LowerError> {
        if self.unit.symbols().symbol(symbol).is_ok_and(|entry| {
            self.unit
                .symbols()
                .is_bound_in_scope(symbol, self.unit.symbols().file_scope())
                .unwrap_or(false)
                || matches!(
                    entry.storage_class(),
                    Some(StorageClass::Static | StorageClass::Extern)
                )
        }) {
            return Ok(());
        }
        if state.cleanups.contains(&symbol) {
            state.cleanups.retain(|active| *active != symbol);
            if let Some(cleanup) = self
                .object_declarations
                .get(&symbol)
                .and_then(|o| o.cleanup)
            {
                self.lower_cleanup(cleanup, state)?;
            }
        }
        // An inlined callback can create blocks. Read the address from its
        // continuation's environment, not a Value ID from the previous block.
        let pointer = self.token(state, EnvKey::Address(symbol))?.value;
        self.end_pointer_lifetime(pointer, state)
    }

    pub(super) fn end_pointer_lifetime(
        &mut self,
        pointer: Value,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::LifetimeEnd {
                mem_in,
                world_in,
                pointer,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            },
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(())
    }

    pub(super) fn contains_addressed_label(&self, statement: &CHirStmt<'_>) -> bool {
        if self.address_labels.is_empty() {
            return false;
        }
        let mut references = FunctionReferences::default();
        collect_references_stmt(statement, &mut references);
        references
            .defined_labels
            .iter()
            .any(|label| self.address_labels.contains_key(label))
    }
}
