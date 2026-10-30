//! Parser configuration.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub fn new(tokens: Vec<Token>, file: FileId, _source: &'src str) -> Self {
        Self::from_tokens(tokens, file)
    }

    /// Token spellings, not source buffers, are the parser's text authority.
    pub fn from_tokens(mut tokens: Vec<Token>, file: FileId) -> Self {
        // Preprocessing numbers and `other` characters remain available to
        // macro expansion through translation phase 4. If either survives to
        // this phase-7 boundary, it has no C-token form and is diagnosed at
        // the exact preserved token rather than leaking into syntax errors.
        let lexical_diagnostic = lexical_diagnostic(&tokens);
        // GNU contextual spellings must remain identifiers through
        // preprocessing so they can be macro-expanded or token-pasted. Turn
        // an exact surviving spelling into its parser token only at this
        // boundary.
        for token in &mut tokens {
            let spelling = match &token.text {
                TokenText::Decoded(text) => Some(text.as_str()),
                TokenText::Borrowed | TokenText::RawBytes { .. } => None,
            };
            if token.kind == TokenKind::Identifier {
                token.kind = match spelling {
                    Some("__signed" | "__signed__") => TokenKind::Signed,
                    Some("__const" | "__const__") => TokenKind::Const,
                    Some("__inline" | "__inline__") => TokenKind::Inline,
                    Some("__restrict" | "__restrict__") => TokenKind::Restrict,
                    Some("__extension__") => TokenKind::GnuExtension,
                    Some("__builtin_va_list") => TokenKind::BuiltinVaList,
                    Some(name)
                        if TargetBuiltinType::from_spelling(name).is_some()
                            || evocc_types::neon::BuiltinType::from_spelling(name).is_some() =>
                    {
                        TokenKind::TargetBuiltinType
                    }
                    Some("__int128" | "__int128_t" | "__uint128_t") => TokenKind::GnuInt128,
                    Some("__alignof" | "__alignof__") => TokenKind::GnuAlignof,
                    Some("__auto_type") => TokenKind::AutoType,
                    Some("__label__") => TokenKind::LocalLabel,
                    Some("asm" | "__asm" | "__asm__") => TokenKind::Asm,
                    Some("__volatile" | "__volatile__") => TokenKind::Volatile,
                    Some(name) => builtins::token_kind(name).unwrap_or(TokenKind::Identifier),
                    None => TokenKind::Identifier,
                };
            }
        }
        let symbols = SymbolTable::new();
        let current_scope = symbols.file_scope();
        Self {
            tokens,
            general_registers_only: false,
            pos: 0,
            file,
            _source_lifetime: std::marker::PhantomData,
            had_error: lexical_diagnostic.is_some(),
            error_count: usize::from(lexical_diagnostic.is_some()),
            error_limit: None,
            standard_builtins: true,
            disabled_builtins: HashSet::new(),
            legacy_declarations: false,
            lint_declarations: false,
            lint_uses: HashSet::new(),
            lint_findings: Vec::new(),
            pointer_lint_findings: std::cell::RefCell::new(Vec::new()),
            gnu_inline: false,
            rollback_error_count: usize::from(lexical_diagnostic.is_some()),
            diagnostics: lexical_diagnostic.into_iter().collect(),
            arena: None,
            target_layout: TargetDataLayout::aarch64_unknown_linux_gnu(),
            availability_target: None,
            symbol_availability: HashMap::new(),
            tag_availability: HashMap::new(),
            member_availability: HashMap::new(),
            availability_frames: Vec::new(),
            types: TypeInterner::new(),
            symbols,
            current_scope,
            temporary_parameter_shadows: Vec::new(),
            inherited_function_parameters: HashMap::new(),
            entity_attributes: HashMap::new(),
            member_attributes: HashMap::new(),
            entities: HashMap::new(),
            external_entities: HashMap::new(),
            entity_undo: Vec::new(),
            external_entity_undo: Vec::new(),
            inherited_function_parameter_undo: Vec::new(),
            entity_attribute_undo: Vec::new(),
            member_attribute_undo: Vec::new(),
            initialized_const_integers: HashMap::new(),
            initialized_const_undo: Vec::new(),
            initialized_strings: HashMap::new(),
            initialized_string_undo: Vec::new(),
            string_constant_uses: Vec::new(),
            declaration_transaction_depth: 0,
            side_declarations: Vec::new(),
            builtin_declarations: Vec::new(),
            neon_tuple_types: HashMap::new(),
            unprototyped_calls: Vec::new(),
            old_style_definitions: Vec::new(),
            prototype_scopes: Vec::new(),
            standard_allocation_calls: Vec::new(),
            loop_depth: 0,
            current_function_parameters: None,
            profile_refusals: Vec::new(),
            detached_attributes: Vec::new(),
            convention_attributes: Vec::new(),
            function_uses: Vec::new(),
            function_definitions: Vec::new(),
            generic_selections: Vec::new(),
            front_end_builtins: Vec::new(),
            switch_depth: 0,
            switch_cases: Vec::new(),
            local_label_scopes: Vec::new(),
            next_local_label: 0,
            pack_alignment: None,
            pack_stack: Vec::new(),
            visibility_stack: Vec::new(),
            fenv_access: false,
            flexible_initializer_allowed: false,
            flexible_initializer_extent: None,
        }
    }

    /// Stop parsing after this many diagnostics have been committed.
    ///
    /// Parsers are unlimited by default so library users retain structural
    /// recovery. Configure a limit before calling [`Self::parse_translation_unit_in`]
    /// when only a bounded diagnostic prefix is required.
    pub fn set_error_limit(&mut self, limit: NonZeroUsize) {
        self.error_limit = Some(limit);
    }

    /// Select target-dependent literal types before parsing the unit.
    pub fn set_target_layout(&mut self, layout: TargetDataLayout) {
        self.types.set_signed_char(layout.plain_char_is_signed());
        self.target_layout = layout;
        if !layout.accepts_bfloat16_constants() {
            self.refuse_bfloat16_constants();
        }
        if layout.provides_clang_atomic_builtins() {
            self.promote_clang_atomic_builtins();
        }
    }

    /// A per-function target attribute cannot re-enable registers that this
    /// compilation excludes. Set this together with the target layout.
    pub fn set_general_registers_only(&mut self, value: bool) {
        self.general_registers_only = value;
    }

    /// Give Clang's availability attributes the meaning of Apple clang for
    /// the macOS deployment target `version` (EvoIR 10.1.9). A version in
    /// an `availability` attribute, such as `10.14.4`, is no stray token.
    pub fn set_availability_target(&mut self, version: availability::Version) {
        debug_assert_eq!(self.pos, 0, "the target is selected before parsing");
        self.availability_target = Some(version);
        if !self.diagnostics.is_empty() {
            // Only the lexical diagnostic precedes parsing.
            let versions = availability_version_tokens(&self.tokens);
            self.diagnostics = lexical_diagnostic_except(&self.tokens, &versions)
                .into_iter()
                .collect();
            let refused = usize::from(!self.diagnostics.is_empty());
            self.had_error = refused != 0;
            self.error_count = refused;
            self.rollback_error_count = refused;
        }
    }

    /// Apple clang's `<stdatomic.h>` calls Clang's `__c11_atomic_*`
    /// builtins, so on such a target their spellings are builtin tokens
    /// (EvoIR 10.1.21). Elsewhere they stay identifiers, as in GCC 14.
    pub(super) fn promote_clang_atomic_builtins(&mut self) {
        debug_assert_eq!(self.pos, 0, "the target is selected before parsing");
        for token in &mut self.tokens {
            if token.kind != TokenKind::Identifier {
                continue;
            }
            if let Some(kind) = token.text.spelling().and_then(builtins::clang_atomic_token) {
                token.kind = kind;
            }
        }
    }

    /// Apple clang rejects the `bf16` and `BF16` constant suffixes, so on
    /// such a target a constant with one is a preprocessing number without a
    /// C token form, and the first stray token is `E0112` (EvoIR 3.13.6).
    pub(super) fn refuse_bfloat16_constants(&mut self) {
        debug_assert_eq!(self.pos, 0, "the target is selected before parsing");
        let mut refused = false;
        for token in &mut self.tokens {
            if token.kind == TokenKind::FloatLiteral
                && token
                    .text
                    .spelling()
                    .and_then(evocc_lex::half_floating_suffix)
                    .is_some_and(|(_, bfloat)| bfloat)
            {
                token.kind = TokenKind::PpNumber;
                refused = true;
            }
        }
        if refused {
            // Only the lexical diagnostic precedes parsing; the first stray
            // token in source order replaces it.
            self.diagnostics = lexical_diagnostic(&self.tokens).into_iter().collect();
            self.had_error = true;
            self.error_count = 1;
            self.rollback_error_count = 1;
        }
    }

    /// Control recognition of ordinary externally linked standard names.
    /// Explicit `__builtin_` spellings are unaffected.
    pub fn set_standard_builtins(&mut self, enabled: bool) {
        self.standard_builtins = enabled;
    }

    /// Disable the recognition of these ordinary names only, as
    /// `-fno-builtin-NAME` does in GCC 14. `-fbuiltin` does not restore a
    /// name; an unrecognized name has no effect.
    pub fn set_disabled_builtins<'n>(&mut self, names: impl IntoIterator<Item = &'n str>) {
        self.disabled_builtins = names.into_iter().map(str::to_owned).collect();
    }

    /// Whether the ordinary name `spelling` may be recognized as a builtin.
    pub(super) fn recognizes_builtin(&self, spelling: &str) -> bool {
        self.standard_builtins && !self.disabled_builtins.contains(spelling)
    }

    /// Admit the omitted integer declarations of explicitly selected C89/GNU89.
    /// The default language keeps unknown calls and missing types diagnosed.
    pub fn set_legacy_declarations(&mut self, enabled: bool) {
        self.legacy_declarations = enabled;
    }

    /// Keep implicit declaration evidence for the driver's warning policy.
    /// This does not enable old-style definitions in a modern language mode.
    pub fn enable_lints(&mut self) {
        self.lint_declarations = true;
    }

    pub fn lint_findings(&self) -> &[evocc_chir::lints::Finding] {
        &self.lint_findings
    }

    pub fn set_gnu_inline(&mut self, enabled: bool) {
        self.gnu_inline = enabled;
    }

    /// In ISO modes, bare GNU spellings remain ordinary identifiers. Apply
    /// this only after preprocessing so macro replacement is unchanged;
    /// reserved spellings such as __asm__ and __typeof__ stay available.
    pub fn use_language_keywords(&mut self, iso: bool, pre_c99: bool) {
        for token in &mut self.tokens {
            let spelling = token.text.spelling();
            if (iso && matches!(spelling, Some("asm" | "typeof")))
                || (pre_c99
                    && (spelling == Some("restrict") || (iso && spelling == Some("inline"))))
            {
                token.kind = TokenKind::Identifier;
            }
        }
    }

    pub fn parse_translation_unit_in(&mut self, arena: &'arena Arena) -> CHirUnit<'arena> {
        assert!(self.arena.is_none(), "Parser instances parse only one unit");
        self.arena = Some(arena);
        let mut decls = Vec::new();
        while !self.is_done() && !self.reached_error_limit() {
            if self.consume_neon_header_marker() {
                decls.append(&mut self.builtin_declarations);
                continue;
            }
            if self.consume_pragma_ms_struct_marker()
                || self.consume_pragma_pack_marker()
                || self.consume_visibility_marker()
                || self.consume_fenv_marker(true)
            {
                continue;
            }
            if let Some((code, message)) = misplaced_statement_diagnostic(self.peek_kind()) {
                self.report(code, message, self.span());
                if self.reached_error_limit() {
                    break;
                }
                self.advance();
                continue;
            }
            let before = self.pos;
            if let Some(mut parsed) = self.parse_external_declaration() {
                decls.append(&mut self.builtin_declarations);
                decls.append(&mut parsed);
            }
            // Failed declaration transactions restore their own scope cursor,
            // so the next declaration starts in the same live scope that
            // preceded the failed one.
            if self.reached_error_limit() {
                break;
            }
            if self.pos == before {
                self.report("E1001", "expected-declaration", self.span());
                if self.reached_error_limit() {
                    break;
                }
                self.advance();
            }
        }
        for (symbol, span) in std::mem::take(&mut self.standard_allocation_calls) {
            if self
                .entities
                .get(&symbol)
                .is_some_and(|state| state.has_definition || state.assembly_named)
            {
                self.report("E2944", "ordinary-builtin-binding-conflict", span);
            }
        }
        // A later weak/alias declaration cannot leave a previously folded
        // object's initializer as the authority for a replaceable definition.
        for (symbol, span) in std::mem::take(&mut self.string_constant_uses) {
            if !self.string_object_binding_is_fixed(symbol) {
                self.report("E2954", "constant-string-binding-conflict", span);
            }
        }
        let definitions = decls
            .iter()
            .filter_map(|declaration| match declaration {
                CHirDecl::Function(function) if function.body.is_some() => Some(function.symbol),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut observed: HashMap<SymbolId, Vec<TypeId>> = HashMap::new();
        for (symbol, parameters, span) in std::mem::take(&mut self.unprototyped_calls) {
            if definitions.contains(&symbol) {
                continue;
            }
            if let Some(first) = observed.get(&symbol) {
                if first.len() != parameters.len()
                    || !first
                        .iter()
                        .zip(&parameters)
                        .all(|(left, right)| self.same_unqualified_type(*left, *right))
                {
                    self.report("E2931", "unprototyped-call-mismatch", span);
                }
            } else {
                observed.insert(symbol, parameters);
            }
        }
        self.pop_availability_frames(0);
        let conventions = self.take_convention_records();
        let mut unit = CHirUnit::new(
            decls,
            std::mem::take(&mut self.types),
            std::mem::take(&mut self.symbols),
        );
        if let Some(deployment) = self.availability_target {
            unit.set_weak_imports(
                self.symbol_availability
                    .iter()
                    .filter(|(_, state)| state.weak_import(deployment))
                    .map(|(symbol, _)| *symbol)
                    .collect(),
            );
        }
        let mut inline_refusals =
            always_inline_returns_twice(&unit, &conventions.uses, self.diagnostics.len());
        let (late_refusals, mut convention_uses) = convention_refusals(
            &unit,
            conventions,
            self.target_layout.names_clang_extensions(),
            self.target_layout.names_clang_extensions() && !self.general_registers_only,
        );
        unit.set_gnu_inline(self.gnu_inline);
        if self.lint_declarations {
            self.lint_findings
                .extend(self.pointer_lint_findings.borrow().iter().copied());
            self.lint_findings.extend(evocc_chir::lints::check(
                &unit,
                &self.lint_uses,
                &self.target_layout,
            ));
            self.lint_findings
                .sort_by_key(|f| (f.span.file.raw(), f.span.byte_start, f.lint));
            self.lint_findings.dedup_by_key(|f| (f.span, f.lint));
        }
        if let Err(error) = evocc_chir::function_definitions::FunctionDefinitions::derive(&unit) {
            self.report("E2002", "incompatible-redeclaration", error.duplicate);
        }
        if let Err(span) = evocc_chir::function_definitions::patchable_entries(&unit) {
            self.report("E2943", "invalid-or-conflicting-symbol-attribute", span);
        }
        unit.set_detached_attributes(std::mem::take(&mut self.detached_attributes));
        unit.set_generic_selections(std::mem::take(&mut self.generic_selections));
        unit.set_front_end_builtins(std::mem::take(&mut self.front_end_builtins));
        unit.set_unprototyped_parameters(observed);
        unit.set_old_style_parameters(
            std::mem::take(&mut self.old_style_definitions)
                .into_iter()
                .collect(),
        );
        let mut refusals = std::mem::take(&mut self.profile_refusals);
        refusals.append(&mut convention_uses);
        refusals.append(&mut inline_refusals);
        refusals.sort_by_key(|(_, preceding, _)| *preceding);
        unit.set_profile_refusals(
            refusals
                .iter()
                .map(|(refusal, _, _)| refusal.clone())
                .collect(),
        );
        let mut late = late_refusals
            .into_iter()
            .map(|(message, span, preceding)| ("E2952", message, span, preceding))
            .collect::<Vec<_>>();
        if !refusals.is_empty() {
            let retained = evocc_chir::inline::retained_functions(&unit);
            let active = refusals.into_iter().filter(|(refusal, _, _)| {
                // A malformed definition/linkage is not proof of disuse.
                !definitions.contains(&refusal.function)
                    || retained
                        .as_ref()
                        .map_or(true, |functions| functions.contains(&refusal.function))
            });
            late.extend(active.map(|(refusal, preceding, _)| {
                (refusal.code, refusal.message, refusal.span, preceding)
            }));
        }
        late.sort_by_key(|(_, _, _, preceding)| *preceding);
        for (inserted, (code, message, span, preceding)) in late.into_iter().enumerate() {
            self.report(code, message, span);
            let diagnostic = self.diagnostics.pop().expect("just reported diagnostic");
            // This is parse encounter order, not a comparison of spans
            // from different headers or macro expansion locations.
            self.diagnostics.insert(preceding + inserted, diagnostic);
        }
        unit
    }

    pub fn had_error(&self) -> bool {
        self.had_error
    }

    /// The diagnostic a build reports: the first one found.
    pub fn diagnostic(&self) -> Option<&ParseDiagnostic> {
        self.diagnostics.first()
    }

    /// Every diagnostic found, in source order.
    ///
    /// A build stops at the first error, so it needs one round per
    /// unsupported construct to learn what a body of code requires. A survey
    /// leaves the error limit unset and reads this instead, enumerating the
    /// whole set in one pass.
    pub fn diagnostics(&self) -> &[ParseDiagnostic] {
        &self.diagnostics
    }

    pub(super) fn reached_error_limit(&self) -> bool {
        self.error_limit
            .is_some_and(|limit| self.error_count >= limit.get())
    }

    pub(super) fn consume_pragma_ms_struct_marker(&mut self) -> bool {
        let Some(action) = self.peek().and_then(pragma_ms_struct_action) else {
            return false;
        };
        let span = self.advance().expect("peeked ms_struct marker").span;
        if action == "on" && self.target_layout.names_clang_extensions() {
            self.report("E2967", "unsupported-darwin-attribute", span);
        }
        true
    }

    pub(super) fn consume_pragma_pack_marker(&mut self) -> bool {
        let Some(action) = self.peek().and_then(pragma_pack_action) else {
            return false;
        };
        self.advance();
        match action {
            PragmaPackAction::Set(alignment) => self.pack_alignment = Some(alignment),
            PragmaPackAction::Reset => self.pack_alignment = None,
            PragmaPackAction::Push(alignment) => {
                self.pack_stack.push(self.pack_alignment);
                if let Some(alignment) = alignment {
                    self.pack_alignment = Some(alignment);
                }
            }
            PragmaPackAction::Pop => {
                self.pack_alignment = self.pack_stack.pop().unwrap_or(None);
            }
        }
        true
    }

    pub(super) fn consume_visibility_marker(&mut self) -> bool {
        let Some(action) = self.peek().and_then(pragma_visibility_action) else {
            return false;
        };
        let span = self.advance().expect("peeked visibility marker").span;
        match action {
            PragmaVisibilityAction::Push { visibility } => {
                self.visibility_stack.push((visibility, span))
            }
            PragmaVisibilityAction::Pop => {
                if self.visibility_stack.pop().is_none() {
                    self.report("E2943", "invalid-or-conflicting-symbol-attribute", span);
                }
            }
        }
        true
    }

    pub(super) fn apply_visibility_default(
        &self,
        symbol: SymbolId,
        attributes: &mut Vec<Attribute>,
        visibility: Option<(&'static str, SourceSpan)>,
    ) {
        let Some((visibility, span)) = visibility else {
            return;
        };
        if self
            .entity_attributes
            .get(&symbol)
            .is_some_and(|a| a.contains("visibility"))
            || attributes
                .iter()
                .any(|a| a.semantic_name() == Some("visibility"))
        {
            return;
        }
        attributes.push(Attribute {
            name: "visibility".into(),
            written: "#pragma GCC visibility".into(),
            syntax: AttributeSyntax::Pragma,
            arguments: vec![AttributeArgument {
                text: format!("\"{visibility}\""),
                span,
                integer: None,
            }],
            placement: AttributePlacement::DeclarationSpecifiers,
            span,
            applied: None,
        });
    }
}
