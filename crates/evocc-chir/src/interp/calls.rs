//! C-HIR interpreter calls.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn call_designator_returns_void(&self, callee: &CHirExpr<'arena>) -> bool {
        let Some(function_type) = self.call_designator_function_type(callee) else {
            return false;
        };
        let Some(TypeKind::Function { return_type, .. }) =
            self.unit.types().get(function_type).map(|ty| &ty.kind)
        else {
            return false;
        };
        matches!(
            self.unit.types().get(*return_type).map(|ty| &ty.kind),
            Some(TypeKind::Void)
        )
    }

    pub(super) fn call_designator_function_type(
        &self,
        callee: &CHirExpr<'arena>,
    ) -> Option<TypeId> {
        let candidate =
            PointerTypeDesc::from_expression(self.unit.symbols(), self.unit.types(), callee)
                .ok()?
                .pointee();
        match self.unit.types().get(candidate).map(|ty| &ty.kind) {
            Some(TypeKind::Function { .. }) => Some(candidate),
            _ => None,
        }
    }

    pub(super) fn mutation_allowed(&self, _allow_root_mutation: bool) -> bool {
        // An executed operand may itself be an assignment or update. The
        // constant-initializer evaluator is the separate non-mutating path.
        !self.static_integer_evaluation
    }

    /// A call without a prototype of an old-style definition is undefined
    /// when the argument count differs, or a promoted argument type does not
    /// agree with the promoted parameter type, outside the two exceptions of
    /// C11 6.5.2.2p6 (EvoIR 10.1.7). The arguments have run.
    pub(super) fn check_old_style_call(
        &self,
        callee: &CHirExpr<'_>,
        arguments: &[&CHirExpr<'_>],
        values: &[CValue],
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        use crate::old_style_calls::{call_verdict, top_bit_set, Verdict};
        let undefined = match call_verdict(self.unit, &self.target, callee, arguments) {
            None | Some(Verdict::Agrees) => false,
            Some(Verdict::IfRepresentable(indices)) => indices.iter().any(|index| {
                matches!(values.get(*index), Some(CValue::Integer(value)) if top_bit_set(*value))
            }),
            Some(Verdict::Undefined) => true,
        };
        if undefined {
            Err(InterpreterError::OldStyleCallMismatch { span })
        } else {
            Ok(())
        }
    }

    pub(super) fn start_call(
        &mut self,
        callee: &'arena CHirExpr<'arena>,
        arguments: Vec<CValue>,
        call_span: SourceSpan,
        expects_value: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        value_base: usize,
    ) -> Result<Option<CValue>, InterpreterError> {
        self.tick(callee.span())?;
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span,
        } = callee
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "indirect scalar function call",
                span: callee.span(),
            });
        };
        let declaration = self.unit.symbols().symbol(*symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown function designator",
                span: *span,
            }
        })?;
        if declaration.kind() != SymbolKind::Function
            || declaration.spelling() != spelling
            || !crate::type_at_use_matches_binding(self.unit.types(), facts.ty(), declaration.ty())
            || facts.category() != ExprCategory::FunctionDesignator
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent function designator",
                span: *span,
            });
        }

        if self.unit.symbols().is_builtin_function(*symbol)
            && evocc_types::neon::signature(spelling).is_some()
        {
            return self.neon_call(spelling, facts.ty(), &arguments, call_span);
        }
        if spelling == "__sync_synchronize" && self.unit.symbols().is_builtin_function(*symbol) {
            if !arguments.is_empty() {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "full memory barrier arity",
                    span: call_span,
                });
            }
            // Match the lowered, ordered assembly boundary instead of
            // pretending that this interpreter models concurrent execution.
            return Err(InterpreterError::Asm { span: call_span });
        }
        if (spelling == "__atomic_thread_fence" || spelling == "__atomic_signal_fence")
            && self.unit.symbols().is_builtin_function(*symbol)
        {
            // The order operand has run. The front end removes a relaxed
            // fence; every other one is the same ordered assembly boundary
            // as `__sync_synchronize` in the lowered program.
            return Err(InterpreterError::Asm { span: call_span });
        }
        if spelling == "__builtin_prefetch" && self.unit.symbols().is_builtin_function(*symbol) {
            // Argument evaluation has already run through the normal call
            // continuation; the hint itself performs no abstract memory access.
            return Ok(None);
        }
        if spelling == "__builtin_extract_return_addr"
            && self.unit.symbols().is_builtin_function(*symbol)
        {
            let [value] = arguments.as_slice() else {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "return-address extraction arity",
                    span: call_span,
                });
            };
            return Ok(Some(*value));
        }
        let target = self
            .linkage
            .target(linked_library_definition(self.unit, *symbol));
        if !self.unit.iter().any(|declaration| matches!(declaration, CHirDecl::Function(function) if function.symbol == target && function.body.is_some())) {
            let name = self.linkage.function_name(*symbol, spelling);
            let result = self.external_library_call(target, name, &arguments, call_span)?;
            return Ok(Some(result));
        }

        let caller_slots = std::mem::take(&mut self.slots);
        let caller_fuel_anchor = self.fuel_anchor.take();
        self.enter_function(
            target,
            arguments,
            Some(caller_slots),
            caller_fuel_anchor,
            Some(call_span),
            expects_value,
            value_base,
            tasks,
        )?;
        Ok(None)
    }

    pub(super) fn external_library_call(
        &mut self,
        symbol: SymbolId,
        spelling: &str,
        arguments: &[CValue],
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if self.program && matches!(spelling, "exit" | "_Exit" | "_exit" | "quick_exit") {
            let [CValue::Integer(value)] = arguments else {
                return Err(InterpreterError::UnsupportedFunctionSignature { symbol, span });
            };
            self.termination = Some((value.raw_bits() & u128::from(u32::MAX), spelling == "exit"));
            return Ok(CValue::Integer(*value));
        }
        let entry = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| InterpreterError::UnknownFunction { symbol })?;
        let Some(TypeKind::Function {
            return_type,
            parameters,
        }) = self.unit.types().get(entry.ty()).map(|ty| &ty.kind)
        else {
            return Err(InterpreterError::UnsupportedFunctionSignature { symbol, span });
        };
        let return_type = self.value_type(*return_type, span)?;
        let parameter_types = match parameters {
            FunctionParameters::Prototype { types, .. } => types.as_slice(),
            FunctionParameters::Unspecified => self
                .unit
                .unprototyped_parameters(symbol)
                .unwrap_or_default(),
        };
        let arguments = arguments
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let value = match parameter_types.get(index) {
                    Some(ty) => {
                        self.cast_runtime_value(*value, self.value_type(*ty, span)?, span)?
                    }
                    None => *value,
                };
                match value {
                    CValue::Integer(value) => u64::try_from(value.raw_bits())
                        .map(LibraryValue::Integer)
                        .map_err(|_| InterpreterError::WideInteger { span }),
                    CValue::Pointer { value, .. } => Ok(LibraryValue::Pointer(value)),
                    _ => Err(self.memory_error(
                        MemoryFault::ExternalFunction {
                            name: spelling.into(),
                        },
                        span,
                    )),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        let (memory, world, result) = self
            .memory
            .call_library_with_world(
                &self.world,
                evocc_memory::FenvRuntime::Aarch64Glibc,
                spelling,
                &arguments,
            )
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.world = world;
        match (result, return_type) {
            (LibraryValue::Integer(value), ValueType::Integer(ty)) => {
                Ok(CValue::Integer(CInteger::from_raw(u128::from(value), ty)))
            }
            (LibraryValue::Pointer(value), ValueType::Pointer(ty)) => {
                Ok(CValue::Pointer { value, ty })
            }
            _ => Err(InterpreterError::UnsupportedFunctionSignature { symbol, span }),
        }
    }

    pub(super) fn start_indirect_call(
        &mut self,
        callee: CValue,
        arguments: Vec<CValue>,
        call_span: SourceSpan,
        expects_value: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
        value_base: usize,
    ) -> Result<Option<CValue>, InterpreterError> {
        let CValue::FunctionPointer { value, ty } = callee else {
            return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
        };
        if !matches!(
            self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
            Some(TypeKind::Function { .. })
        ) {
            return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
        }
        let value = if let PointerValue::Exposed(address) = value {
            (1..=self.symbol_order().functions.len() as u64)
                .map(PointerValue::Function)
                .find(|candidate| self.memory.pointer_address(*candidate) == Ok(address))
                .unwrap_or(value)
        } else {
            value
        };
        let value = self
            .memory
            .resolve_function_pointer(value)
            .map_err(|_| InterpreterError::CallThroughNonFunctionPointer { span: call_span })?;
        let identity = match value {
            PointerValue::Null => {
                return Err(InterpreterError::CallThroughNullFunctionPointer { span: call_span });
            }
            PointerValue::Object(_) | PointerValue::Exposed(_) => {
                return Err(InterpreterError::CallThroughNonFunctionPointer { span: call_span });
            }
            PointerValue::Function(identity) => identity,
        };
        let symbol = self
            .function_symbol(identity)
            .ok_or(InterpreterError::CallThroughNonFunctionPointer { span: call_span })?;
        let entry = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| InterpreterError::UnknownFunction { symbol })?;
        if !self.function_types_compatible(entry.ty(), ty.pointee()) {
            return Err(InterpreterError::IndirectCallDynamicSignatureMismatch { span: call_span });
        }
        if matches!(
            self.unit.types().get(ty.pointee()).map(|ty| &ty.kind),
            Some(TypeKind::Function {
                parameters: FunctionParameters::Unspecified,
                ..
            })
        ) {
            let parameters = match self.unit.types().get(entry.ty()).map(|ty| &ty.kind) {
                Some(TypeKind::Function {
                    parameters: FunctionParameters::Prototype { types, .. },
                    ..
                }) => types.as_slice(),
                _ => &[],
            };
            if arguments.len() != parameters.len() {
                return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                    span: call_span,
                });
            }
            for (argument, parameter) in arguments.iter().zip(parameters) {
                let expected = self.value_type(*parameter, call_span)?;
                let compatible = match (argument, expected) {
                    (CValue::Integer(value), ValueType::Integer(integer)) => {
                        value.ty().bits() == integer.bits()
                    }
                    (
                        CValue::Pointer { ty, .. } | CValue::FunctionPointer { ty, .. },
                        ValueType::Pointer(expected),
                    ) => self
                        .unit
                        .types()
                        .compatible_ignoring_top_qualifiers(ty.pointee(), expected.pointee()),
                    (CValue::Aggregate { ty, .. }, ValueType::Aggregate(expected)) => self
                        .unit
                        .types()
                        .compatible_ignoring_top_qualifiers(*ty, expected),
                    (CValue::Uninitialized(_), _) => true,
                    _ => false,
                };
                if !compatible {
                    return Err(InterpreterError::IndirectCallDynamicSignatureMismatch {
                        span: call_span,
                    });
                }
            }
        }
        if !self.unit.iter().any(|declaration| matches!(declaration, CHirDecl::Function(function) if function.symbol == symbol && function.body.is_some())) {
            let name = self.linkage.function_name(symbol, entry.spelling());
            return self.external_library_call(symbol, name, &arguments, call_span).map(Some);
        }
        let caller_slots = std::mem::take(&mut self.slots);
        let caller_fuel_anchor = self.fuel_anchor.take();
        self.enter_function(
            symbol,
            arguments,
            Some(caller_slots),
            caller_fuel_anchor,
            Some(call_span),
            expects_value,
            value_base,
            tasks,
        )?;
        Ok(None)
    }

    pub(super) fn function_identity(&self, symbol: SymbolId) -> Option<u64> {
        let symbol = self.linkage.target(symbol);
        let name = self.unit.symbols().symbol(symbol).ok()?.spelling();
        self.symbol_order()
            .functions
            .iter()
            .position(|candidate| {
                self.unit
                    .symbols()
                    .symbol(*candidate)
                    .is_ok_and(|entry| entry.spelling() == name)
            })
            .and_then(|index| u64::try_from(index).ok())
            .and_then(|index| index.checked_add(1))
    }

    pub(super) fn label_address_value(
        &mut self,
        expression: &CHirExpr<'arena>,
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span();
        let identity = self
            .symbol_order()
            .labels
            .get(&(expression as *const CHirExpr<'_> as usize))
            .map(|(_, _, id)| *id)
            .ok_or(InterpreterError::InvalidIndirectBranchTarget { span })?;
        let (memory, address) = self
            .memory
            .expose_pointer(PointerValue::Function(identity))
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        Ok(CValue::Pointer {
            value: Pointer::from_address(address),
            ty: self.pointer_type_of_expression(expression)?,
        })
    }
}
