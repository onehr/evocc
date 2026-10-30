//! Lowering statements.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_stmt(
        &mut self,
        statement: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let previous_span = std::mem::replace(&mut self.source_span, statement.span());
        self.record_constant_prefix(statement);
        let previous = std::mem::replace(&mut self.active_control, control.clone());
        let scope = match statement {
            CHirStmt::Switch(_, body, _) => *body as *const CHirStmt<'a> as usize,
            _ => statement as *const CHirStmt<'a> as usize,
        };
        let storage = self.begin_compound_storage(scope, &mut state)?;
        let mut result = self.lower_stmt_active(statement, state, live_after, control);
        if let Ok(flow) = &mut result {
            let mut pending = std::mem::take(&mut self.pending_gotos);
            for state in flow.normal.iter_mut().chain(pending.values_mut().flatten()) {
                self.end_compound_storage(&storage, state)?;
            }
            self.pending_gotos = pending;
        }
        self.active_control = previous;
        self.source_span = previous_span;
        result
    }

    pub(super) fn strict_floating(&self) -> bool {
        self.unit.dynamic_rounding() || self.fenv_access
    }

    pub(super) fn record_constant_prefix(&mut self, body: &CHirStmt<'_>) {
        let CHirStmt::Compound(items, _, _) = body else {
            return;
        };
        let mut candidates = Vec::new();
        for item in items {
            let CHirBlockItem::Declaration(decl) = item else {
                break;
            };
            let mut initializer = FunctionReferences::default();
            collect_references_decl(decl, &mut initializer);
            if !initializer.goto_names.is_empty() || initializer.has_computed_goto {
                break;
            }
            if let CHirDecl::Object(object) = decl {
                if object.initializer.is_some()
                    && !matches!(object.storage, StorageClass::Static | StorageClass::Extern)
                    && !self.unstable_objects.contains(&object.symbol)
                {
                    candidates.push(object.symbol);
                }
            }
        }
        if candidates.is_empty() {
            return;
        }
        // Use the complete compound, never a switch/statement-expression
        // prefix: an incoming label after the prefix could skip initialization.
        // Internal backedges are safe for an object never modified or escaped.
        let mut local = FunctionReferences::default();
        collect_references_stmt(body, &mut local);
        if local.has_unbound_case_label
            || local.defined_labels.iter().any(|label| {
                self.address_labels.contains_key(label)
                    || self.goto_counts.get(label).copied().unwrap_or(0)
                        > local.goto_counts.get(label).copied().unwrap_or(0)
            })
        {
            return;
        }
        self.constant_objects.extend(candidates);
    }

    pub(super) fn lower_stmt_active(
        &mut self,
        statement: &'a CHirStmt<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let previous = self.fenv_access;
        if let CHirStmt::Compound(_, _, Some(enabled)) = statement {
            self.fenv_access = *enabled;
        }
        let result = self.lower_stmt_in_policy(statement, state, live_after, control);
        self.fenv_access = previous;
        result
    }

    pub(super) fn lower_stmt_in_policy(
        &mut self,
        statement: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        match statement {
            CHirStmt::Asm(assembly) => self.lower_assembly(assembly, state, live_after),
            CHirStmt::Expr(CHirExpr::BuiltinUnreachable { .. }, _) => {
                self.terminate(&state, Inst::Trap(TrapReason::UnreachableReached))?;
                Ok(Flow::default())
            }
            CHirStmt::Expr(CHirExpr::BuiltinTrap { .. }, _) => {
                self.terminate(&state, Inst::Trap(TrapReason::BuiltinTrap))?;
                Ok(Flow::default())
            }
            CHirStmt::Expr(expression, _) => self.lower_discarded_flow(expression, state),
            CHirStmt::Return(expression, span) => {
                let mut returned = Vec::new();
                match (self.info.return_type.as_ref(), expression) {
                    (Some(expected), Some(expression)) => {
                        if self.is_aggregate_type(expected.source) {
                            let source = self.lower_expr(expression, &mut state)?;
                            let destination = self.token(&state, EnvKey::ReturnDestination)?.value;
                            self.emit_aggregate_transfer(
                                destination,
                                source.value,
                                expected.source,
                                CopyVolatility::new(
                                    self.aggregate_source_is_volatile(expression),
                                    false,
                                ),
                                *span,
                                &mut state,
                            )?;
                        } else {
                            let lowered = self.lower_expr(expression, &mut state)?;
                            let lowered = if matches!(
                                expected.ir,
                                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } | EvoIRType::Vec(..)
                            ) || evocc_evoir::floating_kind(&expected.ir).is_some()
                            {
                                self.coerce_to_object_type(
                                    lowered,
                                    Some(expression),
                                    expected.source,
                                    *span,
                                    &mut state,
                                )?
                            } else {
                                let value = self.cast(lowered, expected.integer, &mut state)?;
                                // A result that travels wider than its C type
                                // is extended by its signedness (10.1.3).
                                if value.value.ty == expected.ir {
                                    value
                                } else {
                                    self.cast(value, CIntegerType::INT, &mut state)?
                                }
                            };
                            returned.push(lowered.value);
                        }
                    }
                    (None, None) => {}
                    (None, Some(expression)) if self.expression_is_void(expression) => {
                        let live = state.locals.clone();
                        let flow = self.lower_discarded_flow(expression, state)?;
                        let Some(continued) = self.merge_states(flow.normal, &live)? else {
                            return Ok(Flow::default());
                        };
                        state = continued;
                    }
                    _ => {
                        return Err(LowerError::MalformedFunction {
                            function: self.info.name.clone(),
                            span: *span,
                            reason: "return expression disagrees with the function result type",
                        });
                    }
                }
                // Evaluate/copy the return value before callbacks may mutate
                // its source, but keep every object alive while callbacks run.
                let cleanups = state
                    .cleanups
                    .iter()
                    .filter_map(|symbol| {
                        self.object_declarations
                            .get(symbol)
                            .and_then(|object| object.cleanup)
                    })
                    .collect::<Vec<_>>();
                let return_keys = returned
                    .drain(..)
                    .map(|value| self.retain_temporary(value, &mut state))
                    .collect::<Vec<_>>();
                for cleanup in cleanups.into_iter().rev() {
                    self.lower_cleanup(cleanup, &mut state)?;
                }
                for key in return_keys {
                    returned.push(self.token(&state, key)?.value);
                    state.env.remove(&key);
                }
                if self.info.participant {
                    returned.push(self.token(&state, EnvKey::Memory)?.value);
                    returned.push(self.token(&state, EnvKey::World)?.value);
                }
                self.terminate(&state, Inst::Ret(returned))?;
                Ok(Flow::default())
            }
            CHirStmt::Compound(items, _, _) => {
                self.lower_compound(items, state, live_after, control)
            }
            CHirStmt::If(condition, then_branch, else_branch, _) => self.lower_if(
                condition,
                then_branch,
                *else_branch,
                state,
                live_after,
                control,
            ),
            CHirStmt::While(condition, body, _) => {
                self.lower_while(condition, body, state, live_after, control)
            }
            CHirStmt::For(initializer, condition, step, body, _) => self.lower_for(
                initializer,
                *condition,
                *step,
                body,
                state,
                live_after,
                control,
            ),
            CHirStmt::Switch(expression, body, _) => {
                self.lower_switch(expression, body, state, live_after, control)
            }
            CHirStmt::Break(span) => {
                let target =
                    control
                        .break_target
                        .as_ref()
                        .ok_or(LowerError::InvalidControlFlow {
                            span: *span,
                            reason: "break has no enclosing target",
                        })?;
                self.branch(&state, target.id, &target.keys)?;
                Ok(Flow::default())
            }
            CHirStmt::Continue(span) => {
                let target =
                    control
                        .continue_target
                        .as_ref()
                        .ok_or(LowerError::InvalidControlFlow {
                            span: *span,
                            reason: "continue has no enclosing loop",
                        })?;
                self.branch(&state, target.id, &target.keys)?;
                Ok(Flow::default())
            }
            CHirStmt::AttributedNull(_, _) | CHirStmt::Null(_) => Ok(Flow {
                normal: vec![state],
            }),
            CHirStmt::DoWhile(body, condition, _) => {
                self.lower_do_while(body, condition, state, live_after, control)
            }
            CHirStmt::Goto(label, _) => {
                if let Some(target) = self.labels.get(&label.spelling).cloned() {
                    self.branch(&state, target.id, &target.keys)?;
                } else {
                    self.pending_gotos
                        .entry(label.spelling.clone())
                        .or_default()
                        .push(state);
                }
                Ok(Flow::default())
            }
            CHirStmt::ComputedGoto(expression, _) => {
                let target = self.lower_expr(expression, &mut state)?.value;
                self.indirect_branches.push((state, target));
                Ok(Flow::default())
            }
            CHirStmt::Label(label, nested, _) => self.lower_label(
                label.spelling.as_str(),
                nested,
                vec![state],
                live_after,
                control,
            ),
            CHirStmt::Case(_, nested, span)
            | CHirStmt::CaseRange(_, _, nested, span)
            | CHirStmt::Default(nested, span) => {
                let name = self
                    .case_labels
                    .get(&(statement as *const CHirStmt<'_> as usize))
                    .cloned()
                    .ok_or(LowerError::InvalidControlFlow {
                        span: *span,
                        reason: "case label has no enclosing switch dispatch",
                    })?;
                self.lower_label(&name, nested, vec![state], live_after, control)
            }
        }
    }

    pub(super) fn lower_compound(
        &mut self,
        items: &'a [CHirBlockItem<'a>],
        initial: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        self.lower_compound_items(items, initial, live_after, control, true)
    }

    pub(super) fn lower_compound_items(
        &mut self,
        items: &'a [CHirBlockItem<'a>],
        initial: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
        leave_scope: bool,
    ) -> Result<Flow, LowerError> {
        let items = items.iter().map(|item| match item {
            CHirBlockItem::Declaration(declaration) => SwitchSegmentItem::Declaration(declaration),
            CHirBlockItem::Statement(statement) => SwitchSegmentItem::Statement(statement),
        });
        self.lower_sequence(items, initial, live_after, control, leave_scope)
    }

    pub(super) fn lower_sequence(
        &mut self,
        mut items: impl Iterator<Item = SwitchSegmentItem<'a>> + Clone,
        mut initial: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
        leave_scope: bool,
    ) -> Result<Flow, LowerError> {
        let mut declared = Vec::new();
        for item in items.clone() {
            if let SwitchSegmentItem::Declaration(CHirDecl::Object(object)) = item {
                let dynamic = self
                    .target
                    .array_size_factors(self.unit.types(), object.ty)
                    .is_ok_and(|(_, captures)| !captures.is_empty());
                // A VLA's declaration is its allocation point: evaluating
                // its captured bounds cannot be hoisted to compound entry.
                if !dynamic {
                    initial.locals.insert(object.symbol);
                }
                declared.push(object.symbol);
                self.object_declarations.insert(object.symbol, object);
                if self.memory_symbols.contains(&object.symbol)
                    && !dynamic
                    && !matches!(object.storage, StorageClass::Static | StorageClass::Extern)
                    && !initial.env.contains_key(&EnvKey::Address(object.symbol))
                {
                    self.allocate_object_storage(object, &mut initial)?;
                }
            }
        }
        let mut normal = vec![initial];
        let mut last_template = normal.first().cloned();
        while let Some(item) = items.next() {
            let mut future = live_after.clone();
            for later in items.clone() {
                collect_symbols_switch_item(&later, &mut future);
            }
            let mut needed_now = future.clone();
            collect_symbols_switch_item(&item, &mut needed_now);
            let label = match item {
                SwitchSegmentItem::Statement(CHirStmt::Label(label, nested, _)) => {
                    Some((label.spelling.clone(), *nested))
                }
                SwitchSegmentItem::Statement(
                    statement @ (CHirStmt::Case(_, nested, _)
                    | CHirStmt::CaseRange(_, _, nested, _)
                    | CHirStmt::Default(nested, _)),
                ) => self
                    .case_labels
                    .get(&(statement as *const CHirStmt<'_> as usize))
                    .cloned()
                    .map(|name| (name, *nested)),
                _ => None,
            };
            if let Some((name, nested)) = label {
                let incoming = std::mem::take(&mut normal);
                let template = incoming.first().cloned().or_else(|| last_template.clone());
                normal = self
                    .lower_label_with_template(&name, nested, incoming, template, &future, control)?
                    .normal;
                if let Some(state) = normal.first().cloned() {
                    last_template = Some(state);
                }
                continue;
            }
            if normal.is_empty() {
                let SwitchSegmentItem::Statement(statement) = item else {
                    continue;
                };
                if !contains_targeted_label(statement, &self.goto_names, &self.case_labels) {
                    continue;
                }
                let template = last_template
                    .clone()
                    .ok_or(LowerError::InvalidControlFlow {
                        span: statement.span(),
                        reason: "targeted nested label has no environment template",
                    })?;
                let keys = self.keys_for(&template, &needed_now, &[]);
                let detached = self.new_block_from(&template, &keys, BlockRole::Ordinary)?;
                normal = self
                    .lower_stmt(statement, detached, &future, control)?
                    .normal;
                if let Some(state) = normal.first().cloned() {
                    last_template = Some(state);
                }
                continue;
            }
            let pending = std::mem::take(&mut normal);
            let Some(mut state) = self.merge_states(pending, &needed_now)? else {
                continue;
            };
            last_template = Some(state.clone());
            normal = match item {
                SwitchSegmentItem::Declaration(declaration) => {
                    self.lower_declaration(declaration, &mut state)?;
                    vec![state]
                }
                SwitchSegmentItem::Statement(statement) => {
                    self.lower_stmt(statement, state, &future, control)?.normal
                }
            };
        }
        if leave_scope {
            // Unresolved forward gotos leave this compound too. Its internal
            // labels have already consumed their incoming states; retaining a
            // local address on an outgoing edge would both extend its lifetime
            // and demand that address from predecessors which never declared it.
            let mut pending_gotos = std::mem::take(&mut self.pending_gotos);
            for state in normal
                .iter_mut()
                .chain(pending_gotos.values_mut().flatten())
            {
                for symbol in declared.iter().rev() {
                    if self.memory_symbols.contains(symbol)
                        && state.env.contains_key(&EnvKey::Address(*symbol))
                        && self.unit.symbols().symbol(*symbol).is_ok_and(|entry| {
                            entry.storage_class().map(StorageClass::base)
                                != Some(StorageClass::Static)
                        })
                    {
                        self.end_object_lifetime(*symbol, state)?;
                    }
                    state.env.remove(&EnvKey::Local(*symbol));
                    state.env.remove(&EnvKey::Address(*symbol));
                    state.locals.remove(symbol);
                }
            }
            self.pending_gotos = pending_gotos;
        }
        Ok(Flow { normal })
    }

    pub(super) fn lower_label(
        &mut self,
        name: &str,
        nested: &'a CHirStmt<'a>,
        incoming: Vec<State>,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let template = incoming.first().cloned();
        self.lower_label_with_template(name, nested, incoming, template, live_after, control)
    }

    pub(super) fn lower_label_with_template(
        &mut self,
        name: &str,
        nested: &'a CHirStmt<'a>,
        incoming: Vec<State>,
        template: Option<State>,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let Some(state) = self.enter_label(name, nested, incoming, template, live_after)? else {
            return Ok(Flow::default());
        };
        self.lower_stmt(nested, state, live_after, control)
    }

    pub(super) fn enter_label(
        &mut self,
        name: &str,
        nested: &'a CHirStmt<'a>,
        mut incoming: Vec<State>,
        template: Option<State>,
        live_after: &BTreeSet<SymbolId>,
    ) -> Result<Option<State>, LowerError> {
        if self.labels.contains_key(name) {
            return Err(LowerError::InvalidControlFlow {
                span: nested.span(),
                reason: "duplicate label reached lowering",
            });
        }
        incoming.extend(self.pending_gotos.remove(name).unwrap_or_default());
        // The lexical template describes storage at the destination. A jump
        // predecessor can come from an outer scope and lack those objects.
        let Some(template) = template.or_else(|| incoming.first().cloned()) else {
            if self.goto_names.contains(name) {
                return Err(LowerError::InvalidControlFlow {
                    span: nested.span(),
                    reason: "targeted label has no environment template",
                });
            }
            return Ok(None);
        };
        let symbols = live_before_stmt(nested, live_after.clone());
        let mut keys = self.keys_for(&template, &symbols, &[]);
        let indirect_entry = self.has_computed_goto && self.address_labels.contains_key(name);
        if indirect_entry {
            keys.retain(|key| !matches!(key, EnvKey::Local(_) | EnvKey::Temporary(_)));
        }
        let label_state = self.new_block_from(&template, &keys, BlockRole::Ordinary)?;
        let target = JumpTarget {
            id: label_state.block,
            keys: keys.clone(),
        };
        self.labels.insert(name.to_owned(), target);
        for predecessor in incoming {
            self.branch(&predecessor, label_state.block, &keys)?;
        }
        Ok(Some(label_state))
    }

    pub(super) fn lower_declaration(
        &mut self,
        declaration: &'a CHirDecl<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let object = match declaration {
            CHirDecl::Object(object) => object,
            CHirDecl::Asm(assembly) => {
                return Err(LowerError::Asm {
                    span: assembly.span,
                });
            }
            CHirDecl::Tag(_) | CHirDecl::Typedef(_) => return Ok(()),
            CHirDecl::Function(function) if function.body.is_none() => return Ok(()),
            CHirDecl::Function(_) => {
                return Err(LowerError::UnsupportedStatement {
                    span: declaration.span(),
                    feature: "block function declaration is outside scalar lowering",
                });
            }
        };
        validate_object_declaration_for_lowering(self.unit, self.target, object)?;
        if object.storage.base() == StorageClass::Extern {
            // A block declaration introduces a binding, not a new object or
            // an allocation with the external object's existing origin.
            return Ok(());
        }
        if self.memory_symbols.contains(&object.symbol) {
            return self.lower_memory_declaration(object, state);
        }
        let (_, integer) = self.c_value_type(object.ty, object.span)?;
        state.locals.insert(object.symbol);
        let Some(initializer) = &object.initializer else {
            state.env.remove(&EnvKey::Local(object.symbol));
            if self.unit.zero_initialize_automatic(object) {
                let value = if matches!(
                    self.unit.types().get(object.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) {
                    let pointer = self.fresh_value(self.c_value_type(object.ty, object.span)?.0);
                    self.emit(state, Inst::PtrNull(pointer.clone()))?;
                    pointer
                } else {
                    let zero = self.constant(state, integer, 0)?;
                    self.coerce_to_object_type(zero, None, object.ty, object.span, state)?
                        .value
                };
                state.env.insert(
                    EnvKey::Local(object.symbol),
                    BoundValue {
                        value,
                        integer: Some(integer),
                    },
                );
            }
            return Ok(());
        };
        let stored = match initializer.plan.actions() {
            [InitAction::StoreScalar { value, .. }] => {
                let lowered = self.lower_expr(value, state)?;
                self.coerce_to_object_type(lowered, Some(value), object.ty, object.span, state)?
            }
            [InitAction::ZeroScalar { .. }] => {
                if matches!(
                    self.unit.types().get(object.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) {
                    let pointer = self.fresh_value(self.c_value_type(object.ty, object.span)?.0);
                    self.emit(state, Inst::PtrNull(pointer.clone()))?;
                    ExprValue {
                        value: pointer,
                        integer,
                    }
                } else {
                    let zero = self.constant(state, integer, 0)?;
                    self.coerce_to_object_type(zero, None, object.ty, object.span, state)?
                }
            }
            _ => {
                return Err(LowerError::UnsupportedStatement {
                    span: object.span,
                    feature: "aggregate or multi-action initializer is outside scalar lowering",
                });
            }
        };
        state.env.insert(
            EnvKey::Local(object.symbol),
            BoundValue {
                value: stored.value,
                integer: Some(integer),
            },
        );
        Ok(())
    }
}
