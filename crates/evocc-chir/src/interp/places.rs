//! C-HIR interpreter places.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    /// Recheck a place against the tables of the unit being executed, and
    /// confirm the facts recorded at the use site still describe it.
    ///
    /// A place is checked data supplied by the parser, so this guards against a
    /// place transplanted from an unrelated unit whose numeric identities
    /// happen to overlap, not merely against parser bugs.
    pub(super) fn validate_readable_place(
        &self,
        place: &MemoryPlace<'arena>,
        facts: ExprFacts,
        target_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        self.validate_memory_place(place)
            .map_err(|_| InterpreterError::UnsupportedExpression {
                feature: "invalid typed memory place",
                span: target_span,
            })?;
        if facts.ty() != place.value_ty() || facts.category() != ExprCategory::Value {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "memory read disagrees with its place",
                span: target_span,
            });
        }
        Ok(())
    }

    /// Recheck a place that is about to be written, with the extra conditions
    /// a store requires beyond a load.
    pub(super) fn validate_mutable_place(
        &self,
        place: &MemoryPlace<'arena>,
        facts: ExprFacts,
        target_span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        self.validate_readable_place(place, facts, target_span)?;
        if !place.is_modifiable() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "store to an unmodifiable place",
                span: target_span,
            });
        }
        Ok(())
    }

    /// Schedule evaluation of a projected place: its root address first, then
    /// each subscript expression in source order.
    ///
    /// Returns the number of subscript values the completion task must pop.
    /// Nothing here evaluates a subexpression more than once, which is why the
    /// designating syntax is not retained alongside the place.
    pub(super) fn schedule_place_operands(
        &mut self,
        place: &MemoryPlace<'arena>,
        target_span: SourceSpan,
        tasks: &mut Vec<MachineTask<'arena>>,
        values: &mut Vec<CValue>,
    ) -> Result<usize, InterpreterError> {
        let subscripts: Vec<&'arena CHirExpr<'arena>> = place
            .projections()
            .iter()
            .filter_map(PlaceProjection::source_expression)
            .collect();
        // The task stack is last-in-first-out, so pushing in reverse source
        // order makes the machine evaluate the root first and then each
        // subscript left to right.
        for subscript in subscripts.iter().rev() {
            tasks.push(MachineTask::EvalExpression {
                expression: subscript,
                allow_root_mutation: false,
            });
        }
        match place.root() {
            PlaceRoot::PointerExpression { pointer, .. } => {
                tasks.push(MachineTask::EvalExpression {
                    expression: pointer,
                    allow_root_mutation: true,
                });
            }
            PlaceRoot::Object { symbol } => {
                let binding = self.require_memory_binding(*symbol, target_span)?;
                values.push(CValue::Pointer {
                    value: binding.address,
                    ty: self.root_pointer_type(place, target_span)?,
                });
            }
            PlaceRoot::CompoundLiteral { expression, .. } => {
                self.schedule_compound_literal(expression, true, tasks)?;
            }
        }
        Ok(subscripts.len())
    }

    pub(super) fn schedule_compound_literal(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        as_address: bool,
        tasks: &mut Vec<MachineTask<'arena>>,
    ) -> Result<(), InterpreterError> {
        let CHirExpr::CompoundLit(ty, initializer, span) = expression else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "compound-literal allocation requested for another expression",
                span: expression.span(),
            });
        };
        if initializer.plan().target_type() != *ty
            || initializer.plan().validate(self.unit.types()).is_err()
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "invalid compound-literal initializer plan",
                span: *span,
            });
        }
        let site = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?
            .compound_literal_site(expression)
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "compound literal has no allocation site",
                span: *span,
            })?;
        let static_storage = matches!(
            site.kind(),
            AllocationSiteKind::CompoundLiteral {
                static_storage: true,
                ..
            }
        );
        let identity = expression as *const CHirExpr<'arena> as usize;
        let binding = if static_storage {
            self.static_compound_bindings
                .get(&identity)
                .copied()
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "static compound literal was not initialized",
                    span: *span,
                })?
        } else {
            let layout = self.access_layout(*ty, *span)?;
            let (frame, scope) = self
                .memory_frame
                .as_ref()
                .map(|frame| (frame.id, frame.current_scope))
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "automatic compound literal has no active scope",
                    span: *span,
                })?;
            if let Some(binding) = self
                .memory_frame
                .as_ref()
                .and_then(|frame| frame.compound_bindings.get(&(scope, identity)))
                .copied()
            {
                binding
            } else {
                let (memory, address) = self
                    .memory
                    .allocate_automatic(frame, scope, site.ordinal().raw(), layout)
                    .map_err(|error| self.memory_error(error, *span))?;
                self.memory = memory;
                let binding = MemoryBinding { address, ty: *ty };
                self.memory_frame
                    .as_mut()
                    .expect("active compound scope")
                    .compound_bindings
                    .insert((scope, identity), binding);
                binding
            }
        };
        if !static_storage && self.unit.zero_initialize_compound_padding(initializer) {
            self.zero_constructor_padding(binding, initializer, *span)?;
        }
        if !static_storage && initializer.plan().zero_fills_unwritten() {
            self.zero_initialize_subobjects(
                binding.address,
                *ty,
                SubobjectPath::root(),
                initializer.plan().actions(),
                *span,
            )?;
        }
        tasks.push(MachineTask::YieldCompoundLiteral {
            binding,
            as_address,
            span: *span,
        });
        if !static_storage {
            tasks.push(MachineTask::ContinueMemoryInitializer {
                target: InitializerTarget {
                    binding,
                    initializer,
                    symbol: None,
                    span: *span,
                },
                index: 0,
            });
        }
        Ok(())
    }

    /// Pointer type of a place's root, which is what address folding starts
    /// from and therefore what the value stack must be carrying.
    pub(super) fn root_pointer_type(
        &self,
        place: &MemoryPlace<'_>,
        span: SourceSpan,
    ) -> Result<PointerTypeDesc, InterpreterError> {
        place
            .root_pointer_type(self.unit.symbols(), self.unit.types())
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "place root has no pointer type",
                span,
            })
    }

    /// Fold a root address and already-evaluated subscript values into the
    /// address of the designated subobject.
    ///
    /// Every step goes through the memory model's checked pointer arithmetic,
    /// so an out-of-range subscript or an overflowing offset becomes a typed
    /// diagnostic rather than a wrapped address.
    pub(super) fn project_place_address(
        &self,
        root: Pointer,
        place: &MemoryPlace<'_>,
        subscripts: &[CInteger],
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let mut address = root;
        let mut consumed = 0;
        for projection in place.projections() {
            address = match projection {
                PlaceProjection::Index(projection) => {
                    let subscript = subscripts.get(consumed).copied().ok_or(
                        InterpreterError::UnsupportedExpression {
                            feature: "missing subscript value for projected place",
                            span,
                        },
                    )?;
                    consumed += 1;
                    let scale =
                        self.pointer_arithmetic_scale(projection.element_ty(), address, span)?;
                    let signedness = if subscript.ty().is_signed() {
                        IndexSignedness::Signed
                    } else {
                        IndexSignedness::Unsigned
                    };
                    address
                        .checked_scaled_offset(
                            if scale == 0 { 0 } else { subscript.raw_bits() },
                            u16::from(subscript.ty().bits()),
                            signedness,
                            OffsetDirection::Add,
                            scale.max(1),
                        )
                        .map_err(|error| self.memory_error(error, span))?
                }
                PlaceProjection::Field(projection) => {
                    let layout = self
                        .target
                        .record_layout(self.unit.types(), projection.tag())
                        .map_err(|_| InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        })?;
                    let field = layout
                        .fields
                        .iter()
                        .find(|field| field.field_index == projection.field_index())
                        .ok_or(InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        })?;
                    let offset = i64::try_from(field.byte_offset).map_err(|_| {
                        InterpreterError::UnsupportedType {
                            ty: projection.field_ty(),
                            span,
                        }
                    })?;
                    address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?
                }
            };
        }
        if consumed != subscripts.len() {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "unconsumed subscript value at projected place",
                span,
            });
        }
        Ok(address)
    }

    /// Pop a projected place's operands and resolve its final address.
    ///
    /// The value stack holds the root address followed by `subscript_count`
    /// subscript values in source order, so they are popped in reverse and
    /// restored to source order before folding.
    pub(super) fn resolve_projected_address(
        &self,
        place: &MemoryPlace<'_>,
        subscript_count: usize,
        values: &mut Vec<CValue>,
        span: SourceSpan,
    ) -> Result<Pointer, InterpreterError> {
        let mut subscripts = Vec::with_capacity(subscript_count);
        for _ in 0..subscript_count {
            let value = pop_machine_value(values, span)?;
            subscripts.push(expect_machine_integer(value, span)?);
        }
        subscripts.reverse();
        let (root, actual) = expect_machine_pointer(pop_machine_value(values, span)?, span)?;
        let expected = self.root_pointer_type(place, span)?;
        if actual != expected {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent pointer type at projected place",
                span,
            });
        }
        self.project_place_address(root, place, &subscripts, span)
    }

    pub(super) fn aggregate_member_address(
        &self,
        aggregate: CValue,
        name: &str,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Qualifiers), InterpreterError> {
        let CValue::Aggregate {
            address,
            ty,
            qualifiers,
        } = aggregate
        else {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "member base is not an aggregate value",
                span,
            });
        };
        self.aggregate_member_address_in(address, ty, qualifiers, name, span)
    }

    pub(super) fn aggregate_member_address_in(
        &self,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        name: &str,
        span: SourceSpan,
    ) -> Result<(Pointer, TypeId, Qualifiers), InterpreterError> {
        let tag = match self.unit.types().get(ty).map(|ty| &ty.kind) {
            Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => *tag,
            _ => return Err(InterpreterError::UnsupportedType { ty, span }),
        };
        let fields = match &self
            .unit
            .types()
            .tag(tag)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?
            .state
        {
            TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) => {
                fields
            }
            _ => return Err(InterpreterError::UnsupportedType { ty, span }),
        };
        let record = self
            .target
            .record_layout(self.unit.types(), tag)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
        for (field_index, field) in fields.iter().enumerate() {
            let layout = record
                .fields
                .iter()
                .find(|layout| layout.field_index == field_index)
                .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?;
            if layout.bit_field.is_some() {
                continue;
            }
            let offset = i64::try_from(layout.byte_offset)
                .map_err(|_| InterpreterError::UnsupportedType { ty: field.ty, span })?;
            let field_address = address
                .checked_offset(offset)
                .map_err(|error| self.memory_error(error, span))?;
            let field_qualifiers = merge_memory_qualifiers(
                qualifiers,
                self.unit
                    .types()
                    .get(field.ty)
                    .ok_or(InterpreterError::UnsupportedType { ty: field.ty, span })?
                    .qualifiers,
            );
            if field.name.as_deref() == Some(name) {
                return Ok((field_address, field.ty, field_qualifiers));
            }
            if field.name.is_none()
                && matches!(
                    self.unit.types().get(field.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Struct(_) | TypeKind::Union(_))
                )
            {
                if let Ok(found) = self.aggregate_member_address_in(
                    field_address,
                    field.ty,
                    field_qualifiers,
                    name,
                    span,
                ) {
                    return Ok(found);
                }
            }
        }
        Err(InterpreterError::UnsupportedExpression {
            feature: "unknown aggregate member",
            span,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_memory_assignment(
        &mut self,
        operator: AssignmentOp,
        address: Pointer,
        ty: TypeId,
        qualifiers: Qualifiers,
        right: CValue,
        target_span: SourceSpan,
        span: SourceSpan,
        layout: AccessLayout,
    ) -> Result<CValue, InterpreterError> {
        if operator == AssignmentOp::Simple {
            // Simple assignment converts its right operand to the type of the
            // designated object, which is where a null pointer constant becomes
            // a null pointer and a narrower integer is truncated.
            let right = self.cast_runtime_value(right, self.value_type(ty, span)?, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, right, span, layout);
        }
        if qualifiers.is_atomic {
            return self
                .apply_atomic_assignment(operator, address, ty, qualifiers, right, span, layout);
        }
        let left =
            self.load_memory_value_with_layout(address, ty, qualifiers, target_span, layout)?;
        if crate::complex::admitted(self.unit.types(), ty) {
            let op = crate::vector::assignment_binary(operator).ok_or(
                InterpreterError::UnsupportedExpression {
                    feature: "invalid complex assignment",
                    span,
                },
            )?;
            let operation = crate::complex::ComplexExpr {
                op: crate::complex::ComplexOp::Binary(op),
                operands: Vec::new(),
                facts: ExprFacts::value(ty),
                span,
            };
            let result = self.apply_complex(&operation, &[left, right])?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if let CValue::Aggregate { ty: complex, .. } = right {
            if crate::complex::admitted(self.unit.types(), complex) {
                // A real target with a complex value: the front end converted
                // the value to the common complex type. Compute there, then
                // convert the result to the target type (EvoIR 10.1.14).
                let op = crate::vector::assignment_binary(operator).ok_or(
                    InterpreterError::UnsupportedExpression {
                        feature: "invalid complex assignment",
                        span,
                    },
                )?;
                let mut operation = crate::complex::ComplexExpr {
                    op: crate::complex::ComplexOp::Binary(op),
                    operands: Vec::new(),
                    facts: ExprFacts::value(complex),
                    span,
                };
                let result = self.apply_complex(&operation, &[left, right])?;
                operation.op = crate::complex::ComplexOp::Convert;
                operation.facts = ExprFacts::value(ty);
                let result = self.apply_complex(&operation, &[result])?;
                return self
                    .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
            }
        }
        if crate::vector::shape(self.unit.types(), ty).is_none()
            && (matches!(left, CValue::Floating(_)) || matches!(right, CValue::Floating(_)))
        {
            let op =
                assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                    feature: "invalid floating compound assignment",
                    span,
                })?;
            let result = self.apply_machine_binary(op, left, right, span)?;
            let result = self.cast_runtime_value(result, self.value_type(ty, span)?, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if crate::vector::shape(self.unit.types(), ty).is_some() {
            let op = crate::vector::assignment_binary(operator).ok_or(
                InterpreterError::UnsupportedExpression {
                    span,
                    feature: "invalid vector assignment",
                },
            )?;
            let operation = crate::vector::VectorExpr {
                op: crate::vector::VectorOp::Binary(op),
                operands: Vec::new(),
                facts: ExprFacts::value(ty),
                span,
            };
            let result = self.apply_vector(&operation, &[left, right])?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        if matches!(
            left,
            CValue::Pointer { .. } | CValue::FunctionPointer { .. }
        ) {
            let binary = match operator {
                AssignmentOp::Add => BinaryOp::Add,
                AssignmentOp::Subtract => BinaryOp::Sub,
                _ => {
                    return Err(InterpreterError::UnsupportedExpression {
                        feature: "invalid compound pointer assignment operator",
                        span,
                    });
                }
            };
            let result = self.apply_machine_binary(binary, left, right, span)?;
            return self
                .store_memory_value_with_layout(address, ty, qualifiers, result, span, layout);
        }
        let left = expect_machine_integer(left, target_span)?;
        let right = expect_machine_integer(right, span)?;
        let binary =
            assignment_binary(operator).ok_or(InterpreterError::UnsupportedExpression {
                feature: "invalid compound assignment operator",
                span,
            })?;
        let result = self.runtime_integer_binary(binary, left, right, span)?;
        self.store_memory_value_with_layout(
            address,
            ty,
            qualifiers,
            CValue::Integer(result),
            span,
            layout,
        )
    }
}
