//! C-HIR interpreter functions.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn enter_function(
        &mut self,
        function: SymbolId,
        arguments: Vec<CValue>,
        caller_slots: Option<HashMap<SymbolId, Slot>>,
        caller_fuel_anchor: Option<SourceSpan>,
        call_span: Option<SourceSpan>,
        caller_expects_value: bool,
        value_base: usize,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        if let Some(refusal) = self
            .unit
            .profile_refusals()
            .iter()
            .find(|r| r.function == function)
        {
            return Err(InterpreterError::Profile(refusal.clone()));
        }
        let signature = self.validate_function_signature(declaration, arguments.len())?;
        let return_type = signature.return_type;
        let return_value_type = signature.return_value_type;
        if call_span.is_some() && caller_expects_value && return_value_type == ValueType::Void {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "void call in scalar expression",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        if call_span.is_some() && !caller_expects_value && return_value_type != ValueType::Void {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "non-void call scheduled as a void expression statement",
                span: call_span.unwrap_or(declaration.span),
            });
        }
        let binding_span = call_span.unwrap_or(declaration.span);
        let mut arguments = arguments;
        let anonymous = arguments.split_off(signature.parameters.len());
        if anonymous.iter().any(|value| match value {
            CValue::Floating(value) => {
                value.kind().is_half() && !self.target.passes_half_anonymous_arguments()
            }
            CValue::Aggregate { ty, .. } => {
                crate::half_interface_refused(self.unit.types(), &self.target, *ty, true)
            }
            _ => false,
        }) {
            return Err(InterpreterError::FloatingCallInterface { span: binding_span });
        }
        if anonymous
            .iter()
            .any(|value| matches!(value, CValue::Aggregate { ty, .. } if !crate::is_integer_variadic_record(self.unit.types(), &self.target, *ty)))
        {
            return Err(InterpreterError::VarArgs { span: binding_span });
        }
        let mut parameter_bindings = Vec::with_capacity(arguments.len());
        for ((symbol, ty), argument) in signature.parameters.into_iter().zip(arguments) {
            let argument = self.cast_runtime_value(argument, ty, binding_span)?;
            let Some(symbol) = symbol else {
                // Argument evaluation and conversion still happen; there is no
                // source identifier to bind when the parameter name is omitted.
                continue;
            };
            match (ty, argument) {
                (ValueType::Floating(kind), CValue::Floating(value)) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Floating(kind),
                        CValue::Floating(value),
                    ));
                }
                (ValueType::Integer(ty), CValue::Integer(value)) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Integer(ty),
                        CValue::Integer(value),
                    ));
                }
                (ValueType::Pointer(pointer_type), CValue::Pointer { value, ty })
                    if pointer_type == ty =>
                {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Pointer(pointer_type),
                        CValue::Pointer { value, ty },
                    ));
                }
                (ValueType::Pointer(pointer_type), CValue::FunctionPointer { value, ty })
                    if pointer_type == ty =>
                {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Pointer(pointer_type),
                        CValue::FunctionPointer {
                            value,
                            ty: pointer_type,
                        },
                    ));
                }
                (
                    ValueType::Aggregate(parameter_type),
                    CValue::Aggregate {
                        address,
                        ty,
                        qualifiers,
                    },
                ) if self.same_aggregate_type(parameter_type, ty) => {
                    parameter_bindings.push((
                        symbol,
                        ValueType::Aggregate(parameter_type),
                        CValue::Aggregate {
                            address,
                            ty,
                            qualifiers,
                        },
                    ));
                }
                (ValueType::Void, _) | (_, _) => {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                }
            }
        }

        let body = declaration
            .body
            .ok_or(InterpreterError::FunctionHasNoBody {
                symbol: function,
                span: declaration.span,
            })?;
        let body_span = body.span();
        let is_main = declaration.name.spelling == "main";

        self.slots = HashMap::new();
        let caller_memory_frame = if self.memory_mode {
            let caller = self.memory_frame.take();
            let (memory, frame, root_scope) = self
                .memory
                .begin_frame()
                .map_err(|error| self.memory_error(error, binding_span))?;
            self.memory = memory;
            // Anonymous records are values just like named record parameters:
            // preserve a copy before executing the callee, including va_copy.
            let mut anonymous_values = Vec::with_capacity(anonymous.len());
            for value in anonymous {
                let value = if let CValue::Aggregate { ty, .. } = value {
                    let layout = self.access_layout(ty, binding_span)?;
                    let (memory, destination) = self
                        .memory
                        .allocate_automatic(frame, root_scope, self.temporary_origin_site, layout)
                        .map_err(|error| self.memory_error(error, binding_span))?;
                    self.memory = memory;
                    self.store_memory_value(
                        destination,
                        ty,
                        Qualifiers::default(),
                        value,
                        binding_span,
                    )?
                } else {
                    value
                };
                anonymous_values.push(value);
            }
            self.memory_frame = Some(MemoryFrame {
                cleanups: Vec::new(),
                anonymous: anonymous_values,
                id: frame,
                root_scope,
                current_scope: root_scope,
                bindings: HashMap::new(),
                compound_bindings: HashMap::new(),
            });
            for (symbol, _, value) in parameter_bindings {
                let ty = self
                    .unit
                    .symbols()
                    .symbol(symbol)
                    .map_err(|_| InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    })?
                    .ty();
                self.allocate_automatic_object(symbol, ty, None, binding_span)?;
                self.store_bound_value(symbol, value, binding_span)?;
            }
            caller
        } else {
            for (symbol, ty, value) in parameter_bindings {
                let (ValueType::Integer(ty), CValue::Integer(value)) = (ty, value) else {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                };
                if self
                    .slots
                    .insert(
                        symbol,
                        Slot {
                            ty,
                            value: Some(value),
                            active: true,
                        },
                    )
                    .is_some()
                {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: function,
                        span: declaration.span,
                    });
                }
            }
            self.predeclare_stmt(body)?;
            None
        };
        validate_function_control_flow(body)?;

        tasks.push(MachineTask::FunctionEnd(FunctionEnd {
            symbol: function,
            return_type,
            return_value_type,
            is_main,
            body_span,
            caller_slots,
            caller_memory_frame,
            caller_fuel_anchor,
            caller_expects_value,
            value_base,
        }));
        tasks.push(MachineTask::ExecStatement(body));
        Ok(())
    }

    pub(super) fn validate_root_function(
        &self,
        function: SymbolId,
    ) -> Result<(), InterpreterError> {
        let declaration = self.function_definition(function)?;
        let signature = self.validate_function_signature(declaration, 0)?;
        if matches!(signature.return_value_type, ValueType::Pointer(_)) {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        Ok(())
    }

    pub(super) fn validate_function_signature(
        &self,
        declaration: &FunctionDecl<'arena>,
        argument_count: usize,
    ) -> Result<ValidatedFunctionSignature, InterpreterError> {
        let function = declaration.symbol;
        let symbol = self.unit.symbols().symbol(function).map_err(|_| {
            InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            }
        })?;
        let canonical = self.unit.types().get(declaration.ty).ok_or(
            InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            },
        )?;
        let TypeKind::Function {
            return_type,
            parameters: canonical_parameters,
        } = &canonical.kind
        else {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        };
        if symbol.kind() != SymbolKind::Function
            || symbol.spelling() != declaration.name.spelling
            || !self.function_types_compatible(symbol.ty(), declaration.ty)
            || !symbol
                .storage_class()
                .is_some_and(|storage| function_storage_compatible(storage, declaration.storage))
            || !self.written_declarator_matches(
                &declaration.written,
                declaration.ty,
                declaration.return_specifier.type_id(),
                Some(&declaration.parameters),
                false,
                &declaration.attributes,
            )
        {
            return Err(InterpreterError::UnsupportedFunctionSignature {
                symbol: function,
                span: declaration.span,
            });
        }
        // A `__bf16` HFA has no admitted convention (EvoIR 3.13.6).
        let interface = std::iter::once(return_type).chain(match canonical_parameters {
            FunctionParameters::Prototype { types, .. } => types.iter(),
            FunctionParameters::Unspecified => [].iter(),
        });
        for ty in interface {
            if crate::half_interface_refused(self.unit.types(), &self.target, *ty, false) {
                return Err(InterpreterError::FloatingCallInterface {
                    span: declaration.span,
                });
            }
        }
        let return_value_type = self.value_type(*return_type, declaration.span)?;
        let parameters = match (canonical_parameters, &declaration.parameters) {
            (FunctionParameters::Unspecified, ParameterSyntax::Unspecified)
                if argument_count == 0 =>
            {
                Vec::new()
            }
            (FunctionParameters::Prototype { types, variadic }, syntax)
                if !matches!(syntax, ParameterSyntax::Unspecified)
                    && *variadic
                        == matches!(syntax, ParameterSyntax::Prototype { variadic: true, .. })
                    && types.len() == syntax.parameters().len()
                    && (types.len() == argument_count
                        || (*variadic && types.len() <= argument_count)) =>
            {
                let parameters = syntax.parameters();
                let mut validated = Vec::with_capacity(parameters.len());
                let mut parameter_ids = BTreeSet::new();
                let mut parameter_spellings = BTreeSet::new();
                let mut function_scope = None;
                for (canonical_type, parameter) in types.iter().zip(parameters) {
                    if !syntax.parameter_type_matches(
                        self.unit.types(),
                        *canonical_type,
                        parameter.ty,
                    ) || !self.written_declarator_matches(
                        &parameter.written,
                        parameter.ty,
                        parameter.specifier.type_id(),
                        None,
                        true,
                        &parameter.attributes,
                    ) || !matches!(
                        parameter.storage,
                        StorageClass::None | StorageClass::Register
                    ) {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    }
                    if parameter.symbol.is_none() && parameter.name.is_none() {
                        validated.push((None, self.value_type(parameter.ty, parameter.span)?));
                        continue;
                    }
                    let (Some(parameter_symbol), Some(parameter_name)) =
                        (parameter.symbol, &parameter.name)
                    else {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    };
                    let symbol = self.unit.symbols().symbol(parameter_symbol).map_err(|_| {
                        InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        }
                    })?;
                    let scope = self.unit.symbols().scope(symbol.scope()).map_err(|_| {
                        InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        }
                    })?;
                    let scope_is_valid = scope.kind() == ScopeKind::Function
                        && scope.parent() == Some(self.unit.symbols().file_scope())
                        && function_scope.is_none_or(|expected| expected == symbol.scope());
                    if symbol.kind() != SymbolKind::Parameter
                        || symbol.spelling() != parameter_name.spelling
                        || symbol.ty() != parameter.ty
                        || !matches!(
                            symbol.storage_class(),
                            Some(StorageClass::None | StorageClass::Register)
                        )
                        || symbol.storage_class() != Some(parameter.storage)
                        || !scope_is_valid
                        || !parameter_ids.insert(parameter_symbol)
                        || !parameter_spellings.insert(parameter_name.spelling.clone())
                    {
                        return Err(InterpreterError::UnsupportedFunctionSignature {
                            symbol: function,
                            span: parameter.span,
                        });
                    }
                    function_scope = Some(symbol.scope());
                    validated.push((
                        Some(parameter_symbol),
                        self.value_type(parameter.ty, parameter.span)?,
                    ));
                }
                validated
            }
            _ => {
                return Err(InterpreterError::UnsupportedFunctionSignature {
                    symbol: function,
                    span: declaration.span,
                });
            }
        };
        Ok(ValidatedFunctionSignature {
            return_type: *return_type,
            return_value_type,
            parameters,
        })
    }

    pub(super) fn function_types_compatible(&self, left: TypeId, right: TypeId) -> bool {
        if left == right {
            return true;
        }
        let (Some(left), Some(right)) = (self.unit.types().get(left), self.unit.types().get(right))
        else {
            return false;
        };
        let (
            TypeKind::Function {
                return_type: left_return,
                parameters: left_parameters,
            },
            TypeKind::Function {
                return_type: right_return,
                parameters: right_parameters,
            },
        ) = (&left.kind, &right.kind)
        else {
            return false;
        };
        self.unit.types().compatible(*left_return, *right_return)
            && function_parameters_compatible(self.unit.types(), left_parameters, right_parameters)
    }

    pub(super) fn written_declarator_matches(
        &self,
        written: &WrittenDeclarator,
        mut current: TypeId,
        terminal: TypeId,
        function_syntax: Option<&ParameterSyntax>,
        adjusted_parameter: bool,
        attributes: &[crate::Attribute],
    ) -> bool {
        if adjusted_parameter
            && (matches!(
                written.layers.first(),
                Some(WrittenDeclaratorLayer::Function { .. })
            ) || (written.layers.is_empty()
                && self
                    .unit
                    .types()
                    .get(terminal)
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))))
        {
            if let Some(canonical) = self.unit.types().get(current) {
                if let TypeKind::Pointer(pointee) = canonical.kind {
                    if canonical.qualifiers == Qualifiers::default()
                        && self
                            .unit
                            .types()
                            .get(pointee)
                            .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                    {
                        current = pointee;
                    }
                }
            }
        }
        if function_syntax.is_some()
            && !matches!(
                written.layers.first(),
                Some(WrittenDeclaratorLayer::Function { .. })
            )
        {
            return false;
        }
        for (index, layer) in written.layers.iter().enumerate() {
            let Some(canonical) = self.unit.types().get(current) else {
                return false;
            };
            current = match (layer, &canonical.kind) {
                (
                    WrittenDeclaratorLayer::Function { .. },
                    TypeKind::Function {
                        return_type,
                        parameters,
                    },
                ) => {
                    if index == 0
                        && function_syntax.is_some_and(|syntax| {
                            !function_parameters_match(self.unit.types(), parameters, syntax)
                        })
                    {
                        return false;
                    }
                    *return_type
                }
                (
                    WrittenDeclaratorLayer::Pointer { qualifiers, .. },
                    TypeKind::Pointer(pointee),
                ) if canonical.qualifiers == *qualifiers => *pointee,
                (
                    WrittenDeclaratorLayer::Array {
                        parameter_qualifiers,
                        ..
                    },
                    TypeKind::Pointer(pointee),
                ) if adjusted_parameter
                    && index == 0
                    && canonical.qualifiers == *parameter_qualifiers =>
                {
                    *pointee
                }
                (
                    WrittenDeclaratorLayer::Array { bound, .. },
                    TypeKind::Array {
                        element,
                        bound: canonical_bound,
                    },
                ) if bound == canonical_bound => *element,
                _ => return false,
            };
        }
        current == terminal
            || crate::vector::attribute_matches_base(
                self.unit.types(),
                &self.target,
                attributes,
                current,
                terminal,
            )
            || (adjusted_parameter
                && written.layers.is_empty()
                && matches!(
                    (self.unit.types().get(current).map(|ty| &ty.kind), self.unit.types().get(terminal).map(|ty| &ty.kind)),
                    (Some(TypeKind::Pointer(pointee)), Some(TypeKind::Array { element, .. })) if pointee == element
                ))
    }

    pub(super) fn function_definition(
        &self,
        function: SymbolId,
    ) -> Result<&'arena FunctionDecl<'arena>, InterpreterError> {
        let function = self
            .linkage
            .target(linked_library_definition(self.unit, function));
        let symbol = self
            .unit
            .symbols()
            .symbol(function)
            .map_err(|_| InterpreterError::UnknownFunction { symbol: function })?;
        if symbol.kind() != SymbolKind::Function {
            return Err(InterpreterError::UnknownFunction { symbol: function });
        }

        let mut definition = None;
        for declaration in self.unit.iter() {
            let CHirDecl::Function(declaration) = declaration else {
                continue;
            };
            if declaration.symbol != function {
                continue;
            }
            if declaration.name.spelling != symbol.spelling()
                || !self.function_types_compatible(declaration.ty, symbol.ty())
                || !symbol.storage_class().is_some_and(|storage| {
                    function_storage_compatible(storage, declaration.storage)
                })
            {
                return Err(InterpreterError::MalformedFunctionDeclaration {
                    symbol: function,
                    span: declaration.span,
                });
            }
            if declaration.body.is_some() && definition.replace(declaration).is_some() {
                return Err(InterpreterError::DuplicateFunctionBody {
                    symbol: function,
                    span: declaration.span,
                });
            }
        }
        definition.ok_or(InterpreterError::FunctionHasNoBody {
            symbol: function,
            span: symbol.span(),
        })
    }

    pub(super) fn finish_function(
        &mut self,
        end: FunctionEnd<'arena>,
        exit: FunctionExit,
        values: &mut Vec<CValue>,
    ) -> Result<Option<MachineOutcome>, InterpreterError> {
        let mut return_value = match (exit, end.return_value_type) {
            (FunctionExit::Explicit(Some(CValue::Uninitialized(source))), target)
                if source == target =>
            {
                Some(CValue::Uninitialized(target))
            }
            (FunctionExit::Explicit(Some(value)), ValueType::Floating(kind)) => {
                Some(self.cast_runtime_value(value, ValueType::Floating(kind), end.body_span)?)
            }
            (FunctionExit::Explicit(Some(value)), ValueType::Integer(return_integer)) => Some(
                self.cast_runtime_value(value, ValueType::Integer(return_integer), end.body_span)?,
            ),
            (FunctionExit::Explicit(Some(value)), ValueType::Pointer(return_type)) => Some(
                self.cast_runtime_value(value, ValueType::Pointer(return_type), end.body_span)?,
            ),
            (FunctionExit::Explicit(Some(value)), ValueType::Aggregate(return_type)) => Some(
                self.cast_runtime_value(value, ValueType::Aggregate(return_type), end.body_span)?,
            ),
            (FunctionExit::Explicit(None), ValueType::Void)
            | (FunctionExit::FellOffEnd, ValueType::Void) => None,
            (FunctionExit::FellOffEnd, ValueType::Integer(return_integer)) if end.is_main => {
                Some(CValue::Integer(CInteger::int(0).cast(return_integer)))
            }
            (
                FunctionExit::Explicit(None),
                ValueType::Integer(_)
                | ValueType::Floating(_)
                | ValueType::Pointer(_)
                | ValueType::Aggregate(_),
            ) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "empty return in scalar function",
                    span: end.body_span,
                });
            }
            (FunctionExit::Explicit(Some(_)), ValueType::Void) => {
                return Err(InterpreterError::UnsupportedStatement {
                    feature: "value return in void function",
                    span: end.body_span,
                });
            }
            (
                FunctionExit::FellOffEnd,
                ValueType::Integer(_)
                | ValueType::Floating(_)
                | ValueType::Pointer(_)
                | ValueType::Aggregate(_),
            ) => Some(CValue::Uninitialized(end.return_value_type)),
        };

        if self.memory_mode {
            if let Some(CValue::Aggregate {
                address: source,
                ty,
                qualifiers,
            }) = return_value
            {
                let caller = end.caller_memory_frame.as_ref().ok_or(
                    InterpreterError::UnsupportedFunctionSignature {
                        symbol: end.symbol,
                        span: end.body_span,
                    },
                )?;
                let layout = self.access_layout(ty, end.body_span)?;
                let (memory, destination) = self
                    .memory
                    .allocate_automatic(
                        caller.id,
                        caller.current_scope,
                        self.temporary_origin_site,
                        layout,
                    )
                    .map_err(|error| self.memory_error(error, end.body_span))?;
                self.memory = memory;
                return_value = Some(self.store_memory_value(
                    destination,
                    ty,
                    Qualifiers::default(),
                    CValue::Aggregate {
                        address: source,
                        ty,
                        qualifiers,
                    },
                    end.body_span,
                )?);
            }
            self.run_scope_cleanups(None)?;
            let frame = self
                .memory_frame
                .take()
                .ok_or(InterpreterError::UnsupportedStatement {
                    feature: "missing interpreter memory frame at function exit",
                    span: end.body_span,
                })?;
            self.memory = self
                .memory
                .end_frame(frame.id)
                .map_err(|error| self.memory_error(error, end.body_span))?;
            self.memory_frame = end.caller_memory_frame;
        }

        values.truncate(end.value_base);
        if let Some(caller_slots) = end.caller_slots {
            self.slots = caller_slots;
            self.fuel_anchor = end.caller_fuel_anchor;
            match (end.caller_expects_value, return_value) {
                (true, Some(value)) => values.push(value),
                (false, None) => {}
                _ => {
                    return Err(InterpreterError::UnsupportedFunctionSignature {
                        symbol: end.symbol,
                        span: end.body_span,
                    });
                }
            }
            Ok(None)
        } else {
            let return_value = match return_value {
                Some(CValue::Uninitialized(_)) => {
                    return Err(InterpreterError::UninitializedValue {
                        span: end.body_span,
                    });
                }
                Some(CValue::Integer(value)) => Some(ScalarValue {
                    ty: end.return_type,
                    raw_bits: value.raw_bits(),
                }),
                Some(CValue::Floating(value)) => Some(ScalarValue {
                    ty: end.return_type,
                    raw_bits: value.bits(),
                }),
                Some(CValue::Pointer { .. } | CValue::FunctionPointer { .. }) => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "pointer return from scalar interpreter root",
                        span: end.body_span,
                    });
                }
                Some(CValue::Aggregate { .. }) => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "aggregate return from scalar interpreter root",
                        span: end.body_span,
                    });
                }
                None => None,
            };
            Ok(Some(MachineOutcome { return_value }))
        }
    }
}
