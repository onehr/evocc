//! C-HIR interpreter static storage.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    /// Bound only the initializer paths that still use recursive object walks.
    /// Pure integer evaluation uses the fuel-bounded task machine instead.
    pub(super) fn guard_static_initializer_evaluation(
        &mut self,
        value: &'arena CHirExpr<'arena>,
    ) -> Result<(), InterpreterError> {
        let mut pending: Vec<(&'arena CHirExpr<'arena>, usize)> = vec![(value, 1)];
        while let Some((expression, depth)) = pending.pop() {
            if depth > MAX_STATIC_INITIALIZER_DEPTH {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "deeply-nested-static-initializer",
                    span: expression.span(),
                });
            }
            match expression {
                CHirExpr::Ternary(condition, left, right, _)
                    if self.infer_integer_type(expression).is_err() =>
                {
                    pending.extend([condition, left, right].map(|child| (*child, depth + 1)));
                }
                CHirExpr::Binary(BinaryOp::Eq | BinaryOp::Ne, left, right, _)
                    if self.infer_integer_type(left).is_err()
                        || self.infer_integer_type(right).is_err() =>
                {
                    pending.extend([left, right].map(|child| (*child, depth + 1)));
                }
                _ => push_static_initializer_children(expression, depth, &mut pending),
            }
        }
        Ok(())
    }

    pub(super) fn initialize_static_objects(
        &mut self,
        required: &BTreeSet<SymbolId>,
    ) -> Result<(), InterpreterError> {
        let mut plans = Vec::new();
        let mut compound_plans = Vec::new();
        let manifest = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?;
        for site in manifest.sites() {
            if let AllocationSiteKind::StringLiteral(expression) = site.kind() {
                let CHirExpr::StringLit(literal, span) = expression else {
                    unreachable!("manifest string site")
                };
                let (bytes, align) = literal.object_bytes(&self.target).ok_or(
                    InterpreterError::UnsupportedExpression {
                        feature: "invalid string representation",
                        span: *span,
                    },
                )?;
                let layout = AccessLayout::new(bytes.len() as u64, align);
                let bytes = ByteSequence::try_from_bytes(bytes)
                    .map_err(|error| self.memory_error(error, *span))?;
                let (memory, pointer) = self
                    .memory
                    .allocate_string_literal(site.ordinal().raw(), layout, &bytes)
                    .map_err(|error| self.memory_error(error, *span))?;
                self.memory = memory;
                self.string_bindings
                    .insert(expression as *const CHirExpr<'arena> as usize, pointer);
                continue;
            }
            if let AllocationSiteKind::CompoundLiteral {
                expression,
                static_storage: true,
            } = site.kind()
            {
                compound_plans.push((expression, site.ordinal().raw()));
                continue;
            }
            let AllocationSiteKind::Object(symbol) = site.kind() else {
                continue;
            };
            if !required.contains(&symbol) {
                continue;
            }
            if !has_static_storage_duration(self.unit, symbol) {
                continue;
            }
            let object = find_storage_object_declaration(
                self.unit,
                symbol,
                manifest.function_static_objects(),
            )
            .ok_or(InterpreterError::MalformedObjectDeclaration {
                symbol,
                span: site.anchor(),
            })?;
            plans.push((object, site.ordinal().raw()));
        }

        for (expression, site) in &compound_plans {
            let CHirExpr::CompoundLit(ty, _, span) = expression else {
                unreachable!("manifest compound site has compound expression authority")
            };
            let layout = self.access_layout(*ty, *span)?;
            let (memory, address) = self
                .memory
                .allocate_static(*site, layout, InitialState::Zeroed)
                .map_err(|error| self.memory_error(error, *span))?;
            self.memory = memory;
            self.static_compound_bindings.insert(
                *expression as *const CHirExpr<'arena> as usize,
                MemoryBinding { address, ty: *ty },
            );
        }

        for (declaration, site) in &plans {
            let mut layout = self.object_access_layout(
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
            layout.align_bytes = layout.align_bytes.max(
                self.unit
                    .symbols()
                    .symbol(declaration.symbol)
                    .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                        symbol: declaration.symbol,
                        span: declaration.span,
                    })?
                    .declared_alignment(),
            );
            let allocation = if declaration.storage.is_thread_local() {
                self.memory
                    .allocate_thread_local(*site, layout, InitialState::Zeroed)
                    .map(|(memory, address, _)| (memory, address))
            } else {
                self.memory
                    .allocate_static(*site, layout, InitialState::Zeroed)
            };
            let (memory, address) =
                allocation.map_err(|error| self.memory_error(error, declaration.span))?;
            self.memory = memory;
            if self
                .static_bindings
                .insert(
                    declaration.symbol,
                    MemoryBinding {
                        address,
                        ty: declaration.ty,
                    },
                )
                .is_some()
            {
                return Err(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                });
            }
        }

        for symbol in required {
            if self.static_bindings.contains_key(symbol) {
                continue;
            }
            let entry = self.unit.symbols().symbol(*symbol).map_err(|_| {
                InterpreterError::MalformedObjectDeclaration {
                    symbol: *symbol,
                    span: self
                        .fuel_anchor
                        .unwrap_or_else(|| self.unit.iter().next().expect("nonempty unit").span()),
                }
            })?;
            if entry.storage_class().map(StorageClass::base) != Some(StorageClass::Extern) {
                continue;
            }
            let (size, align) = self
                .target
                .external_object_layout(self.unit.types(), entry.ty())
                .map_err(|_| InterpreterError::UnsupportedType {
                    ty: entry.ty(),
                    span: entry.span(),
                })?;
            let (memory, address) = self
                .memory
                .register_external(
                    self.symbol_order().objects[entry.spelling()],
                    size,
                    align.max(entry.declared_alignment()),
                )
                .map_err(|error| self.memory_error(error, entry.span()))?;
            self.memory = memory;
            self.static_bindings.insert(
                *symbol,
                MemoryBinding {
                    address,
                    ty: entry.ty(),
                },
            );
        }

        for (expression, _) in compound_plans {
            let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
                unreachable!("manifest compound site has compound expression authority")
            };
            if initializer.plan().target_type() != *ty
                || initializer.plan().validate(self.unit.types()).is_err()
            {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "invalid static compound-literal initializer plan",
                    span: *span,
                });
            }
            let binding = self
                .static_compound_bindings
                .get(&(expression as *const CHirExpr<'arena> as usize))
                .copied()
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "static compound literal has no binding",
                    span: *span,
                })?;
            self.initialize_static_plan(binding, initializer, *span)?;
            self.memory = self
                .memory
                .finish_static_initializer(binding.address)
                .map_err(|error| self.memory_error(error, *span))?;
            if self.unit.types().is_const_qualified_object(*ty) {
                self.memory = self
                    .memory
                    .freeze_static(binding.address)
                    .map_err(|error| self.memory_error(error, *span))?;
            }
        }

        for (declaration, _) in plans {
            self.validate_initializer_plan(&declaration)?;
            let binding = self
                .static_bindings
                .get(&declaration.symbol)
                .copied()
                .ok_or(InterpreterError::MalformedObjectDeclaration {
                    symbol: declaration.symbol,
                    span: declaration.span,
                })?;
            if let Some(initializer) = &declaration.initializer {
                self.initialize_static_plan(binding, initializer, declaration.span)?;
            }
            self.memory = self
                .memory
                .finish_static_initializer(binding.address)
                .map_err(|error| self.memory_error(error, declaration.span))?;
            if self.unit.types().is_const_qualified_object(binding.ty) {
                self.memory = self
                    .memory
                    .freeze_static(binding.address)
                    .map_err(|error| self.memory_error(error, declaration.span))?;
            }
        }
        Ok(())
    }

    pub(super) fn initialize_static_plan(
        &mut self,
        binding: MemoryBinding,
        initializer: &crate::ObjectInitializer<'arena>,
        object_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        for action in initializer.plan().actions() {
            let span = action.span().unwrap_or(object_span);
            let (address, actual_ty, bit_field) =
                self.subobject_address(binding, action.path(), span)?;
            if actual_ty != action.destination_type() {
                return Err(InterpreterError::UnsupportedExpression {
                    feature: "static initializer path disagrees with its destination type",
                    span,
                });
            }
            match action {
                InitAction::StoreScalar {
                    destination_type,
                    value,
                    ..
                } => {
                    if let Some(bits) = crate::constant::floating_constant_representation(
                        value,
                        *destination_type,
                        self.unit.types(),
                        self.unit.symbols(),
                        &self.target,
                    ) {
                        self.store_constant_representation(
                            address,
                            *destination_type,
                            bits,
                            false,
                            span,
                        )?;
                        continue;
                    }
                    if crate::complex::floating(self.unit.types(), *destination_type).is_some() {
                        self.store_static_complex(address, *destination_type, value, span)?;
                        continue;
                    }
                    if let Some(element) =
                        crate::complex::element(self.unit.types(), *destination_type)
                    {
                        let pair = crate::complex::constant_components(
                            value,
                            self.unit.types(),
                            self.unit.symbols(),
                            &self.target,
                        )
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: *destination_type,
                            span,
                        })?;
                        let layout = self.access_layout(element, span)?;
                        let integer = self.integer_type(element, span)?;
                        for (index, value) in pair.into_iter().enumerate() {
                            let at = address
                                .checked_offset(index as i64 * layout.size_bytes as i64)
                                .map_err(|e| self.memory_error(e, span))?;
                            self.store_static_scalar(
                                at,
                                element,
                                CValue::Integer(value.cast(integer)),
                                span,
                            )?;
                        }
                        continue;
                    }
                    self.guard_static_initializer_evaluation(value)?;
                    let value = self.eval_static_initializer_value(value)?;
                    let value = cast_machine_value(
                        value,
                        self.value_type(*destination_type, span)?,
                        span,
                        self.unit.types(),
                        &mut self.memory,
                    )?;
                    if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            *destination_type,
                            field,
                            value,
                            false,
                            span,
                        )?;
                    } else {
                        self.store_static_scalar(address, *destination_type, value, span)?;
                    }
                }
                InitAction::ZeroScalar {
                    destination_type, ..
                } => {
                    if matches!(
                        self.unit.types().get(*destination_type).map(|ty| &ty.kind),
                        Some(TypeKind::Float(
                            evocc_types::FloatKind::F32
                                | evocc_types::FloatKind::F64
                                | evocc_types::FloatKind::F128
                        ))
                    ) {
                        self.store_constant_representation(
                            address,
                            *destination_type,
                            CInteger::new(0, CIntegerType::ULONG),
                            false,
                            span,
                        )?;
                        continue;
                    }
                    if crate::complex::admitted(self.unit.types(), *destination_type) {
                        let layout = self.access_layout(*destination_type, span)?;
                        self.memory = self
                            .memory
                            .store_integer(address, layout, 0)
                            .map_err(|e| self.memory_error(e, span))?;
                        continue;
                    }
                    let value = self.zero_value(*destination_type, span)?;
                    if let Some(field) = bit_field {
                        self.store_bit_field(
                            address,
                            *destination_type,
                            field,
                            value,
                            false,
                            span,
                        )?;
                    } else {
                        self.store_static_scalar(address, *destination_type, value, span)?;
                    }
                }
                InitAction::StoreString {
                    destination_type,
                    literal,
                    ..
                } => {
                    let (layout, value) =
                        self.string_initializer_bytes(*destination_type, literal, span)?;
                    self.store_initializer_bytes(address, layout, &value, span)?;
                }
                InitAction::CopyAggregate { value, span, .. } => {
                    let CHirExpr::CompoundLit(ty, initializer, _) = value else {
                        return Err(InterpreterError::UnsupportedExpression {
                            feature: "nonconstant aggregate copy in static initializer",
                            span: *span,
                        });
                    };
                    self.guard_static_initializer_evaluation(value)?;
                    let layout = self.access_layout(*ty, *span)?;
                    let zero = ByteSequence::try_from_bytes(vec![0; layout.size_bytes as usize])
                        .map_err(|error| self.memory_error(error, *span))?;
                    self.store_initializer_bytes(address, layout, &zero, *span)?;
                    self.initialize_static_plan(
                        MemoryBinding { address, ty: *ty },
                        initializer,
                        *span,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn store_initializer_bytes(
        &mut self,
        address: Pointer,
        layout: AccessLayout,
        bytes: &ByteSequence,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        if layout.size_bytes == 0 && bytes.is_empty() {
            return self
                .memory
                .check_pointer(address)
                .map_err(|error| self.memory_error(error, span));
        }
        self.memory = self
            .memory
            .store_bytes(address, layout, bytes)
            .map_err(|error| self.memory_error(error, span))?;
        Ok(())
    }

    // An admitted numeric address expression computes representation bits,
    // not an access. Named objects still require the symbolic relocation path.
    pub(super) fn numeric_static_address(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<Option<CValue>, InterpreterError> {
        let Some(integer) = crate::constant::eval_absolute_address_constant(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) else {
            return Ok(None);
        };
        cast_machine_value(
            CValue::Integer(integer),
            ValueType::Pointer(self.pointer_type_of_expression(expression)?),
            expression.span(),
            self.unit.types(),
            &mut self.memory,
        )
        .map(Some)
    }

    pub(super) fn eval_static_initializer_value(
        &mut self,
        mut expression: &'arena CHirExpr<'arena>,
    ) -> Result<CValue, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(CValue::Integer(value));
        }
        if self.pointer_type_of_expression(expression).is_ok() {
            if let Some(integer) = crate::constant::eval_absolute_address_constant(
                expression,
                self.unit.types(),
                self.unit.symbols(),
                &self.target,
            ) {
                let span = expression.span();
                self.tick(span)?;
                return cast_machine_value(
                    CValue::Integer(integer),
                    self.expression_value_type(expression)?,
                    span,
                    self.unit.types(),
                    &mut self.memory,
                );
            }
        }
        let mut casts = Vec::new();
        while let CHirExpr::Cast(ty, operand, span) = expression {
            self.tick(*span)?;
            casts.push((*ty, *span));
            expression = operand;
        }
        let mut value = match expression {
            CHirExpr::LabelAddress { .. } => self.label_address_value(expression),
            CHirExpr::Ternary(condition, left, right, span)
                if self.infer_integer_type(expression).is_err() =>
            {
                self.tick(*span)?;
                let condition =
                    machine_truth(self.eval_static_initializer_value(condition)?, *span)?;
                let value =
                    self.eval_static_initializer_value(if condition { left } else { right })?;
                let target = self.expression_value_type(expression)?;
                cast_machine_value(value, target, *span, self.unit.types(), &mut self.memory)
            }
            CHirExpr::Binary(operator @ (BinaryOp::Add | BinaryOp::Sub), left, right, span) => {
                if let Ok(integer) = self.eval_static_integer_expression(expression) {
                    Ok(CValue::Integer(integer))
                } else {
                    self.tick(*span)?;
                    let left = self.eval_static_initializer_value(left)?;
                    let right = self.eval_static_initializer_value(right)?;
                    match (left, right) {
                        (CValue::Integer(left), CValue::Integer(right)) => {
                            eval_constant_binary_integer(*operator, left, right)
                                .map(CValue::Integer)
                                .map_err(|error| self.integer_error(error, *span))
                        }
                        (left, right) => self.apply_machine_binary(*operator, left, right, *span),
                    }
                }
            }
            CHirExpr::Binary(operator @ (BinaryOp::Eq | BinaryOp::Ne), left, right, span)
                if self.infer_integer_type(left).is_err()
                    || self.infer_integer_type(right).is_err() =>
            {
                self.tick(*span)?;
                let left = self.eval_static_initializer_value(left)?;
                let right = self.eval_static_initializer_value(right)?;
                self.apply_machine_binary(*operator, left, right, *span)
            }
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => {
                if let Some(value) = self.numeric_static_address(expression)? {
                    Ok(value)
                } else {
                    self.static_address_value(operand, false)
                }
            }
            CHirExpr::Ident { facts, .. }
                if self.unit.types().get(facts.ty()).is_some_and(|ty| {
                    matches!(ty.kind, TypeKind::Array { .. } | TypeKind::Function { .. })
                }) =>
            {
                self.static_address_value(expression, true)
            }
            CHirExpr::MemoryRead { place, .. }
                if self
                    .unit
                    .types()
                    .get(place.value_ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Array { .. })) =>
            {
                self.static_address_value(expression, true)
            }
            CHirExpr::StringLit(..) => self.static_address_value(expression, true),
            CHirExpr::Member(..) | CHirExpr::PtrMember(..) => {
                self.static_address_value(expression, true)
            }
            CHirExpr::CompoundLit(_, _, span) => {
                self.static_compound_pointer(expression, *span, true)
            }
            _ => self
                .eval_static_integer_expression(expression)
                .map(CValue::Integer),
        }?;
        for (ty, span) in casts.into_iter().rev() {
            value = cast_machine_value(
                value,
                self.value_type(ty, span)?,
                span,
                self.unit.types(),
                &mut self.memory,
            )?;
        }
        Ok(value)
    }

    pub(super) fn static_address_value(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        decay: bool,
    ) -> Result<CValue, InterpreterError> {
        let span = expression.span();
        let unsupported = || InterpreterError::UnsupportedExpression {
            feature: "static initializer is not an address constant",
            span,
        };
        let pointer_type =
            |ty| PointerTypeDesc::to_object(self.unit.types(), ty).ok_or_else(unsupported);
        match expression {
            CHirExpr::Ident { symbol, facts, .. } => {
                if self
                    .unit
                    .types()
                    .get(facts.ty())
                    .is_some_and(|ty| matches!(ty.kind, TypeKind::Function { .. }))
                {
                    return Ok(CValue::FunctionPointer {
                        value: PointerValue::Function(
                            self.function_identity(*symbol).ok_or_else(unsupported)?,
                        ),
                        ty: PointerTypeDesc::from_expression(
                            self.unit.symbols(),
                            self.unit.types(),
                            expression,
                        )
                        .map_err(|_| unsupported())?,
                    });
                }
                let binding = self.require_memory_binding(*symbol, span)?;
                let ty = if decay {
                    match self.unit.types().get(binding.ty).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => binding.ty,
                    }
                } else {
                    binding.ty
                };
                Ok(CValue::Pointer {
                    value: binding.address,
                    ty: pointer_type(ty)?,
                })
            }
            CHirExpr::MemoryRead { place, .. } => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| unsupported())?;
                let root = match place.root() {
                    PlaceRoot::Object { symbol } => {
                        self.require_memory_binding(*symbol, span)?.address
                    }
                    PlaceRoot::PointerExpression { pointer, .. } => {
                        expect_machine_pointer(self.eval_static_initializer_value(pointer)?, span)?
                            .0
                    }
                    PlaceRoot::CompoundLiteral { expression, .. } => {
                        expect_machine_pointer(
                            self.static_compound_pointer(expression, span, false)?,
                            span,
                        )?
                        .0
                    }
                };
                let mut indices = Vec::new();
                for projection in place.projections() {
                    if let PlaceProjection::Index(index) = projection {
                        indices.push(self.eval_static_integer_expression(index.index())?);
                    }
                }
                let value = self.project_place_address(root, place, &indices, span)?;
                let ty = if decay {
                    match self.unit.types().get(place.value_ty()).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => place.value_ty(),
                    }
                } else {
                    place.value_ty()
                };
                Ok(CValue::Pointer {
                    value,
                    ty: PointerTypeDesc::to_object(self.unit.types(), ty)
                        .ok_or_else(unsupported)?,
                })
            }
            CHirExpr::StringLit(..) => self.string_pointer(expression, decay),
            CHirExpr::CompoundLit(..) => self.static_compound_pointer(expression, span, decay),
            CHirExpr::Member(base, name, _) | CHirExpr::PtrMember(base, name, _) => {
                let base = if matches!(expression, CHirExpr::Member(..)) {
                    self.static_address_value(base, false)?
                } else {
                    self.eval_static_initializer_value(base)?
                };
                let (mut address, base_ty) = expect_machine_pointer(base, span)?;
                let path = crate::record_member_path(self.unit.types(), base_ty.pointee(), name)
                    .ok_or_else(unsupported)?;
                let mut ty = path.last().ok_or_else(unsupported)?.field_ty();
                for field in path {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), field.tag())
                        .map_err(|_| unsupported())?;
                    let offset = layout
                        .fields
                        .iter()
                        .find(|entry| entry.field_index == field.field_index())
                        .ok_or_else(unsupported)?
                        .byte_offset;
                    address = address
                        .checked_offset(i64::try_from(offset).map_err(|_| unsupported())?)
                        .map_err(|error| self.memory_error(error, span))?;
                }
                if decay {
                    ty = match self.unit.types().get(ty).map(|ty| &ty.kind) {
                        Some(TypeKind::Array { element, .. }) => *element,
                        _ => return Err(unsupported()),
                    };
                }
                Ok(CValue::Pointer {
                    value: address,
                    ty: pointer_type(ty)?,
                })
            }
            CHirExpr::Unary(UnaryOp::Deref, operand, _) => {
                self.eval_static_initializer_value(operand)
            }
            _ => Err(unsupported()),
        }
    }

    pub(super) fn eval_static_integer_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Result<CInteger, InterpreterError> {
        if let Some(value) = crate::constant::fold_floating_integer_arithmetic(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            &self.target,
        ) {
            return Ok(value);
        }
        let previous = std::mem::replace(&mut self.static_integer_evaluation, true);
        let result = self.run_tasks(vec![MachineTask::EvalExpression {
            expression,
            allow_root_mutation: false,
        }]);
        self.static_integer_evaluation = previous;
        let (outcome, mut values) = result?;
        if outcome.is_some() || values.len() != 1 {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "static initializer did not produce one integer value",
                span: expression.span(),
            });
        }
        expect_machine_integer(
            pop_machine_value(&mut values, expression.span())?,
            expression.span(),
        )
    }

    pub(super) fn static_compound_pointer(
        &self,
        expression: &'arena CHirExpr<'arena>,
        span: SourceSpan,
        decay_array: bool,
    ) -> Result<CValue, InterpreterError> {
        let CHirExpr::CompoundLit(ty, _, _) = expression else {
            unreachable!("static compound pointer requires compound expression")
        };
        let binding = self
            .static_compound_bindings
            .get(&(expression as *const CHirExpr<'arena> as usize))
            .copied()
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "static compound literal has no binding",
                span,
            })?;
        let pointee = if decay_array {
            match self.unit.types().get(*ty).map(|canonical| &canonical.kind) {
                Some(TypeKind::Array { element, .. }) => *element,
                _ => *ty,
            }
        } else {
            *ty
        };
        let pointer = PointerTypeDesc::to_object(self.unit.types(), pointee)
            .ok_or(InterpreterError::UnsupportedType { ty: pointee, span })?;
        Ok(CValue::Pointer {
            value: binding.address,
            ty: pointer,
        })
    }
}
