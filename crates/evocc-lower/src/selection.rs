//! Reuse integer operands that a conditional has already compared.
//!
//! This is deliberately not general load forwarding: both source trees must
//! name the same stable expressions, with no calls, writes, atomic/volatile
//! reads or activation-dependent operations anywhere in them. All original
//! comparison accesses and UB obligations execute before selection.

use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn reuse_compared_selection(
        &mut self,
        condition: &CHirExpr<'_>,
        when_true: &CHirExpr<'_>,
        when_false: &CHirExpr<'_>,
        predicate: &Value,
        state: &mut State,
    ) -> Result<Option<ExprValue>, LowerError> {
        if !self.optimize {
            return Ok(None);
        }
        let CHirExpr::Binary(op, left, right, _) = condition else {
            return Ok(None);
        };
        if !matches!(
            op,
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge | BinaryOp::Eq | BinaryOp::Ne
        ) {
            return Ok(None);
        }
        let reversed = if self.same_stable_expression(left, when_true, 32)
            && self.same_stable_expression(right, when_false, 32)
        {
            false
        } else if self.same_stable_expression(right, when_true, 32)
            && self.same_stable_expression(left, when_false, 32)
        {
            true
        } else {
            return Ok(None);
        };
        let (Some(left_type), Some(right_type)) = (
            self.integer_expression_type(left),
            self.integer_expression_type(right),
        ) else {
            return Ok(None);
        };
        let common = arithmetic_common_type(left_type, right_type);
        let Some((mut left_value, mut right_value)) = self
            .blocks
            .iter()
            .find(|block| block.id == state.block)
            .and_then(|block| {
                block.insts.iter().rev().find_map(|inst| match inst {
                    Inst::ICmp(result, _, left, right)
                        if result == predicate
                            && left.ty == bits(common)
                            && right.ty == bits(common) =>
                    {
                        Some((left.clone(), right.clone()))
                    }
                    _ => None,
                })
            })
        else {
            // A separately proven truth or another lowering shape does not
            // authorize guessing which values the predicate compared.
            return Ok(None);
        };
        if matches!(op, BinaryOp::Gt | BinaryOp::Ge) != reversed {
            std::mem::swap(&mut left_value, &mut right_value);
        }

        // Both values have already been semantically read by the comparison.
        // Select their bit patterns with b ^ ((a ^ b) & -zext(predicate)).
        // This is NOT speculative evaluation of a general conditional arm;
        // uninitialized operands still fail at the original comparison. The
        // internal subtraction is a total bit-vector operation, not new C
        // signed arithmetic, and is valid at every admitted integer width.
        let condition = self
            .cast(
                ExprValue {
                    value: predicate.clone(),
                    integer: CIntegerType::BOOL,
                },
                common,
                state,
            )?
            .value;
        let zero = self.constant(state, common, 0)?.value;
        let mask = self.emit_raw_binary(state, Inst::Sub, &zero, &condition)?;
        let different = self.emit_raw_binary(state, Inst::Xor, &left_value, &right_value)?;
        let selected = self.emit_raw_binary(state, Inst::And, &different, &mask)?;
        let result = self.emit_raw_binary(state, Inst::Xor, &right_value, &selected)?;
        Ok(Some(ExprValue {
            value: result,
            integer: common,
        }))
    }

    fn same_stable_expression(&self, a: &CHirExpr<'_>, b: &CHirExpr<'_>, depth: u8) -> bool {
        let Some(depth) = depth.checked_sub(1) else {
            return false;
        };
        match (a, b) {
            (CHirExpr::Lit(a, _), CHirExpr::Lit(b, _)) => integer_literal_value(*a)
                .ok()
                .is_some_and(|a| Some(a) == integer_literal_value(*b).ok()),
            (CHirExpr::CharLit(a, _), CHirExpr::CharLit(b, _)) => {
                a.integer_value() == b.integer_value()
            }
            (
                CHirExpr::Ident {
                    symbol: a,
                    facts: af,
                    ..
                },
                CHirExpr::Ident {
                    symbol: b,
                    facts: bf,
                    ..
                },
            ) => a == b && af.ty() == bf.ty() && self.stable_object(*a),
            (CHirExpr::MemoryRead { place: a, .. }, CHirExpr::MemoryRead { place: b, .. }) => {
                self.same_stable_place(a, b, depth)
            }
            (CHirExpr::Unary(ao, a, _), CHirExpr::Unary(bo, b, _)) if ao == bo => {
                let ordinary = if *ao == UnaryOp::Deref {
                    let pointer = |e| self.pointer_descriptor(e).ok();
                    match (pointer(a), pointer(b)) {
                        (Some(a), Some(b)) if a == b => {
                            let q = a.pointee_qualifiers();
                            !q.is_volatile && !q.is_atomic
                        }
                        _ => false,
                    }
                } else {
                    matches!(
                        ao,
                        UnaryOp::Plus
                            | UnaryOp::Neg
                            | UnaryOp::BitNot
                            | UnaryOp::Not
                            | UnaryOp::Abs
                    )
                };
                ordinary && self.same_stable_expression(a, b, depth)
            }
            (CHirExpr::Binary(ao, al, ar, _), CHirExpr::Binary(bo, bl, br, _)) => {
                ao == bo
                    && matches!(
                        ao,
                        BinaryOp::Add
                            | BinaryOp::Sub
                            | BinaryOp::Mul
                            | BinaryOp::Div
                            | BinaryOp::Mod
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                            | BinaryOp::Shl
                            | BinaryOp::Shr
                            | BinaryOp::Eq
                            | BinaryOp::Ne
                            | BinaryOp::Lt
                            | BinaryOp::Le
                            | BinaryOp::Gt
                            | BinaryOp::Ge
                    )
                    && self.same_stable_expression(al, bl, depth)
                    && self.same_stable_expression(ar, br, depth)
            }
            (CHirExpr::Cast(at, a, _), CHirExpr::Cast(bt, b, _)) => {
                at == bt
                    && matches!(
                        self.unit.types().get(*at).map(|ty| &ty.kind),
                        Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::GnuInt128(_))
                    )
                    && self.same_stable_expression(a, b, depth)
            }
            // Exclude evaluate-once nodes too: their semantic identity is an
            // occurrence, not just their spelling. No pointer-equality shortcut
            // may bypass the effect/qualifier checks above.
            _ => false,
        }
    }

    fn stable_object(&self, symbol: SymbolId) -> bool {
        if self.global_registers.contains_key(&symbol) || self.local_registers.contains_key(&symbol)
        {
            return false;
        }
        self.unit
            .symbols()
            .symbol(symbol)
            .ok()
            .and_then(|s| self.unit.types().get(s.ty()))
            .is_some_and(|ty| !ty.qualifiers.is_volatile && !ty.qualifiers.is_atomic)
    }

    fn same_stable_place(&self, a: &MemoryPlace<'_>, b: &MemoryPlace<'_>, depth: u8) -> bool {
        let ordinary = |p: &MemoryPlace<'_>| {
            let q = p.access_qualifiers();
            !q.is_volatile && !q.is_atomic && !p.is_bit_field()
        };
        if a.value_ty() != b.value_ty() || !ordinary(a) || !ordinary(b) {
            return false;
        }
        let root_matches = match (a.root(), b.root()) {
            (PlaceRoot::Object { symbol: a }, PlaceRoot::Object { symbol: b }) => {
                a == b && self.stable_object(*a)
            }
            (
                PlaceRoot::PointerExpression {
                    pointer: a,
                    pointer_type: at,
                },
                PlaceRoot::PointerExpression {
                    pointer: b,
                    pointer_type: bt,
                },
            ) => at == bt && self.same_stable_expression(a, b, depth),
            _ => false,
        };
        root_matches
            && a.projections().len() == b.projections().len()
            && a.projections()
                .iter()
                .zip(b.projections())
                .all(|(a, b)| match (a, b) {
                    (PlaceProjection::Index(a), PlaceProjection::Index(b)) => {
                        a.element_ty() == b.element_ty()
                            && a.promoted_index() == b.promoted_index()
                            && self.same_stable_expression(a.index(), b.index(), depth)
                    }
                    (PlaceProjection::Field(a), PlaceProjection::Field(b)) => {
                        a.tag() == b.tag()
                            && a.field_index() == b.field_index()
                            && a.field_ty() == b.field_ty()
                    }
                    _ => false,
                })
    }
}
