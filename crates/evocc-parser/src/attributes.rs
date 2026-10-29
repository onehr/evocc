//! Parser attributes.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn parse_storage_class(&mut self) -> StorageClass {
        let storage = match self.peek_kind() {
            Some(TokenKind::Typedef) => StorageClass::Typedef,
            Some(TokenKind::Extern) => StorageClass::Extern,
            Some(TokenKind::Static) => StorageClass::Static,
            Some(TokenKind::Auto) => StorageClass::Auto,
            Some(TokenKind::Register) => StorageClass::Register,
            Some(TokenKind::ThreadLocal) => StorageClass::ThreadLocal,
            _ => return StorageClass::None,
        };
        self.advance();
        storage
    }

    // ── GNU attributes ───────────────────────────────────────────────────

    pub(super) fn bracket_attribute_starts_at(&self, position: usize) -> bool {
        self.tokens.get(position).map(|token| token.kind) == Some(TokenKind::LBracket)
            && self.tokens.get(position + 1).map(|token| token.kind) == Some(TokenKind::LBracket)
    }

    pub(super) fn attribute_starts_at(&self, position: usize) -> bool {
        self.tokens.get(position).map(|token| token.kind) == Some(TokenKind::Attribute)
            || self.bracket_attribute_starts_at(position)
    }

    pub(super) fn parse_bracket_attributes(
        &mut self,
        placement: AttributePlacement,
    ) -> Vec<Attribute> {
        if self.bracket_attribute_starts_at(self.pos) {
            self.parse_attribute_specifiers(placement)
        } else {
            Vec::new()
        }
    }

    /// Look ahead through one attribute group without consuming or applying it.
    pub(super) fn attribute_specifier_end(&self, position: usize) -> Option<usize> {
        let bracketed = self.bracket_attribute_starts_at(position);
        let start = position + usize::from(!bracketed);
        let (open, close) = if bracketed {
            (TokenKind::LBracket, TokenKind::RBracket)
        } else {
            (TokenKind::LParen, TokenKind::RParen)
        };
        if self.tokens.get(start)?.kind != open || self.tokens.get(start + 1)?.kind != open {
            return None;
        }
        let mut depth = 0_usize;
        for (index, token) in self.tokens.iter().enumerate().skip(start) {
            if token.kind == open {
                depth += 1;
            } else if token.kind == close {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index + 1);
                }
            } else if token.kind == TokenKind::Eof {
                return None;
            }
        }
        None
    }

    /// Consume GNU or bracketed attribute specifiers at the cursor.
    ///
    /// Each attribute is kept as written, with arguments as text, and is
    /// labelled with the placement the caller is parsing, since placement is
    /// what decides attachment. Nothing is applied here; callers that own a
    /// type, tag or member apply `aligned` and `packed` and leave `applied`
    /// set so the dump and the inventory can tell the two apart.
    pub(super) fn parse_attribute_specifiers(
        &mut self,
        placement: AttributePlacement,
    ) -> Vec<Attribute> {
        self.parse_attribute_specifiers_in(placement, false)
    }

    /// `type_name_suffix` marks the attributes after the abstract declarator
    /// of a type name. GCC 14 and Apple clang refuse that syntax, so an x86
    /// calling convention keeps E2952 there (EvoIR 10.1.9).
    pub(super) fn parse_attribute_specifiers_in(
        &mut self,
        placement: AttributePlacement,
        type_name_suffix: bool,
    ) -> Vec<Attribute> {
        let is_attribute_name = |text: &str| {
            let mut chars = text.chars();
            chars
                .next()
                .is_some_and(|c| c == '_' || c == '$' || c.is_alphabetic())
                && chars.all(|c| c == '_' || c == '$' || c.is_alphanumeric())
        };
        let mut attributes = Vec::new();
        let mut saw_gnu = false;
        while self.attribute_starts_at(self.pos) {
            let bracketed = self.bracket_attribute_starts_at(self.pos);
            let keyword = self.peek().expect("peeked attribute start").clone();
            if bracketed && (saw_gnu || placement == AttributePlacement::BeforeDeclarator) {
                self.report("E2903", "malformed-attribute", keyword.span);
            }
            if !bracketed {
                saw_gnu = true;
                self.advance();
            }
            let (open_kind, close_kind) = if bracketed {
                (TokenKind::LBracket, TokenKind::RBracket)
            } else {
                (TokenKind::LParen, TokenKind::RParen)
            };
            // `expect` reports its own diagnostic, and the first diagnostic is
            // the one a reader sees, so the delimiters are checked by hand to
            // keep the report on the attribute rather than on a stray token.
            for _ in 0..2 {
                if self.peek_kind() != Some(open_kind) {
                    self.report("E2903", "malformed-attribute", keyword.span);
                    return attributes;
                }
                self.advance();
            }
            loop {
                // GNU permits empty entries: `__attribute__(())` and `,,`.
                if self.match_kind(TokenKind::Comma) {
                    continue;
                }
                if self.peek_kind() == Some(close_kind) {
                    break;
                }
                let Some(name_token) = self.advance() else {
                    self.report("E2903", "malformed-attribute", keyword.span);
                    return attributes;
                };
                // Attribute names may be keywords (`const`, `noreturn`), so any
                // single token is accepted as a name and read by its text.
                if matches!(name_token.kind, TokenKind::LParen | TokenKind::Comma) {
                    self.report("E2903", "malformed-attribute", name_token.span);
                    return attributes;
                }
                let mut span = name_token.span;
                let mut written = self.token_text(&name_token).to_owned();
                let mut name = Attribute::canonical_name(&written);
                let mut namespace = None;
                if bracketed {
                    if !is_attribute_name(&written) {
                        self.report("E2903", "malformed-attribute", name_token.span);
                        return attributes;
                    }
                    if self.match_kind(TokenKind::Colon) {
                        if !self.match_kind(TokenKind::Colon) {
                            self.report("E2903", "malformed-attribute", name_token.span);
                            return attributes;
                        }
                        let Some(member) = self.advance() else {
                            self.report("E2903", "malformed-attribute", name_token.span);
                            return attributes;
                        };
                        let member_text = self.token_text(&member);
                        if !is_attribute_name(member_text) {
                            self.report("E2903", "malformed-attribute", member.span);
                            return attributes;
                        }
                        name = Attribute::canonical_name(member_text);
                        namespace = Some(written.clone());
                        written = format!("{written}::{member_text}");
                        span = self.merge_span(name_token.span, member.span);
                    }
                }
                let syntax = if bracketed {
                    AttributeSyntax::Brackets { namespace }
                } else {
                    AttributeSyntax::Gnu
                };
                let gnu_name = matches!(&syntax, AttributeSyntax::Gnu)
                    || matches!(&syntax, AttributeSyntax::Brackets { namespace: Some(ns) } if matches!(ns.as_str(), "gnu" | "__gnu__"));
                if self.target_layout.names_clang_extensions()
                    && name != "internal_linkage"
                    && darwin_refuses_attribute(&name)
                {
                    self.report("E2967", "unsupported-darwin-attribute", name_token.span);
                }
                // These attributes change the execution or call contract, not
                // just optimization advice. In particular, a naked asm body
                // cannot return through our ordinary frame. EvoIR 10.1.9
                // accepts one on a function declaration that is not a
                // definition and records it; the definition and the uses of
                // that function are checked when the unit is complete. Every
                // other placement is refused here: a type, a typedef or an
                // object must not hide an unsupported convention.
                // Apple clang moves such an attribute after the `*` of a
                // function's result type to the function, as Clang's
                // `<arm_acle.h>` writes `void *__attribute__((target("gcs")))
                // __gcsss(void *)`; the declaration settles it.
                let declaration_position = matches!(
                    placement,
                    AttributePlacement::DeclarationSpecifiers
                        | AttributePlacement::BeforeDeclarator
                        | AttributePlacement::AfterDeclarator
                        | AttributePlacement::FunctionDefinition
                ) || (placement
                    == AttributePlacement::PointerDeclarator
                    && self.target_layout.names_clang_extensions());
                let darwin = self.target_layout.names_clang_extensions();
                // EvoIR 10.1.9: an x86 calling convention has no AArch64
                // meaning. It is W0011 or E2952 after its arguments, below.
                let x86 = gnu_name && ignored_x86_convention(&name, darwin);
                let mut convention = convention_attribute(&name)
                    .filter(|(attribute, _)| gnu_name && *attribute != "optimize" && !x86);
                let preceding = self.diagnostics.len();
                if let Some((_, message)) = convention.filter(|_| !declaration_position) {
                    self.report("E2952", message, name_token.span);
                }
                // Clang's `overloadable` changes the symbol name and lets
                // declarations of one name differ in type (EvoIR 10.1.9).
                let clang_name = gnu_name
                    || matches!(&syntax, AttributeSyntax::Brackets { namespace: Some(ns) } if matches!(ns.as_str(), "clang" | "_Clang"));
                if clang_name
                    && name == "overloadable"
                    && self.target_layout.names_clang_extensions()
                {
                    self.report(
                        "E2964",
                        "unsupported-overloadable-attribute",
                        name_token.span,
                    );
                }
                if gnu_name && name == "scalar_storage_order" {
                    self.defer_profile_refusal(
                        "E2940",
                        "unsupported-scalar-storage-order",
                        name_token.span,
                    );
                }
                let mut arguments = Vec::new();
                if self.peek_kind() == Some(TokenKind::LParen) {
                    let open = self.advance().expect("peeked argument list");
                    let parsed = if gnu_name
                        && matches!(
                            name.as_str(),
                            "aligned"
                                | "vector_size"
                                | "neon_vector_type"
                                | "neon_polyvector_type"
                                | "constructor"
                                | "destructor"
                                | "patchable_function_entry"
                        )
                        && self.peek_kind() != Some(TokenKind::RParen)
                    {
                        self.parse_integer_constant_arguments(
                            open.span,
                            if name == "patchable_function_entry" {
                                2
                            } else {
                                1
                            },
                        )
                    } else if gnu_name && name == "optimize" {
                        self.parse_optimize_arguments(open.span)
                    } else {
                        self.parse_attribute_arguments(open.span)
                    };
                    match parsed {
                        Some((parsed, close)) => {
                            arguments = parsed;
                            span = self.merge_span(name_token.span, close);
                        }
                        None => return attributes,
                    }
                }
                // EvoIR 10.1.9: an optimization level and the listed disabling
                // options select contracts that this compiler already has.
                // Inspect every option after parsing: one permitted option
                // must not hide a semantic request such as `wrapv`.
                if gnu_name && name == "optimize" && !optimize_options_are_satisfied(&arguments) {
                    convention = convention_attribute("optimize");
                    if !declaration_position {
                        self.report(
                            "E2952",
                            "unsupported-function-attribute: optimize",
                            name_token.span,
                        );
                    }
                }
                // EvoIR 10.1.9: GCC 14 and Apple clang ignore an x86 calling
                // convention with a warning, and so does EvoCC (W0011). The
                // function, its type and each call stay base AAPCS64. E2952
                // stays where a reference refuses the form: both refuse an
                // attribute after the abstract declarator of a type name, and
                // Apple clang refuses a convention that it knows on a
                // statement or with an argument.
                if x86 {
                    if type_name_suffix
                        || (darwin
                            && apple_clang_x86_convention(&name, bracketed)
                            && (placement == AttributePlacement::NullStatement
                                || !arguments.is_empty()))
                    {
                        self.report(
                            "E2952",
                            format!("unsupported-function-attribute: {name}"),
                            name_token.span,
                        );
                    } else {
                        self.lint_findings.push(evocc_chir::lints::Finding {
                            lint: 10,
                            span: name_token.span,
                        });
                    }
                }
                if bracketed
                    && gnu_name
                    && ((placement == AttributePlacement::TagClosingBrace
                        && matches!(name.as_str(), "packed" | "aligned"))
                        || (placement == AttributePlacement::TypeName
                            && matches!(name.as_str(), "packed" | "aligned"))
                        || (matches!(
                            placement,
                            AttributePlacement::PointerDeclarator | AttributePlacement::ArrayType
                        ) && name == "aligned")
                        || (matches!(
                            placement,
                            AttributePlacement::Label | AttributePlacement::NullStatement
                        ) && name == "unused"))
                {
                    self.report("E2903", "malformed-attribute", name_token.span);
                }
                if let Some((attribute, _)) = convention.filter(|_| declaration_position) {
                    self.convention_attributes.push(ConventionAttribute {
                        attribute,
                        span,
                        name: name_token.span,
                        preceding,
                        state: ConventionState::Open,
                    });
                }
                attributes.push(Attribute {
                    name,
                    written,
                    syntax,
                    arguments,
                    placement,
                    span,
                    applied: None,
                });
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
            }
            for _ in 0..2 {
                if self.peek_kind() != Some(close_kind) {
                    self.report("E2903", "malformed-attribute", keyword.span);
                    return attributes;
                }
                self.advance();
            }
        }
        attributes
    }

    /// Read a comma-separated argument list up to its closing parenthesis,
    /// keeping each argument as the exact source text it spans. Nested
    /// parentheses are balanced so `format(printf, 1, 2)` and
    /// `aligned(sizeof(long))` are one argument each where they should be.
    pub(super) fn parse_attribute_arguments(
        &mut self,
        open: SourceSpan,
    ) -> Option<(Vec<AttributeArgument>, SourceSpan)> {
        let mut arguments = Vec::new();
        let mut depth = 0_usize;
        let mut argument_start: Option<usize> = None;
        loop {
            let Some(kind) = self.peek_kind() else {
                self.report("E2903", "malformed-attribute", open);
                return None;
            };
            match kind {
                TokenKind::RParen if depth == 0 => {
                    let close = self.advance().expect("peeked close").span;
                    if let Some(start) = argument_start {
                        arguments.push(self.attribute_argument(&self.tokens[start..self.pos - 1]));
                    } else if !arguments.is_empty() {
                        // `aligned(4,)` -- a trailing separator with nothing after it.
                        self.report("E2903", "malformed-attribute", close);
                        return None;
                    }
                    return Some((arguments, close));
                }
                TokenKind::Comma if depth == 0 => {
                    let comma = self.advance().expect("peeked comma").span;
                    match argument_start {
                        Some(start) => arguments
                            .push(self.attribute_argument(&self.tokens[start..self.pos - 1])),
                        None => {
                            self.report("E2903", "malformed-attribute", comma);
                            return None;
                        }
                    }
                    argument_start = None;
                }
                TokenKind::Semicolon | TokenKind::LBrace | TokenKind::RBrace => {
                    // An argument list cannot contain these; the list was never
                    // closed. Report at the opening parenthesis, where the
                    // reader will look.
                    self.report("E2903", "malformed-attribute", open);
                    return None;
                }
                _ => {
                    let token_index = self.pos;
                    let token = self.advance().expect("peeked argument token");
                    if token.text.has_raw_bytes() {
                        self.report(
                            "E0002",
                            "invalid encoding in attribute argument",
                            token.span,
                        );
                        return None;
                    }
                    match token.kind {
                        TokenKind::LParen => depth += 1,
                        TokenKind::RParen => depth -= 1,
                        _ => {}
                    }
                    argument_start.get_or_insert(token_index);
                }
            }
        }
    }

    /// Read a bounded list of integer constant expressions in place, so type
    /// names and `sizeof` resolve in the surrounding scope; the source text is
    /// kept alongside the value.
    pub(super) fn parse_integer_constant_arguments(
        &mut self,
        open: SourceSpan,
        maximum: usize,
    ) -> Option<(Vec<AttributeArgument>, SourceSpan)> {
        let mut arguments = Vec::new();
        loop {
            if matches!(self.peek_kind(), Some(TokenKind::Comma) | None) {
                // `aligned(,)`: an argument slot with nothing in it is a malformed
                // attribute, reported as such rather than as a missing expression.
                self.report("E2903", "malformed-attribute", self.span());
                return None;
            }
            let argument_start = self.pos;
            let expression = self.parse_required_constant_conditional()?;
            let mut argument = self.attribute_argument(&self.tokens[argument_start..self.pos]);
            argument.integer = self
                .eval_integer_constant_expression(expression)
                .ok()
                .and_then(CInteger::try_mathematical_value)
                .and_then(|value| u64::try_from(value).ok());
            if argument.integer.is_none() {
                self.report("E2903", "malformed-attribute", argument.span);
                return None;
            }
            arguments.push(argument);
            if self.peek_kind() == Some(TokenKind::Comma) && arguments.len() < maximum {
                self.advance();
                continue;
            }
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.report("E2903", "malformed-attribute", open);
                return None;
            }
            let close = self.advance().expect("peeked close").span;
            return Some((arguments, close));
        }
    }

    /// Read the arguments of `optimize` in place, as GCC reads them: each one
    /// is a constant expression. The text is kept, so a string argument
    /// keeps its written form. An integer constant expression also records
    /// its value.
    pub(super) fn parse_optimize_arguments(
        &mut self,
        open: SourceSpan,
    ) -> Option<(Vec<AttributeArgument>, SourceSpan)> {
        let mut arguments = Vec::new();
        if self.peek_kind() == Some(TokenKind::RParen) {
            let close = self.advance().expect("peeked close").span;
            return Some((arguments, close));
        }
        loop {
            // An empty slot, such as `optimize(,1)` or `optimize(1,)`, is a
            // malformed attribute, as in the other argument lists.
            if matches!(
                self.peek_kind(),
                Some(TokenKind::Comma | TokenKind::RParen) | None
            ) {
                self.report("E2903", "malformed-attribute", self.span());
                return None;
            }
            let argument_start = self.pos;
            let expression = self.parse_required_constant_conditional()?;
            let mut argument = self.attribute_argument(&self.tokens[argument_start..self.pos]);
            argument.integer = self
                .eval_integer_constant_expression(expression)
                .ok()
                .and_then(CInteger::try_mathematical_value)
                .and_then(|value| u64::try_from(value).ok());
            arguments.push(argument);
            match self.peek_kind() {
                Some(TokenKind::Comma) => {
                    self.advance();
                }
                Some(TokenKind::RParen) => {
                    let close = self.advance().expect("peeked close").span;
                    return Some((arguments, close));
                }
                _ => {
                    self.report("E2903", "malformed-attribute", open);
                    return None;
                }
            }
        }
    }

    pub(super) fn attribute_argument(&self, tokens: &[Token]) -> AttributeArgument {
        let first = tokens.first().expect("attribute argument is non-empty");
        let last = tokens.last().expect("attribute argument is non-empty");
        let mut text = String::new();
        for (index, token) in tokens.iter().enumerate() {
            if index > 0 && token.preceded_by_whitespace {
                text.push(' ');
            }
            text.push_str(self.token_text(token));
        }
        AttributeArgument {
            text,
            span: self.merge_span(first.span, last.span),
            integer: None,
        }
    }

    /// Resolve `aligned` to bytes, or report why it cannot be.
    ///
    /// GNU C bounds the argument to a power of two no larger than 2^28; the
    /// target layout represents any power of two up to that, so the two
    /// limits coincide here. Bare `aligned` is the largest fundamental
    /// alignment, sixteen bytes on AArch64.
    pub(super) fn aligned_bytes(&mut self, attribute: &Attribute) -> Option<u32> {
        const LARGEST_FUNDAMENTAL_ALIGNMENT: u32 = 16;
        const GNU_MAXIMUM_ALIGNMENT: u64 = 1 << 28;
        match attribute.arguments.as_slice() {
            [argument] if attribute.written == "_Alignas" && argument.integer == Some(0) => None,
            [] => Some(LARGEST_FUNDAMENTAL_ALIGNMENT),
            [argument] => match argument.integer {
                Some(bytes)
                    if bytes != 0 && bytes.is_power_of_two() && bytes <= GNU_MAXIMUM_ALIGNMENT =>
                {
                    u32::try_from(bytes).ok()
                }
                _ => {
                    self.report("E2903", "malformed-attribute", argument.span);
                    None
                }
            },
            [_, second, ..] => {
                self.report("E2903", "malformed-attribute", second.span);
                None
            }
        }
    }

    pub(super) fn apply_declaration_attributes(
        &mut self,
        ty: TypeId,
        attributes: &mut [Attribute],
        typedef: bool,
    ) -> TypeId {
        if typedef {
            self.validate_alignment_specifiers(ty, attributes, false);
            return self.apply_attributes_to_type(ty, attributes);
        }
        if matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Function { .. })
        ) {
            self.validate_alignment_specifiers(ty, attributes, false);
        }
        let mut ty = ty;
        for attribute in attributes {
            if attribute.semantic_name() == Some("aligned") {
                attribute.applied = self.aligned_bytes(attribute).map(AppliedAttribute::Aligned);
            } else {
                ty = self.apply_attributes_to_type(ty, std::slice::from_mut(attribute));
            }
        }
        ty
    }

    /// Apply integer representation and alignment attributes to a type,
    /// yielding the (possibly re-interned) type and
    /// marking the attribute applied. `packed` on a record type is forwarded to
    /// the tag; on anything else it is preserved without effect, which is what
    /// GNU C does too.
    pub(super) fn apply_attributes_to_type(
        &mut self,
        ty: TypeId,
        attributes: &mut [Attribute],
    ) -> TypeId {
        let mut ty = ty;
        for attribute in attributes.iter_mut() {
            match attribute.semantic_name().unwrap_or("") {
                "vector_size" => {
                    let vector = attribute
                        .arguments
                        .first()
                        .and_then(|argument| argument.integer)
                        .filter(|_| attribute.arguments.len() == 1)
                        .and_then(|bytes| {
                            self.apply_vector_size(
                                ty,
                                bytes,
                                attribute.applied == Some(AppliedAttribute::VectorSize),
                            )
                        });
                    let Some(vector) = vector else {
                        self.report("E2903", "malformed-attribute", attribute.span);
                        continue;
                    };
                    ty = vector;
                    attribute.applied = Some(AppliedAttribute::VectorSize);
                }
                "neon_vector_type" | "neon_polyvector_type"
                    if self.target_layout.names_clang_extensions() =>
                {
                    let vector = attribute
                        .arguments
                        .first()
                        .and_then(|argument| argument.integer)
                        .filter(|_| attribute.arguments.len() == 1)
                        .and_then(|lanes| {
                            self.apply_neon_vector(
                                ty,
                                lanes,
                                attribute.semantic_name() == Some("neon_polyvector_type"),
                                attribute.applied.is_some(),
                            )
                        });
                    let Some(vector) = vector else {
                        self.report("E2903", "malformed-neon-vector-attribute", attribute.span);
                        continue;
                    };
                    ty = vector;
                    attribute.applied = Some(AppliedAttribute::VectorSize);
                }
                "mode" => ty = self.apply_integer_mode(ty, attribute),
                "transparent_union" => {
                    if let Some(TypeKind::Union(tag)) = self.types.get(ty).map(|ty| ty.kind.clone())
                    {
                        self.apply_transparent_union(tag, attribute);
                    } else {
                        self.report("E2903", "malformed-attribute", attribute.span);
                    }
                }
                "aligned" => {
                    let Some(bytes) = self.aligned_bytes(attribute) else {
                        continue;
                    };
                    attribute.applied = Some(AppliedAttribute::Aligned(bytes));
                    let canonical = self
                        .types
                        .get(ty)
                        .expect("attributed types are interned")
                        .clone();
                    let raised = canonical
                        .qualifiers
                        .aligned
                        .map_or(bytes, |current| current.max(bytes));
                    let qualifiers = Qualifiers {
                        aligned: Some(raised),
                        ..canonical.qualifiers
                    };
                    ty = self.intern(canonical.kind, qualifiers);
                }
                "packed" => {
                    if !attribute.arguments.is_empty() {
                        self.report("E2903", "malformed-attribute", attribute.span);
                        continue;
                    }
                    let tag = match self.types.get(ty).map(|canonical| &canonical.kind) {
                        Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => Some(*tag),
                        _ => None,
                    };
                    if let Some(tag) = tag {
                        self.types.set_tag_layout(
                            tag,
                            TagLayoutAttributes {
                                packed: true,
                                aligned: None,
                                max_align: None,
                            },
                        );
                        attribute.applied = Some(AppliedAttribute::Packed);
                    }
                }
                _ => {}
            }
        }
        if let Some(TypeKind::Union(tag)) = self.types.get(ty).map(|ty| &ty.kind) {
            if self
                .types
                .tag(*tag)
                .is_some_and(|tag| tag.transparent_union)
                && attributes.last().is_some()
            {
                let target = self.target_layout;
                let first = match &self.types.tag(*tag).unwrap().state {
                    TagState::Complete(TagDefinition::Union(fields)) => fields.first(),
                    _ => None,
                };
                if first.is_some_and(|field| {
                    target.layout_of(&self.types, field.ty).ok()
                        != target.layout_of(&self.types, ty).ok()
                }) {
                    self.report(
                        "E2903",
                        "malformed-attribute",
                        attributes.last().unwrap().span,
                    );
                }
            }
        }
        ty
    }

    pub(super) fn apply_integer_mode(&mut self, ty: TypeId, attribute: &mut Attribute) -> TypeId {
        let Some(canonical) = self.types.get(ty).cloned() else {
            return ty;
        };
        let source = match canonical.kind {
            TypeKind::Int(rank) if rank != IntRank::Bool => Some(integer_type_for_rank(rank)),
            TypeKind::GnuInt128(GnuInt128Signedness::Signed) => Some(CIntegerType::INT128),
            TypeKind::GnuInt128(GnuInt128Signedness::Unsigned) => Some(CIntegerType::UINT128),
            _ => None,
        };
        let selected = attribute.arguments.first().and_then(|argument| {
            let written = argument.text.trim();
            let name = written
                .strip_prefix("__")
                .and_then(|name| name.strip_suffix("__"))
                .unwrap_or(written);
            match (name, source?.is_signed()) {
                ("QI" | "byte", true) => Some(TypeKind::Int(IntRank::SChar)),
                ("QI" | "byte", false) => Some(TypeKind::Int(IntRank::UChar)),
                ("HI", true) => Some(TypeKind::Int(IntRank::Short)),
                ("HI", false) => Some(TypeKind::Int(IntRank::UShort)),
                ("SI", true) => Some(TypeKind::Int(IntRank::Int)),
                ("SI", false) => Some(TypeKind::Int(IntRank::UInt)),
                ("DI" | "word", true) => Some(TypeKind::Int(IntRank::Long)),
                ("DI" | "word", false) => Some(TypeKind::Int(IntRank::ULong)),
                ("TI", true) => Some(TypeKind::GnuInt128(GnuInt128Signedness::Signed)),
                ("TI", false) => Some(TypeKind::GnuInt128(GnuInt128Signedness::Unsigned)),
                _ => None,
            }
        });
        if attribute.arguments.len() != 1 || attribute.placement == AttributePlacement::TypeName {
            self.report("E2903", "malformed-attribute", attribute.span);
            return ty;
        }
        let Some(kind) = selected else {
            self.report("E2903", "malformed-attribute", attribute.span);
            return ty;
        };
        let resolved = self.intern(kind, canonical.qualifiers);
        attribute.applied = Some(AppliedAttribute::IntegerMode);
        resolved
    }

    /// Pointer type attributes are never applied, but an `aligned`
    /// argument must still satisfy the attribute grammar's representability
    /// and power-of-two constraint. Symbol linkage is handled separately.
    pub(super) fn validate_pointer_attributes(&mut self, attributes: &[Attribute]) {
        for attribute in attributes {
            if attribute.semantic_name() == Some("aligned") {
                let _ = self.aligned_bytes(attribute);
            }
        }
    }

    /// Apply `packed` and `aligned` written on a tag itself.
    pub(super) fn merge_tag_availability(&mut self, tag: TagId, attributes: &[Attribute]) {
        if self.availability_target.is_some() && !attributes.is_empty() {
            let previous = self.tag_availability.get(&tag).cloned().unwrap_or_default();
            let merged = self.merged_availability(previous, attributes);
            self.tag_availability.insert(tag, merged);
        }
    }

    pub(super) fn tag_unavailable(&self, tag: TagId) -> Option<String> {
        let deployment = self.availability_target?;
        self.tag_availability.get(&tag)?.unavailable(deployment)
    }

    pub(super) fn apply_attributes_to_tag(&mut self, tag: TagId, attributes: &mut [Attribute]) {
        let fixed_enumeration = self
            .types
            .tag(tag)
            .is_some_and(|tag| tag.fixed_underlying.is_some());
        for attribute in attributes.iter_mut() {
            match attribute.semantic_name().unwrap_or("") {
                "transparent_union" => self.apply_transparent_union(tag, attribute),
                "packed" => {
                    if !attribute.arguments.is_empty() {
                        self.report("E2903", "malformed-attribute", attribute.span);
                        continue;
                    }
                    // GCC 14 and Apple clang keep the fixed underlying type.
                    if fixed_enumeration {
                        continue;
                    }
                    let conflicts = self.types.tag(tag).is_some_and(|definition| {
                        definition.kind == TagKind::Enum && definition.layout.aligned.is_some()
                    });
                    if conflicts {
                        self.report("E2903", "malformed-attribute", attribute.span);
                        continue;
                    }
                    self.types.set_tag_layout(
                        tag,
                        TagLayoutAttributes {
                            packed: true,
                            aligned: None,
                            max_align: None,
                        },
                    );
                    attribute.applied = Some(AppliedAttribute::Packed);
                }
                "aligned" => {
                    if let Some(bytes) = self.aligned_bytes(attribute) {
                        // GCC 14 ignores it on a fixed enumeration, and Apple
                        // clang raises the alignment.
                        let conflicts = fixed_enumeration
                            || self.types.tag(tag).is_some_and(|definition| {
                                definition.kind == TagKind::Enum && definition.layout.packed
                            });
                        if conflicts {
                            self.report("E2903", "malformed-attribute", attribute.span);
                            continue;
                        }
                        self.types.set_tag_layout(
                            tag,
                            TagLayoutAttributes {
                                packed: false,
                                aligned: Some(bytes),
                                max_align: None,
                            },
                        );
                        attribute.applied = Some(AppliedAttribute::Aligned(bytes));
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn apply_transparent_union(&mut self, tag: TagId, attribute: &mut Attribute) {
        if !attribute.arguments.is_empty() || !self.types.set_transparent_union(tag) {
            self.report("E2903", "malformed-attribute", attribute.span);
            return;
        }
        // Tag-keyword attributes arrive before the fields are completed.
        if matches!(
            self.types.tag(tag).map(|tag| &tag.state),
            Some(TagState::Complete(_))
        ) && !self.transparent_union_has_register_representation(tag)
        {
            self.report("E2903", "malformed-attribute", attribute.span);
            return;
        }
        attribute.applied = Some(AppliedAttribute::TransparentUnion);
    }

    pub(super) fn transparent_union_has_register_representation(&self, tag: TagId) -> bool {
        let Some(TagState::Complete(TagDefinition::Union(fields))) =
            self.types.tag(tag).map(|tag| &tag.state)
        else {
            return false;
        };
        let target = self.target_layout;
        let Some(first) = fields
            .first()
            .and_then(|field| target.layout_of(&self.types, field.ty).ok())
        else {
            return false;
        };
        let layout = self.types.tag(tag).unwrap().layout;
        if (layout.packed && first.align_bytes > 1)
            || layout
                .aligned
                .is_some_and(|align| align > first.align_bytes)
            || layout.max_align.is_some_and(|cap| cap < first.align_bytes)
        {
            return false;
        }
        fields.iter().all(|field| {
            field.bit_field.is_none()
                && self.types.get(field.ty).is_some_and(|ty| {
                    matches!(
                        ty.kind,
                        TypeKind::Pointer(_) | TypeKind::Int(_) | TypeKind::GnuInt128(_)
                    )
                })
                && target
                    .layout_of(&self.types, field.ty)
                    .is_ok_and(|layout| layout == first)
        })
    }

    /// The layout attributes a member carries, with `applied` marked.
    pub(super) fn member_layout_attributes(
        &mut self,
        attributes: &mut [Attribute],
    ) -> FieldLayoutAttributes {
        let mut layout = FieldLayoutAttributes::default();
        for attribute in attributes.iter_mut() {
            match attribute.semantic_name().unwrap_or("") {
                "packed" => {
                    layout.packed = true;
                    attribute.applied = Some(AppliedAttribute::Packed);
                }
                "aligned" => {
                    if let Some(bytes) = self.aligned_bytes(attribute) {
                        layout.aligned =
                            Some(layout.aligned.map_or(bytes, |current| current.max(bytes)));
                        attribute.applied = Some(AppliedAttribute::Aligned(bytes));
                    }
                }
                _ => {}
            }
        }
        layout
    }

    pub(super) fn parse_declaration_modifiers(
        &mut self,
        storage: &mut Option<StorageClass>,
        inline_spans: &mut Vec<SourceSpan>,
        attributes: &mut Vec<Attribute>,
    ) {
        loop {
            if self.capture_specifier_attribute(&mut Some(&mut *attributes)) {
                continue;
            }
            if self.peek_kind() == Some(TokenKind::Inline) {
                inline_spans.push(self.advance().expect("peeked inline specifier").span);
                continue;
            }
            if self.capture_storage_specifier(&mut Some(&mut *storage)) {
                continue;
            }
            break;
        }
    }
}
