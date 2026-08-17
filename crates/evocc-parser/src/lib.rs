//! C declarator, expression, and statement parser producing C-HIR

use std::collections::HashMap;

use evocc_chir::{
    BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, CHirUnit, CharLit,
    CharPrefix, EnumeratorDecl, FloatLit, FloatSuffix, FunctionDecl, IntBase, IntLit, IntSuffix,
    ObjectDecl, ParameterDecl, ParameterSyntax, RecordMember, SpannedName, StorageClass, StringLit,
    StringPrefix, TagDecl, TagMembers, TypeSpecifier, TypedefDecl, UnaryOp, WrittenDeclarator,
    WrittenDeclaratorLayer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_lex::{Token, TokenKind};
use evocc_types::{
    ArrayBound, CanonicalType, EnumConstant, FloatKind, FunctionParameters, IntRank, Qualifiers,
    RecordField, TagDefinition, TagId, TagKind, TagState, TypeId, TypeInterner, TypeKind,
};

#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    pub span: SourceSpan,
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Clone)]
enum DerivedDeclarator {
    Pointer {
        qualifiers: Qualifiers,
        span: SourceSpan,
    },
    Array {
        bound: ArrayBound,
        span: SourceSpan,
    },
    Function {
        parameters: ParameterSyntax,
        span: SourceSpan,
    },
}

#[derive(Debug, Clone)]
struct ParsedDeclarator {
    name: SpannedName,
    derived: Vec<DerivedDeclarator>,
    span: SourceSpan,
}

struct ParsedSpecifier<'arena> {
    surface: TypeSpecifier,
    tag_event: Option<PendingTag<'arena>>,
}

struct PendingTag<'arena> {
    tag: TagId,
    name: Option<SpannedName>,
    members: TagMembers<'arena>,
    specifier_span: SourceSpan,
}

#[derive(Clone, Copy)]
struct TagBinding {
    tag: TagId,
    kind: TagKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclarationContext {
    File,
    Block,
    ForInitializer,
}

pub struct Parser<'src, 'arena> {
    tokens: Vec<Token>,
    pos: usize,
    file: FileId,
    source: &'src str,
    had_error: bool,
    error_count: usize,
    diagnostic: Option<ParseDiagnostic>,
    arena: Option<&'arena Arena>,
    types: TypeInterner,
    tags: HashMap<String, TagBinding>,
    side_declarations: Vec<&'arena CHirDecl<'arena>>,
    loop_depth: usize,
    switch_depth: usize,
}

impl<'src, 'arena> Parser<'src, 'arena> {
    pub fn new(tokens: Vec<Token>, file: FileId, source: &'src str) -> Self {
        Self {
            tokens,
            pos: 0,
            file,
            source,
            had_error: false,
            error_count: 0,
            diagnostic: None,
            arena: None,
            types: TypeInterner::new(),
            tags: HashMap::new(),
            side_declarations: Vec::new(),
            loop_depth: 0,
            switch_depth: 0,
        }
    }

    pub fn parse_translation_unit_in(&mut self, arena: &'arena Arena) -> CHirUnit<'arena> {
        assert!(self.arena.is_none(), "Parser instances parse only one unit");
        self.arena = Some(arena);
        let mut decls = Vec::new();
        while !self.is_done() {
            if let Some((code, message)) = misplaced_statement_diagnostic(self.peek_kind()) {
                self.report(code, message, self.span());
                self.advance();
                continue;
            }
            let before = self.pos;
            if let Some(mut parsed) = self.parse_external_declaration() {
                decls.append(&mut parsed);
            }
            if self.pos == before {
                self.report("E1001", "expected-declaration", self.span());
                self.advance();
            }
        }
        CHirUnit::new(decls, std::mem::take(&mut self.types))
    }

    pub fn had_error(&self) -> bool {
        self.had_error
    }

    pub fn diagnostic(&self) -> Option<&ParseDiagnostic> {
        self.diagnostic.as_ref()
    }

    // ── helpers ────────────────────────────────────────────────────────

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind)
    }

    fn advance(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn is_done(&self) -> bool {
        self.pos >= self.tokens.len() || matches!(self.peek_kind(), Some(TokenKind::Eof))
    }

    fn match_kind(&mut self, kind: TokenKind) -> bool {
        if self.peek_kind() == Some(kind) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
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

    fn report(&mut self, code: &'static str, message: impl Into<String>, span: SourceSpan) {
        self.had_error = true;
        self.error_count += 1;
        if self.diagnostic.is_none() {
            self.diagnostic = Some(ParseDiagnostic {
                span,
                code,
                message: message.into(),
            });
        }
    }

    fn token_text<'a>(&'a self, tok: &'a Token) -> &'a str {
        if let evocc_lex::TokenText::Decoded(text) = &tok.text {
            return text;
        }
        self.source
            .get(tok.span.byte_start as usize..tok.span.byte_end as usize)
            .unwrap_or("")
    }

    fn span(&self) -> SourceSpan {
        if let Some(tok) = self.peek() {
            tok.span
        } else {
            SourceSpan::new(self.file, 0, 0, 0, 0)
        }
    }

    fn merge_span(&self, start: SourceSpan, end: SourceSpan) -> SourceSpan {
        SourceSpan::new(
            self.file,
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
    fn alloc_node<T: 'arena>(&self, value: T) -> &'arena T {
        let arena = self.arena.expect("parse_translation_unit_in sets Arena");
        // Safety: upheld by the module invariant documented above.
        unsafe { arena.alloc_shared(value) }
    }

    fn intern(&mut self, kind: TypeKind, qualifiers: Qualifiers) -> TypeId {
        self.types.intern(CanonicalType { kind, qualifiers })
    }

    // ── external declarations ──────────────────────────────────────────

    fn parse_external_declaration(&mut self) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        self.parse_declaration(DeclarationContext::File)
    }

    fn parse_declaration(
        &mut self,
        context: DeclarationContext,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let types_checkpoint = self.types.clone();
        let tags_checkpoint = self.tags.clone();
        let side_checkpoint = self.side_declarations.clone();
        let error_checkpoint = self.error_count;
        let side_start = side_checkpoint.len();

        let result = self.parse_declaration_inner(context, side_start);
        if result.is_none() || self.error_count != error_checkpoint {
            self.types = types_checkpoint;
            self.tags = tags_checkpoint;
            self.side_declarations = side_checkpoint;
            None
        } else {
            result
        }
    }

    fn parse_declaration_inner(
        &mut self,
        context: DeclarationContext,
        side_start: usize,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let start_span = self.span();
        let storage = self.parse_storage_class();
        let Some(specifier) = self.parse_type_specifier() else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };

        if self.peek_kind() == Some(TokenKind::Semicolon) {
            let semi = self.advance().expect("peeked semicolon");
            let declaration_span = self.merge_span(start_span, semi.span);
            let mut declarations = self.side_declarations.split_off(side_start);
            if let Some(event) = specifier.tag_event {
                declarations.push(self.finish_tag_event(event, declaration_span));
            } else {
                self.report("E1001", "expected-declaration", semi.span);
            }
            return Some(declarations);
        }

        let Some(declarator) = self.parse_declarator() else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        let full_type = self.apply_declarator(specifier.surface.type_id(), &declarator.derived);
        let written = self.written_declarator(&declarator);

        if matches!(
            self.types.get(full_type).map(|ty| &ty.kind),
            Some(TypeKind::Function { .. })
        ) {
            let parameters = declarator
                .derived
                .iter()
                .find_map(|layer| match layer {
                    DerivedDeclarator::Function { parameters, .. } => Some(parameters.clone()),
                    _ => None,
                })
                .unwrap_or(ParameterSyntax::Unspecified);
            let body = if self.peek_kind() == Some(TokenKind::LBrace) {
                let open = self.span();
                let body = self.parse_function_body();
                if context != DeclarationContext::File {
                    self.report("E1205", "nested-function-definition", open);
                }
                Some(body)
            } else {
                None
            };
            let end_span = if let Some(body) = body {
                body.span()
            } else {
                self.declaration_end(declarator.name.span)
            };
            let declaration_span = self.merge_span(start_span, end_span);
            let mut declarations = self.side_declarations.split_off(side_start);
            if let Some(event) = specifier.tag_event {
                declarations.push(self.finish_tag_event(event, declaration_span));
            }
            declarations.push(self.alloc_node(CHirDecl::Function(FunctionDecl {
                name: declarator.name,
                ty: full_type,
                return_specifier: specifier.surface,
                written,
                parameters,
                storage,
                body,
                span: declaration_span,
            })));
            return Some(declarations);
        }

        let initializer = if self.match_kind(TokenKind::Assign) {
            Some(self.parse_expression()?)
        } else {
            None
        };
        let end_span = self.declaration_end(declarator.name.span);
        let declaration_span = self.merge_span(start_span, end_span);
        let mut declarations = self.side_declarations.split_off(side_start);
        if let Some(event) = specifier.tag_event {
            declarations.push(self.finish_tag_event(event, declaration_span));
        }
        let declaration = if storage == StorageClass::Typedef {
            CHirDecl::Typedef(TypedefDecl {
                name: declarator.name,
                ty: full_type,
                specifier: specifier.surface,
                written,
                span: declaration_span,
            })
        } else {
            CHirDecl::Object(ObjectDecl {
                name: declarator.name,
                ty: full_type,
                specifier: specifier.surface,
                written,
                storage,
                initializer,
                span: declaration_span,
            })
        };
        declarations.push(self.alloc_node(declaration));
        Some(declarations)
    }

    fn parse_function_body(&mut self) -> &'arena CHirStmt<'arena> {
        let outer_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let outer_switch_depth = std::mem::replace(&mut self.switch_depth, 0);
        let body = self.parse_compound_statement();
        self.loop_depth = outer_loop_depth;
        self.switch_depth = outer_switch_depth;
        body
    }

    fn declaration_end(&mut self, fallback: SourceSpan) -> SourceSpan {
        if self.peek_kind() == Some(TokenKind::Semicolon) {
            return self.advance().expect("peeked semicolon").span;
        }
        let span = self.span();
        if self.peek_kind() == Some(TokenKind::Identifier) {
            self.report("E1004", "trailing-token", span);
        } else {
            self.report("E1002", "expected-token: ;", span);
        }
        fallback
    }

    fn finish_tag_event(
        &self,
        event: PendingTag<'arena>,
        declaration_span: SourceSpan,
    ) -> &'arena CHirDecl<'arena> {
        self.alloc_node(CHirDecl::Tag(TagDecl {
            tag: event.tag,
            name: event.name,
            members: event.members,
            span: declaration_span,
            specifier_span: event.specifier_span,
        }))
    }

    // ── type specifier ─────────────────────────────────────────────────

    fn parse_storage_class(&mut self) -> StorageClass {
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

    fn parse_type_specifier(&mut self) -> Option<ParsedSpecifier<'arena>> {
        let start = self.span();
        let qualifiers = self.parse_qualifiers();
        let token = self.advance()?;
        let kind = match token.kind {
            TokenKind::Void => TypeKind::Void,
            TokenKind::Int | TokenKind::Signed => TypeKind::Int(IntRank::Int),
            TokenKind::Long => TypeKind::Int(IntRank::Long),
            TokenKind::Short => TypeKind::Int(IntRank::Short),
            TokenKind::Char => TypeKind::Int(IntRank::Char),
            TokenKind::Unsigned => {
                let rank = match self.peek_kind() {
                    Some(TokenKind::Char) => {
                        self.advance();
                        IntRank::UChar
                    }
                    Some(TokenKind::Short) => {
                        self.advance();
                        IntRank::UShort
                    }
                    Some(TokenKind::Long) => {
                        self.advance();
                        IntRank::ULong
                    }
                    Some(TokenKind::Int) => {
                        self.advance();
                        IntRank::UInt
                    }
                    _ => IntRank::UInt,
                };
                TypeKind::Int(rank)
            }
            TokenKind::Float => TypeKind::Float(FloatKind::F32),
            TokenKind::Double => TypeKind::Float(FloatKind::F64),
            TokenKind::Bool => TypeKind::Int(IntRank::Bool),
            TokenKind::Struct => {
                return self.parse_tag_specifier(TagKind::Struct, qualifiers, token.span)
            }
            TokenKind::Union => {
                return self.parse_tag_specifier(TagKind::Union, qualifiers, token.span)
            }
            TokenKind::Enum => {
                return self.parse_tag_specifier(TagKind::Enum, qualifiers, token.span)
            }
            _ => {
                self.pos = self.pos.saturating_sub(1);
                return None;
            }
        };
        let end = self
            .tokens
            .get(self.pos.saturating_sub(1))
            .map_or(token.span, |t| t.span);
        let ty = self.intern(kind, qualifiers);
        Some(ParsedSpecifier {
            surface: TypeSpecifier::Builtin {
                ty,
                span: self.merge_span(start, end),
            },
            tag_event: None,
        })
    }

    fn parse_qualifiers(&mut self) -> Qualifiers {
        let mut qualifiers = Qualifiers::default();
        loop {
            match self.peek_kind() {
                Some(TokenKind::Const) => qualifiers.is_const = true,
                Some(TokenKind::Volatile) => qualifiers.is_volatile = true,
                Some(TokenKind::Restrict) => qualifiers.is_restrict = true,
                Some(TokenKind::Atomic) => qualifiers.is_atomic = true,
                _ => break,
            }
            self.advance();
        }
        qualifiers
    }

    fn parse_tag_specifier(
        &mut self,
        requested_kind: TagKind,
        qualifiers: Qualifiers,
        keyword_span: SourceSpan,
    ) -> Option<ParsedSpecifier<'arena>> {
        let name = if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().expect("peeked identifier");
            Some(SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            })
        } else {
            None
        };
        let is_definition = self.peek_kind() == Some(TokenKind::LBrace);
        if name.is_none() && !is_definition {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        }

        let mut fresh = false;
        let binding = if let Some(name) = &name {
            if let Some(binding) = self.tags.get(&name.spelling).copied() {
                if binding.kind != requested_kind {
                    self.report(
                        "E1102",
                        format!("conflicting-tag-kind: {}", name.spelling),
                        name.span,
                    );
                }
                binding
            } else {
                fresh = true;
                let tag = self
                    .types
                    .declare_tag(requested_kind, Some(name.spelling.clone()));
                let binding = TagBinding {
                    tag,
                    kind: requested_kind,
                };
                self.tags.insert(name.spelling.clone(), binding);
                binding
            }
        } else {
            fresh = true;
            TagBinding {
                tag: self.types.declare_tag(requested_kind, None),
                kind: requested_kind,
            }
        };

        let semantic_kind = match binding.kind {
            TagKind::Struct => TypeKind::Struct(binding.tag),
            TagKind::Union => TypeKind::Union(binding.tag),
            TagKind::Enum => TypeKind::Enum(binding.tag),
        };
        let ty = self.intern(semantic_kind, qualifiers);

        if is_definition {
            let duplicate = matches!(
                self.types.tag(binding.tag).map(|tag| &tag.state),
                Some(TagState::Complete(_))
            );
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
                TagKind::Struct | TagKind::Union => self.parse_record_members(requested_kind),
                TagKind::Enum => self.parse_enum_members(),
            };
            let close = self.expect(TokenKind::RBrace);
            let end = close
                .as_ref()
                .map_or_else(|| self.span(), |token| token.span);
            if !duplicate
                && binding.kind == requested_kind
                && self.types.complete_tag(binding.tag, definition).is_err()
            {
                self.report(
                    "E1101",
                    "duplicate-tag-definition",
                    name.as_ref().map_or(keyword_span, |name| name.span),
                );
            }
            let specifier_span = self.merge_span(keyword_span, end);
            Some(ParsedSpecifier {
                surface: TypeSpecifier::Tag {
                    ty,
                    tag: binding.tag,
                    kind: requested_kind,
                    name: name.clone(),
                    span: specifier_span,
                },
                tag_event: Some(PendingTag {
                    tag: binding.tag,
                    name,
                    members,
                    specifier_span,
                }),
            })
        } else {
            let end = name.as_ref().map_or(keyword_span, |name| name.span);
            let specifier_span = self.merge_span(keyword_span, end);
            Some(ParsedSpecifier {
                surface: TypeSpecifier::Tag {
                    ty,
                    tag: binding.tag,
                    kind: requested_kind,
                    name: name.clone(),
                    span: specifier_span,
                },
                tag_event: fresh.then_some(PendingTag {
                    tag: binding.tag,
                    name,
                    members: TagMembers::Incomplete,
                    specifier_span,
                }),
            })
        }
    }

    fn parse_record_members(&mut self, kind: TagKind) -> (TagMembers<'arena>, TagDefinition) {
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            let start = self.span();
            let Some(specifier) = self.parse_type_specifier() else {
                self.report("E1001", "expected-declaration", self.span());
                self.advance();
                continue;
            };
            let declarator = if matches!(
                self.peek_kind(),
                Some(TokenKind::Colon | TokenKind::Semicolon)
            ) {
                None
            } else {
                self.parse_declarator()
            };
            let ty = declarator
                .as_ref()
                .map_or(specifier.surface.type_id(), |decl| {
                    self.apply_declarator(specifier.surface.type_id(), &decl.derived)
                });
            let bit_width = if self.match_kind(TokenKind::Colon) {
                self.parse_expression()
            } else {
                None
            };
            let fallback = declarator
                .as_ref()
                .map_or(specifier.surface.span(), |decl| decl.span);
            let semi = self.expect(TokenKind::Semicolon);
            let end = semi.as_ref().map_or(fallback, |token| token.span);
            let span = self.merge_span(start, end);
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span);
                self.side_declarations.push(declaration);
            }
            let name = declarator.as_ref().map(|decl| decl.name.clone());
            let written = declarator.as_ref().map_or(
                WrittenDeclarator {
                    layers: Vec::new(),
                    span: specifier.surface.span(),
                },
                |decl| self.written_declarator(decl),
            );
            canonical.push(RecordField {
                name: name.as_ref().map(|name| name.spelling.clone()),
                ty,
            });
            members.push(RecordMember {
                name,
                ty,
                specifier: specifier.surface,
                written,
                bit_width,
                span,
            });
        }
        match kind {
            TagKind::Struct => (
                TagMembers::Struct(members),
                TagDefinition::Struct(canonical),
            ),
            TagKind::Union => (TagMembers::Union(members), TagDefinition::Union(canonical)),
            TagKind::Enum => unreachable!("enum members use parse_enum_members"),
        }
    }

    fn parse_enum_members(&mut self) -> (TagMembers<'arena>, TagDefinition) {
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        let mut next_value = 0_i64;
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            let Some(token) = self.expect(TokenKind::Identifier) else {
                break;
            };
            let name = SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            };
            let written_value = if self.match_kind(TokenKind::Assign) {
                self.parse_assignment()
            } else {
                None
            };
            let value = written_value.map_or(next_value, integer_constant_value);
            let end = written_value.map_or(name.span, CHirExpr::span);
            let span = self.merge_span(name.span, end);
            canonical.push(EnumConstant {
                name: name.spelling.clone(),
                value,
            });
            members.push(EnumeratorDecl {
                name,
                value,
                written_value,
                span,
            });
            next_value = value.saturating_add(1);
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        (TagMembers::Enum(members), TagDefinition::Enum(canonical))
    }

    // ── declarators ────────────────────────────────────────────────────

    fn parse_declarator(&mut self) -> Option<ParsedDeclarator> {
        let mut pointers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            let star = self.advance().expect("peeked star");
            let qualifiers = self.parse_qualifiers();
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(star.span, |t| t.span);
            pointers.push(DerivedDeclarator::Pointer {
                qualifiers,
                span: self.merge_span(star.span, end),
            });
        }

        let mut declarator = if self.peek_kind() == Some(TokenKind::Identifier) {
            let token = self.advance().unwrap();
            ParsedDeclarator {
                name: SpannedName {
                    spelling: self.token_text(&token).to_owned(),
                    span: token.span,
                },
                derived: Vec::new(),
                span: token.span,
            }
        } else if self.peek_kind() == Some(TokenKind::LParen) {
            let open = self.advance().expect("peeked left parenthesis");
            let mut inner = self.parse_declarator()?;
            let close = self.expect(TokenKind::RParen)?;
            inner.span = self.merge_span(open.span, close.span);
            inner
        } else {
            return None;
        };

        loop {
            match self.peek_kind() {
                Some(TokenKind::LBracket) => {
                    let open = self.advance().unwrap();
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Function { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let bound = if self.peek_kind() == Some(TokenKind::RBracket) {
                        ArrayBound::Unbounded
                    } else if self.peek_kind() == Some(TokenKind::IntLiteral) {
                        let token = self.advance().unwrap();
                        ArrayBound::Fixed(parse_int_literal(self.token_text(&token)))
                    } else {
                        ArrayBound::Variable
                    };
                    let close = self.expect(TokenKind::RBracket)?;
                    declarator.derived.push(DerivedDeclarator::Array {
                        bound,
                        span: self.merge_span(open.span, close.span),
                    });
                    declarator.span = self.merge_span(declarator.span, close.span);
                }
                Some(TokenKind::LParen) => {
                    let open = self.advance().unwrap();
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Array { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let parameters = self.parse_parameter_list();
                    let close = self.expect(TokenKind::RParen)?;
                    declarator.derived.push(DerivedDeclarator::Function {
                        parameters,
                        span: self.merge_span(open.span, close.span),
                    });
                    declarator.span = self.merge_span(declarator.span, close.span);
                }
                _ => break,
            }
        }
        if let Some(pointer) = pointers.first() {
            let pointer_span = match pointer {
                DerivedDeclarator::Pointer { span, .. } => *span,
                _ => unreachable!(),
            };
            declarator.span = self.merge_span(pointer_span, declarator.span);
        }
        declarator.derived.extend(pointers);
        Some(declarator)
    }

    fn parse_parameter_list(&mut self) -> ParameterSyntax {
        let mut params = Vec::new();
        let mut variadic = false;
        if self.peek_kind() == Some(TokenKind::RParen) {
            return ParameterSyntax::Unspecified;
        }
        if self.peek_kind() == Some(TokenKind::Void)
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::RParen)
        {
            self.advance();
            return ParameterSyntax::Prototype {
                parameters: params,
                variadic,
            };
        }
        loop {
            if self.match_kind(TokenKind::Ellipsis) {
                variadic = true;
                break;
            }
            let Some(specifier) = self.parse_type_specifier() else {
                self.report("E1001", "expected-declaration", self.span());
                break;
            };
            let start = specifier.surface.span();
            let declarator =
                if matches!(self.peek_kind(), Some(TokenKind::Comma | TokenKind::RParen)) {
                    None
                } else {
                    self.parse_declarator()
                };
            let unadjusted = declarator
                .as_ref()
                .map_or(specifier.surface.type_id(), |decl| {
                    self.apply_declarator(specifier.surface.type_id(), &decl.derived)
                });
            let ty = self.adjust_parameter_type(unadjusted);
            let end = declarator.as_ref().map_or(start, |decl| decl.span);
            let span = self.merge_span(start, end);
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span);
                self.side_declarations.push(declaration);
            }
            params.push(ParameterDecl {
                name: declarator.as_ref().map(|decl| decl.name.clone()),
                ty,
                specifier: specifier.surface,
                written: declarator.as_ref().map_or(
                    WrittenDeclarator {
                        layers: Vec::new(),
                        span: start,
                    },
                    |decl| self.written_declarator(decl),
                ),
                span,
            });
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        ParameterSyntax::Prototype {
            parameters: params,
            variadic,
        }
    }

    fn adjust_parameter_type(&mut self, ty: TypeId) -> TypeId {
        match self.types.get(ty).map(|ty| ty.kind.clone()) {
            Some(TypeKind::Array { element, .. }) => {
                self.intern(TypeKind::Pointer(element), Qualifiers::default())
            }
            Some(TypeKind::Function { .. }) => {
                self.intern(TypeKind::Pointer(ty), Qualifiers::default())
            }
            _ => ty,
        }
    }

    fn apply_declarator(&mut self, mut ty: TypeId, derived: &[DerivedDeclarator]) -> TypeId {
        for layer in derived.iter().rev() {
            ty = match layer {
                DerivedDeclarator::Pointer { qualifiers, .. } => {
                    self.intern(TypeKind::Pointer(ty), *qualifiers)
                }
                DerivedDeclarator::Array { bound, .. } => self.intern(
                    TypeKind::Array {
                        element: ty,
                        bound: bound.clone(),
                    },
                    Qualifiers::default(),
                ),
                DerivedDeclarator::Function { parameters, .. } => {
                    let parameters = match parameters {
                        ParameterSyntax::Unspecified => FunctionParameters::Unspecified,
                        ParameterSyntax::Prototype {
                            parameters,
                            variadic,
                        } => FunctionParameters::Prototype {
                            types: parameters.iter().map(|parameter| parameter.ty).collect(),
                            variadic: *variadic,
                        },
                    };
                    self.intern(
                        TypeKind::Function {
                            return_type: ty,
                            parameters,
                        },
                        Qualifiers::default(),
                    )
                }
            };
        }
        ty
    }

    fn written_declarator(&self, declarator: &ParsedDeclarator) -> WrittenDeclarator {
        WrittenDeclarator {
            layers: declarator
                .derived
                .iter()
                .map(|layer| match layer {
                    DerivedDeclarator::Pointer { qualifiers, span } => {
                        WrittenDeclaratorLayer::Pointer {
                            qualifiers: *qualifiers,
                            span: *span,
                        }
                    }
                    DerivedDeclarator::Array { bound, span } => WrittenDeclaratorLayer::Array {
                        bound: bound.clone(),
                        span: *span,
                    },
                    DerivedDeclarator::Function { span, .. } => {
                        WrittenDeclaratorLayer::Function { span: *span }
                    }
                })
                .collect(),
            span: declarator.span,
        }
    }

    // ── statements ─────────────────────────────────────────────────────

    fn parse_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        match self.peek_kind()? {
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Switch => self.parse_switch_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::Do => self.parse_do_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => {
                let ret_tok = self.advance().unwrap();
                let start_span = ret_tok.span;
                if self.peek_kind() == Some(TokenKind::Semicolon) {
                    let semi_span = self.advance().expect("peeked semicolon").span;
                    let span = self.merge_span(start_span, semi_span);
                    return Some(self.alloc_node(CHirStmt::Return(None, span)));
                }
                let expr = self.parse_expression()?;
                let semi = self.expect(TokenKind::Semicolon);
                let end_span = semi.as_ref().map_or(expr.span(), |token| token.span);
                let span = self.merge_span(start_span, end_span);
                Some(self.alloc_node(CHirStmt::Return(Some(expr), span)))
            }
            TokenKind::Break => {
                let keyword = self.advance().expect("peeked break");
                if self.loop_depth == 0 && self.switch_depth == 0 {
                    self.report("E1201", "break-outside-loop-or-switch", keyword.span);
                }
                let semicolon = self.expect(TokenKind::Semicolon);
                let end = semicolon.as_ref().map_or(keyword.span, |token| token.span);
                let span = self.merge_span(keyword.span, end);
                Some(self.alloc_node(CHirStmt::Break(span)))
            }
            TokenKind::Continue => {
                let keyword = self.advance().expect("peeked continue");
                if self.loop_depth == 0 {
                    self.report("E1202", "continue-outside-loop", keyword.span);
                }
                let semicolon = self.expect(TokenKind::Semicolon);
                let end = semicolon.as_ref().map_or(keyword.span, |token| token.span);
                let span = self.merge_span(keyword.span, end);
                Some(self.alloc_node(CHirStmt::Continue(span)))
            }
            TokenKind::Goto => self.parse_goto_statement(),
            TokenKind::Case => self.parse_case_statement(),
            TokenKind::Default => self.parse_default_statement(),
            TokenKind::LBrace => Some(self.parse_compound_statement()),
            TokenKind::Semicolon => {
                let semi = self.advance().unwrap();
                Some(self.alloc_node(CHirStmt::Null(semi.span)))
            }
            TokenKind::Identifier
                if self.tokens.get(self.pos + 1).map(|token| token.kind)
                    == Some(TokenKind::Colon) =>
            {
                self.parse_label_statement()
            }
            _ => {
                let start = self.span();
                let expr = self.parse_expression()?;
                let semi = self.expect(TokenKind::Semicolon);
                let end = semi.as_ref().map_or(expr.span(), |token| token.span);
                let span = self.merge_span(start, end);
                Some(self.alloc_node(CHirStmt::Expr(expr, span)))
            }
        }
    }

    fn parse_if_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked if");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        let then_branch = self.parse_statement()?;
        let else_branch = if self.match_kind(TokenKind::Else) {
            Some(self.parse_statement()?)
        } else {
            None
        };
        let end = else_branch.map_or_else(|| then_branch.span(), CHirStmt::span);
        let span = self.merge_span(keyword.span, end);
        Some(self.alloc_node(CHirStmt::If(condition, then_branch, else_branch, span)))
    }

    fn parse_switch_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked switch");
        self.expect(TokenKind::LParen)?;
        let expression = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.switch_depth += 1;
        let body = self.parse_statement();
        self.switch_depth -= 1;
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::Switch(expression, body, span)))
    }

    fn parse_while_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked while");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::While(condition, body, span)))
    }

    fn parse_do_while_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked do");
        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        self.expect(TokenKind::While)?;
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_expression()?;
        self.expect(TokenKind::RParen)?;
        let semicolon = self.expect(TokenKind::Semicolon)?;
        let span = self.merge_span(keyword.span, semicolon.span);
        Some(self.alloc_node(CHirStmt::DoWhile(body, condition, span)))
    }

    fn parse_for_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked for");
        self.expect(TokenKind::LParen)?;

        let initializer = if self.match_kind(TokenKind::Semicolon) {
            CHirForInit::Empty
        } else if self
            .peek_kind()
            .is_some_and(|kind| self.is_declaration_start(kind))
        {
            let declarations = self.parse_declaration(DeclarationContext::ForInitializer)?;
            CHirForInit::Declaration(declarations)
        } else {
            let expression = self.parse_expression()?;
            self.expect(TokenKind::Semicolon)?;
            CHirForInit::Expression(expression)
        };

        let condition = if self.match_kind(TokenKind::Semicolon) {
            None
        } else {
            let expression = self.parse_expression()?;
            self.expect(TokenKind::Semicolon)?;
            Some(expression)
        };
        let step = if self.peek_kind() == Some(TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(TokenKind::RParen)?;

        self.loop_depth += 1;
        let body = self.parse_statement();
        self.loop_depth -= 1;
        let body = body?;
        let span = self.merge_span(keyword.span, body.span());
        Some(self.alloc_node(CHirStmt::For(initializer, condition, step, body, span)))
    }

    fn parse_goto_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked goto");
        let token = self.expect(TokenKind::Identifier)?;
        let label = SpannedName {
            spelling: self.token_text(&token).to_owned(),
            span: token.span,
        };
        let semicolon = self.expect(TokenKind::Semicolon)?;
        let span = self.merge_span(keyword.span, semicolon.span);
        Some(self.alloc_node(CHirStmt::Goto(label, span)))
    }

    fn parse_label_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let token = self.advance().expect("peeked label");
        let label = SpannedName {
            spelling: self.token_text(&token).to_owned(),
            span: token.span,
        };
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(token.span, nested.span());
        Some(self.alloc_node(CHirStmt::Label(label, nested, span)))
    }

    fn parse_case_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked case");
        if self.switch_depth == 0 {
            self.report("E1203", "case-outside-switch", keyword.span);
        }
        let value = self.parse_expression()?;
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(keyword.span, nested.span());
        Some(self.alloc_node(CHirStmt::Case(value, nested, span)))
    }

    fn parse_default_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked default");
        if self.switch_depth == 0 {
            self.report("E1204", "default-outside-switch", keyword.span);
        }
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(keyword.span, nested.span());
        Some(self.alloc_node(CHirStmt::Default(nested, span)))
    }

    fn parse_compound_statement(&mut self) -> &'arena CHirStmt<'arena> {
        let lbrace = self.expect(TokenKind::LBrace).unwrap();
        let start_span = lbrace.span;
        let mut items = Vec::new();
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            let before = self.pos;
            if self
                .peek_kind()
                .is_some_and(|kind| self.is_declaration_start(kind))
            {
                if let Some(declarations) = self.parse_declaration(DeclarationContext::Block) {
                    items.extend(declarations.into_iter().map(CHirBlockItem::Declaration));
                }
            } else if let Some(statement) = self.parse_statement() {
                items.push(CHirBlockItem::Statement(statement));
            }
            if self.pos == before {
                self.report("E1003", "expected-expression", self.span());
                self.advance();
            }
        }
        let rbrace = if self.peek_kind() == Some(TokenKind::RBrace) {
            self.advance()
        } else {
            self.expect(TokenKind::RBrace)
        };
        let end = rbrace.as_ref().map_or(start_span, |token| token.span);
        let span = self.merge_span(start_span, end);
        self.alloc_node(CHirStmt::Compound(items, span))
    }

    fn is_declaration_start(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Auto
                | TokenKind::Extern
                | TokenKind::Register
                | TokenKind::Static
                | TokenKind::ThreadLocal
                | TokenKind::Typedef
                | TokenKind::Const
                | TokenKind::Volatile
                | TokenKind::Restrict
                | TokenKind::Atomic
        ) || self.is_type_specifier_kind(kind)
    }

    fn is_type_specifier_kind(&self, kind: TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Int
                | TokenKind::Void
                | TokenKind::Long
                | TokenKind::Char
                | TokenKind::Short
                | TokenKind::Signed
                | TokenKind::Unsigned
                | TokenKind::Float
                | TokenKind::Double
                | TokenKind::Bool
                | TokenKind::Struct
                | TokenKind::Union
                | TokenKind::Enum
        )
    }

    // ── expressions ────────────────────────────────────────────────────

    fn parse_expression(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_comma()
    }

    fn parse_comma(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let mut expression = self.parse_assignment()?;
        while self.match_kind(TokenKind::Comma) {
            let right = self.parse_assignment()?;
            let span = self.merge_span(expression.span(), right.span());
            expression = self.alloc_node(CHirExpr::Comma(expression, right, span));
        }
        Some(expression)
    }

    fn parse_argument_expression(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let lhs = self.parse_conditional()?;
        if self.match_kind(TokenKind::Assign) {
            let rhs = self.parse_assignment()?;
            let span = self.merge_span(lhs.span(), rhs.span());
            return Some(self.alloc_node(CHirExpr::Assign(lhs, rhs, span)));
        }
        Some(lhs)
    }

    fn parse_conditional(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let condition = self.parse_binary(1)?;
        if !self.match_kind(TokenKind::Question) {
            return Some(condition);
        }
        let when_true = self.parse_expression()?;
        self.expect(TokenKind::Colon)?;
        let when_false = self.parse_conditional()?;
        let span = self.merge_span(condition.span(), when_false.span());
        Some(self.alloc_node(CHirExpr::Ternary(condition, when_true, when_false, span)))
    }

    fn parse_binary(&mut self, min_prec: u8) -> Option<&'arena CHirExpr<'arena>> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op_kind = match self.peek_kind() {
                Some(k) if prec(k) >= min_prec => k,
                _ => break,
            };
            let op = match binary_op(op_kind) {
                Some(op) => op,
                None => break,
            };
            let op_prec = prec(op_kind);
            self.advance();
            let rhs = self.parse_binary(op_prec + 1)?;
            let span = self.merge_span(lhs.span(), rhs.span());
            lhs = self.alloc_node(CHirExpr::Binary(op, lhs, rhs, span));
        }
        Some(lhs)
    }

    fn parse_unary(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let unary = match self.peek_kind() {
            Some(TokenKind::Minus) => Some(UnaryOp::Neg),
            Some(TokenKind::Plus) => Some(UnaryOp::Plus),
            Some(TokenKind::Bang) => Some(UnaryOp::Not),
            Some(TokenKind::Tilde) => Some(UnaryOp::BitNot),
            Some(TokenKind::Amp) => Some(UnaryOp::AddrOf),
            Some(TokenKind::Star) => Some(UnaryOp::Deref),
            Some(TokenKind::PlusPlus) => Some(UnaryOp::PreInc),
            Some(TokenKind::MinusMinus) => Some(UnaryOp::PreDec),
            _ => None,
        };
        if let Some(op) = unary {
            let token = self.advance().unwrap();
            let operand = self.parse_unary()?;
            let span = self.merge_span(token.span, operand.span());
            return Some(self.alloc_node(CHirExpr::Unary(op, operand, span)));
        }

        if self.peek_kind() == Some(TokenKind::Sizeof) {
            let token = self.advance().unwrap();
            if self.looks_like_parenthesized_type() {
                self.advance();
                let specifier = self.parse_type_specifier()?;
                let end = self.peek().map(|token| token.span).unwrap_or(token.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(token.span, end);
                if let Some(event) = specifier.tag_event {
                    let declaration = self.finish_tag_event(event, span);
                    self.side_declarations.push(declaration);
                }
                return Some(self.alloc_node(CHirExpr::SizeOf(specifier.surface.type_id(), span)));
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(token.span, operand.span());
            return Some(self.alloc_node(CHirExpr::SizeOfExpr(operand, span)));
        }

        if self.peek_kind() == Some(TokenKind::Alignof) {
            let token = self.advance().unwrap();
            self.expect(TokenKind::LParen)?;
            let specifier = if self
                .peek_kind()
                .is_some_and(|kind| self.is_type_specifier_kind(kind))
            {
                self.parse_type_specifier()
            } else {
                self.report("E1003", "expected-expression", self.span());
                None
            }?;
            let end = self.peek().map(|token| token.span).unwrap_or(token.span);
            self.expect(TokenKind::RParen)?;
            let span = self.merge_span(token.span, end);
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span);
                self.side_declarations.push(declaration);
            }
            return Some(self.alloc_node(CHirExpr::AlignOf(specifier.surface.type_id(), span)));
        }

        if self.looks_like_parenthesized_type() {
            let open = self.advance().unwrap();
            let specifier = self.parse_type_specifier()?;
            let close = self.expect(TokenKind::RParen)?;
            if !self.peek_kind().is_some_and(can_start_expression) {
                self.report("E1003", "expected-expression", close.span);
                return None;
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(open.span, operand.span());
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span);
                self.side_declarations.push(declaration);
            }
            return Some(self.alloc_node(CHirExpr::Cast(
                specifier.surface.type_id(),
                operand,
                span,
            )));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let mut expression = self.parse_primary()?;
        loop {
            match self.peek_kind() {
                Some(TokenKind::LParen) => {
                    self.advance();
                    let mut arguments = Vec::new();
                    if self.peek_kind() != Some(TokenKind::RParen) {
                        loop {
                            arguments.push(self.parse_argument_expression()?);
                            if !self.match_kind(TokenKind::Comma) {
                                break;
                            }
                        }
                    }
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RParen)?;
                    let span = self.merge_span(expression.span(), end);
                    expression = self.alloc_node(CHirExpr::Call(expression, arguments, span));
                }
                Some(TokenKind::LBracket) => {
                    self.advance();
                    let index = self.parse_expression()?;
                    let end = self
                        .peek()
                        .map(|token| token.span)
                        .unwrap_or_else(|| self.span());
                    self.expect(TokenKind::RBracket)?;
                    let span = self.merge_span(expression.span(), end);
                    expression = self.alloc_node(CHirExpr::Index(expression, index, span));
                }
                Some(TokenKind::Dot) | Some(TokenKind::Arrow) => {
                    let pointer = self.peek_kind() == Some(TokenKind::Arrow);
                    self.advance();
                    let Some(member) = self.advance() else {
                        self.report("E1003", "expected-expression", self.span());
                        break;
                    };
                    if member.kind != TokenKind::Identifier {
                        self.report("E1003", "expected-expression", member.span);
                        break;
                    }
                    let name = self.token_text(&member).to_owned();
                    let span = self.merge_span(expression.span(), member.span);
                    expression = if pointer {
                        self.alloc_node(CHirExpr::PtrMember(expression, name, span))
                    } else {
                        self.alloc_node(CHirExpr::Member(expression, name, span))
                    };
                }
                Some(TokenKind::PlusPlus) | Some(TokenKind::MinusMinus) => {
                    let token = self.advance().unwrap();
                    let op = if token.kind == TokenKind::PlusPlus {
                        UnaryOp::PostInc
                    } else {
                        UnaryOp::PostDec
                    };
                    let span = self.merge_span(expression.span(), token.span);
                    expression = self.alloc_node(CHirExpr::Unary(op, expression, span));
                }
                _ => break,
            }
        }
        Some(expression)
    }

    fn parse_primary(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let Some(tok) = self.advance() else {
            self.report("E1003", "expected-expression", self.span());
            return None;
        };
        let expression = match tok.kind {
            TokenKind::IntLiteral => {
                let text = self.token_text(&tok);
                let value = parse_int_literal(text);
                CHirExpr::Lit(
                    IntLit {
                        value,
                        suffix: IntSuffix::None,
                        base: IntBase::Dec,
                    },
                    tok.span,
                )
            }
            TokenKind::Identifier => {
                let name = self.token_text(&tok).to_owned();
                CHirExpr::Ident(name, tok.span)
            }
            TokenKind::FloatLiteral => CHirExpr::FloatLit(
                FloatLit {
                    value: self.token_text(&tok).parse().unwrap_or(0.0),
                    suffix: FloatSuffix::None,
                },
                tok.span,
            ),
            TokenKind::CharLiteral => CHirExpr::CharLit(
                CharLit {
                    value: 0,
                    prefix: CharPrefix::None,
                },
                tok.span,
            ),
            TokenKind::StringLiteral => CHirExpr::StringLit(
                StringLit {
                    value: self.token_text(&tok).to_owned(),
                    prefix: StringPrefix::None,
                },
                tok.span,
            ),
            TokenKind::LParen => {
                let expr = self.parse_expression()?;
                let rparen_span = self.peek().map(|t| t.span).unwrap_or(tok.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(tok.span, rparen_span);
                return Some(self.alloc_node(respan_expr(expr, span)));
            }
            _ => {
                self.report("E1003", "expected-expression", tok.span);
                return None;
            }
        };
        Some(self.alloc_node(expression))
    }

    fn looks_like_parenthesized_type(&self) -> bool {
        self.peek_kind() == Some(TokenKind::LParen)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| self.is_type_specifier_kind(token.kind))
    }
}

// ── precedence table ───────────────────────────────────────────────────────

fn prec(kind: TokenKind) -> u8 {
    match kind {
        TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 11,
        TokenKind::Plus | TokenKind::Minus => 10,
        TokenKind::ShiftLeft | TokenKind::ShiftRight => 9,
        TokenKind::Less | TokenKind::LessEq | TokenKind::Greater | TokenKind::GreaterEq => 8,
        TokenKind::EqEq | TokenKind::NotEq => 7,
        TokenKind::Amp => 6,
        TokenKind::Caret => 5,
        TokenKind::Pipe => 4,
        TokenKind::AmpAmp => 3,
        TokenKind::PipePipe => 2,
        _ => 0,
    }
}

fn binary_op(kind: TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::Plus => Some(BinaryOp::Add),
        TokenKind::Minus => Some(BinaryOp::Sub),
        TokenKind::Star => Some(BinaryOp::Mul),
        TokenKind::Slash => Some(BinaryOp::Div),
        TokenKind::Percent => Some(BinaryOp::Mod),
        TokenKind::Less => Some(BinaryOp::Lt),
        TokenKind::LessEq => Some(BinaryOp::Le),
        TokenKind::Greater => Some(BinaryOp::Gt),
        TokenKind::GreaterEq => Some(BinaryOp::Ge),
        TokenKind::EqEq => Some(BinaryOp::Eq),
        TokenKind::NotEq => Some(BinaryOp::Ne),
        TokenKind::ShiftLeft => Some(BinaryOp::Shl),
        TokenKind::ShiftRight => Some(BinaryOp::Shr),
        TokenKind::Amp => Some(BinaryOp::BitAnd),
        TokenKind::Caret => Some(BinaryOp::BitXor),
        TokenKind::Pipe => Some(BinaryOp::BitOr),
        TokenKind::AmpAmp => Some(BinaryOp::And),
        TokenKind::PipePipe => Some(BinaryOp::Or),
        _ => None,
    }
}

fn token_spelling(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Identifier => "identifier",
        TokenKind::While => "while",
        TokenKind::Semicolon => ";",
        TokenKind::RParen => ")",
        TokenKind::RBracket => "]",
        TokenKind::RBrace => "}",
        TokenKind::LParen => "(",
        TokenKind::Colon => ":",
        _ => "token",
    }
}

fn misplaced_statement_diagnostic(kind: Option<TokenKind>) -> Option<(&'static str, &'static str)> {
    match kind {
        Some(TokenKind::Break) => Some(("E1201", "break-outside-loop-or-switch")),
        Some(TokenKind::Continue) => Some(("E1202", "continue-outside-loop")),
        Some(TokenKind::Case) => Some(("E1203", "case-outside-switch")),
        Some(TokenKind::Default) => Some(("E1204", "default-outside-switch")),
        _ => None,
    }
}

fn can_start_expression(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::IntLiteral
            | TokenKind::FloatLiteral
            | TokenKind::CharLiteral
            | TokenKind::StringLiteral
            | TokenKind::LParen
            | TokenKind::Minus
            | TokenKind::Plus
            | TokenKind::Bang
            | TokenKind::Tilde
            | TokenKind::Amp
            | TokenKind::Star
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::Sizeof
            | TokenKind::Alignof
    )
}

// ── integer literal parsing ────────────────────────────────────────────────

fn parse_int_literal(text: &str) -> u64 {
    if text.starts_with("0x") || text.starts_with("0X") {
        u64::from_str_radix(&text[2..], 16).unwrap_or(0)
    } else if text.starts_with('0')
        && text.len() > 1
        && !text.starts_with("0x")
        && !text.starts_with("0X")
    {
        u64::from_str_radix(&text[1..], 8).unwrap_or(0)
    } else {
        text.parse().unwrap_or(0)
    }
}

/// When a parenthesized expression appears, preserve its original span.
fn respan_expr<'arena>(inner: &CHirExpr<'arena>, paren_span: SourceSpan) -> CHirExpr<'arena> {
    match inner {
        CHirExpr::Lit(literal, _) => CHirExpr::Lit(*literal, paren_span),
        CHirExpr::FloatLit(literal, _) => CHirExpr::FloatLit(*literal, paren_span),
        CHirExpr::CharLit(literal, _) => CHirExpr::CharLit(*literal, paren_span),
        CHirExpr::StringLit(literal, _) => CHirExpr::StringLit(literal.clone(), paren_span),
        CHirExpr::Ident(name, _) => CHirExpr::Ident(name.clone(), paren_span),
        CHirExpr::Unary(op, operand, _) => CHirExpr::Unary(*op, operand, paren_span),
        CHirExpr::Binary(op, lhs, rhs, _) => CHirExpr::Binary(*op, lhs, rhs, paren_span),
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            CHirExpr::Ternary(condition, when_true, when_false, paren_span)
        }
        CHirExpr::Call(callee, arguments, _) => {
            CHirExpr::Call(callee, arguments.clone(), paren_span)
        }
        CHirExpr::Cast(ty, operand, _) => CHirExpr::Cast(*ty, operand, paren_span),
        CHirExpr::SizeOf(ty, _) => CHirExpr::SizeOf(*ty, paren_span),
        CHirExpr::SizeOfExpr(operand, _) => CHirExpr::SizeOfExpr(operand, paren_span),
        CHirExpr::AlignOf(ty, _) => CHirExpr::AlignOf(*ty, paren_span),
        CHirExpr::Member(object, name, _) => CHirExpr::Member(object, name.clone(), paren_span),
        CHirExpr::PtrMember(object, name, _) => {
            CHirExpr::PtrMember(object, name.clone(), paren_span)
        }
        CHirExpr::Index(object, index, _) => CHirExpr::Index(object, index, paren_span),
        CHirExpr::Assign(lhs, rhs, _) => CHirExpr::Assign(lhs, rhs, paren_span),
        CHirExpr::CompoundLit(ty, values, _) => {
            CHirExpr::CompoundLit(*ty, values.clone(), paren_span)
        }
        CHirExpr::Comma(lhs, rhs, _) => CHirExpr::Comma(lhs, rhs, paren_span),
    }
}

fn integer_constant_value(expression: &CHirExpr<'_>) -> i64 {
    match expression {
        CHirExpr::Lit(literal, _) => literal.value as i64,
        CHirExpr::Unary(UnaryOp::Neg, operand, _) => -integer_constant_value(operand),
        CHirExpr::Unary(UnaryOp::Plus, operand, _) => integer_constant_value(operand),
        CHirExpr::Unary(UnaryOp::BitNot, operand, _) => !integer_constant_value(operand),
        _ => 0,
    }
}

// ── tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_lex::Lexer;

    fn parse_unit<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
        let mut lexer = Lexer::new(FileId::new(99), source);
        let mut tokens = Vec::new();
        while let Some(tok) = lexer.next() {
            tokens.push(tok);
        }
        let mut parser = Parser::new(tokens, FileId::new(99), source);
        let unit = parser.parse_translation_unit_in(arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        unit
    }

    fn parse_expr<'arena>(source: &str, arena: &'arena Arena) -> &'arena CHirExpr<'arena> {
        let wrapped = format!("int sink = {source}");
        let unit = parse_unit(&wrapped, arena);
        let CHirDecl::Object(object) = unit.declarations()[0] else {
            panic!("expected initialized object")
        };
        object.initializer.expect("expected initializer")
    }

    fn parse_rejected<'arena>(
        source: &str,
        arena: &'arena Arena,
    ) -> (CHirUnit<'arena>, ParseDiagnostic) {
        let mut lexer = Lexer::new(FileId::new(99), source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens, FileId::new(99), source);
        let unit = parser.parse_translation_unit_in(arena);
        let diagnostic = parser
            .diagnostic()
            .cloned()
            .expect("malformed input must produce one diagnostic");
        (unit, diagnostic)
    }

    #[test]
    fn parser_recognizes_declarators() {
        let arena = Arena::default();
        let unit = parse_unit("int x;", &arena);
        assert!(!unit.is_empty());
        match unit.declarations()[0] {
            CHirDecl::Object(object) => {
                assert_eq!(object.name.spelling, "x");
                assert!(matches!(
                    unit.types().get(object.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Int(IntRank::Int))
                ));
                assert!(object.initializer.is_none());
            }
            _ => panic!("expected VarDecl, got {:?}", unit[0]),
        }
    }

    #[test]
    fn parser_recognizes_declaration_with_init() {
        let arena = Arena::default();
        let unit = parse_unit("int y = 42;", &arena);
        assert!(!unit.is_empty());
        match unit.declarations()[0] {
            CHirDecl::Object(object) => {
                assert_eq!(object.name.spelling, "y");
                assert!(matches!(
                    unit.types().get(object.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Int(IntRank::Int))
                ));
                assert!(object.initializer.is_some());
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn parser_builds_expression_tree() {
        let arena = Arena::default();
        let unit = parse_unit("int r = 1 + 2 * 3;", &arena);
        assert!(!unit.is_empty());
        match unit.declarations()[0] {
            CHirDecl::Object(ObjectDecl {
                initializer: Some(init),
                ..
            }) => match init {
                CHirExpr::Binary(BinaryOp::Add, lhs, rhs, _) => {
                    assert!(matches!(lhs, CHirExpr::Lit(_, _)));
                    assert!(matches!(rhs, CHirExpr::Binary(BinaryOp::Mul, _, _, _)));
                }
                _ => panic!("expected Add at root with Mul as right child"),
            },
            _ => panic!("expected declaration with initializer"),
        }
    }

    #[test]
    fn parser_builds_function_definition() {
        let arena = Arena::default();
        let unit = parse_unit("int f(int x) { return x + 1; }", &arena);
        assert_eq!(unit.len(), 1);
        match unit.declarations()[0] {
            CHirDecl::Function(function) => {
                assert_eq!(function.name.spelling, "f");
                let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
                    panic!("expected function prototype parameters")
                };
                assert_eq!(parameters.len(), 1);
                assert_eq!(parameters[0].name.as_ref().unwrap().spelling, "x");
                assert!(matches!(
                    unit.types()
                        .get(function.return_specifier.type_id())
                        .map(|ty| &ty.kind),
                    Some(TypeKind::Int(IntRank::Int))
                ));
                match function.body.expect("expected function body") {
                    CHirStmt::Compound(stmts, _) => {
                        assert!(!stmts.is_empty());
                        assert!(matches!(
                            stmts[0],
                            CHirBlockItem::Statement(CHirStmt::Return(Some(_), _))
                        ));
                    }
                    _ => panic!("expected compound statement body"),
                }
            }
            _ => panic!("expected function declaration"),
        }
    }

    #[test]
    fn parser_handles_parenthesized_expressions() {
        let arena = Arena::default();
        let expr = parse_expr("(1 + 2) * 3;", &arena);
        match expr {
            CHirExpr::Binary(BinaryOp::Mul, lhs, rhs, _) => {
                assert!(matches!(*lhs, CHirExpr::Binary(BinaryOp::Add, _, _, _)));
                assert!(matches!(*rhs, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected Mul of (Add) * Lit"),
        }
    }

    #[test]
    fn parser_handles_comparison_operators() {
        let arena = Arena::default();
        let expr = parse_expr("1 < 2;", &arena);
        assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Lt, _, _, _)));
    }

    #[test]
    fn parser_handles_equality_operators() {
        let arena = Arena::default();
        let expr = parse_expr("1 == 2;", &arena);
        assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Eq, _, _, _)));
    }

    #[test]
    fn parser_handles_unary_negation() {
        let arena = Arena::default();
        let expr = parse_expr("-42;", &arena);
        match expr {
            CHirExpr::Unary(UnaryOp::Neg, inner, _) => {
                assert!(matches!(*inner, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected unary negation"),
        }
    }

    #[test]
    fn parser_handles_assignment() {
        let arena = Arena::default();
        let expr = parse_expr("x = 5;", &arena);
        match expr {
            CHirExpr::Assign(lhs, rhs, _) => {
                assert!(matches!(*lhs, CHirExpr::Ident(_, _)));
                assert!(matches!(*rhs, CHirExpr::Lit(_, _)));
            }
            _ => panic!("expected assignment"),
        }
    }

    #[test]
    fn parser_empty_statement() {
        let arena = Arena::default();
        let unit = parse_unit("void f(void) { ; ; }", &arena);
        assert_eq!(unit.len(), 1);
        match unit.declarations()[0] {
            CHirDecl::Function(function) => match function.body.expect("expected body") {
                CHirStmt::Compound(stmts, _) => {
                    assert_eq!(stmts.len(), 2);
                    assert!(matches!(
                        stmts[0],
                        CHirBlockItem::Statement(CHirStmt::Null(_))
                    ));
                }
                _ => panic!("expected compound"),
            },
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn misplaced_statement_keywords_have_context_diagnostics() {
        let cases = [
            ("break;", "break", "E1201", "break-outside-loop-or-switch"),
            ("continue;", "continue", "E1202", "continue-outside-loop"),
            ("case 1: ;", "case", "E1203", "case-outside-switch"),
            ("default: ;", "default", "E1204", "default-outside-switch"),
            (
                "int f(void) { break; }",
                "break",
                "E1201",
                "break-outside-loop-or-switch",
            ),
            (
                "int f(void) { continue; }",
                "continue",
                "E1202",
                "continue-outside-loop",
            ),
            (
                "int f(void) { case 1: ; }",
                "case",
                "E1203",
                "case-outside-switch",
            ),
            (
                "int f(void) { default: ; }",
                "default",
                "E1204",
                "default-outside-switch",
            ),
        ];

        for (source, keyword, code, message) in cases {
            let arena = Arena::default();
            let (_, diagnostic) = parse_rejected(source, &arena);
            let start = source.find(keyword).expect("keyword in source") as u32;
            assert_eq!(diagnostic.code, code, "source {source:?}");
            assert_eq!(diagnostic.message, message, "source {source:?}");
            assert_eq!(diagnostic.span.file, FileId::new(99));
            assert_eq!(diagnostic.span.byte_start, start, "source {source:?}");
            assert_eq!(
                diagnostic.span.byte_end,
                start + keyword.len() as u32,
                "source {source:?}"
            );
        }
    }

    #[test]
    fn malformed_expression_does_not_fabricate_a_chir_declaration() {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected("int broken = !;", &arena);
        assert_eq!(diagnostic.code, "E1003");
        assert_eq!(diagnostic.message, "expected-expression");
        assert!(
            unit.is_empty(),
            "a rejected expression must not survive in reachable C-HIR: {unit:?}"
        );
    }
}
