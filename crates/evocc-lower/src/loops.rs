//! Lowering loops.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_if(
        &mut self,
        condition: &'a CHirExpr<'a>,
        then_branch: &'a CHirStmt<'a>,
        else_branch: Option<&'a CHirStmt<'a>>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        control: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut known_truth = self.result_after_evaluation(condition).map(CInteger::truth);
        if known_truth.is_some()
            && (self.contains_addressed_label(then_branch)
                || else_branch.is_some_and(|branch| self.contains_addressed_label(branch)))
        {
            // An address-taken label is an observable code identity.
            // Section 2.5 still requires its terminator-edge path from
            // entry; do not remove that path merely to fold a branch.
            known_truth = None;
        }
        let condition = self.lower_condition(condition, &mut state)?;
        if let Some(take_then) = known_truth {
            let unselected = if take_then {
                else_branch
            } else {
                Some(then_branch)
            };
            if !unselected.is_some_and(|branch| {
                contains_targeted_label(branch, &self.goto_names, &self.case_labels)
            }) {
                // Only the condition's ordinary edge could enter this arm.
                // Do not require execution support for its unselected values.
                // The condition above still retains its runtime obligations.
                let selected = if take_then {
                    Some(then_branch)
                } else {
                    else_branch
                };
                return if let Some(branch) = selected {
                    self.lower_stmt(branch, state, live_after, control)
                } else {
                    Ok(Flow {
                        normal: vec![state],
                    })
                };
            }
        }
        let mut then_symbols = live_after.clone();
        collect_symbols_stmt(then_branch, &mut then_symbols);
        let then_keys = self.keys_for(&state, &then_symbols, &[]);
        let can_jump_directly = |target: &&JumpTarget| {
            !state.env.keys().any(|key| {
                matches!(
                    key,
                    EnvKey::Compound(_) | EnvKey::TemporaryAddress(_) | EnvKey::Address(_)
                ) && !target.keys.contains(key)
            })
        };
        let direct_then = self
            .direct_jump(then_branch, control)
            .filter(can_jump_directly);
        let then_state = if direct_then.is_none() {
            Some(self.new_block_from(&state, &then_keys, BlockRole::Ordinary)?)
        } else {
            None
        };

        let mut else_symbols = live_after.clone();
        if let Some(branch) = else_branch {
            collect_symbols_stmt(branch, &mut else_symbols);
        }
        let else_keys = self.keys_for(&state, &else_symbols, &[]);
        let direct_else = else_branch
            .and_then(|branch| self.direct_jump(branch, control))
            .filter(can_jump_directly);
        let else_state = if direct_else.is_none() {
            Some(self.new_block_from(&state, &else_keys, BlockRole::Ordinary)?)
        } else {
            None
        };

        let (then_target, then_args) = if let Some(target) = direct_then {
            (target.id, self.edge_values(&state, &target.keys)?)
        } else {
            let target = then_state.as_ref().ok_or(LowerError::InvalidControlFlow {
                span: then_branch.span(),
                reason: "if-then block was not constructed",
            })?;
            (target.block, self.edge_values(&state, &then_keys)?)
        };
        let (else_target, else_args) = if let Some(target) = direct_else {
            (target.id, self.edge_values(&state, &target.keys)?)
        } else {
            let target = else_state.as_ref().ok_or(LowerError::InvalidControlFlow {
                span: else_branch.map_or(then_branch.span(), CHirStmt::span),
                reason: "if-else block was not constructed",
            })?;
            (target.block, self.edge_values(&state, &else_keys)?)
        };
        // An unknown condition or another entry requires both arms. Fold only
        // the ordinary conditional edge; keep the independently targeted labels.
        let terminator = match known_truth {
            Some(true) => Inst::Br(then_target, then_args),
            Some(false) => Inst::Br(else_target, else_args),
            None => Inst::CondBr(condition, then_target, then_args, else_target, else_args),
        };
        self.terminate(&state, terminator)?;

        let mut normal = Vec::new();
        if let Some(then_state) = then_state {
            normal.extend(
                self.lower_stmt(then_branch, then_state, live_after, control)?
                    .normal,
            );
        }
        if let Some(else_state) = else_state {
            if let Some(branch) = else_branch {
                normal.extend(
                    self.lower_stmt(branch, else_state, live_after, control)?
                        .normal,
                );
            } else {
                normal.push(else_state);
            }
        }
        Ok(Flow { normal })
    }

    pub(super) fn direct_jump<'b>(
        &self,
        statement: &'b CHirStmt<'b>,
        control: &'b ControlTargets,
    ) -> Option<&'b JumpTarget> {
        match statement {
            CHirStmt::Break(_) => control.break_target.as_ref(),
            CHirStmt::Continue(_) => control.continue_target.as_ref(),
            _ => None,
        }
    }

    pub(super) fn lower_while(
        &mut self,
        condition: &'a CHirExpr<'a>,
        body: &'a CHirStmt<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut carried = live_after.clone();
        collect_symbols_expr(condition, &mut carried);
        collect_symbols_stmt(body, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &[]);
        let header = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        self.branch(&state, header.block, &carried_keys)?;
        let mut header_after_condition = header.clone();
        let known_truth = constant_truth(self.unit, self.target, condition);
        let predicate = if known_truth.is_none() {
            Some(self.lower_condition(condition, &mut header_after_condition)?)
        } else {
            None
        };
        let body_keys = self.keys_for(&header_after_condition, &carried, &[]);
        let body_state =
            self.new_block_from(&header_after_condition, &body_keys, BlockRole::Ordinary)?;
        let break_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&header_after_condition, live_after, &[]);
        let body_arguments = self.edge_values(&header_after_condition, &body_keys)?;
        let break_arguments = self.edge_values(&header_after_condition, &break_keys)?;
        match (known_truth, predicate) {
            (Some(true), None) => self.terminate(
                &header_after_condition,
                Inst::Br(body_state.block, body_arguments),
            )?,
            (Some(false), None) => self.terminate(
                &header_after_condition,
                Inst::Br(break_placeholder, break_arguments),
            )?,
            (None, Some(predicate)) => self.terminate(
                &header_after_condition,
                Inst::CondBr(
                    predicate,
                    body_state.block,
                    body_arguments,
                    break_placeholder,
                    break_arguments,
                ),
            )?,
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: condition.span(),
                    reason: "while condition classification is inconsistent",
                });
            }
        }
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: header.block,
                keys: carried_keys.clone(),
            }),
        };
        let flow = self.lower_stmt(body, body_state, &carried, &control)?;
        for normal in flow.normal {
            self.branch(&normal, header.block, &carried_keys)?;
        }
        let exit =
            self.new_block_from(&header_after_condition, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        let _ = outer;
        Ok(Flow { normal: vec![exit] })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_for(
        &mut self,
        initializer: &'a CHirForInit<'a>,
        condition: Option<&'a CHirExpr<'a>>,
        step: Option<&'a CHirExpr<'a>>,
        body: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        _outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut init_symbols = Vec::new();
        match initializer {
            CHirForInit::Empty => {}
            CHirForInit::Expression(expression) => {
                let live = state.locals.clone();
                let flow = self.lower_discarded_flow(expression, state)?;
                let Some(continued) = self.merge_states(flow.normal, &live)? else {
                    return Ok(Flow::default());
                };
                state = continued;
            }
            CHirForInit::Declaration(declarations) => {
                for declaration in declarations {
                    if let CHirDecl::Object(object) = declaration {
                        init_symbols.push(object.symbol);
                    }
                    self.lower_declaration(declaration, &mut state)?;
                }
            }
        }
        let mut carried = live_after.clone();
        if let Some(condition) = condition {
            collect_symbols_expr(condition, &mut carried);
        }
        if let Some(step) = step {
            collect_symbols_expr(step, &mut carried);
        }
        collect_symbols_stmt(body, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &init_symbols);
        let header = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        self.branch(&state, header.block, &carried_keys)?;
        let mut header_condition = header.clone();
        let known_truth =
            condition.and_then(|condition| constant_truth(self.unit, self.target, condition));
        let predicate = if condition.is_some() && known_truth.is_none() {
            condition
                .map(|condition| self.lower_condition(condition, &mut header_condition))
                .transpose()?
        } else {
            None
        };
        let body_keys = self.keys_for(&header_condition, &carried, &init_symbols);
        let body_state = self.new_block_from(&header_condition, &body_keys, BlockRole::Ordinary)?;
        let break_placeholder = self.fresh_placeholder();
        let continue_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&header_condition, live_after, &[]);
        let body_arguments = self.edge_values(&header_condition, &body_keys)?;
        match (condition, known_truth, predicate) {
            (None, None, None) | (Some(_), Some(true), None) => {
                self.terminate(
                    &header_condition,
                    Inst::Br(body_state.block, body_arguments),
                )?;
            }
            (Some(_), Some(false), None) => {
                let break_arguments = self.edge_values(&header_condition, &break_keys)?;
                self.terminate(
                    &header_condition,
                    Inst::Br(break_placeholder, break_arguments),
                )?;
            }
            (Some(_), None, Some(predicate)) => {
                let break_arguments = self.edge_values(&header_condition, &break_keys)?;
                self.terminate(
                    &header_condition,
                    Inst::CondBr(
                        predicate,
                        body_state.block,
                        body_arguments,
                        break_placeholder,
                        break_arguments,
                    ),
                )?;
            }
            _ => {
                return Err(LowerError::InvalidControlFlow {
                    span: condition.map_or(self.info.span, CHirExpr::span),
                    reason: "for condition classification is inconsistent",
                });
            }
        }
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: continue_placeholder,
                keys: carried_keys.clone(),
            }),
        };
        let body_flow = self.lower_stmt(body, body_state, &carried, &control)?;
        let step_template = body_flow
            .normal
            .first()
            .cloned()
            .unwrap_or_else(|| header.clone());
        let step_state = self.new_block_from(&step_template, &carried_keys, BlockRole::LoopStep)?;
        self.replace_target(continue_placeholder, step_state.block);
        for normal in &body_flow.normal {
            self.branch(normal, step_state.block, &carried_keys)?;
        }
        let normal_predecessor_positions = body_flow
            .normal
            .iter()
            .filter_map(|normal| {
                self.blocks
                    .iter()
                    .position(|block| block.id == normal.block)
            })
            .collect::<Vec<_>>();
        let step_block = step_state.block;
        let step_flow = if let Some(step) = step {
            self.lower_discarded_flow(step, step_state)?
        } else {
            Flow {
                normal: vec![step_state],
            }
        };
        for continued in step_flow.normal {
            self.branch(&continued, header.block, &carried_keys)?;
        }
        if let Some(position) = normal_predecessor_positions.into_iter().min() {
            self.move_block_before(step_block, position)?;
        }
        let exit = self.new_block_from(&header_condition, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        let mut exit = exit;
        for symbol in init_symbols.into_iter().rev() {
            if self.memory_symbols.contains(&symbol)
                && exit.env.contains_key(&EnvKey::Address(symbol))
            {
                self.end_object_lifetime(symbol, &mut exit)?;
            }
            exit.env.remove(&EnvKey::Local(symbol));
            exit.env.remove(&EnvKey::Address(symbol));
            exit.locals.remove(&symbol);
        }
        Ok(Flow { normal: vec![exit] })
    }

    pub(super) fn lower_do_while(
        &mut self,
        body: &'a CHirStmt<'a>,
        condition: &'a CHirExpr<'a>,
        state: State,
        live_after: &BTreeSet<SymbolId>,
        _outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let mut carried = live_after.clone();
        collect_symbols_stmt(body, &mut carried);
        collect_symbols_expr(condition, &mut carried);
        let carried_keys = self.keys_for(&state, &carried, &[]);
        let body_state = self.new_block_from(&state, &carried_keys, BlockRole::Ordinary)?;
        let body_block = body_state.block;
        self.branch(&state, body_state.block, &carried_keys)?;
        let condition_placeholder = self.fresh_placeholder();
        let break_placeholder = self.fresh_placeholder();
        let break_keys = self.keys_for(&body_state, live_after, &[]);
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: Some(JumpTarget {
                id: condition_placeholder,
                keys: carried_keys.clone(),
            }),
        };
        let body_flow = self.lower_stmt(body, body_state.clone(), &carried, &control)?;
        let condition_template = body_flow.normal.first().cloned().unwrap_or(body_state);
        let mut condition_state =
            self.new_block_from(&condition_template, &carried_keys, BlockRole::Ordinary)?;
        self.replace_target(condition_placeholder, condition_state.block);
        for normal in body_flow.normal {
            self.branch(&normal, condition_state.block, &carried_keys)?;
        }
        match constant_truth(self.unit, self.target, condition) {
            Some(true) => {
                let body_arguments = self.edge_values(&condition_state, &carried_keys)?;
                self.terminate(&condition_state, Inst::Br(body_block, body_arguments))?;
            }
            Some(false) => {
                let break_arguments = self.edge_values(&condition_state, &break_keys)?;
                self.terminate(
                    &condition_state,
                    Inst::Br(break_placeholder, break_arguments),
                )?;
            }
            None => {
                let predicate = self.lower_condition(condition, &mut condition_state)?;
                let body_arguments = self.edge_values(&condition_state, &carried_keys)?;
                let break_arguments = self.edge_values(&condition_state, &break_keys)?;
                self.terminate(
                    &condition_state,
                    Inst::CondBr(
                        predicate,
                        body_block,
                        body_arguments,
                        break_placeholder,
                        break_arguments,
                    ),
                )?;
            }
        }
        let exit = self.new_block_from(&condition_state, &break_keys, BlockRole::Ordinary)?;
        self.replace_target(break_placeholder, exit.block);
        Ok(Flow { normal: vec![exit] })
    }
}
