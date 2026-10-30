//! C-HIR interpreter objects.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn enter_memory_scope(
        &mut self,
        span: SourceSpan,
    ) -> Result<Option<MemoryScopeExit>, InterpreterError> {
        if !self.memory_mode {
            return Ok(None);
        }
        let frame = self
            .memory_frame
            .as_ref()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "lexical scope without an interpreter memory frame",
                span,
            })?;
        let parent = frame.current_scope;
        let bindings = frame.bindings.clone();
        let (memory, scope) = self
            .memory
            .begin_scope(parent)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        self.memory_frame
            .as_mut()
            .expect("memory frame checked immediately before scope creation")
            .current_scope = scope;
        Ok(Some(MemoryScopeExit {
            scope,
            parent,
            bindings,
            span,
        }))
    }

    pub(super) fn exit_memory_scope(
        &mut self,
        exit: MemoryScopeExit,
    ) -> Result<(), InterpreterError> {
        let frame = self
            .memory_frame
            .as_ref()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "lexical scope exit without an interpreter memory frame",
                span: exit.span,
            })?;
        if frame.current_scope != exit.scope {
            return Err(InterpreterError::UnsupportedStatement {
                feature: "non-LIFO interpreter memory scope exit",
                span: exit.span,
            });
        }
        self.run_scope_cleanups(Some(exit.scope))?;
        self.memory = self
            .memory
            .end_scope(exit.scope)
            .map_err(|error| self.memory_error(error, exit.span))?;
        let frame = self
            .memory_frame
            .as_mut()
            .expect("memory frame checked immediately before scope exit");
        frame.current_scope = exit.parent;
        frame.bindings = exit.bindings;
        frame
            .compound_bindings
            .retain(|(scope, _), _| *scope != exit.scope);
        Ok(())
    }

    pub(super) fn run_scope_cleanups(
        &mut self,
        scope: Option<MemoryScopeId>,
    ) -> Result<(), InterpreterError> {
        let frame = self
            .memory_frame
            .as_mut()
            .expect("cleanup has a live frame");
        let mut cleanups = Vec::new();
        frame.cleanups.retain(|(owner, expression)| {
            if scope.is_none_or(|scope| scope == *owner) {
                cleanups.push(*expression);
                false
            } else {
                true
            }
        });
        for expression in cleanups.into_iter().rev() {
            let mut tasks = Vec::new();
            self.schedule_discarded_expression(expression, &mut tasks)?;
            self.run_tasks(tasks)?;
        }
        Ok(())
    }

    pub(super) fn validate_memory_object_declaration(
        &self,
        declaration: &ObjectDecl<'arena>,
    ) -> Result<(), InterpreterError> {
        let symbol = self
            .unit
            .symbols()
            .symbol(declaration.symbol)
            .map_err(|_| InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            })?;
        if symbol.kind() != SymbolKind::Object
            || symbol.spelling() != declaration.name.spelling
            || !self.unit.types().compatible(symbol.ty(), declaration.ty)
            || !(symbol.storage_class() == Some(declaration.storage)
                || (declaration.storage.base() == StorageClass::Extern
                    && matches!(
                        symbol.storage_class().map(StorageClass::base),
                        Some(StorageClass::None | StorageClass::Static)
                    )))
            || !matches!(
                declaration.storage.base(),
                StorageClass::None
                    | StorageClass::Auto
                    | StorageClass::Register
                    | StorageClass::Static
                    | StorageClass::Extern
            )
        {
            return Err(InterpreterError::MalformedObjectDeclaration {
                symbol: declaration.symbol,
                span: declaration.span,
            });
        }
        let canonical =
            self.unit
                .types()
                .get(declaration.ty)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: declaration.ty,
                    span: declaration.span,
                })?;
        // Object storage includes nominal target scalar types as well as
        // ordinary scalars and aggregates. Execution checks their value kind
        // separately; an object declaration is not an arithmetic operation.
        if !matches!(
            canonical.kind,
            TypeKind::Int(_)
                | TypeKind::GnuInt128(_)
                | TypeKind::Enum(_)
                | TypeKind::Pointer(_)
                | TypeKind::Array { .. }
                | TypeKind::Vector { .. }
                | TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::BuiltinVaList
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::TargetBuiltin(_)
        ) {
            return Err(InterpreterError::UnsupportedType {
                ty: declaration.ty,
                span: declaration.span,
            });
        }
        // A declaration does not allocate an object: an earlier extern array
        // may still be incomplete even when its later definition is complete.
        if declaration.storage.base() != StorageClass::Extern {
            self.object_access_layout(
                declaration.ty,
                declaration.initializer.as_ref(),
                declaration.span,
            )?;
        }
        Ok(())
    }

    pub(super) fn allocate_automatic_object(
        &mut self,
        symbol: SymbolId,
        ty: TypeId,
        initializer: Option<&crate::ObjectInitializer<'arena>>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let site = self
            .manifest
            .as_ref()
            .ok_or(InterpreterError::MissingAllocationManifest)?
            .object_site(symbol)
            .ok_or(InterpreterError::MalformedObjectDeclaration { symbol, span })?
            .ordinal()
            .raw();
        let mut layout = self.object_access_layout(ty, initializer, span)?;
        layout.align_bytes = layout.align_bytes.max(
            self.unit
                .symbols()
                .symbol(symbol)
                .map_err(|_| InterpreterError::MalformedObjectDeclaration { symbol, span })?
                .declared_alignment(),
        );
        let (frame, scope) = self
            .memory_frame
            .as_ref()
            .map(|frame| (frame.id, frame.current_scope))
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "automatic object without an interpreter memory frame",
                span,
            })?;
        let (memory, address) = self
            .memory
            .allocate_automatic(frame, scope, site, layout)
            .map_err(|error| self.memory_error(error, span))?;
        self.memory = memory;
        let bindings = &mut self
            .memory_frame
            .as_mut()
            .ok_or(InterpreterError::UnsupportedStatement {
                feature: "automatic object without an interpreter memory frame",
                span,
            })?
            .bindings;
        // Re-entering a scope allocates a fresh object at the same site, so a
        // previous binding is replaced rather than rejected: the memory model
        // has already issued a new object identity and left the old one as a
        // tombstone, which is what makes a stale pointer stay dangling instead
        // of silently addressing the new object. Two declarations of one symbol
        // within a single scope activation are handled by reusing the binding:
        // a backward jump can execute one declaration's initializer again.
        bindings.insert(symbol, MemoryBinding { address, ty });
        Ok(())
    }

    pub(super) fn zero_constructor_padding(
        &mut self,
        binding: MemoryBinding,
        initializer: &crate::ObjectInitializer<'arena>,
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        let ranges = initializer
            .plan()
            .padding_ranges(self.unit.types(), self.target)
            .map_err(|_| InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?;
        for range in ranges {
            let at = binding
                .address
                .checked_offset(range.start as i64)
                .map_err(|error| self.memory_error(error, span))?;
            let bytes = ByteSequence::try_from_bytes(vec![0; (range.end - range.start) as usize])
                .map_err(|error| self.memory_error(error, span))?;
            self.memory = self
                .memory
                .store_bytes(at, AccessLayout::new(range.end - range.start, 1), &bytes)
                .map_err(|error| self.memory_error(error, span))?;
        }
        Ok(())
    }

    pub(super) fn zero_initialize_subobjects(
        &mut self,
        address: Pointer,
        ty: TypeId,
        path: SubobjectPath,
        actions: &[InitAction<'arena>],
        span: SourceSpan,
    ) -> Result<(), InterpreterError> {
        if actions
            .iter()
            .any(|action| path.steps().starts_with(action.path().steps()))
        {
            return Ok(());
        }
        let kind = self
            .unit
            .types()
            .get(ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })?
            .kind
            .clone();
        match kind {
            TypeKind::Float(
                evocc_types::FloatKind::F32
                | evocc_types::FloatKind::F64
                | evocc_types::FloatKind::F128,
            ) => self.store_constant_representation(
                address,
                ty,
                CInteger::new(0, CIntegerType::ULONG),
                false,
                span,
            ),
            TypeKind::Int(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::TargetBuiltin(_) => {
                let value = self.zero_value(ty, span)?;
                self.store_initialized_value(address, ty, value, span)?;
                Ok(())
            }
            TypeKind::BuiltinVaList => Ok(()),
            TypeKind::Vector { element, lanes, .. } => {
                let layout = self.access_layout(element, span)?;
                for index in 0..u64::from(lanes) {
                    let at = address
                        .checked_offset((index * layout.size_bytes) as i64)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        at,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(count),
            } => {
                let element_layout = self.object_access_layout(element, None, span)?;
                for index in 0..count {
                    let offset = index
                        .checked_mul(element_layout.size_bytes)
                        .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                    let offset = i64::try_from(offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                    let element_address = address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        element_address,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(()),
            TypeKind::Array {
                element,
                bound: ArrayBound::Captured(_),
            } => {
                let layout = self.object_access_layout(ty, None, span)?;
                let element_layout = self.object_access_layout(element, None, span)?;
                if element_layout.size_bytes == 0 {
                    return Ok(());
                }
                for index in 0..layout.size_bytes / element_layout.size_bytes {
                    let at = address
                        .checked_offset((index * element_layout.size_bytes) as i64)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        at,
                        element,
                        path.clone().with_index(index),
                        actions,
                        span,
                    )?;
                }
                Ok(())
            }
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            }
            | TypeKind::Void
            | TypeKind::Function { .. } => Err(InterpreterError::UnsupportedType { ty, span }),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let is_union = matches!(kind, TypeKind::Union(_));
                let definition = self
                    .unit
                    .types()
                    .tag(tag)
                    .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                let fields = match &definition.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields.clone(),
                    _ => return Err(InterpreterError::UnsupportedType { ty, span }),
                };
                let layout = self
                    .target
                    .record_layout(self.unit.types(), tag)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                let selected_union_field = is_union.then(|| {
                    actions.iter().find_map(|action| {
                        let suffix = action.path().steps().strip_prefix(path.steps())?;
                        match suffix.first()? {
                            SubobjectStep::Field { field_index } => Some(*field_index),
                            SubobjectStep::Index { .. } => None,
                        }
                    })
                });
                for (field_index, field) in fields.iter().enumerate() {
                    if selected_union_field
                        .flatten()
                        .is_some_and(|selected| selected != field_index)
                    {
                        continue;
                    }
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    let field_layout = layout
                        .fields
                        .iter()
                        .find(|layout| layout.field_index == field_index)
                        .ok_or(InterpreterError::UnsupportedType { ty, span })?;
                    if let Some(field) = field_layout.bit_field {
                        let count = (field.bit_offset % 8 + field.width_bits).div_ceil(8);
                        for byte in 0..count {
                            let at = address
                                .checked_offset((field_layout.byte_offset + u64::from(byte)) as i64)
                                .map_err(|error| self.memory_error(error, span))?;
                            self.memory = self
                                .memory
                                .store_integer(at, AccessLayout::new(1, 1), 0)
                                .map_err(|error| self.memory_error(error, span))?;
                        }
                        continue;
                    }
                    let offset = i64::try_from(field_layout.byte_offset)
                        .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                    let field_address = address
                        .checked_offset(offset)
                        .map_err(|error| self.memory_error(error, span))?;
                    self.zero_initialize_subobjects(
                        field_address,
                        field.ty,
                        path.clone().with_field(field_index),
                        actions,
                        span,
                    )?;
                    if is_union {
                        break;
                    }
                }
                Ok(())
            }
        }
    }

    pub(super) fn string_initializer_bytes(
        &self,
        destination_type: TypeId,
        literal: &StringLit,
        span: SourceSpan,
    ) -> Result<(AccessLayout, ByteSequence), InterpreterError> {
        let Some(TypeKind::Array { element, bound }) = self
            .unit
            .types()
            .get(destination_type)
            .map(|canonical| &canonical.kind)
        else {
            return Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            });
        };
        let count =
            literal
                .initializer_element_count(bound)
                .ok_or(InterpreterError::UnsupportedType {
                    ty: destination_type,
                    span,
                })?;
        let element = self.access_layout(*element, span)?;
        let destination = if *bound == ArrayBound::Unbounded {
            AccessLayout::new(
                element
                    .size_bytes
                    .checked_mul(count)
                    .ok_or(InterpreterError::UnsupportedType {
                        ty: destination_type,
                        span,
                    })?,
                element.align_bytes,
            )
        } else {
            self.access_layout(destination_type, span)?
        };
        let width =
            usize::try_from(element.size_bytes).map_err(|_| InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            })?;
        let count = usize::try_from(count).map_err(|_| InterpreterError::UnsupportedType {
            ty: destination_type,
            span,
        })?;
        if width == 0
            || width > 4
            || count.checked_mul(width) != usize::try_from(destination.size_bytes).ok()
        {
            return Err(InterpreterError::UnsupportedType {
                ty: destination_type,
                span,
            });
        }
        let mut units = literal
            .decoded_units()
            .ok_or(InterpreterError::UnsupportedExpression {
                feature: "invalid string initializer",
                span,
            })?;
        if units.len() > count {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "string initializer exceeds destination",
                span,
            });
        }
        units.resize(count, 0);
        let mut bytes = Vec::with_capacity(count.saturating_mul(width));
        for unit in units {
            let encoded = match self.target.byte_order() {
                evocc_types::ByteOrder::Little => unit.to_le_bytes(),
                evocc_types::ByteOrder::Big => unit.to_be_bytes(),
            };
            match self.target.byte_order() {
                evocc_types::ByteOrder::Little => bytes.extend_from_slice(&encoded[..width]),
                evocc_types::ByteOrder::Big => bytes.extend_from_slice(&encoded[4 - width..]),
            }
        }
        let value =
            ByteSequence::try_from_bytes(bytes).map_err(|error| self.memory_error(error, span))?;
        Ok((destination, value))
    }
}
