//! Parser assembly.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_optional_asm_label(
        &mut self,
    ) -> Option<(Option<String>, Option<SourceSpan>)> {
        if self.peek_kind() != Some(TokenKind::Asm) {
            return Some((None, None));
        }
        let keyword = self.advance().expect("peeked asm declarator label");
        self.expect(TokenKind::LParen)?;
        let (label, _) = self.parse_asm_string()?;
        let close = self.expect(TokenKind::RParen)?;
        Some((Some(label), Some(self.merge_span(keyword.span, close.span))))
    }

    pub(super) fn parse_asm_construct(&mut self) -> Option<AsmStmt<'arena>> {
        let keyword = self.advance().expect("peeked GNU asm");
        let mut volatile_span = None;
        let mut inline_span = None;
        let mut goto_span = None;
        loop {
            let destination = match self.peek_kind() {
                Some(TokenKind::Volatile) => &mut volatile_span,
                Some(TokenKind::Inline) => &mut inline_span,
                Some(TokenKind::Goto) => &mut goto_span,
                _ => break,
            };
            let qualifier = self.advance().expect("peeked asm qualifier");
            if destination.replace(qualifier.span).is_some() {
                self.report("E2914", "malformed-asm", qualifier.span);
                return None;
            }
        }

        self.expect(TokenKind::LParen)?;
        let (template, template_span) = self.parse_asm_string()?;
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut clobbers = Vec::new();
        let mut clobber_spans = Vec::new();
        let mut labels = Vec::new();

        if self.match_kind(TokenKind::Colon) {
            outputs = self.parse_asm_operands(true)?;
            if self.match_kind(TokenKind::Colon) {
                inputs = self.parse_asm_operands(false)?;
                if self.match_kind(TokenKind::Colon) {
                    while !matches!(self.peek_kind(), Some(TokenKind::Colon | TokenKind::RParen)) {
                        let (clobber, span) = self.parse_asm_string()?;
                        clobbers.push(clobber);
                        clobber_spans.push(span);
                        if !self.match_kind(TokenKind::Comma) {
                            break;
                        }
                    }
                    if self.match_kind(TokenKind::Colon) {
                        if goto_span.is_none() {
                            self.report("E2914", "malformed-asm", self.span());
                            return None;
                        }
                        while self.peek_kind() != Some(TokenKind::RParen) {
                            let token = self.expect(TokenKind::Identifier)?;
                            labels.push(SpannedName {
                                spelling: self.token_text(&token).to_owned(),
                                span: token.span,
                            });
                            if !self.match_kind(TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                }
            }
        }
        if self.peek_kind() == Some(TokenKind::Colon) {
            self.report("E2914", "malformed-asm", self.span());
            return None;
        }
        let close = self.expect(TokenKind::RParen)?;

        let known_names = outputs
            .iter()
            .chain(&inputs)
            .filter_map(|operand| operand.symbolic.as_deref())
            .chain(labels.iter().map(|label| label.spelling.as_str()))
            .collect::<HashSet<_>>();
        if asm_template_symbolic_names(&template)
            .into_iter()
            .any(|name| !known_names.contains(name))
        {
            self.report("E2914", "malformed-asm", template_span);
            return None;
        }
        for label in &mut labels {
            if let Some(internal) = self.resolve_local_label(&label.spelling) {
                label.spelling = internal;
            }
        }

        Some(AsmStmt {
            template,
            template_span,
            outputs,
            inputs,
            clobbers,
            clobber_spans,
            labels,
            volatile: volatile_span.is_some(),
            volatile_span,
            inline: inline_span.is_some(),
            inline_span,
            is_goto: goto_span.is_some(),
            goto_span,
            span: self.merge_span(keyword.span, close.span),
        })
    }

    pub(super) fn parse_asm_operands(&mut self, outputs: bool) -> Option<Vec<AsmOperand<'arena>>> {
        let mut operands = Vec::new();
        while !matches!(self.peek_kind(), Some(TokenKind::Colon | TokenKind::RParen)) {
            let start = self.span();
            let (symbolic, symbolic_span) = if self.match_kind(TokenKind::LBracket) {
                let name = self.expect(TokenKind::Identifier)?;
                self.expect(TokenKind::RBracket)?;
                (Some(self.token_text(&name).to_owned()), Some(name.span))
            } else {
                (None, None)
            };
            let (constraint, constraint_span) = self.parse_asm_string()?;
            self.expect(TokenKind::LParen)?;
            let expression = self.parse_expression()?;
            let close = self.expect(TokenKind::RParen)?;
            let span = self.merge_span(start, close.span);
            let memory = matches!(
                constraint.as_str(),
                "m" | "=m" | "+m" | "Q" | "=Q" | "+Q" | "Qo"
            );
            let memory_place = if memory {
                let Some(place) = self
                    .resolve_memory_place(expression)
                    .filter(MemoryPlace::is_addressable)
                else {
                    self.report("E2914", "malformed-asm", expression.span());
                    return None;
                };
                Some(place)
            } else {
                None
            };
            let writable = memory_place.as_ref().map_or_else(
                || self.is_modifiable_asm_output(expression),
                |place| self.is_writable_asm_memory(place),
            );
            if outputs && !writable {
                self.report("E2914", "malformed-asm", expression.span());
                return None;
            }
            operands.push(AsmOperand {
                symbolic,
                symbolic_span,
                constraint,
                constraint_span,
                expression,
                span,
            });
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        Some(operands)
    }

    pub(super) fn parse_asm_string(&mut self) -> Option<(String, SourceSpan)> {
        let first = self.expect(TokenKind::StringLiteral)?;
        if first.text.has_raw_bytes() {
            self.report("E0002", "invalid encoding in assembly text", first.span);
            return None;
        }
        let Some(mut literal) = parse_string_literal(self.token_text(&first)) else {
            self.report("E1006", "invalid-string-literal", first.span);
            return None;
        };
        let mut end = first.span;
        while self.peek_kind() == Some(TokenKind::StringLiteral) {
            let next = self.advance().expect("peeked adjacent asm string literal");
            if next.text.has_raw_bytes() {
                self.report("E0002", "invalid encoding in assembly text", next.span);
                return None;
            }
            let Some(fragment) = parse_string_literal(self.token_text(&next)) else {
                self.report("E1006", "invalid-string-literal", next.span);
                return None;
            };
            let Some(prefix) = concatenate_string_prefix(literal.prefix, fragment.prefix) else {
                self.report("E1006", "incompatible-string-literal-prefixes", next.span);
                return None;
            };
            literal.prefix = prefix;
            literal.value.push_str(&fragment.value);
            literal.fragments.extend(fragment.fragments);
            end = next.span;
        }
        let span = self.merge_span(first.span, end);
        let Some(bytes) = literal.decoded_units().and_then(|units| {
            units
                .into_iter()
                .map(u8::try_from)
                .collect::<Result<Vec<_>, _>>()
                .ok()
        }) else {
            self.report("E1006", "invalid-string-literal", span);
            return None;
        };
        let Ok(value) = String::from_utf8(bytes) else {
            self.report("E1006", "invalid-string-literal", span);
            return None;
        };
        Some((value, span))
    }

    pub(super) fn is_modifiable_asm_output(&self, expression: &CHirExpr<'_>) -> bool {
        if let CHirExpr::MemoryRead { place, .. } = expression {
            return place.is_modifiable();
        }
        let Some(inferred) = self.expression_type(expression) else {
            return false;
        };
        if inferred.category != ExprCategory::LValue {
            return false;
        }
        let InferredType::Canonical(ty) = inferred.ty else {
            return false;
        };
        is_modifiable_object_type(&self.types, ty)
    }

    /// Assembly writes an array's storage, not an array assignment. Preserve
    /// const subobject checks while testing its element type for writability.
    pub(super) fn is_writable_asm_memory(&self, place: &MemoryPlace<'_>) -> bool {
        if place.access_qualifiers().is_const {
            return false;
        }
        let mut ty = place.value_ty();
        while let Some(canonical) = self.types.get(ty) {
            if canonical.qualifiers.is_const {
                return false;
            }
            let TypeKind::Array { element, .. } = canonical.kind else {
                return is_modifiable_object_type(&self.types, ty);
            };
            ty = element;
        }
        false
    }
}
