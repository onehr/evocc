use super::*;
use evocc_chir::complex::{ComplexExpr, ComplexOp};

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn complex_expression_type(&self, value: &CHirExpr<'_>) -> Option<TypeId> {
        match self.expression_type(value)?.ty {
            InferredType::Canonical(ty) if evocc_chir::complex::rank(&self.types, ty).is_some() => {
                Some(ty)
            }
            InferredType::Complex(ComplexBase::Integer(rank)) => self.types.find(&CanonicalType {
                kind: TypeKind::Complex(ComplexBase::Integer(rank)),
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
    /// integer operand is promoted (EvoIR 10.1.14).
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
    /// operand component types.
    fn complex_common_type(&mut self, left: &CHirExpr<'_>, right: &CHirExpr<'_>) -> Option<TypeId> {
        let a = self.complex_operand_component(left)?;
        let b = self.complex_operand_component(right)?;
        let rank = rank_for_integer_type(usual_arithmetic_type(a, b))?;
        Some(self.intern(
            TypeKind::Complex(ComplexBase::Integer(rank)),
            Qualifiers::default(),
        ))
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
        let operands = vec![
            self.complex_conversion(ty, left, span),
            self.complex_conversion(ty, right, span),
        ];
        Some(self.complex_node(ComplexOp::Binary(op), operands, result, span))
    }

    /// Prepare the value of `target op= value` when either side is integer
    /// complex. The value converts to the common complex type, so a real
    /// operand is promoted and both sides share one component type. A real
    /// target must be an integer; it keeps the converted result.
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
        Some(self.complex_conversion(ty, value, span))
    }

    pub(super) fn complex_component(
        &mut self,
        operand: &'arena CHirExpr<'arena>,
        imaginary: bool,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let Some(ty) = self.complex_expression_type(operand) else {
            let OperandTypeClass::Integer(ty) =
                self.operand_type_class(self.expression_type(operand)?.ty)
            else {
                return None;
            };
            let ty = self.intern_c_integer_type(ty);
            return Some(self.complex_node(
                ComplexOp::Component { imaginary },
                vec![operand],
                ty,
                span,
            ));
        };
        let rank = evocc_chir::complex::rank(&self.types, ty)?;
        let qualifiers = self.types.get(ty)?.qualifiers;
        let component = self.intern(TypeKind::Int(rank), qualifiers);
        if self.expression_type(operand)?.category != ExprCategory::Value
            || matches!(operand, CHirExpr::MemoryRead { .. })
        {
            // The two components have the ordinary adjacent-element object
            // representation. Project its address; do not read the other half.
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
            let place = self.dereference_place(address, span)?;
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
}
