//! Parser enums.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    /// Evaluate the C11 integer constant-expression subset needed by enum
    /// initializers and bit-field widths for `aarch64-unknown-linux-gnu`.
    /// Unsupported or undefined operations fail closed and are diagnosed by
    /// the semantic caller as E1303.
    pub(super) fn eval_integer_constant_expression(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Result<CInteger, ()> {
        match expression {
            CHirExpr::BuiltinObjectSize { .. } => evocc_chir::constant::eval_integer_constant(
                expression,
                &self.types,
                &self.symbols,
                &self.target_layout,
            )
            .ok_or(()),
            CHirExpr::BuiltinConstantP { operand, .. } => Ok(CInteger::int(i32::from(
                builtin_constant_p(operand, &self.types, &self.symbols, &self.target_layout),
            ))),
            CHirExpr::Lit(literal, _) => integer_literal_value(*literal).map_err(|_| ()),
            CHirExpr::MemoryRead { .. }
            | CHirExpr::BuiltinExpect(..)
            | CHirExpr::Atomic(_)
            | CHirExpr::Vector(_)
            | CHirExpr::Floating(_) => Err(()),
            CHirExpr::Complex(operation) => {
                self.canonical_integer_type(operation.facts.ty())?;
                evocc_chir::complex::constant_components(
                    expression,
                    &self.types,
                    &self.symbols,
                    &self.target_layout,
                )
                .map(|pair| pair[0])
                .ok_or(())
            }
            CHirExpr::CharLit(literal, _) => Ok(literal.integer_value()),
            CHirExpr::Ident { symbol, .. } => {
                let symbol = self.symbols.symbol(*symbol).map_err(|_| ())?;
                let SymbolData::Enumerator { value } = symbol.data() else {
                    return Err(());
                };
                Ok(CInteger::from_raw(
                    value as u128,
                    self.canonical_integer_type(symbol.ty())?,
                ))
            }
            CHirExpr::Unary(op, operand, _) => {
                eval_constant_unary_integer(*op, self.eval_integer_constant_expression(operand)?)
                    .map_err(|_| ())
            }
            CHirExpr::EvaluateOnce(operand, _) => self.eval_integer_constant_expression(operand),
            CHirExpr::Binary(op, left, right, _) => {
                let left = self.eval_integer_constant_expression(left)?;
                if *op == BinaryOp::And && !left.truth() {
                    return Ok(CInteger::int(0));
                }
                if *op == BinaryOp::Or && left.truth() {
                    return Ok(CInteger::int(1));
                }
                let right = self.eval_integer_constant_expression(right)?;
                eval_constant_binary_integer(*op, left, right).map_err(|_| ())
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let selected = if self.eval_integer_constant_expression(condition)?.truth() {
                    when_true
                } else {
                    when_false
                };
                let value = self.eval_integer_constant_expression(selected)?;
                match (
                    self.integer_expression_type(when_true),
                    self.integer_expression_type(when_false),
                ) {
                    (Ok(when_true), Ok(when_false)) => Ok(value.cast(usual_arithmetic_type(
                        CInteger::new(0, when_true).promoted().ty(),
                        CInteger::new(0, when_false).promoted().ty(),
                    ))),
                    // Keep the established dead-arm behavior for identifiers
                    // whose ordinary declaration/type is not available yet.
                    // The selected arm was evaluated and is authoritative.
                    _ => Ok(value),
                }
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let operand_value = self.eval_integer_constant_expression(operand)?;
                let selected = if operand_value.truth() {
                    operand_value
                } else {
                    self.eval_integer_constant_expression(fallback)?
                };
                let operand_type = CInteger::new(0, self.integer_expression_type(operand)?)
                    .promoted()
                    .ty();
                let fallback_type = CInteger::new(0, self.integer_expression_type(fallback)?)
                    .promoted()
                    .ty();
                Ok(selected.cast(usual_arithmetic_type(operand_type, fallback_type)))
            }
            CHirExpr::BitBuiltin { op, operand, .. } => {
                eval_bit_builtin(*op, self.eval_integer_constant_expression(operand)?).ok_or(())
            }
            CHirExpr::Cast(ty, operand, _) => {
                let target = self.canonical_integer_type(*ty)?;
                if let CHirExpr::FloatLit(literal, _) = operand {
                    return floating_literal_to_integer(*literal, target).map_err(|_| ());
                }
                Ok(self.eval_integer_constant_expression(operand)?.cast(target))
            }
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => Ok(CInteger::new(
                self.target_layout
                    .sizeof_type(&self.types, *ty)
                    .map_err(|_| ())?,
                CIntegerType::ULONG,
            )),
            CHirExpr::AlignOf(ty, _) => {
                let alignment = self
                    .target_layout
                    .alignof_type(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CInteger::new(u64::from(alignment), CIntegerType::ULONG))
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. }
            | CHirExpr::BuiltinVarArgs { .. }
            | CHirExpr::BuiltinVarArgPack { .. }
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _) => Err(()),
        }
    }

    /// Determine an integer expression's target type without evaluating its
    /// value. This is what permits conditional conversion and `sizeof expr`
    /// while preserving C's unevaluated/dead operand rules.
    pub(super) fn integer_expression_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Result<CIntegerType, ()> {
        match expression {
            CHirExpr::BuiltinVarArgPack { length: true, .. } => Ok(CIntegerType::INT),
            CHirExpr::BuiltinVarArgPack { .. } => Err(()),
            CHirExpr::Floating(operation) => self.canonical_integer_type(operation.facts.ty()),
            CHirExpr::Vector(operation) => self.canonical_integer_type(operation.facts.ty()),
            CHirExpr::Complex(operation) => self.canonical_integer_type(operation.facts.ty()),
            CHirExpr::Atomic(access) => self.canonical_integer_type(access.facts.ty()),
            CHirExpr::BuiltinExpect(..) => Ok(CIntegerType::LONG),
            CHirExpr::BuiltinConstantP { .. } => Ok(CIntegerType::INT),
            CHirExpr::BuiltinObjectSize { .. } => Ok(CIntegerType::ULONG),
            CHirExpr::Lit(literal, _) => Ok(integer_literal_value(*literal).map_err(|_| ())?.ty()),
            CHirExpr::MemoryRead { .. } => Err(()),
            CHirExpr::CharLit(literal, _) => Ok(literal.integer_value().ty()),
            CHirExpr::Ident { symbol, .. } => {
                let symbol = self.symbols.symbol(*symbol).map_err(|_| ())?;
                if symbol.kind() != SymbolKind::Enumerator {
                    return Err(());
                }
                self.canonical_integer_type(symbol.ty())
            }
            CHirExpr::Unary(op, operand, _) => match op {
                UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Abs | UnaryOp::BitNot => {
                    Ok(CInteger::new(0, self.integer_expression_type(operand)?)
                        .promoted()
                        .ty())
                }
                UnaryOp::Not => {
                    self.integer_expression_type(operand)?;
                    Ok(CIntegerType::INT)
                }
                UnaryOp::AddrOf | UnaryOp::Deref => Err(()),
            },
            CHirExpr::EvaluateOnce(operand, _) => self.integer_expression_type(operand),
            CHirExpr::Binary(op, left, right, _) => {
                let left = CInteger::new(0, self.integer_expression_type(left)?).promoted();
                let right = CInteger::new(0, self.integer_expression_type(right)?).promoted();
                match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor => Ok(usual_arithmetic_type(left.ty(), right.ty())),
                    BinaryOp::Shl | BinaryOp::Shr => Ok(left.ty()),
                    BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::And
                    | BinaryOp::Or => Ok(CIntegerType::INT),
                    BinaryOp::Comma
                    | BinaryOp::Assign
                    | BinaryOp::AddAssign
                    | BinaryOp::SubAssign
                    | BinaryOp::MulAssign
                    | BinaryOp::DivAssign
                    | BinaryOp::ModAssign
                    | BinaryOp::ShlAssign
                    | BinaryOp::ShrAssign
                    | BinaryOp::BitAndAssign
                    | BinaryOp::BitOrAssign
                    | BinaryOp::BitXorAssign => Err(()),
                }
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                self.integer_expression_type(condition)?;
                let when_true = CInteger::new(0, self.integer_expression_type(when_true)?)
                    .promoted()
                    .ty();
                let when_false = CInteger::new(0, self.integer_expression_type(when_false)?)
                    .promoted()
                    .ty();
                Ok(usual_arithmetic_type(when_true, when_false))
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let operand = CInteger::new(0, self.integer_expression_type(operand)?)
                    .promoted()
                    .ty();
                let fallback = CInteger::new(0, self.integer_expression_type(fallback)?)
                    .promoted()
                    .ty();
                Ok(usual_arithmetic_type(operand, fallback))
            }
            CHirExpr::Cast(ty, _, _) => self.canonical_integer_type(*ty),
            CHirExpr::CheckedArithmetic { .. } => Ok(CIntegerType::INT),
            CHirExpr::BitBuiltin { .. } => Ok(CIntegerType::INT),
            CHirExpr::StatementExpr { facts, .. } => self.canonical_integer_type(facts.ty()),
            CHirExpr::SizeOf(ty, _) | CHirExpr::SizeOfExpr(_, ty, _) => {
                self.target_layout
                    .sizeof_type(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::AlignOf(ty, _) => {
                self.target_layout
                    .alignof_type(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. }
            | CHirExpr::BuiltinVarArgs { .. }
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _)
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _) => Err(()),
        }
    }

    /// Whether `: type-name` follows: the start of an enum type specifier.
    pub(super) fn enum_type_specifier_follows(&self) -> bool {
        self.peek_kind() == Some(TokenKind::Colon)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| self.token_starts_type_name(token))
    }

    /// Parse `: specifier-qualifier-list`, the fixed underlying type of an
    /// enumeration (C23 6.7.2.2). The type is an integer type; `const` and
    /// `volatile` are dropped, as in GCC 14 and Apple clang. `_Bool`,
    /// `__int128`, an atomic or an aligned type are refused, because the
    /// reference compilers convert to them differently or this profile does
    /// not implement them. `None` means that a diagnostic was reported.
    pub(super) fn parse_enum_type_specifier(&mut self) -> (Option<IntRank>, SourceSpan) {
        let colon = self.advance().expect("peeked enumeration type colon");
        let mut attributes = Vec::new();
        let specifier = self.parse_type_specifier_with_attributes(&mut attributes);
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map_or(colon.span, |token| token.span);
        let span = self.merge_span(colon.span, end);
        if let Some(attribute) = attributes.first() {
            self.report("E2903", "malformed-attribute", attribute.span);
            return (None, span);
        }
        let Some(canonical) = specifier
            .and_then(|specifier| self.types.get(specifier.surface.type_id()))
            .cloned()
        else {
            return (None, span);
        };
        let qualifiers = canonical.qualifiers;
        let rank = match canonical.kind {
            _ if qualifiers.is_restrict => None,
            TypeKind::Int(IntRank::Bool) | TypeKind::GnuInt128(_) => {
                self.report("E2958", "unsupported-enumeration-underlying-type", span);
                return (None, span);
            }
            _ if qualifiers.is_atomic || qualifiers.aligned.is_some() => {
                self.report("E2958", "unsupported-enumeration-underlying-type", span);
                return (None, span);
            }
            TypeKind::Int(rank) => Some(rank),
            _ => None,
        };
        if rank.is_none() {
            self.report("E1307", "invalid-enumeration-underlying-type", span);
        }
        (rank, span)
    }

    /// Parse an enumerator list. With a fixed underlying type, each value is
    /// representable in that type and each enumerator has the enumerated type.
    pub(super) fn parse_enum_members(
        &mut self,
        fixed: Option<(TagId, IntRank)>,
    ) -> Option<(TagMembers<'arena>, TagDefinition)> {
        let fixed = fixed.map(|(tag, rank)| {
            let ty = self.intern(TypeKind::Enum(tag), Qualifiers::default());
            let integer = integer_type_for_rank(rank);
            if integer.is_signed() {
                (ty, integer.signed_min(), integer.signed_max())
            } else {
                (ty, 0, integer.mask() as i128)
            }
        });
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        let mut next_value = Some(0_i128);
        let mut minimum = 0_i128;
        let mut maximum = 0_i128;
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            let Some(token) = self.expect(TokenKind::Identifier) else {
                break;
            };
            let name = SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            };
            let enumerator_attributes =
                self.parse_attribute_specifiers(AttributePlacement::Enumerator);
            let written_value = if self.match_kind(TokenKind::Assign) {
                Some(self.parse_required_constant_assignment()?)
            } else {
                None
            };
            let mathematical_value = if let Some(expression) = written_value {
                self.eval_integer_constant_expression(expression)
                    .and_then(|value| value.try_mathematical_value().ok_or(()))
            } else {
                next_value.ok_or(())
            };
            let mathematical_value = match mathematical_value {
                Ok(value) => value,
                Err(()) => {
                    self.report(
                        "E1303",
                        "enumerator-value-is-not-an-integer-constant",
                        written_value.map_or(name.span, CHirExpr::span),
                    );
                    while !matches!(self.peek_kind(), Some(TokenKind::RBrace) | None) {
                        self.advance();
                    }
                    break;
                }
            };
            if let Some((_, low, high)) = fixed {
                if !(low..=high).contains(&mathematical_value) {
                    self.report(
                        "E1306",
                        "enumerator-outside-underlying-type",
                        written_value.map_or(name.span, CHirExpr::span),
                    );
                    while !matches!(self.peek_kind(), Some(TokenKind::RBrace) | None) {
                        self.advance();
                    }
                    break;
                }
            }
            let candidate_minimum = minimum.min(mathematical_value);
            let candidate_maximum = maximum.max(mathematical_value);
            if fixed.is_none() && !enum_range_is_representable(candidate_minimum, candidate_maximum)
            {
                self.report(
                    "E1303",
                    "enumerator-value-is-not-representable",
                    written_value.map_or(name.span, CHirExpr::span),
                );
                while !matches!(self.peek_kind(), Some(TokenKind::RBrace) | None) {
                    self.advance();
                }
                break;
            }
            minimum = candidate_minimum;
            maximum = candidate_maximum;
            let value = mathematical_value;
            let end = written_value.map_or(name.span, CHirExpr::span);
            let span = self.merge_span(name.span, end);
            canonical.push(EnumConstant {
                name: name.spelling.clone(),
                value,
            });
            let expression_ty = match fixed {
                Some((ty, ..)) => ty,
                None => self.intern_c_integer_type(enum_integer_type(value, value)),
            };
            let symbol =
                self.declare_symbol(&name, SymbolData::Enumerator { value }, expression_ty);
            self.add_entity_attributes(symbol, &enumerator_attributes);
            members.push(EnumeratorDecl {
                symbol,
                name,
                value,
                written_value,
                attributes: enumerator_attributes,
                span,
            });
            next_value = mathematical_value.checked_add(1);
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        // Enumerator constants fitting int have type int, independently of
        // the unsigned representation selected for a nonnegative enum object.
        // With a fixed underlying type they keep the enumerated type.
        if fixed.is_none() {
            let expression_ty = self.intern_c_integer_type(enum_integer_type(minimum, maximum));
            for member in &members {
                self.symbols
                    .update_symbol_type(member.symbol, expression_ty)
                    .expect("enumerator symbols belong to the parser symbol table");
            }
        }
        Some((TagMembers::Enum(members), TagDefinition::Enum(canonical)))
    }
}
