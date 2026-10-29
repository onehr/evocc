//! Parser cursor.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    // ── helpers ────────────────────────────────────────────────────────

    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub(super) fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    pub(super) fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn is_done(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), Some(TokenKind::Eof))
    }

    pub(super) fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    pub(super) fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.peek_kind() == Some(kind) {
            self.advance()
        } else {
            self.report(
                "E1002",
                format!("expected-token: {}", token_spelling(kind)),
                self.span(),
            );
            None
        }
    }

    pub(super) fn report(
        &mut self,
        code: &'static str,
        message: impl Into<String>,
        span: SourceSpan,
    ) {
        self.had_error = true;
        self.error_count += 1;
        // An unresolved use is removed at the expression/statement boundary,
        // so a surrounding function can still retain its fully resolved
        // declarations and scopes for diagnostics. A use of an unavailable
        // declaration leaves a well-formed declaration. Declaration-shape and
        // redefinition errors remain transactional.
        if code != "E2001" && code != "E2962" {
            self.rollback_error_count += 1;
        }
        {
            self.diagnostics.push(ParseDiagnostic {
                span,
                previous_declaration: None,
                code,
                message: message.into(),
            });
        }
    }

    /// C requires a scalar value in a controlling expression. Pointers and
    /// complex values are scalar; records, unions, vectors and void are not.
    pub(super) fn require_scalar_controlling_expression(
        &mut self,
        expression: &CHirExpr<'arena>,
        code: &'static str,
        message: &'static str,
    ) -> Option<()> {
        let inferred = self.converted_expression_type(expression)?;
        if is_scalar_class(self.operand_type_class(inferred.ty)) {
            return Some(());
        }
        self.report(code, message, expression.span());
        None
    }

    // ── availability contexts (EvoIR 10.1.9) ──────────────────────────

    pub(super) fn push_availability_frame(
        &mut self,
        exempt: Option<bool>,
        parent: Option<usize>,
    ) -> usize {
        self.availability_frames.push(AvailabilityFrame {
            exempt,
            parent,
            function: false,
            pending: Vec::new(),
        });
        self.availability_frames.len() - 1
    }

    pub(super) fn innermost_availability_frame(&self) -> Option<usize> {
        self.availability_frames.len().checked_sub(1)
    }

    /// The innermost function body, which encloses a local record.
    pub(super) fn function_availability_frame(&self) -> Option<usize> {
        self.availability_frames
            .iter()
            .rposition(|frame| frame.function)
    }

    /// Refuse a use of the unavailable entity `name` unless its context is
    /// unavailable too. The context may settle later in the declaration.
    pub(super) fn use_unavailable(&mut self, name: &str, reason: &str, span: SourceSpan) {
        let message = if reason.is_empty() {
            format!("unavailable-declaration: {name}")
        } else {
            format!("unavailable-declaration: {name}: {reason}")
        };
        let usage = UnavailableUse {
            message,
            span,
            preceding: self.diagnostics.len(),
        };
        self.dispatch_unavailable_uses(self.innermost_availability_frame(), vec![usage]);
    }

    pub(super) fn dispatch_unavailable_uses(
        &mut self,
        from: Option<usize>,
        uses: Vec<UnavailableUse>,
    ) {
        let mut refused = Vec::new();
        'uses: for usage in uses {
            let mut frame = from;
            while let Some(index) = frame {
                match self.availability_frames[index].exempt {
                    Some(true) => continue 'uses,
                    Some(false) => frame = self.availability_frames[index].parent,
                    None => {
                        self.availability_frames[index].pending.push(usage);
                        continue 'uses;
                    }
                }
            }
            refused.push(usage);
        }
        // Each refusal takes its place in encounter order; later ones are
        // inserted first so that equal positions keep their order.
        for usage in refused.into_iter().rev() {
            self.report("E2962", usage.message, usage.span);
            let diagnostic = self.diagnostics.pop().expect("just reported diagnostic");
            let index = usage.preceding.min(self.diagnostics.len());
            self.diagnostics.insert(index, diagnostic);
        }
    }

    /// Settle a context once the attributes of its entity are complete.
    pub(super) fn resolve_availability_frame(&mut self, frame: usize, exempt: bool) {
        let state = &mut self.availability_frames[frame];
        if state.exempt.is_some() {
            return;
        }
        state.exempt = Some(exempt);
        let pending = std::mem::take(&mut state.pending);
        let parent = state.parent;
        if !exempt {
            self.dispatch_unavailable_uses(parent, pending);
        }
    }

    /// Leave every context above `depth`. A context that never settled is
    /// not unavailable.
    pub(super) fn pop_availability_frames(&mut self, depth: usize) {
        while self.availability_frames.len() > depth {
            let index = self.availability_frames.len() - 1;
            self.resolve_availability_frame(index, false);
            self.availability_frames.pop();
        }
    }

    /// Execution-only profile refusals must wait until unit references are
    /// known. Ordinary syntax/type errors continue through `report` unchanged.
    pub(super) fn defer_profile_refusal(
        &mut self,
        code: &'static str,
        message: &'static str,
        span: SourceSpan,
    ) -> bool {
        if let Some(function) = self.function_owner() {
            self.profile_refusals.push((
                evocc_chir::ProfileRefusal {
                    function,
                    code,
                    message,
                    span,
                },
                self.diagnostics.len(),
                None,
            ));
            return true;
        }
        self.report(code, message, span);
        false
    }

    /// The function definition whose body encloses the current scope.
    pub(super) fn function_owner(&self) -> Option<SymbolId> {
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            let entry = self.symbols.scope(id).ok()?;
            if let Some(function) = entry.function_owner() {
                return Some(function);
            }
            scope = entry.parent();
        }
        None
    }

    /// Record a use of a function designator. A call through a pointer cannot
    /// be checked, so every use counts, not only a direct call.
    pub(super) fn note_function_use(&mut self, function: SymbolId, span: SourceSpan) {
        if self
            .symbols
            .symbol(function)
            .is_ok_and(|entry| entry.kind() == SymbolKind::Function)
        {
            self.function_uses.push(FunctionUse {
                function,
                owner: self.function_owner(),
                span,
                preceding: self.diagnostics.len(),
            });
        }
    }

    /// A function prototype accepts the deferred list attributes it receives.
    /// A definition accepts only the attributes whose functions can have a
    /// definition. Any other declaration refuses them at the attribute name.
    pub(super) fn settle_convention_attributes(
        &mut self,
        attributes: &[Attribute],
        settlement: Settlement,
    ) {
        for attribute in attributes {
            let Some((name, message)) = attribute.semantic_name().and_then(convention_attribute)
            else {
                continue;
            };
            let accept = match settlement {
                Settlement::Prototype => true,
                Settlement::Definition => {
                    convention_permits(name, true)
                        || (self.target_layout.names_clang_extensions()
                            && !self.general_registers_only
                            && positive_target_attribute(attribute))
                }
                Settlement::Other => false,
            };
            let mut refused = None;
            for entry in &mut self.convention_attributes {
                if entry.span != attribute.span {
                    continue;
                }
                match (accept, entry.state) {
                    (true, ConventionState::Open) => entry.state = ConventionState::Accepted,
                    (false, ConventionState::Open | ConventionState::Accepted) => {
                        entry.state = ConventionState::Refused;
                        refused.get_or_insert(entry.name);
                    }
                    _ => {}
                }
            }
            if let Some(span) = refused {
                self.report("E2952", message, span);
            }
        }
    }

    /// Take the list-attribute records of the unit. An attribute that no
    /// declaration settled is reported at once.
    pub(super) fn take_convention_records(&mut self) -> ConventionRecords {
        let mut records = ConventionRecords {
            unsettled: Vec::new(),
            accepted: HashSet::new(),
            definitions: std::mem::take(&mut self.function_definitions),
            uses: std::mem::take(&mut self.function_uses),
        };
        let mut unsettled = Vec::new();
        for entry in std::mem::take(&mut self.convention_attributes) {
            match entry.state {
                ConventionState::Open if !unsettled.contains(&entry.span) => {
                    unsettled.push(entry.span);
                    let (_, message) =
                        convention_attribute(entry.attribute).expect("deferred list attribute");
                    records
                        .unsettled
                        .push((message, entry.name, entry.preceding));
                }
                ConventionState::Accepted => {
                    records.accepted.insert(span_key(entry.span));
                }
                _ => {}
            }
        }
        records
    }

    pub(super) fn token_text<'a>(&'a self, tok: &'a Token) -> &'a str {
        tok.text.spelling().unwrap_or("")
    }

    pub(super) fn span(&self) -> SourceSpan {
        if let Some(tok) = self.peek() {
            tok.span
        } else {
            SourceSpan::new(self.file, 0, 0, 0, 0)
        }
    }

    pub(super) fn merge_span(&self, start: SourceSpan, end: SourceSpan) -> SourceSpan {
        // A SourceSpan is a range within exactly one source file. Most parser
        // composites join tokens from that same file, so preserve the token
        // origin instead of replacing it with the parser's main-file ID.
        //
        // Macro expansion can eventually place endpoints from different
        // files in one syntactic node. Until CHIR has an explicit expansion
        // provenance model, fail closed to the start token as the node's
        // primary origin. Combining offsets from different files would create
        // a range that is valid in neither source.
        if start.file != end.file || end.byte_start < start.byte_start {
            return start;
        }
        SourceSpan::new(
            start.file,
            start.byte_start,
            end.byte_end,
            start.line_start,
            start.column_start,
        )
    }

    /// Allocate one C-HIR node through the caller-owned arena.
    ///
    /// # Invariant
    ///
    /// Every caller in this module passes a `CHirDecl`, `CHirStmt`, or
    /// `CHirExpr`. Each child reference points to an earlier allocation in the
    /// same caller-owned arena, and the caller keeps that arena alive through
    /// the physical teardown of the complete C-HIR graph. C-HIR owns all of
    /// its source-fact buffers and contains no reference or handle to the
    /// `Arena` itself. Its transitive destructors therefore release only those
    /// owned buffers and cannot re-enter or otherwise access this arena.
    pub(super) fn alloc_node<T: 'arena>(&self, value: T) -> &'arena T {
        let arena = self.arena.expect("parse_translation_unit_in sets Arena");
        // Safety: upheld by the module invariant documented above.
        unsafe { arena.alloc_shared(value) }
    }

    pub(super) fn intern(&mut self, kind: TypeKind, qualifiers: Qualifiers) -> TypeId {
        if let TypeKind::Complex(ComplexBase::Integer(rank)) = kind {
            self.types.intern(CanonicalType {
                kind: TypeKind::Int(rank),
                qualifiers: Qualifiers::default(),
            });
        }
        self.types.intern(CanonicalType { kind, qualifiers })
    }

    pub(super) fn qualify_typedef_type(&mut self, ty: TypeId, qualifiers: Qualifiers) -> TypeId {
        let aliased = self
            .types
            .get(ty)
            .expect("typedef symbols only reference interned types")
            .clone();
        let qualifiers = merge_qualifiers(aliased.qualifiers, qualifiers);
        match aliased.kind {
            // C11 6.7.3p9: qualifiers applied to an array typedef qualify its
            // element type, not the array layer. Recurse for multidimensional
            // arrays so no impossible qualified-array canonical form escapes.
            TypeKind::Array { element, bound } => {
                let element = self.qualify_typedef_type(element, qualifiers);
                self.intern(TypeKind::Array { element, bound }, Qualifiers::default())
            }
            kind => self.intern(kind, qualifiers),
        }
    }

    pub(super) fn strip_top_level_qualifiers(&mut self, ty: TypeId) -> TypeId {
        let canonical = self
            .types
            .get(ty)
            .expect("resolved typeof operands reference interned types")
            .clone();
        let qualifiers = Qualifiers {
            aligned: canonical.qualifiers.aligned,
            ..Qualifiers::default()
        };
        self.intern(canonical.kind, qualifiers)
    }
}
