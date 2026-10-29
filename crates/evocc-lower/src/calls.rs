//! Lowering calls.

use super::*;

impl<'a> FunctionLowerer<'a> {
    /// The obligations of a call without a prototype of an old-style
    /// definition, after its arguments ran and before the call (EvoIR
    /// 10.1.7): `const.bits<1> 0` for an undefined call, and `icmp.sle` of
    /// zero and the argument for each signed/unsigned pair.
    pub(super) fn emit_old_style_obligations(
        &mut self,
        verdict: &OldStyleVerdict,
        arguments: &[Value],
        span: SourceSpan,
        state: &State,
    ) -> Result<(), LowerError> {
        let previous = std::mem::replace(&mut self.source_span, span);
        let result = self.old_style_obligations(verdict, arguments, span, state);
        self.source_span = previous;
        result
    }

    pub(super) fn old_style_obligations(
        &mut self,
        verdict: &OldStyleVerdict,
        arguments: &[Value],
        span: SourceSpan,
        state: &State,
    ) -> Result<(), LowerError> {
        let mut conditions = Vec::new();
        match verdict {
            OldStyleVerdict::Agrees => {}
            OldStyleVerdict::Undefined => {
                let never = self.fresh_value(EvoIRType::Bits(1));
                self.emit(state, Inst::Const(never.clone(), 0))?;
                conditions.push(never);
            }
            OldStyleVerdict::IfRepresentable(indices) => {
                for index in indices {
                    let argument =
                        arguments
                            .get(*index)
                            .cloned()
                            .ok_or(LowerError::InvalidControlFlow {
                                span,
                                reason: "old-style call verdict names a missing argument",
                            })?;
                    let EvoIRType::Bits(width) = argument.ty else {
                        return Err(LowerError::UnsupportedExpression {
                            span,
                            feature: "old-style call integer argument is not bits",
                        });
                    };
                    let zero = self.fresh_value(EvoIRType::Bits(width));
                    self.emit(state, Inst::Const(zero.clone(), 0))?;
                    let fits = self.fresh_value(EvoIRType::Bits(1));
                    self.emit(state, Inst::ICmp(fits.clone(), CmpOp::SLe, zero, argument))?;
                    conditions.push(fits);
                }
            }
        }
        for condition in conditions {
            self.emit(state, Inst::UbAssume(condition, UbReason::OldStyleCall))?;
        }
        Ok(())
    }

    pub(super) fn emit_call(
        &mut self,
        expression: &'a CHirExpr<'a>,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        let CHirExpr::Call(callee, arguments, span) = expression else {
            unreachable!("call lowering receives a call expression")
        };
        let span = *span;
        if let Some(size) = self.unit.stack_allocation_size(expression) {
            let size = self.lower_expr(size, state)?;
            let size = self.cast(size, CIntegerType::ULONG, state)?;
            let origin = *self
                .dynamic_origins
                .get(&(expression as *const CHirExpr<'_> as usize))
                .ok_or(LowerError::UnsupportedExpression {
                    span,
                    feature: "dynamic allocation has no origin",
                })?;
            let mem_in = self.token(state, EnvKey::Memory)?.value;
            let world_in = self.token(state, EnvKey::World)?.value;
            let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
            let world_out = self.fresh_value(EvoIRType::World);
            self.emit(
                state,
                Inst::DynamicAlloca {
                    result: result.clone(),
                    mem_in,
                    world_in,
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    origin,
                    size: size.value,
                    align: 16,
                },
            )?;
            self.replace_effect_tokens(state, mem_out, Some(world_out));
            return Ok(Some(ExprValue {
                value: result,
                integer: CIntegerType::ULONG,
            }));
        }
        // EvoIR 10.1.7: a call without a prototype of an old-style
        // definition. An undefined call passes the promoted arguments through
        // the function's address, as GCC 14 does.
        let old_style =
            evocc_chir::old_style_calls::call_verdict(self.unit, self.target, callee, arguments);
        let undefined_old_style = old_style == Some(OldStyleVerdict::Undefined);
        let direct = match callee {
            CHirExpr::Ident {
                symbol,
                spelling,
                facts,
                span: designator_span,
            } if facts.category() == ExprCategory::FunctionDesignator => {
                let designator =
                    self.unit
                        .symbols()
                        .symbol(*symbol)
                        .map_err(|_| LowerError::UnknownSymbol {
                            symbol: *symbol,
                            span: *designator_span,
                        })?;
                if designator.kind() != SymbolKind::Function
                    || designator.spelling() != spelling
                    || !evocc_chir::type_at_use_matches_binding(
                        self.unit.types(),
                        facts.ty(),
                        designator.ty(),
                    )
                {
                    return Err(LowerError::UnsupportedExpression {
                        span: *designator_span,
                        feature: "function designator disagrees with its semantic identity",
                    });
                }
                if self.unit.symbols().is_builtin_function(*symbol) {
                    if evocc_types::neon::signature(spelling).is_some()
                        || evocc_types::neon::apple_signature(spelling).is_some()
                    {
                        return self.lower_neon_call(spelling, facts.ty(), arguments, span, state);
                    }
                    match spelling.as_str() {
                        "__sync_synchronize" => {
                            if !arguments.is_empty() {
                                return Err(LowerError::UnsupportedExpression {
                                    span,
                                    feature: "full memory barrier arity",
                                });
                            }
                            // Use the existing opaque, ordered assembly
                            // path: this is a hardware fence, not a call.
                            self.emit_barrier("dmb ish", span, state)?;
                            return Ok(None);
                        }
                        "__atomic_thread_fence" | "__atomic_signal_fence" => {
                            let [argument] = arguments.as_slice() else {
                                return Err(LowerError::UnsupportedExpression {
                                    span,
                                    feature: "fence arity",
                                });
                            };
                            // EvoIR 10.1.14: a run-time order is evaluated
                            // once and makes the fence seq_cst. The front end
                            // already removed a relaxed constant fence.
                            let order = constant_integer(self.unit, self.target, argument)
                                .and_then(|value| value.try_mathematical_value())
                                .map_or(
                                    Some(evocc_evoir::AtomicOrder::SeqCst),
                                    evocc_evoir::AtomicOrder::from_c,
                                )
                                .ok_or(LowerError::UnsupportedExpression {
                                    span,
                                    feature: "invalid fence order",
                                })?;
                            self.lower_discarded_expr(argument, state)?;
                            // A thread fence is the same opaque, ordered
                            // barrier as `__sync_synchronize`, `dmb ishld`
                            // when it only acquires. A signal fence orders the
                            // compiler only: an empty barrier template.
                            use evocc_evoir::AtomicOrder as O;
                            let template = if spelling == "__atomic_signal_fence" {
                                ""
                            } else if matches!(order, O::Consume | O::Acquire) {
                                "dmb ishld"
                            } else {
                                "dmb ish"
                            };
                            if order != O::Relaxed {
                                self.emit_barrier(template, span, state)?;
                            }
                            return Ok(None);
                        }
                        "__builtin_extract_return_addr" => {
                            let [argument] = arguments.as_slice() else {
                                return Err(LowerError::UnsupportedExpression {
                                    span,
                                    feature: "return-address extraction arity",
                                });
                            };
                            return self.lower_expr(argument, state).map(Some);
                        }
                        "__builtin_prefetch" => {
                            // A target hint is optional; evaluating its
                            // operands is not. Keep their effects once.
                            for argument in arguments {
                                self.lower_discarded_expr(argument, state)?;
                            }
                            return Ok(None);
                        }
                        "alloca" => {
                            return Err(LowerError::UnsupportedExpression {
                                span,
                                feature: "malformed dynamic allocation call",
                            });
                        }
                        _ => {}
                    }
                }
                if undefined_old_style {
                    None
                } else {
                    Some(
                        self.functions
                            .get(symbol)
                            .ok_or(LowerError::UnknownSymbol {
                                symbol: *symbol,
                                span,
                            })?,
                    )
                }
            }
            // GCC folds `*f` and `&f` of a function designator, so such a
            // call of a returns-twice function is direct (EvoIR 3.10). Every
            // other designator keeps its indirect lowering.
            _ => evocc_chir::returns_twice::designated_function(callee)
                .and_then(|symbol| self.functions.get(&symbol))
                .filter(|function| function.returns_twice),
        };
        // The binding type of an old-style definition is its promoted
        // prototype; an undefined call takes the type without a prototype
        // that the designator has, so its signature is that of its promoted
        // arguments.
        let function_ty = match callee {
            CHirExpr::Ident { facts, .. } if undefined_old_style => facts.ty(),
            _ => self.call_designator_function_type(callee).ok_or(
                LowerError::UnsupportedExpression {
                    span: callee.span(),
                    feature: "indirect call designator has no function signature",
                },
            )?,
        };
        let (mut parameter_types, return_type, direct_name) = if let Some(function) = direct {
            (
                function.parameter_types.clone(),
                function.return_type.clone(),
                Some(function.name.clone()),
            )
        } else {
            let (parameters, returned) =
                self.scalar_call_signature(function_ty, arguments, span)?;
            (parameters, returned, None)
        };
        let fixed_parameter_count = parameter_types.len();
        let variadic = matches!(
            self.unit.types().get(function_ty).map(|ty| &ty.kind),
            Some(TypeKind::Function {
                parameters: FunctionParameters::Prototype { variadic: true, .. },
                ..
            })
        );
        if variadic {
            for argument in arguments.iter().skip(parameter_types.len()) {
                let CHirExpr::Cast(ty, _, _) = argument else {
                    return Err(LowerError::UnsupportedExpression {
                        span: argument.span(),
                        feature: "anonymous argument lacks default promotion",
                    });
                };
                if evocc_chir::half_interface_refused(self.unit.types(), self.target, *ty, true) {
                    return Err(LowerError::FloatingCallInterface {
                        span: argument.span(),
                    });
                }
                if self.is_aggregate_type(*ty)
                    && !evocc_chir::is_integer_variadic_record(self.unit.types(), self.target, *ty)
                {
                    return Err(LowerError::VarArgs {
                        span: argument.span(),
                    });
                }
                parameter_types.push(function_value_type(
                    self.unit.types(),
                    self.target,
                    *ty,
                    argument.span(),
                )?);
            }
        }
        if arguments.len() != parameter_types.len() {
            return Err(LowerError::CallArity {
                function: direct_name
                    .clone()
                    .unwrap_or_else(|| "<indirect>".to_owned()),
                span,
            });
        }
        let callee_key = if direct_name.is_none() {
            let mut lowered = self.lower_expr(callee, state)?;
            let expected = self.fnptr_type(function_ty, span)?;
            // The address of an old-style definition carries the
            // definition's interface; the call converts it like an
            // unprototyped function pointer.
            let old_style_address =
                undefined_old_style && matches!(lowered.value.ty, EvoIRType::FnPtr { .. });
            if lowered.value.ty != expected && !old_style_address {
                return Err(LowerError::UnsupportedExpression {
                    span: callee.span(),
                    feature: "indirect callee value disagrees with its fnptr signature",
                });
            }
            if matches!(expected, EvoIRType::Ptr(_)) {
                let Some(TypeKind::Function { return_type, .. }) =
                    self.unit.types().get(function_ty).map(|ty| &ty.kind)
                else {
                    unreachable!("call designator was checked above")
                };
                let actual = function_pointer_interface(
                    self.unit.types(),
                    self.target,
                    *return_type,
                    &parameter_types
                        .iter()
                        .map(|ty| ty.source)
                        .collect::<Vec<_>>(),
                    false,
                    span,
                )?;
                let address = self.cast(lowered, CIntegerType::ULONG, state)?;
                let pointer = self.fresh_value(actual);
                self.emit(
                    state,
                    Inst::PointerCast {
                        result: pointer.clone(),
                        operand: address.value,
                    },
                )?;
                lowered = ExprValue {
                    value: pointer,
                    integer: CIntegerType::ULONG,
                };
            }
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: lowered.value,
                    integer: None,
                },
            );
            Some(key)
        } else {
            None
        };
        let mut arg_marks = Vec::with_capacity(parameter_types.len());
        for (index, expected) in parameter_types.iter().enumerate() {
            let mark = if index >= fixed_parameter_count && self.is_aggregate_type(expected.source)
            {
                let layout = self
                    .target
                    .layout_of(self.unit.types(), expected.source)
                    .map_err(|source| LowerError::Layout {
                        ty: expected.source,
                        span,
                        source,
                    })?;
                ParamMark::ByVal(RecordShape {
                    size: layout.size_bytes,
                    align: layout.align_bytes,
                    vector: homogeneous_vector_aggregate(
                        self.unit.types(),
                        self.target,
                        expected.source,
                    ),
                    floating: evocc_chir::homogeneous_floating_aggregate(
                        self.unit.types(),
                        self.target,
                        expected.source,
                    ),
                })
            } else {
                ParamMark::None
            };
            arg_marks.push(mark);
        }
        let mut argument_keys = Vec::new();
        for (argument, expected) in arguments.iter().zip(&parameter_types) {
            let value = if self.is_aggregate_type(expected.source) {
                let source = self.lower_expr(argument, state)?;
                let origin = self.synthetic_origin()?;
                let destination =
                    self.emit_alloca(expected.source, argument.span(), origin, state)?;
                self.emit_aggregate_transfer(
                    destination.clone(),
                    source.value,
                    expected.source,
                    CopyVolatility::new(self.aggregate_source_is_volatile(argument), false),
                    argument.span(),
                    state,
                )?;
                ExprValue {
                    value: destination,
                    integer: CIntegerType::ULONG,
                }
            } else {
                let value = self.lower_expr(argument, state)?;
                if matches!(
                    expected.ir,
                    EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } | EvoIRType::Vec(..)
                ) || evocc_evoir::floating_kind(&expected.ir).is_some()
                {
                    self.coerce_to_object_type(
                        value,
                        Some(argument),
                        expected.source,
                        argument.span(),
                        state,
                    )?
                } else {
                    self.cast(value, expected.integer, state)?
                }
            };
            let key = EnvKey::Temporary(self.next_temporary);
            self.next_temporary += 1;
            state.env.insert(
                key,
                BoundValue {
                    value: value.value,
                    integer: (!self.is_aggregate_type(expected.source)).then_some(expected.integer),
                },
            );
            argument_keys.push(key);
        }
        let mut args = argument_keys
            .into_iter()
            .map(|key| {
                state.env.remove(&key).map(|bound| bound.value).ok_or(
                    LowerError::InvalidControlFlow {
                        span,
                        reason: "call argument control flow lost an evaluated argument",
                    },
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        if let Some(verdict) = &old_style {
            self.emit_old_style_obligations(verdict, &args, span, state)?;
        }
        let aggregate_return = if return_type
            .as_ref()
            .is_some_and(|value_type| self.is_aggregate_type(value_type.source))
        {
            let value_type = return_type
                .as_ref()
                .expect("aggregate return predicate requires a return type");
            let origin = self.synthetic_origin()?;
            let destination = self.emit_alloca(value_type.source, span, origin, state)?;
            // The return destination is a fixed parameter. Anonymous
            // arguments follow the entire fixed signature, including it.
            args.insert(fixed_parameter_count, destination.clone());
            arg_marks.insert(fixed_parameter_count, ParamMark::None);
            Some(destination)
        } else {
            None
        };
        let mem_in = self.token(state, EnvKey::Memory)?.value;
        let world_in = self.token(state, EnvKey::World)?.value;
        let mut results = Vec::new();
        if let Some(value_type) = return_type
            .as_ref()
            .filter(|value_type| !self.is_aggregate_type(value_type.source))
        {
            results.push(self.fresh_value(value_type.ir.clone()));
        }
        let mem_out = self.fresh_value(EvoIRType::Mem(Region::All));
        let world_out = self.fresh_value(EvoIRType::World);
        let arg_marks = canonical_marks(arg_marks, args.len());
        let call = if let Some(callee) = direct_name {
            Inst::Call {
                results: results.clone(),
                callee,
                args,
                arg_marks,
                returns_twice: direct.is_some_and(|function| function.returns_twice),
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }
        } else {
            let callee = state
                .env
                .remove(&callee_key.expect("indirect call inserted its callee temporary"))
                .ok_or(LowerError::InvalidControlFlow {
                    span,
                    reason: "indirect call control flow lost its callee",
                })?
                .value;
            Inst::IndirectCall {
                results: results.clone(),
                callee,
                args,
                arg_marks,
                mem_in,
                world_in,
                mem_out: mem_out.clone(),
                world_out: world_out.clone(),
            }
        };
        self.emit(state, call)?;
        state.env.insert(
            EnvKey::Memory,
            BoundValue {
                value: mem_out,
                integer: None,
            },
        );
        state.env.insert(
            EnvKey::World,
            BoundValue {
                value: world_out,
                integer: None,
            },
        );
        let Some(value_type) = return_type else {
            return Ok(None);
        };
        if self.is_aggregate_type(value_type.source) {
            return Ok(Some(ExprValue {
                value: aggregate_return
                    .expect("aggregate-returning call allocated its result object"),
                integer: CIntegerType::ULONG,
            }));
        }
        let value = results.remove(0);
        // A result that travels wider than its C type is truncated (10.1.3).
        if value_type.ir != bits(value_type.integer) && matches!(value_type.ir, EvoIRType::Bits(_))
        {
            let narrow = self.fresh_value(bits(value_type.integer));
            self.emit(
                state,
                Inst::Trunc(narrow.clone(), value, bits(value_type.integer)),
            )?;
            return Ok(Some(ExprValue {
                value: narrow,
                integer: value_type.integer,
            }));
        }
        Ok(Some(ExprValue {
            value,
            integer: value_type.integer,
        }))
    }
}
