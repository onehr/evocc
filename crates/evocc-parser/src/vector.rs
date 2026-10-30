use super::*;
use evocc_chir::vector::{VectorExpr, VectorOp};

impl<'src, 'arena> Parser<'src, 'arena> {
    /// Clang's NEON attribute counts lanes. Only 64- and 128-bit vectors are
    /// valid. Polynomial vectors use unsigned 8-, 16- or 64-bit elements.
    pub(super) fn apply_neon_vector(
        &mut self,
        ty: TypeId,
        lanes: u64,
        polynomial: bool,
        already_applied: bool,
    ) -> Option<TypeId> {
        let canonical = self.types.get(ty)?.clone();
        if already_applied
            && matches!(canonical.kind, TypeKind::Vector { lanes: n, .. } if u64::from(n) == lanes)
        {
            return Some(ty);
        }
        let valid = if polynomial {
            matches!(
                canonical.kind,
                TypeKind::Int(
                    IntRank::UChar | IntRank::UShort | IntRank::ULong | IntRank::ULongLong
                )
            )
        } else {
            matches!(canonical.kind, TypeKind::Int(rank) if rank != IntRank::Bool)
                || matches!(
                    canonical.kind,
                    TypeKind::Float(FloatKind::F32 | FloatKind::F64)
                )
                || matches!(canonical.kind, TypeKind::TargetBuiltin(kind) if kind.half_format().is_some() || kind == TargetBuiltinType::ArmMfp8)
        };
        let bytes = self
            .target_layout
            .layout_of(&self.types, ty)
            .ok()?
            .size_bytes;
        if !valid || !matches!(bytes.checked_mul(lanes), Some(8 | 16)) {
            return None;
        }
        let element = self.intern(canonical.kind, Qualifiers::default());
        // Clang makes ordinary NEON and GNU vectors of the same element
        // compatible. Half and opaque target vectors keep their existing
        // declaration-only family.
        let family = if matches!(self.types.get(element)?.kind, TypeKind::TargetBuiltin(_)) {
            evocc_types::VectorFamily::ArmNeon
        } else {
            evocc_types::VectorFamily::Gnu
        };
        Some(self.intern(
            TypeKind::Vector {
                element,
                lanes: u16::try_from(lanes).ok()?,
                family,
            },
            canonical.qualifiers,
        ))
    }

    /// A vector-size attribute changes the scalar base, not the surrounding
    /// array, pointer or function declarator. Reapplying the same inventory
    /// entry to a freshly built declarator must not skip that transformation.
    pub(super) fn apply_vector_size(
        &mut self,
        ty: TypeId,
        bytes: u64,
        already_applied: bool,
    ) -> Option<TypeId> {
        let canonical = self.types.get(ty)?.clone();
        let kind = match canonical.kind {
            TypeKind::Pointer(pointee) => {
                TypeKind::Pointer(self.apply_vector_size(pointee, bytes, already_applied)?)
            }
            TypeKind::Array { element, bound } => TypeKind::Array {
                element: self.apply_vector_size(element, bytes, already_applied)?,
                bound,
            },
            TypeKind::Function {
                return_type,
                parameters,
            } => TypeKind::Function {
                return_type: self.apply_vector_size(return_type, bytes, already_applied)?,
                parameters,
            },
            TypeKind::Vector {
                family: evocc_types::VectorFamily::Gnu,
                ..
            } if already_applied
                && self
                    .target_layout
                    .layout_of(&self.types, ty)
                    .ok()?
                    .size_bytes
                    == bytes =>
            {
                return Some(ty);
            }
            TypeKind::Int(IntRank::Bool) => return None,
            kind @ (TypeKind::Int(_) | TypeKind::Float(_) | TypeKind::GnuInt128(_)) => {
                let size = self
                    .target_layout
                    .layout_of(&self.types, ty)
                    .ok()?
                    .size_bytes;
                if !bytes.is_power_of_two() || size == 0 || !bytes.is_multiple_of(size) {
                    return None;
                }
                let lanes = u16::try_from(bytes / size).ok()?;
                let element = self.intern(kind, Qualifiers::default());
                TypeKind::Vector {
                    element,
                    lanes,
                    family: evocc_types::VectorFamily::Gnu,
                }
            }
            _ => return None,
        };
        Some(self.intern(kind, canonical.qualifiers))
    }

    pub(super) fn vector_expression_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<(TypeId, TypeId, u16)> {
        let InferredType::Canonical(ty) = self.expression_type(expression)?.ty else {
            return None;
        };
        let TypeKind::Vector { element, lanes, .. } = self.types.get(ty)?.kind else {
            return None;
        };
        Some((ty, element, lanes))
    }

    pub(super) fn vector_node(
        &self,
        op: VectorOp,
        operands: Vec<&'arena CHirExpr<'arena>>,
        ty: TypeId,
        span: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        self.alloc_node(CHirExpr::Vector(VectorExpr {
            op,
            operands,
            facts: ExprFacts::value(ty),
            span,
        }))
    }

    fn vector_mask_type(&mut self, element: TypeId, lanes: u16) -> Option<TypeId> {
        // Comparison masks have the lane's representation width even when
        // the operands are floating vectors in an unused target-header body.
        let bits = self
            .target_layout
            .layout_of(&self.types, element)
            .ok()?
            .size_bytes
            * 8;
        // Darwin follows Apple clang: 64-bit lanes give `long long` masks and
        // character lanes give `char` masks. Linux follows GCC 14.
        let signed = match bits {
            8 if self.target_layout.long_long_vector_masks() => TypeKind::Int(IntRank::Char),
            8 => TypeKind::Int(IntRank::SChar),
            16 => TypeKind::Int(IntRank::Short),
            32 => TypeKind::Int(IntRank::Int),
            64 if self.target_layout.long_long_vector_masks() => TypeKind::Int(IntRank::LongLong),
            64 => TypeKind::Int(IntRank::Long),
            128 => TypeKind::GnuInt128(GnuInt128Signedness::Signed),
            _ => return None,
        };
        let element = self.intern(signed, Qualifiers::default());
        Some(self.intern(
            TypeKind::Vector {
                element,
                lanes,
                family: evocc_types::VectorFamily::Gnu,
            },
            Qualifiers::default(),
        ))
    }

    fn vector_broadcast_operand(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        element: TypeId,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let inferred = self.converted_expression_type(expression)?;
        if let OperandTypeClass::Real(target) =
            self.operand_type_class(InferredType::Canonical(element))
        {
            let layout = self.target_layout;
            let precision = |kind| match layout.float_format(kind) {
                FloatKind::F16 => 11,
                FloatKind::BF16 => 8,
                FloatKind::F32 => 24,
                FloatKind::F64 => 53,
                FloatKind::F128 => 113,
            };
            let source = self.operand_type_class(inferred.ty);
            let type_fits = match source {
                OperandTypeClass::Real(kind) => precision(kind) <= precision(target),
                OperandTypeClass::Integer(kind) => {
                    u32::from(kind.bits()) - u32::from(kind.is_signed()) <= precision(target)
                }
                _ => false,
            };
            let source_format = match source {
                OperandTypeClass::Real(kind) => Some(layout.float_format(kind)),
                _ => None,
            };
            let constant_fits = if let Ok(value) = self.eval_integer_constant_expression(expression)
            {
                let magnitude = if value.ty().is_signed() {
                    value.mathematical_value().unsigned_abs()
                } else {
                    value.raw_bits()
                };
                magnitude == 0
                    || (128 - magnitude.leading_zeros() - magnitude.trailing_zeros()
                        <= precision(target)
                        && (target != FloatKind::F16 || magnitude < 65536))
            } else if source_format == Some(FloatKind::F128)
                && !matches!(target, FloatKind::F32 | FloatKind::F64)
            {
                false
            } else {
                evocc_chir::constant::floating_constant_value(
                    expression,
                    &self.types,
                    &self.symbols,
                    &self.target_layout,
                )
                .is_some_and(|value| {
                    match target {
                        FloatKind::F32 => {
                            value.is_finite()
                                && value
                                    .convert(target)
                                    .and_then(|narrowed| narrowed.convert(value.kind()))
                                    .is_some_and(|roundtrip| {
                                        roundtrip.compare(value) == Some(std::cmp::Ordering::Equal)
                                    })
                        }
                        // An exact finite binary128 constant broadcasts too,
                        // as GCC 14, Clang 19 and Apple clang accept it.
                        FloatKind::F64 if source_format == Some(FloatKind::F128) => {
                            value.is_finite()
                                && value
                                    .convert(target)
                                    .and_then(|narrowed| narrowed.convert(value.kind()))
                                    .is_some_and(|roundtrip| {
                                        roundtrip.compare(value) == Some(std::cmp::Ordering::Equal)
                                    })
                        }
                        FloatKind::F64 => value.is_finite(),
                        // Retain the existing declarable-only type boundary.
                        FloatKind::F16 | FloatKind::BF16 | FloatKind::F128 => value.is_zero(),
                    }
                })
            };
            if !type_fits && !constant_fits {
                self.report(
                    "E2102",
                    "vector-scalar-conversion-loses-values",
                    expression.span(),
                );
                return None;
            }
            return Some(self.alloc_node(CHirExpr::Cast(element, expression, expression.span())));
        }
        let target = self.canonical_integer_type(element).ok()?;
        let source = match self.operand_type_class(inferred.ty) {
            OperandTypeClass::Integer(source) => source,
            OperandTypeClass::WideInteger { ty, .. } => self.canonical_integer_type(ty).ok()?,
            _ => {
                self.report("E2102", "invalid-vector-operand", expression.span());
                return None;
            }
        };
        // GNU vectors admit sign-changing conversion to an unsigned lane
        // when the scalar representation is no wider. The reverse change
        // at equal width is an oracle conflict and remains refused.
        let source_width_admitted = source.bits() <= target.bits()
            && !(target.is_signed() && !source.is_signed() && source.bits() == target.bits());
        let constant_fits = self
            .eval_integer_constant_expression(expression)
            .ok()
            .is_some_and(|value| {
                value.try_mathematical_value().is_some_and(|value| {
                    let converted = CInteger::from_raw(value as u128, target);
                    converted.try_mathematical_value() == Some(value)
                        || (!target.is_signed() && value < 0 && value >= target.signed_min())
                })
            });
        if !source_width_admitted && !constant_fits {
            self.report(
                "E2102",
                "vector-scalar-conversion-loses-values",
                expression.span(),
            );
            return None;
        }
        Some(self.alloc_node(CHirExpr::Cast(element, expression, expression.span())))
    }

    pub(super) fn make_vector_binary(
        &mut self,
        op: BinaryOp,
        mut left: &'arena CHirExpr<'arena>,
        mut right: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let left_type = self.vector_expression_type(left);
        let right_type = self.vector_expression_type(right);
        let (ty, element, lanes) = left_type.or(right_type)?;
        if self.canonical_integer_type(element).is_err()
            && matches!(
                op,
                BinaryOp::Mod
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::Shl
                    | BinaryOp::Shr
            )
        {
            self.report("E2102", "invalid-vector-operand", span);
            return None;
        }
        if matches!(op, BinaryOp::And | BinaryOp::Or) {
            self.report("E2102", "invalid-vector-operand", span);
            return None;
        }
        if let (Some((a, _, _)), Some((b, _, _))) = (left_type, right_type) {
            if !self.types.compatible_ignoring_top_qualifiers(a, b) {
                self.report("E2102", "incompatible-vector-types", span);
                return None;
            }
        }
        if left_type.is_none() {
            left = self.vector_broadcast_operand(left, element)?;
        }
        if right_type.is_none() {
            right = self.vector_broadcast_operand(right, element)?;
        }
        let result = if matches!(
            op,
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge
        ) {
            self.vector_mask_type(element, lanes)?
        } else {
            ty
        };
        Some(self.vector_node(VectorOp::Binary(op), vec![left, right], result, span))
    }

    pub(super) fn make_vector_unary(
        &mut self,
        op: UnaryOp,
        operand: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let (ty, element, lanes) = self.vector_expression_type(operand)?;
        // GCC 14 and Clang 19 refuse `!` on a floating vector in C (EvoIR 10.1.14).
        if matches!(op, UnaryOp::BitNot | UnaryOp::Not)
            && self.canonical_integer_type(element).is_err()
        {
            self.report("E2102", "invalid-vector-operand", span);
            return None;
        }
        let ty = if op == UnaryOp::Not {
            self.vector_mask_type(element, lanes)?
        } else {
            ty
        };
        Some(self.vector_node(VectorOp::Unary(op), vec![operand], ty, span))
    }

    pub(super) fn make_vector_cast(
        &mut self,
        ty: TypeId,
        operand: &'arena CHirExpr<'arena>,
        span: SourceSpan,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let source = self.expression_type(operand)?.ty;
        let source = self.materialize_inferred_type(source, Some(operand))?;
        if ![source, ty].into_iter().all(|ty| {
            matches!(
                self.types.get(ty).map(|t| &t.kind),
                Some(
                    TypeKind::Int(_)
                        | TypeKind::GnuInt128(_)
                        | TypeKind::Vector { .. }
                        | TypeKind::TargetBuiltin(_)
                )
            )
        }) {
            self.report("E2102", "invalid-vector-cast", span);
            return None;
        }
        let source_size = self
            .target_layout
            .layout_of(&self.types, source)
            .ok()?
            .size_bytes;
        let target_size = self
            .target_layout
            .layout_of(&self.types, ty)
            .ok()?
            .size_bytes;
        if source_size != target_size {
            self.report("E2102", "vector-cast-size-mismatch", span);
            return None;
        }
        Some(self.vector_node(VectorOp::Reinterpret, vec![operand], ty, span))
    }

    pub(super) fn parse_vector_builtin(
        &mut self,
        name: &str,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        self.expect(TokenKind::LParen)?;
        let first = self.parse_argument_expression()?;
        self.expect(TokenKind::Comma)?;
        if name == "__builtin_convertvector" {
            let (ty, _) = self.parse_type_name()?;
            let end = self.expect(TokenKind::RParen)?;
            let (_, _, lanes) = self.vector_expression_type(first)?;
            if !matches!(self.types.get(ty).map(|ty| &ty.kind), Some(TypeKind::Vector { lanes: output, .. }) if *output == lanes)
            {
                self.report("E2102", "incompatible-vector-types", keyword.span);
                return None;
            }
            return Some(self.vector_node(
                VectorOp::Convert,
                vec![first],
                ty,
                self.merge_span(keyword.span, end.span),
            ));
        }
        let mut arguments = vec![first, self.parse_argument_expression()?];
        while self.match_kind(TokenKind::Comma) {
            arguments.push(self.parse_argument_expression()?);
        }
        let end = self.expect(TokenKind::RParen)?;
        let span = self.merge_span(keyword.span, end.span);
        let (ty, element, lanes) = self.vector_expression_type(first)?;
        if name == "__builtin_shufflevector" {
            if arguments.len() < 3
                || self
                    .vector_expression_type(arguments[1])
                    .is_none_or(|(second, _, _)| {
                        !self.types.compatible_ignoring_top_qualifiers(ty, second)
                    })
            {
                self.report("E2102", "invalid-vector-shuffle", span);
                return None;
            }
            let indices: Option<Vec<i32>> = arguments[2..]
                .iter()
                .map(|argument| {
                    self.eval_integer_constant_expression(argument)
                        .ok()?
                        .try_mathematical_value()
                        .and_then(|value| i32::try_from(value).ok())
                })
                .collect();
            let indices = indices.filter(|indices| {
                indices.len().is_power_of_two()
                    && indices
                        .iter()
                        .all(|i| *i >= -1 && *i < i32::from(lanes) * 2)
            });
            let Some(indices) = indices else {
                self.report("E2102", "invalid-vector-shuffle", span);
                return None;
            };
            let output_lanes = u16::try_from(indices.len()).ok()?;
            let TypeKind::Vector { family, .. } = self.types.get(ty)?.kind else {
                return None;
            };
            let result = self.intern(
                TypeKind::Vector {
                    element,
                    lanes: output_lanes,
                    family,
                },
                Qualifiers::default(),
            );
            return Some(self.vector_node(
                VectorOp::ShuffleStatic(indices),
                arguments[..2].to_vec(),
                result,
                span,
            ));
        }
        if !matches!(arguments.len(), 2 | 3)
            || (arguments.len() == 3
                && self
                    .vector_expression_type(arguments[1])
                    .is_none_or(|(second, _, _)| {
                        !self.types.compatible_ignoring_top_qualifiers(ty, second)
                    }))
        {
            self.report("E2102", "invalid-vector-shuffle", span);
            return None;
        }
        let (_, mask_element, mask_lanes) = self.vector_expression_type(arguments.last()?)?;
        if mask_lanes != lanes
            || self.canonical_integer_type(mask_element).is_err()
            || self
                .target_layout
                .layout_of(&self.types, mask_element)
                .ok()?
                .size_bytes
                != self
                    .target_layout
                    .layout_of(&self.types, element)
                    .ok()?
                    .size_bytes
        {
            self.report("E2102", "invalid-vector-shuffle", span);
            return None;
        }
        Some(self.vector_node(VectorOp::Shuffle, arguments, ty, span))
    }
}
