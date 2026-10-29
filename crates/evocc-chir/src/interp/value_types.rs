//! C-HIR interpreter value types.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn tick(&mut self, span: SourceSpan) -> Result<(), InterpreterError> {
        if self.consumed >= self.fuel_limit {
            return Err(InterpreterError::FuelExhausted {
                limit: self.fuel_limit,
                consumed: self.consumed,
                span: self.fuel_anchor.unwrap_or(span),
            });
        }
        self.consumed += 1;
        Ok(())
    }

    pub(super) fn integer_type(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<CIntegerType, InterpreterError> {
        if let Some(TypeKind::BuiltinVaList) =
            self.unit.types().get(ty).map(|canonical| &canonical.kind)
        {
            return Err(InterpreterError::VarArgs { span });
        }
        canonical_integer_type(self.unit.types(), ty)
            .ok_or(InterpreterError::UnsupportedType { ty, span })
    }

    pub(super) fn value_type(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<ValueType, InterpreterError> {
        match self.unit.types().get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Void) => Ok(ValueType::Void),
            // A value has the format of its type: a Darwin `long double`
            // value is binary64.
            Some(TypeKind::Float(
                kind @ (evocc_types::FloatKind::F32
                | evocc_types::FloatKind::F64
                | evocc_types::FloatKind::F128),
            )) => Ok(ValueType::Floating(self.target.float_format(*kind))),
            // A half value is its storage format (EvoIR 3.13.6).
            Some(TypeKind::TargetBuiltin(builtin)) if builtin.half_format().is_some() => {
                Ok(ValueType::Floating(builtin.half_format().unwrap()))
            }
            Some(
                TypeKind::Struct(_)
                | TypeKind::Union(_)
                | TypeKind::BuiltinVaList
                | TypeKind::Complex(evocc_types::ComplexBase::Integer(_))
                | TypeKind::Vector { .. },
            ) => Ok(ValueType::Aggregate(ty)),
            Some(TypeKind::Complex(base)) if base.executable_part().is_some() => {
                Ok(ValueType::Aggregate(ty))
            }
            Some(TypeKind::Pointer(_)) => {
                PointerTypeDesc::from_canonical_pointer(self.unit.types(), ty)
                    .map(ValueType::Pointer)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })
            }
            _ => self.integer_type(ty, span).map(ValueType::Integer),
        }
    }

    pub(super) fn integer_error(&self, error: IntegerError, span: SourceSpan) -> InterpreterError {
        InterpreterError::Integer { error, span }
    }

    pub(super) fn memory_error(&self, error: MemoryFault, span: SourceSpan) -> InterpreterError {
        InterpreterError::Memory { error, span }
    }

    pub(super) fn pointer_load_error(
        error: MemoryFault,
        address: Pointer,
        access: AccessLayout,
        span: SourceSpan,
    ) -> InterpreterError {
        let error = if matches!(error, MemoryFault::UninitializedRead { .. }) {
            MemoryFault::InvalidPointerRepresentation { address, access }
        } else {
            error
        };
        InterpreterError::Memory { error, span }
    }

    pub(super) fn access_layout(
        &self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        self.target
            .layout_of(self.unit.types(), ty)
            .map(AccessLayout::from_type_layout)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })
    }

    pub(super) fn object_access_layout(
        &self,
        ty: TypeId,
        initializer: Option<&crate::ObjectInitializer<'arena>>,
        span: SourceSpan,
    ) -> Result<AccessLayout, InterpreterError> {
        let Some(initializer) = initializer else {
            if self
                .target
                .array_size_factors(self.unit.types(), ty)
                .is_ok_and(|(_, captures)| !captures.is_empty())
            {
                let size = self.pointer_arithmetic_scale(ty, Pointer::NULL, span)?;
                let align = self
                    .target
                    .alignof_type(self.unit.types(), ty)
                    .map_err(|_| InterpreterError::UnsupportedType { ty, span })?;
                return Ok(AccessLayout::new(size, align));
            }
            return self.access_layout(ty, span);
        };
        if initializer.plan().target_type() != ty {
            return Err(InterpreterError::UnsupportedType { ty, span });
        }
        initializer
            .plan()
            .storage_layout(self.unit.types(), &self.target)
            .map(AccessLayout::from_type_layout)
            .map_err(|_| InterpreterError::UnsupportedType { ty, span })
    }

    pub(super) fn memory_binding(&self, symbol: SymbolId) -> Option<MemoryBinding> {
        let symbol = self.linkage.target(symbol);
        self.memory_frame
            .as_ref()
            .and_then(|frame| frame.bindings.get(&symbol).copied())
            .or_else(|| self.static_bindings.get(&symbol).copied())
    }

    pub(super) fn require_memory_binding(
        &self,
        symbol: SymbolId,
        span: SourceSpan,
    ) -> Result<MemoryBinding, InterpreterError> {
        self.memory_binding(symbol)
            .ok_or(InterpreterError::OutOfScopeObject {
                symbol,
                spelling: self.unit.symbols().symbol(symbol).map_or_else(
                    |_| format!("symbol#{}", symbol.raw()),
                    |symbol| symbol.spelling().to_owned(),
                ),
                span,
            })
    }
}
