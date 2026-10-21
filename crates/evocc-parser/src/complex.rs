use super::*;
use evocc_chir::complex::{ComplexExpr, ComplexOp};

impl<'src, 'arena> Parser<'src, 'arena> {
    /// The type of an integer or floating complex value whose operations
    /// are admitted (EvoIR 10.1.14). `_FloatN _Complex` and half complex
    /// types are not among them.
    pub(super) fn complex_expression_type(&self, value: &CHirExpr<'_>) -> Option<TypeId> {
        match self.expression_type(value)?.ty {
            InferredType::Canonical(ty) if evocc_chir::complex::admitted(&self.types, ty) => {
                Some(ty)
            }
            InferredType::Complex(
                base @ (ComplexBase::Integer(_)
                | ComplexBase::CF32
                | ComplexBase::CF64
                | ComplexBase::CF128),
            ) => self.types.find(&CanonicalType {
                kind: TypeKind::Complex(base),
                qualifiers: Qualifiers::default(),
            }),
            _ => None,
        }
    }

    pub(super) fn complex_node(
        &self,
        op: ComplexOp,
        operands: Vec<&'arena CHirExpr<'arena>>,
        ty: TypeId,
        span: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        self.alloc_node(CHirExpr::Complex(ComplexExpr {
            op,
            operands,
            facts: ExprFacts::value(ty),
            span,
        }))
    }

    pub(super) fn complex_conversion(
        &self,
        ty: TypeId,
        operand: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        self.complex_node(ComplexOp::Convert, vec![operand], ty, span)
    }

    /// The component type that an operand contributes to integer-complex
    /// arithmetic: a complex operand keeps its component type, and a real
    /// integer operand is promoted (EvoIR 10.1.14). `None` when the
    /// operation is not integer complex.
    fn complex_operand_component(&self, value: &CHirExpr<'_>) -> Option<CIntegerType> {
        if let Some(ty) = self.complex_expression_type(value) {
            Some(integer_type_for_rank(evocc_chir::complex::rank(
                &self.types,
                ty,
            )?))
        } else {
            match self.operand_type_class(self.converted_expression_type(value)?.ty) {
                OperandTypeClass::Integer(ty) => Some(CInteger::new(0, ty).promoted().ty()),
                _ => None,
            }
        }
    }

    /// `_Complex U`, where `U` is the usual arithmetic conversion of the two
    /// operand component types. With a floating operand, `U` is the floating
    /// type of the usual arithmetic conversions (C11 6.3.1.8).
    fn complex_common_type(&mut self, left: &CHirExpr<'_>, right: &CHirExpr<'_>) -> Option<TypeId> {
        if let (Some(a), Some(b)) = (
            self.complex_operand_component(left),
            self.complex_operand_component(right),
        ) {
            let rank = rank_for_integer_type(usual_arithmetic_type(a, b))?;
            return Some(self.intern(
                TypeKind::Complex(ComplexBase::Integer(rank)),
                Qualifiers::default(),
            ));
        }
        let left = self.converted_expression_type(left)?.ty;
        let right = self.converted_expression_type(right)?.ty;
        let InferredType::Complex(base) = self.usual_arithmetic_expression_type(left, right)?
        else {
            return None;
        };
        base.executable_part()?;
        Some(self.intern(TypeKind::Complex(base), Qualifiers::default()))
    }

    /// The real type of the parts of the floating complex type `ty`.
    pub(super) fn complex_part_type(&mut self, ty: TypeId) -> Option<TypeId> {
        let TypeKind::Complex(base) = self.types.get(ty)?.kind else {
            return None;
        };
        let kind = base.executable_part()?;
        Some(self.intern(TypeKind::Float(kind), Qualifiers::default()))
    }

    /// A real operand of mixed real and complex arithmetic converts to the
    /// part type and stays real (C11 6.3.1.8, G.5.1).
    fn complex_real_operand(
        &mut self,
        part: TypeId,
        operand: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        let part_kind = match self.types.get(part).map(|ty| &ty.kind) {
            Some(TypeKind::Float(kind)) => Some(*kind),
            _ => None,
        };
        let same = match self
            .converted_expression_type(operand)
            .map(|inferred| inferred.ty)
        {
            Some(InferredType::Canonical(ty)) => self
                .types
                .get(ty)
                .is_some_and(|ty| Some(&ty.kind) == part_kind.map(TypeKind::Float).as_ref()),
            Some(InferredType::Real(kind)) => Some(kind) == part_kind,
            _ => false,
        };
        if same {
            operand
        } else {
            self.alloc_node(CHirExpr::Cast(part, operand, operand.span()))
        }
    }

    pub(super) fn complex_binary(
        &mut self,
        op: BinaryOp,
        left: &'arena CHirExpr<'arena>,
        right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            return Some(self.alloc_node(CHirExpr::Binary(op, left, right, span)));
        }
        if !matches!(
            op,
            BinaryOp::Add
                | BinaryOp::Sub
                | BinaryOp::Mul
                | BinaryOp::Div
                | BinaryOp::Eq
                | BinaryOp::Ne
        ) {
            self.report("E2102", "invalid-complex-operands", span);
            return None;
        }
        let Some(ty) = self.complex_common_type(left, right) else {
            self.report("E2102", "invalid-complex-operands", span);
            return None;
        };
        let result = if matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
            self.intern_c_integer_type(CIntegerType::INT)
        } else {
            ty
        };
        let operands = match self.complex_part_type(ty) {
            // Floating: a real operand stays real, except a real dividend of
            // a complex divisor, which divides as (r, +0). Equality compares
            // a real operand as (r, +0) too.
            Some(part) if !matches!(op, BinaryOp::Eq | BinaryOp::Ne) => {
                let left_complex = self.complex_expression_type(left).is_some();
                let right_complex = self.complex_expression_type(right).is_some();
                let left = if left_complex || op == BinaryOp::Div {
                    self.complex_conversion(ty, left, span)
                } else {
                    self.complex_real_operand(part, left)
                };
                let right = if right_complex {
                    self.complex_conversion(ty, right, span)
                } else {
                    self.complex_real_operand(part, right)
                };
                vec![left, right]
            }
            _ => vec![
                self.complex_conversion(ty, left, span),
                self.complex_conversion(ty, right, span),
            ],
        };
        Some(self.complex_node(ComplexOp::Binary(op), operands, result, span))
    }

    /// Prepare the value of `target op= value` when either side is complex.
    /// For integer complex, the value converts to the common complex type,
    /// so a real operand is promoted and both sides share one component
    /// type; a real target must be an integer. For floating complex, a real
    /// value converts to the common part type and stays real, and a real
    /// target is any arithmetic object. A real target keeps the converted
    /// result.
    pub(super) fn complex_update_value(
        &mut self,
        op: AssignmentOp,
        target: &'arena CHirExpr<'arena>,
        value: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if !matches!(
            op,
            AssignmentOp::Add
                | AssignmentOp::Subtract
                | AssignmentOp::Multiply
                | AssignmentOp::Divide
        ) {
            // The ordinary update constraint reports these operators.
            return Some(value);
        }
        let Some(ty) = self.complex_common_type(target, value) else {
            self.report("E2102", "invalid-update-operands", span);
            return None;
        };
        if let Some(part) = self.complex_part_type(ty) {
            let target_complex = self.complex_expression_type(target).is_some();
            if self.complex_expression_type(value).is_none()
                && !(op == AssignmentOp::Divide && !target_complex)
            {
                return Some(self.complex_real_operand(part, value));
            }
        }
        Some(self.complex_conversion(ty, value, span))
    }

    pub(super) fn complex_component(
        &mut self,
        operand: &'arena CHirExpr<'arena>,
        imaginary: bool,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let Some(ty) = self.complex_expression_type(operand) else {
            // A real operand: `__real__` gives its value and `__imag__` a zero
            // of its type, as GCC gives them.
            let ty = match self.operand_type_class(self.expression_type(operand)?.ty) {
                OperandTypeClass::Integer(ty) => self.intern_c_integer_type(ty),
                OperandTypeClass::Real(
                    kind @ (FloatKind::F32 | FloatKind::F64 | FloatKind::F128),
                ) => self.intern(TypeKind::Float(kind), Qualifiers::default()),
                _ => {
                    self.report("E2102", "invalid-complex-operands", span);
                    return None;
                }
            };
            return Some(self.complex_node(
                ComplexOp::Component { imaginary },
                vec![operand],
                ty,
                span,
            ));
        };
        let qualifiers = self.types.get(ty)?.qualifiers;
        let component = match self.types.get(ty)?.kind {
            TypeKind::Complex(ComplexBase::Integer(rank)) => {
                self.intern(TypeKind::Int(rank), qualifiers)
            }
            TypeKind::Complex(base) => {
                self.intern(TypeKind::Float(base.executable_part()?), qualifiers)
            }
            _ => return None,
        };
        if self.expression_type(operand)?.category != ExprCategory::Value
            || matches!(operand, CHirExpr::MemoryRead { .. })
        {
            // The two components have the ordinary adjacent-element object
            // representation. Project its address; do not read the other half.
            let operand = self.address_operand(operand);
            let address = self.alloc_node(CHirExpr::Unary(UnaryOp::AddrOf, operand, span));
            let pointer = self.intern(TypeKind::Pointer(component), Qualifiers::default());
            let mut address = self.alloc_node(CHirExpr::Cast(pointer, address, span));
            if imaginary {
                let one = self.alloc_node(CHirExpr::Lit(
                    IntLit {
                        value: 1,
                        base: IntBase::Dec,
                        suffix: IntSuffix::None,
                    },
                    span,
                ));
                address = self.alloc_node(CHirExpr::Binary(BinaryOp::Add, address, one, span));
            }
            // A read place: a const part is readable, and an update of it
            // is refused where the update resolves its target.
            let part = self.alloc_node(CHirExpr::Unary(UnaryOp::Deref, address, span));
            let place = self.resolve_memory_place(part).or_else(|| {
                self.report("E2102", "invalid-projected-lvalue", span);
                None
            })?;
            return Some(self.alloc_node(CHirExpr::MemoryRead {
                facts: ExprFacts::value(component),
                place,
                target_span: span,
                span,
            }));
        }
        Some(self.complex_node(
            ComplexOp::Component { imaginary },
            vec![operand],
            component,
            span,
        ))
    }

    /// `__builtin_complex(x, y)`: two operands of one real floating type,
    /// after lvalue conversion and without promotion, as GCC requires. The
    /// parts are copied exactly; no arithmetic and no status is involved.
    pub(super) fn resolve_builtin_complex(
        &mut self,
        operands: Vec<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let [real, imaginary] = operands[..] else {
            self.report("E2909", "builtin-arity", span);
            return None;
        };
        let kind = |this: &Self, operand: &CHirExpr<'_>| match this
            .operand_type_class(this.converted_expression_type(operand)?.ty)
        {
            OperandTypeClass::Real(kind @ (FloatKind::F32 | FloatKind::F64 | FloatKind::F128)) => {
                Some(kind)
            }
            _ => None,
        };
        let (Some(first), Some(second)) = (kind(self, real), kind(self, imaginary)) else {
            let operand = if kind(self, real).is_none() {
                real
            } else {
                imaginary
            };
            self.report("E2951", "builtin-argument-type", operand.span());
            return None;
        };
        if first != second {
            self.report("E2951", "builtin-argument-type", imaginary.span());
            return None;
        }
        let ty = self.intern(
            TypeKind::Complex(complex_base_for_float(first)),
            Qualifiers::default(),
        );
        Some(self.complex_node(ComplexOp::Construct, vec![real, imaginary], ty, span))
    }

    /// `__builtin_creal`, `__builtin_cimag` and `__builtin_conj` and their
    /// `f` and `l` forms. The operand converts as the prototype converts it;
    /// a part projection converts only the projected part.
    pub(super) fn resolve_complex_builtin(
        &mut self,
        name: &str,
        operands: Vec<&'arena CHirExpr<'arena>>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let [operand] = operands[..] else {
            self.report("E2909", "builtin-arity", span);
            return None;
        };
        let kind = match name.as_bytes().last() {
            Some(b'f') => FloatKind::F32,
            Some(b'l') => FloatKind::F128,
            _ => FloatKind::F64,
        };
        let complex = self.intern(
            TypeKind::Complex(complex_base_for_float(kind)),
            Qualifiers::default(),
        );
        let class = self.operand_type_class(self.converted_expression_type(operand)?.ty);
        if !is_arithmetic_class(class) {
            self.report("E2951", "builtin-argument-type", operand.span());
            return None;
        }
        if name.starts_with("__builtin_conj") {
            let converted = self.complex_conversion(complex, operand, span);
            return Some(self.complex_node(
                ComplexOp::Unary(UnaryOp::BitNot),
                vec![converted],
                complex,
                span,
            ));
        }
        let imaginary = name.starts_with("__builtin_cimag");
        let part = self.complex_component(operand, imaginary, span)?;
        let real = self.intern(TypeKind::Float(kind), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::Cast(real, part, span)))
    }
}
