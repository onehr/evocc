//! Lowering effects.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn lower_cleanup(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        // One lexical callback has several mutually exclusive exit sites.
        // Its inlined body's private labels belong to each emission, not to
        // the enclosing function's source-label lookup. Existing edges already
        // hold their concrete BlockIds; keep them and build fresh blocks here.
        let mut references = FunctionReferences::default();
        collect_references_expr(expression, &mut references);
        let private = references
            .defined_labels
            .into_iter()
            .filter(|name| !self.address_labels.contains_key(name))
            .map(|name| {
                let saved = self.labels.remove(&name);
                (name, saved)
            })
            .collect::<Vec<_>>();
        let result = self.lower_discarded_expr(expression, state);
        for (name, saved) in private {
            self.labels.remove(&name);
            if let Some(target) = saved {
                self.labels.insert(name, target);
            }
        }
        result
    }

    pub(super) fn lower_discarded_expr(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<(), LowerError> {
        let live = state.locals.clone();
        let flow = self.lower_discarded_flow(expression, state.clone())?;
        *state = self.expression_continuation(flow, state, &live)?;
        Ok(())
    }

    pub(super) fn expression_continuation(
        &mut self,
        flow: Flow,
        previous: &State,
        live: &BTreeSet<SymbolId>,
    ) -> Result<State, LowerError> {
        if let Some(continued) = self.merge_states(flow.normal, live)? {
            return Ok(continued);
        }
        // Keep an insertion point for the syntactic remainder without adding
        // an edge from a return, goto, break or continue. Only label entries
        // can make subsequent code reachable; ordinary reachability pruning
        // removes this predecessorless continuation. No value is fabricated.
        let keys = previous.env.keys().copied().collect::<Vec<_>>();
        self.new_block_from(previous, &keys, BlockRole::Ordinary)
    }

    /// Discarding a value does not require that evaluating it falls through.
    /// Keep abrupt exits out of the join, rather than fabricating a value or
    /// an edge from a return, break, or goto back into expression evaluation.
    pub(super) fn lower_discarded_flow(
        &mut self,
        expression: &'a CHirExpr<'a>,
        mut state: State,
    ) -> Result<Flow, LowerError> {
        if let Some((place, bits, target_span)) =
            evocc_chir::constant::floating_representation_store(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                self.target,
            )
            .filter(|_| !self.unit.runtime_floating_environment())
        {
            if let PlaceRoot::Object { symbol } = place.root() {
                if !self.memory_symbols.contains(symbol) && place.projections().is_empty() {
                    let kind =
                        floating::scalar_format(self.unit.types(), self.target, place.value_ty())
                            .expect("floating representation store has a floating destination");
                    let value = self.floating_constant(
                        evocc_types::floating::FloatBits::from_bits(kind, bits.raw_bits())
                            .expect("canonical floating object representation"),
                        &state,
                    )?;
                    self.store_local(*symbol, value, target_span, &mut state)?;
                    return Ok(Flow {
                        normal: vec![state],
                    });
                }
            }
            let address = self.lower_place_address(&place, target_span, &mut state)?;
            let value = self.wide_constant(&state, bits.ty(), bits.raw_bits())?;
            let layout = self.place_access_layout(&place, target_span)?;
            self.emit_memory_store_with_layout(
                address,
                value,
                layout,
                place.access_qualifiers().is_volatile,
                &mut state,
            )?;
            return Ok(Flow {
                normal: vec![state],
            });
        }
        match expression {
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                let known_truth =
                    self.known_expression_truth(condition, &[then_expression, else_expression]);
                let predicate = self.lower_condition(condition, &mut state)?;
                let keys = self.expression_join_keys(&state, &[then_expression, else_expression]);
                let then_state = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                let else_state = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                let arguments = self.edge_values(&state, &keys)?;
                self.terminate(
                    &state,
                    conditional_branch(
                        predicate,
                        known_truth,
                        then_state.block,
                        arguments.clone(),
                        else_state.block,
                        arguments,
                    ),
                )?;
                let mut normal = self
                    .lower_discarded_flow(then_expression, then_state)?
                    .normal;
                normal.extend(
                    self.lower_discarded_flow(else_expression, else_state)?
                        .normal,
                );
                if normal.len() <= 1 {
                    return Ok(Flow { normal });
                }
                let merge = self.new_block_from(&normal[0], &keys, BlockRole::Ordinary)?;
                for predecessor in normal {
                    self.branch(&predecessor, merge.block, &keys)?;
                }
                state = merge;
            }
            CHirExpr::Cast(ty, operand, _)
                if matches!(
                    self.unit.types().get(*ty).map(|ty| &ty.kind),
                    Some(TypeKind::Void)
                ) =>
            {
                return self.lower_discarded_flow(operand, state);
            }
            CHirExpr::BuiltinVarArgs {
                op,
                first,
                second,
                requested_ty,
                span,
                ..
            } => {
                self.lower_varargs(*op, first, *second, *requested_ty, *span, &mut state)?;
            }
            CHirExpr::Call(..) => {
                let _ = self.emit_call(expression, &mut state)?;
            }
            CHirExpr::Comma(left, right, _) | CHirExpr::Binary(BinaryOp::Comma, left, right, _) => {
                let flow = self.lower_discarded_flow(left, state)?;
                let mut normal = Vec::new();
                for continued in flow.normal {
                    normal.extend(self.lower_discarded_flow(right, continued)?.normal);
                }
                return Ok(Flow { normal });
            }
            CHirExpr::StatementExpr { body, .. } => {
                let live = state.locals.clone();
                let control = self.active_control.clone();
                return self.lower_stmt(body, state, &live, &control);
            }
            CHirExpr::BuiltinUnreachable { .. } => {
                self.terminate(&state, Inst::Trap(TrapReason::UnreachableReached))?;
                return Ok(Flow::default());
            }
            CHirExpr::BuiltinTrap { .. } => {
                self.terminate(&state, Inst::Trap(TrapReason::BuiltinTrap))?;
                return Ok(Flow::default());
            }
            _ => {
                let _ = self.lower_expr(expression, &mut state)?;
            }
        }
        Ok(Flow {
            normal: vec![state],
        })
    }

    pub(super) fn lower_varargs(
        &mut self,
        op: evocc_chir::VarArgsBuiltinOp,
        first: &'a CHirExpr<'a>,
        second: Option<&'a CHirExpr<'a>>,
        requested_ty: Option<TypeId>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        use evocc_chir::VarArgsBuiltinOp;
        use evocc_evoir::{VariadicAccess, VariadicOp};
        let list = self.lower_expr(first, state)?.value;
        if op == VarArgsBuiltinOp::Copy {
            let source = second.ok_or(LowerError::VarArgs { span })?;
            let source = self.lower_expr(source, state)?.value;
            let mem_in = self.token(state, EnvKey::Memory)?.value;
            let world_in = self.token(state, EnvKey::World)?.value;
            let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
            let world_out = self.fresh_value(EvoIRType::World);
            self.emit(
                state,
                Inst::Copy {
                    destination: list,
                    source,
                    mem_in,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    layout: AccessLayout {
                        size_bytes: self.target.va_list_layout().size_bytes,
                        align_bytes: self.target.va_list_layout().align_bytes,
                    },
                    volatile: CopyVolatility::None,
                },
            )?;
            self.replace_effect_tokens(state, mem_out, Some(world_out));
            return Ok(None);
        }
        let mut record_shape = None;
        let result = if op == VarArgsBuiltinOp::Arg {
            let ty = requested_ty.ok_or(LowerError::VarArgs { span })?;
            if evocc_chir::half_interface_refused(self.unit.types(), self.target, ty, true) {
                return Err(LowerError::FloatingCallInterface { span });
            }
            if self.is_aggregate_type(ty) {
                if !evocc_chir::is_integer_variadic_record(self.unit.types(), self.target, ty) {
                    return Err(LowerError::VarArgs { span });
                }
                let layout = self
                    .target
                    .layout_of(self.unit.types(), ty)
                    .map_err(|source| LowerError::Layout { ty, span, source })?;
                record_shape = Some(RecordShape {
                    size: layout.size_bytes,
                    align: layout.align_bytes,
                    vector: homogeneous_vector_aggregate(self.unit.types(), self.target, ty),
                    floating: evocc_chir::homogeneous_floating_aggregate(
                        self.unit.types(),
                        self.target,
                        ty,
                    ),
                });
                let origin = self.synthetic_origin()?;
                Some(ExprValue {
                    value: self.emit_alloca(ty, span, origin, state)?,
                    integer: CIntegerType::ULONG,
                })
            } else {
                let value_type = function_value_type(self.unit.types(), self.target, ty, span)?;
                Some(ExprValue {
                    value: self.fresh_value(value_type.ir),
                    integer: value_type.integer,
                })
            }
        } else {
            None
        };
        let op = match op {
            VarArgsBuiltinOp::Start => VariadicOp::Start,
            VarArgsBuiltinOp::End => VariadicOp::End,
            VarArgsBuiltinOp::Arg => match record_shape {
                Some(shape) => VariadicOp::ArgRecord {
                    destination: result.as_ref().unwrap().value.clone(),
                    shape,
                },
                None => VariadicOp::Arg(result.as_ref().unwrap().value.clone()),
            },
            VarArgsBuiltinOp::Copy => unreachable!("copy returned above"),
        };
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        self.emit(
            state,
            Inst::VaAccess(VariadicAccess {
                op,
                list,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }),
        )?;
        self.replace_effect_tokens(state, mem_out, Some(world_out));
        Ok(result)
    }
}
