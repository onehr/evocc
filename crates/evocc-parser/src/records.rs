//! Parser records.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_tag_specifier(
        &mut self,
        requested_kind: TagKind,
        qualifiers: Qualifiers,
        keyword_span: SourceSpan,
        bare: bool,
    ) -> Option<ParsedSpecifier<'arena>> {
        let mut tag_attributes = self.parse_attribute_specifiers(AttributePlacement::TagKeyword);
        let name = if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().expect("peeked identifier");
            Some(SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            })
        } else {
            None
        };
        // C23 6.7.2.2: `enum E : T` fixes the underlying type. As in GCC 14,
        // only a colon that a type name follows begins it, so a member
        // `enum E : 3;` stays a bit-field.
        let fixed = (requested_kind == TagKind::Enum && self.enum_type_specifier_follows())
            .then(|| self.parse_enum_type_specifier());
        let is_definition = self.peek_kind() == Some(TokenKind::LBrace);
        let is_pure_forward =
            !is_definition && name.is_some() && self.peek_kind() == Some(TokenKind::Semicolon);
        if name.is_none() && !is_definition {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        }
        // Without a list, a fixed type is admitted only in `enum E : T;`.
        let fixed = match fixed {
            Some((rank, span)) if !is_definition && !(is_pure_forward && bare) => {
                if rank.is_some() {
                    self.report("E1307", "invalid-enumeration-underlying-type", span);
                }
                None
            }
            fixed => fixed,
        };

        let declaration_scope = self.current_scope;
        let mut fresh = false;
        let (tag, binding_kind) = if let Some(name) = &name {
            // A definition and a declaration containing only a tag specifier
            // operate in the current scope. In particular, `struct S;` in an
            // inner block introduces a fresh tag even when an outer S is
            // visible. A tag specifier used by a declarator instead denotes
            // the nearest visible tag.
            let existing = if is_definition || is_pure_forward {
                self.tag_in_current_scope(&name.spelling)
            } else {
                self.symbols
                    .resolve_tag(self.current_scope, &name.spelling)
                    .expect("the parser current scope is always live")
            };
            if let Some(tag) = existing {
                let kind = self
                    .types
                    .tag(tag)
                    .expect("symbol-table tags belong to the type interner")
                    .kind;
                if kind != requested_kind {
                    self.report(
                        "E1102",
                        format!("conflicting-tag-kind: {}", name.spelling),
                        name.span,
                    );
                }
                (tag, kind)
            } else {
                fresh = true;
                let tag = self
                    .types
                    .declare_tag(requested_kind, Some(name.spelling.clone()));
                self.symbols
                    .declare_tag(self.current_scope, &name.spelling, tag, name.span)
                    .expect("a fresh scoped tag spelling cannot be a duplicate");
                (tag, requested_kind)
            }
        } else {
            fresh = true;
            (self.types.declare_tag(requested_kind, None), requested_kind)
        };

        let semantic_kind = match binding_kind {
            TagKind::Struct => TypeKind::Struct(tag),
            TagKind::Union => TypeKind::Union(tag),
            TagKind::Enum => TypeKind::Enum(tag),
        };
        let ty = self.intern(semantic_kind, qualifiers);
        // Every declaration of an enumeration with a fixed underlying type
        // names that type, and its definition does too (C23 6.7.2.2).
        let earlier_fixed = self.types.tag(tag).and_then(|tag| tag.fixed_underlying);
        if requested_kind == TagKind::Enum && binding_kind == TagKind::Enum {
            match fixed {
                Some((Some(rank), span)) => {
                    let declared_without = !fresh && earlier_fixed.is_none();
                    if declared_without || self.types.fix_enum_underlying(tag, rank).is_err() {
                        self.report("E1307", "invalid-enumeration-underlying-type", span);
                    }
                }
                None if is_definition && earlier_fixed.is_some() => {
                    self.report(
                        "E1307",
                        "invalid-enumeration-underlying-type",
                        name.as_ref().map_or(keyword_span, |name| name.span),
                    );
                }
                _ => {}
            }
        }
        let fixed_rank = self.types.tag(tag).and_then(|tag| tag.fixed_underlying);
        // A reference to a declared tag uses it; a definition and a
        // declaration of the tag alone do not.
        if !fresh && !is_definition && !is_pure_forward {
            if let (Some(name), Some(reason)) = (&name, self.tag_unavailable(tag)) {
                self.use_unavailable(&name.spelling, &reason, name.span);
            }
        }
        self.merge_tag_availability(tag, &tag_attributes);

        if is_definition {
            let duplicate = match self.types.tag(tag) {
                Some(tag) if tag.fixed_underlying.is_some() => tag.fixed_enumerators_defined,
                Some(tag) => matches!(tag.state, TagState::Complete(_)),
                None => false,
            };
            if !duplicate && matches!(requested_kind, TagKind::Struct | TagKind::Union) {
                if let Some(max_align) = self.pack_alignment {
                    self.types.set_tag_layout(
                        tag,
                        TagLayoutAttributes {
                            max_align: Some(max_align),
                            ..TagLayoutAttributes::default()
                        },
                    );
                }
            }
            if duplicate {
                if let Some(name) = &name {
                    self.report(
                        "E1101",
                        format!("duplicate-tag-definition: {}", name.spelling),
                        name.span,
                    );
                }
            }
            self.advance();
            let (members, definition) = match requested_kind {
                TagKind::Struct | TagKind::Union => {
                    self.parse_record_members(requested_kind, tag)?
                }
                TagKind::Enum => self.parse_enum_members(fixed_rank.map(|rank| (tag, rank)))?,
            };
            let close = self.expect(TokenKind::RBrace);
            let closing = self.parse_attribute_specifiers(AttributePlacement::TagClosingBrace);
            self.merge_tag_availability(tag, &closing);
            tag_attributes.extend(closing);
            self.apply_attributes_to_tag(tag, &mut tag_attributes);
            let end = close
                .as_ref()
                .map_or_else(|| self.span(), |token| token.span);
            if !duplicate && binding_kind == requested_kind {
                let completed = match (fixed_rank, definition) {
                    (Some(_), TagDefinition::Enum(constants)) => {
                        self.types.define_fixed_enumerators(tag, constants)
                    }
                    (_, definition) => self.types.complete_tag(tag, definition),
                };
                if completed.is_err() {
                    self.report(
                        "E1101",
                        "duplicate-tag-definition",
                        name.as_ref().map_or(keyword_span, |name| name.span),
                    );
                }
            }
            if self.types.tag(tag).is_some_and(|tag| tag.transparent_union)
                && !self.transparent_union_has_register_representation(tag)
            {
                self.report("E2903", "malformed-attribute", keyword_span);
            }
            let specifier_span = self.merge_span(keyword_span, end);
            Some(ParsedSpecifier {
                surface: TypeSpecifier::Tag {
                    ty,
                    tag,
                    kind: requested_kind,
                    name: name.clone(),
                    span: specifier_span,
                },
                tag_event: Some(PendingTag {
                    tag,
                    scope: declaration_scope,
                    name,
                    members,
                    specifier_span,
                    attributes: tag_attributes,
                }),
                inherited_function_parameters: None,
            })
        } else {
            let end = name.as_ref().map_or(keyword_span, |name| name.span);
            let specifier_span = self.merge_span(keyword_span, end);
            Some(ParsedSpecifier {
                surface: TypeSpecifier::Tag {
                    ty,
                    tag,
                    kind: requested_kind,
                    name: name.clone(),
                    span: specifier_span,
                },
                tag_event: {
                    self.apply_attributes_to_tag(tag, &mut tag_attributes);
                    (fresh || is_pure_forward).then_some(PendingTag {
                        tag,
                        scope: declaration_scope,
                        name,
                        members: TagMembers::Incomplete,
                        attributes: tag_attributes.clone(),
                        specifier_span,
                    })
                },
                inherited_function_parameters: None,
            })
        }
    }

    pub(super) fn parse_extension_prefix(&mut self) -> Vec<SourceSpan> {
        let mut spans = Vec::new();
        while self.peek_kind() == Some(TokenKind::GnuExtension) {
            spans.push(self.advance().expect("peeked GNU extension prefix").span);
        }
        spans
    }

    pub(super) fn parse_record_members(
        &mut self,
        kind: TagKind,
        containing_tag: TagId,
    ) -> Option<(TagMembers<'arena>, TagDefinition)> {
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        // EvoIR 10.1.9: a member's context is the record and then the
        // function that encloses the record, as a C record does not nest.
        let record_frame = self.push_availability_frame(
            Some(self.tag_unavailable(containing_tag).is_some()),
            self.function_availability_frame(),
        );
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            // A pack marker changes layout between declarations, not midway
            // through the member list of one record. Do not mutate the pack
            // stack before reporting this unsupported placement.
            if self.peek().and_then(pragma_pack_action).is_some() {
                self.report("E0107", "pragma-pack-unsupported", self.span());
                return None;
            }
            if self.match_kind(TokenKind::Semicolon) {
                continue;
            }
            if self.peek_kind() == Some(TokenKind::StaticAssert) {
                self.parse_static_assert_declaration()?;
                continue;
            }
            let start = self.span();
            let member_frame = self.push_availability_frame(None, Some(record_frame));
            let mut declarator_frame = member_frame;
            self.parse_extension_prefix();
            let mut specifier_attributes =
                self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers);
            let Some(specifier) =
                self.parse_type_specifier_with_attributes(&mut specifier_attributes)
            else {
                self.report("E1001", "expected-declaration", self.span());
                self.advance();
                self.pop_availability_frames(member_frame);
                continue;
            };
            // `__u64 __attribute__((aligned(8))) name;`: attributes between the
            // specifiers and the first declarator sit among the specifiers.
            specifier_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers));
            let is_anonymous_aggregate = matches!(
                &specifier.surface,
                TypeSpecifier::Tag {
                    kind: TagKind::Struct | TagKind::Union,
                    name: None,
                    ..
                }
            );
            // One specifier may introduce several declarators (`int *p, q;`);
            // each becomes its own member with its own bit width and
            // attributes, while specifier-position attributes apply to all.
            let mut declaration_end;
            let mut first = true;
            loop {
                let mut member_attributes = specifier_attributes.clone();
                if !first {
                    if declarator_frame != member_frame {
                        self.pop_availability_frames(declarator_frame);
                    }
                    declarator_frame = self.push_availability_frame(None, Some(record_frame));
                    member_attributes.extend(
                        self.parse_attribute_specifiers(AttributePlacement::BeforeDeclarator),
                    );
                }
                first = false;
                let declarator = if matches!(
                    self.peek_kind(),
                    Some(TokenKind::Colon | TokenKind::Semicolon | TokenKind::Attribute)
                ) {
                    None
                } else {
                    self.parse_declarator(false)
                };
                if let Some(declarator) = declarator.as_ref() {
                    member_attributes.extend(declarator.attributes.iter().cloned());
                    self.validate_array_parameter_modifiers(&declarator.derived, false);
                    self.validate_variable_array_bounds(&declarator.derived, false);
                }
                let ty = declarator
                    .as_ref()
                    .map_or(specifier.surface.type_id(), |decl| {
                        self.apply_declarator(specifier.surface.type_id(), &decl.derived)
                    });
                member_attributes
                    .extend(self.parse_attribute_specifiers(AttributePlacement::Member));
                // A bit width is a constant expression, so it stops before a
                // comma that introduces the next declarator.
                let bit_width = if self.match_kind(TokenKind::Colon) {
                    Some(self.parse_required_constant_conditional()?)
                } else {
                    None
                };
                member_attributes
                    .extend(self.parse_attribute_specifiers(AttributePlacement::Member));
                self.settle_convention_attributes(&member_attributes, Settlement::Other);
                // Representation changes use the same resolver as object,
                // typedef and parameter declarations. Member alignment stays
                // on the member, never on its canonical type identity.
                let mut ty = ty;
                for attribute in &mut member_attributes {
                    if matches!(attribute.semantic_name(), Some("mode" | "vector_size")) {
                        ty = self.apply_attributes_to_type(ty, std::slice::from_mut(attribute));
                    }
                }
                let member_layout = self.member_layout_attributes(&mut member_attributes);
                self.validate_alignment_specifiers(ty, &member_attributes, bit_width.is_none());
                let fallback = declarator
                    .as_ref()
                    .map_or(specifier.surface.span(), |decl| decl.span);
                let more = self.match_kind(TokenKind::Comma);
                let end = if more {
                    fallback
                } else {
                    let semi = self.expect(TokenKind::Semicolon);
                    semi.as_ref().map_or(fallback, |token| token.span)
                };
                declaration_end = end;
                let span = self.merge_span(start, end);
                if declarator.is_none() && bit_width.is_none() && !is_anonymous_aggregate {
                    if more {
                        continue;
                    }
                    break;
                }
                let name = declarator.as_ref().map(|decl| decl.name.clone());
                let canonical_bit_field = bit_width.and_then(|width| {
                    self.validate_bit_field(
                        ty,
                        name.as_ref().map(|name| name.spelling.as_str()),
                        specifier.surface.span(),
                        width,
                    )
                });
                let written = declarator.as_ref().map_or(
                    WrittenDeclarator {
                        layers: Vec::new(),
                        span: specifier.surface.span(),
                    },
                    |decl| self.written_declarator(decl),
                );
                self.set_member_attributes(containing_tag, canonical.len(), &member_attributes);
                let unavailable = self.availability_target.is_some_and(|deployment| {
                    self.member_availability
                        .get(&(containing_tag, canonical.len()))
                        .is_some_and(|member| member.unavailable(deployment).is_some())
                });
                self.resolve_availability_frame(declarator_frame, unavailable);
                canonical.push(RecordField {
                    layout: member_layout,
                    name: name.as_ref().map(|name| name.spelling.clone()),
                    ty,
                    bit_field: canonical_bit_field,
                });
                members.push(RecordMember {
                    name,
                    ty,
                    specifier: specifier.surface.clone(),
                    written,
                    attributes: member_attributes,
                    bit_width,
                    span,
                });
                if !more {
                    break;
                }
            }
            self.pop_availability_frames(member_frame);
            if let Some(event) = specifier.tag_event {
                let span = self.merge_span(start, declaration_end);
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
        }
        self.pop_availability_frames(record_frame);
        self.validate_record_member_types(kind, containing_tag, &canonical, &members);
        Some(match kind {
            TagKind::Struct => (
                TagMembers::Struct(members),
                TagDefinition::Struct(canonical),
            ),
            TagKind::Union => (TagMembers::Union(members), TagDefinition::Union(canonical)),
            TagKind::Enum => unreachable!("enum members use parse_enum_members"),
        })
    }

    pub(super) fn validate_record_member_types(
        &mut self,
        kind: TagKind,
        containing_tag: TagId,
        fields: &[RecordField],
        members: &[RecordMember<'arena>],
    ) {
        for (index, (field, member)) in fields.iter().zip(members).enumerate() {
            // A typedef can hide every written array layer. Check the
            // canonical type, including pointers to a variable array, rather
            // than just this member declarator's bracket expressions.
            if self.type_is_variably_modified(field.ty) {
                self.report("E2203", "variable-length-object", member.written.span);
                return;
            }
            let is_unbounded_array = matches!(
                self.types.get(field.ty).map(|ty| &ty.kind),
                Some(TypeKind::Array {
                    bound: ArrayBound::Unbounded,
                    ..
                })
            );
            if kind == TagKind::Struct && is_unbounded_array && index + 1 != fields.len() {
                self.report("E1005", "invalid-declarator", member.written.span);
                return;
            }
            if !self.record_member_type_is_complete(field.ty, containing_tag, &mut HashSet::new()) {
                self.report("E1005", "invalid-declarator", member.written.span);
                return;
            }
        }
    }

    pub(super) fn record_member_type_is_complete(
        &self,
        ty: TypeId,
        containing_tag: TagId,
        visiting: &mut HashSet<TypeId>,
    ) -> bool {
        let Some(kind) = self.types.get(ty).map(|ty| ty.kind.clone()) else {
            return false;
        };
        match kind {
            TypeKind::TargetBuiltin(kind) => kind.layout().is_some(),
            TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_)
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_) => true,
            TypeKind::Array { element, .. } | TypeKind::Vector { element, .. } => {
                self.record_member_type_is_complete(element, containing_tag, visiting)
            }
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                if tag == containing_tag || !visiting.insert(ty) {
                    return false;
                }
                let complete = self.types.tag(tag).is_some_and(|tag| match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields))
                    | TagState::Complete(TagDefinition::Union(fields)) => {
                        fields.iter().all(|field| {
                            self.record_member_type_is_complete(field.ty, containing_tag, visiting)
                        })
                    }
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => false,
                });
                visiting.remove(&ty);
                complete
            }
            TypeKind::Enum(tag) => self
                .types
                .tag(tag)
                .is_some_and(|tag| matches!(tag.state, TagState::Complete(TagDefinition::Enum(_)))),
            TypeKind::Void | TypeKind::Function { .. } => false,
        }
    }

    /// Validate the C11 bit-field constraints that are independent of record
    /// placement. The written expression remains in C-HIR even when this
    /// reports an error; only validated metadata enters the canonical type.
    pub(super) fn validate_bit_field(
        &mut self,
        ty: TypeId,
        name: Option<&str>,
        type_span: SourceSpan,
        width_expression: &CHirExpr<'_>,
    ) -> Option<BitFieldSpec> {
        // C11 6.7.2.1p5 leaves atomic bit-fields implementation-defined.
        // EvoCC does not permit them, as GCC and Clang do not.
        if self
            .types
            .get(ty)
            .is_some_and(|canonical| canonical.qualifiers.is_atomic)
        {
            self.report("E2955", "invalid-atomic-type", type_span);
            return None;
        }
        let Some(base_width_bits) = self.bit_field_base_width(ty) else {
            self.report("E1305", "bit-field-has-non-integer-type", type_span);
            return None;
        };

        let width = match self
            .eval_integer_constant_expression(width_expression)
            .ok()
            .and_then(CInteger::try_mathematical_value)
        {
            Some(width) => width,
            None => {
                self.report(
                    "E1303",
                    "bit-field-width-is-not-an-integer-constant",
                    width_expression.span(),
                );
                return None;
            }
        };

        if width < 0 {
            self.report(
                "E1302",
                format!("bit-field-width-is-negative: {width}"),
                width_expression.span(),
            );
            return None;
        }
        if width == 0 && name.is_some() {
            self.report(
                "E1304",
                "named-bit-field-has-zero-width",
                width_expression.span(),
            );
            return None;
        }
        if width > i128::from(base_width_bits) {
            self.report(
                "E1301",
                format!("bit-field-width-exceeds-type: {width} > {base_width_bits}"),
                width_expression.span(),
            );
            return None;
        }

        Some(BitFieldSpec {
            width_bits: u16::try_from(width)
                .expect("validated bit-field width fits the supported integer base type"),
        })
    }

    pub(super) fn bit_field_base_width(&self, ty: TypeId) -> Option<u16> {
        let canonical = self.types.get(ty)?;
        match &canonical.kind {
            TypeKind::Int(IntRank::Bool) => Some(1),
            TypeKind::Int(
                IntRank::Char | IntRank::SignedPlainChar | IntRank::SChar | IntRank::UChar,
            ) => Some(8),
            TypeKind::Int(IntRank::Short | IntRank::UShort) => Some(16),
            TypeKind::Int(IntRank::Int | IntRank::UInt) => Some(32),
            TypeKind::Enum(_) => self
                .target_layout
                .layout_of(&self.types, ty)
                .ok()
                .and_then(|layout| layout.size_bytes.checked_mul(8))
                .and_then(|bits| u16::try_from(bits).ok()),
            TypeKind::Int(
                IntRank::Long | IntRank::ULong | IntRank::LongLong | IntRank::ULongLong,
            ) => Some(64),
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::TargetBuiltin(_)
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Vector { .. }
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Struct(_)
            | TypeKind::Union(_) => None,
        }
    }

    pub(super) fn canonical_integer_type(&self, ty: TypeId) -> Result<CIntegerType, ()> {
        let canonical = self.types.get(ty).ok_or(())?;
        match canonical.kind {
            TypeKind::TargetBuiltin(kind) => {
                evocc_chir::integer::integer_type_for_target(kind).ok_or(())
            }
            TypeKind::Int(rank) => Ok(integer_type_for_rank(rank)),
            TypeKind::GnuInt128(GnuInt128Signedness::Signed) => Ok(CIntegerType::INT128),
            TypeKind::GnuInt128(GnuInt128Signedness::Unsigned) => Ok(CIntegerType::UINT128),
            TypeKind::Enum(tag) => self
                .types
                .enum_representation(tag)
                .map(integer_type_for_rank)
                .ok_or(()),
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::Float(_)
            | TypeKind::Vector { .. }
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Struct(_)
            | TypeKind::Union(_) => Err(()),
        }
    }

    pub(super) fn intern_c_integer_type(&mut self, ty: CIntegerType) -> TypeId {
        let kind = if ty == CIntegerType::INT128 {
            TypeKind::GnuInt128(GnuInt128Signedness::Signed)
        } else if ty == CIntegerType::UINT128 {
            TypeKind::GnuInt128(GnuInt128Signedness::Unsigned)
        } else {
            TypeKind::Int(
                rank_for_integer_type(ty)
                    .unwrap_or_else(|| unreachable!("unsupported target integer type: {ty:?}")),
            )
        };
        self.intern(kind, Qualifiers::default())
    }
}
