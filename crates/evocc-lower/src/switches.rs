//! Lowering switches.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_nested_switch(
        &mut self,
        expression: &'a CHirExpr<'a>,
        body: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        let selector = self.lower_expr(expression, &mut state)?;
        let selector = self.cast(
            selector.clone(),
            integer_promotion(selector.integer),
            &mut state,
        )?;
        let known_selector = self.known_switch_selector(expression, body, selector.integer);
        let selector_key = self.retain_temporary(selector.value, &mut state);
        let mut labels = Vec::new();
        collect_nested_switch_labels(body, &mut labels);
        let mut carried = live_after.clone();
        collect_symbols_stmt(body, &mut carried);
        let keys = self.keys_for(&state, &carried, &[]);
        let mut exit_keys = self.keys_for(&state, live_after, &[]);
        exit_keys.retain(|key| *key != selector_key);
        let exit = self.new_block_from(&state, &exit_keys, BlockRole::SwitchExit)?;
        let mut entries = Vec::new();
        for (statement, matcher) in labels {
            // Punctuation separates these names from C labels; ordinals are
            // deterministic and do not serialize host pointer identities.
            let name = format!(":case:{}", self.case_labels.len());
            self.case_labels
                .insert(statement as *const CHirStmt<'_> as usize, name.clone());
            let entry = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
            self.pending_gotos.insert(name, vec![entry.clone()]);
            entries.push((matcher, entry));
        }
        let default = entries.iter().position(|(matcher, _)| matcher.is_none());
        let mut dispatch = state.clone();
        for (matcher, entry) in &entries {
            let Some(matcher) = matcher else { continue };
            let next = self.new_block_from(&dispatch, &keys, BlockRole::Ordinary)?;
            if let Some(selector) = known_selector {
                let target = if self.switch_case_matches(*matcher, selector)? {
                    entry.block
                } else {
                    next.block
                };
                self.branch(&dispatch, target, &keys)?;
                dispatch = next;
                continue;
            }
            let selected = self.token(&dispatch, selector_key)?;
            let condition = self.lower_switch_condition(
                *matcher,
                ExprValue {
                    value: selected.value,
                    integer: selector.integer,
                },
                &mut dispatch,
            )?;
            let true_args = self.edge_values(&dispatch, &keys)?;
            let false_args = self.edge_values(&dispatch, &keys)?;
            self.terminate(
                &dispatch,
                Inst::CondBr(condition, entry.block, true_args, next.block, false_args),
            )?;
            dispatch = next;
        }
        if let Some(index) = default {
            self.branch(&dispatch, entries[index].1.block, &keys)?;
        } else {
            self.branch(&dispatch, exit.block, &exit_keys)?;
        }
        // Walk syntax once from a disconnected entry. Dispatch reaches the
        // label without executing enclosing guards or loop initializers.
        let detached = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: exit.block,
                keys: exit_keys.clone(),
            }),
            continue_target: outer.continue_target.clone(),
        };
        // lower_stmt already activated the switch body's lexical storage.
        // Do not allocate the same compound-literal site a second time.
        for normal in self
            .lower_stmt_active(body, detached, live_after, &control)?
            .normal
        {
            self.branch(&normal, exit.block, &exit_keys)?;
        }
        Ok(Flow { normal: vec![exit] })
    }

    pub(super) fn known_switch_selector(
        &self,
        expression: &CHirExpr<'_>,
        body: &CHirStmt<'_>,
        integer: CIntegerType,
    ) -> Option<CInteger> {
        if self.unit.runtime_floating_environment() || self.contains_addressed_label(body) {
            return None;
        }
        constant_integer(self.unit, self.target, expression).map(|v| v.cast(integer))
    }

    pub(super) fn lower_case_constant(
        &mut self,
        expression: &'a CHirExpr<'a>,
        integer: CIntegerType,
        state: &State,
    ) -> Result<ExprValue, LowerError> {
        let value = self.case_constant(expression, integer)?;
        self.wide_constant(state, integer, value.raw_bits())
    }

    pub(super) fn case_constant(
        &self,
        expression: &'a CHirExpr<'a>,
        integer: CIntegerType,
    ) -> Result<CInteger, LowerError> {
        let value = evocc_chir::interp::interpret_static_integer_expression(
            self.unit,
            expression,
            self.target,
        )
        .map_err(|_| LowerError::UnsupportedExpression {
            span: expression.span(),
            feature: "case value is not an integer constant expression",
        })?
        .cast(integer);
        Ok(value)
    }

    pub(super) fn switch_case_matches(
        &self,
        matcher: SwitchCase<'a>,
        selector: CInteger,
    ) -> Result<bool, LowerError> {
        match matcher {
            SwitchCase::Single(expression) => Ok(self
                .case_constant(expression, selector.ty())?
                .raw_bits()
                == selector.raw_bits()),
            SwitchCase::Range(first, last) => {
                let first = self.case_constant(first, selector.ty())?;
                let last = self.case_constant(last, selector.ty())?;
                // Both operands have the promoted selector type. Comparing
                // them does not discharge any source arithmetic obligation.
                Ok(
                    eval_binary_integer(BinaryOp::Le, first, selector).is_ok_and(CInteger::truth)
                        && eval_binary_integer(BinaryOp::Le, selector, last)
                            .is_ok_and(CInteger::truth),
                )
            }
        }
    }

    pub(super) fn lower_switch_condition(
        &mut self,
        case_matcher: SwitchCase<'a>,
        selector: ExprValue,
        dispatch: &mut State,
    ) -> Result<Value, LowerError> {
        let compared_type = selector.integer;
        let condition = match case_matcher {
            SwitchCase::Single(expression) => {
                let case_value = self.lower_case_constant(expression, compared_type, dispatch)?;
                let condition = self.fresh_value(EvoIRType::Bits(1));
                self.emit(
                    dispatch,
                    Inst::ICmp(
                        condition.clone(),
                        CmpOp::Eq,
                        selector.value,
                        case_value.value,
                    ),
                )?;
                condition
            }
            SwitchCase::Range(first, last) => {
                let first = self.lower_case_constant(first, compared_type, dispatch)?;
                let last = self.lower_case_constant(last, compared_type, dispatch)?;
                let comparison = if compared_type.is_signed() {
                    CmpOp::SLe
                } else {
                    CmpOp::ULe
                };
                let above_first = self.fresh_value(EvoIRType::Bits(1));
                let below_last = self.fresh_value(EvoIRType::Bits(1));
                let condition = self.fresh_value(EvoIRType::Bits(1));
                self.emit(
                    dispatch,
                    Inst::ICmp(
                        above_first.clone(),
                        comparison,
                        first.value,
                        selector.value.clone(),
                    ),
                )?;
                self.emit(
                    dispatch,
                    Inst::ICmp(below_last.clone(), comparison, selector.value, last.value),
                )?;
                self.emit(
                    dispatch,
                    Inst::And(condition.clone(), above_first, below_last),
                )?;
                condition
            }
        };
        Ok(condition)
    }

    // A constant bit mask gives an over-approximation [0, mask]. Prove that
    // ordinary cases cover that entire interval; never assume that the
    // mere presence of an assertion in default makes it unreachable.
    pub(super) fn switch_cases_cover_mask(
        &self,
        expression: &CHirExpr<'_>,
        integer: CIntegerType,
        cases: impl Iterator<Item = SwitchCase<'a>>,
    ) -> bool {
        let CHirExpr::Binary(BinaryOp::BitAnd, left, right, _) = expression else {
            return false;
        };
        let Some(mask) = self
            .result_after_evaluation(right)
            .or_else(|| self.result_after_evaluation(left))
        else {
            return false;
        };
        let upper = mask.cast(integer).raw_bits();
        if integer.is_signed() && upper > integer.signed_max() as u128 {
            return false;
        }
        let mut intervals = Vec::new();
        for case in cases {
            let (first, last) = match case {
                SwitchCase::Single(value) => (value, value),
                SwitchCase::Range(first, last) => (first, last),
            };
            let (Ok(first), Ok(last)) = (
                self.case_constant(first, integer),
                self.case_constant(last, integer),
            ) else {
                return false;
            };
            intervals.push((first.raw_bits(), last.raw_bits()));
        }
        intervals.sort_unstable();
        let mut next = 0;
        for (first, last) in intervals {
            if last < next {
                continue;
            }
            if first > next {
                return false;
            }
            if last >= upper {
                return true;
            }
            next = last + 1;
        }
        false
    }

    pub(super) fn lower_switch(
        &mut self,
        expression: &'a CHirExpr<'a>,
        body: &'a CHirStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
        outer: &ControlTargets,
    ) -> Result<Flow, LowerError> {
        if self.unit.runtime_floating_environment()
            || !switch_has_flat_labels(body)
            || matches!(body, CHirStmt::Compound(items, _, _) if items.iter().any(|item| {
                matches!(item, CHirBlockItem::Declaration(CHirDecl::Object(object))
                    if self.memory_symbols.contains(&object.symbol))
            }))
        {
            return self.lower_nested_switch(expression, body, state, live_after, outer);
        }
        let selector = self.lower_expr(expression, &mut state)?;
        let selector_type = integer_promotion(selector.integer);
        let known_selector = self.known_switch_selector(expression, body, selector_type);
        let already_promoted = selector.integer == selector_type;
        let selector = self.cast(selector, selector_type, &mut state)?;
        let selector_temporary = if matches!(expression, CHirExpr::Ident { symbol, .. }
            if already_promoted && !self.global_registers.contains_key(symbol))
        {
            None
        } else {
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: selector.value.clone(),
                    integer: Some(selector.integer),
                },
            );
            Some(key)
        };
        let (prefix, segments) = switch_segments(body, self.unit, self.target, selector_type)?;
        let exhaustive = self.switch_cases_cover_mask(
            expression,
            selector_type,
            segments.iter().filter_map(|segment| segment.case),
        );
        let last_case = segments.iter().rposition(|segment| segment.case.is_some());
        let default_index = segments.iter().position(|segment| segment.case.is_none());
        // Every failed case comparison may ultimately enter default, even
        // when its label was written before that case. Source-order suffix
        // liveness alone loses values used only by this dynamic successor.
        // Include its fallthrough suffix, but keep the values on explicit
        // edges rather than making entry definitions visible across blocks.
        let mut default_symbols = BTreeSet::new();
        if let Some(default) = default_index {
            for segment in &segments[default..] {
                for item in &segment.items {
                    collect_symbols_switch_item(item, &mut default_symbols);
                }
            }
        }
        let mut switch_locals = state.locals.clone();
        for item in prefix {
            if let CHirBlockItem::Declaration(CHirDecl::Object(object)) = item {
                switch_locals.insert(object.symbol);
            }
        }
        for segment in &segments {
            for item in &segment.items {
                if let SwitchSegmentItem::Declaration(CHirDecl::Object(object)) = item {
                    switch_locals.insert(object.symbol);
                }
            }
        }
        let prefix_is_targeted = prefix.iter().any(|item| {
            matches!(
                item,
                CHirBlockItem::Statement(statement)
                    if contains_targeted_label(statement, &self.goto_names, &self.case_labels)
            )
        });
        if segments.is_empty() {
            if !prefix_is_targeted {
                if let Some(selector_temporary) = selector_temporary {
                    state.env.remove(&selector_temporary);
                }
                return Ok(Flow {
                    normal: vec![state],
                });
            }
            let mut prefix_symbols = live_after.clone();
            for item in prefix {
                collect_symbols_item(item, &mut prefix_symbols);
            }
            let mut prefix_template = state.clone();
            prefix_template.locals.clone_from(&switch_locals);
            let mut prefix_keys = self.keys_for(&prefix_template, &prefix_symbols, &[]);
            if let Some(selector_temporary) = selector_temporary {
                prefix_keys.retain(|key| *key != selector_temporary);
            }
            let detached =
                self.new_block_from(&prefix_template, &prefix_keys, BlockRole::Ordinary)?;
            let exit_keys = self.keys_for(&state, live_after, &[]);
            let exit = self.new_block_from(&state, &exit_keys, BlockRole::SwitchExit)?;
            let control = ControlTargets {
                break_target: Some(JumpTarget {
                    id: exit.block,
                    keys: exit_keys.clone(),
                }),
                continue_target: outer.continue_target.clone(),
            };
            let prefix_flow =
                self.lower_compound_items(prefix, detached, live_after, &control, false)?;
            self.branch(&state, exit.block, &exit_keys)?;
            for normal in prefix_flow.normal {
                self.branch(&normal, exit.block, &exit_keys)?;
            }
            return Ok(Flow { normal: vec![exit] });
        }
        let break_placeholder = self.fresh_placeholder();
        let mut break_keys = self.keys_for(&state, live_after, &[]);
        if let Some(selector_temporary) = selector_temporary {
            break_keys.retain(|key| *key != selector_temporary);
        }
        let mut entries = Vec::new();
        let mut dispatch_states = Vec::new();
        let mut current_dispatch = state.clone();
        for (index, segment) in segments.iter().enumerate() {
            let mut symbols = live_after.clone();
            for later in &segments[index..] {
                for item in &later.items {
                    collect_symbols_switch_item(item, &mut symbols);
                }
            }
            let mut entry_template = current_dispatch.clone();
            entry_template.locals.clone_from(&switch_locals);
            let mut keys = self.keys_for(&entry_template, &symbols, &[]);
            if let Some(selector_temporary) = selector_temporary {
                keys.retain(|key| *key != selector_temporary);
            }
            let entry = self.new_block_from(&entry_template, &keys, BlockRole::Ordinary)?;
            entries.push((entry, keys));
            if segment.case.is_some() {
                let next_case = segments[index + 1..]
                    .iter()
                    .position(|candidate| candidate.case.is_some())
                    .map(|offset| index + 1 + offset);
                if next_case.is_some() {
                    let mut remaining = live_after.clone();
                    remaining.extend(&default_symbols);
                    collect_symbols_expr(expression, &mut remaining);
                    for later in &segments[index + 1..] {
                        if let Some(case) = later.case {
                            match case {
                                SwitchCase::Single(expression) => {
                                    collect_symbols_expr(expression, &mut remaining);
                                }
                                SwitchCase::Range(first, last) => {
                                    collect_symbols_expr(first, &mut remaining);
                                    collect_symbols_expr(last, &mut remaining);
                                }
                            }
                        }
                        for item in &later.items {
                            collect_symbols_switch_item(item, &mut remaining);
                        }
                    }
                    let next_keys = self.keys_for(&current_dispatch, &remaining, &[]);
                    let next =
                        self.new_block_from(&current_dispatch, &next_keys, BlockRole::Ordinary)?;
                    dispatch_states.push((index, current_dispatch, next.clone(), next_keys));
                    current_dispatch = next;
                } else {
                    dispatch_states.push((
                        index,
                        current_dispatch.clone(),
                        entries[index].0.clone(),
                        Vec::new(),
                    ));
                }
            }
        }

        if dispatch_states.is_empty() {
            let default = default_index.ok_or(LowerError::InvalidControlFlow {
                span: body.span(),
                reason: "switch has no case or default dispatch target",
            })?;
            self.branch(&state, entries[default].0.block, &entries[default].1)?;
        }
        for (position, (index, mut dispatch, next, next_keys)) in
            dispatch_states.into_iter().enumerate()
        {
            let case_matcher = segments[index].case.ok_or(LowerError::InvalidControlFlow {
                span: body.span(),
                reason: "switch dispatch does not reference a case",
            })?;
            let current_selector = if selector_temporary.is_none() {
                let CHirExpr::Ident { symbol, span, .. } = expression else {
                    unreachable!("only a local selector can omit its temporary")
                };
                self.local(&mut dispatch, *symbol, *span)?
            } else {
                let key = selector_temporary.ok_or(LowerError::InvalidControlFlow {
                    span: expression.span(),
                    reason: "switch selector temporary is absent",
                })?;
                let bound = dispatch
                    .env
                    .get(&key)
                    .ok_or(LowerError::InvalidControlFlow {
                        span: expression.span(),
                        reason: "switch selector was not threaded to a dispatch block",
                    })?;
                ExprValue {
                    value: bound.value.clone(),
                    integer: bound.integer.ok_or(LowerError::InvalidControlFlow {
                        span: expression.span(),
                        reason: "switch selector temporary is not an integer",
                    })?,
                }
            };
            let true_entry = &entries[index];
            let (false_target, false_args) =
                if position + 1 < segments.len() && !next_keys.is_empty() {
                    (next.block, self.edge_values(&dispatch, &next_keys)?)
                } else if let Some(default) = default_index {
                    let default_entry = &entries[default];
                    (
                        default_entry.0.block,
                        self.edge_values(&dispatch, &default_entry.1)?,
                    )
                } else {
                    (break_placeholder, self.edge_values(&dispatch, &break_keys)?)
                };
            let true_args = self.edge_values(&dispatch, &true_entry.1)?;
            if let Some(selector) = known_selector {
                let (target, arguments) = if self.switch_case_matches(case_matcher, selector)? {
                    (true_entry.0.block, true_args)
                } else {
                    (false_target, false_args)
                };
                self.terminate(&dispatch, Inst::Br(target, arguments))?;
                continue;
            }
            if exhaustive && Some(index) == last_case {
                self.terminate(&dispatch, Inst::Br(true_entry.0.block, true_args))?;
                continue;
            }
            let condition =
                self.lower_switch_condition(case_matcher, current_selector, &mut dispatch)?;
            self.terminate(
                &dispatch,
                Inst::CondBr(
                    condition,
                    true_entry.0.block,
                    true_args,
                    false_target,
                    false_args,
                ),
            )?;
        }

        let control = ControlTargets {
            break_target: Some(JumpTarget {
                id: break_placeholder,
                keys: break_keys.clone(),
            }),
            continue_target: outer.continue_target.clone(),
        };
        if prefix_is_targeted {
            let mut prefix_live = live_after.clone();
            for segment in &segments {
                for item in &segment.items {
                    collect_symbols_switch_item(item, &mut prefix_live);
                }
            }
            let mut prefix_template = state.clone();
            prefix_template.locals.clone_from(&switch_locals);
            let mut prefix_keys = self.keys_for(&prefix_template, &prefix_live, &[]);
            if let Some(selector_temporary) = selector_temporary {
                prefix_keys.retain(|key| *key != selector_temporary);
            }
            let detached =
                self.new_block_from(&prefix_template, &prefix_keys, BlockRole::Ordinary)?;
            let prefix_flow =
                self.lower_compound_items(prefix, detached, &prefix_live, &control, false)?;
            for normal in prefix_flow.normal {
                self.branch(&normal, entries[0].0.block, &entries[0].1)?;
            }
        }
        for index in 0..segments.len() {
            let mut future = live_after.clone();
            for later in &segments[index + 1..] {
                for later_item in &later.items {
                    collect_symbols_switch_item(later_item, &mut future);
                }
            }
            // A terminated segment can still contain goto targets. Use the
            // same label, liveness and detached-entry handling as a compound,
            // without inventing a lexical scope at each case label.
            let normal = self
                .lower_sequence(
                    segments[index].items.iter().copied(),
                    entries[index].0.clone(),
                    &future,
                    &control,
                    false,
                )?
                .normal;
            if index + 1 < segments.len() {
                let next = &entries[index + 1];
                for current in normal {
                    self.branch(&current, next.0.block, &next.1)?;
                }
            } else {
                for current in normal {
                    self.branch(&current, break_placeholder, &break_keys)?;
                }
            }
        }
        let exit = self.new_block_from(&state, &break_keys, BlockRole::SwitchExit)?;
        self.replace_target(break_placeholder, exit.block);
        Ok(Flow { normal: vec![exit] })
    }
}

pub(super) fn collect_nested_switch_labels<'a>(
    statement: &'a CHirStmt<'a>,
    labels: &mut Vec<(&'a CHirStmt<'a>, Option<SwitchCase<'a>>)>,
) {
    match statement {
        CHirStmt::Case(value, child, _) => {
            labels.push((statement, Some(SwitchCase::Single(value))));
            collect_nested_switch_labels(child, labels);
        }
        CHirStmt::CaseRange(first, last, child, _) => {
            labels.push((statement, Some(SwitchCase::Range(first, last))));
            collect_nested_switch_labels(child, labels);
        }
        CHirStmt::Default(child, _) => {
            labels.push((statement, None));
            collect_nested_switch_labels(child, labels);
        }
        CHirStmt::If(_, then_branch, else_branch, _) => {
            collect_nested_switch_labels(then_branch, labels);
            if let Some(branch) = else_branch {
                collect_nested_switch_labels(branch, labels);
            }
        }
        CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Label(_, body, _) => collect_nested_switch_labels(body, labels),
        CHirStmt::Compound(items, _, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_nested_switch_labels(statement, labels);
                }
            }
        }
        // A nested switch's labels do not participate in this dispatch.
        _ => {}
    }
}

pub(super) fn switch_has_flat_labels(body: &CHirStmt<'_>) -> bool {
    let CHirStmt::Compound(items, _, _) = body else {
        return false;
    };
    items.iter().all(|item| {
        let CHirBlockItem::Statement(mut statement) = item else {
            return true;
        };
        while let CHirStmt::Case(_, child, _)
        | CHirStmt::CaseRange(_, _, child, _)
        | CHirStmt::Default(child, _) = statement
        {
            statement = child;
        }
        let mut nested = Vec::new();
        collect_nested_switch_labels(statement, &mut nested);
        nested.is_empty()
    })
}

pub(super) fn switch_segments<'a>(
    body: &'a CHirStmt<'a>,
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    selector_type: CIntegerType,
) -> Result<(&'a [CHirBlockItem<'a>], Vec<SwitchSegment<'a>>), LowerError> {
    let CHirStmt::Compound(items, _, _) = body else {
        return Err(LowerError::UnsupportedStatement {
            span: body.span(),
            feature: "switch body must be a compound statement",
        });
    };
    let mut segments: Vec<SwitchSegment<'a>> = Vec::new();
    let mut prefix_end = items.len();
    for (index, item) in items.iter().enumerate() {
        let CHirBlockItem::Statement(statement) = item else {
            let Some(segment) = segments.last_mut() else {
                // Ordinary switch dispatch jumps directly to a case/default;
                // declarations before the first label establish scope but do
                // not execute their initializer.
                continue;
            };
            let CHirBlockItem::Declaration(declaration) = item else {
                return Err(LowerError::InvalidControlFlow {
                    span: body.span(),
                    reason: "switch block item has an unknown form",
                });
            };
            segment
                .items
                .push(SwitchSegmentItem::Declaration(declaration));
            continue;
        };
        match statement {
            CHirStmt::Case(..) | CHirStmt::CaseRange(..) | CHirStmt::Default(..) => {
                if segments.is_empty() {
                    prefix_end = index;
                }
                let (labels, nested) = switch_label_chain(statement, unit, target, selector_type);
                let label_count = labels.len();
                for (label_index, case) in labels.into_iter().enumerate() {
                    let items = if label_index + 1 == label_count {
                        vec![SwitchSegmentItem::Statement(nested)]
                    } else {
                        Vec::new()
                    };
                    segments.push(SwitchSegment { case, items });
                }
            }
            other => {
                let Some(segment) = segments.last_mut() else {
                    // Statements before the first case/default are bypassed by
                    // normal switch dispatch. A separately targeted ordinary
                    // label is materialized by the detached-goto path.
                    continue;
                };
                segment.items.push(SwitchSegmentItem::Statement(other));
            }
        }
    }
    Ok((&items[..prefix_end], segments))
}

pub(super) fn switch_label_chain<'a>(
    statement: &'a CHirStmt<'a>,
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    selector_type: CIntegerType,
) -> (Vec<Option<SwitchCase<'a>>>, &'a CHirStmt<'a>) {
    let mut labels = Vec::new();
    let mut nested = statement;
    loop {
        match nested {
            CHirStmt::Case(value, body, _) => {
                labels.push(Some(SwitchCase::Single(value)));
                nested = body;
            }
            CHirStmt::CaseRange(first, last, body, _) => {
                labels.push(Some(SwitchCase::Range(first, last)));
                nested = body;
            }
            CHirStmt::Default(body, _) => {
                labels.push(None);
                nested = body;
            }
            _ => break,
        }
    }

    let mut coalesced = Vec::with_capacity(labels.len());
    let mut index = 0;
    while index < labels.len() {
        let Some(SwitchCase::Single(first)) = labels[index] else {
            coalesced.push(labels[index]);
            index += 1;
            continue;
        };
        let mut last = first;
        let mut last_value = normalized_case_value(unit, target, first, selector_type);
        let mut end = index + 1;
        while end < labels.len() {
            let Some(SwitchCase::Single(candidate)) = labels[end] else {
                break;
            };
            let candidate_value = normalized_case_value(unit, target, candidate, selector_type);
            if last_value.and_then(|value| value.checked_add(1)) != candidate_value {
                break;
            }
            last = candidate;
            last_value = candidate_value;
            end += 1;
        }
        if end == index + 1 {
            coalesced.push(Some(SwitchCase::Single(first)));
        } else {
            coalesced.push(Some(SwitchCase::Range(first, last)));
        }
        index = end;
    }
    (coalesced, nested)
}

pub(super) fn normalized_case_value(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    expression: &CHirExpr<'_>,
    selector_type: CIntegerType,
) -> Option<i128> {
    constant_integer(unit, target, expression)
        .map(|value| value.cast(selector_type).mathematical_value())
}

pub(super) fn collect_symbols_switch_item(
    item: &SwitchSegmentItem<'_>,
    symbols: &mut BTreeSet<SymbolId>,
) {
    match item {
        SwitchSegmentItem::Declaration(declaration) => {
            collect_symbols_item(&CHirBlockItem::Declaration(declaration), symbols);
        }
        SwitchSegmentItem::Statement(statement) => collect_symbols_stmt(statement, symbols),
    }
}
