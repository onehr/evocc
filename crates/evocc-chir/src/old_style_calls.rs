//! Calls without a prototype of an old-style definition (C11 6.5.2.2p6,
//! EvoIR 10.1.7).
//!
//! The unit defines the function with an identifier list or an empty
//! parameter list, and no prototype of it is in scope at the call. The front
//! end applies the default argument promotions to each argument and checks
//! no argument type against a parameter type. This module compares each
//! promoted argument type with the promoted type of the declared parameter.
//! The substitution pass, the lowering and the C-HIR interpreter use this
//! one verdict.

use evocc_types::{FunctionParameters, IntRank, TargetDataLayout, TypeId, TypeInterner, TypeKind};

use crate::{CHirExpr, CHirUnit, ExprCategory};

/// The verdict for one promoted argument type and one promoted parameter type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agreement {
    /// The types are compatible, or both are pointers to a character type or
    /// `void`, in any qualification.
    Agrees,
    /// A signed integer type and the corresponding unsigned type. The pair
    /// agrees when the value is representable in both types, that is when
    /// the top bit of the value is clear.
    IfRepresentable,
    /// Every other pair.
    Undefined,
}

/// The verdict for one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every argument agrees.
    Agrees,
    /// Every other argument agrees. The call is defined only when the top bit
    /// of each argument at these indices is clear.
    IfRepresentable(Vec<usize>),
    /// The argument count differs, or a pair does not agree.
    Undefined,
}

/// Compare one promoted argument type with one promoted parameter type.
#[must_use]
pub fn agreement(types: &TypeInterner, argument: TypeId, parameter: TypeId) -> Agreement {
    if types.compatible_ignoring_top_qualifiers(argument, parameter)
        || (character_or_void_pointer(types, argument)
            && character_or_void_pointer(types, parameter))
    {
        return Agreement::Agrees;
    }
    let rank = |ty| match types.get(ty).map(|ty| &ty.kind) {
        Some(TypeKind::Int(rank)) => Some(Ok(*rank)),
        Some(TypeKind::GnuInt128(signedness)) => Some(Err(*signedness)),
        _ => None,
    };
    let corresponding = match (rank(argument), rank(parameter)) {
        (Some(Ok(left)), Some(Ok(right))) => matches!(
            (left, right),
            (IntRank::Int, IntRank::UInt)
                | (IntRank::UInt, IntRank::Int)
                | (IntRank::Long, IntRank::ULong)
                | (IntRank::ULong, IntRank::Long)
                | (IntRank::LongLong, IntRank::ULongLong)
                | (IntRank::ULongLong, IntRank::LongLong)
        ),
        (Some(Err(left)), Some(Err(right))) => left != right,
        _ => false,
    };
    if corresponding {
        Agreement::IfRepresentable
    } else {
        Agreement::Undefined
    }
}

/// A pointer to a qualified or unqualified character type or `void`. An
/// atomic target does not count (C11 6.2.5p27).
fn character_or_void_pointer(types: &TypeInterner, ty: TypeId) -> bool {
    let Some(TypeKind::Pointer(target)) = types.get(ty).map(|ty| &ty.kind) else {
        return false;
    };
    types.get(*target).is_some_and(|target| {
        !target.qualifiers.is_atomic
            && matches!(
                target.kind,
                TypeKind::Void
                    | TypeKind::Int(
                        IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar
                    )
            )
    })
}

/// The promoted parameter types of the old-style definition that a call
/// without a prototype names, or `None` for any other call.
#[must_use]
pub fn old_style_callee<'unit>(
    unit: &'unit CHirUnit<'_>,
    callee: &CHirExpr<'_>,
) -> Option<&'unit [TypeId]> {
    let CHirExpr::Ident { symbol, facts, .. } = callee else {
        return None;
    };
    let unprototyped = matches!(
        unit.types().get(facts.ty()).map(|ty| &ty.kind),
        Some(TypeKind::Function {
            parameters: FunctionParameters::Unspecified,
            ..
        })
    );
    if facts.category() != ExprCategory::FunctionDesignator || !unprototyped {
        return None;
    }
    unit.old_style_parameters(*symbol)
}

/// The verdict for a call without a prototype of an old-style definition,
/// or `None` for any other call. Each argument is the promotion that the
/// front end wrote (`Cast` to the promoted type). A constant argument of a
/// signed/unsigned pair agrees when its top bit is clear.
#[must_use]
pub fn call_verdict(
    unit: &CHirUnit<'_>,
    target: &TargetDataLayout,
    callee: &CHirExpr<'_>,
    arguments: &[&CHirExpr<'_>],
) -> Option<Verdict> {
    let parameters = old_style_callee(unit, callee)?;
    let promoted = arguments
        .iter()
        .map(|argument| match argument {
            CHirExpr::Cast(ty, ..) => Some(*ty),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    if promoted.len() != parameters.len() {
        return Some(Verdict::Undefined);
    }
    let mut checked = Vec::new();
    for (index, (argument, parameter)) in promoted.iter().zip(parameters).enumerate() {
        match agreement(unit.types(), *argument, *parameter) {
            Agreement::Agrees => {}
            Agreement::IfRepresentable => {
                let constant = crate::constant::eval_integer_constant(
                    arguments[index],
                    unit.types(),
                    unit.symbols(),
                    target,
                );
                if !constant.is_some_and(|value| !top_bit_set(value)) {
                    checked.push(index);
                }
            }
            Agreement::Undefined => return Some(Verdict::Undefined),
        }
    }
    Some(if checked.is_empty() {
        Verdict::Agrees
    } else {
        Verdict::IfRepresentable(checked)
    })
}

/// Whether the top bit of an integer value is set: the value is then not
/// representable in both types of a signed/unsigned pair.
#[must_use]
pub fn top_bit_set(value: crate::integer::CInteger) -> bool {
    let bits = u32::from(value.ty().bits());
    bits > 0 && (value.raw_bits() >> (bits - 1)) & 1 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_types::{CanonicalType, Qualifiers};

    fn intern(types: &mut TypeInterner, kind: TypeKind, qualifiers: Qualifiers) -> TypeId {
        types.intern(CanonicalType { kind, qualifiers })
    }

    #[test]
    fn the_two_exceptions_and_the_undefined_pairs() {
        let mut types = TypeInterner::new();
        let plain = Qualifiers::default();
        let constant = Qualifiers {
            is_const: true,
            ..Qualifiers::default()
        };
        let atomic = Qualifiers {
            is_atomic: true,
            ..Qualifiers::default()
        };
        let int = intern(&mut types, TypeKind::Int(IntRank::Int), plain);
        let uint = intern(&mut types, TypeKind::Int(IntRank::UInt), plain);
        let long = intern(&mut types, TypeKind::Int(IntRank::Long), plain);
        let ulong_long = intern(&mut types, TypeKind::Int(IntRank::ULongLong), plain);
        let char_ty = intern(&mut types, TypeKind::Int(IntRank::Char), plain);
        let const_uchar = intern(&mut types, TypeKind::Int(IntRank::UChar), constant);
        let atomic_char = intern(&mut types, TypeKind::Int(IntRank::Char), atomic);
        let void = intern(&mut types, TypeKind::Void, plain);
        let const_int = intern(&mut types, TypeKind::Int(IntRank::Int), constant);
        let char_pointer = intern(&mut types, TypeKind::Pointer(char_ty), plain);
        let const_uchar_pointer = intern(&mut types, TypeKind::Pointer(const_uchar), plain);
        let atomic_char_pointer = intern(&mut types, TypeKind::Pointer(atomic_char), plain);
        let void_pointer = intern(&mut types, TypeKind::Pointer(void), plain);
        let int_pointer = intern(&mut types, TypeKind::Pointer(int), plain);
        let const_int_pointer = intern(&mut types, TypeKind::Pointer(const_int), plain);

        assert_eq!(agreement(&types, int, int), Agreement::Agrees);
        assert_eq!(agreement(&types, int, uint), Agreement::IfRepresentable);
        assert_eq!(agreement(&types, uint, int), Agreement::IfRepresentable);
        assert_eq!(agreement(&types, long, int), Agreement::Undefined);
        assert_eq!(agreement(&types, long, ulong_long), Agreement::Undefined);
        assert_eq!(
            agreement(&types, char_pointer, void_pointer),
            Agreement::Agrees
        );
        assert_eq!(
            agreement(&types, const_uchar_pointer, char_pointer),
            Agreement::Agrees
        );
        assert_eq!(
            agreement(&types, atomic_char_pointer, char_pointer),
            Agreement::Undefined
        );
        assert_eq!(
            agreement(&types, int_pointer, const_int_pointer),
            Agreement::Undefined
        );
        assert_eq!(
            agreement(&types, int_pointer, void_pointer),
            Agreement::Undefined
        );
    }
}
