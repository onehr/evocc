//! C-HIR interpreter runtime values.

use super::*;

pub(super) fn pop_machine_value(
    values: &mut Vec<CValue>,
    span: SourceSpan,
) -> Result<CValue, InterpreterError> {
    values.pop().ok_or(InterpreterError::UnsupportedExpression {
        feature: "malformed interpreter expression continuation",
        span,
    })
}

/// Truth of a controlling expression.
///
/// C11 6.5.13 and the statement forms compare the operand against zero, and a
/// pointer compares against the null pointer, so a pointer condition is well
/// formed rather than an integer-only operation.
pub(super) fn machine_truth(value: CValue, _span: SourceSpan) -> Result<bool, InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span: _span }),
        CValue::Integer(value) => Ok(value.truth()),
        CValue::Floating(value) => Ok(!value.is_zero()),
        CValue::Pointer { value, .. } => Ok(value != Pointer::NULL),
        CValue::FunctionPointer { value, .. } => Ok(value != PointerValue::Null),
        CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "aggregate controlling expression",
            span: _span,
        }),
    }
}

pub(super) fn machine_equality_result(operator: BinaryOp, equal: bool) -> CValue {
    CValue::Integer(CInteger::int(i32::from(if operator == BinaryOp::Eq {
        equal
    } else {
        !equal
    })))
}

pub(super) fn expect_machine_integer(
    value: CValue,
    span: SourceSpan,
) -> Result<CInteger, InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        CValue::Integer(value) => Ok(value),
        CValue::Floating(_)
        | CValue::Pointer { .. }
        | CValue::FunctionPointer { .. }
        | CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "pointer value in integer task-machine operation",
            span,
        }),
    }
}

pub(super) fn expect_machine_pointer(
    value: CValue,
    span: SourceSpan,
) -> Result<(Pointer, PointerTypeDesc), InterpreterError> {
    match value {
        CValue::Uninitialized(_) => Err(InterpreterError::UninitializedValue { span }),
        CValue::Pointer { value, ty } => Ok((value, ty)),
        CValue::Floating(_)
        | CValue::Integer(_)
        | CValue::FunctionPointer { .. }
        | CValue::Aggregate { .. } => Err(InterpreterError::UnsupportedExpression {
            feature: "integer value in pointer task-machine operation",
            span,
        }),
    }
}

pub(super) fn cast_machine_value(
    value: CValue,
    target: ValueType,
    span: SourceSpan,
    types: &TypeInterner,
    memory: &mut MemoryState,
) -> Result<CValue, InterpreterError> {
    match (value, target) {
        (CValue::Uninitialized(_), _) => Err(InterpreterError::UninitializedValue { span }),
        (value, ValueType::Floating(kind)) => {
            floating::convert(value, kind, span).map(CValue::Floating)
        }
        (CValue::Floating(value), ValueType::Integer(target)) => {
            let raw = if target == CIntegerType::BOOL {
                u128::from(!value.is_zero())
            } else {
                value
                    .to_integer(u16::from(target.bits()), target.is_signed())
                    .ok_or(InterpreterError::FloatingToInteger { span })?
            };
            Ok(CValue::Integer(CInteger::from_raw(raw, target)))
        }
        (CValue::Floating(_), ValueType::Pointer(_) | ValueType::Aggregate(_)) => {
            Err(InterpreterError::UnsupportedExpression {
                feature: "invalid floating conversion",
                span,
            })
        }
        (CValue::Pointer { value, .. }, ValueType::Integer(target)) => {
            if target == CIntegerType::BOOL {
                return Ok(CValue::Integer(CInteger::from_raw(
                    u128::from(value != Pointer::NULL),
                    target,
                )));
            }
            let (next, address) = memory
                .expose_pointer(PointerValue::object(value))
                .map_err(|error| InterpreterError::Memory { error, span })?;
            *memory = next;
            Ok(CValue::Integer(
                CInteger::from_raw(u128::from(address), CIntegerType::ULONG).cast(target),
            ))
        }
        (CValue::FunctionPointer { value, .. }, ValueType::Integer(target)) => {
            if target == CIntegerType::BOOL {
                return Ok(CValue::Integer(CInteger::from_raw(
                    u128::from(value != PointerValue::Null),
                    target,
                )));
            }
            let (next, address) = memory
                .expose_pointer(value)
                .map_err(|error| InterpreterError::Memory { error, span })?;
            *memory = next;
            Ok(CValue::Integer(
                CInteger::from_raw(u128::from(address), CIntegerType::ULONG).cast(target),
            ))
        }
        (CValue::Integer(value), ValueType::Integer(target)) => {
            Ok(CValue::Integer(value.cast(target)))
        }
        (value @ CValue::Pointer { .. }, ValueType::Pointer(target))
            if pointer_type_is_function(types, target) =>
        {
            let address = cast_machine_value(
                value,
                ValueType::Integer(CIntegerType::ULONG),
                span,
                types,
                memory,
            )?;
            cast_machine_value(address, ValueType::Pointer(target), span, types, memory)
        }
        (value @ CValue::FunctionPointer { .. }, ValueType::Pointer(target))
            if !pointer_type_is_function(types, target) =>
        {
            let address = cast_machine_value(
                value,
                ValueType::Integer(CIntegerType::ULONG),
                span,
                types,
                memory,
            )?;
            cast_machine_value(address, ValueType::Pointer(target), span, types, memory)
        }
        (CValue::Pointer { value, ty }, ValueType::Pointer(target)) if ty == target => {
            Ok(CValue::Pointer { value, ty: target })
        }
        (CValue::Pointer { value, .. }, ValueType::Pointer(target))
            if value == Pointer::NULL && pointer_type_is_function(types, target) =>
        {
            Ok(CValue::FunctionPointer {
                value: PointerValue::Null,
                ty: target,
            })
        }
        (CValue::Pointer { value, ty }, ValueType::Pointer(target))
            if pointer_runtime_conversion(types, ty, target) =>
        {
            Ok(CValue::Pointer { value, ty: target })
        }
        (CValue::FunctionPointer { value, .. }, ValueType::Pointer(target))
            if pointer_type_is_function(types, target) =>
        {
            // A cast retags the interface; start_indirect_call checks it
            // against the function identity after evaluating the arguments.
            Ok(CValue::FunctionPointer { value, ty: target })
        }
        (
            CValue::Aggregate {
                address,
                ty,
                qualifiers,
            },
            ValueType::Aggregate(target),
        ) if crate::vector::assignment_compatible(types, ty, target)
            || types
                .get(ty)
                .zip(types.get(target))
                .is_some_and(|(source, target)| {
                    (matches!(
                        source.kind,
                        TypeKind::Struct(_)
                            | TypeKind::Union(_)
                            | TypeKind::BuiltinVaList
                            | TypeKind::Complex(evocc_types::ComplexBase::Integer(_))
                            | TypeKind::Vector { .. }
                    ) || matches!(source.kind, TypeKind::Complex(base) if base.executable_part().is_some()))
                        && source.kind == target.kind
                }) =>
        {
            Ok(CValue::Aggregate {
                address,
                ty: target,
                qualifiers,
            })
        }
        // C11 6.3.2.3p3: a null pointer constant converts to a null pointer of
        // the destination type. Whether the source really was a *constant* is
        // settled where the assignment was type checked; only the conversion
        // itself is executed here.
        (CValue::Integer(value), ValueType::Pointer(target)) if value.raw_bits() == 0 => {
            if pointer_type_is_function(types, target) {
                Ok(CValue::FunctionPointer {
                    value: PointerValue::Null,
                    ty: target,
                })
            } else {
                Ok(CValue::Pointer {
                    value: Pointer::NULL,
                    ty: target,
                })
            }
        }
        (CValue::Integer(value), ValueType::Pointer(target)) => {
            let address = value.cast(CIntegerType::ULONG).raw_bits() as u64;
            Ok(if pointer_type_is_function(types, target) {
                CValue::FunctionPointer {
                    value: PointerValue::Exposed(address),
                    ty: target,
                }
            } else {
                CValue::Pointer {
                    value: Pointer::from_address(address),
                    ty: target,
                }
            })
        }
        (CValue::Pointer { .. }, ValueType::Pointer(_))
        | (CValue::FunctionPointer { .. }, ValueType::Pointer(_))
        | (CValue::Aggregate { .. }, ValueType::Integer(_) | ValueType::Pointer(_))
        | (
            CValue::Integer(_) | CValue::Pointer { .. } | CValue::FunctionPointer { .. },
            ValueType::Aggregate(_),
        )
        | (CValue::Aggregate { .. }, ValueType::Aggregate(_)) => {
            Err(InterpreterError::UnsupportedExpression {
                feature: "conversion to or from an incompatible aggregate or pointer type",
                span,
            })
        }
        (_, ValueType::Void) => Err(InterpreterError::UnsupportedExpression {
            feature: "value cast to void in task-machine continuation",
            span,
        }),
    }
}

pub(super) fn pointer_type_is_function(types: &TypeInterner, pointer: PointerTypeDesc) -> bool {
    matches!(
        types.get(pointer.pointee()).map(|ty| &ty.kind),
        Some(TypeKind::Function { .. })
    )
}

pub(super) fn pointer_runtime_conversion(
    types: &TypeInterner,
    source: PointerTypeDesc,
    target: PointerTypeDesc,
) -> bool {
    let (Some(source_ty), Some(target_ty)) =
        (types.get(source.pointee()), types.get(target.pointee()))
    else {
        return false;
    };
    // The parser has already distinguished implicit qualification conversion
    // from an explicit cast. Runtime pointer values carry provenance, not C
    // qualifiers, so either source-level form preserves the same capability
    // when the unqualified pointee kind is compatible.
    source_ty.kind == target_ty.kind
        || (!matches!(source_ty.kind, TypeKind::Function { .. })
            && !matches!(target_ty.kind, TypeKind::Function { .. }))
}

pub(super) fn merge_memory_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
        is_atomic: left.is_atomic || right.is_atomic,
        aligned: match (left.aligned, right.aligned) {
            (Some(left), Some(right)) => Some(left.max(right)),
            (left, right) => left.or(right),
        },
    }
}

pub(super) fn assignment_binary(operator: AssignmentOp) -> Option<BinaryOp> {
    operator.binary_operation()
}

pub(super) fn function_parameters_match(
    interner: &TypeInterner,
    canonical: &FunctionParameters,
    written: &ParameterSyntax,
) -> bool {
    match (canonical, written) {
        (FunctionParameters::Unspecified, ParameterSyntax::Unspecified) => true,
        (FunctionParameters::Prototype { types, variadic }, syntax)
            if !matches!(syntax, ParameterSyntax::Unspecified) =>
        {
            let parameters = syntax.parameters();
            *variadic == matches!(syntax, ParameterSyntax::Prototype { variadic: true, .. })
                && types.len() == parameters.len()
                && types.iter().zip(parameters).all(|(canonical, parameter)| {
                    syntax.parameter_type_matches(interner, *canonical, parameter.ty)
                })
        }
        _ => false,
    }
}

pub(super) fn function_parameters_compatible(
    types: &TypeInterner,
    left: &FunctionParameters,
    right: &FunctionParameters,
) -> bool {
    match (left, right) {
        (FunctionParameters::Unspecified, FunctionParameters::Unspecified) => true,
        (
            FunctionParameters::Prototype {
                types: left_types,
                variadic: left_variadic,
            },
            FunctionParameters::Prototype {
                types: right_types,
                variadic: right_variadic,
            },
        ) => {
            left_variadic == right_variadic
                && left_types.len() == right_types.len()
                && left_types
                    .iter()
                    .zip(right_types)
                    .all(|(left, right)| types.compatible_ignoring_top_qualifiers(*left, *right))
        }
        (
            FunctionParameters::Unspecified,
            FunctionParameters::Prototype {
                types: prototype,
                variadic,
            },
        )
        | (
            FunctionParameters::Prototype {
                types: prototype,
                variadic,
            },
            FunctionParameters::Unspecified,
        ) => {
            !variadic
                && prototype
                    .iter()
                    .all(|ty| default_argument_promotion_preserves_type(types, *ty))
        }
    }
}

pub(super) fn default_argument_promotion_preserves_type(types: &TypeInterner, ty: TypeId) -> bool {
    let Some(canonical) = types.get(ty) else {
        return false;
    };
    match canonical.kind {
        TypeKind::Int(_) | TypeKind::Enum(_) => canonical_integer_type(types, ty)
            .is_some_and(|integer| CInteger::new(0, integer).promoted().ty() == integer),
        TypeKind::Float(kind) => matches!(
            kind,
            evocc_types::FloatKind::F64 | evocc_types::FloatKind::F128
        ),
        TypeKind::Void
        | TypeKind::TargetBuiltin(_)
        | TypeKind::Array { .. }
        | TypeKind::Function { .. } => false,
        TypeKind::BuiltinVaList
        | TypeKind::GnuInt128(_)
        | TypeKind::Vector { .. }
        | TypeKind::Complex(_)
        | TypeKind::Pointer(_)
        | TypeKind::Struct(_)
        | TypeKind::Union(_) => true,
    }
}

pub(super) fn function_storage_compatible(entity: StorageClass, written: StorageClass) -> bool {
    match entity {
        StorageClass::Static => matches!(
            written,
            StorageClass::None | StorageClass::Extern | StorageClass::Static
        ),
        StorageClass::None | StorageClass::Extern => {
            matches!(written, StorageClass::None | StorageClass::Extern)
        }
        StorageClass::Typedef
        | StorageClass::Auto
        | StorageClass::Register
        | StorageClass::ThreadLocal
        | StorageClass::StaticThreadLocal
        | StorageClass::ExternThreadLocal => false,
    }
}

pub(super) fn canonical_integer_type(types: &TypeInterner, ty: TypeId) -> Option<CIntegerType> {
    let canonical = types.get(ty)?;
    match canonical.kind {
        TypeKind::TargetBuiltin(kind) => crate::integer::integer_type_for_target(kind),
        TypeKind::Int(rank) => Some(integer_type_for_rank(rank)),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Signed) => Some(CIntegerType::INT128),
        TypeKind::GnuInt128(evocc_types::GnuInt128Signedness::Unsigned) => {
            Some(CIntegerType::UINT128)
        }
        TypeKind::Enum(tag) => types.enum_representation(tag).map(integer_type_for_rank),
        _ => None,
    }
}

pub(super) fn integer_storage_size(ty: CIntegerType) -> u64 {
    u64::from(ty.bits()).div_ceil(8).max(1)
}
