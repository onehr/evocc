//! `__builtin_tgmath`, as GCC 14 documents and implements it (EvoIR 10.1.14).
use super::*;

/// How one parameter position, or the return type, varies between the
/// listed functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Variation {
    Fixed,
    Real,
    Complex,
}

/// One listed function: its return type and parameter types, without
/// qualifiers.
struct Candidate {
    kinds: Vec<TypeKind>,
    parameters: Vec<TypeId>,
}

fn complex_real(kind: &TypeKind) -> Option<FloatKind> {
    match kind {
        TypeKind::Complex(ComplexBase::CF16) => Some(FloatKind::F16),
        TypeKind::Complex(ComplexBase::CF32) => Some(FloatKind::F32),
        TypeKind::Complex(ComplexBase::CF64) => Some(FloatKind::F64),
        TypeKind::Complex(ComplexBase::CF128) => Some(FloatKind::F128),
        _ => None,
    }
}

fn floating(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Float(_)) || complex_real(kind).is_some()
}

/// The real type of a real or complex floating type.
fn real_part(kind: &TypeKind) -> TypeKind {
    complex_real(kind).map_or_else(|| kind.clone(), TypeKind::Float)
}

/// The C rank of a real floating type, which orders the usual arithmetic
/// conversions.
fn floating_rank(kind: FloatKind) -> u8 {
    match kind {
        FloatKind::F16 => 0,
        FloatKind::F32 => 1,
        FloatKind::F64 => 2,
        FloatKind::F128 => 3,
    }
}

impl<'src, 'arena> Parser<'src, 'arena> {
    /// The return type and parameter types of a pointer to a prototyped
    /// function with fixed parameters, as `check_tgmath_function` of GCC 14
    /// requires.
    fn tgmath_candidate(&self, operand: &CHirExpr<'_>) -> Option<Candidate> {
        let inferred = self.converted_expression_type(operand)?;
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(inferred.ty) else {
            return None;
        };
        let TypeKind::Function {
            return_type,
            parameters:
                FunctionParameters::Prototype {
                    types,
                    variadic: false,
                },
        } = &self.types.get(pointer.pointee)?.kind
        else {
            return None;
        };
        if types.is_empty() {
            return None;
        }
        let kinds = std::iter::once(*return_type)
            .chain(types.iter().copied())
            .map(|ty| self.types.get(ty).map(|ty| ty.kind.clone()))
            .collect::<Option<Vec<_>>>()?;
        Some(Candidate {
            kinds,
            parameters: types.clone(),
        })
    }

    /// `__builtin_tgmath(functions..., arguments...)`. The parameter count of
    /// the first function says how many trailing operands are arguments.
    /// The call goes to the function that the `<tgmath.h>` rules select, and
    /// each argument is evaluated once, as an ordinary call evaluates it.
    pub(super) fn resolve_tgmath(
        &mut self,
        operands: Vec<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if operands.len() < 3 {
            self.report("E2909", "builtin-arity", span);
            return None;
        }
        let Some(first) = self.tgmath_candidate(operands[0]) else {
            self.report("E2951", "builtin-argument-type", operands[0].span());
            return None;
        };
        let count = first.parameters.len();
        if operands.len() < count + 2 {
            self.report("E2909", "builtin-arity", span);
            return None;
        }
        let functions = operands.len() - count;
        let mut candidates = vec![first];
        for operand in &operands[1..functions] {
            match self.tgmath_candidate(operand) {
                Some(candidate) if candidate.parameters.len() == count => {
                    candidates.push(candidate)
                }
                _ => {
                    self.report("E2951", "builtin-argument-type", operand.span());
                    return None;
                }
            }
        }
        // Position 0 is the return type; positions 1 to `count` are the
        // parameters. A position whose types differ must hold real or
        // complex floating types, and it is complex when any of them is.
        let mut variation = Vec::with_capacity(count + 1);
        for position in 0..=count {
            let first = &candidates[0].kinds[position];
            let varies = candidates.iter().any(|c| &c.kinds[position] != first);
            if !varies {
                variation.push(Variation::Fixed);
                continue;
            }
            if !candidates.iter().all(|c| floating(&c.kinds[position])) {
                self.report("E2951", "builtin-argument-type", operands[0].span());
                return None;
            }
            variation.push(
                if candidates
                    .iter()
                    .any(|c| complex_real(&c.kinds[position]).is_some())
                {
                    Variation::Complex
                } else {
                    Variation::Real
                },
            );
        }
        let widest = variation.iter().copied().max().unwrap_or(Variation::Fixed);
        let Some(generic) = (1..=count).find(|&position| variation[position] == widest) else {
            self.report("E2951", "builtin-argument-type", span);
            return None;
        };
        if widest == Variation::Fixed {
            self.report("E2951", "builtin-argument-type", span);
            return None;
        }
        // The type-generic parameter types are distinct, and every other
        // varying position follows them as GCC 14 checks.
        let generic_types = candidates
            .iter()
            .map(|c| c.kinds[generic].clone())
            .collect::<Vec<_>>();
        for (index, ty) in generic_types.iter().enumerate() {
            if generic_types[..index].contains(ty) {
                self.report("E2951", "builtin-argument-type", operands[index].span());
                return None;
            }
        }
        for (index, candidate) in candidates.iter().enumerate() {
            let expected = &generic_types[index];
            let expected_real = real_part(expected);
            for (position, kind) in candidate.kinds.iter().enumerate() {
                let consistent = match variation[position] {
                    Variation::Fixed => true,
                    Variation::Complex => kind == expected,
                    Variation::Real => kind == &expected_real,
                };
                if !consistent {
                    self.report("E2951", "builtin-argument-type", operands[index].span());
                    return None;
                }
            }
        }
        // Combine the type-generic arguments: an integer counts as `double`,
        // the usual arithmetic conversions apply, and a complex argument or a
        // complex-only list asks for a complex type.
        let arguments = &operands[functions..];
        let all_complex = generic_types.iter().all(|ty| complex_real(ty).is_some());
        let mut complex = all_complex;
        let mut combined: Option<FloatKind> = None;
        for (index, argument) in arguments.iter().enumerate() {
            if variation[index + 1] == Variation::Fixed {
                continue;
            }
            let Some(inferred) = self.converted_expression_type(argument) else {
                self.report("E2951", "builtin-argument-type", argument.span());
                return None;
            };
            let real = match self.operand_type_class(inferred.ty) {
                OperandTypeClass::Integer(_) | OperandTypeClass::WideInteger { .. } => {
                    FloatKind::F64
                }
                OperandTypeClass::Real(kind) if kind != FloatKind::F16 => kind,
                OperandTypeClass::Complex(base) => {
                    complex = true;
                    match complex_real(&TypeKind::Complex(base)) {
                        Some(kind) if kind != FloatKind::F16 => kind,
                        // A complex integer counts as `_Complex double`.
                        None => FloatKind::F64,
                        Some(_) => {
                            self.report("E2951", "builtin-argument-type", argument.span());
                            return None;
                        }
                    }
                }
                _ => {
                    self.report("E2951", "builtin-argument-type", argument.span());
                    return None;
                }
            };
            combined = Some(match combined {
                Some(previous) if floating_rank(previous) >= floating_rank(real) => previous,
                _ => real,
            });
        }
        let real = combined.expect("a type-generic position has an argument");
        let wanted = if complex {
            TypeKind::Complex(match real {
                FloatKind::F32 => ComplexBase::CF32,
                FloatKind::F64 => ComplexBase::CF64,
                FloatKind::F16 => ComplexBase::CF16,
                FloatKind::F128 => ComplexBase::CF128,
            })
        } else {
            TypeKind::Float(real)
        };
        let mut selected = generic_types.iter().position(|ty| ty == &wanted);
        // A macro that rounds its result to a narrower type has a fixed
        // floating return type. It takes the first function whose format has
        // at least the range and precision of the argument type.
        if selected.is_none()
            && variation[0] == Variation::Fixed
            && matches!(candidates[0].kinds[0], TypeKind::Float(_))
        {
            let target = self.target_layout;
            let format = |kind: FloatKind| floating_rank(target.float_format(kind));
            selected = generic_types.iter().position(|ty| {
                complex_real(ty).is_some() == complex
                    && match real_part(ty) {
                        TypeKind::Float(kind) => format(kind) >= format(real),
                        _ => false,
                    }
            });
        }
        let Some(selected) = selected else {
            self.report("E2951", "builtin-argument-type", span);
            return None;
        };
        if complex_real(&generic_types[selected]).is_some() {
            self.report("E2959", "unsupported-complex-type-generic-call", span);
            return None;
        }
        let callee = operands[selected];
        let mut arguments = arguments.to_vec();
        for (argument, parameter) in arguments
            .iter_mut()
            .zip(candidates[selected].parameters.clone())
        {
            *argument = self.resolve_union_argument(parameter, argument)?;
            if self.assignment_constraint(AssignmentOp::Simple, parameter, argument)
                == UpdateConstraint::Invalid
            {
                self.report("E2202", "invalid-initializer-type", argument.span());
                return None;
            }
            *argument = self.explicit_floating_integer_conversion(parameter, argument);
        }
        if let Some(name) = self.standard_floating_builtin(callee) {
            return self.resolve_floating_builtin(name, arguments, span);
        }
        Some(self.alloc_node(CHirExpr::Call(callee, arguments, span)))
    }
}
