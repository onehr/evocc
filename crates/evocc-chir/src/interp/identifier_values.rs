//! C-HIR interpreter identifier values.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn zero_value(
        &mut self,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        if crate::complex::admitted(self.unit.types(), ty) {
            let address = self.vector_temporary(ty, span)?;
            let layout = self.access_layout(ty, span)?;
            self.memory = self
                .memory
                .store_integer(address, layout, 0)
                .map_err(|e| self.memory_error(e, span))?;
            return Ok(CValue::Aggregate {
                address,
                ty,
                qualifiers: Qualifiers::default(),
            });
        }
        match self.value_type(ty, span)? {
            ValueType::Integer(integer) => Ok(CValue::Integer(CInteger::new(0, integer))),
            ValueType::Floating(kind) => Ok(CValue::Floating(
                evocc_types::floating::FloatBits::from_bits(kind, 0).unwrap(),
            )),
            ValueType::Pointer(pointer)
                if matches!(
                    self.unit.types().get(pointer.pointee()).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) =>
            {
                Ok(CValue::FunctionPointer {
                    value: PointerValue::Null,
                    ty: pointer,
                })
            }
            ValueType::Pointer(pointer) => Ok(CValue::Pointer {
                value: Pointer::NULL,
                ty: pointer,
            }),
            ValueType::Void | ValueType::Aggregate(_) => {
                Err(InterpreterError::UnsupportedType { ty, span })
            }
        }
    }

    pub(super) fn load_identifier(
        &mut self,
        symbol: SymbolId,
        spelling: &str,
        facts: ExprFacts,
        span: SourceSpan,
    ) -> Result<CValue, InterpreterError> {
        let declaration = self.unit.symbols().symbol(symbol).map_err(|_| {
            InterpreterError::UnsupportedExpression {
                feature: "unknown identifier",
                span,
            }
        })?;
        if declaration.spelling() != spelling
            || !crate::type_at_use_matches_binding(self.unit.types(), facts.ty(), declaration.ty())
        {
            return Err(InterpreterError::UnsupportedExpression {
                feature: "inconsistent identifier facts",
                span,
            });
        }
        if is_global_register(self.unit, symbol) {
            return Err(InterpreterError::Asm { span });
        }
        if let Some(value) = declaration.enumerator_value() {
            let ty = self.integer_type(declaration.ty(), span)?;
            return Ok(CValue::Integer(CInteger::new(value as u64, ty)));
        }
        if declaration.kind() == SymbolKind::Function
            && facts.category() == ExprCategory::FunctionDesignator
            && matches!(
                self.unit.types().get(facts.ty()).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            )
        {
            let identity =
                self.function_identity(symbol)
                    .ok_or(InterpreterError::UnsupportedExpression {
                        feature: "function designator has no definition identity",
                        span,
                    })?;
            let ty = PointerTypeDesc::to_object(self.unit.types(), facts.ty()).ok_or(
                InterpreterError::UnsupportedType {
                    ty: facts.ty(),
                    span,
                },
            )?;
            return Ok(CValue::FunctionPointer {
                value: PointerValue::function(identity)
                    .map_err(|error| self.memory_error(error, span))?,
                ty,
            });
        }
        // C11 6.3.2.1p3: an array lvalue used as a value becomes a pointer to
        // its first element. The array itself is never loaded, so this is the
        // only way an array-typed identifier can produce one.
        if let Some(TypeKind::Array { element, .. }) = self
            .unit
            .types()
            .get(declaration.ty())
            .map(|canonical| &canonical.kind)
        {
            let element = *element;
            let binding = self.require_memory_binding(symbol, span)?;
            let ty = PointerTypeDesc::to_object(self.unit.types(), element)
                .ok_or(InterpreterError::UnsupportedType { ty: element, span })?;
            return Ok(CValue::Pointer {
                value: binding.address,
                ty,
            });
        }
        if !self.memory_mode {
            let slot = self
                .slots
                .get(&symbol)
                .ok_or(InterpreterError::UnsupportedExpression {
                    feature: "identifier without automatic scalar slot",
                    span,
                })?;
            if !slot.active {
                return Err(InterpreterError::OutOfScopeObject {
                    symbol,
                    spelling: spelling.to_owned(),
                    span,
                });
            }
            return slot.value.map(CValue::Integer).ok_or_else(|| {
                InterpreterError::ReadOfUninitializedObject {
                    symbol,
                    spelling: spelling.to_owned(),
                    span,
                }
            });
        }
        let binding = self.require_memory_binding(symbol, span)?;
        let qualifiers = self
            .unit
            .types()
            .get(binding.ty)
            .ok_or(InterpreterError::UnsupportedType {
                ty: binding.ty,
                span,
            })?
            .qualifiers;
        self.load_memory_value(binding.address, binding.ty, qualifiers, span)
    }
}
