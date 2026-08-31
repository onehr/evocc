//! C declarator, expression, and statement parser producing C-HIR

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;

use evocc_chir::integer::{
    enum_integer_type, eval_binary_integer, eval_bit_builtin, eval_unary_integer,
    floating_literal_to_integer, integer_literal_value, integer_type_for_rank,
    rank_for_integer_type, usual_arithmetic_type, BitBuiltinOp, CInteger, CIntegerType,
};
use evocc_chir::{
    builtin_constant_p, AppliedAttribute, AsmOperand, AsmStmt, AssignmentOp, Attribute,
    AttributeArgument, AttributePlacement, BinaryOp, CHirBlockItem, CHirDecl, CHirExpr,
    CHirForInit, CHirStmt, CHirUnit, CharLit, CharPrefix, CheckedArithmeticOp, EnumeratorDecl,
    ExprCategory, ExprFacts, FieldProjection, FloatLit, FloatSuffix, FrontEndBuiltinRecord,
    FunctionDecl, GenericSelectionRecord, IndexProjection, InitAction, InitPlan,
    InitializerDesignation, IntBase, IntLit, IntSuffix, MemoryPlace, ObjectDecl, ObjectInitializer,
    ParameterDecl, ParameterSyntax, PlaceProjection, PlaceRoot, PointerTypeDesc, RecordMember,
    ScalarPlace, ScopeId, ScopeKind, SpannedName, StorageClass, StringLit, StringPrefix,
    SubobjectPath, SubobjectStep, SymbolData, SymbolId, SymbolKind, SymbolTable, SymbolTableError,
    TagDecl, TagMembers, TypeSpecifier, TypedefDecl, TypeofOperand, UnaryOp, UpdateOp,
    WrittenDeclarator, WrittenDeclaratorLayer, WrittenDesignator, WrittenInitializer,
};
use evocc_common::{Arena, FileId, SourceSpan};
use evocc_cpp::{pragma_pack_action, PragmaPackAction};
use evocc_lex::{Token, TokenKind, TokenText};
use evocc_types::{
    ArrayBound, BitFieldSpec, CanonicalType, ComplexBase, EnumConstant, FieldLayoutAttributes,
    FloatKind, FunctionParameters, GnuInt128Signedness, IntRank, Qualifiers, RecordField,
    TagDefinition, TagId, TagKind, TagLayoutAttributes, TagState, TargetDataLayout, TypeId,
    TypeInterner, TypeKind,
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
        attributes: Vec<Attribute>,
        span: SourceSpan,
    },
    Array {
        bound: ArrayBound,
        span: SourceSpan,
    },
    Function {
        parameters: ParameterSyntax,
        scope: ScopeId,
        span: SourceSpan,
    },
}

fn derived_declarator_span(layer: &DerivedDeclarator) -> SourceSpan {
    match layer {
        DerivedDeclarator::Pointer { span, .. }
        | DerivedDeclarator::Array { span, .. }
        | DerivedDeclarator::Function { span, .. } => *span,
    }
}

#[derive(Debug, Clone)]
struct ParsedDeclarator {
    name: SpannedName,
    derived: Vec<DerivedDeclarator>,
    span: SourceSpan,
}

#[derive(Debug, Clone)]
struct ParsedAbstractDeclarator {
    derived: Vec<DerivedDeclarator>,
    span: SourceSpan,
}

struct ParsedSpecifier<'arena> {
    surface: TypeSpecifier,
    tag_event: Option<PendingTag<'arena>>,
    inherited_function_parameters: Option<ParameterSyntax>,
}

struct PendingTag<'arena> {
    tag: TagId,
    scope: ScopeId,
    name: Option<SpannedName>,
    members: TagMembers<'arena>,
    specifier_span: SourceSpan,
    attributes: Vec<Attribute>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeclarationContext {
    File,
    Block,
    ForInitializer,
}

#[derive(Debug, Clone, Copy)]
struct DirectUpdateTarget {
    symbol: SymbolId,
    ty: TypeId,
}

#[derive(Debug, Clone)]
enum TypedUpdateTarget<'arena> {
    Direct(DirectUpdateTarget),
    Memory(MemoryPlace<'arena>),
}

impl TypedUpdateTarget<'_> {
    fn ty(&self) -> TypeId {
        match self {
            Self::Direct(target) => target.ty,
            Self::Memory(place) => place.value_ty(),
        }
    }
}

/// The pointed-to type of a converted pointer expression. `pointee` carries
/// structural identity while `qualifiers` may be the union formed by a
/// conditional expression even when no matching canonical TypeId exists yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InferredPointerType {
    pointee: TypeId,
    qualifiers: Qualifiers,
}

/// A side-effect-free expression-type fact used while validating update
/// operators. Synthetic literal/arithmetic types are kept out of the
/// canonical interner so semantic checking cannot perturb declaration TypeId
/// allocation or source-order fingerprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InferredType {
    Canonical(TypeId),
    Integer(CIntegerType),
    Real(FloatKind),
    Complex(ComplexBase),
    Pointer(InferredPointerType),
    StringArray(StringPrefix),
    StringPointer(StringPrefix),
}

#[derive(Debug, Clone, Copy)]
struct InferredExpressionType {
    ty: InferredType,
    category: ExprCategory,
    null_pointer_constant: bool,
}

#[derive(Debug, Clone, Copy)]
enum OperandTypeClass {
    Integer(CIntegerType),
    Real(FloatKind),
    Complex(ComplexBase),
    Pointer(InferredPointerType),
    StringPointer(StringPrefix),
    Aggregate(TypeId),
    Void,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateConstraint {
    Scalar,
    PointerBoundary,
    AggregateBoundary,
    Invalid,
}

const MAX_INITIALIZER_SUBOBJECTS: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitializerPreflightError {
    TooLarge,
    RecursiveObject,
    InvalidObject,
}

#[derive(Debug, Clone)]
struct ResolvedInitializerTarget {
    path: SubobjectPath,
    ty: TypeId,
}

fn checked_initializer_action_add(left: u64, right: u64) -> Result<u64, InitializerPreflightError> {
    let total = left
        .checked_add(right)
        .ok_or(InitializerPreflightError::TooLarge)?;
    if total > MAX_INITIALIZER_SUBOBJECTS {
        Err(InitializerPreflightError::TooLarge)
    } else {
        Ok(total)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntityLinkage {
    None,
    Internal,
    External,
}

#[derive(Debug, Clone, Copy)]
struct EntityState {
    linkage: EntityLinkage,
    thread_local: bool,
    has_definition: bool,
}

#[derive(Debug)]
struct DeclarationTransaction {
    entity_undo: usize,
    external_entity_undo: usize,
    inherited_function_parameter_undo: usize,
    side_declarations: usize,
}

#[derive(Debug, Clone)]
struct TemporaryParameterShadow {
    scope: ScopeId,
    spelling: String,
    ty: TypeId,
    span: SourceSpan,
}

#[derive(Debug)]
struct SwitchCaseContext {
    selector_type: Option<CIntegerType>,
    intervals: Vec<(i128, i128)>,
}

pub struct Parser<'src, 'arena> {
    tokens: Vec<Token>,
    pos: usize,
    file: FileId,
    source: &'src str,
    had_error: bool,
    error_count: usize,
    error_limit: Option<NonZeroUsize>,
    rollback_error_count: usize,
    diagnostics: Vec<ParseDiagnostic>,
    arena: Option<&'arena Arena>,
    types: TypeInterner,
    symbols: SymbolTable,
    current_scope: ScopeId,
    /// Parameter names whose C point of declaration has been reached while
    /// their prototype is still being parsed. They shadow outer typedef names
    /// immediately, but receive permanent SymbolIds only after the enclosing
    /// function declarator is complete, preserving declaration-point ID order.
    temporary_parameter_shadows: Vec<TemporaryParameterShadow>,
    /// Parameter syntax retained for symbols whose declared type is a
    /// function. Later declarations through a function-type typedef or
    /// `typeof(function-designator)` have no written function suffix of their
    /// own, so the named source declaration is their prototype authority.
    inherited_function_parameters: HashMap<SymbolId, ParameterSyntax>,
    entities: HashMap<SymbolId, EntityState>,
    external_entities: HashMap<String, SymbolId>,
    entity_undo: Vec<(SymbolId, Option<EntityState>)>,
    external_entity_undo: Vec<(String, Option<SymbolId>)>,
    inherited_function_parameter_undo: Vec<(SymbolId, Option<ParameterSyntax>)>,
    declaration_transaction_depth: usize,
    side_declarations: Vec<&'arena CHirDecl<'arena>>,
    loop_depth: usize,
    /// Attributes on labels and type names, which have no declaration node.
    detached_attributes: Vec<Attribute>,
    generic_selections: Vec<GenericSelectionRecord>,
    front_end_builtins: Vec<FrontEndBuiltinRecord<'arena>>,
    switch_depth: usize,
    switch_cases: Vec<SwitchCaseContext>,
    local_label_scopes: Vec<HashMap<String, String>>,
    next_local_label: u64,
    pack_alignment: Option<u32>,
    pack_stack: Vec<Option<u32>>,
    /// True only while normalizing an initializer attached to a file-scope
    /// object.  It admits the final flexible member without changing the
    /// record type itself.
    flexible_initializer_allowed: bool,
}

impl<'src, 'arena> Parser<'src, 'arena> {
    pub fn new(mut tokens: Vec<Token>, file: FileId, source: &'src str) -> Self {
        // Preprocessing numbers and `other` characters remain available to
        // macro expansion through translation phase 4. If either survives to
        // this phase-7 boundary, it has no C-token form and is diagnosed at
        // the exact preserved token rather than leaking into syntax errors.
        let lexical_diagnostic = tokens
            .iter()
            .find(|token| matches!(token.kind, TokenKind::PpNumber | TokenKind::PpOther))
            .map(|token| ParseDiagnostic {
                span: token.span,
                code: "E0112",
                message: "stray-token".to_owned(),
            });
        // GNU contextual spellings must remain identifiers through
        // preprocessing so they can be macro-expanded or token-pasted. Turn
        // an exact surviving spelling into its parser token only at this
        // boundary.
        for token in &mut tokens {
            let spelling = match &token.text {
                TokenText::Decoded(text) => Some(text.as_str()),
                TokenText::Borrowed => {
                    source.get(token.span.byte_start as usize..token.span.byte_end as usize)
                }
            };
            if token.kind == TokenKind::Identifier {
                token.kind = match spelling {
                    Some("__signed__") => TokenKind::Signed,
                    Some("__extension__") => TokenKind::GnuExtension,
                    Some("__builtin_va_list") => TokenKind::BuiltinVaList,
                    Some("__int128" | "__int128_t" | "__uint128_t") => TokenKind::GnuInt128,
                    Some("__alignof__") => TokenKind::GnuAlignof,
                    Some("__builtin_expect") => TokenKind::BuiltinExpect,
                    Some("__builtin_constant_p") => TokenKind::BuiltinConstantP,
                    Some("__builtin_choose_expr") => TokenKind::BuiltinChooseExpr,
                    Some("__builtin_types_compatible_p") => TokenKind::BuiltinTypesCompatibleP,
                    Some("__builtin_offsetof") => TokenKind::BuiltinOffsetof,
                    Some("__builtin_object_size") => TokenKind::BuiltinObjectSize,
                    Some(
                        "__builtin_ffs"
                        | "__builtin_ffsl"
                        | "__builtin_ffsll"
                        | "__builtin_clz"
                        | "__builtin_clzl"
                        | "__builtin_clzll"
                        | "__builtin_ctz"
                        | "__builtin_ctzl"
                        | "__builtin_ctzll"
                        | "__builtin_popcount"
                        | "__builtin_popcountl"
                        | "__builtin_popcountll",
                    ) => TokenKind::BuiltinBit,
                    Some("__builtin_unreachable") => TokenKind::BuiltinUnreachable,
                    Some("__builtin_return_address") => TokenKind::BuiltinReturnAddress,
                    Some("__auto_type") => TokenKind::AutoType,
                    Some("__label__") => TokenKind::LocalLabel,
                    Some("__builtin_add_overflow") => TokenKind::BuiltinAddOverflow,
                    Some("__builtin_sub_overflow") => TokenKind::BuiltinSubOverflow,
                    Some("__builtin_mul_overflow") => TokenKind::BuiltinMulOverflow,
                    Some("asm" | "__asm__") => TokenKind::Asm,
                    Some("__volatile__") => TokenKind::Volatile,
                    _ => TokenKind::Identifier,
                };
            }
        }
        let symbols = SymbolTable::new();
        let current_scope = symbols.file_scope();
        Self {
            tokens,
            pos: 0,
            file,
            source,
            had_error: lexical_diagnostic.is_some(),
            error_count: usize::from(lexical_diagnostic.is_some()),
            error_limit: None,
            rollback_error_count: usize::from(lexical_diagnostic.is_some()),
            diagnostics: lexical_diagnostic.into_iter().collect(),
            arena: None,
            types: TypeInterner::new(),
            symbols,
            current_scope,
            temporary_parameter_shadows: Vec::new(),
            inherited_function_parameters: HashMap::new(),
            entities: HashMap::new(),
            external_entities: HashMap::new(),
            entity_undo: Vec::new(),
            external_entity_undo: Vec::new(),
            inherited_function_parameter_undo: Vec::new(),
            declaration_transaction_depth: 0,
            side_declarations: Vec::new(),
            loop_depth: 0,
            detached_attributes: Vec::new(),
            generic_selections: Vec::new(),
            front_end_builtins: Vec::new(),
            switch_depth: 0,
            switch_cases: Vec::new(),
            local_label_scopes: Vec::new(),
            next_local_label: 0,
            pack_alignment: None,
            pack_stack: Vec::new(),
            flexible_initializer_allowed: false,
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

    pub fn parse_translation_unit_in(&mut self, arena: &'arena Arena) -> CHirUnit<'arena> {
        assert!(self.arena.is_none(), "Parser instances parse only one unit");
        self.arena = Some(arena);
        let mut decls = Vec::new();
        while !self.is_done() && !self.reached_error_limit() {
            if self.consume_pragma_pack_marker() {
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
        let mut unit = CHirUnit::new(
            decls,
            std::mem::take(&mut self.types),
            std::mem::take(&mut self.symbols),
        );
        unit.set_detached_attributes(std::mem::take(&mut self.detached_attributes));
        unit.set_generic_selections(std::mem::take(&mut self.generic_selections));
        unit.set_front_end_builtins(std::mem::take(&mut self.front_end_builtins));
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

    fn reached_error_limit(&self) -> bool {
        self.error_limit
            .is_some_and(|limit| self.error_count >= limit.get())
    }

    fn consume_pragma_pack_marker(&mut self) -> bool {
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
        // An unresolved use is removed at the expression/statement boundary,
        // so a surrounding function can still retain its fully resolved
        // declarations and scopes for diagnostics. Declaration-shape and
        // redefinition errors remain transactional.
        if code != "E2001" {
            self.rollback_error_count += 1;
        }
        {
            self.diagnostics.push(ParseDiagnostic {
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
        // A SourceSpan is a range within exactly one source file. Most parser
        // composites join tokens from that same file, so preserve the token
        // origin instead of replacing it with the parser's main-file ID.
        //
        // Macro expansion can eventually place endpoints from different
        // files in one syntactic node. Until CHIR has an explicit expansion
        // provenance model, fail closed to the start token as the node's
        // primary origin. Combining offsets from different files would create
        // a range that is valid in neither source.
        if start.file != end.file {
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
    fn alloc_node<T: 'arena>(&self, value: T) -> &'arena T {
        let arena = self.arena.expect("parse_translation_unit_in sets Arena");
        // Safety: upheld by the module invariant documented above.
        unsafe { arena.alloc_shared(value) }
    }

    fn intern(&mut self, kind: TypeKind, qualifiers: Qualifiers) -> TypeId {
        self.types.intern(CanonicalType { kind, qualifiers })
    }

    fn qualify_typedef_type(&mut self, ty: TypeId, qualifiers: Qualifiers) -> TypeId {
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

    fn strip_top_level_qualifiers(&mut self, ty: TypeId) -> TypeId {
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

    fn symbol_in_current_scope(&self, spelling: &str) -> Option<SymbolId> {
        self.symbols
            .scope_symbols(self.current_scope)
            .expect("the parser current scope is always live")
            .iter()
            .rev()
            .copied()
            .find(|symbol| {
                self.symbols
                    .symbol(*symbol)
                    .is_ok_and(|symbol| symbol.spelling() == spelling)
            })
    }

    fn linked_symbol_in_parent_scopes(&self, spelling: &str) -> Option<SymbolId> {
        let mut scope = self
            .symbols
            .scope(self.current_scope)
            .expect("the parser current scope is always live")
            .parent();
        while let Some(scope_id) = scope {
            let lexical_scope = self
                .symbols
                .scope(scope_id)
                .expect("scope parent IDs belong to the same table");
            if let Some(symbol) = lexical_scope
                .symbols()
                .iter()
                .rev()
                .copied()
                .find(|symbol| {
                    self.symbols
                        .symbol(*symbol)
                        .is_ok_and(|symbol| symbol.spelling() == spelling)
                })
            {
                if self
                    .entities
                    .get(&symbol)
                    .is_some_and(|state| state.linkage != EntityLinkage::None)
                {
                    return Some(symbol);
                }
            }
            scope = lexical_scope.parent();
        }
        None
    }

    fn begin_declaration_transaction(&mut self) -> DeclarationTransaction {
        self.declaration_transaction_depth += 1;
        DeclarationTransaction {
            entity_undo: self.entity_undo.len(),
            external_entity_undo: self.external_entity_undo.len(),
            inherited_function_parameter_undo: self.inherited_function_parameter_undo.len(),
            side_declarations: self.side_declarations.len(),
        }
    }

    fn commit_declaration_transaction(&mut self, transaction: &DeclarationTransaction) {
        debug_assert!(self.declaration_transaction_depth > 0);
        debug_assert!(self.entity_undo.len() >= transaction.entity_undo);
        debug_assert!(self.external_entity_undo.len() >= transaction.external_entity_undo);
        debug_assert!(
            self.inherited_function_parameter_undo.len()
                >= transaction.inherited_function_parameter_undo
        );
        self.declaration_transaction_depth -= 1;
        if self.declaration_transaction_depth == 0 {
            self.entity_undo.clear();
            self.external_entity_undo.clear();
            self.inherited_function_parameter_undo.clear();
        }
    }

    fn rollback_declaration_transaction(&mut self, transaction: &DeclarationTransaction) {
        while self.entity_undo.len() > transaction.entity_undo {
            let (symbol, previous) = self.entity_undo.pop().expect("undo marker is bounded");
            match previous {
                Some(previous) => {
                    self.entities.insert(symbol, previous);
                }
                None => {
                    self.entities.remove(&symbol);
                }
            }
        }
        while self.external_entity_undo.len() > transaction.external_entity_undo {
            let (spelling, previous) = self
                .external_entity_undo
                .pop()
                .expect("undo marker is bounded");
            match previous {
                Some(previous) => {
                    self.external_entities.insert(spelling, previous);
                }
                None => {
                    self.external_entities.remove(&spelling);
                }
            }
        }
        while self.inherited_function_parameter_undo.len()
            > transaction.inherited_function_parameter_undo
        {
            let (symbol, previous) = self
                .inherited_function_parameter_undo
                .pop()
                .expect("undo marker is bounded");
            match previous {
                Some(previous) => {
                    self.inherited_function_parameters.insert(symbol, previous);
                }
                None => {
                    self.inherited_function_parameters.remove(&symbol);
                }
            }
        }
        self.side_declarations
            .truncate(transaction.side_declarations);
        self.declaration_transaction_depth -= 1;
        if self.declaration_transaction_depth == 0 {
            self.entity_undo.clear();
            self.external_entity_undo.clear();
            self.inherited_function_parameter_undo.clear();
        }
    }

    fn set_entity_state(&mut self, symbol: SymbolId, state: EntityState) {
        let previous = self.entities.insert(symbol, state);
        if self.declaration_transaction_depth > 0 {
            self.entity_undo.push((symbol, previous));
        }
    }

    fn set_external_entity(&mut self, spelling: String, symbol: SymbolId) {
        let previous = self.external_entities.insert(spelling.clone(), symbol);
        if self.declaration_transaction_depth > 0 {
            self.external_entity_undo.push((spelling, previous));
        }
    }

    fn set_inherited_function_parameters(&mut self, symbol: SymbolId, parameters: ParameterSyntax) {
        let previous = self
            .inherited_function_parameters
            .insert(symbol, parameters);
        if self.declaration_transaction_depth > 0 {
            self.inherited_function_parameter_undo
                .push((symbol, previous));
        }
    }

    fn declare_symbol(&mut self, name: &SpannedName, data: SymbolData, ty: TypeId) -> SymbolId {
        let in_current_scope = self.symbol_in_current_scope(&name.spelling);
        let lexically_visible = in_current_scope.or_else(|| {
            if matches!(data.kind(), SymbolKind::Function)
                || matches!(
                    data,
                    SymbolData::Object {
                        storage: StorageClass::Extern
                    }
                )
            {
                self.linked_symbol_in_parent_scopes(&name.spelling)
            } else {
                None
            }
        });
        let new_linkage = self.declaration_linkage(data);
        let existing = lexically_visible.or_else(|| {
            (new_linkage != EntityLinkage::None
                && matches!(data.kind(), SymbolKind::Object | SymbolKind::Function))
            .then(|| self.external_entities.get(&name.spelling).copied())
            .flatten()
        });

        if let Some(existing) = existing {
            let joins_outer_scope = in_current_scope.is_none();
            if self.merge_redeclaration(existing, data, ty) {
                if joins_outer_scope {
                    self.symbols
                        .bind_existing(self.current_scope, existing)
                        .expect("a resolved outer spelling is not duplicated locally");
                }
                return existing;
            }
            self.report_redefinition(name);
            return existing;
        }

        match self
            .symbols
            .declare(self.current_scope, &name.spelling, data, ty, name.span)
        {
            Ok(symbol) => {
                let linkage = self.declaration_linkage(data);
                self.set_entity_state(
                    symbol,
                    EntityState {
                        linkage,
                        thread_local: data.storage_class() == Some(StorageClass::ThreadLocal),
                        has_definition: false,
                    },
                );
                if linkage == EntityLinkage::External {
                    self.set_external_entity(name.spelling.clone(), symbol);
                }
                symbol
            }
            Err(SymbolTableError::DuplicateInScope { existing, .. }) => {
                self.report_redefinition(name);
                existing
            }
            Err(error) => panic!("parser constructed an invalid symbol-table operation: {error}"),
        }
    }

    fn report_redefinition(&mut self, name: &SpannedName) {
        self.report(
            "E2002",
            format!("ordinary-identifier-redefinition: {}", name.spelling),
            name.span,
        );
    }

    fn declaration_linkage(&self, data: SymbolData) -> EntityLinkage {
        let file_scope = self.current_scope == self.symbols.file_scope();
        match data {
            SymbolData::Function {
                storage: StorageClass::Static,
            } if file_scope => EntityLinkage::Internal,
            SymbolData::Function { .. } => EntityLinkage::External,
            SymbolData::Object {
                storage: StorageClass::Static,
            } if file_scope => EntityLinkage::Internal,
            SymbolData::Object {
                storage: StorageClass::Extern,
            } => EntityLinkage::External,
            SymbolData::Object { .. } if file_scope => EntityLinkage::External,
            SymbolData::Object { .. }
            | SymbolData::Parameter { .. }
            | SymbolData::Typedef
            | SymbolData::Enumerator { .. } => EntityLinkage::None,
        }
    }

    fn merge_redeclaration(&mut self, symbol: SymbolId, data: SymbolData, ty: TypeId) -> bool {
        let (existing_kind, existing_ty) = {
            let existing = self
                .symbols
                .symbol(symbol)
                .expect("scope symbol IDs belong to the same table");
            (existing.kind(), existing.ty())
        };
        if existing_kind != data.kind() {
            return false;
        }

        if existing_kind == SymbolKind::Typedef {
            return existing_ty == ty;
        }
        if !matches!(existing_kind, SymbolKind::Object | SymbolKind::Function) {
            return false;
        }

        let Some(mut state) = self.entities.get(&symbol).copied() else {
            return false;
        };
        let new_thread_local = data.storage_class() == Some(StorageClass::ThreadLocal);
        if state.thread_local != new_thread_local {
            return false;
        }

        let new_linkage = self.declaration_linkage(data);
        let storage = data.storage_class().unwrap_or(StorageClass::None);
        state.linkage = match (state.linkage, new_linkage) {
            (EntityLinkage::Internal, EntityLinkage::External)
                if storage == StorageClass::Extern
                    || (existing_kind == SymbolKind::Function && storage == StorageClass::None) =>
            {
                EntityLinkage::Internal
            }
            (old, new) if old == new && old != EntityLinkage::None => old,
            _ => return false,
        };

        let Some(composite) = self.composite_type(existing_ty, ty, false) else {
            return false;
        };
        self.symbols
            .update_symbol_type(symbol, composite)
            .expect("scope symbol IDs belong to the same table");
        self.set_entity_state(symbol, state);
        true
    }

    fn mark_definition(&mut self, symbol: SymbolId, name: &SpannedName) {
        let Some(mut state) = self.entities.get(&symbol).copied() else {
            return;
        };
        if state.has_definition {
            self.report_redefinition(name);
        } else {
            state.has_definition = true;
            self.set_entity_state(symbol, state);
        }
    }

    fn composite_type(
        &mut self,
        left: TypeId,
        right: TypeId,
        ignore_top_qualifiers: bool,
    ) -> Option<TypeId> {
        if left == right {
            return Some(right);
        }
        let left_type = self.types.get(left)?.clone();
        let right_type = self.types.get(right)?.clone();
        if !ignore_top_qualifiers
            && left_type.qualifiers.compatibility() != right_type.qualifiers.compatibility()
        {
            return None;
        }
        let qualifiers = if ignore_top_qualifiers {
            Qualifiers::default()
        } else {
            right_type.qualifiers
        };

        let kind = match (left_type.kind, right_type.kind) {
            (TypeKind::Void, TypeKind::Void) => TypeKind::Void,
            (TypeKind::BuiltinVaList, TypeKind::BuiltinVaList) => TypeKind::BuiltinVaList,
            (TypeKind::GnuInt128(left), TypeKind::GnuInt128(right)) if left == right => {
                TypeKind::GnuInt128(right)
            }
            (TypeKind::Int(left), TypeKind::Int(right)) if left == right => TypeKind::Int(right),
            (TypeKind::Float(left), TypeKind::Float(right)) if left == right => {
                TypeKind::Float(right)
            }
            (TypeKind::Complex(left), TypeKind::Complex(right)) if left == right => {
                TypeKind::Complex(right)
            }
            (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
                TypeKind::Pointer(self.composite_type(left, right, false)?)
            }
            (
                TypeKind::Array {
                    element: left_element,
                    bound: left_bound,
                },
                TypeKind::Array {
                    element: right_element,
                    bound: right_bound,
                },
            ) => {
                let element = self.composite_type(left_element, right_element, false)?;
                let bound = match (left_bound, right_bound) {
                    (ArrayBound::Unbounded, bound) | (bound, ArrayBound::Unbounded) => bound,
                    (left, right) if left == right => right,
                    _ => return None,
                };
                TypeKind::Array { element, bound }
            }
            (
                TypeKind::Function {
                    return_type: left_return,
                    parameters: left_parameters,
                },
                TypeKind::Function {
                    return_type: right_return,
                    parameters: right_parameters,
                },
            ) => {
                let return_type = self.composite_type(left_return, right_return, false)?;
                let parameters =
                    self.composite_function_parameters(left_parameters, right_parameters)?;
                TypeKind::Function {
                    return_type,
                    parameters,
                }
            }
            (TypeKind::Struct(left), TypeKind::Struct(right)) if left == right => {
                TypeKind::Struct(right)
            }
            (TypeKind::Union(left), TypeKind::Union(right)) if left == right => {
                TypeKind::Union(right)
            }
            (TypeKind::Enum(left), TypeKind::Enum(right)) if left == right => TypeKind::Enum(right),
            _ => return None,
        };
        Some(self.intern(kind, qualifiers))
    }

    fn composite_function_parameters(
        &mut self,
        left: FunctionParameters,
        right: FunctionParameters,
    ) -> Option<FunctionParameters> {
        match (left, right) {
            (FunctionParameters::Unspecified, FunctionParameters::Unspecified) => {
                Some(FunctionParameters::Unspecified)
            }
            (FunctionParameters::Unspecified, prototype @ FunctionParameters::Prototype { .. })
            | (prototype @ FunctionParameters::Prototype { .. }, FunctionParameters::Unspecified) => {
                self.prototype_survives_default_promotions(&prototype)
                    .then_some(prototype)
            }
            (
                FunctionParameters::Prototype {
                    types: left,
                    variadic: left_variadic,
                },
                FunctionParameters::Prototype {
                    types: right,
                    variadic: right_variadic,
                },
            ) => {
                if left_variadic != right_variadic || left.len() != right.len() {
                    return None;
                }
                let types = left
                    .into_iter()
                    .zip(right)
                    .map(|(left, right)| self.composite_type(left, right, true))
                    .collect::<Option<Vec<_>>>()?;
                Some(FunctionParameters::Prototype {
                    types,
                    variadic: right_variadic,
                })
            }
        }
    }

    fn prototype_survives_default_promotions(&self, parameters: &FunctionParameters) -> bool {
        let FunctionParameters::Prototype { types, variadic } = parameters else {
            return true;
        };
        !variadic
            && types.iter().all(|ty| {
                self.types.get(*ty).is_some_and(|ty| match ty.kind {
                    TypeKind::Int(rank) => matches!(
                        rank,
                        IntRank::Int
                            | IntRank::UInt
                            | IntRank::Long
                            | IntRank::ULong
                            | IntRank::LongLong
                            | IntRank::ULongLong
                    ),
                    TypeKind::Float(FloatKind::F16 | FloatKind::F32) => false,
                    TypeKind::Complex(ComplexBase::CF16 | ComplexBase::CF32) => false,
                    TypeKind::Enum(_) => false,
                    _ => true,
                })
            })
    }

    fn create_scope(&mut self, kind: ScopeKind, span: Option<SourceSpan>) -> ScopeId {
        self.symbols
            .create_scope(self.current_scope, kind, span)
            .expect("the parser current scope is always live")
    }

    fn tag_in_current_scope(&self, spelling: &str) -> Option<TagId> {
        self.symbols
            .scope_tags(self.current_scope)
            .expect("the parser current scope is always live")
            .iter()
            .rev()
            .copied()
            .find(|tag| {
                self.symbols
                    .tags_in_source_order()
                    .iter()
                    .rev()
                    .any(|binding| {
                        binding.scope() == self.current_scope
                            && binding.tag() == *tag
                            && binding.spelling() == spelling
                    })
            })
    }

    fn facts_for_symbol(&self, symbol: SymbolId) -> ExprFacts {
        let symbol = self
            .symbols
            .symbol(symbol)
            .expect("resolved symbol belongs to the same table");
        match symbol.kind() {
            SymbolKind::Function => ExprFacts::function_designator(symbol.ty()),
            SymbolKind::Enumerator => ExprFacts::value(symbol.ty()),
            SymbolKind::Object | SymbolKind::Parameter => {
                let canonical = self
                    .types
                    .get(symbol.ty())
                    .expect("symbol types are interned before binding");
                let modifiable = !canonical.qualifiers.is_const
                    && !matches!(
                        canonical.kind,
                        TypeKind::Array { .. } | TypeKind::Function { .. }
                    );
                let addressable = symbol.storage_class() != Some(StorageClass::Register);
                ExprFacts::lvalue(symbol.ty(), modifiable, addressable)
                    .expect("ordinary object facts are internally consistent")
            }
            SymbolKind::Typedef => {
                unreachable!("typedef names are rejected before expression facts are built")
            }
        }
    }

    // ── external declarations ──────────────────────────────────────────

    fn parse_external_declaration(&mut self) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        if self.match_kind(TokenKind::Semicolon) {
            return Some(Vec::new());
        }
        if self.peek_kind() == Some(TokenKind::StaticAssert) {
            self.parse_static_assert_declaration()?;
            return Some(Vec::new());
        }
        if self.peek_kind() == Some(TokenKind::Asm) {
            let mut assembly = self.parse_asm_construct()?;
            let semicolon = self.expect(TokenKind::Semicolon)?;
            assembly.span = self.merge_span(assembly.span, semicolon.span);
            return Some(vec![self.alloc_node(CHirDecl::Asm(assembly))]);
        }
        self.parse_declaration(DeclarationContext::File)
    }

    fn parse_declaration(
        &mut self,
        context: DeclarationContext,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let scope_checkpoint = self.current_scope;
        let types_transaction = self.types.begin_transaction();
        let symbols_transaction = self.symbols.begin_transaction();
        let declaration_transaction = self.begin_declaration_transaction();
        let generic_selection_checkpoint = self.generic_selections.len();
        let front_end_builtin_checkpoint = self.front_end_builtins.len();
        let rollback_error_checkpoint = self.rollback_error_count;
        let side_start = declaration_transaction.side_declarations;

        let result = self.parse_declaration_inner(context, side_start);
        if result.is_none() || self.rollback_error_count != rollback_error_checkpoint {
            self.types
                .rollback_transaction(&types_transaction)
                .expect("parser type transactions are table-local and LIFO");
            self.symbols
                .rollback_transaction(&symbols_transaction)
                .expect("parser declaration transactions are table-local and LIFO");
            self.current_scope = scope_checkpoint;
            self.rollback_declaration_transaction(&declaration_transaction);
            self.generic_selections
                .truncate(generic_selection_checkpoint);
            self.front_end_builtins
                .truncate(front_end_builtin_checkpoint);
            None
        } else {
            self.types
                .commit_transaction(&types_transaction)
                .expect("parser type transactions are table-local and LIFO");
            self.symbols
                .commit_transaction(&symbols_transaction)
                .expect("parser declaration transactions are table-local and LIFO");
            self.commit_declaration_transaction(&declaration_transaction);
            result
        }
    }

    fn parse_declaration_inner(
        &mut self,
        context: DeclarationContext,
        side_start: usize,
    ) -> Option<Vec<&'arena CHirDecl<'arena>>> {
        let start_span = self.span();
        let mut gnu_extension_spans = Vec::new();
        while self.peek_kind() == Some(TokenKind::GnuExtension) {
            gnu_extension_spans.push(
                self.advance()
                    .expect("peeked GNU extension declaration prefix")
                    .span,
            );
        }
        let mut storage = None;
        let mut inline_spans = Vec::new();
        let mut attributes = Vec::new();
        self.parse_declaration_modifiers(&mut storage, &mut inline_spans, &mut attributes);
        let Some(specifier) = self.parse_declaration_type_specifier(&mut inline_spans) else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        self.parse_declaration_modifiers(&mut storage, &mut inline_spans, &mut attributes);
        let storage = storage.unwrap_or(StorageClass::None);
        let auto_type_span = match &specifier.surface {
            TypeSpecifier::AutoType { span, .. } => Some(*span),
            _ => None,
        };

        if self.peek_kind() == Some(TokenKind::Semicolon) {
            let semi = self.advance().expect("peeked semicolon");
            if auto_type_span.is_some() {
                self.report("E2915", "auto-type-misuse", semi.span);
                return None;
            }
            let declaration_span = self.merge_span(start_span, semi.span);
            if let Some(span) = inline_spans.first().copied() {
                self.report("E1401", "inline-specifier-on-non-function", span);
                return None;
            }
            let mut declarations = self.side_declarations.split_off(side_start);
            if let Some(event) = specifier.tag_event {
                declarations.push(self.finish_tag_event(
                    event,
                    declaration_span,
                    gnu_extension_spans,
                ));
            } else {
                self.report("E1001", "expected-declaration", semi.span);
            }
            return Some(declarations);
        }

        let Some(declarator) = self.parse_declarator(true) else {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        };
        if auto_type_span.is_some()
            && (storage == StorageClass::Typedef || !declarator.derived.is_empty())
        {
            self.report("E2915", "auto-type-misuse", declarator.span);
            return None;
        }
        let (asm_label, asm_label_span) = self.parse_optional_asm_label()?;
        // Attributes after the declarator attach to it alone. When a function
        // body follows they are in function-definition position; the same
        // tokens, a different row of the placement table.
        let trailing_placement = if self
            .tokens
            .iter()
            .skip(self.pos)
            .find(|token| token.kind != TokenKind::Attribute)
            .is_some_and(|_| self.declarator_is_followed_by_body())
        {
            AttributePlacement::FunctionDefinition
        } else {
            AttributePlacement::AfterDeclarator
        };
        attributes.extend(self.parse_attribute_specifiers(trailing_placement));
        let mut full_type = self.apply_declarator(specifier.surface.type_id(), &declarator.derived);
        full_type = self.apply_attributes_to_type(full_type, &mut attributes);
        let written = self.written_declarator(&declarator);

        if (context != DeclarationContext::File && storage == StorageClass::ThreadLocal)
            || (context == DeclarationContext::File && storage == StorageClass::Auto)
        {
            self.report_redefinition(&declarator.name);
        }

        if storage != StorageClass::Typedef
            && matches!(
                self.types.get(full_type).map(|ty| &ty.kind),
                Some(TypeKind::Function { .. })
            )
        {
            let written_parameters = declarator.derived.iter().find_map(|layer| match layer {
                DerivedDeclarator::Function {
                    parameters, scope, ..
                } => Some((parameters.clone(), *scope)),
                _ => None,
            });
            let inherited_parameters = specifier.inherited_function_parameters.clone();
            let inherited_from_specifier = written_parameters.is_none();
            let (mut parameters, parameter_scope) = if let Some(written) = written_parameters {
                written
            } else if let Some(mut inherited) = inherited_parameters {
                if let ParameterSyntax::Prototype { parameters, .. } = &mut inherited {
                    for parameter in parameters {
                        parameter.symbol = None;
                        parameter.name = None;
                    }
                }
                let scope =
                    self.create_scope(ScopeKind::FunctionPrototype, Some(declarator.name.span));
                (inherited, scope)
            } else {
                self.report("E1001", "expected-declaration", declarator.name.span);
                return None;
            };
            let symbol = self.declare_symbol(
                &declarator.name,
                SymbolData::Function { storage },
                full_type,
            );
            let has_body = self.peek_kind() == Some(TokenKind::LBrace);
            if has_body && inherited_from_specifier {
                self.report("E1001", "expected-declaration", self.span());
                return None;
            }
            if has_body {
                self.mark_definition(symbol, &declarator.name);
            }
            let body = if has_body {
                let open = self.span();
                let outer_scope = self.current_scope;
                self.current_scope = parameter_scope;
                self.bind_parameters(&mut parameters);
                let body = self.parse_function_body();
                self.validate_asm_goto_labels(body)?;
                self.validate_statement_expression_jumps(body)?;
                self.current_scope = outer_scope;
                if context != DeclarationContext::File {
                    self.report("E1205", "nested-function-definition", open);
                }
                Some(body)
            } else {
                let outer_scope = self.current_scope;
                self.current_scope = parameter_scope;
                self.bind_parameters(&mut parameters);
                self.current_scope = outer_scope;
                None
            };
            self.set_inherited_function_parameters(symbol, parameters.clone());
            let end_span = if let Some(body) = body {
                body.span()
            } else {
                self.declaration_end(declarator.name.span)
            };
            let declaration_span = self.merge_span(start_span, end_span);
            if context == DeclarationContext::ForInitializer {
                self.report(
                    "E1206",
                    "function-declaration-in-for-initializer",
                    declarator.name.span,
                );
                return None;
            }
            if storage == StorageClass::Typedef {
                if let Some(span) = inline_spans.first().copied() {
                    self.report("E1401", "inline-specifier-on-non-function", span);
                    return None;
                }
            }
            let mut declarations = self.side_declarations.split_off(side_start);
            if let Some(event) = specifier.tag_event {
                declarations.push(self.finish_tag_event(
                    event,
                    declaration_span,
                    gnu_extension_spans.clone(),
                ));
            }
            declarations.push(self.alloc_node(CHirDecl::Function(FunctionDecl {
                symbol,
                name: declarator.name,
                ty: full_type,
                return_specifier: specifier.surface,
                written,
                parameters,
                storage,
                asm_label,
                asm_label_span,
                inline_spans,
                gnu_extension_spans,
                attributes,
                body,
                span: declaration_span,
            })));
            return Some(declarations);
        }

        let symbol_data = if storage == StorageClass::Typedef {
            SymbolData::Typedef
        } else {
            SymbolData::Object { storage }
        };

        // C11 6.7: one declaration may carry several init-declarators. Each
        // gets its own symbol and initializer in sequence, so a later
        // initializer can refer to an earlier name, and every one receives the
        // attributes written among the specifiers. Attributes written after a
        // particular declarator belong to it alone.
        struct Pending<'arena> {
            declarator: ParsedDeclarator,
            full_type: TypeId,
            symbol: SymbolId,
            initializer: Option<ObjectInitializer<'arena>>,
            specifier: TypeSpecifier,
            written: WrittenDeclarator,
            attributes: Vec<Attribute>,
            asm_label: Option<String>,
            asm_label_span: Option<SourceSpan>,
        }
        let specifier_attributes: Vec<Attribute> = attributes
            .iter()
            .filter(|attribute| attribute.placement == AttributePlacement::DeclarationSpecifiers)
            .cloned()
            .collect();
        let mut pending: Vec<Pending<'arena>> = Vec::new();
        let mut declarator = declarator;
        let mut written = written;
        let mut asm_label = asm_label;
        let mut asm_label_span = asm_label_span;
        loop {
            // An ordinary identifier's scope begins immediately after its
            // declarator, so self-references in an initializer resolve to this
            // declaration rather than an outer shadowed name.
            let has_initializer = self.peek_kind() == Some(TokenKind::Assign);
            if !has_initializer {
                self.validate_variable_array_bounds(&declarator.derived);
            }
            if auto_type_span.is_some() && !has_initializer {
                self.report("E2915", "auto-type-misuse", declarator.span);
                return None;
            }
            let inherited_function_syntax = if storage == StorageClass::Typedef
                && matches!(
                    self.types.get(full_type).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                ) {
                declarator
                    .derived
                    .iter()
                    .find_map(|layer| match layer {
                        DerivedDeclarator::Function { parameters, .. } => Some(parameters.clone()),
                        _ => None,
                    })
                    .or_else(|| specifier.inherited_function_parameters.clone())
            } else {
                None
            };
            let symbol = self.declare_symbol(&declarator.name, symbol_data, full_type);
            full_type = self
                .symbols
                .symbol(symbol)
                .expect("declared symbols belong to the parser symbol table")
                .ty();
            if let Some(parameters) = inherited_function_syntax {
                self.set_inherited_function_parameters(symbol, parameters);
            }
            if has_initializer && storage != StorageClass::Typedef {
                self.mark_definition(symbol, &declarator.name);
            }
            let mut declaration_specifier = specifier.surface.clone();
            let initializer = if self.match_kind(TokenKind::Assign) {
                let written_initializer = self.parse_written_initializer()?;
                if auto_type_span.is_some()
                    && written_initializer_references_symbol(&written_initializer, symbol)
                {
                    self.report("E2915", "auto-type-misuse", declarator.span);
                    return None;
                }
                if storage == StorageClass::Typedef
                    || (storage == StorageClass::Extern && context != DeclarationContext::File)
                {
                    self.report(
                        "E2202",
                        "invalid-initializer-type",
                        written_initializer.span(),
                    );
                    return None;
                }
                if let Some(span) = auto_type_span {
                    let base = self.infer_auto_type_base(
                        &written_initializer,
                        &declarator.derived,
                        declarator.span,
                    )?;
                    full_type = self.apply_declarator(base, &declarator.derived);
                    declaration_specifier = TypeSpecifier::AutoType {
                        resolved: base,
                        span,
                    };
                } else {
                    full_type =
                        self.infer_initializer_target_type(full_type, &written_initializer)?;
                }
                self.symbols
                    .update_symbol_type(symbol, full_type)
                    .expect("declared symbols belong to the parser symbol table");
                let plan = self.normalize_initializer_plan(
                    full_type,
                    &written_initializer,
                    context == DeclarationContext::File,
                )?;
                if self.initializer_has_static_storage(context, storage) {
                    self.validate_static_initializer_plan(&plan)?;
                }
                Some(ObjectInitializer::new(written_initializer, plan))
            } else {
                None
            };
            let current_declarator_span = declarator.span;
            pending.push(Pending {
                declarator,
                full_type,
                symbol,
                initializer,
                specifier: declaration_specifier,
                written,
                attributes,
                asm_label,
                asm_label_span,
            });
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
            if auto_type_span.is_some() {
                self.report("E2915", "auto-type-misuse", current_declarator_span);
                return None;
            }
            let mut next_attributes = specifier_attributes.clone();
            next_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::BeforeDeclarator));
            let Some(next) = self.parse_declarator(false) else {
                self.report("E1001", "expected-declaration", self.span());
                return None;
            };
            let (next_asm_label, next_asm_label_span) = self.parse_optional_asm_label()?;
            next_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::AfterDeclarator));
            let mut next_type = self.apply_declarator(specifier.surface.type_id(), &next.derived);
            next_type = self.apply_attributes_to_type(next_type, &mut next_attributes);
            if storage != StorageClass::Typedef
                && matches!(
                    self.types.get(next_type).map(|ty| &ty.kind),
                    Some(TypeKind::Function { .. })
                )
            {
                // A function declarator inside an init-declarator list is legal
                // C but has no consumer in this profile; refuse it plainly
                // rather than mis-declaring it as an object.
                self.report("E1002", "expected-token: ;", next.name.span);
                return None;
            }
            written = self.written_declarator(&next);
            declarator = next;
            full_type = next_type;
            attributes = next_attributes;
            asm_label = next_asm_label;
            asm_label_span = next_asm_label_span;
        }

        let last_name_span = pending
            .last()
            .map(|entry| entry.declarator.name.span)
            .expect("at least one declarator was parsed");
        let end_span = self.declaration_end(last_name_span);
        let declaration_span = self.merge_span(start_span, end_span);
        if let Some(span) = inline_spans.first().copied() {
            self.report("E1401", "inline-specifier-on-non-function", span);
            return None;
        }
        let mut declarations = self.side_declarations.split_off(side_start);
        if let Some(event) = specifier.tag_event {
            declarations.push(self.finish_tag_event(
                event,
                declaration_span,
                gnu_extension_spans.clone(),
            ));
        }
        for entry in pending {
            let declaration = if storage == StorageClass::Typedef {
                CHirDecl::Typedef(TypedefDecl {
                    symbol: entry.symbol,
                    name: entry.declarator.name,
                    ty: entry.full_type,
                    specifier: entry.specifier,
                    written: entry.written,
                    gnu_extension_spans: gnu_extension_spans.clone(),
                    attributes: entry.attributes,
                    span: declaration_span,
                })
            } else {
                CHirDecl::Object(ObjectDecl {
                    symbol: entry.symbol,
                    name: entry.declarator.name,
                    ty: entry.full_type,
                    specifier: entry.specifier,
                    written: entry.written,
                    storage,
                    initializer: entry.initializer,
                    asm_label: entry.asm_label,
                    asm_label_span: entry.asm_label_span,
                    gnu_extension_spans: gnu_extension_spans.clone(),
                    attributes: entry.attributes,
                    span: declaration_span,
                })
            };
            declarations.push(self.alloc_node(declaration));
        }
        Some(declarations)
    }

    fn parse_written_initializer(&mut self) -> Option<WrittenInitializer<'arena>> {
        if self.peek_kind() != Some(TokenKind::LBrace) {
            return self.parse_assignment().map(WrittenInitializer::Expression);
        }

        let open = self.advance().expect("peeked initializer brace");
        let mut elements = Vec::new();
        let mut designations = Vec::new();
        if self.peek_kind() != Some(TokenKind::RBrace) {
            loop {
                let designation =
                    if matches!(self.peek_kind(), Some(TokenKind::Dot | TokenKind::LBracket)) {
                        Some(self.parse_initializer_designation()?)
                    } else {
                        None
                    };
                elements.push(self.parse_written_initializer()?);
                designations.push(designation);
                if !self.match_kind(TokenKind::Comma) {
                    break;
                }
                if self.peek_kind() == Some(TokenKind::RBrace) {
                    break;
                }
            }
        }
        let close = self.expect(TokenKind::RBrace)?;
        Some(WrittenInitializer::List {
            elements,
            designations,
            span: self.merge_span(open.span, close.span),
        })
    }

    fn parse_initializer_designation(&mut self) -> Option<InitializerDesignation<'arena>> {
        let start = self.span();
        let mut designators = Vec::new();
        loop {
            match self.peek_kind() {
                Some(TokenKind::Dot) => {
                    self.advance();
                    let token = self.expect(TokenKind::Identifier)?;
                    designators.push(WrittenDesignator::Field(SpannedName {
                        spelling: self.token_text(&token).to_owned(),
                        span: token.span,
                    }));
                }
                Some(TokenKind::LBracket) => {
                    let open = self.advance().expect("peeked designator bracket");
                    let first = self.parse_conditional()?;
                    if self.match_kind(TokenKind::Ellipsis) {
                        let last = self.parse_conditional()?;
                        let close = self.expect(TokenKind::RBracket)?;
                        designators.push(WrittenDesignator::Range {
                            first,
                            last,
                            span: self.merge_span(open.span, close.span),
                        });
                    } else {
                        let close = self.expect(TokenKind::RBracket)?;
                        designators.push(WrittenDesignator::Index {
                            expression: first,
                            span: self.merge_span(open.span, close.span),
                        });
                    }
                }
                _ => break,
            }
        }
        let assign = self.expect(TokenKind::Assign)?;
        Some(InitializerDesignation {
            designators,
            span: self.merge_span(start, assign.span),
        })
    }

    fn infer_initializer_target_type(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Option<TypeId> {
        if self.initialized_array_contains_vla(target) {
            self.report("E2203", "variable-length-object", written.span());
            return None;
        }
        let canonical = self.types.get(target)?.clone();
        let TypeKind::Array { element, bound } = canonical.kind else {
            return Some(target);
        };
        match bound {
            ArrayBound::Fixed(_) => Some(target),
            ArrayBound::Variable => unreachable!("VLA targets are rejected above"),
            ArrayBound::Unbounded => {
                let count = match written {
                    WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)) => {
                        if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
                            self.report(
                                "E2205",
                                "not-covered-day5: wide-string-initializer",
                                *span,
                            );
                            return None;
                        }
                        if !self.is_narrow_character_type(element) {
                            self.report("E2202", "invalid-initializer-type", *span);
                            return None;
                        }
                        literal.explicit_unit_count()?.checked_add(1)?
                    }
                    WrittenInitializer::Expression(expression) => {
                        self.report("E2202", "invalid-initializer-type", expression.span());
                        return None;
                    }
                    WrittenInitializer::List { elements, span, .. } => {
                        if let Some(WrittenInitializer::Expression(CHirExpr::StringLit(
                            literal,
                            literal_span,
                        ))) = elements.first()
                        {
                            if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
                                self.report(
                                    "E2205",
                                    "not-covered-day5: wide-string-initializer",
                                    *literal_span,
                                );
                                return None;
                            }
                            if self.is_narrow_character_type(element) {
                                if elements.len() > 1 {
                                    self.report("E2201", "excess-initializer", elements[1].span());
                                    return None;
                                }
                                literal.explicit_unit_count()?.checked_add(1)?
                            } else {
                                self.inferred_array_element_count(element, elements, *span)?
                            }
                        } else {
                            self.inferred_array_element_count(element, elements, *span)?
                        }
                    }
                };
                Some(self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(count),
                    },
                    canonical.qualifiers,
                ))
            }
        }
    }

    /// Resolve the base type denoted by `__auto_type` from its required
    /// initializer. The converted initializer determines the declared object
    /// type; written pointer layers are peeled to recover the declaration
    /// specifier's base and then reapplied by the ordinary declarator path.
    fn infer_auto_type_base(
        &mut self,
        written: &WrittenInitializer<'arena>,
        derived: &[DerivedDeclarator],
        declarator_span: SourceSpan,
    ) -> Option<TypeId> {
        let WrittenInitializer::Expression(expression) = written else {
            self.report("E2915", "auto-type-misuse", declarator_span);
            return None;
        };
        let mut inferred = self.generic_controlling_type(expression).or_else(|| {
            self.report("E2202", "invalid-initializer-type", expression.span());
            None
        })?;
        for layer in derived {
            let DerivedDeclarator::Pointer { .. } = layer else {
                // Arrays and functions are rejected at the declarator before
                // the initializer is parsed; keep this helper total if a
                // future caller forgets that boundary.
                self.report("E2915", "auto-type-misuse", declarator_span);
                return None;
            };
            let Some(TypeKind::Pointer(pointee)) = self
                .types
                .get(inferred)
                .map(|canonical| canonical.kind.clone())
            else {
                self.report("E2202", "invalid-initializer-type", expression.span());
                return None;
            };
            inferred = pointee;
        }
        Some(inferred)
    }

    fn initialized_array_contains_vla(&self, target: TypeId) -> bool {
        match self.types.get(target).map(|canonical| &canonical.kind) {
            Some(TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            }) => true,
            Some(TypeKind::Array { element, .. }) => self.initialized_array_contains_vla(*element),
            _ => false,
        }
    }

    fn initializer_has_static_storage(
        &self,
        context: DeclarationContext,
        storage: StorageClass,
    ) -> bool {
        context == DeclarationContext::File
            || matches!(storage, StorageClass::Static | StorageClass::ThreadLocal)
    }

    fn validate_static_initializer_plan(&mut self, plan: &InitPlan<'arena>) -> Option<()> {
        for action in plan.actions() {
            let InitAction::StoreScalar { value, .. } = action else {
                continue;
            };
            if !self.static_initializer_expression_is_constant(value) {
                self.report("E2208", "non-constant-static-initializer", value.span());
                return None;
            }
        }
        Some(())
    }

    fn static_initializer_expression_is_constant(&self, expression: &CHirExpr<'_>) -> bool {
        self.is_arithmetic_constant_expression(expression)
            || self.is_static_address_constant(expression)
    }

    /// Reading storage is never an arithmetic constant expression: C11 6.6
    /// admits only operands whose value is fixed at translation time.
    fn is_arithmetic_constant_expression(&self, expression: &CHirExpr<'_>) -> bool {
        if self.integer_expression_type(expression).is_ok() {
            return self.eval_integer_constant_expression(expression).is_ok();
        }
        match expression {
            CHirExpr::Lit(_, _) | CHirExpr::FloatLit(_, _) | CHirExpr::CharLit(_, _) => true,
            CHirExpr::BitBuiltin { .. } => {
                self.eval_integer_constant_expression(expression).is_ok()
            }
            CHirExpr::MemoryRead { .. } => false,
            CHirExpr::Ident { symbol, .. } => self
                .symbols
                .symbol(*symbol)
                .is_ok_and(|symbol| symbol.kind() == SymbolKind::Enumerator),
            CHirExpr::Unary(
                UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Not | UnaryOp::BitNot,
                operand,
                _,
            ) => self.is_arithmetic_constant_expression(operand),
            CHirExpr::EvaluateOnce(operand, _) => self.is_arithmetic_constant_expression(operand),
            CHirExpr::Binary(op, left, right, _) => {
                if *op == BinaryOp::And
                    && self
                        .constant_arithmetic_truth(left)
                        .is_some_and(|truth| !truth)
                {
                    return true;
                }
                if *op == BinaryOp::Or
                    && self
                        .constant_arithmetic_truth(left)
                        .is_some_and(|truth| truth)
                {
                    return true;
                }
                self.is_arithmetic_constant_expression(left)
                    && self.is_arithmetic_constant_expression(right)
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let Some(condition) = self.constant_arithmetic_truth(condition) else {
                    return false;
                };
                self.is_arithmetic_constant_expression(if condition {
                    when_true
                } else {
                    when_false
                })
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let Some(truth) = self.constant_arithmetic_truth(operand) else {
                    return false;
                };
                truth || self.is_arithmetic_constant_expression(fallback)
            }
            CHirExpr::Cast(target, operand, _) => {
                matches!(
                    self.operand_type_class(InferredType::Canonical(*target)),
                    OperandTypeClass::Integer(_)
                        | OperandTypeClass::Real(_)
                        | OperandTypeClass::Complex(_)
                ) && self.is_arithmetic_constant_expression(operand)
            }
            CHirExpr::SizeOf(ty, _) => TargetDataLayout::aarch64_unknown_linux_gnu()
                .sizeof_type(&self.types, *ty)
                .is_ok(),
            CHirExpr::AlignOf(ty, _) => TargetDataLayout::aarch64_unknown_linux_gnu()
                .layout_of(&self.types, *ty)
                .is_ok(),
            CHirExpr::SizeOfExpr(_, ty, _) => TargetDataLayout::aarch64_unknown_linux_gnu()
                .sizeof_type(&self.types, *ty)
                .is_ok(),
            CHirExpr::Unary(UnaryOp::AddrOf | UnaryOp::Deref, _, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _) => false,
        }
    }

    fn constant_arithmetic_truth(&self, expression: &CHirExpr<'_>) -> Option<bool> {
        if let Ok(value) = self.eval_integer_constant_expression(expression) {
            return Some(value.truth());
        }
        self.eval_real_constant_expression(expression)
            .map(|value| value != 0.0)
    }

    fn eval_real_constant_expression(&self, expression: &CHirExpr<'_>) -> Option<f64> {
        match expression {
            CHirExpr::FloatLit(literal, _) => Some(literal.value),
            CHirExpr::MemoryRead { .. } => None,
            CHirExpr::Lit(_, _)
            | CHirExpr::CharLit(_, _)
            | CHirExpr::Ident { .. }
            | CHirExpr::BitBuiltin { .. } => self
                .eval_integer_constant_expression(expression)
                .ok()
                .map(|value| value.mathematical_value() as f64),
            CHirExpr::Unary(op, operand, _) => {
                let operand = self.eval_real_constant_expression(operand)?;
                match op {
                    UnaryOp::Plus => Some(operand),
                    UnaryOp::Neg => Some(-operand),
                    UnaryOp::Not => Some(f64::from(operand == 0.0)),
                    UnaryOp::BitNot | UnaryOp::AddrOf | UnaryOp::Deref => None,
                }
            }
            CHirExpr::EvaluateOnce(operand, _) => self.eval_real_constant_expression(operand),
            CHirExpr::Binary(op, left, right, _) => {
                let left = self.eval_real_constant_expression(left)?;
                if *op == BinaryOp::And && left == 0.0 {
                    return Some(0.0);
                }
                if *op == BinaryOp::Or && left != 0.0 {
                    return Some(1.0);
                }
                let right = self.eval_real_constant_expression(right)?;
                match op {
                    BinaryOp::Add => Some(left + right),
                    BinaryOp::Sub => Some(left - right),
                    BinaryOp::Mul => Some(left * right),
                    BinaryOp::Div => Some(left / right),
                    BinaryOp::Lt => Some(f64::from(left < right)),
                    BinaryOp::Le => Some(f64::from(left <= right)),
                    BinaryOp::Gt => Some(f64::from(left > right)),
                    BinaryOp::Ge => Some(f64::from(left >= right)),
                    BinaryOp::Eq => Some(f64::from(left == right)),
                    BinaryOp::Ne => Some(f64::from(left != right)),
                    BinaryOp::And => Some(f64::from(left != 0.0 && right != 0.0)),
                    BinaryOp::Or => Some(f64::from(left != 0.0 || right != 0.0)),
                    BinaryOp::Mod
                    | BinaryOp::Shl
                    | BinaryOp::Shr
                    | BinaryOp::BitAnd
                    | BinaryOp::BitOr
                    | BinaryOp::BitXor
                    | BinaryOp::Comma
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
                    | BinaryOp::BitXorAssign => None,
                }
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let selected = if self.constant_arithmetic_truth(condition)? {
                    when_true
                } else {
                    when_false
                };
                self.eval_real_constant_expression(selected)
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let operand_value = self.eval_real_constant_expression(operand)?;
                if operand_value != 0.0 {
                    Some(operand_value)
                } else {
                    self.eval_real_constant_expression(fallback)
                }
            }
            CHirExpr::Cast(target, operand, _) => {
                match self.operand_type_class(InferredType::Canonical(*target)) {
                    OperandTypeClass::Integer(_) => self
                        .eval_integer_constant_expression(expression)
                        .ok()
                        .map(|value| value.mathematical_value() as f64),
                    OperandTypeClass::Real(_) => self.eval_real_constant_expression(operand),
                    OperandTypeClass::Complex(_)
                    | OperandTypeClass::Pointer(_)
                    | OperandTypeClass::StringPointer(_)
                    | OperandTypeClass::Aggregate(_)
                    | OperandTypeClass::Void
                    | OperandTypeClass::Other => None,
                }
            }
            CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _, _) | CHirExpr::AlignOf(_, _) => {
                self.eval_integer_constant_expression(expression)
                    .ok()
                    .map(|value| value.mathematical_value() as f64)
            }
            CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::Assign { .. }
            | CHirExpr::MemoryAssign { .. }
            | CHirExpr::Update { .. }
            | CHirExpr::MemoryUpdate { .. }
            | CHirExpr::StatementExpr { .. }
            | CHirExpr::CompoundLit(_, _, _)
            | CHirExpr::Comma(_, _, _)
            | CHirExpr::Member(_, _, _)
            | CHirExpr::PtrMember(_, _, _)
            | CHirExpr::Index(_, _, _) => None,
        }
    }

    fn is_static_address_constant(&self, expression: &CHirExpr<'_>) -> bool {
        if self.is_null_pointer_constant(expression) {
            return true;
        }
        match expression {
            CHirExpr::StringLit(_, _) => true,
            CHirExpr::CompoundLit(_, _, _) => self.current_scope == self.symbols.file_scope(),
            CHirExpr::Ident { symbol, .. } => self.symbol_decays_to_static_address(*symbol),
            CHirExpr::Unary(UnaryOp::AddrOf, operand, _) => self.lvalue_has_static_address(operand),
            CHirExpr::Binary(BinaryOp::Add, left, right, _) => {
                (self.is_static_address_constant(left)
                    && self.eval_integer_constant_expression(right).is_ok())
                    || (self.is_static_address_constant(right)
                        && self.eval_integer_constant_expression(left).is_ok())
            }
            CHirExpr::Binary(BinaryOp::Sub, left, right, _) => {
                self.is_static_address_constant(left)
                    && self.eval_integer_constant_expression(right).is_ok()
            }
            CHirExpr::Cast(target, operand, _)
                if matches!(
                    self.types.get(*target).map(|ty| &ty.kind),
                    Some(TypeKind::Pointer(_))
                ) =>
            {
                self.is_static_address_constant(operand)
                    || self.eval_integer_constant_expression(operand).is_ok()
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => self
                .constant_arithmetic_truth(condition)
                .is_some_and(|condition| {
                    self.is_static_address_constant(if condition { when_true } else { when_false })
                }),
            _ => false,
        }
    }

    fn lvalue_has_static_address(&self, expression: &CHirExpr<'_>) -> bool {
        match expression {
            CHirExpr::Ident { symbol, .. } => self.symbol_has_static_address(*symbol),
            CHirExpr::Member(base, _, _) => self.lvalue_has_static_address(base),
            CHirExpr::Index(base, index, _) => {
                self.is_static_address_constant(base)
                    && self.eval_integer_constant_expression(index).is_ok()
            }
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => self.is_static_address_constant(pointer),
            CHirExpr::CompoundLit(_, _, _) => self.current_scope == self.symbols.file_scope(),
            _ => false,
        }
    }

    fn symbol_has_static_address(&self, symbol: SymbolId) -> bool {
        let Ok(symbol) = self.symbols.symbol(symbol) else {
            return false;
        };
        if symbol.storage_class() == Some(StorageClass::ThreadLocal) {
            return false;
        }
        symbol.kind() == SymbolKind::Function
            || (symbol.kind() == SymbolKind::Object
                && (symbol.scope() == self.symbols.file_scope()
                    || matches!(
                        symbol.storage_class(),
                        Some(StorageClass::Static | StorageClass::Extern)
                    )))
    }

    fn symbol_decays_to_static_address(&self, symbol: SymbolId) -> bool {
        let Ok(symbol_data) = self.symbols.symbol(symbol) else {
            return false;
        };
        self.symbol_has_static_address(symbol)
            && (symbol_data.kind() == SymbolKind::Function
                || matches!(
                    self.types.get(symbol_data.ty()).map(|ty| &ty.kind),
                    Some(TypeKind::Array { .. })
                ))
    }

    fn inferred_array_element_count(
        &mut self,
        element: TypeId,
        elements: &[WrittenInitializer<'arena>],
        span: SourceSpan,
    ) -> Option<u64> {
        let mut cursor = 0;
        let mut count = 0_u64;
        while cursor < elements.len() {
            let next = match self.initializer_stream_action_count(element, elements, cursor) {
                Ok((next, _)) => next,
                Err(InitializerPreflightError::TooLarge) => {
                    self.report("E2207", "initializer-plan-too-large", span);
                    return None;
                }
                Err(
                    InitializerPreflightError::RecursiveObject
                    | InitializerPreflightError::InvalidObject,
                ) => {
                    self.report("E2202", "invalid-initializer-type", span);
                    return None;
                }
            };
            cursor = if next > cursor { next } else { cursor + 1 };
            count = count.checked_add(1)?;
        }
        Some(count)
    }

    fn initializer_stream_consumption(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        cursor: usize,
    ) -> Option<usize> {
        let initializer = elements.get(cursor)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            return Some(cursor + 1);
        }

        let children = self.aggregate_initializer_children(target)?;
        if children.is_empty() {
            return Some(cursor + 1);
        }
        let mut next = cursor;
        for (_, child) in children {
            if next >= elements.len() {
                break;
            }
            next = self.initializer_stream_consumption(child, elements, next)?;
        }
        Some(next)
    }

    fn normalize_initializer_plan(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
        allow_flexible_member: bool,
    ) -> Option<InitPlan<'arena>> {
        let previous = self.flexible_initializer_allowed;
        self.flexible_initializer_allowed = allow_flexible_member;
        let result = self.normalize_initializer_plan_inner(target, written);
        self.flexible_initializer_allowed = previous;
        result
    }

    fn normalize_initializer_plan_inner(
        &mut self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Option<InitPlan<'arena>> {
        match self.validate_initializer_object_graph(target, &mut HashSet::new()) {
            Ok(()) => {}
            Err(InitializerPreflightError::TooLarge) => {
                unreachable!("graph validation does not count actions")
            }
            Err(
                InitializerPreflightError::RecursiveObject
                | InitializerPreflightError::InvalidObject,
            ) => {
                self.report("E2202", "invalid-initializer-type", written.span());
                return None;
            }
        }
        let action_count = if self.written_initializer_has_designations(written) {
            self.initializer_subobject_count(target, &mut HashSet::new())
        } else {
            self.initializer_action_count(target, written)
        };
        match action_count {
            Ok(_) => {}
            Err(InitializerPreflightError::TooLarge) => {
                self.report("E2207", "initializer-plan-too-large", written.span());
                return None;
            }
            Err(
                InitializerPreflightError::RecursiveObject
                | InitializerPreflightError::InvalidObject,
            ) => {
                self.report("E2202", "invalid-initializer-type", written.span());
                return None;
            }
        }
        let mut actions = Vec::new();
        self.normalize_initializer_value(target, SubobjectPath::root(), written, &mut actions)?;
        if actions.len() > MAX_INITIALIZER_SUBOBJECTS as usize {
            self.report("E2207", "initializer-plan-too-large", written.span());
            return None;
        }
        let flexible_array_elements = self.flexible_initializer_element_count(target, &actions);
        let plan = InitPlan::with_last_write_wins(target, actions, &self.types)
            .with_flexible_array_elements(flexible_array_elements);
        if plan.validate(&self.types).is_err() {
            self.report("E2202", "invalid-initializer-type", written.span());
            return None;
        }
        Some(plan)
    }

    fn written_initializer_has_designations(&self, written: &WrittenInitializer<'arena>) -> bool {
        match written {
            WrittenInitializer::Expression(_) => false,
            WrittenInitializer::List {
                elements,
                designations,
                ..
            } => {
                designations.iter().any(Option::is_some)
                    || elements
                        .iter()
                        .any(|element| self.written_initializer_has_designations(element))
            }
        }
    }

    fn validate_initializer_object_graph(
        &self,
        target: TypeId,
        visiting: &mut HashSet<TypeId>,
    ) -> Result<(), InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_) => Ok(()),
            TypeKind::Array { element, .. } => {
                self.validate_initializer_object_graph(element, visiting)
            }
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                if !visiting.insert(target) {
                    return Err(InitializerPreflightError::RecursiveObject);
                }
                let tag = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let fields = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields))
                    | TagState::Complete(TagDefinition::Union(fields)) => fields,
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        visiting.remove(&target);
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
                for field in fields {
                    self.validate_initializer_object_graph(field.ty, visiting)?;
                }
                visiting.remove(&target);
                Ok(())
            }
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }

    fn initializer_action_count(
        &self,
        target: TypeId,
        written: &WrittenInitializer<'arena>,
    ) -> Result<u64, InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        if matches!(kind, TypeKind::Array { .. })
            && match written {
                WrittenInitializer::Expression(CHirExpr::StringLit(literal, _)) => {
                    self.string_initializes_array_directly(target, literal)
                }
                WrittenInitializer::List { elements, .. } => {
                    elements.first().is_some_and(|element| match element {
                        WrittenInitializer::Expression(CHirExpr::StringLit(literal, _)) => {
                            self.string_initializes_array_directly(target, literal)
                        }
                        WrittenInitializer::Expression(_) | WrittenInitializer::List { .. } => {
                            false
                        }
                    })
                }
                WrittenInitializer::Expression(_) => false,
            }
        {
            return Ok(1);
        }
        match (kind, written) {
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::Expression(_),
            ) => Ok(1),
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::List { elements, .. },
            ) => elements
                .first()
                .map_or(Ok(1), |first| self.initializer_action_count(target, first)),
            (
                TypeKind::Array {
                    element,
                    bound: ArrayBound::Unbounded,
                },
                WrittenInitializer::List { elements, .. },
            ) if self.flexible_initializer_allowed => {
                elements.iter().try_fold(0_u64, |count, written| {
                    count
                        .checked_add(self.initializer_action_count(element, written)?)
                        .ok_or(InitializerPreflightError::TooLarge)
                })
            }
            (
                TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::Union(_),
                WrittenInitializer::List { elements, .. },
            ) => self
                .initializer_aggregate_action_count(target, elements, 0)
                .map(|(_, count)| count),
            (
                TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::Union(_),
                WrittenInitializer::Expression(_),
            ) => Ok(0),
            (TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. }, _) => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }

    fn initializer_aggregate_action_count(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        mut cursor: usize,
    ) -> Result<(usize, u64), InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        let mut count = 0_u64;
        match kind {
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => {
                let mut index = 0_u64;
                while index < bound && cursor < elements.len() {
                    let (next, child_count) =
                        self.initializer_stream_action_count(element, elements, cursor)?;
                    cursor = next;
                    count = checked_initializer_action_add(count, child_count)?;
                    index += 1;
                }
                if index < bound {
                    let child_count =
                        self.initializer_subobject_count(element, &mut HashSet::new())?;
                    let omitted = bound
                        .checked_sub(index)
                        .and_then(|remaining| remaining.checked_mul(child_count))
                        .ok_or(InitializerPreflightError::TooLarge)?;
                    count = checked_initializer_action_add(count, omitted)?;
                }
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => {}
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            } => return Err(InitializerPreflightError::InvalidObject),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let tag_data = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let (fields, is_union) = match &tag_data.state {
                    TagState::Complete(TagDefinition::Struct(fields)) => (fields, false),
                    TagState::Complete(TagDefinition::Union(fields)) => (fields, true),
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
                for (field_index, field) in fields.iter().enumerate() {
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    if !is_union
                        && field_index + 1 == fields.len()
                        && matches!(
                            self.types.get(field.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array {
                                bound: ArrayBound::Unbounded,
                                ..
                            })
                        )
                    {
                        continue;
                    }
                    if cursor < elements.len() {
                        let (next, child_count) =
                            self.initializer_stream_action_count(field.ty, elements, cursor)?;
                        cursor = next;
                        count = checked_initializer_action_add(count, child_count)?;
                    } else {
                        let child_count =
                            self.initializer_subobject_count(field.ty, &mut HashSet::new())?;
                        count = checked_initializer_action_add(count, child_count)?;
                    }
                    if is_union {
                        break;
                    }
                }
            }
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_)
            | TypeKind::Function { .. } => {
                return Err(InitializerPreflightError::InvalidObject);
            }
        }
        Ok((cursor, count))
    }

    fn initializer_stream_action_count(
        &self,
        target: TypeId,
        elements: &[WrittenInitializer<'arena>],
        cursor: usize,
    ) -> Result<(usize, u64), InitializerPreflightError> {
        let initializer = elements
            .get(cursor)
            .ok_or(InitializerPreflightError::InvalidObject)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            return Ok((
                cursor + 1,
                self.initializer_action_count(target, initializer)?,
            ));
        }
        self.initializer_aggregate_action_count(target, elements, cursor)
    }

    fn initializer_subobject_count(
        &self,
        target: TypeId,
        visiting: &mut HashSet<TypeId>,
    ) -> Result<u64, InitializerPreflightError> {
        let kind = self
            .types
            .get(target)
            .ok_or(InitializerPreflightError::InvalidObject)?
            .kind
            .clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_) => Ok(1),
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => {
                if bound > MAX_INITIALIZER_SUBOBJECTS {
                    return Err(InitializerPreflightError::TooLarge);
                }
                let element_count = self.initializer_subobject_count(element, visiting)?;
                let count = bound
                    .checked_mul(element_count)
                    .ok_or(InitializerPreflightError::TooLarge)?;
                if count > MAX_INITIALIZER_SUBOBJECTS {
                    return Err(InitializerPreflightError::TooLarge);
                }
                Ok(count)
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Ok(0),
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            } => Err(InitializerPreflightError::InvalidObject),
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                if !visiting.insert(target) {
                    return Err(InitializerPreflightError::RecursiveObject);
                }
                let tag = self
                    .types
                    .tag(tag)
                    .ok_or(InitializerPreflightError::InvalidObject)?;
                let (fields, is_union) = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields)) => (fields, false),
                    TagState::Complete(TagDefinition::Union(fields)) => (fields, true),
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        visiting.remove(&target);
                        return Err(InitializerPreflightError::InvalidObject);
                    }
                };
                let mut count = 0_u64;
                for (field_index, field) in fields.iter().enumerate() {
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    if !is_union
                        && field_index + 1 == fields.len()
                        && matches!(
                            self.types.get(field.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array {
                                bound: ArrayBound::Unbounded,
                                ..
                            })
                        )
                    {
                        continue;
                    }
                    count = count
                        .checked_add(self.initializer_subobject_count(field.ty, visiting)?)
                        .ok_or(InitializerPreflightError::TooLarge)?;
                    if count > MAX_INITIALIZER_SUBOBJECTS {
                        visiting.remove(&target);
                        return Err(InitializerPreflightError::TooLarge);
                    }
                    if is_union {
                        break;
                    }
                }
                visiting.remove(&target);
                Ok(count)
            }
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => {
                Err(InitializerPreflightError::InvalidObject)
            }
        }
    }

    fn normalize_initializer_value(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        written: &WrittenInitializer<'arena>,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        let kind = self.types.get(target)?.kind.clone();
        match (&kind, written) {
            (
                TypeKind::Array { .. },
                WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)),
            ) if self.string_initializes_array_directly(target, literal) => {
                self.normalize_string_initializer(target, path, literal, *span, actions)
            }
            (
                TypeKind::Array { .. },
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) if designations.first().is_none_or(Option::is_none)
                && elements.first().is_some_and(|element| {
                    matches!(
                        element,
                        WrittenInitializer::Expression(CHirExpr::StringLit(literal, _))
                            if self.string_initializes_array_directly(target, literal)
                    )
                }) =>
            {
                let WrittenInitializer::Expression(CHirExpr::StringLit(literal, span)) =
                    &elements[0]
                else {
                    unreachable!("guard established a string expression")
                };
                self.normalize_string_initializer(target, path, literal, *span, actions)?;
                if let Some(excess) = elements.get(1) {
                    self.report("E2201", "excess-initializer", excess.span());
                    return None;
                }
                Some(())
            }
            (
                TypeKind::Array {
                    element,
                    bound: ArrayBound::Unbounded,
                },
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) if self.flexible_initializer_allowed => {
                if designations.iter().any(Option::is_some) {
                    for (written, designation) in elements.iter().zip(designations) {
                        let Some(designation) = designation else {
                            self.report("E2906", "designator-outside-object", written.span());
                            return None;
                        };
                        let resolved =
                            self.resolve_initializer_designation(target, &path, designation)?;
                        for destination in resolved {
                            self.normalize_initializer_value(
                                destination.ty,
                                destination.path,
                                written,
                                actions,
                            )?;
                        }
                    }
                    return Some(());
                }
                for (index, written) in elements.iter().enumerate() {
                    let index = u64::try_from(index).ok()?;
                    self.normalize_initializer_value(
                        *element,
                        path.clone().with_index(index),
                        written,
                        actions,
                    )?;
                }
                Some(())
            }
            (
                TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::Union(_),
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) => self.normalize_aggregate_elements(
                target,
                path,
                elements,
                designations,
                actions,
                true,
            ),
            (
                TypeKind::Struct(_) | TypeKind::Union(_),
                WrittenInitializer::Expression(expression),
            ) => {
                if self.assignment_constraint(AssignmentOp::Simple, target, expression)
                    != UpdateConstraint::AggregateBoundary
                {
                    self.report("E2202", "invalid-initializer-type", expression.span());
                    return None;
                }
                actions.push(InitAction::copy_aggregate(
                    path,
                    target,
                    expression,
                    expression.span(),
                ));
                Some(())
            }
            (TypeKind::Array { .. }, WrittenInitializer::Expression(expression)) => {
                self.report("E2202", "invalid-initializer-type", expression.span());
                None
            }
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::Expression(expression),
            ) => {
                if self.assignment_constraint(AssignmentOp::Simple, target, expression)
                    != UpdateConstraint::Scalar
                {
                    self.report("E2202", "invalid-initializer-type", expression.span());
                    return None;
                }
                actions.push(InitAction::store_scalar(
                    path,
                    target,
                    expression,
                    expression.span(),
                ));
                Some(())
            }
            (
                TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_),
                WrittenInitializer::List {
                    elements,
                    designations,
                    ..
                },
            ) => {
                if let Some(designation) = designations.iter().flatten().next() {
                    self.report("E2906", "designator-outside-object", designation.span);
                    return None;
                }
                let Some(first) = elements.first() else {
                    actions.push(InitAction::zero_scalar(path, target));
                    return Some(());
                };
                self.normalize_initializer_value(target, path, first, actions)?;
                if let Some(excess) = elements.get(1) {
                    self.report("E2201", "excess-initializer", excess.span());
                    return None;
                }
                Some(())
            }
            _ => {
                self.report("E2202", "invalid-initializer-type", written.span());
                None
            }
        }
    }

    fn normalize_aggregate_elements(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        designations: &[Option<InitializerDesignation<'arena>>],
        actions: &mut Vec<InitAction<'arena>>,
        diagnose_excess: bool,
    ) -> Option<()> {
        if self.aggregate_first_initializable_member_is_unbounded(target)
            && self.final_flexible_array_member(target).is_none()
        {
            if let Some(first) = elements.first() {
                self.report(
                    "E2706",
                    "unsupported-unbounded-array-initializer",
                    first.span(),
                );
                return None;
            }
        }
        if designations.iter().any(Option::is_some) {
            return self.normalize_designated_aggregate_elements(
                target,
                path,
                elements,
                designations,
                actions,
            );
        }
        let Some(children) = self.aggregate_initializer_children(target) else {
            self.report(
                "E2202",
                "invalid-initializer-type",
                elements
                    .first()
                    .map_or_else(|| self.span(), WrittenInitializer::span),
            );
            return None;
        };
        if !self.flexible_initializer_allowed
            && self.final_flexible_array_member(target).is_some()
            && elements.len() > children.len()
        {
            self.report(
                "E2919",
                "flexible-array-initializer-not-static",
                elements[children.len()].span(),
            );
            return None;
        }
        let mut cursor = 0;
        for (step, child) in children {
            let child_path = extend_subobject_path(path.clone(), step);
            if cursor < elements.len() {
                self.normalize_stream_subobject(child, child_path, elements, &mut cursor, actions)?;
            } else {
                self.zero_fill_initializer(child, child_path, actions)?;
            }
        }
        if diagnose_excess {
            if let Some(excess) = elements.get(cursor) {
                self.report("E2201", "excess-initializer", excess.span());
                return None;
            }
        }
        Some(())
    }

    fn normalize_designated_aggregate_elements(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        designations: &[Option<InitializerDesignation<'arena>>],
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        if elements.len() != designations.len() {
            unreachable!("initializer elements and designations are positional peers");
        }
        if !self.flexible_initializer_allowed
            && self.final_flexible_array_member(target).is_some()
            && designations.iter().flatten().any(|designation| {
                designation.designators.iter().any(|designator| {
                    matches!(designator, WrittenDesignator::Field(name) if self.final_flexible_array_member(target).is_some_and(|(_, field, _)| name.spelling == field))
                })
            })
        {
            let span = designations
                .iter()
                .flatten()
                .find_map(|designation| {
                    designation.designators.iter().find_map(|designator| match designator {
                        WrittenDesignator::Field(name) => Some(name.span),
                        _ => None,
                    })
                })
                .unwrap_or_else(|| self.span());
            self.report("E2919", "flexible-array-initializer-not-static", span);
            return None;
        }
        let Some(children) = self.aggregate_initializer_children(target) else {
            let span = designations
                .iter()
                .flatten()
                .next()
                .map_or_else(|| self.span(), |designation| designation.span);
            self.report("E2906", "designator-outside-object", span);
            return None;
        };

        // Establish the same complete-object zero state as a positional
        // initializer. Explicit source writes follow it and are canonicalized
        // with last-write-wins when the plan is constructed.
        self.zero_fill_initializer(target, path.clone(), actions)?;
        let mut cursor = 0_usize;
        for (element, designation) in elements.iter().zip(designations) {
            if let Some(designation) = designation {
                let resolved = self.resolve_initializer_designation(target, &path, designation)?;
                cursor = self.designation_successor_cursor(&path, &children, &resolved);
                for destination in resolved {
                    self.normalize_initializer_value(
                        destination.ty,
                        destination.path,
                        element,
                        actions,
                    )?;
                }
                continue;
            }

            let Some((step, child)) = children.get(cursor).copied() else {
                self.report("E2201", "excess-initializer", element.span());
                return None;
            };
            self.normalize_initializer_value(
                child,
                extend_subobject_path(path.clone(), step),
                element,
                actions,
            )?;
            cursor += 1;
        }
        Some(())
    }

    fn resolve_initializer_designation(
        &mut self,
        target: TypeId,
        base_path: &SubobjectPath,
        designation: &InitializerDesignation<'arena>,
    ) -> Option<Vec<ResolvedInitializerTarget>> {
        let mut resolved = vec![ResolvedInitializerTarget {
            path: base_path.clone(),
            ty: target,
        }];
        for designator in &designation.designators {
            let mut next = Vec::new();
            match designator {
                WrittenDesignator::Field(name) => {
                    for current in resolved {
                        let Some(fields) = self.designated_field(current.ty, &name.spelling) else {
                            self.report("E2906", "designator-outside-object", name.span);
                            return None;
                        };
                        let field_type = fields.last()?.field_ty();
                        let path = fields.into_iter().fold(current.path, |path, field| {
                            path.with_field(field.field_index())
                        });
                        next.push(ResolvedInitializerTarget {
                            path,
                            ty: field_type,
                        });
                    }
                }
                WrittenDesignator::Index { expression, span } => {
                    let index = self.designator_constant_index(expression, *span)?;
                    for current in resolved {
                        let element = self.designated_element_type(current.ty, index, *span)?;
                        next.push(ResolvedInitializerTarget {
                            path: current.path.with_index(index),
                            ty: element,
                        });
                    }
                }
                WrittenDesignator::Range { first, last, span } => {
                    let first = self.designator_constant_index(first, *span)?;
                    let last = self.designator_constant_index(last, *span)?;
                    if first > last
                        || last
                            .checked_sub(first)
                            .is_none_or(|width| width >= MAX_INITIALIZER_SUBOBJECTS)
                    {
                        self.report("E2906", "designator-outside-object", *span);
                        return None;
                    }
                    for current in resolved {
                        for index in first..=last {
                            let element = self.designated_element_type(current.ty, index, *span)?;
                            next.push(ResolvedInitializerTarget {
                                path: current.path.clone().with_index(index),
                                ty: element,
                            });
                        }
                    }
                }
            }
            resolved = next;
        }
        Some(resolved)
    }

    fn designated_field(&self, target: TypeId, spelling: &str) -> Option<Vec<FieldProjection>> {
        self.record_member_path(target, spelling)
    }

    fn designator_constant_index(
        &mut self,
        expression: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Option<u64> {
        let index = self
            .eval_integer_constant_expression(expression)
            .ok()
            .and_then(|value| u64::try_from(value.mathematical_value()).ok());
        let Some(index) = index else {
            self.report("E2906", "designator-outside-object", span);
            return None;
        };
        Some(index)
    }

    fn designated_element_type(
        &mut self,
        target: TypeId,
        index: u64,
        span: SourceSpan,
    ) -> Option<TypeId> {
        let Some(TypeKind::Array { element, bound }) =
            self.types.get(target).map(|ty| ty.kind.clone())
        else {
            self.report("E2906", "designator-outside-object", span);
            return None;
        };
        if matches!(bound, ArrayBound::Fixed(size) if index >= size) {
            self.report("E2906", "designator-outside-object", span);
            return None;
        }
        Some(element)
    }

    fn designation_successor_cursor(
        &self,
        base_path: &SubobjectPath,
        children: &[(SubobjectStep, TypeId)],
        resolved: &[ResolvedInitializerTarget],
    ) -> usize {
        resolved
            .iter()
            .filter_map(|target| target.path.steps().get(base_path.steps().len()))
            .filter_map(|step| children.iter().position(|(candidate, _)| candidate == step))
            .max()
            .map_or(children.len(), |index| index + 1)
    }

    fn final_flexible_array_member(&self, target: TypeId) -> Option<(usize, String, TypeId)> {
        let Some(TypeKind::Struct(tag)) = self.types.get(target).map(|ty| &ty.kind) else {
            return None;
        };
        let tag = self.types.tag(*tag)?;
        let fields = match &tag.state {
            TagState::Complete(TagDefinition::Struct(fields)) => fields,
            TagState::Incomplete
            | TagState::Complete(TagDefinition::Union(_) | TagDefinition::Enum(_)) => return None,
        };
        let (field_index, field) = fields.iter().enumerate().next_back()?;
        let Some(TypeKind::Array {
            element,
            bound: ArrayBound::Unbounded,
        }) = self.types.get(field.ty).map(|ty| &ty.kind)
        else {
            return None;
        };
        Some((field_index, field.name.clone()?, *element))
    }

    fn aggregate_first_initializable_member_is_unbounded(&self, target: TypeId) -> bool {
        let Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) =
            self.types.get(target).map(|ty| &ty.kind)
        else {
            return false;
        };
        let Some(tag) = self.types.tag(*tag) else {
            return false;
        };
        let fields = match &tag.state {
            TagState::Complete(TagDefinition::Struct(fields))
            | TagState::Complete(TagDefinition::Union(fields)) => fields,
            TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => return false,
        };
        fields
            .iter()
            .find(|field| !(field.name.is_none() && field.bit_field.is_some()))
            .is_some_and(|field| {
                matches!(
                    self.types.get(field.ty).map(|ty| &ty.kind),
                    Some(TypeKind::Array {
                        bound: ArrayBound::Unbounded,
                        ..
                    })
                )
            })
    }

    fn flexible_initializer_element_count(
        &self,
        target: TypeId,
        actions: &[InitAction<'arena>],
    ) -> u64 {
        let Some((field_index, _, _)) = self.final_flexible_array_member(target) else {
            return 0;
        };
        actions
            .iter()
            .filter_map(|action| match action.path().steps() {
                [SubobjectStep::Field {
                    field_index: candidate,
                }, SubobjectStep::Index { index }]
                    if *candidate == field_index =>
                {
                    index.checked_add(1)
                }
                _ => None,
            })
            .max()
            .unwrap_or(0)
    }

    fn normalize_stream_subobject(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        elements: &[WrittenInitializer<'arena>],
        cursor: &mut usize,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        let initializer = elements.get(*cursor)?;
        if matches!(initializer, WrittenInitializer::List { .. })
            || self.direct_initializer_expression(target, initializer)
        {
            *cursor += 1;
            return self.normalize_initializer_value(target, path, initializer, actions);
        }
        let undesignated = vec![None; elements.len().saturating_sub(*cursor)];
        self.normalize_aggregate_elements(
            target,
            path,
            &elements[*cursor..],
            &undesignated,
            actions,
            false,
        )?;
        *cursor = self.initializer_stream_consumption(target, elements, *cursor)?;
        Some(())
    }

    fn zero_fill_initializer(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        let kind = self.types.get(target)?.kind.clone();
        match kind {
            TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Enum(_)
            | TypeKind::GnuInt128(_) => {
                actions.push(InitAction::zero_scalar(path, target));
                Some(())
            }
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Some(()),
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            } => {
                self.report("E2203", "variable-length-object", self.span());
                None
            }
            TypeKind::Array { .. } | TypeKind::Struct(_) | TypeKind::Union(_) => {
                let children = self.aggregate_initializer_children(target)?;
                for (step, child) in children {
                    self.zero_fill_initializer(
                        child,
                        extend_subobject_path(path.clone(), step),
                        actions,
                    )?;
                }
                Some(())
            }
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => None,
        }
    }

    fn normalize_string_initializer(
        &mut self,
        target: TypeId,
        path: SubobjectPath,
        literal: &StringLit,
        span: SourceSpan,
        actions: &mut Vec<InitAction<'arena>>,
    ) -> Option<()> {
        if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
            self.report("E2205", "not-covered-day5: wide-string-initializer", span);
            return None;
        }
        let canonical = self.types.get(target)?;
        let TypeKind::Array { element, bound } = &canonical.kind else {
            self.report("E2202", "invalid-initializer-type", span);
            return None;
        };
        if !self.is_narrow_character_type(*element) {
            self.report("E2202", "invalid-initializer-type", span);
            return None;
        }
        let units = literal.explicit_unit_count()?;
        if let ArrayBound::Fixed(bound) = bound {
            if units > *bound {
                self.report(
                    "E2204",
                    format!("string-initializer-too-long: {units} > {bound}"),
                    span,
                );
                return None;
            }
        }
        actions.push(InitAction::store_string(
            path,
            target,
            literal.clone(),
            span,
        ));
        Some(())
    }

    fn direct_initializer_expression(
        &self,
        target: TypeId,
        initializer: &WrittenInitializer<'arena>,
    ) -> bool {
        let WrittenInitializer::Expression(expression) = initializer else {
            return false;
        };
        let Some(kind) = self.types.get(target).map(|canonical| &canonical.kind) else {
            return false;
        };
        if matches!(
            kind,
            TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_)
        ) {
            return true;
        }
        if matches!(expression, CHirExpr::StringLit(_, _)) && matches!(kind, TypeKind::Array { .. })
        {
            return true;
        }
        matches!(kind, TypeKind::Struct(_) | TypeKind::Union(_))
            && self.assignment_constraint(AssignmentOp::Simple, target, expression)
                == UpdateConstraint::AggregateBoundary
    }

    fn aggregate_initializer_children(
        &self,
        target: TypeId,
    ) -> Option<Vec<(SubobjectStep, TypeId)>> {
        match self.types.get(target)?.kind.clone() {
            TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(bound),
            } => Some(
                (0..bound)
                    .map(|index| (SubobjectStep::Index { index }, element))
                    .collect(),
            ),
            TypeKind::Array {
                bound: ArrayBound::Unbounded,
                ..
            } => Some(Vec::new()),
            TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            } => None,
            TypeKind::Struct(tag) | TypeKind::Union(tag) => {
                let tag = self.types.tag(tag)?;
                let fields = match &tag.state {
                    TagState::Complete(TagDefinition::Struct(fields))
                    | TagState::Complete(TagDefinition::Union(fields)) => fields,
                    TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => {
                        return None;
                    }
                };
                let is_union = matches!(self.types.get(target)?.kind, TypeKind::Union(_));
                let mut children = Vec::new();
                for (field_index, field) in fields.iter().enumerate() {
                    if field.name.is_none() && field.bit_field.is_some() {
                        continue;
                    }
                    if !is_union
                        && field_index + 1 == fields.len()
                        && matches!(
                            self.types.get(field.ty).map(|ty| &ty.kind),
                            Some(TypeKind::Array {
                                bound: ArrayBound::Unbounded,
                                ..
                            })
                        )
                        && !self.flexible_initializer_allowed
                    {
                        continue;
                    }
                    children.push((SubobjectStep::Field { field_index }, field.ty));
                    if is_union {
                        break;
                    }
                }
                Some(children)
            }
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_)
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Function { .. }
            | TypeKind::Enum(_) => None,
        }
    }

    fn is_narrow_character_type(&self, target: TypeId) -> bool {
        self.types.get(target).is_some_and(|canonical| {
            !canonical.qualifiers.is_atomic
                && matches!(
                    canonical.kind,
                    TypeKind::Int(IntRank::Char | IntRank::SChar | IntRank::UChar)
                )
        })
    }

    fn string_initializes_array_directly(&self, target: TypeId, literal: &StringLit) -> bool {
        if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
            return true;
        }
        matches!(
            self.types.get(target).map(|ty| &ty.kind),
            Some(TypeKind::Array { element, .. }) if self.is_narrow_character_type(*element)
        )
    }

    fn bind_parameters(&mut self, syntax: &mut ParameterSyntax) {
        let ParameterSyntax::Prototype { parameters, .. } = syntax else {
            return;
        };
        for parameter in parameters {
            let Some(name) = parameter.name.as_ref() else {
                parameter.symbol = None;
                continue;
            };
            parameter.symbol = Some(self.declare_symbol(
                name,
                SymbolData::Parameter {
                    storage: StorageClass::None,
                },
                parameter.ty,
            ));
        }
    }

    fn parse_function_body(&mut self) -> &'arena CHirStmt<'arena> {
        let outer_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let outer_switch_depth = std::mem::replace(&mut self.switch_depth, 0);
        // The function's outer compound statement and its parameters share
        // one function scope. Nested compounds create block scopes normally.
        let body = self.parse_compound_statement(false);
        self.loop_depth = outer_loop_depth;
        self.switch_depth = outer_switch_depth;
        body
    }

    fn in_function_scope(&self) -> bool {
        let mut scope = Some(self.current_scope);
        while let Some(id) = scope {
            let current = self
                .symbols
                .scope(id)
                .expect("the parser current scope chain is always live");
            if current.kind() == ScopeKind::Function {
                return true;
            }
            scope = current.parent();
        }
        false
    }

    fn parse_required_constant_conditional(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_conditional()
    }

    fn parse_required_constant_assignment(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        self.parse_assignment()
    }

    fn validate_asm_goto_labels(&mut self, body: &CHirStmt<'_>) -> Option<()> {
        let mut defined = HashSet::new();
        let mut referenced = Vec::new();
        collect_asm_goto_labels(body, &mut defined, &mut referenced);
        if let Some(label) = referenced
            .into_iter()
            .find(|label| !defined.contains(&label.spelling))
        {
            self.report("E2914", "malformed-asm", label.span);
            return None;
        }
        Some(())
    }

    fn validate_statement_expression_jumps(&mut self, body: &CHirStmt<'_>) -> Option<()> {
        let mut labels = HashMap::new();
        let mut gotos = Vec::new();
        let mut regions = Vec::new();
        let mut next_region = 0_u32;
        collect_jump_regions_stmt(
            body,
            &mut regions,
            &mut next_region,
            &mut labels,
            &mut gotos,
        );
        if let Some(reference) = gotos
            .iter()
            .find(|reference| !labels.contains_key(&reference.name))
        {
            self.report("E2920", "undeclared-label", reference.span);
            return None;
        }
        if let Some(reference) = gotos.into_iter().find(|reference| {
            reference.checks_region_entry
                && labels
                    .get(&reference.name)
                    .is_some_and(|target| !reference.regions.starts_with(target))
        }) {
            // Revision 1.9.12 makes the construct malformed but does not
            // assign a dedicated diagnostic. Keep the established generic
            // syntax diagnostic until that contract gap is adjudicated.
            self.report("E1003", "expected-expression", reference.span);
            return None;
        }
        Some(())
    }

    /// Does a function body follow the current position once any attribute
    /// specifiers are skipped? Decides the placement label of those attributes
    /// without consuming them.
    fn declarator_is_followed_by_body(&self) -> bool {
        let mut index = self.pos;
        while self.tokens.get(index).map(|token| token.kind) == Some(TokenKind::Attribute) {
            // skip `__attribute__ ( ( ... ) )` by balancing parentheses
            index += 1;
            let mut depth = 0_usize;
            let mut seen_open = false;
            while let Some(token) = self.tokens.get(index) {
                match token.kind {
                    TokenKind::LParen => {
                        depth += 1;
                        seen_open = true;
                    }
                    TokenKind::RParen => {
                        depth = depth.saturating_sub(1);
                    }
                    _ => {}
                }
                index += 1;
                if seen_open && depth == 0 {
                    break;
                }
            }
        }
        self.tokens.get(index).map(|token| token.kind) == Some(TokenKind::LBrace)
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
        gnu_extension_spans: Vec<SourceSpan>,
    ) -> &'arena CHirDecl<'arena> {
        self.alloc_node(CHirDecl::Tag(TagDecl {
            tag: event.tag,
            scope: event.scope,
            name: event.name,
            members: event.members,
            span: declaration_span,
            specifier_span: event.specifier_span,
            gnu_extension_spans,
            attributes: event.attributes,
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

    // ── GNU attributes ───────────────────────────────────────────────────

    /// Consume every `__attribute__((...))` specifier at the cursor.
    ///
    /// Each attribute is kept as written, with arguments as text, and is
    /// labelled with the placement the caller is parsing, since placement is
    /// what decides attachment. Nothing is applied here; callers that own a
    /// type, tag or member apply `aligned` and `packed` and leave `applied`
    /// set so the dump and the inventory can tell the two apart.
    fn parse_attribute_specifiers(&mut self, placement: AttributePlacement) -> Vec<Attribute> {
        let mut attributes = Vec::new();
        while self.peek_kind() == Some(TokenKind::Attribute) {
            let keyword = self.advance().expect("peeked attribute keyword");
            // `expect` reports its own diagnostic, and the first diagnostic is
            // the one a reader sees, so the delimiters are checked by hand to
            // keep the report on the attribute rather than on a stray token.
            for _ in 0..2 {
                if self.peek_kind() != Some(TokenKind::LParen) {
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
                if self.peek_kind() == Some(TokenKind::RParen) {
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
                let written = self.token_text(&name_token).to_owned();
                let name = Attribute::canonical_name(&written);
                let mut span = name_token.span;
                let mut arguments = Vec::new();
                if self.peek_kind() == Some(TokenKind::LParen) {
                    let open = self.advance().expect("peeked argument list");
                    let parsed = if name == "aligned" && self.peek_kind() != Some(TokenKind::RParen)
                    {
                        self.parse_integer_constant_argument(open.span)
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
                attributes.push(Attribute {
                    name,
                    written,
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
                if self.peek_kind() != Some(TokenKind::RParen) {
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
    fn parse_attribute_arguments(
        &mut self,
        open: SourceSpan,
    ) -> Option<(Vec<AttributeArgument>, SourceSpan)> {
        let mut arguments = Vec::new();
        let mut depth = 0_usize;
        let mut first: Option<SourceSpan> = None;
        let mut last: Option<SourceSpan> = None;
        loop {
            let Some(kind) = self.peek_kind() else {
                self.report("E2903", "malformed-attribute", open);
                return None;
            };
            match kind {
                TokenKind::RParen if depth == 0 => {
                    let close = self.advance().expect("peeked close").span;
                    if let (Some(start), Some(end)) = (first, last) {
                        arguments.push(self.attribute_argument(start, end));
                    } else if !arguments.is_empty() {
                        // `aligned(4,)` -- a trailing separator with nothing after it.
                        self.report("E2903", "malformed-attribute", close);
                        return None;
                    }
                    return Some((arguments, close));
                }
                TokenKind::Comma if depth == 0 => {
                    let comma = self.advance().expect("peeked comma").span;
                    match (first, last) {
                        (Some(start), Some(end)) => {
                            arguments.push(self.attribute_argument(start, end))
                        }
                        _ => {
                            self.report("E2903", "malformed-attribute", comma);
                            return None;
                        }
                    }
                    first = None;
                    last = None;
                }
                TokenKind::Semicolon | TokenKind::LBrace | TokenKind::RBrace => {
                    // An argument list cannot contain these; the list was never
                    // closed. Report at the opening parenthesis, where the
                    // reader will look.
                    self.report("E2903", "malformed-attribute", open);
                    return None;
                }
                _ => {
                    let token = self.advance().expect("peeked argument token");
                    match token.kind {
                        TokenKind::LParen => depth += 1,
                        TokenKind::RParen => depth -= 1,
                        _ => {}
                    }
                    first.get_or_insert(token.span);
                    last = Some(token.span);
                }
            }
        }
    }

    /// Read one argument as an integer constant expression in place, so type
    /// names and `sizeof` resolve in the surrounding scope; the source text is
    /// kept alongside the value.
    fn parse_integer_constant_argument(
        &mut self,
        open: SourceSpan,
    ) -> Option<(Vec<AttributeArgument>, SourceSpan)> {
        if matches!(self.peek_kind(), Some(TokenKind::Comma) | None) {
            // `aligned(,)`: an argument slot with nothing in it is a malformed
            // attribute, reported as such rather than as a missing expression.
            self.report("E2903", "malformed-attribute", self.span());
            return None;
        }
        let expression = self.parse_required_constant_conditional()?;
        let span = expression.span();
        let mut argument = self.attribute_argument(span, span);
        argument.integer = self
            .eval_integer_constant_expression(expression)
            .ok()
            .and_then(|value| u64::try_from(value.mathematical_value()).ok());
        if argument.integer.is_none() {
            self.report("E2903", "malformed-attribute", argument.span);
            return None;
        }
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2903", "malformed-attribute", open);
            return None;
        }
        let close = self.advance().expect("peeked close").span;
        Some((vec![argument], close))
    }

    fn attribute_argument(&self, start: SourceSpan, end: SourceSpan) -> AttributeArgument {
        let text = self.source[start.byte_start as usize..end.byte_end as usize].to_owned();
        AttributeArgument {
            text,
            span: self.merge_span(start, end),
            integer: None,
        }
    }

    /// Resolve `aligned` to bytes, or report why it cannot be.
    ///
    /// GNU C bounds the argument to a power of two no larger than 2^28; the
    /// target layout represents any power of two up to that, so the two
    /// limits coincide here. Bare `aligned` is the largest fundamental
    /// alignment, sixteen bytes on AArch64.
    fn aligned_bytes(&mut self, attribute: &Attribute) -> Option<u32> {
        const LARGEST_FUNDAMENTAL_ALIGNMENT: u32 = 16;
        const GNU_MAXIMUM_ALIGNMENT: u64 = 1 << 28;
        match attribute.arguments.as_slice() {
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

    /// Apply `aligned` to a type, yielding the (possibly re-interned) type and
    /// marking the attribute applied. `packed` on a record type is forwarded to
    /// the tag; on anything else it is preserved without effect, which is what
    /// GNU C does too.
    fn apply_attributes_to_type(&mut self, ty: TypeId, attributes: &mut [Attribute]) -> TypeId {
        let mut ty = ty;
        for attribute in attributes.iter_mut() {
            match attribute.name.as_str() {
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
        ty
    }

    /// Pointer-position attributes are never applied, but an `aligned`
    /// argument must still satisfy the attribute grammar's representability
    /// and power-of-two constraint.
    fn validate_pointer_attributes(&mut self, attributes: &[Attribute]) {
        for attribute in attributes {
            if attribute.name == "aligned" {
                let _ = self.aligned_bytes(attribute);
            }
        }
    }

    /// Apply `packed` and `aligned` written on a tag itself.
    fn apply_attributes_to_tag(&mut self, tag: TagId, attributes: &mut [Attribute]) {
        for attribute in attributes.iter_mut() {
            match attribute.name.as_str() {
                "packed" => {
                    if !attribute.arguments.is_empty() {
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

    /// The layout attributes a member carries, with `applied` marked.
    fn member_layout_attributes(&mut self, attributes: &mut [Attribute]) -> FieldLayoutAttributes {
        let mut layout = FieldLayoutAttributes::default();
        for attribute in attributes.iter_mut() {
            match attribute.name.as_str() {
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

    fn parse_declaration_modifiers(
        &mut self,
        storage: &mut Option<StorageClass>,
        inline_spans: &mut Vec<SourceSpan>,
        attributes: &mut Vec<Attribute>,
    ) {
        loop {
            if self.peek_kind() == Some(TokenKind::Attribute) {
                attributes.extend(
                    self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers),
                );
                continue;
            }
            if self.peek_kind() == Some(TokenKind::Inline) {
                inline_spans.push(self.advance().expect("peeked inline specifier").span);
                continue;
            }
            if storage.is_none()
                && matches!(
                    self.peek_kind(),
                    Some(
                        TokenKind::Typedef
                            | TokenKind::Extern
                            | TokenKind::Static
                            | TokenKind::Auto
                            | TokenKind::Register
                            | TokenKind::ThreadLocal
                    )
                )
            {
                *storage = Some(self.parse_storage_class());
                continue;
            }
            break;
        }
    }

    fn parse_type_specifier(&mut self) -> Option<ParsedSpecifier<'arena>> {
        self.parse_type_specifier_inner(None)
    }

    fn parse_declaration_type_specifier(
        &mut self,
        inline_spans: &mut Vec<SourceSpan>,
    ) -> Option<ParsedSpecifier<'arena>> {
        if self.peek_kind() == Some(TokenKind::AutoType) {
            let keyword = self.advance().expect("peeked __auto_type");
            // The real type is installed transactionally after the one
            // required initializer has been parsed. This placeholder is
            // never allowed to escape a completed declaration.
            let placeholder = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::AutoType {
                    resolved: placeholder,
                    span: keyword.span,
                },
                tag_event: None,
                inherited_function_parameters: None,
            });
        }
        self.parse_type_specifier_inner(Some(inline_spans))
    }

    fn parse_type_specifier_inner(
        &mut self,
        mut inline_spans: Option<&mut Vec<SourceSpan>>,
    ) -> Option<ParsedSpecifier<'arena>> {
        let start = self.span();
        let mut qualifiers = self.parse_qualifiers_with_inline(&mut inline_spans);
        if matches!(
            self.peek_kind(),
            Some(TokenKind::Struct | TokenKind::Union | TokenKind::Enum)
        ) {
            let token = self.advance().expect("peeked tag keyword");
            let kind = match token.kind {
                TokenKind::Struct => TagKind::Struct,
                TokenKind::Union => TagKind::Union,
                TokenKind::Enum => TagKind::Enum,
                _ => unreachable!("tag keyword checked above"),
            };
            let mut parsed = self.parse_tag_specifier(kind, qualifiers, token.span)?;
            let trailing = self.parse_qualifiers_with_inline(&mut inline_spans);
            if trailing != Qualifiers::default() {
                let TypeSpecifier::Tag {
                    ty,
                    tag,
                    kind,
                    name,
                    span,
                } = parsed.surface
                else {
                    unreachable!("tag parser returns a tag specifier")
                };
                parsed.surface = TypeSpecifier::Tag {
                    ty: self.qualify_typedef_type(ty, trailing),
                    tag,
                    kind,
                    name,
                    span,
                };
            }
            return Some(parsed);
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::Typeof | TokenKind::TypeofUnqual)
        ) {
            let keyword = self.advance().expect("peeked typeof keyword");
            let strip_top_level = keyword.kind == TokenKind::TypeofUnqual;
            self.expect(TokenKind::LParen)?;
            let (base, operand, tag_event, inherited_function_parameters) = if self
                .peek()
                .is_some_and(|token| self.token_starts_type_name(token))
            {
                let (ty, tag_event) = self.parse_type_name_with_vla_error(Some(keyword.span))?;
                (ty, TypeofOperand::TypeName, tag_event, None)
            } else {
                let transaction = self.symbols.begin_transaction();
                let mut installed = HashSet::new();
                for shadow in self.temporary_parameter_shadows.clone() {
                    if shadow.scope == self.current_scope
                        && installed.insert(shadow.spelling.clone())
                    {
                        self.symbols
                            .declare(
                                shadow.scope,
                                &shadow.spelling,
                                SymbolData::Parameter {
                                    storage: StorageClass::None,
                                },
                                shadow.ty,
                                shadow.span,
                            )
                            .expect("temporary parameter spellings are unique per probe");
                    }
                }
                let expression = self.parse_expression();
                let inferred = expression.and_then(|expression| {
                    let inherited = match expression {
                        CHirExpr::Ident { symbol, .. } => {
                            self.inherited_function_parameters.get(symbol).cloned()
                        }
                        _ => None,
                    };
                    self.expression_type(expression)
                        .map(|inferred| (expression, inferred.ty, inherited))
                });
                self.symbols
                    .rollback_transaction(&transaction)
                    .expect("temporary typeof bindings are table-local and LIFO");
                let Some((expression, inferred, inherited)) = inferred else {
                    if self.diagnostics.is_empty() {
                        self.report("E1003", "expected-expression", self.span());
                    }
                    return None;
                };
                let ty = self.materialize_inferred_type(inferred, Some(expression))?;
                if self.type_is_variably_modified(ty) {
                    self.report(
                        "E2904",
                        "not-covered: variably-modified-typeof",
                        keyword.span,
                    );
                }
                (ty, TypeofOperand::Expression, None, inherited)
            };
            let close = self.expect(TokenKind::RParen)?;
            let base = if strip_top_level {
                self.strip_top_level_qualifiers(base)
            } else {
                base
            };
            let trailing = self.parse_qualifiers_with_inline(&mut inline_spans);
            qualifiers = merge_qualifiers(qualifiers, trailing);
            let resolved = self.qualify_typedef_type(base, qualifiers);
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::Typeof {
                    resolved,
                    operand,
                    span: self.merge_span(start, close.span),
                },
                tag_event,
                inherited_function_parameters,
            });
        }

        if let Some((symbol, alias_ty)) = self.peek_typedef_name() {
            let token = self.advance().expect("peeked typedef name");
            let name = SpannedName {
                spelling: self.token_text(&token).to_owned(),
                span: token.span,
            };
            let trailing = self.parse_qualifiers_with_inline(&mut inline_spans);
            qualifiers = merge_qualifiers(qualifiers, trailing);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(token.span, |token| token.span);
            let ty = self.qualify_typedef_type(alias_ty, qualifiers);
            return Some(ParsedSpecifier {
                surface: TypeSpecifier::TypedefName {
                    ty,
                    symbol,
                    name,
                    span: self.merge_span(start, end),
                },
                tag_event: None,
                inherited_function_parameters: self
                    .inherited_function_parameters
                    .get(&symbol)
                    .cloned(),
            });
        }

        let mut words = Vec::new();
        let mut end = None;
        while let Some(kind) = self.peek_kind() {
            if self.capture_inline_specifier(&mut inline_spans) {
                continue;
            }
            if is_builtin_type_word(kind) {
                let token = self.advance().expect("peeked builtin type word");
                let forced_int128_signedness = match (kind, self.token_text(&token)) {
                    (TokenKind::GnuInt128, "__int128_t") => Some(GnuInt128Signedness::Signed),
                    (TokenKind::GnuInt128, "__uint128_t") => Some(GnuInt128Signedness::Unsigned),
                    _ => None,
                };
                words.push(BuiltinTypeWord {
                    kind,
                    forced_int128_signedness,
                });
                end = Some(token.span);
                continue;
            }
            let qualifier = match kind {
                TokenKind::Const => Some((&mut qualifiers.is_const, true)),
                TokenKind::Volatile => Some((&mut qualifiers.is_volatile, true)),
                TokenKind::Restrict => Some((&mut qualifiers.is_restrict, true)),
                TokenKind::Atomic => Some((&mut qualifiers.is_atomic, true)),
                _ => None,
            };
            let Some((slot, value)) = qualifier else {
                break;
            };
            *slot = value;
            end = self.advance().map(|token| token.span);
        }
        if words.is_empty() {
            return None;
        }
        let end = end.expect("a builtin type word established an end span");
        let Some(kind) = resolve_builtin_type(&words) else {
            self.report(
                "E1007",
                "invalid-type-specifier-combination",
                self.merge_span(start, end),
            );
            return None;
        };
        let ty = self.intern(kind, qualifiers);
        Some(ParsedSpecifier {
            surface: TypeSpecifier::Builtin {
                ty,
                span: self.merge_span(start, end),
            },
            tag_event: None,
            inherited_function_parameters: None,
        })
    }

    fn capture_inline_specifier(
        &mut self,
        inline_spans: &mut Option<&mut Vec<SourceSpan>>,
    ) -> bool {
        if inline_spans.is_none() || self.peek_kind() != Some(TokenKind::Inline) {
            return false;
        }
        let span = self.advance().expect("peeked inline specifier").span;
        inline_spans
            .as_mut()
            .expect("inline capture was enabled")
            .push(span);
        true
    }

    fn parse_qualifiers_with_inline(
        &mut self,
        inline_spans: &mut Option<&mut Vec<SourceSpan>>,
    ) -> Qualifiers {
        let mut qualifiers = Qualifiers::default();
        loop {
            if self.capture_inline_specifier(inline_spans) {
                continue;
            }
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
        let is_definition = self.peek_kind() == Some(TokenKind::LBrace);
        let is_pure_forward =
            !is_definition && name.is_some() && self.peek_kind() == Some(TokenKind::Semicolon);
        if name.is_none() && !is_definition {
            self.report("E1001", "expected-declaration", self.span());
            return None;
        }

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

        if is_definition {
            let duplicate = matches!(
                self.types.tag(tag).map(|tag| &tag.state),
                Some(TagState::Complete(_))
            );
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
                TagKind::Enum => self.parse_enum_members()?,
            };
            let close = self.expect(TokenKind::RBrace);
            tag_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::TagClosingBrace));
            self.apply_attributes_to_tag(tag, &mut tag_attributes);
            let end = close
                .as_ref()
                .map_or_else(|| self.span(), |token| token.span);
            if !duplicate
                && binding_kind == requested_kind
                && self.types.complete_tag(tag, definition).is_err()
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

    fn parse_record_members(
        &mut self,
        kind: TagKind,
        containing_tag: TagId,
    ) -> Option<(TagMembers<'arena>, TagDefinition)> {
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            if self.match_kind(TokenKind::Semicolon) {
                continue;
            }
            if self.peek_kind() == Some(TokenKind::StaticAssert) {
                self.parse_static_assert_declaration()?;
                continue;
            }
            let start = self.span();
            let mut specifier_attributes =
                self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers);
            let Some(specifier) = self.parse_type_specifier() else {
                self.report("E1001", "expected-declaration", self.span());
                self.advance();
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
                    self.validate_variable_array_bounds(&declarator.derived);
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
                let member_layout = self.member_layout_attributes(&mut member_attributes);
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
            if let Some(event) = specifier.tag_event {
                let span = self.merge_span(start, declaration_end);
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
        }
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

    fn validate_record_member_types(
        &mut self,
        kind: TagKind,
        containing_tag: TagId,
        fields: &[RecordField],
        members: &[RecordMember<'arena>],
    ) {
        for (index, (field, member)) in fields.iter().zip(members).enumerate() {
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

    fn record_member_type_is_complete(
        &self,
        ty: TypeId,
        containing_tag: TagId,
        visiting: &mut HashSet<TypeId>,
    ) -> bool {
        let Some(kind) = self.types.get(ty).map(|ty| ty.kind.clone()) else {
            return false;
        };
        match kind {
            TypeKind::GnuInt128(_)
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_) => true,
            TypeKind::Array { element, .. } => {
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
            TypeKind::Void | TypeKind::BuiltinVaList | TypeKind::Function { .. } => false,
        }
    }

    /// Validate the C11 bit-field constraints that are independent of record
    /// placement. The written expression remains in C-HIR even when this
    /// reports an error; only validated metadata enters the canonical type.
    fn validate_bit_field(
        &mut self,
        ty: TypeId,
        name: Option<&str>,
        type_span: SourceSpan,
        width_expression: &CHirExpr<'_>,
    ) -> Option<BitFieldSpec> {
        let Some(base_width_bits) = self.bit_field_base_width(ty) else {
            self.report("E1305", "bit-field-has-non-integer-type", type_span);
            return None;
        };

        let width = match self.eval_integer_constant_expression(width_expression) {
            Ok(width) => width.mathematical_value(),
            Err(()) => {
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

    fn bit_field_base_width(&self, ty: TypeId) -> Option<u16> {
        let canonical = self.types.get(ty)?;
        match &canonical.kind {
            TypeKind::Int(IntRank::Bool) => Some(1),
            TypeKind::Int(IntRank::Char | IntRank::SChar | IntRank::UChar) => Some(8),
            TypeKind::Int(IntRank::Short | IntRank::UShort) => Some(16),
            TypeKind::Int(IntRank::Int | IntRank::UInt) => Some(32),
            TypeKind::Enum(_) => TargetDataLayout::aarch64_unknown_linux_gnu()
                .layout_of(&self.types, ty)
                .ok()
                .and_then(|layout| layout.size_bytes.checked_mul(8))
                .and_then(|bits| u16::try_from(bits).ok()),
            TypeKind::Int(
                IntRank::Long | IntRank::ULong | IntRank::LongLong | IntRank::ULongLong,
            ) => Some(64),
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Struct(_)
            | TypeKind::Union(_) => None,
        }
    }

    fn canonical_integer_type(&self, ty: TypeId) -> Result<CIntegerType, ()> {
        let canonical = self.types.get(ty).ok_or(())?;
        match canonical.kind {
            TypeKind::Int(rank) => Ok(integer_type_for_rank(rank)),
            TypeKind::Enum(tag) => {
                let definition = self.types.tag(tag).ok_or(())?;
                let TagState::Complete(TagDefinition::Enum(constants)) = &definition.state else {
                    return Err(());
                };
                let minimum = constants
                    .iter()
                    .map(|constant| constant.value)
                    .min()
                    .unwrap_or(0);
                let maximum = constants
                    .iter()
                    .map(|constant| constant.value)
                    .max()
                    .unwrap_or(0);
                Ok(enum_integer_type(minimum, maximum))
            }
            TypeKind::Void
            | TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Struct(_)
            | TypeKind::Union(_) => Err(()),
        }
    }

    fn intern_c_integer_type(&mut self, ty: CIntegerType) -> TypeId {
        let rank = rank_for_integer_type(ty)
            .unwrap_or_else(|| unreachable!("unsupported target integer type: {ty:?}"));
        self.intern(TypeKind::Int(rank), Qualifiers::default())
    }

    /// Evaluate the C11 integer constant-expression subset needed by enum
    /// initializers and bit-field widths for `aarch64-unknown-linux-gnu`.
    /// Unsupported or undefined operations fail closed and are diagnosed by
    /// the semantic caller as E1303.
    fn eval_integer_constant_expression(&self, expression: &CHirExpr<'_>) -> Result<CInteger, ()> {
        match expression {
            CHirExpr::Lit(literal, _) => integer_literal_value(*literal).map_err(|_| ()),
            CHirExpr::MemoryRead { .. } => Err(()),
            CHirExpr::CharLit(literal, _) => {
                Ok(CInteger::int(i32::try_from(literal.value).map_err(|_| ())?))
            }
            CHirExpr::Ident { symbol, .. } => {
                let symbol = self.symbols.symbol(*symbol).map_err(|_| ())?;
                let SymbolData::Enumerator { value } = symbol.data() else {
                    return Err(());
                };
                Ok(CInteger::new(
                    value as u64,
                    self.canonical_integer_type(symbol.ty())?,
                ))
            }
            CHirExpr::Unary(op, operand, _) => {
                eval_unary_integer(*op, self.eval_integer_constant_expression(operand)?)
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
                eval_binary_integer(*op, left, right).map_err(|_| ())
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
                TargetDataLayout::aarch64_unknown_linux_gnu()
                    .sizeof_type(&self.types, *ty)
                    .map_err(|_| ())?,
                CIntegerType::ULONG,
            )),
            CHirExpr::AlignOf(ty, _) => {
                let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
                    .layout_of(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CInteger::new(
                    u64::from(layout.align_bytes),
                    CIntegerType::ULONG,
                ))
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::CheckedArithmetic { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
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
    fn integer_expression_type(&self, expression: &CHirExpr<'_>) -> Result<CIntegerType, ()> {
        match expression {
            CHirExpr::Lit(literal, _) => Ok(integer_literal_value(*literal).map_err(|_| ())?.ty()),
            CHirExpr::MemoryRead { .. } => Err(()),
            CHirExpr::CharLit(_, _) => Ok(CIntegerType::INT),
            CHirExpr::Ident { symbol, .. } => {
                let symbol = self.symbols.symbol(*symbol).map_err(|_| ())?;
                if symbol.kind() != SymbolKind::Enumerator {
                    return Err(());
                }
                self.canonical_integer_type(symbol.ty())
            }
            CHirExpr::Unary(op, operand, _) => match op {
                UnaryOp::Plus | UnaryOp::Neg | UnaryOp::BitNot => {
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
                TargetDataLayout::aarch64_unknown_linux_gnu()
                    .sizeof_type(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::AlignOf(ty, _) => {
                TargetDataLayout::aarch64_unknown_linux_gnu()
                    .layout_of(&self.types, *ty)
                    .map_err(|_| ())?;
                Ok(CIntegerType::ULONG)
            }
            CHirExpr::FloatLit(_, _)
            | CHirExpr::StringLit(_, _)
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::Call(_, _, _)
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
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

    fn parse_enum_members(&mut self) -> Option<(TagMembers<'arena>, TagDefinition)> {
        let mut members = Vec::new();
        let mut canonical = Vec::new();
        let mut next_value = Some(0_i128);
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
                    .map(CInteger::mathematical_value)
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
            let Ok(value) = i64::try_from(mathematical_value) else {
                self.report(
                    "E1303",
                    "enumerator-value-is-not-representable",
                    written_value.map_or(name.span, CHirExpr::span),
                );
                while !matches!(self.peek_kind(), Some(TokenKind::RBrace) | None) {
                    self.advance();
                }
                break;
            };
            let end = written_value.map_or(name.span, CHirExpr::span);
            let span = self.merge_span(name.span, end);
            canonical.push(EnumConstant {
                name: name.spelling.clone(),
                value,
            });
            let expression_ty = self.intern_c_integer_type(enum_integer_type(value, value));
            let symbol =
                self.declare_symbol(&name, SymbolData::Enumerator { value }, expression_ty);
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
        Some((TagMembers::Enum(members), TagDefinition::Enum(canonical)))
    }

    fn parse_type_name(&mut self) -> Option<(TypeId, Option<PendingTag<'arena>>)> {
        self.parse_type_name_with_vla_error(None)
    }

    fn parse_type_name_with_vla_error(
        &mut self,
        vla_error_span: Option<SourceSpan>,
    ) -> Option<(TypeId, Option<PendingTag<'arena>>)> {
        let specifier = self.parse_type_specifier()?;
        let derived = if matches!(
            self.peek_kind(),
            Some(TokenKind::Star | TokenKind::LBracket | TokenKind::LParen)
        ) {
            self.parse_abstract_declarator_with_vla_error(vla_error_span)?
                .derived
        } else {
            Vec::new()
        };
        let mut type_attributes = self.parse_attribute_specifiers(AttributePlacement::TypeName);
        let ty = self.apply_declarator(specifier.surface.type_id(), &derived);
        let ty = self.apply_attributes_to_type(ty, &mut type_attributes);
        self.detached_attributes.extend(type_attributes);
        Some((ty, specifier.tag_event))
    }

    // ── declarators ────────────────────────────────────────────────────

    fn classify_array_bound(&mut self, expression: &CHirExpr<'_>) -> ArrayBound {
        match self.eval_integer_constant_expression(expression) {
            Ok(value) if value.mathematical_value() < 0 => {
                self.report("E2923", "invalid-array-bound", expression.span());
                // Preserve a concrete, non-VLA layer after the diagnostic so
                // the declaration context cannot replace E2923 with E2203.
                ArrayBound::Fixed(0)
            }
            Ok(value) => ArrayBound::Fixed(
                u64::try_from(value.mathematical_value())
                    .expect("positive C integer values fit in u64"),
            ),
            Err(()) => ArrayBound::Variable,
        }
    }

    fn validate_variable_array_bounds(&mut self, derived: &[DerivedDeclarator]) {
        if let Some(span) = derived.iter().find_map(|layer| match layer {
            DerivedDeclarator::Array {
                bound: ArrayBound::Variable,
                span,
            } => Some(*span),
            _ => None,
        }) {
            self.report("E2203", "variable-length-object", span);
        }
    }

    fn parse_abstract_declarator(&mut self) -> Option<ParsedAbstractDeclarator> {
        self.parse_abstract_declarator_with_vla_error(None)
    }

    fn parse_abstract_declarator_with_vla_error(
        &mut self,
        vla_error_span: Option<SourceSpan>,
    ) -> Option<ParsedAbstractDeclarator> {
        let mut pointers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            let star = self.advance().expect("peeked star");
            let qualifiers = self.parse_qualifiers();
            let attributes = self.parse_attribute_specifiers(AttributePlacement::PointerDeclarator);
            self.validate_pointer_attributes(&attributes);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(star.span, |token| token.span);
            pointers.push(DerivedDeclarator::Pointer {
                qualifiers,
                attributes,
                span: self.merge_span(star.span, end),
            });
        }

        let grouped = self.peek_kind() == Some(TokenKind::LParen)
            && matches!(
                self.tokens.get(self.pos + 1).map(|token| token.kind),
                Some(TokenKind::Star | TokenKind::LParen | TokenKind::LBracket)
            );
        let mut declarator = if grouped {
            let open = self.advance().expect("peeked abstract grouping");
            let mut inner = self.parse_abstract_declarator_with_vla_error(vla_error_span)?;
            let close = self.expect(TokenKind::RParen)?;
            inner.span = self.merge_span(open.span, close.span);
            inner
        } else {
            let span = pointers
                .first()
                .map(derived_declarator_span)
                .unwrap_or_else(|| self.span());
            ParsedAbstractDeclarator {
                derived: Vec::new(),
                span,
            }
        };

        loop {
            match self.peek_kind() {
                Some(TokenKind::LBracket) => {
                    let open = self.advance().expect("peeked abstract array");
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Function { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let bound = if self.peek_kind() == Some(TokenKind::RBracket) {
                        ArrayBound::Unbounded
                    } else {
                        let expression = self.parse_assignment()?;
                        let bound = self.classify_array_bound(expression);
                        if matches!(bound, ArrayBound::Variable) {
                            if let Some(span) = vla_error_span {
                                self.report("E2904", "not-covered: variably-modified-typeof", span);
                            } else {
                                self.report(
                                    "E1005",
                                    "unsupported-abstract-declarator",
                                    expression.span(),
                                );
                            }
                        }
                        bound
                    };
                    let close = self.expect(TokenKind::RBracket)?;
                    let layer_span = self.merge_span(open.span, close.span);
                    declarator.derived.push(DerivedDeclarator::Array {
                        bound,
                        span: layer_span,
                    });
                    declarator.span = self.merge_span(declarator.span, layer_span);
                }
                Some(TokenKind::LParen) => {
                    let open = self.advance().expect("peeked abstract function");
                    if matches!(
                        declarator.derived.last(),
                        Some(DerivedDeclarator::Array { .. })
                    ) {
                        self.report("E1005", "invalid-declarator", open.span);
                    }
                    let outer_scope = self.current_scope;
                    let parameter_scope =
                        self.create_scope(ScopeKind::FunctionPrototype, Some(open.span));
                    self.current_scope = parameter_scope;
                    let parameters = self.parse_parameter_list();
                    self.current_scope = outer_scope;
                    let close = self.expect(TokenKind::RParen)?;
                    let layer_span = self.merge_span(open.span, close.span);
                    declarator.derived.push(DerivedDeclarator::Function {
                        parameters,
                        scope: parameter_scope,
                        span: layer_span,
                    });
                    declarator.span = self.merge_span(declarator.span, layer_span);
                }
                _ => break,
            }
        }

        if let Some(pointer) = pointers.first() {
            declarator.span = self.merge_span(derived_declarator_span(pointer), declarator.span);
        }
        declarator.derived.extend(pointers);
        if declarator.derived.is_empty() {
            self.report("E1005", "unsupported-abstract-declarator", self.span());
            None
        } else {
            Some(declarator)
        }
    }

    fn parse_declarator(&mut self, function_definition_possible: bool) -> Option<ParsedDeclarator> {
        let mut pointers = Vec::new();
        while self.peek_kind() == Some(TokenKind::Star) {
            let star = self.advance().expect("peeked star");
            let qualifiers = self.parse_qualifiers();
            let attributes = self.parse_attribute_specifiers(AttributePlacement::PointerDeclarator);
            self.validate_pointer_attributes(&attributes);
            let end = self
                .tokens
                .get(self.pos.saturating_sub(1))
                .map_or(star.span, |t| t.span);
            pointers.push(DerivedDeclarator::Pointer {
                qualifiers,
                attributes,
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
            let mut inner = self.parse_declarator(function_definition_possible)?;
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
                    } else {
                        let expression = self.parse_assignment()?;
                        self.classify_array_bound(expression)
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
                    let parameter_scope_kind = if function_definition_possible {
                        self.upcoming_function_scope_kind()
                    } else {
                        ScopeKind::FunctionPrototype
                    };
                    let outer_scope = self.current_scope;
                    let parameter_scope = self.create_scope(parameter_scope_kind, Some(open.span));
                    self.current_scope = parameter_scope;
                    let parameters = self.parse_parameter_list();
                    self.current_scope = outer_scope;
                    let close = self.expect(TokenKind::RParen)?;
                    declarator.derived.push(DerivedDeclarator::Function {
                        parameters,
                        scope: parameter_scope,
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

    fn upcoming_function_scope_kind(&self) -> ScopeKind {
        let mut depth = 1_usize;
        let mut index = self.pos;
        while let Some(token) = self.tokens.get(index) {
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        index += 1;
                        // Parenthesized declarators may contribute closing
                        // parentheses between the parameter list and body.
                        while self.tokens.get(index).map(|token| token.kind)
                            == Some(TokenKind::RParen)
                        {
                            index += 1;
                        }
                        return if self.tokens.get(index).map(|token| token.kind)
                            == Some(TokenKind::LBrace)
                        {
                            ScopeKind::Function
                        } else {
                            ScopeKind::FunctionPrototype
                        };
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            index += 1;
        }
        ScopeKind::FunctionPrototype
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
        let shadow_checkpoint = self.temporary_parameter_shadows.len();
        loop {
            if self.match_kind(TokenKind::Ellipsis) {
                variadic = true;
                break;
            }
            while self.peek_kind() == Some(TokenKind::Inline) {
                let token = self.advance().expect("peeked inline specifier");
                self.report(
                    "E1401",
                    "inline-specifier-on-parameter-declaration",
                    token.span,
                );
            }
            // Attributes among a parameter's specifiers, before or after the
            // type, sit in the declaration-specifiers position.
            let mut specifier_attributes =
                self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers);
            let Some(specifier) = self.parse_type_specifier() else {
                self.report("E1001", "expected-declaration", self.span());
                break;
            };
            while self.peek_kind() == Some(TokenKind::Inline) {
                let token = self.advance().expect("peeked inline specifier");
                self.report(
                    "E1401",
                    "inline-specifier-on-parameter-declaration",
                    token.span,
                );
            }
            specifier_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::DeclarationSpecifiers));
            let start = specifier.surface.span();
            let (name, derived, written, end) =
                if matches!(self.peek_kind(), Some(TokenKind::Comma | TokenKind::RParen)) {
                    (
                        None,
                        Vec::new(),
                        WrittenDeclarator {
                            layers: Vec::new(),
                            span: start,
                        },
                        start,
                    )
                } else if self.parameter_declarator_prefers_abstract() {
                    match self.parse_abstract_declarator() {
                        Some(declarator) => {
                            let written = self.written_abstract_declarator(&declarator);
                            (None, declarator.derived, written, declarator.span)
                        }
                        None => (
                            None,
                            Vec::new(),
                            WrittenDeclarator {
                                layers: Vec::new(),
                                span: start,
                            },
                            start,
                        ),
                    }
                } else {
                    let declarator_start = self.pos;
                    let error_checkpoint = self.error_count;
                    match self.parse_declarator(false) {
                        Some(declarator) => {
                            let written = self.written_declarator(&declarator);
                            (
                                Some(declarator.name),
                                declarator.derived,
                                written,
                                declarator.span,
                            )
                        }
                        None if self.error_count == error_checkpoint => {
                            // A name-less declarator begins with the same pointer
                            // and parenthesis tokens as a named declarator. Retry
                            // from the exact token checkpoint with the bounded
                            // abstract grammar instead of silently dropping the
                            // consumed layers.
                            self.pos = declarator_start;
                            if let Some(declarator) = self.parse_abstract_declarator() {
                                let written = self.written_abstract_declarator(&declarator);
                                (None, declarator.derived, written, declarator.span)
                            } else {
                                (
                                    None,
                                    Vec::new(),
                                    WrittenDeclarator {
                                        layers: Vec::new(),
                                        span: start,
                                    },
                                    start,
                                )
                            }
                        }
                        None => (
                            None,
                            Vec::new(),
                            WrittenDeclarator {
                                layers: Vec::new(),
                                span: start,
                            },
                            start,
                        ),
                    }
                };
            self.validate_variable_array_bounds(&derived);
            let unadjusted = self.apply_declarator(specifier.surface.type_id(), &derived);
            let ty = self.adjust_parameter_type(unadjusted);
            let span = self.merge_span(start, end);
            if let Some(event) = specifier.tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            let declared_name = name.as_ref().map(|name| (name.spelling.clone(), name.span));
            let mut parameter_attributes = specifier_attributes;
            parameter_attributes
                .extend(self.parse_attribute_specifiers(AttributePlacement::Parameter));
            let ty = self.apply_attributes_to_type(ty, &mut parameter_attributes);
            params.push(ParameterDecl {
                attributes: parameter_attributes,
                symbol: None,
                name,
                ty,
                specifier: specifier.surface,
                written,
                span,
            });
            if let Some((spelling, span)) = declared_name {
                self.temporary_parameter_shadows
                    .push(TemporaryParameterShadow {
                        scope: self.current_scope,
                        spelling,
                        ty,
                        span,
                    });
            }
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        self.temporary_parameter_shadows.truncate(shadow_checkpoint);
        ParameterSyntax::Prototype {
            parameters: params,
            variadic,
        }
    }

    fn parameter_declarator_prefers_abstract(&self) -> bool {
        let mut index = self.pos;
        if self.tokens.get(index).map(|token| token.kind) != Some(TokenKind::LParen) {
            return false;
        }
        while self.tokens.get(index).map(|token| token.kind) == Some(TokenKind::LParen) {
            index += 1;
        }
        self.tokens
            .get(index)
            .is_some_and(|token| self.typedef_name(token).is_some())
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
        self.written_derived(&declarator.derived, declarator.span)
    }

    fn written_abstract_declarator(
        &self,
        declarator: &ParsedAbstractDeclarator,
    ) -> WrittenDeclarator {
        self.written_derived(&declarator.derived, declarator.span)
    }

    fn written_derived(
        &self,
        derived: &[DerivedDeclarator],
        span: SourceSpan,
    ) -> WrittenDeclarator {
        WrittenDeclarator {
            layers: derived
                .iter()
                .map(|layer| match layer {
                    DerivedDeclarator::Pointer {
                        qualifiers,
                        attributes,
                        span,
                    } => WrittenDeclaratorLayer::Pointer {
                        qualifiers: *qualifiers,
                        attributes: attributes.clone(),
                        span: *span,
                    },
                    DerivedDeclarator::Array { bound, span } => WrittenDeclaratorLayer::Array {
                        bound: bound.clone(),
                        span: *span,
                    },
                    DerivedDeclarator::Function { span, .. } => {
                        WrittenDeclaratorLayer::Function { span: *span }
                    }
                })
                .collect(),
            span,
        }
    }

    fn parse_optional_asm_label(&mut self) -> Option<(Option<String>, Option<SourceSpan>)> {
        if self.peek_kind() != Some(TokenKind::Asm) {
            return Some((None, None));
        }
        let keyword = self.advance().expect("peeked asm declarator label");
        self.expect(TokenKind::LParen)?;
        let (label, _) = self.parse_asm_string()?;
        let close = self.expect(TokenKind::RParen)?;
        Some((Some(label), Some(self.merge_span(keyword.span, close.span))))
    }

    fn parse_asm_construct(&mut self) -> Option<AsmStmt<'arena>> {
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

    fn parse_asm_operands(&mut self, outputs: bool) -> Option<Vec<AsmOperand<'arena>>> {
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
            if outputs && !self.is_modifiable_asm_output(expression) {
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

    fn parse_asm_string(&mut self) -> Option<(String, SourceSpan)> {
        let first = self.expect(TokenKind::StringLiteral)?;
        let Some(mut literal) = parse_string_literal(self.token_text(&first)) else {
            self.report("E1006", "invalid-string-literal", first.span);
            return None;
        };
        let mut end = first.span;
        while self.peek_kind() == Some(TokenKind::StringLiteral) {
            let next = self.advance().expect("peeked adjacent asm string literal");
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
        if literal.explicit_unit_count().is_none() {
            self.report("E1006", "invalid-string-literal", span);
            return None;
        }
        Some((literal.value, span))
    }

    fn is_modifiable_asm_output(&self, expression: &CHirExpr<'_>) -> bool {
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
        self.types.get(ty).is_some_and(|canonical| {
            !canonical.qualifiers.is_const
                && !matches!(
                    canonical.kind,
                    TypeKind::Array { .. } | TypeKind::Function { .. }
                )
        })
    }

    // ── statements ─────────────────────────────────────────────────────

    fn parse_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        match self.peek_kind()? {
            TokenKind::Attribute => {
                let start = self.span();
                let attributes = self.parse_attribute_specifiers(AttributePlacement::NullStatement);
                if self.peek_kind() != Some(TokenKind::Semicolon) {
                    self.report("E2924", "attribute-on-non-null-statement", start);
                    return None;
                }
                let semicolon = self.advance().expect("peeked semicolon");
                let span = self.merge_span(start, semicolon.span);
                Some(self.alloc_node(CHirStmt::AttributedNull(attributes, span)))
            }
            TokenKind::Asm => {
                let mut assembly = self.parse_asm_construct()?;
                let semicolon = self.expect(TokenKind::Semicolon)?;
                assembly.span = self.merge_span(assembly.span, semicolon.span);
                Some(self.alloc_node(CHirStmt::Asm(assembly)))
            }
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
            TokenKind::LBrace => Some(self.parse_compound_statement(true)),
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
        let selector_type = self
            .converted_expression_type(expression)
            .and_then(|inferred| {
                let OperandTypeClass::Integer(ty) = self.operand_type_class(inferred.ty) else {
                    return None;
                };
                Some(CInteger::new(0, ty).promoted().ty())
            });
        self.switch_cases.push(SwitchCaseContext {
            selector_type,
            intervals: Vec::new(),
        });
        self.switch_depth += 1;
        let body = self.parse_statement();
        self.switch_depth -= 1;
        self.switch_cases
            .pop()
            .expect("every active switch owns one case context");
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
        let outer_scope = self.current_scope;
        self.current_scope = self.create_scope(ScopeKind::For, Some(keyword.span));
        let result = self.parse_for_statement_in_scope(keyword);
        self.current_scope = outer_scope;
        result
    }

    fn parse_for_statement_in_scope(&mut self, keyword: Token) -> Option<&'arena CHirStmt<'arena>> {
        self.expect(TokenKind::LParen)?;

        // `__extension__` in a for initializer is outside the frozen Day 5
        // declaration-prefix consensus and must not be silently reinterpreted
        // as one here.
        let initializer = if self.match_kind(TokenKind::Semicolon) {
            CHirForInit::Empty
        } else if self.peek_kind() != Some(TokenKind::GnuExtension) && self.is_declaration_start() {
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
        if self.match_kind(TokenKind::Star) {
            let target = self.parse_expression()?;
            let semicolon = self.expect(TokenKind::Semicolon)?;
            let span = self.merge_span(keyword.span, semicolon.span);
            return Some(self.alloc_node(CHirStmt::ComputedGoto(target, span)));
        }
        let token = self.expect(TokenKind::Identifier)?;
        let spelling = self.token_text(&token).to_owned();
        let label = SpannedName {
            spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
            span: token.span,
        };
        let semicolon = self.expect(TokenKind::Semicolon)?;
        let span = self.merge_span(keyword.span, semicolon.span);
        Some(self.alloc_node(CHirStmt::Goto(label, span)))
    }

    fn parse_label_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let token = self.advance().expect("peeked label");
        let spelling = self.token_text(&token).to_owned();
        let label = SpannedName {
            spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
            span: token.span,
        };
        self.expect(TokenKind::Colon)?;
        // `label: __attribute__((unused));` -- the attribute has no node to
        // live on, so it is preserved on the unit with its placement.
        let label_attributes = self.parse_attribute_specifiers(AttributePlacement::Label);
        self.detached_attributes.extend(label_attributes);
        let nested = self.parse_statement()?;
        let span = self.merge_span(token.span, nested.span());
        Some(self.alloc_node(CHirStmt::Label(label, nested, span)))
    }

    fn parse_case_statement(&mut self) -> Option<&'arena CHirStmt<'arena>> {
        let keyword = self.advance().expect("peeked case");
        if self.switch_depth == 0 {
            self.report("E1203", "case-outside-switch", keyword.span);
        }
        let first = self.parse_conditional()?;
        let last = if self.match_kind(TokenKind::Ellipsis) {
            Some(self.parse_conditional()?)
        } else {
            None
        };
        let endpoint = last.unwrap_or(first);
        let interval_span = self.merge_span(first.span(), endpoint.span());
        self.register_switch_interval(first, endpoint, interval_span, last.is_some())?;
        self.expect(TokenKind::Colon)?;
        let nested = self.parse_statement()?;
        let span = self.merge_span(keyword.span, nested.span());
        Some(match last {
            Some(last) => self.alloc_node(CHirStmt::CaseRange(first, last, nested, span)),
            None => self.alloc_node(CHirStmt::Case(first, nested, span)),
        })
    }

    fn register_switch_interval(
        &mut self,
        first: &CHirExpr<'_>,
        last: &CHirExpr<'_>,
        span: SourceSpan,
        require_constant: bool,
    ) -> Option<()> {
        let selector_type = self
            .switch_cases
            .last()
            .and_then(|context| context.selector_type);
        let converted = selector_type.and_then(|ty| {
            let first = self.eval_integer_constant_expression(first).ok()?.cast(ty);
            let last = self.eval_integer_constant_expression(last).ok()?.cast(ty);
            Some((first.mathematical_value(), last.mathematical_value()))
        });
        let Some((first, last)) = converted else {
            if require_constant {
                self.report("E2916", "case-range-invalid", span);
                return None;
            }
            return Some(());
        };
        if first > last {
            self.report("E2916", "case-range-invalid", span);
            return None;
        }
        let overlaps = self.switch_cases.last().is_some_and(|context| {
            context
                .intervals
                .iter()
                .any(|(other_first, other_last)| first <= *other_last && *other_first <= last)
        });
        if overlaps {
            self.report("E2916", "case-range-invalid", span);
            return None;
        }
        self.switch_cases
            .last_mut()
            .expect("case labels inside a switch have one active context")
            .intervals
            .push((first, last));
        Some(())
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

    fn parse_compound_statement(&mut self, create_block_scope: bool) -> &'arena CHirStmt<'arena> {
        let lbrace = self.expect(TokenKind::LBrace).unwrap();
        let start_span = lbrace.span;
        let outer_scope = self.current_scope;
        if create_block_scope {
            self.current_scope = self.create_scope(ScopeKind::Block, Some(lbrace.span));
        }
        self.local_label_scopes.push(HashMap::new());
        let mut items = Vec::new();
        let mut at_block_start = true;
        while self.peek_kind() != Some(TokenKind::RBrace) && !self.is_done() {
            let before = self.pos;
            if at_block_start && self.peek_kind() == Some(TokenKind::LocalLabel) {
                self.parse_local_label_declaration();
            } else if self.peek_kind() == Some(TokenKind::StaticAssert) {
                at_block_start = false;
                self.parse_static_assert_declaration();
            } else if self.peek_kind() == Some(TokenKind::Attribute)
                && !self.attribute_sequence_starts_declaration()
            {
                at_block_start = false;
                if let Some(statement) = self.parse_statement() {
                    items.push(CHirBlockItem::Statement(statement));
                }
            } else if self.is_declaration_start() || self.peek_kind() == Some(TokenKind::Inline) {
                at_block_start = false;
                if let Some(declarations) = self.parse_declaration(DeclarationContext::Block) {
                    items.extend(declarations.into_iter().map(CHirBlockItem::Declaration));
                }
            } else {
                at_block_start = false;
                if let Some(statement) = self.parse_statement() {
                    items.push(CHirBlockItem::Statement(statement));
                }
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
        self.local_label_scopes
            .pop()
            .expect("every compound statement owns one local-label scope");
        if create_block_scope {
            self.current_scope = outer_scope;
        }
        self.alloc_node(CHirStmt::Compound(items, span))
    }

    fn parse_local_label_declaration(&mut self) -> Option<()> {
        self.advance().expect("peeked __label__");
        let token = self.expect(TokenKind::Identifier)?;
        let spelling = self.token_text(&token).to_owned();
        let internal = format!("{spelling}.__local_label.{}", self.next_local_label);
        let Some(next_local_label) = self.next_local_label.checked_add(1) else {
            self.report("E1001", "expected-declaration", token.span);
            return None;
        };
        self.next_local_label = next_local_label;
        self.local_label_scopes
            .last_mut()
            .expect("local-label declarations occur inside a compound statement")
            .insert(spelling, internal);
        self.expect(TokenKind::Semicolon)?;
        Some(())
    }

    fn resolve_local_label(&self, spelling: &str) -> Option<String> {
        self.local_label_scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(spelling).cloned())
    }

    fn is_declaration_start(&self) -> bool {
        let Some(token) = self.peek() else {
            return false;
        };
        // Labels occupy their own namespace. Even when the same spelling is
        // currently a typedef name, `identifier:` is unambiguously a labeled
        // statement rather than the start of a declaration.
        if token.kind == TokenKind::Identifier
            && self.tokens.get(self.pos + 1).map(|token| token.kind) == Some(TokenKind::Colon)
        {
            return false;
        }
        matches!(
            token.kind,
            TokenKind::Attribute
                | TokenKind::Auto
                | TokenKind::Extern
                | TokenKind::GnuExtension
                | TokenKind::AutoType
                | TokenKind::Register
                | TokenKind::Static
                | TokenKind::ThreadLocal
                | TokenKind::Typedef
                | TokenKind::Const
                | TokenKind::Volatile
                | TokenKind::Restrict
                | TokenKind::Atomic
        ) || self.token_starts_type_name(token)
    }

    fn attribute_sequence_starts_declaration(&self) -> bool {
        let mut position = self.pos;
        while self.tokens.get(position).map(|token| token.kind) == Some(TokenKind::Attribute) {
            position += 1;
            if self.tokens.get(position).map(|token| token.kind) != Some(TokenKind::LParen)
                || self.tokens.get(position + 1).map(|token| token.kind) != Some(TokenKind::LParen)
            {
                return false;
            }
            let mut depth = 0_u32;
            while let Some(token) = self.tokens.get(position) {
                match token.kind {
                    TokenKind::LParen => depth += 1,
                    TokenKind::RParen => {
                        if depth == 0 {
                            return false;
                        }
                        depth -= 1;
                        if depth == 0 {
                            position += 1;
                            break;
                        }
                    }
                    TokenKind::Eof => return false,
                    _ => {}
                }
                position += 1;
            }
            if depth != 0 {
                return false;
            }
        }
        let Some(token) = self.tokens.get(position) else {
            return false;
        };
        matches!(token.kind, TokenKind::Inline)
            || matches!(
                token.kind,
                TokenKind::Auto
                    | TokenKind::Extern
                    | TokenKind::GnuExtension
                    | TokenKind::AutoType
                    | TokenKind::Register
                    | TokenKind::Static
                    | TokenKind::ThreadLocal
                    | TokenKind::Typedef
                    | TokenKind::Const
                    | TokenKind::Volatile
                    | TokenKind::Restrict
                    | TokenKind::Atomic
            )
            || self.token_starts_type_name(token)
    }

    fn peek_typedef_name(&self) -> Option<(SymbolId, TypeId)> {
        self.typedef_name(self.peek()?)
    }

    fn typedef_name(&self, token: &Token) -> Option<(SymbolId, TypeId)> {
        if token.kind != TokenKind::Identifier {
            return None;
        }
        let spelling = self.token_text(token);
        if self.temporary_parameter_shadow_is_visible(spelling) {
            return None;
        }
        let symbol = self
            .symbols
            .resolve(self.current_scope, spelling)
            .expect("the parser current scope is always live")?;
        let declaration = self
            .symbols
            .symbol(symbol)
            .expect("resolved symbol belongs to the same table");
        (declaration.kind() == SymbolKind::Typedef).then_some((symbol, declaration.ty()))
    }

    fn temporary_parameter_shadow_is_visible(&self, spelling: &str) -> bool {
        let mut scope = Some(self.current_scope);
        while let Some(current) = scope {
            if self
                .temporary_parameter_shadows
                .iter()
                .rev()
                .any(|shadow| shadow.scope == current && shadow.spelling == spelling)
            {
                return true;
            }
            scope = self
                .symbols
                .scope(current)
                .expect("the parser current scope ancestry is always live")
                .parent();
        }
        false
    }

    fn token_starts_type_name(&self, token: &Token) -> bool {
        matches!(
            token.kind,
            TokenKind::Const | TokenKind::Volatile | TokenKind::Restrict | TokenKind::Atomic
        ) || self.is_type_specifier_kind(token.kind)
            || self.typedef_name(token).is_some()
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
                | TokenKind::Complex
                | TokenKind::Bool
                | TokenKind::Struct
                | TokenKind::Union
                | TokenKind::Enum
                | TokenKind::BuiltinVaList
                | TokenKind::GnuInt128
                | TokenKind::Typeof
                | TokenKind::TypeofUnqual
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
        let target = self.parse_conditional()?;
        let Some(op) = self.peek_kind().and_then(assignment_op) else {
            return Some(target);
        };
        self.advance();
        // Diagnose the target first, but still consume the complete
        // right-associative RHS so recovery resumes at the statement boundary.
        let typed_target = self.typed_update_target(target);
        let value = self.parse_assignment()?;
        let typed_target = typed_target?;
        let span = self.merge_span(target.span(), value.span());
        let constraint = self.assignment_constraint(op, typed_target.ty(), value);
        match constraint {
            UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
            UpdateConstraint::AggregateBoundary => {}
            UpdateConstraint::Invalid => {
                self.report("E2102", "invalid-update-operands", span);
                return None;
            }
        }
        if constraint == UpdateConstraint::AggregateBoundary
            && self.volatile_aggregate_copy_is_unsupported(typed_target.ty(), value)
        {
            self.report("E2922", "volatile-aggregate-copy-unsupported", span);
            return None;
        }
        Some(match typed_target {
            TypedUpdateTarget::Direct(target_identity) => {
                let place = self.finish_scalar_update_place(target_identity, target.span())?;
                let facts = ExprFacts::value(place.ty());
                self.alloc_node(CHirExpr::Assign {
                    op,
                    place,
                    target,
                    value,
                    facts,
                    span,
                })
            }
            TypedUpdateTarget::Memory(place) => {
                let facts = ExprFacts::value(place.value_ty());
                self.alloc_node(CHirExpr::MemoryAssign {
                    op,
                    place,
                    target_span: target.span(),
                    value,
                    facts,
                    span,
                })
            }
        })
    }

    fn typed_update_target(
        &mut self,
        target: &'arena CHirExpr<'arena>,
    ) -> Option<TypedUpdateTarget<'arena>> {
        if let CHirExpr::Unary(UnaryOp::Deref, pointer, _) = target {
            return self
                .dereference_place(pointer, target.span())
                .map(TypedUpdateTarget::Memory);
        }
        // A projected scalar lvalue already carries its checked place, so the
        // target is recovered from it rather than derived a second time.
        if let CHirExpr::MemoryRead { place, .. } = target {
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                return None;
            }
            if place.access_qualifiers().is_atomic {
                self.report("E2702", "atomic-update-unsupported", target.span());
                return None;
            }
            return Some(TypedUpdateTarget::Memory(place.clone()));
        }
        let aggregate_target =
            self.expression_type(target)
                .is_some_and(|inferred| match inferred.ty {
                    InferredType::Canonical(ty) => matches!(
                        self.types.get(ty).map(|canonical| &canonical.kind),
                        Some(TypeKind::Struct(_) | TypeKind::Union(_))
                    ),
                    _ => false,
                });
        if aggregate_target {
            let place = self.resolve_memory_place(target).or_else(|| {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                None
            })?;
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target.span());
                return None;
            }
            if place.access_qualifiers().is_atomic {
                self.report("E2702", "atomic-update-unsupported", target.span());
                return None;
            }
            return Some(TypedUpdateTarget::Memory(place));
        }
        if matches!(
            target,
            CHirExpr::Member(_, _, _) | CHirExpr::PtrMember(_, _, _) | CHirExpr::Index(_, _, _)
        ) {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        }

        let CHirExpr::Ident { symbol, facts, .. } = target else {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        };
        if !facts.is_modifiable_lvalue() {
            self.report("E2101", "expected-modifiable-lvalue", target.span());
            return None;
        }
        Some(TypedUpdateTarget::Direct(DirectUpdateTarget {
            symbol: *symbol,
            ty: facts.ty(),
        }))
    }

    /// Resolve a member name to its complete projection path. Anonymous record
    /// members are traversed recursively; two matching anonymous paths are
    /// ambiguous and return `None`. Member access, initializer designators and
    /// `__builtin_offsetof` all use this one lookup rule.
    fn record_member_path(&self, record_ty: TypeId, name: &str) -> Option<Vec<FieldProjection>> {
        self.record_member_path_inner(record_ty, name, &mut HashSet::new())
    }

    fn record_member_path_inner(
        &self,
        record_ty: TypeId,
        name: &str,
        visiting: &mut HashSet<TypeId>,
    ) -> Option<Vec<FieldProjection>> {
        if !visiting.insert(record_ty) {
            return None;
        }
        let canonical = self.types.get(record_ty)?;
        let tag = match canonical.kind {
            TypeKind::Struct(tag) | TypeKind::Union(tag) => tag,
            _ => return None,
        };
        let definition = self.types.tag(tag)?;
        let fields = match &definition.state {
            TagState::Complete(TagDefinition::Struct(fields))
            | TagState::Complete(TagDefinition::Union(fields)) => fields,
            TagState::Incomplete | TagState::Complete(TagDefinition::Enum(_)) => return None,
        };
        if let Some((index, field)) = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name.as_deref() == Some(name))
        {
            visiting.remove(&record_ty);
            return Some(vec![FieldProjection::new(tag, index, field.ty)]);
        }

        let mut found = None;
        for (index, field) in fields
            .iter()
            .enumerate()
            .filter(|(_, field)| field.name.is_none() && field.bit_field.is_none())
        {
            if !matches!(
                self.types.get(field.ty).map(|ty| &ty.kind),
                Some(TypeKind::Struct(_) | TypeKind::Union(_))
            ) {
                continue;
            }
            let Some(mut nested) = self.record_member_path_inner(field.ty, name, visiting) else {
                continue;
            };
            let mut path = vec![FieldProjection::new(tag, index, field.ty)];
            path.append(&mut nested);
            if found.replace(path).is_some() {
                visiting.remove(&record_ty);
                return None;
            }
        }
        visiting.remove(&record_ty);
        found
    }

    /// Split a subscript into its pointer-like and integer operands.
    ///
    /// C11 6.5.2.1p1 requires one operand to have pointer-to-object type and
    /// the other integer type, without fixing which is which, so `2[a]` is as
    /// well formed as `a[2]`.
    fn subscript_operands<'e>(
        &self,
        left: &'e CHirExpr<'arena>,
        right: &'e CHirExpr<'arena>,
    ) -> Option<(&'e CHirExpr<'arena>, &'e CHirExpr<'arena>)> {
        let left_class = self.operand_type_class(self.converted_expression_type(left)?.ty);
        if matches!(left_class, OperandTypeClass::Pointer(_)) {
            return Some((left, right));
        }
        let right_class = self.operand_type_class(self.converted_expression_type(right)?.ty);
        if matches!(right_class, OperandTypeClass::Pointer(_)) {
            return Some((right, left));
        }
        None
    }

    /// The element type a subscript of this base designates.
    fn subscript_element_type(&self, base: &CHirExpr<'arena>) -> Option<TypeId> {
        match self.operand_type_class(self.converted_expression_type(base)?.ty) {
            OperandTypeClass::Pointer(pointer) => Some(pointer.pointee),
            _ => None,
        }
    }

    /// Build the checked place designated by an lvalue expression.
    ///
    /// The walk is outside-in over the written syntax and bottoms out at an
    /// object or at a pointer expression. Returning `None` means the syntax
    /// does not designate a place this slice can represent; the caller reports
    /// the diagnostic, because only the caller knows which construct the
    /// failure belongs to.
    fn resolve_memory_place(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Option<MemoryPlace<'arena>> {
        let (root, projections, value_ty) = self.resolve_place_chain(expression)?;
        MemoryPlace::derive(&self.symbols, &self.types, root, projections, value_ty).ok()
    }

    fn resolve_place_chain(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> Option<(PlaceRoot<'arena>, Vec<PlaceProjection<'arena>>, TypeId)> {
        match expression {
            // Already-resolved subobjects carry their own authority, so a
            // further projection extends that chain instead of re-deriving it
            // from syntax that is deliberately no longer present.
            CHirExpr::MemoryRead { place, .. } => Some((
                place.root().clone(),
                place.projections().to_vec(),
                place.value_ty(),
            )),
            CHirExpr::Ident { symbol, facts, .. } => {
                let declaration = self.symbols.symbol(*symbol).ok()?;
                if !matches!(
                    declaration.kind(),
                    SymbolKind::Object | SymbolKind::Parameter
                ) {
                    return None;
                }
                Some((
                    PlaceRoot::Object { symbol: *symbol },
                    Vec::new(),
                    facts.ty(),
                ))
            }
            CHirExpr::CompoundLit(ty, _, _) => Some((
                PlaceRoot::CompoundLiteral {
                    expression,
                    ty: *ty,
                },
                Vec::new(),
                *ty,
            )),
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                // Taking an address and immediately dereferencing it preserves
                // the original place identity. Keep that checked root and its
                // projections instead of turning `&place` into an opaque
                // pointer-expression root that a later consumer would have to
                // rediscover from syntax.
                if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
                    return self.resolve_place_chain(addressed);
                }
                let pointer_type = self.pointer_root_descriptor(pointer)?;
                Some((
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    },
                    Vec::new(),
                    pointer_type.pointee(),
                ))
            }
            CHirExpr::Member(base, name, _) => {
                let (root, mut projections, base_ty) = self.resolve_place_chain(base)?;
                let fields = self.record_member_path(base_ty, name)?;
                let field_ty = fields.last()?.field_ty();
                projections.extend(fields.into_iter().map(PlaceProjection::Field));
                Some((root, projections, field_ty))
            }
            CHirExpr::PtrMember(pointer, name, _) => {
                if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
                    let (root, mut projections, addressed_ty) =
                        self.resolve_place_chain(addressed)?;
                    let fields = self.record_member_path(addressed_ty, name)?;
                    let field_ty = fields.last()?.field_ty();
                    projections.extend(fields.into_iter().map(PlaceProjection::Field));
                    return Some((root, projections, field_ty));
                }
                let pointer_type = self.pointer_root_descriptor(pointer)?;
                let fields = self.record_member_path(pointer_type.pointee(), name)?;
                let field_ty = fields.last()?.field_ty();
                Some((
                    PlaceRoot::PointerExpression {
                        pointer,
                        pointer_type,
                    },
                    fields.into_iter().map(PlaceProjection::Field).collect(),
                    field_ty,
                ))
            }
            CHirExpr::Index(left, right, _) => {
                let (base, index) = self.subscript_operands(left, right)?;
                let element_ty = self.subscript_element_type(base)?;
                let promoted = self.promoted_subscript_type(index)?;
                let projection =
                    PlaceProjection::Index(IndexProjection::new(index, element_ty, promoted));
                // An array lvalue is subscripted in place; anything else has
                // already decayed to a pointer value and becomes the root.
                if self.designates_array_object(base) {
                    let (root, mut projections, _) = self.resolve_place_chain(base)?;
                    projections.push(projection);
                    Some((root, projections, element_ty))
                } else {
                    let pointer_type = self.pointer_root_descriptor(base)?;
                    Some((
                        PlaceRoot::PointerExpression {
                            pointer: base,
                            pointer_type,
                        },
                        vec![projection],
                        element_ty,
                    ))
                }
            }
            _ => None,
        }
    }

    /// Does this expression designate an array object, rather than hold a
    /// pointer value that already decayed from one?
    fn designates_array_object(&self, expression: &CHirExpr<'arena>) -> bool {
        let Some(inferred) = self.expression_type(expression) else {
            return false;
        };
        let InferredType::Canonical(ty) = inferred.ty else {
            return false;
        };
        matches!(
            self.types.get(ty).map(|canonical| &canonical.kind),
            Some(TypeKind::Array { .. })
        )
    }

    /// The canonical pointer descriptor of an expression used as a place root.
    fn pointer_root_descriptor(&self, expression: &CHirExpr<'arena>) -> Option<PointerTypeDesc> {
        let InferredType::Canonical(ty) = self.converted_expression_type(expression)?.ty else {
            return None;
        };
        PointerTypeDesc::from_canonical_pointer(&self.types, ty).ok()
    }

    /// The subscript's type after integer promotion, which is what a backend
    /// needs to choose a machine width for the scaled offset.
    fn promoted_subscript_type(&self, index: &CHirExpr<'arena>) -> Option<CIntegerType> {
        match self.operand_type_class(self.converted_expression_type(index)?.ty) {
            OperandTypeClass::Integer(integer) => Some(CInteger::new(0, integer).promoted().ty()),
            _ => None,
        }
    }

    /// Convert a projected lvalue to a value where C requires it.
    ///
    /// Only loadable scalars are converted here. An aggregate or array place is
    /// left as written syntax so a following projection can continue the chain;
    /// if such a node instead reaches a consumer, that consumer reports the
    /// aggregate boundary it actually hit.
    fn convert_projected_lvalue(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
    ) -> &'arena CHirExpr<'arena> {
        let span = expression.span();
        // A member selected from an aggregate value (for example
        // `(destination = source).field`) is not rooted in a separately
        // derivable lvalue.  Keep the value-producing base attached so the
        // interpreter and lowering evaluate the copy exactly once before
        // selecting the member.
        if let CHirExpr::Member(base, _, _) = expression {
            if self
                .expression_type(base)
                .is_some_and(|inferred| inferred.category == ExprCategory::Value)
            {
                return expression;
            }
        }
        // A projection that cannot even be typed is ill formed, not merely
        // outside this slice, so it is reported here rather than left to
        // surface as a confusing failure in a later stage.
        let Some(inferred) = self.expression_type(expression) else {
            self.report("E2102", "invalid-projected-lvalue", span);
            return expression;
        };
        let InferredType::Canonical(value_ty) = inferred.ty else {
            return expression;
        };
        if !self.is_loadable_scalar(value_ty) {
            return expression;
        }
        let Some(place) = self.resolve_memory_place(expression) else {
            self.report("E2102", "invalid-projected-lvalue", span);
            return expression;
        };
        let facts = ExprFacts::value(place.value_ty());
        self.alloc_node(CHirExpr::MemoryRead {
            place,
            target_span: span,
            facts,
            span,
        })
    }

    fn is_loadable_scalar(&self, ty: TypeId) -> bool {
        matches!(
            self.types.get(ty).map(|canonical| &canonical.kind),
            Some(TypeKind::Int(_) | TypeKind::Enum(_) | TypeKind::Pointer(_) | TypeKind::Float(_))
        )
    }

    fn dereference_place(
        &mut self,
        pointer: &'arena CHirExpr<'arena>,
        target_span: SourceSpan,
    ) -> Option<MemoryPlace<'arena>> {
        if let CHirExpr::Unary(UnaryOp::AddrOf, addressed, _) = pointer {
            let place = self.resolve_memory_place(addressed).or_else(|| {
                self.report("E2102", "invalid-update-operands", target_span);
                None
            })?;
            if !place.is_modifiable() {
                self.report("E2101", "expected-modifiable-lvalue", target_span);
                return None;
            }
            if place.access_qualifiers().is_atomic {
                self.report("E2702", "atomic-update-unsupported", target_span);
                return None;
            }
            return Some(place);
        }
        let pointer_ty = self
            .canonical_pointer_expression_type(pointer)
            .or_else(|| {
                self.report("E2102", "invalid-update-operands", target_span);
                None
            })?;
        let pointer_type = match PointerTypeDesc::from_canonical_pointer(&self.types, pointer_ty) {
            Ok(pointer_type) => pointer_type,
            Err(_) => {
                self.report("E2102", "invalid-update-operands", target_span);
                return None;
            }
        };
        let place = match MemoryPlace::derive(
            &self.symbols,
            &self.types,
            PlaceRoot::PointerExpression {
                pointer,
                pointer_type,
            },
            Vec::new(),
            pointer_type.pointee(),
        ) {
            Ok(place) => place,
            Err(_) => {
                self.report("E2102", "invalid-update-operands", target_span);
                return None;
            }
        };
        if !place.is_modifiable() {
            self.report("E2101", "expected-modifiable-lvalue", target_span);
            return None;
        }
        if place.access_qualifiers().is_atomic {
            self.report("E2702", "atomic-update-unsupported", target_span);
            return None;
        }
        Some(place)
    }

    /// Return the canonical pointer type of the bounded expression forms used
    /// as indirect-place roots in this slice. Composite/derived pointer values
    /// remain fail-closed until their structural descriptor is attached by the
    /// full memory expression pass.
    fn canonical_pointer_expression_type(&self, expression: &CHirExpr<'_>) -> Option<TypeId> {
        let ty = match expression {
            CHirExpr::Ident { facts, .. }
            | CHirExpr::Assign { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::Update { facts, .. }
            | CHirExpr::MemoryUpdate { facts, .. }
            | CHirExpr::MemoryRead { facts, .. }
            | CHirExpr::StatementExpr { facts, .. } => facts.ty(),
            CHirExpr::Cast(ty, _, _) => *ty,
            CHirExpr::Unary(UnaryOp::Deref, pointer, _) => {
                let pointer_ty = self.canonical_pointer_expression_type(pointer)?;
                let canonical = self.types.get(pointer_ty)?;
                let TypeKind::Pointer(pointee) = canonical.kind else {
                    return None;
                };
                pointee
            }
            _ => return None,
        };
        matches!(
            self.types.get(ty).map(|ty| &ty.kind),
            Some(TypeKind::Pointer(_))
        )
        .then_some(ty)
    }

    fn finish_scalar_update_place(
        &mut self,
        target: DirectUpdateTarget,
        target_span: SourceSpan,
    ) -> Option<ScalarPlace> {
        let canonical = self
            .types
            .get(target.ty)
            .expect("identifier facts only reference interned types");
        if canonical.qualifiers.is_atomic {
            self.report("E2702", "atomic-update-unsupported", target_span);
            return None;
        }
        match ScalarPlace::bind(&self.symbols, &self.types, target.symbol) {
            Ok(place) => Some(place),
            Err(_) => {
                self.report("E2101", "expected-modifiable-lvalue", target_span);
                None
            }
        }
    }

    fn assignment_constraint(
        &self,
        op: AssignmentOp,
        target_ty: TypeId,
        value: &CHirExpr<'_>,
    ) -> UpdateConstraint {
        let left = self.operand_type_class(InferredType::Canonical(target_ty));
        let Some(right) = self.converted_expression_type(value) else {
            return UpdateConstraint::Invalid;
        };
        let right_type = right.ty;
        let right_class = self.operand_type_class(right_type);

        if op == AssignmentOp::Simple {
            if let InferredType::Canonical(right_ty) = right_type {
                if self.same_unqualified_type(target_ty, right_ty)
                    && matches!(
                        self.types.get(target_ty).map(|canonical| &canonical.kind),
                        Some(TypeKind::GnuInt128(_))
                    )
                {
                    return UpdateConstraint::Scalar;
                }
            }
        }

        match op {
            AssignmentOp::Simple => match (left, right_class) {
                (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Integer(ty), OperandTypeClass::Pointer(_))
                    if ty == CIntegerType::BOOL =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Integer(ty), OperandTypeClass::StringPointer(_))
                    if ty == CIntegerType::BOOL =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right)) => {
                    if self.pointer_assignment_compatible(left, right) {
                        UpdateConstraint::Scalar
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::StringPointer(prefix)) => {
                    if self.pointer_accepts_string(left, prefix) {
                        UpdateConstraint::Scalar
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                (OperandTypeClass::Pointer(_), OperandTypeClass::Integer(_))
                    if right.null_pointer_constant =>
                {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Aggregate(left), OperandTypeClass::Aggregate(right)) => {
                    if self.same_unqualified_type(left, right) {
                        UpdateConstraint::AggregateBoundary
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                _ => UpdateConstraint::Invalid,
            },
            AssignmentOp::Add | AssignmentOp::Subtract => match (left, right_class) {
                (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                    UpdateConstraint::Scalar
                }
                (OperandTypeClass::Pointer(pointee), OperandTypeClass::Integer(_)) => {
                    if self.pointer_arithmetic_is_valid(pointee.pointee) {
                        UpdateConstraint::PointerBoundary
                    } else {
                        UpdateConstraint::Invalid
                    }
                }
                _ => UpdateConstraint::Invalid,
            },
            AssignmentOp::Multiply | AssignmentOp::Divide => {
                if is_arithmetic_class(left) && is_arithmetic_class(right_class) {
                    UpdateConstraint::Scalar
                } else {
                    UpdateConstraint::Invalid
                }
            }
            AssignmentOp::Remainder
            | AssignmentOp::ShiftLeft
            | AssignmentOp::ShiftRight
            | AssignmentOp::BitAnd
            | AssignmentOp::BitOr
            | AssignmentOp::BitXor => {
                if is_integer_class(left) && is_integer_class(right_class) {
                    UpdateConstraint::Scalar
                } else {
                    UpdateConstraint::Invalid
                }
            }
        }
    }

    fn volatile_aggregate_copy_is_unsupported(
        &self,
        target_ty: TypeId,
        value: &CHirExpr<'_>,
    ) -> bool {
        let Some(target) = self.types.get(target_ty) else {
            return false;
        };
        let source_is_volatile = self
            .expression_type(value)
            .and_then(|inferred| match inferred.ty {
                InferredType::Canonical(ty) => self.types.get(ty),
                InferredType::Integer(_)
                | InferredType::Real(_)
                | InferredType::Complex(_)
                | InferredType::Pointer(_)
                | InferredType::StringArray(_)
                | InferredType::StringPointer(_) => None,
            })
            .is_some_and(|source| source.qualifiers.is_volatile);
        if !target.qualifiers.is_volatile && !source_is_volatile {
            return false;
        }
        match &target.kind {
            TypeKind::Union(_) => true,
            TypeKind::Struct(tag) => self.types.tag(*tag).is_some_and(|tag| {
                matches!(
                    &tag.state,
                    TagState::Complete(TagDefinition::Struct(fields))
                        if fields.iter().any(|field| field.bit_field.is_some())
                )
            }),
            TypeKind::Void
            | TypeKind::Int(_)
            | TypeKind::Float(_)
            | TypeKind::Complex(_)
            | TypeKind::Pointer(_)
            | TypeKind::Array { .. }
            | TypeKind::Function { .. }
            | TypeKind::Enum(_)
            | TypeKind::BuiltinVaList
            | TypeKind::GnuInt128(_) => false,
        }
    }

    fn update_constraint(&self, target_ty: TypeId) -> UpdateConstraint {
        match self.operand_type_class(InferredType::Canonical(target_ty)) {
            OperandTypeClass::Integer(_) | OperandTypeClass::Real(_) => UpdateConstraint::Scalar,
            OperandTypeClass::Pointer(pointee) => {
                if self.pointer_arithmetic_is_valid(pointee.pointee) {
                    UpdateConstraint::PointerBoundary
                } else {
                    UpdateConstraint::Invalid
                }
            }
            OperandTypeClass::Complex(_)
            | OperandTypeClass::StringPointer(_)
            | OperandTypeClass::Aggregate(_)
            | OperandTypeClass::Void
            | OperandTypeClass::Other => UpdateConstraint::Invalid,
        }
    }

    fn materialize_inferred_type(
        &mut self,
        inferred: InferredType,
        expression: Option<&CHirExpr<'_>>,
    ) -> Option<TypeId> {
        Some(match inferred {
            InferredType::Canonical(ty) => ty,
            InferredType::Integer(integer) => self.intern_c_integer_type(integer),
            InferredType::Real(real) => self.intern(TypeKind::Float(real), Qualifiers::default()),
            InferredType::Complex(complex) => {
                self.intern(TypeKind::Complex(complex), Qualifiers::default())
            }
            InferredType::Pointer(pointer) => {
                let pointee = self.qualify_typedef_type(pointer.pointee, pointer.qualifiers);
                self.intern(TypeKind::Pointer(pointee), Qualifiers::default())
            }
            InferredType::StringArray(prefix) => {
                let CHirExpr::StringLit(literal, _) = expression? else {
                    return None;
                };
                let element = self.string_element_type(prefix);
                let bound = literal.explicit_unit_count()?.checked_add(1)?;
                self.intern(
                    TypeKind::Array {
                        element,
                        bound: ArrayBound::Fixed(bound),
                    },
                    Qualifiers::default(),
                )
            }
            InferredType::StringPointer(prefix) => {
                let element = self.string_element_type(prefix);
                self.intern(TypeKind::Pointer(element), Qualifiers::default())
            }
        })
    }

    fn generic_controlling_type(&mut self, expression: &CHirExpr<'_>) -> Option<TypeId> {
        let converted = self.converted_expression_type(expression)?;
        let ty = self.materialize_inferred_type(converted.ty, Some(expression))?;
        let canonical = self.types.get(ty)?.clone();
        Some(self.intern(canonical.kind, Qualifiers::default()))
    }

    fn string_element_type(&mut self, prefix: StringPrefix) -> TypeId {
        let rank = match prefix {
            StringPrefix::None | StringPrefix::U8 => IntRank::Char,
            StringPrefix::L => IntRank::Int,
            StringPrefix::U16 => IntRank::UShort,
            StringPrefix::U => IntRank::UInt,
        };
        self.intern(TypeKind::Int(rank), Qualifiers::default())
    }

    fn type_is_variably_modified(&self, ty: TypeId) -> bool {
        self.type_is_variably_modified_from(ty, &mut HashSet::new())
    }

    fn type_is_variably_modified_from(&self, ty: TypeId, seen: &mut HashSet<TypeId>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match self.types.get(ty).map(|canonical| &canonical.kind) {
            Some(TypeKind::Array {
                bound: ArrayBound::Variable,
                ..
            }) => true,
            Some(TypeKind::Array { element, .. } | TypeKind::Pointer(element)) => {
                self.type_is_variably_modified_from(*element, seen)
            }
            Some(TypeKind::Function {
                return_type,
                parameters,
            }) => {
                self.type_is_variably_modified_from(*return_type, seen)
                    || match parameters {
                        FunctionParameters::Unspecified => false,
                        FunctionParameters::Prototype { types, .. } => types
                            .iter()
                            .any(|ty| self.type_is_variably_modified_from(*ty, seen)),
                    }
            }
            _ => false,
        }
    }

    /// Infer the type and value category represented by an existing C-HIR
    /// expression. This is intentionally read-only over the type interner:
    /// literal and usual-arithmetic result types use `InferredType` values so
    /// validating an expression never changes later declaration identities.
    fn expression_type(&self, expression: &CHirExpr<'_>) -> Option<InferredExpressionType> {
        let (ty, category) = match expression {
            CHirExpr::Lit(literal, _) => (
                InferredType::Integer(integer_literal_value(*literal).ok()?.ty()),
                ExprCategory::Value,
            ),
            CHirExpr::FloatLit(literal, _) => (
                InferredType::Real(match literal.suffix {
                    FloatSuffix::F => FloatKind::F32,
                    FloatSuffix::None => FloatKind::F64,
                    FloatSuffix::L => FloatKind::F128,
                }),
                ExprCategory::Value,
            ),
            CHirExpr::CharLit(_, _) => (
                InferredType::Integer(CIntegerType::INT),
                ExprCategory::Value,
            ),
            CHirExpr::Ident { facts, .. } => {
                (InferredType::Canonical(facts.ty()), facts.category())
            }
            // The place is authority for the designated subobject's type, and
            // this node already represents the converted value.
            CHirExpr::MemoryRead { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::Unary(op, operand, _) => {
                let operand = match op {
                    UnaryOp::AddrOf => self.expression_type(operand)?,
                    UnaryOp::Plus | UnaryOp::Neg | UnaryOp::Not | UnaryOp::BitNot => {
                        self.converted_expression_type(operand)?
                    }
                    UnaryOp::Deref => self.converted_expression_type(operand)?,
                };
                match op {
                    UnaryOp::Plus | UnaryOp::Neg => (
                        self.promoted_arithmetic_type(operand.ty)?,
                        ExprCategory::Value,
                    ),
                    UnaryOp::Not => {
                        if !is_scalar_class(self.operand_type_class(operand.ty)) {
                            return None;
                        }
                        (
                            InferredType::Integer(CIntegerType::INT),
                            ExprCategory::Value,
                        )
                    }
                    UnaryOp::BitNot => {
                        let OperandTypeClass::Integer(integer) =
                            self.operand_type_class(operand.ty)
                        else {
                            return None;
                        };
                        (
                            InferredType::Integer(CInteger::new(0, integer).promoted().ty()),
                            ExprCategory::Value,
                        )
                    }
                    UnaryOp::AddrOf => {
                        let InferredType::Canonical(pointee) = operand.ty else {
                            return None;
                        };
                        (
                            InferredType::Pointer(self.inferred_pointer_type(pointee)?),
                            ExprCategory::Value,
                        )
                    }
                    UnaryOp::Deref => {
                        let OperandTypeClass::Pointer(pointee) =
                            self.operand_type_class(operand.ty)
                        else {
                            return None;
                        };
                        let category = if matches!(
                            self.types.get(pointee.pointee).map(|ty| &ty.kind),
                            Some(TypeKind::Function { .. })
                        ) {
                            ExprCategory::FunctionDesignator
                        } else {
                            ExprCategory::LValue
                        };
                        (InferredType::Canonical(pointee.pointee), category)
                    }
                }
            }
            CHirExpr::EvaluateOnce(operand, _) => {
                let inner = self.expression_type(operand)?;
                (inner.ty, inner.category)
            }
            CHirExpr::Binary(op, left, right, _) => {
                let left = self.converted_expression_type(left)?;
                let right = self.converted_expression_type(right)?;
                let ty = self.binary_expression_type(*op, left, right)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Ternary(condition, when_true, when_false, _) => {
                let condition = self.converted_expression_type(condition)?;
                if !is_scalar_class(self.operand_type_class(condition.ty)) {
                    return None;
                }
                let when_true = self.converted_expression_type(when_true)?;
                let when_false = self.converted_expression_type(when_false)?;
                let ty = self.conditional_expression_type(when_true, when_false)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Elvis(operand, fallback, _) => {
                let operand = self.converted_expression_type(operand)?;
                if !is_scalar_class(self.operand_type_class(operand.ty)) {
                    return None;
                }
                let fallback = self.converted_expression_type(fallback)?;
                let ty = self.conditional_expression_type(operand, fallback)?;
                (ty, ExprCategory::Value)
            }
            CHirExpr::Call(callee, _, _) => {
                let callee = self.converted_expression_type(callee)?;
                let OperandTypeClass::Pointer(function) = self.operand_type_class(callee.ty) else {
                    return None;
                };
                let TypeKind::Function { return_type, .. } =
                    self.types.get(function.pointee).map(|ty| &ty.kind)?
                else {
                    return None;
                };
                (InferredType::Canonical(*return_type), ExprCategory::Value)
            }
            CHirExpr::CheckedArithmetic { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::BitBuiltin { facts, .. }
            | CHirExpr::BuiltinUnreachable { facts, .. }
            | CHirExpr::BuiltinReturnAddress { facts, .. }
            | CHirExpr::LabelAddress { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::Cast(ty, _, _) => (InferredType::Canonical(*ty), ExprCategory::Value),
            CHirExpr::SizeOf(_, _) | CHirExpr::SizeOfExpr(_, _, _) | CHirExpr::AlignOf(_, _) => (
                InferredType::Integer(CIntegerType::ULONG),
                ExprCategory::Value,
            ),
            CHirExpr::Assign { facts, .. }
            | CHirExpr::MemoryAssign { facts, .. }
            | CHirExpr::Update { facts, .. }
            | CHirExpr::MemoryUpdate { facts, .. } => {
                (InferredType::Canonical(facts.ty()), facts.category())
            }
            CHirExpr::StatementExpr { facts, .. } => {
                (InferredType::Canonical(facts.ty()), ExprCategory::Value)
            }
            CHirExpr::CompoundLit(ty, _, _) => (InferredType::Canonical(*ty), ExprCategory::LValue),
            CHirExpr::Comma(_, right, _) => {
                let right = self.converted_expression_type(right)?;
                (right.ty, ExprCategory::Value)
            }
            CHirExpr::StringLit(literal, _) => (
                InferredType::StringArray(literal.prefix),
                ExprCategory::LValue,
            ),
            // A projected lvalue's type comes from the member or element it
            // designates. Qualifier propagation is deliberately not modelled
            // here: `MemoryPlace` accumulates it along the whole chain, which
            // is the authority every consumer already consults.
            CHirExpr::Member(base, name, _) => {
                let InferredType::Canonical(base_ty) = self.expression_type(base)?.ty else {
                    return None;
                };
                (
                    InferredType::Canonical(
                        self.record_member_path(base_ty, name)?.last()?.field_ty(),
                    ),
                    ExprCategory::LValue,
                )
            }
            CHirExpr::PtrMember(pointer, name, _) => {
                let OperandTypeClass::Pointer(pointee) =
                    self.operand_type_class(self.converted_expression_type(pointer)?.ty)
                else {
                    return None;
                };
                (
                    InferredType::Canonical(
                        self.record_member_path(pointee.pointee, name)?
                            .last()?
                            .field_ty(),
                    ),
                    ExprCategory::LValue,
                )
            }
            CHirExpr::Index(left, right, _) => {
                let (base, index) = self.subscript_operands(left, right)?;
                if !matches!(
                    self.operand_type_class(self.converted_expression_type(index)?.ty),
                    OperandTypeClass::Integer(_)
                ) {
                    return None;
                }
                (
                    InferredType::Canonical(self.subscript_element_type(base)?),
                    ExprCategory::LValue,
                )
            }
        };
        Some(InferredExpressionType {
            ty,
            category,
            null_pointer_constant: self.is_null_pointer_constant(expression),
        })
    }

    fn converted_expression_type(
        &self,
        expression: &CHirExpr<'_>,
    ) -> Option<InferredExpressionType> {
        let mut inferred = self.expression_type(expression)?;
        if let InferredType::Canonical(ty) = inferred.ty {
            match self.types.get(ty).map(|ty| &ty.kind)? {
                TypeKind::Array { element, .. } => {
                    inferred.ty = InferredType::Pointer(self.inferred_pointer_type(*element)?);
                }
                TypeKind::Function { .. } => {
                    inferred.ty = InferredType::Pointer(self.inferred_pointer_type(ty)?);
                }
                _ => {}
            }
        } else if let InferredType::StringArray(prefix) = inferred.ty {
            inferred.ty = InferredType::StringPointer(prefix);
        }
        if inferred.category != ExprCategory::Value {
            inferred.category = ExprCategory::Value;
        }
        Some(inferred)
    }

    fn promoted_arithmetic_type(&self, ty: InferredType) -> Option<InferredType> {
        Some(match self.operand_type_class(ty) {
            OperandTypeClass::Integer(integer) => {
                InferredType::Integer(CInteger::new(0, integer).promoted().ty())
            }
            OperandTypeClass::Real(real) => InferredType::Real(real),
            OperandTypeClass::Complex(complex) => InferredType::Complex(complex),
            OperandTypeClass::Pointer(_)
            | OperandTypeClass::StringPointer(_)
            | OperandTypeClass::Aggregate(_)
            | OperandTypeClass::Void
            | OperandTypeClass::Other => return None,
        })
    }

    fn binary_expression_type(
        &self,
        op: BinaryOp,
        left: InferredExpressionType,
        right: InferredExpressionType,
    ) -> Option<InferredType> {
        let left_class = self.operand_type_class(left.ty);
        let right_class = self.operand_type_class(right.ty);
        match op {
            BinaryOp::Add => match (left_class, right_class) {
                (OperandTypeClass::Pointer(pointee), OperandTypeClass::Integer(_))
                | (OperandTypeClass::Integer(_), OperandTypeClass::Pointer(pointee)) => {
                    Some(InferredType::Pointer(pointee))
                }
                (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Integer(_))
                | (OperandTypeClass::Integer(_), OperandTypeClass::StringPointer(prefix)) => {
                    Some(InferredType::StringPointer(prefix))
                }
                _ => self.usual_arithmetic_expression_type(left.ty, right.ty),
            },
            BinaryOp::Sub => match (left_class, right_class) {
                (OperandTypeClass::Pointer(pointee), OperandTypeClass::Integer(_)) => {
                    Some(InferredType::Pointer(pointee))
                }
                (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Integer(_)) => {
                    Some(InferredType::StringPointer(prefix))
                }
                (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right))
                    if self.pointer_types_compatible(left, right) =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Pointer(pointee))
                | (OperandTypeClass::Pointer(pointee), OperandTypeClass::StringPointer(prefix))
                    if self.pointer_accepts_string(pointee, prefix) =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                (OperandTypeClass::StringPointer(left), OperandTypeClass::StringPointer(right))
                    if left == right =>
                {
                    Some(InferredType::Integer(CIntegerType::LONG))
                }
                _ => self.usual_arithmetic_expression_type(left.ty, right.ty),
            },
            BinaryOp::Mul | BinaryOp::Div => {
                self.usual_arithmetic_expression_type(left.ty, right.ty)
            }
            BinaryOp::Mod | BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                let (OperandTypeClass::Integer(left), OperandTypeClass::Integer(right)) =
                    (left_class, right_class)
                else {
                    return None;
                };
                Some(InferredType::Integer(usual_arithmetic_type(
                    CInteger::new(0, left).promoted().ty(),
                    CInteger::new(0, right).promoted().ty(),
                )))
            }
            BinaryOp::Shl | BinaryOp::Shr => {
                let (OperandTypeClass::Integer(left), OperandTypeClass::Integer(_)) =
                    (left_class, right_class)
                else {
                    return None;
                };
                Some(InferredType::Integer(
                    CInteger::new(0, left).promoted().ty(),
                ))
            }
            BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge
            | BinaryOp::Eq
            | BinaryOp::Ne => {
                let compatible_pointers = match (left_class, right_class) {
                    (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right)) => {
                        self.pointer_types_compatible(left, right)
                    }
                    (
                        OperandTypeClass::Pointer(pointee),
                        OperandTypeClass::StringPointer(prefix),
                    )
                    | (
                        OperandTypeClass::StringPointer(prefix),
                        OperandTypeClass::Pointer(pointee),
                    ) => self.pointer_accepts_string(pointee, prefix),
                    (
                        OperandTypeClass::StringPointer(left),
                        OperandTypeClass::StringPointer(right),
                    ) => left == right,
                    _ => false,
                };
                let comparable = (is_arithmetic_class(left_class)
                    && is_arithmetic_class(right_class))
                    || compatible_pointers
                    || (matches!(left_class, OperandTypeClass::Pointer(_))
                        && right.null_pointer_constant)
                    || (matches!(right_class, OperandTypeClass::Pointer(_))
                        && left.null_pointer_constant)
                    || (matches!(left_class, OperandTypeClass::StringPointer(_))
                        && right.null_pointer_constant)
                    || (matches!(right_class, OperandTypeClass::StringPointer(_))
                        && left.null_pointer_constant);
                comparable.then_some(InferredType::Integer(CIntegerType::INT))
            }
            BinaryOp::And | BinaryOp::Or => (is_scalar_class(left_class)
                && is_scalar_class(right_class))
            .then_some(InferredType::Integer(CIntegerType::INT)),
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
            | BinaryOp::BitXorAssign => None,
        }
    }

    fn conditional_expression_type(
        &self,
        when_true: InferredExpressionType,
        when_false: InferredExpressionType,
    ) -> Option<InferredType> {
        if when_true.ty == when_false.ty {
            return Some(when_true.ty);
        }
        if let Some(arithmetic) = self.usual_arithmetic_expression_type(when_true.ty, when_false.ty)
        {
            return Some(arithmetic);
        }
        match (
            self.operand_type_class(when_true.ty),
            self.operand_type_class(when_false.ty),
        ) {
            (OperandTypeClass::Pointer(_), OperandTypeClass::Pointer(right))
                if when_true.null_pointer_constant =>
            {
                Some(InferredType::Pointer(right))
            }
            (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(_))
                if when_false.null_pointer_constant =>
            {
                Some(InferredType::Pointer(left))
            }
            (OperandTypeClass::Pointer(left), OperandTypeClass::Pointer(right))
                if self.pointer_types_compatible(left, right) =>
            {
                self.composite_pointer_type(left, right)
                    .map(InferredType::Pointer)
            }
            (OperandTypeClass::Pointer(pointee), OperandTypeClass::StringPointer(prefix))
            | (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Pointer(pointee))
                if self.pointer_accepts_string(pointee, prefix) =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::Pointer(pointee), OperandTypeClass::Integer(_))
                if when_false.null_pointer_constant =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::StringPointer(prefix), OperandTypeClass::Integer(_))
                if when_false.null_pointer_constant =>
            {
                Some(InferredType::StringPointer(prefix))
            }
            (OperandTypeClass::Integer(_), OperandTypeClass::StringPointer(prefix))
                if when_true.null_pointer_constant =>
            {
                Some(InferredType::StringPointer(prefix))
            }
            (OperandTypeClass::Integer(_), OperandTypeClass::Pointer(pointee))
                if when_true.null_pointer_constant =>
            {
                Some(InferredType::Pointer(pointee))
            }
            (OperandTypeClass::Aggregate(left), OperandTypeClass::Aggregate(right))
                if self.same_unqualified_type(left, right) =>
            {
                Some(InferredType::Canonical(left))
            }
            _ => None,
        }
    }

    fn usual_arithmetic_expression_type(
        &self,
        left: InferredType,
        right: InferredType,
    ) -> Option<InferredType> {
        let left = self.operand_type_class(left);
        let right = self.operand_type_class(right);
        match (left, right) {
            (OperandTypeClass::Integer(left), OperandTypeClass::Integer(right)) => {
                Some(InferredType::Integer(usual_arithmetic_type(
                    CInteger::new(0, left).promoted().ty(),
                    CInteger::new(0, right).promoted().ty(),
                )))
            }
            (left, right) if is_arithmetic_class(left) && is_arithmetic_class(right) => {
                let has_complex = matches!(left, OperandTypeClass::Complex(_))
                    || matches!(right, OperandTypeClass::Complex(_));
                let floating = widest_floating_kind(left, right)?;
                if has_complex {
                    Some(InferredType::Complex(complex_base_for_float(floating)))
                } else {
                    Some(InferredType::Real(floating))
                }
            }
            _ => None,
        }
    }

    fn operand_type_class(&self, ty: InferredType) -> OperandTypeClass {
        match ty {
            InferredType::Integer(integer) => OperandTypeClass::Integer(integer),
            InferredType::Real(real) => OperandTypeClass::Real(real),
            InferredType::Complex(complex) => OperandTypeClass::Complex(complex),
            InferredType::Pointer(pointee) => OperandTypeClass::Pointer(pointee),
            InferredType::StringArray(_) => OperandTypeClass::Other,
            InferredType::StringPointer(prefix) => OperandTypeClass::StringPointer(prefix),
            InferredType::Canonical(ty) => {
                let Some(canonical) = self.types.get(ty) else {
                    return OperandTypeClass::Other;
                };
                match canonical.kind {
                    TypeKind::Int(rank) => OperandTypeClass::Integer(integer_type_for_rank(rank)),
                    TypeKind::Enum(_) => self
                        .canonical_integer_type(ty)
                        .map(OperandTypeClass::Integer)
                        .unwrap_or(OperandTypeClass::Other),
                    TypeKind::Float(real) => OperandTypeClass::Real(real),
                    TypeKind::Complex(complex) => OperandTypeClass::Complex(complex),
                    TypeKind::Pointer(pointee) => self
                        .inferred_pointer_type(pointee)
                        .map(OperandTypeClass::Pointer)
                        .unwrap_or(OperandTypeClass::Other),
                    TypeKind::Struct(_) | TypeKind::Union(_) => OperandTypeClass::Aggregate(ty),
                    TypeKind::Void => OperandTypeClass::Void,
                    TypeKind::Array { .. }
                    | TypeKind::Function { .. }
                    | TypeKind::BuiltinVaList
                    | TypeKind::GnuInt128(_) => OperandTypeClass::Other,
                }
            }
        }
    }

    fn pointer_arithmetic_is_valid(&self, pointee: TypeId) -> bool {
        TargetDataLayout::aarch64_unknown_linux_gnu()
            .layout_of(&self.types, pointee)
            .is_ok()
    }

    fn inferred_pointer_type(&self, pointee: TypeId) -> Option<InferredPointerType> {
        Some(InferredPointerType {
            pointee,
            qualifiers: self.types.get(pointee)?.qualifiers,
        })
    }

    fn pointer_accepts_string(&self, pointer: InferredPointerType, prefix: StringPrefix) -> bool {
        self.types.get(pointer.pointee).is_some_and(|pointee| {
            matches!(pointee.kind, TypeKind::Void)
                || matches!(
                    (prefix, &pointee.kind),
                    (
                        StringPrefix::None | StringPrefix::U8,
                        TypeKind::Int(IntRank::Char)
                    ) | (StringPrefix::L, TypeKind::Int(IntRank::Int))
                        | (StringPrefix::U16, TypeKind::Int(IntRank::UShort))
                        | (StringPrefix::U, TypeKind::Int(IntRank::UInt))
                )
        })
    }

    fn pointer_assignment_compatible(
        &self,
        target: InferredPointerType,
        source: InferredPointerType,
    ) -> bool {
        self.pointer_types_compatible(target, source)
            && qualifiers_include(
                target.qualifiers.compatibility(),
                source.qualifiers.compatibility(),
            )
    }

    fn pointer_types_compatible(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> bool {
        let (Some(left), Some(right)) =
            (self.types.get(left.pointee), self.types.get(right.pointee))
        else {
            return false;
        };
        if left.kind == right.kind {
            return true;
        }
        matches!(
            (&left.kind, &right.kind),
            (TypeKind::Void, right) if is_object_type_kind(right)
        ) || matches!(
            (&left.kind, &right.kind),
            (left, TypeKind::Void) if is_object_type_kind(left)
        )
    }

    fn composite_pointer_type(
        &self,
        left: InferredPointerType,
        right: InferredPointerType,
    ) -> Option<InferredPointerType> {
        if !self.pointer_types_compatible(left, right) {
            return None;
        }
        let left_kind = &self.types.get(left.pointee)?.kind;
        let right_kind = &self.types.get(right.pointee)?.kind;
        let pointee = match (left_kind, right_kind) {
            (TypeKind::Void, _) => left.pointee,
            (_, TypeKind::Void) => right.pointee,
            _ => left.pointee,
        };
        Some(InferredPointerType {
            pointee,
            qualifiers: merge_qualifiers(left.qualifiers, right.qualifiers),
        })
    }

    fn same_unqualified_type(&self, left: TypeId, right: TypeId) -> bool {
        match (self.types.get(left), self.types.get(right)) {
            (Some(left), Some(right)) => left.kind == right.kind,
            _ => false,
        }
    }

    fn is_null_pointer_constant(&self, expression: &CHirExpr<'_>) -> bool {
        let integer_zero = || {
            self.integer_expression_type(expression).is_ok()
                && self
                    .eval_integer_constant_expression(expression)
                    .is_ok_and(|value| value.mathematical_value() == 0)
        };
        if integer_zero() {
            return true;
        }
        let CHirExpr::Cast(target, operand, _) = expression else {
            return false;
        };
        let Some(TypeKind::Pointer(pointee)) = self.types.get(*target).map(|ty| &ty.kind) else {
            return false;
        };
        if !matches!(
            self.types.get(*pointee).map(|ty| &ty.kind),
            Some(TypeKind::Void)
        ) {
            return false;
        }
        self.integer_expression_type(operand).is_ok()
            && self
                .eval_integer_constant_expression(operand)
                .is_ok_and(|value| value.mathematical_value() == 0)
    }

    fn parse_conditional(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let condition = self.parse_binary(1)?;
        if !self.match_kind(TokenKind::Question) {
            return Some(condition);
        }
        if self.match_kind(TokenKind::Colon) {
            let fallback = self.parse_conditional()?;
            let span = self.merge_span(condition.span(), fallback.span());
            return Some(self.alloc_node(CHirExpr::Elvis(condition, fallback, span)));
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
        if self.peek_kind() == Some(TokenKind::AmpAmp) {
            let operator = self.advance().expect("peeked label-address operator");
            let token = self.expect(TokenKind::Identifier)?;
            let spelling = self.token_text(&token).to_owned();
            if !self.in_function_scope() {
                self.report("E2920", "undeclared-label", token.span);
                return None;
            }
            let label = SpannedName {
                spelling: self.resolve_local_label(&spelling).unwrap_or(spelling),
                span: token.span,
            };
            let void = self.intern(TypeKind::Void, Qualifiers::default());
            let pointer = self.intern(TypeKind::Pointer(void), Qualifiers::default());
            let span = self.merge_span(operator.span, token.span);
            return Some(self.alloc_node(CHirExpr::LabelAddress {
                label,
                facts: ExprFacts::value(pointer),
                span,
            }));
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::PlusPlus | TokenKind::MinusMinus)
        ) {
            let token = self.advance().expect("peeked prefix update");
            let target = self.parse_unary()?;
            let typed_target = self.typed_update_target(target)?;
            let op = if token.kind == TokenKind::PlusPlus {
                UpdateOp::PreIncrement
            } else {
                UpdateOp::PreDecrement
            };
            let span = self.merge_span(token.span, target.span());
            match self.update_constraint(typed_target.ty()) {
                UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
                UpdateConstraint::AggregateBoundary | UpdateConstraint::Invalid => {
                    self.report("E2102", "invalid-update-operands", span);
                    return None;
                }
            }
            return Some(match typed_target {
                TypedUpdateTarget::Direct(target_identity) => {
                    let place = self.finish_scalar_update_place(target_identity, target.span())?;
                    self.alloc_node(CHirExpr::Update {
                        op,
                        place,
                        target,
                        facts: ExprFacts::value(place.ty()),
                        span,
                    })
                }
                TypedUpdateTarget::Memory(place) => {
                    let facts = ExprFacts::value(place.value_ty());
                    self.alloc_node(CHirExpr::MemoryUpdate {
                        op,
                        place,
                        target_span: target.span(),
                        facts,
                        span,
                    })
                }
            });
        }

        let unary = match self.peek_kind() {
            Some(TokenKind::Minus) => Some(UnaryOp::Neg),
            Some(TokenKind::Plus) => Some(UnaryOp::Plus),
            Some(TokenKind::Bang) => Some(UnaryOp::Not),
            Some(TokenKind::Tilde) => Some(UnaryOp::BitNot),
            Some(TokenKind::Amp) => Some(UnaryOp::AddrOf),
            Some(TokenKind::Star) => Some(UnaryOp::Deref),
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
                let (ty, tag_event) = self.parse_type_name()?;
                let end = self.peek().map(|token| token.span).unwrap_or(token.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(token.span, end);
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(event, span, Vec::new());
                    self.side_declarations.push(declaration);
                }
                return Some(self.alloc_node(CHirExpr::SizeOf(ty, span)));
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(token.span, operand.span());
            let inferred = self.expression_type(operand).or_else(|| {
                self.report("E1003", "expected-expression", operand.span());
                None
            })?;
            let operand_ty = self.materialize_inferred_type(inferred.ty, Some(operand))?;
            return Some(self.alloc_node(CHirExpr::SizeOfExpr(operand, operand_ty, span)));
        }

        if matches!(
            self.peek_kind(),
            Some(TokenKind::Alignof | TokenKind::GnuAlignof)
        ) {
            let token = self.advance().unwrap();
            let is_gnu = token.kind == TokenKind::GnuAlignof;
            if is_gnu && self.peek_kind() != Some(TokenKind::LParen) {
                self.report("E2910", "builtin-not-called", token.span);
                return None;
            }
            self.expect(TokenKind::LParen)?;
            if is_gnu && self.peek_kind() == Some(TokenKind::RParen) {
                self.report("E2909", "builtin-arity", token.span);
                return None;
            }
            let (ty, tag_event) = if self
                .peek()
                .is_some_and(|token| self.token_starts_type_name(token))
            {
                self.parse_type_name()
            } else {
                let operand = self.parse_expression()?;
                let inferred = self.expression_type(operand).or_else(|| {
                    self.report("E1003", "expected-expression", operand.span());
                    None
                })?;
                let ty = self.materialize_inferred_type(inferred.ty, Some(operand))?;
                Some((ty, None))
            }?;
            if self.peek_kind() != Some(TokenKind::RParen) {
                self.report(
                    if is_gnu { "E2909" } else { "E1002" },
                    if is_gnu {
                        "builtin-arity"
                    } else {
                        "unexpected-token"
                    },
                    self.span(),
                );
                return None;
            }
            let end = self.peek().map(|token| token.span).unwrap_or(token.span);
            self.expect(TokenKind::RParen)?;
            let span = self.merge_span(token.span, end);
            if let Some(event) = tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            return Some(self.alloc_node(CHirExpr::AlignOf(ty, span)));
        }

        if self.looks_like_parenthesized_type() {
            let open = self.advance().unwrap();
            let (ty, tag_event) = self.parse_type_name()?;
            let close = self.expect(TokenKind::RParen)?;
            if self.peek_kind() == Some(TokenKind::LBrace) {
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(
                        event,
                        self.merge_span(open.span, close.span),
                        Vec::new(),
                    );
                    self.side_declarations.push(declaration);
                }
                let written = self.parse_written_initializer()?;
                let ty = self.infer_initializer_target_type(ty, &written)?;
                let plan = self.normalize_initializer_plan(ty, &written, false)?;
                if self.current_scope == self.symbols.file_scope() {
                    self.validate_static_initializer_plan(&plan)?;
                }
                let span = self.merge_span(open.span, written.span());
                let expression = self.alloc_node(CHirExpr::CompoundLit(
                    ty,
                    ObjectInitializer::new(written, plan),
                    span,
                ));
                return self.parse_postfix_tail(expression);
            }
            if !self.peek_kind().is_some_and(can_start_expression) {
                self.report("E1003", "expected-expression", close.span);
                return None;
            }
            let operand = self.parse_unary()?;
            let span = self.merge_span(open.span, operand.span());
            if let Some(event) = tag_event {
                let declaration = self.finish_tag_event(event, span, Vec::new());
                self.side_declarations.push(declaration);
            }
            return Some(self.alloc_node(CHirExpr::Cast(ty, operand, span)));
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Option<&'arena CHirExpr<'arena>> {
        let expression = self.parse_primary()?;
        self.parse_postfix_tail(expression)
    }

    fn parse_postfix_tail(
        &mut self,
        mut expression: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
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
                    let subscript = self.alloc_node(CHirExpr::Index(expression, index, span));
                    expression = self.convert_projected_lvalue(subscript);
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
                    let projected = if pointer {
                        self.alloc_node(CHirExpr::PtrMember(expression, name, span))
                    } else {
                        self.alloc_node(CHirExpr::Member(expression, name, span))
                    };
                    expression = self.convert_projected_lvalue(projected);
                }
                Some(TokenKind::PlusPlus) | Some(TokenKind::MinusMinus) => {
                    let token = self.advance().unwrap();
                    let target = expression;
                    let typed_target = self.typed_update_target(target)?;
                    let op = if token.kind == TokenKind::PlusPlus {
                        UpdateOp::PostIncrement
                    } else {
                        UpdateOp::PostDecrement
                    };
                    let span = self.merge_span(target.span(), token.span);
                    match self.update_constraint(typed_target.ty()) {
                        UpdateConstraint::Scalar | UpdateConstraint::PointerBoundary => {}
                        UpdateConstraint::AggregateBoundary | UpdateConstraint::Invalid => {
                            self.report("E2102", "invalid-update-operands", span);
                            return None;
                        }
                    }
                    expression = match typed_target {
                        TypedUpdateTarget::Direct(target_identity) => {
                            let place =
                                self.finish_scalar_update_place(target_identity, target.span())?;
                            self.alloc_node(CHirExpr::Update {
                                op,
                                place,
                                target,
                                facts: ExprFacts::value(place.ty()),
                                span,
                            })
                        }
                        TypedUpdateTarget::Memory(place) => {
                            let facts = ExprFacts::value(place.value_ty());
                            self.alloc_node(CHirExpr::MemoryUpdate {
                                op,
                                place,
                                target_span: target.span(),
                                facts,
                                span,
                            })
                        }
                    };
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
                let Some(literal) = parse_int_literal(text) else {
                    self.report("E1006", "invalid-integer-literal", tok.span);
                    return None;
                };
                CHirExpr::Lit(literal, tok.span)
            }
            TokenKind::Identifier => {
                let spelling = self.token_text(&tok).to_owned();
                let Some(symbol) = self
                    .symbols
                    .resolve(self.current_scope, &spelling)
                    .expect("the parser current scope is always live")
                else {
                    self.report(
                        "E2001",
                        format!("undeclared-identifier: {spelling}"),
                        tok.span,
                    );
                    return None;
                };
                if self
                    .symbols
                    .symbol(symbol)
                    .expect("resolved symbol belongs to the same table")
                    .kind()
                    == SymbolKind::Typedef
                {
                    self.report(
                        "E2003",
                        format!("typedef-name-not-expression: {spelling}"),
                        tok.span,
                    );
                    return None;
                }
                let facts = self.facts_for_symbol(symbol);
                CHirExpr::Ident {
                    symbol,
                    spelling,
                    facts,
                    span: tok.span,
                }
            }
            TokenKind::FloatLiteral => {
                let Some(literal) = parse_float_literal(self.token_text(&tok)) else {
                    self.report("E1006", "invalid-floating-literal", tok.span);
                    return None;
                };
                CHirExpr::FloatLit(literal, tok.span)
            }
            TokenKind::CharLiteral => {
                let Some(literal) = parse_char_literal(self.token_text(&tok)) else {
                    self.report("E1006", "invalid-character-literal", tok.span);
                    return None;
                };
                CHirExpr::CharLit(literal, tok.span)
            }
            TokenKind::StringLiteral => {
                let Some(mut literal) = parse_string_literal(self.token_text(&tok)) else {
                    self.report("E1006", "invalid-string-literal", tok.span);
                    return None;
                };
                let mut end = tok.span;
                while self.peek_kind() == Some(TokenKind::StringLiteral) {
                    let next = self.advance().expect("peeked adjacent string literal");
                    let Some(fragment) = parse_string_literal(self.token_text(&next)) else {
                        self.report("E1006", "invalid-string-literal", next.span);
                        return None;
                    };
                    let Some(prefix) = concatenate_string_prefix(literal.prefix, fragment.prefix)
                    else {
                        self.report("E1006", "incompatible-string-literal-prefixes", next.span);
                        return None;
                    };
                    literal.prefix = prefix;
                    literal.value.push_str(&fragment.value);
                    literal.fragments.extend(fragment.fragments);
                    end = next.span;
                }
                let span = self.merge_span(tok.span, end);
                if literal.explicit_unit_count().is_none() {
                    self.report("E1006", "invalid-string-literal", span);
                    return None;
                }
                return Some(self.alloc_node(CHirExpr::StringLit(literal, span)));
            }
            TokenKind::LParen => {
                if self.peek_kind() == Some(TokenKind::LBrace) {
                    if !self.in_function_scope() {
                        self.report("E2905", "statement-expression-outside-function", tok.span);
                        return None;
                    }
                    let body = self.parse_compound_statement(true);
                    let close = self.expect(TokenKind::RParen)?;
                    let value = match body {
                        CHirStmt::Compound(items, _) => items.last().and_then(|item| match item {
                            CHirBlockItem::Statement(CHirStmt::Expr(expression, _)) => {
                                Some(*expression)
                            }
                            CHirBlockItem::Declaration(_) | CHirBlockItem::Statement(_) => None,
                        }),
                        _ => unreachable!("statement expression parses one compound statement"),
                    };
                    let ty = if let Some(expression) = value {
                        let inferred =
                            self.converted_expression_type(expression).or_else(|| {
                                self.report("E1003", "expected-expression", expression.span());
                                None
                            })?;
                        self.materialize_inferred_type(inferred.ty, Some(expression))?
                    } else {
                        self.intern(TypeKind::Void, Qualifiers::default())
                    };
                    let span = self.merge_span(tok.span, close.span);
                    return Some(self.alloc_node(CHirExpr::StatementExpr {
                        body,
                        value,
                        facts: ExprFacts::value(ty),
                        span,
                    }));
                }
                let expr = self.parse_expression()?;
                let rparen_span = self.peek().map(|t| t.span).unwrap_or(tok.span);
                self.expect(TokenKind::RParen)?;
                let span = self.merge_span(tok.span, rparen_span);
                return Some(self.alloc_node(respan_expr(expr, span)));
            }
            TokenKind::Generic => return self.parse_generic_selection(tok),
            TokenKind::BuiltinExpect => return self.parse_builtin_expect(tok),
            TokenKind::BuiltinConstantP => return self.parse_builtin_constant_p(tok),
            TokenKind::BuiltinChooseExpr => return self.parse_builtin_choose_expr(tok),
            TokenKind::BuiltinTypesCompatibleP => {
                return self.parse_builtin_types_compatible_p(tok);
            }
            TokenKind::BuiltinOffsetof => return self.parse_builtin_offsetof(tok),
            TokenKind::BuiltinObjectSize => return self.parse_builtin_object_size(tok),
            TokenKind::BuiltinBit => return self.parse_builtin_bit(tok),
            TokenKind::BuiltinUnreachable => return self.parse_builtin_unreachable(tok),
            TokenKind::BuiltinReturnAddress => return self.parse_builtin_return_address(tok),
            TokenKind::BuiltinAddOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Add);
            }
            TokenKind::BuiltinSubOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Sub);
            }
            TokenKind::BuiltinMulOverflow => {
                return self.parse_checked_arithmetic(tok, CheckedArithmeticOp::Mul);
            }
            _ => {
                self.report("E1003", "expected-expression", tok.span);
                return None;
            }
        };
        Some(self.alloc_node(expression))
    }

    fn parse_builtin_expect(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let value = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let expected = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");

        for operand in [value, expected] {
            let Some(inferred) = self.converted_expression_type(operand) else {
                self.report("E1003", "expected-expression", operand.span());
                return None;
            };
            if !is_arithmetic_class(self.operand_type_class(inferred.ty)) {
                self.report("E1003", "expected-expression", operand.span());
                return None;
            }
        }

        let long_ty = self.intern(TypeKind::Int(IntRank::Long), Qualifiers::default());
        let converted_value = self.alloc_node(CHirExpr::Cast(long_ty, value, value.span()));
        let converted_expected =
            self.alloc_node(CHirExpr::Cast(long_ty, expected, expected.span()));
        let span = self.merge_span(keyword.span, close.span);
        self.front_end_builtins.push(FrontEndBuiltinRecord::Expect {
            span,
            value: converted_value,
            expected: converted_expected,
        });
        Some(self.alloc_node(CHirExpr::Cast(long_ty, value, span)))
    }

    fn parse_builtin_constant_p(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let operand = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        if self.expression_type(operand).is_none() {
            self.report("E1003", "expected-expression", operand.span());
            return None;
        }

        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let result = builtin_constant_p(operand, &self.types, &self.symbols, &target);
        let span = self.merge_span(keyword.span, close.span);
        self.front_end_builtins
            .push(FrontEndBuiltinRecord::ConstantP {
                span,
                operand,
                result,
            });
        Some(self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: u64::from(result),
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        )))
    }

    fn parse_builtin_choose_expr(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let selector = self.parse_required_constant_assignment()?;
        let selector_span = selector.span();
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let when_true = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let when_false = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        self.advance();

        let selected_true = match self.eval_integer_constant_expression(selector) {
            Ok(value) => value.truth(),
            Err(()) => {
                self.report("E2912", "static-assertion-not-constant", selector_span);
                return None;
            }
        };
        // Both arms are semantically checked, but only the selected C-HIR node
        // is retained. This preserves its exact type and value category while
        // making evaluation of the other arm impossible downstream.
        for arm in [when_true, when_false] {
            if self.expression_type(arm).is_none() {
                self.report("E1003", "expected-expression", arm.span());
                return None;
            }
        }
        Some(if selected_true { when_true } else { when_false })
    }

    fn parse_builtin_types_compatible_p(
        &mut self,
        keyword: Token,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let (left, left_tag) = self.parse_type_name()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let (right, right_tag) = self.parse_type_name()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        for event in [left_tag, right_tag].into_iter().flatten() {
            let declaration = self.finish_tag_event(event, span, Vec::new());
            self.side_declarations.push(declaration);
        }
        let value = self.types.compatible_ignoring_top_qualifiers(left, right);
        Some(self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: u64::from(value),
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        )))
    }

    fn parse_builtin_offsetof(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let (mut current_ty, tag_event) = self.parse_type_name()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let first = self.advance()?;
        if first.kind != TokenKind::Identifier {
            self.report("E2906", "designator-outside-object", first.span);
            return None;
        }
        let mut fixed_offset = 0_u64;
        let mut runtime_offset = None;
        let size_ty = self.intern(TypeKind::Int(IntRank::ULong), Qualifiers::default());
        let mut field_name = Some((self.token_text(&first).to_owned(), first.span));
        loop {
            if let Some((name, span)) = field_name.take() {
                let Some((field_offset, field_ty)) = self.offsetof_field(current_ty, &name) else {
                    self.report("E2906", "designator-outside-object", span);
                    return None;
                };
                fixed_offset = fixed_offset.wrapping_add(field_offset);
                current_ty = field_ty;
            }
            match self.peek_kind() {
                Some(TokenKind::Dot) => {
                    self.advance();
                    let member = self.advance()?;
                    if member.kind != TokenKind::Identifier {
                        self.report("E2906", "designator-outside-object", member.span);
                        return None;
                    }
                    field_name = Some((self.token_text(&member).to_owned(), member.span));
                }
                Some(TokenKind::LBracket) => {
                    let open = self.advance().expect("peeked subscript");
                    let index_expression = self.parse_conditional()?;
                    let end = self.expect(TokenKind::RBracket)?;
                    let span = self.merge_span(open.span, end.span);
                    let Some(TypeKind::Array { element, .. }) =
                        self.types.get(current_ty).map(|ty| ty.kind.clone())
                    else {
                        self.report("E2906", "designator-outside-object", span);
                        return None;
                    };
                    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
                    let Ok(layout) = target.layout_of(&self.types, element) else {
                        self.report("E2906", "designator-outside-object", span);
                        return None;
                    };
                    if let Ok(index) = self.eval_integer_constant_expression(index_expression) {
                        let scaled = eval_binary_integer(
                            BinaryOp::Mul,
                            index.cast(CIntegerType::ULONG),
                            CInteger::new(layout.size_bytes, CIntegerType::ULONG),
                        )
                        .ok()?;
                        fixed_offset = eval_binary_integer(
                            BinaryOp::Add,
                            CInteger::new(fixed_offset, CIntegerType::ULONG),
                            scaled,
                        )
                        .ok()?
                        .raw_bits();
                    } else {
                        let Some(inferred) = self.converted_expression_type(index_expression)
                        else {
                            self.report("E2906", "designator-outside-object", span);
                            return None;
                        };
                        if !matches!(
                            self.operand_type_class(inferred.ty),
                            OperandTypeClass::Integer(_)
                        ) {
                            self.report("E2906", "designator-outside-object", span);
                            return None;
                        }
                        let base = match runtime_offset {
                            Some(expression) if fixed_offset == 0 => expression,
                            Some(expression) => {
                                let fixed = self.size_literal(fixed_offset, span);
                                self.alloc_node(CHirExpr::Binary(
                                    BinaryOp::Add,
                                    expression,
                                    fixed,
                                    span,
                                ))
                            }
                            None => self.size_literal(fixed_offset, span),
                        };
                        fixed_offset = 0;
                        let evaluated = self.alloc_node(CHirExpr::EvaluateOnce(
                            index_expression,
                            index_expression.span(),
                        ));
                        let converted = self.alloc_node(CHirExpr::Cast(
                            size_ty,
                            evaluated,
                            index_expression.span(),
                        ));
                        let stride = self.size_literal(layout.size_bytes, span);
                        let scaled = self.alloc_node(CHirExpr::Binary(
                            BinaryOp::Mul,
                            converted,
                            stride,
                            span,
                        ));
                        runtime_offset = Some(
                            if matches!(base, CHirExpr::Lit(IntLit { value: 0, .. }, _)) {
                                scaled
                            } else {
                                self.alloc_node(CHirExpr::Binary(BinaryOp::Add, base, scaled, span))
                            },
                        );
                    }
                    current_ty = element;
                }
                _ => break,
            }
        }
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let span = self.merge_span(keyword.span, close.span);
        if let Some(event) = tag_event {
            let declaration = self.finish_tag_event(event, span, Vec::new());
            self.side_declarations.push(declaration);
        }
        Some(match runtime_offset {
            Some(expression) if fixed_offset == 0 => expression,
            Some(expression) => {
                let fixed = self.size_literal(fixed_offset, span);
                self.alloc_node(CHirExpr::Binary(BinaryOp::Add, expression, fixed, span))
            }
            None => self.size_literal(fixed_offset, span),
        })
    }

    fn size_literal(&self, value: u64, span: SourceSpan) -> &'arena CHirExpr<'arena> {
        self.alloc_node(CHirExpr::Lit(
            IntLit {
                value,
                suffix: IntSuffix::UL,
                base: IntBase::Dec,
            },
            span,
        ))
    }

    fn parse_builtin_object_size(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let operand = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let mode_expression = self.parse_required_constant_assignment()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        if self.expression_type(operand).is_none() {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        }
        let Some(mode) = self
            .eval_integer_constant_expression(mode_expression)
            .ok()
            .and_then(|value| u8::try_from(value.mathematical_value()).ok())
            .filter(|mode| *mode <= 3)
        else {
            self.report("E2925", "builtin-mode-out-of-range", mode_expression.span());
            return None;
        };
        let result = if mode <= 1 { u64::MAX } else { 0 };
        let span = self.merge_span(keyword.span, close.span);
        self.front_end_builtins
            .push(FrontEndBuiltinRecord::ObjectSize {
                span,
                operand,
                mode,
                result,
            });
        if mode >= 2 {
            return Some(self.size_literal(0, span));
        }
        let one = self.alloc_node(CHirExpr::Lit(
            IntLit {
                value: 1,
                suffix: IntSuffix::None,
                base: IntBase::Dec,
            },
            span,
        ));
        let negative_one = self.alloc_node(CHirExpr::Unary(UnaryOp::Neg, one, span));
        let size_ty = self.intern_c_integer_type(CIntegerType::ULONG);
        Some(self.alloc_node(CHirExpr::Cast(size_ty, negative_one, span)))
    }

    fn offsetof_field(&self, record_ty: TypeId, name: &str) -> Option<(u64, TypeId)> {
        let fields = self.record_member_path(record_ty, name)?;
        let target = TargetDataLayout::aarch64_unknown_linux_gnu();
        let mut offset = 0_u64;
        for field in &fields {
            let layout = target.record_layout(&self.types, field.tag()).ok()?;
            let placed = layout
                .fields
                .iter()
                .find(|placed| placed.field_index == field.field_index())?;
            offset = offset.checked_add(placed.byte_offset)?;
        }
        Some((offset, fields.last()?.field_ty()))
    }

    fn parse_builtin_bit(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        let spelling = self.token_text(&keyword).to_owned();
        let (op, rank) = match spelling.as_str() {
            "__builtin_ffs" => (BitBuiltinOp::FindFirstSet, IntRank::Int),
            "__builtin_ffsl" => (BitBuiltinOp::FindFirstSet, IntRank::Long),
            "__builtin_ffsll" => (BitBuiltinOp::FindFirstSet, IntRank::LongLong),
            "__builtin_clz" => (BitBuiltinOp::CountLeadingZeros, IntRank::UInt),
            "__builtin_clzl" => (BitBuiltinOp::CountLeadingZeros, IntRank::ULong),
            "__builtin_clzll" => (BitBuiltinOp::CountLeadingZeros, IntRank::ULongLong),
            "__builtin_ctz" => (BitBuiltinOp::CountTrailingZeros, IntRank::UInt),
            "__builtin_ctzl" => (BitBuiltinOp::CountTrailingZeros, IntRank::ULong),
            "__builtin_ctzll" => (BitBuiltinOp::CountTrailingZeros, IntRank::ULongLong),
            "__builtin_popcount" => (BitBuiltinOp::PopulationCount, IntRank::UInt),
            "__builtin_popcountl" => (BitBuiltinOp::PopulationCount, IntRank::ULong),
            "__builtin_popcountll" => (BitBuiltinOp::PopulationCount, IntRank::ULongLong),
            _ => unreachable!("BuiltinBit is assigned only to the frozen builtin spellings"),
        };
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let operand = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let Some(inferred) = self.converted_expression_type(operand) else {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        };
        if !matches!(
            self.operand_type_class(inferred.ty),
            OperandTypeClass::Integer(_)
        ) {
            self.report("E2913", "builtin-operand-not-convertible", operand.span());
            return None;
        }

        let parameter_ty = self.intern(TypeKind::Int(rank), Qualifiers::default());
        let parameter_integer = integer_type_for_rank(rank);
        let span = self.merge_span(keyword.span, close.span);
        if let Ok(value) = self.eval_integer_constant_expression(operand) {
            if let Some(result) = eval_bit_builtin(op, value.cast(parameter_integer)) {
                return Some(self.alloc_node(CHirExpr::Lit(
                    IntLit {
                        value: result.raw_bits(),
                        suffix: IntSuffix::None,
                        base: IntBase::Dec,
                    },
                    span,
                )));
            }
        }
        let converted = self.alloc_node(CHirExpr::Cast(parameter_ty, operand, operand.span()));
        let result_ty = self.intern(TypeKind::Int(IntRank::Int), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BitBuiltin {
            op,
            operand: converted,
            facts: ExprFacts::value(result_ty),
            span,
        }))
    }

    fn parse_builtin_unreachable(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BuiltinUnreachable {
            facts: ExprFacts::value(void_ty),
            span: self.merge_span(keyword.span, close.span),
        }))
    }

    fn parse_builtin_return_address(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        let argument = self.parse_required_constant_assignment()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");
        if !self
            .eval_integer_constant_expression(argument)
            .is_ok_and(|value| value.raw_bits() == 0)
        {
            self.report("E2913", "builtin-operand-not-convertible", argument.span());
            return None;
        }
        let void_ty = self.intern(TypeKind::Void, Qualifiers::default());
        let pointer_ty = self.intern(TypeKind::Pointer(void_ty), Qualifiers::default());
        Some(self.alloc_node(CHirExpr::BuiltinReturnAddress {
            facts: ExprFacts::value(pointer_ty),
            span: self.merge_span(keyword.span, close.span),
        }))
    }

    fn parse_checked_arithmetic(
        &mut self,
        keyword: Token,
        op: CheckedArithmeticOp,
    ) -> Option<&'arena CHirExpr<'arena>> {
        if self.peek_kind() != Some(TokenKind::LParen) {
            self.report("E2910", "builtin-not-called", keyword.span);
            return None;
        }
        self.advance();
        if self.peek_kind() == Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let lhs = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let rhs = self.parse_argument_expression()?;
        if !self.match_kind(TokenKind::Comma) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let target = self.parse_argument_expression()?;
        if self.peek_kind() != Some(TokenKind::RParen) {
            self.report("E2909", "builtin-arity", keyword.span);
            return None;
        }
        let close = self.advance().expect("peeked builtin closing parenthesis");

        for operand in [lhs, rhs] {
            let Some(inferred) = self.converted_expression_type(operand) else {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            };
            if !matches!(
                self.operand_type_class(inferred.ty),
                OperandTypeClass::Integer(_)
            ) {
                self.report("E2913", "builtin-operand-not-convertible", operand.span());
                return None;
            }
        }

        let Some(target_inferred) = self.converted_expression_type(target) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        let OperandTypeClass::Pointer(pointer) = self.operand_type_class(target_inferred.ty) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        let Ok(target_integer) = self.canonical_integer_type(pointer.pointee) else {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        };
        if target_integer == CIntegerType::BOOL {
            self.report("E2913", "builtin-operand-not-convertible", target.span());
            return None;
        }

        let result_ty = self.intern(TypeKind::Int(IntRank::Bool), Qualifiers::default());
        let span = self.merge_span(keyword.span, close.span);
        Some(self.alloc_node(CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            target_ty: pointer.pointee,
            facts: ExprFacts::value(result_ty),
            span,
        }))
    }

    fn parse_static_assert_declaration(&mut self) -> Option<()> {
        let keyword = self.advance().expect("peeked static assertion");
        self.expect(TokenKind::LParen)?;
        let condition = self.parse_required_constant_conditional()?;
        let condition_span = condition.span();
        if self.match_kind(TokenKind::Comma) {
            self.expect(TokenKind::StringLiteral)?;
            while self.peek_kind() == Some(TokenKind::StringLiteral) {
                self.advance();
            }
        }
        self.expect(TokenKind::RParen)?;
        self.expect(TokenKind::Semicolon)?;
        match self.eval_integer_constant_expression(condition) {
            Ok(value) if value.truth() => Some(()),
            Ok(_) => {
                self.report("E2911", "static-assertion-failed", keyword.span);
                None
            }
            Err(()) => {
                self.report("E2912", "static-assertion-not-constant", condition_span);
                None
            }
        }
    }

    fn parse_generic_selection(&mut self, keyword: Token) -> Option<&'arena CHirExpr<'arena>> {
        self.expect(TokenKind::LParen)?;
        let controlling = self.parse_assignment()?;
        let controlling_span = controlling.span();
        let controlling_ty = self.generic_controlling_type(controlling).or_else(|| {
            self.report("E1003", "expected-expression", controlling_span);
            None
        })?;
        self.expect(TokenKind::Comma)?;

        let mut association_types = Vec::new();
        let mut default = None;
        let mut matching = None;
        let mut association_count = 0_usize;
        let mut duplicate = false;
        loop {
            let index = association_count;
            let association_start = self.span();
            let association_ty = if self.peek_kind() == Some(TokenKind::Default) {
                let token = self.advance().expect("peeked generic default");
                if default.is_some() {
                    self.report(
                        "E2907",
                        "generic-selection-duplicate-association",
                        token.span,
                    );
                    duplicate = true;
                }
                None
            } else {
                let (ty, tag_event) = self.parse_type_name()?;
                if association_types
                    .iter()
                    .any(|previous| self.types.compatible(*previous, ty))
                {
                    self.report(
                        "E2907",
                        "generic-selection-duplicate-association",
                        association_start,
                    );
                    duplicate = true;
                }
                association_types.push(ty);
                if let Some(event) = tag_event {
                    let declaration = self.finish_tag_event(event, association_start, Vec::new());
                    self.side_declarations.push(declaration);
                }
                Some(ty)
            };
            self.expect(TokenKind::Colon)?;
            let expression = self.parse_assignment()?;
            if let Some(association_ty) = association_ty {
                if self.types.compatible(controlling_ty, association_ty) {
                    matching = Some((index, expression));
                }
            } else if default.is_none() {
                default = Some((index, expression));
            }
            association_count += 1;
            if !self.match_kind(TokenKind::Comma) {
                break;
            }
        }
        let close = self.expect(TokenKind::RParen)?;
        if duplicate {
            return None;
        }
        let Some((selected_index, selected)) = matching.or(default) else {
            self.report("E2908", "generic-selection-no-match", keyword.span);
            return None;
        };
        self.generic_selections.push(GenericSelectionRecord {
            span: self.merge_span(keyword.span, close.span),
            controlling_span,
            selected_index,
            association_count,
        });
        Some(selected)
    }

    fn looks_like_parenthesized_type(&self) -> bool {
        self.peek_kind() == Some(TokenKind::LParen)
            && self
                .tokens
                .get(self.pos + 1)
                .is_some_and(|token| self.token_starts_type_name(token))
    }
}

fn merge_qualifiers(left: Qualifiers, right: Qualifiers) -> Qualifiers {
    Qualifiers {
        is_const: left.is_const || right.is_const,
        is_volatile: left.is_volatile || right.is_volatile,
        is_restrict: left.is_restrict || right.is_restrict,
        is_atomic: left.is_atomic || right.is_atomic,
        aligned: match (left.aligned, right.aligned) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        },
    }
}

fn extend_subobject_path(path: SubobjectPath, step: SubobjectStep) -> SubobjectPath {
    match step {
        SubobjectStep::Field { field_index } => path.with_field(field_index),
        SubobjectStep::Index { index } => path.with_index(index),
    }
}

fn is_integer_class(class: OperandTypeClass) -> bool {
    matches!(class, OperandTypeClass::Integer(_))
}

fn is_arithmetic_class(class: OperandTypeClass) -> bool {
    matches!(
        class,
        OperandTypeClass::Integer(_) | OperandTypeClass::Real(_) | OperandTypeClass::Complex(_)
    )
}

fn is_scalar_class(class: OperandTypeClass) -> bool {
    is_arithmetic_class(class)
        || matches!(
            class,
            OperandTypeClass::Pointer(_) | OperandTypeClass::StringPointer(_)
        )
}

fn floating_kind(class: OperandTypeClass) -> Option<FloatKind> {
    match class {
        OperandTypeClass::Real(kind) => Some(kind),
        OperandTypeClass::Complex(base) => Some(match base {
            ComplexBase::CF16 => FloatKind::F16,
            ComplexBase::CF32 => FloatKind::F32,
            ComplexBase::CF64 => FloatKind::F64,
            ComplexBase::CF128 => FloatKind::F128,
        }),
        OperandTypeClass::Integer(_)
        | OperandTypeClass::Pointer(_)
        | OperandTypeClass::StringPointer(_)
        | OperandTypeClass::Aggregate(_)
        | OperandTypeClass::Void
        | OperandTypeClass::Other => None,
    }
}

fn floating_rank(kind: FloatKind) -> u8 {
    match kind {
        FloatKind::F16 => 0,
        FloatKind::F32 => 1,
        FloatKind::F64 => 2,
        FloatKind::F128 => 3,
    }
}

fn widest_floating_kind(left: OperandTypeClass, right: OperandTypeClass) -> Option<FloatKind> {
    match (floating_kind(left), floating_kind(right)) {
        (Some(left), Some(right)) => Some(if floating_rank(left) >= floating_rank(right) {
            left
        } else {
            right
        }),
        (Some(kind), None) | (None, Some(kind)) => Some(kind),
        (None, None) => None,
    }
}

fn complex_base_for_float(kind: FloatKind) -> ComplexBase {
    match kind {
        FloatKind::F16 => ComplexBase::CF16,
        FloatKind::F32 => ComplexBase::CF32,
        FloatKind::F64 => ComplexBase::CF64,
        FloatKind::F128 => ComplexBase::CF128,
    }
}

fn qualifiers_include(target: Qualifiers, source: Qualifiers) -> bool {
    (!source.is_const || target.is_const)
        && (!source.is_volatile || target.is_volatile)
        && (!source.is_restrict || target.is_restrict)
        && (!source.is_atomic || target.is_atomic)
}

fn is_object_type_kind(kind: &TypeKind) -> bool {
    !matches!(kind, TypeKind::Void | TypeKind::Function { .. })
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

fn assignment_op(kind: TokenKind) -> Option<AssignmentOp> {
    match kind {
        TokenKind::Assign => Some(AssignmentOp::Simple),
        TokenKind::PlusEq => Some(AssignmentOp::Add),
        TokenKind::MinusEq => Some(AssignmentOp::Subtract),
        TokenKind::StarEq => Some(AssignmentOp::Multiply),
        TokenKind::SlashEq => Some(AssignmentOp::Divide),
        TokenKind::PercentEq => Some(AssignmentOp::Remainder),
        TokenKind::ShiftLeftEq => Some(AssignmentOp::ShiftLeft),
        TokenKind::ShiftRightEq => Some(AssignmentOp::ShiftRight),
        TokenKind::AmpEq => Some(AssignmentOp::BitAnd),
        TokenKind::CaretEq => Some(AssignmentOp::BitXor),
        TokenKind::PipeEq => Some(AssignmentOp::BitOr),
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
            | TokenKind::AmpAmp
            | TokenKind::Star
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::Sizeof
            | TokenKind::Alignof
            | TokenKind::GnuAlignof
            | TokenKind::BuiltinExpect
            | TokenKind::BuiltinConstantP
            | TokenKind::BuiltinChooseExpr
            | TokenKind::BuiltinTypesCompatibleP
            | TokenKind::BuiltinOffsetof
            | TokenKind::BuiltinObjectSize
            | TokenKind::BuiltinBit
            | TokenKind::BuiltinUnreachable
            | TokenKind::BuiltinReturnAddress
            | TokenKind::BuiltinAddOverflow
            | TokenKind::BuiltinSubOverflow
            | TokenKind::BuiltinMulOverflow
            | TokenKind::Generic
    )
}

/// Return symbolic references in a GNU asm template.
///
/// Besides `%[name]`, GCC permits a one-letter modifier such as `%l[name]`
/// for goto labels. `%%` is a quoted percent and cannot start a reference.
fn asm_template_symbolic_names(template: &str) -> Vec<&str> {
    let bytes = template.as_bytes();
    let mut names = Vec::new();
    let mut index = 0_usize;
    while index < bytes.len() {
        if bytes[index] != b'%' || index + 1 >= bytes.len() {
            index += 1;
            continue;
        }
        if bytes[index + 1] == b'%' {
            index += 2;
            continue;
        }
        let mut open = index + 1;
        if bytes[open].is_ascii_alphabetic()
            && bytes.get(open + 1).is_some_and(|byte| *byte == b'[')
        {
            open += 1;
        }
        if bytes[open] != b'[' {
            index += 1;
            continue;
        }
        let name_start = open + 1;
        let Some(relative_end) = bytes[name_start..].iter().position(|byte| *byte == b']') else {
            break;
        };
        let name_end = name_start + relative_end;
        if let Some(name) = template.get(name_start..name_end) {
            names.push(name);
        }
        index = name_end + 1;
    }
    names
}

fn collect_asm_goto_labels(
    statement: &CHirStmt<'_>,
    defined: &mut HashSet<String>,
    referenced: &mut Vec<SpannedName>,
) {
    match statement {
        CHirStmt::Asm(assembly) => referenced.extend(assembly.labels.iter().cloned()),
        CHirStmt::If(_, then_statement, else_statement, _) => {
            collect_asm_goto_labels(then_statement, defined, referenced);
            if let Some(else_statement) = else_statement {
                collect_asm_goto_labels(else_statement, defined, referenced);
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::DoWhile(body, _, _)
        | CHirStmt::For(_, _, _, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::CaseRange(_, _, body, _)
        | CHirStmt::Default(body, _) => collect_asm_goto_labels(body, defined, referenced),
        CHirStmt::Compound(items, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_asm_goto_labels(statement, defined, referenced);
                }
            }
        }
        CHirStmt::Label(label, body, _) => {
            defined.insert(label.spelling.clone());
            collect_asm_goto_labels(body, defined, referenced);
        }
        CHirStmt::Expr(_, _)
        | CHirStmt::Return(_, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::ComputedGoto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

#[derive(Debug)]
struct JumpRegionReference {
    name: String,
    span: SourceSpan,
    regions: Vec<u32>,
    checks_region_entry: bool,
}

fn collect_jump_regions_decl(
    declaration: &CHirDecl<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut u32,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match declaration {
        CHirDecl::Object(object) => {
            if let Some(initializer) = &object.initializer {
                for action in initializer.plan().actions() {
                    if let InitAction::StoreScalar { value, .. } = action {
                        collect_jump_regions_expr(value, regions, next_region, labels, gotos);
                    }
                }
            }
        }
        CHirDecl::Asm(_) | CHirDecl::Tag(_) | CHirDecl::Typedef(_) | CHirDecl::Function(_) => {}
    }
}

fn collect_jump_regions_stmt(
    statement: &CHirStmt<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut u32,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match statement {
        CHirStmt::Asm(assembly) => {
            gotos.extend(assembly.labels.iter().map(|label| JumpRegionReference {
                name: label.spelling.clone(),
                span: label.span,
                regions: regions.clone(),
                checks_region_entry: true,
            }));
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                collect_jump_regions_expr(operand.expression, regions, next_region, labels, gotos);
            }
        }
        CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
        }
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(then_statement, regions, next_region, labels, gotos);
            if let Some(else_statement) = else_statement {
                collect_jump_regions_stmt(else_statement, regions, next_region, labels, gotos);
            }
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            collect_jump_regions_expr(first, regions, next_region, labels, gotos);
            collect_jump_regions_expr(last, regions, next_region, labels, gotos);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::DoWhile(body, condition, _) => {
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => {
                    collect_jump_regions_expr(expression, regions, next_region, labels, gotos);
                }
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        collect_jump_regions_decl(declaration, regions, next_region, labels, gotos);
                    }
                }
            }
            if let Some(condition) = condition {
                collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            }
            if let Some(step) = step {
                collect_jump_regions_expr(step, regions, next_region, labels, gotos);
            }
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        collect_jump_regions_decl(declaration, regions, next_region, labels, gotos)
                    }
                    CHirBlockItem::Statement(statement) => {
                        collect_jump_regions_stmt(statement, regions, next_region, labels, gotos)
                    }
                }
            }
        }
        CHirStmt::Label(label, body, _) => {
            labels.insert(label.spelling.clone(), regions.clone());
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Default(body, _) => {
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
        }
        CHirStmt::Goto(label, _) => gotos.push(JumpRegionReference {
            name: label.spelling.clone(),
            span: label.span,
            regions: regions.clone(),
            checks_region_entry: true,
        }),
        CHirStmt::ComputedGoto(target, _) => {
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirStmt::Return(None, _)
        | CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => {}
    }
}

fn collect_jump_regions_expr(
    expression: &CHirExpr<'_>,
    regions: &mut Vec<u32>,
    next_region: &mut u32,
    labels: &mut HashMap<String, Vec<u32>>,
    gotos: &mut Vec<JumpRegionReference>,
) {
    match expression {
        CHirExpr::LabelAddress { label, .. } => gotos.push(JumpRegionReference {
            name: label.spelling.clone(),
            span: label.span,
            regions: regions.clone(),
            checks_region_entry: false,
        }),
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::SizeOfExpr(operand, _, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => {
            collect_jump_regions_expr(operand, regions, next_region, labels, gotos);
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            collect_jump_regions_expr(left, regions, next_region, labels, gotos);
            collect_jump_regions_expr(right, regions, next_region, labels, gotos);
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            collect_jump_regions_expr(condition, regions, next_region, labels, gotos);
            collect_jump_regions_expr(when_true, regions, next_region, labels, gotos);
            collect_jump_regions_expr(when_false, regions, next_region, labels, gotos);
        }
        CHirExpr::Call(callee, arguments, _) => {
            collect_jump_regions_expr(callee, regions, next_region, labels, gotos);
            for argument in arguments {
                collect_jump_regions_expr(argument, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            collect_jump_regions_expr(lhs, regions, next_region, labels, gotos);
            collect_jump_regions_expr(rhs, regions, next_region, labels, gotos);
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirExpr::Assign { target, value, .. } => {
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
            collect_jump_regions_expr(value, regions, next_region, labels, gotos);
        }
        CHirExpr::Update { target, .. } => {
            collect_jump_regions_expr(target, regions, next_region, labels, gotos);
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
            for source in place.source_expressions() {
                collect_jump_regions_expr(source, regions, next_region, labels, gotos);
            }
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                collect_jump_regions_expr(source, regions, next_region, labels, gotos);
            }
            collect_jump_regions_expr(value, regions, next_region, labels, gotos);
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. } = action {
                    collect_jump_regions_expr(value, regions, next_region, labels, gotos);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => {
            let region = *next_region;
            *next_region = next_region.saturating_add(1);
            regions.push(region);
            collect_jump_regions_stmt(body, regions, next_region, labels, gotos);
            regions.pop();
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::Ident { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. } => {}
    }
}

fn written_initializer_references_symbol(
    initializer: &WrittenInitializer<'_>,
    symbol: SymbolId,
) -> bool {
    match initializer {
        WrittenInitializer::Expression(expression) => {
            expression_references_symbol(expression, symbol)
        }
        WrittenInitializer::List {
            elements,
            designations,
            ..
        } => {
            elements
                .iter()
                .any(|element| written_initializer_references_symbol(element, symbol))
                || designations.iter().flatten().any(|designation| {
                    designation
                        .designators
                        .iter()
                        .any(|designator| match designator {
                            WrittenDesignator::Field(_) => false,
                            WrittenDesignator::Index { expression, .. } => {
                                expression_references_symbol(expression, symbol)
                            }
                            WrittenDesignator::Range { first, last, .. } => {
                                expression_references_symbol(first, symbol)
                                    || expression_references_symbol(last, symbol)
                            }
                        })
                })
        }
    }
}

fn declaration_references_symbol(declaration: &CHirDecl<'_>, symbol: SymbolId) -> bool {
    match declaration {
        CHirDecl::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_references_symbol(operand.expression, symbol)),
        CHirDecl::Object(object) => object.initializer.as_ref().is_some_and(|initializer| {
            written_initializer_references_symbol(initializer.written(), symbol)
        }),
        CHirDecl::Function(function) => function
            .body
            .is_some_and(|body| statement_references_symbol(body, symbol)),
        CHirDecl::Tag(tag) => match &tag.members {
            TagMembers::Incomplete => false,
            TagMembers::Struct(members) | TagMembers::Union(members) => {
                members.iter().any(|member| {
                    member
                        .bit_width
                        .is_some_and(|width| expression_references_symbol(width, symbol))
                })
            }
            TagMembers::Enum(enumerators) => enumerators.iter().any(|enumerator| {
                enumerator
                    .written_value
                    .is_some_and(|value| expression_references_symbol(value, symbol))
            }),
        },
        CHirDecl::Typedef(_) => false,
    }
}

fn statement_references_symbol(statement: &CHirStmt<'_>, symbol: SymbolId) -> bool {
    match statement {
        CHirStmt::Asm(assembly) => assembly
            .outputs
            .iter()
            .chain(&assembly.inputs)
            .any(|operand| expression_references_symbol(operand.expression, symbol)),
        CHirStmt::Expr(expression, _) => expression_references_symbol(expression, symbol),
        CHirStmt::If(condition, then_statement, else_statement, _) => {
            expression_references_symbol(condition, symbol)
                || statement_references_symbol(then_statement, symbol)
                || else_statement
                    .is_some_and(|statement| statement_references_symbol(statement, symbol))
        }
        CHirStmt::Switch(expression, body, _)
        | CHirStmt::While(expression, body, _)
        | CHirStmt::Case(expression, body, _) => {
            expression_references_symbol(expression, symbol)
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::DoWhile(body, condition, _) => {
            statement_references_symbol(body, symbol)
                || expression_references_symbol(condition, symbol)
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer_references = match initializer {
                CHirForInit::Empty => false,
                CHirForInit::Expression(expression) => {
                    expression_references_symbol(expression, symbol)
                }
                CHirForInit::Declaration(declarations) => declarations
                    .iter()
                    .any(|declaration| declaration_references_symbol(declaration, symbol)),
            };
            initializer_references
                || condition
                    .is_some_and(|expression| expression_references_symbol(expression, symbol))
                || step.is_some_and(|expression| expression_references_symbol(expression, symbol))
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::Return(expression, _) => {
            expression.is_some_and(|expression| expression_references_symbol(expression, symbol))
        }
        CHirStmt::ComputedGoto(target, _) => expression_references_symbol(target, symbol),
        CHirStmt::Compound(items, _) => items.iter().any(|item| match item {
            CHirBlockItem::Declaration(declaration) => {
                declaration_references_symbol(declaration, symbol)
            }
            CHirBlockItem::Statement(statement) => statement_references_symbol(statement, symbol),
        }),
        CHirStmt::Label(_, body, _) | CHirStmt::Default(body, _) => {
            statement_references_symbol(body, symbol)
        }
        CHirStmt::CaseRange(first, last, body, _) => {
            expression_references_symbol(first, symbol)
                || expression_references_symbol(last, symbol)
                || statement_references_symbol(body, symbol)
        }
        CHirStmt::Break(_)
        | CHirStmt::Continue(_)
        | CHirStmt::Goto(_, _)
        | CHirStmt::AttributedNull(_, _)
        | CHirStmt::Null(_) => false,
    }
}

fn place_references_symbol(place: &MemoryPlace<'_>, symbol: SymbolId) -> bool {
    place.root_symbol() == Some(symbol)
        || matches!(
            place.root(),
            PlaceRoot::CompoundLiteral { expression, .. }
                if expression_references_symbol(expression, symbol)
        )
        || place
            .source_expressions()
            .any(|expression| expression_references_symbol(expression, symbol))
}

fn expression_references_symbol(expression: &CHirExpr<'_>, symbol: SymbolId) -> bool {
    match expression {
        CHirExpr::Ident {
            symbol: referenced, ..
        } => *referenced == symbol,
        CHirExpr::Unary(_, operand, _)
        | CHirExpr::EvaluateOnce(operand, _)
        | CHirExpr::Cast(_, operand, _)
        | CHirExpr::SizeOfExpr(operand, _, _)
        | CHirExpr::BitBuiltin { operand, .. }
        | CHirExpr::Member(operand, _, _)
        | CHirExpr::PtrMember(operand, _, _) => expression_references_symbol(operand, symbol),
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _)
        | CHirExpr::Elvis(left, right, _) => {
            expression_references_symbol(left, symbol)
                || expression_references_symbol(right, symbol)
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            expression_references_symbol(condition, symbol)
                || expression_references_symbol(when_true, symbol)
                || expression_references_symbol(when_false, symbol)
        }
        CHirExpr::Call(callee, arguments, _) => {
            expression_references_symbol(callee, symbol)
                || arguments
                    .iter()
                    .any(|argument| expression_references_symbol(argument, symbol))
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            expression_references_symbol(lhs, symbol)
                || expression_references_symbol(rhs, symbol)
                || expression_references_symbol(target, symbol)
        }
        CHirExpr::Assign { target, value, .. } => {
            expression_references_symbol(target, symbol)
                || expression_references_symbol(value, symbol)
        }
        CHirExpr::Update { target, .. } => expression_references_symbol(target, symbol),
        CHirExpr::MemoryAssign { place, value, .. } => {
            place_references_symbol(place, symbol) || expression_references_symbol(value, symbol)
        }
        CHirExpr::MemoryRead { place, .. } | CHirExpr::MemoryUpdate { place, .. } => {
            place_references_symbol(place, symbol)
        }
        CHirExpr::StatementExpr { body, .. } => statement_references_symbol(body, symbol),
        CHirExpr::CompoundLit(_, initializer, _) => {
            written_initializer_references_symbol(initializer.written(), symbol)
        }
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::LabelAddress { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. } => false,
    }
}

fn is_builtin_type_word(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Void
            | TokenKind::BuiltinVaList
            | TokenKind::GnuInt128
            | TokenKind::Bool
            | TokenKind::Char
            | TokenKind::Short
            | TokenKind::Int
            | TokenKind::Long
            | TokenKind::Signed
            | TokenKind::Unsigned
            | TokenKind::Float
            | TokenKind::Double
            | TokenKind::Complex
    )
}

#[derive(Debug, Clone, Copy)]
struct BuiltinTypeWord {
    kind: TokenKind,
    forced_int128_signedness: Option<GnuInt128Signedness>,
}

/// Resolve the order-independent builtin declaration-specifier multiset.
///
/// C permits combinations such as `unsigned const long int`; consuming the
/// words one at a time as complete types loses that grammar. This resolver
/// accepts exactly the standard integer, real, and complex families and
/// rejects contradictory or duplicate words.
fn resolve_builtin_type(words: &[BuiltinTypeWord]) -> Option<TypeKind> {
    let count = |needle| words.iter().filter(|word| word.kind == needle).count();
    let void = count(TokenKind::Void);
    let builtin_va_list = count(TokenKind::BuiltinVaList);
    let gnu_int128 = count(TokenKind::GnuInt128);
    let boolean = count(TokenKind::Bool);
    let character = count(TokenKind::Char);
    let short = count(TokenKind::Short);
    let integer = count(TokenKind::Int);
    let long = count(TokenKind::Long);
    let signed = count(TokenKind::Signed);
    let unsigned = count(TokenKind::Unsigned);
    let float = count(TokenKind::Float);
    let double = count(TokenKind::Double);
    let complex = count(TokenKind::Complex);

    if void > 1
        || builtin_va_list > 1
        || gnu_int128 > 1
        || boolean > 1
        || character > 1
        || short > 1
        || integer > 1
        || long > 2
        || signed > 1
        || unsigned > 1
        || float > 1
        || double > 1
        || complex > 1
        || (signed != 0 && unsigned != 0)
    {
        return None;
    }

    let total = words.len();
    if builtin_va_list == 1 {
        return (total == 1).then_some(TypeKind::BuiltinVaList);
    }
    if gnu_int128 == 1 {
        let forced_signedness = words.iter().find_map(|word| word.forced_int128_signedness);
        if forced_signedness.is_some() && total != 1 {
            return None;
        }
        if forced_signedness.is_none() && total != 1 + signed + unsigned {
            return None;
        }
        return Some(TypeKind::GnuInt128(forced_signedness.unwrap_or({
            if unsigned == 1 {
                GnuInt128Signedness::Unsigned
            } else {
                GnuInt128Signedness::Signed
            }
        })));
    }
    if void == 1 {
        return (total == 1).then_some(TypeKind::Void);
    }
    if boolean == 1 {
        return (total == 1).then_some(TypeKind::Int(IntRank::Bool));
    }

    if complex == 1 {
        if signed != 0
            || unsigned != 0
            || short != 0
            || integer != 0
            || character != 0
            || boolean != 0
            || void != 0
        {
            return None;
        }
        return match (float, double, long) {
            (1, 0, 0) => Some(TypeKind::Complex(ComplexBase::CF32)),
            (0, 1, 0) => Some(TypeKind::Complex(ComplexBase::CF64)),
            (0, 1, 1) => Some(TypeKind::Complex(ComplexBase::CF128)),
            _ => None,
        };
    }

    if float == 1 {
        return (total == 1).then_some(TypeKind::Float(FloatKind::F32));
    }
    if double == 1 {
        if signed != 0 || unsigned != 0 || short != 0 || integer != 0 || character != 0 {
            return None;
        }
        return match long {
            0 => Some(TypeKind::Float(FloatKind::F64)),
            1 => Some(TypeKind::Float(FloatKind::F128)),
            _ => None,
        };
    }
    if character == 1 {
        if short != 0 || long != 0 || integer != 0 {
            return None;
        }
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UChar
        } else if signed == 1 {
            IntRank::SChar
        } else {
            IntRank::Char
        }));
    }
    if short == 1 {
        if long != 0 || character != 0 {
            return None;
        }
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UShort
        } else {
            IntRank::Short
        }));
    }
    if long != 0 {
        if short != 0 || character != 0 {
            return None;
        }
        return Some(TypeKind::Int(match (unsigned, long) {
            (1, 1) => IntRank::ULong,
            (1, 2) => IntRank::ULongLong,
            (0, 1) => IntRank::Long,
            (0, 2) => IntRank::LongLong,
            _ => return None,
        }));
    }
    if integer <= 1 && signed <= 1 && unsigned <= 1 && total != 0 {
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UInt
        } else {
            IntRank::Int
        }));
    }
    None
}

// ── integer literal parsing ────────────────────────────────────────────────

fn parse_int_literal(text: &str) -> Option<IntLit> {
    let (base, radix, prefix_len) = if text.starts_with("0x") || text.starts_with("0X") {
        (IntBase::Hex, 16, 2)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        (IntBase::Bin, 2, 2)
    } else if text.starts_with('0') && text.len() > 1 {
        (IntBase::Oct, 8, 1)
    } else {
        (IntBase::Dec, 10, 0)
    };

    let digit_count = text[prefix_len..]
        .bytes()
        .take_while(|byte| match radix {
            2 => matches!(byte, b'0' | b'1'),
            8 => matches!(byte, b'0'..=b'7'),
            10 => byte.is_ascii_digit(),
            16 => byte.is_ascii_hexdigit(),
            _ => unreachable!("known integer literal radix"),
        })
        .count();
    if digit_count == 0 && prefix_len != 1 {
        return None;
    }
    let digit_end = prefix_len + digit_count;
    let digits = if prefix_len == 1 && digit_count == 0 {
        "0"
    } else {
        &text[prefix_len..digit_end]
    };
    let suffix = match text[digit_end..].to_ascii_lowercase().as_str() {
        "" => IntSuffix::None,
        "u" => IntSuffix::U,
        "l" => IntSuffix::L,
        "ul" | "lu" => IntSuffix::UL,
        "ll" => IntSuffix::LL,
        "ull" | "llu" => IntSuffix::ULL,
        _ => return None,
    };
    Some(IntLit {
        value: u64::from_str_radix(digits, radix).ok()?,
        suffix,
        base,
    })
}

fn parse_float_literal(text: &str) -> Option<FloatLit> {
    let (number, suffix) = match text.as_bytes().last().copied() {
        Some(b'f' | b'F') => (&text[..text.len() - 1], FloatSuffix::F),
        Some(b'l' | b'L') => (&text[..text.len() - 1], FloatSuffix::L),
        _ => (text, FloatSuffix::None),
    };
    Some(FloatLit {
        value: number.parse().ok()?,
        suffix,
    })
}

fn parse_string_literal(text: &str) -> Option<StringLit> {
    let (quoted, prefix) = if let Some(quoted) = text.strip_prefix("u8") {
        (quoted, StringPrefix::U8)
    } else if let Some(quoted) = text.strip_prefix('u') {
        (quoted, StringPrefix::U16)
    } else if let Some(quoted) = text.strip_prefix('U') {
        (quoted, StringPrefix::U)
    } else if let Some(quoted) = text.strip_prefix('L') {
        (quoted, StringPrefix::L)
    } else {
        (text, StringPrefix::None)
    };
    let contents = quoted.strip_prefix('"')?.strip_suffix('"')?;
    Some(StringLit::single(contents, prefix))
}

fn concatenate_string_prefix(left: StringPrefix, right: StringPrefix) -> Option<StringPrefix> {
    match (left, right) {
        (left, right) if left == right => Some(left),
        (StringPrefix::None, right) => Some(right),
        (left, StringPrefix::None) => Some(left),
        _ => None,
    }
}

fn parse_char_literal(text: &str) -> Option<CharLit> {
    let (quoted, prefix) = if let Some(quoted) = text.strip_prefix("u8") {
        (quoted, CharPrefix::U8)
    } else if let Some(quoted) = text.strip_prefix('u') {
        (quoted, CharPrefix::U)
    } else if let Some(quoted) = text.strip_prefix('U') {
        (quoted, CharPrefix::U)
    } else if let Some(quoted) = text.strip_prefix('L') {
        (quoted, CharPrefix::L)
    } else {
        (text, CharPrefix::None)
    };
    let contents = quoted.strip_prefix('\'')?.strip_suffix('\'')?;
    let value = if let Some(escape) = contents.strip_prefix('\\') {
        match escape {
            "'" => u32::from(b'\''),
            "\"" => u32::from(b'\"'),
            "?" => u32::from(b'?'),
            "\\" => u32::from(b'\\'),
            "a" => 7,
            "b" => 8,
            "f" => 12,
            "n" => 10,
            "r" => 13,
            "t" => 9,
            "v" => 11,
            _ if escape.starts_with('x') => {
                let digits = &escape[1..];
                if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                u32::from_str_radix(digits, 16).ok()?
            }
            _ if escape.starts_with('u') && escape.len() == 5 => {
                u32::from_str_radix(&escape[1..], 16).ok()?
            }
            _ if escape.starts_with('U') && escape.len() == 9 => {
                u32::from_str_radix(&escape[1..], 16).ok()?
            }
            _ if escape.len() <= 3 && escape.bytes().all(|byte| matches!(byte, b'0'..=b'7')) => {
                u32::from_str_radix(escape, 8).ok()?
            }
            _ => return None,
        }
    } else {
        let mut characters = contents.chars();
        let value = u32::from(characters.next()?);
        if characters.next().is_some() {
            return None;
        }
        value
    };
    if matches!(prefix, CharPrefix::None | CharPrefix::U8) && value > u32::from(u8::MAX) {
        return None;
    }
    char::from_u32(value)?;
    Some(CharLit { value, prefix })
}

/// When a parenthesized expression appears, preserve its original span.
fn respan_expr<'arena>(inner: &CHirExpr<'arena>, paren_span: SourceSpan) -> CHirExpr<'arena> {
    match inner {
        CHirExpr::Lit(literal, _) => CHirExpr::Lit(*literal, paren_span),
        CHirExpr::MemoryRead {
            place,
            target_span,
            facts,
            ..
        } => CHirExpr::MemoryRead {
            place: place.clone(),
            target_span: *target_span,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::FloatLit(literal, _) => CHirExpr::FloatLit(*literal, paren_span),
        CHirExpr::CharLit(literal, _) => CHirExpr::CharLit(*literal, paren_span),
        CHirExpr::StringLit(literal, _) => CHirExpr::StringLit(literal.clone(), paren_span),
        CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            ..
        } => CHirExpr::Ident {
            symbol: *symbol,
            spelling: spelling.clone(),
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Unary(op, operand, _) => CHirExpr::Unary(*op, operand, paren_span),
        CHirExpr::EvaluateOnce(operand, _) => CHirExpr::EvaluateOnce(operand, paren_span),
        CHirExpr::Binary(op, lhs, rhs, _) => CHirExpr::Binary(*op, lhs, rhs, paren_span),
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            CHirExpr::Ternary(condition, when_true, when_false, paren_span)
        }
        CHirExpr::Elvis(operand, fallback, _) => CHirExpr::Elvis(operand, fallback, paren_span),
        CHirExpr::Call(callee, arguments, _) => {
            CHirExpr::Call(callee, arguments.clone(), paren_span)
        }
        CHirExpr::CheckedArithmetic {
            op,
            lhs,
            rhs,
            target,
            target_ty,
            facts,
            ..
        } => CHirExpr::CheckedArithmetic {
            op: *op,
            lhs,
            rhs,
            target,
            target_ty: *target_ty,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BitBuiltin {
            op, operand, facts, ..
        } => CHirExpr::BitBuiltin {
            op: *op,
            operand,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinUnreachable { facts, .. } => CHirExpr::BuiltinUnreachable {
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::BuiltinReturnAddress { facts, .. } => CHirExpr::BuiltinReturnAddress {
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::LabelAddress { label, facts, .. } => CHirExpr::LabelAddress {
            label: label.clone(),
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Cast(ty, operand, _) => CHirExpr::Cast(*ty, operand, paren_span),
        CHirExpr::SizeOf(ty, _) => CHirExpr::SizeOf(*ty, paren_span),
        CHirExpr::SizeOfExpr(operand, ty, _) => CHirExpr::SizeOfExpr(operand, *ty, paren_span),
        CHirExpr::AlignOf(ty, _) => CHirExpr::AlignOf(*ty, paren_span),
        CHirExpr::Member(object, name, _) => CHirExpr::Member(object, name.clone(), paren_span),
        CHirExpr::PtrMember(object, name, _) => {
            CHirExpr::PtrMember(object, name.clone(), paren_span)
        }
        CHirExpr::Index(object, index, _) => CHirExpr::Index(object, index, paren_span),
        CHirExpr::Assign {
            op,
            place,
            target,
            value,
            facts,
            ..
        } => CHirExpr::Assign {
            op: *op,
            place: *place,
            target,
            value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::MemoryAssign {
            op,
            place,
            target_span,
            value,
            facts,
            ..
        } => CHirExpr::MemoryAssign {
            op: *op,
            place: place.clone(),
            target_span: *target_span,
            value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Update {
            op,
            place,
            target,
            facts,
            ..
        } => CHirExpr::Update {
            op: *op,
            place: *place,
            target,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::MemoryUpdate {
            op,
            place,
            target_span,
            facts,
            ..
        } => CHirExpr::MemoryUpdate {
            op: *op,
            place: place.clone(),
            target_span: *target_span,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::CompoundLit(ty, values, _) => {
            CHirExpr::CompoundLit(*ty, values.clone(), paren_span)
        }
        CHirExpr::StatementExpr {
            body, value, facts, ..
        } => CHirExpr::StatementExpr {
            body,
            value: *value,
            facts: *facts,
            span: paren_span,
        },
        CHirExpr::Comma(lhs, rhs, _) => CHirExpr::Comma(lhs, rhs, paren_span),
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
        object
            .initializer
            .as_ref()
            .expect("expected initializer")
            .written()
            .expression()
            .expect("expression initializer")
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

    fn diagnostic_text<'src>(source: &'src str, diagnostic: &ParseDiagnostic) -> &'src str {
        &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize]
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
    fn gnu_empty_initializer_completes_an_unbounded_array() {
        let arena = Arena::default();
        let unit = parse_unit("int inferred[] = {};", &arena);
        for declaration in unit.declarations() {
            let CHirDecl::Object(object) = declaration else {
                panic!("expected object declaration")
            };
            assert!(matches!(
                unit.types().get(object.ty).map(|ty| &ty.kind),
                Some(TypeKind::Array {
                    bound: ArrayBound::Fixed(0),
                    ..
                })
            ));
            assert_eq!(
                unit.symbols().symbol(object.symbol).unwrap().ty(),
                object.ty
            );
            let initializer = object
                .initializer
                .as_ref()
                .expect("written empty initializer");
            assert!(initializer
                .written()
                .elements()
                .is_some_and(|elements| elements.is_empty()));
            assert!(initializer.plan().is_empty());
            initializer.plan().validate(unit.types()).unwrap();
        }
    }

    #[test]
    fn a_written_zero_bound_remains_fixed_with_an_empty_initializer() {
        let arena = Arena::default();
        let unit = parse_unit("int fixed[0] = {};", &arena);
        let CHirDecl::Object(object) = unit.declarations()[0] else {
            panic!("expected zero-length object declaration")
        };
        assert!(matches!(
            unit.types().get(object.ty).map(|ty| &ty.kind),
            Some(TypeKind::Array {
                bound: ArrayBound::Fixed(0),
                ..
            })
        ));
        assert!(object
            .initializer
            .as_ref()
            .expect("written empty initializer")
            .plan()
            .is_empty());
    }

    #[test]
    fn initialized_integer_constant_array_bounds_remain_fixed() {
        let arena = Arena::default();
        let unit = parse_unit(
            "enum { N = 2 }; int sized[sizeof(int)] = { 1 }; \
             int named[N] = { 1 }; int folded[1 + 1] = { 1 };",
            &arena,
        );
        let bounds = unit
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                CHirDecl::Object(object) => match &unit.types().get(object.ty)?.kind {
                    TypeKind::Array { bound, .. } => Some(bound.clone()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            bounds,
            [
                ArrayBound::Fixed(4),
                ArrayBound::Fixed(2),
                ArrayBound::Fixed(2),
            ]
        );

        let arena = Arena::default();
        let source = "int f(int n) { int values[2][n] = { { 1 } }; return 0; }";
        let (_, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2203");
        assert_eq!(diagnostic.message, "variable-length-object");
        assert_eq!(
            &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            "{ { 1 } }"
        );
    }

    #[test]
    fn adjacent_string_fragments_and_ucns_infer_execution_unit_bounds() {
        let arena = Arena::default();
        let source = r#"char split[] = "\x1" "2"; char ucn[] = "\u00e9";"#;
        let unit = parse_unit(source, &arena);
        for (declaration, expected_value, expected_fragments) in [
            (unit.declarations()[0], "\\x12", vec!["\\x1", "2"]),
            (unit.declarations()[1], "\\u00e9", vec!["\\u00e9"]),
        ] {
            let CHirDecl::Object(object) = declaration else {
                panic!("expected character array")
            };
            assert!(matches!(
                unit.types().get(object.ty).map(|ty| &ty.kind),
                Some(TypeKind::Array {
                    bound: ArrayBound::Fixed(3),
                    ..
                })
            ));
            let expression = object
                .initializer
                .as_ref()
                .expect("string initializer")
                .written()
                .expression()
                .expect("string expression");
            let CHirExpr::StringLit(literal, _) = expression else {
                panic!("expected string literal")
            };
            assert_eq!(literal.value, expected_value);
            assert_eq!(
                literal.fragments,
                expected_fragments
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            );
            assert_eq!(literal.explicit_unit_count(), Some(2));
        }
    }

    #[test]
    fn malformed_string_units_fail_with_a_literal_diagnostic() {
        let arena = Arena::default();
        for source in [
            r#"char value[] = "\x";"#,
            r#"char value[] = "\u0061";"#,
            r#"char value[] = "\uD800";"#,
        ] {
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert!(unit.is_empty());
            assert_eq!(diagnostic.code, "E1006");
            assert_eq!(diagnostic.message, "invalid-string-literal");
            assert_eq!(
                &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
                source
                    .split('=')
                    .nth(1)
                    .unwrap()
                    .trim_end_matches(';')
                    .trim(),
            );
        }
    }

    #[test]
    fn forbidden_typedef_and_block_extern_initializers_rollback_bindings() {
        for (source, rejected) in [
            ("int keep; typedef int bad = 1; long after;", "bad"),
            (
                "int keep; int bad(void) { extern int local = 1; return local; } long after;",
                "bad",
            ),
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, "E2202");
            assert_eq!(diagnostic.message, "invalid-initializer-type");
            assert_eq!(
                &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
                "1"
            );
            assert_eq!(
                unit.symbols()
                    .symbols_in_source_order()
                    .iter()
                    .map(|symbol| symbol.spelling())
                    .collect::<Vec<_>>(),
                ["keep", "after"]
            );
            assert_eq!(
                unit.symbols()
                    .resolve(unit.symbols().file_scope(), rejected),
                Ok(None)
            );
        }

        let arena = Arena::default();
        let unit = parse_unit("extern int definition = 1;", &arena);
        let CHirDecl::Object(object) = unit.declarations()[0] else {
            panic!("expected file-scope extern definition")
        };
        assert_eq!(object.storage, StorageClass::Extern);
        assert!(object.initializer.is_some());
    }

    #[test]
    fn static_storage_initializers_validate_the_source_constant_expression() {
        let arena = Arena::default();
        let unit = parse_unit(
            "int anchor; int scalar = 1.5; enum E { A }; enum E e = 1.5; \
             _Bool truth = &anchor; int *address = (int *)1; \
             double maybe(void); double dead = 1 ? 2.0 : maybe(); \
             int ordinary; double shorted = 0 && ordinary; int folded = 1 + 2; \
             int local(int n) { static int stable = 1 + 2; int automatic = n; return automatic; }",
            &arena,
        );
        assert!(unit.declarations().len() >= 10);

        for (source, offending) in [
            ("int keep; int value; int bad = value; long after;", "value"),
            (
                "int keep; int call(void); int bad = call(); long after;",
                "call()",
            ),
            (
                "int keep; int bad(int n) { static int local = n; return local; } long after;",
                "n",
            ),
            (
                "int keep; _Thread_local int tls; int *bad = &tls; long after;",
                "&tls",
            ),
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, "E2208");
            assert_eq!(diagnostic.message, "non-constant-static-initializer");
            assert_eq!(diagnostic_text(source, &diagnostic), offending);
            assert_eq!(
                unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
                Ok(None)
            );
            assert!(unit
                .symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn storage_context_restrictions_are_transactional() {
        for (source, offending) in [
            ("int keep; auto int bad = 1; long after;", "bad"),
            (
                "int keep; int bad(void) { _Thread_local int local = 1; return local; } long after;",
                "local",
            ),
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, "E2002");
            assert_eq!(diagnostic_text(source, &diagnostic), offending);
            assert_eq!(
                unit.symbols()
                    .resolve(unit.symbols().file_scope(), "bad"),
                Ok(None)
            );
            assert!(unit
                .symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap()
                .is_some());
        }

        let arena = Arena::default();
        let unit = parse_unit(
            "_Thread_local int tls = 1; extern int definition = 1;",
            &arena,
        );
        assert_eq!(unit.declarations().len(), 2);
    }

    #[test]
    fn initializer_uses_the_effective_composite_redeclaration_type() {
        let arena = Arena::default();
        let unit = parse_unit(
            "extern int values[3]; int values[] = { 1, 2 }; \
             extern int other[3]; extern int other[];",
            &arena,
        );
        let values = unit
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                CHirDecl::Object(object) if object.name.spelling == "values" => Some(object),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(values.len(), 2);
        assert_eq!(values[0].symbol, values[1].symbol);
        assert_eq!(values[0].ty, values[1].ty);
        assert!(matches!(
            unit.types().get(values[1].ty).map(|ty| &ty.kind),
            Some(TypeKind::Array {
                bound: ArrayBound::Fixed(3),
                ..
            })
        ));
        let plan = values[1].initializer.as_ref().unwrap().plan();
        assert_eq!(plan.actions().len(), 3);
        assert!(matches!(plan.actions()[2], InitAction::ZeroScalar { .. }));

        let other = unit
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                CHirDecl::Object(object) if object.name.spelling == "other" => Some(object),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(other.len(), 2);
        assert_eq!(other[0].symbol, other[1].symbol);
        assert_eq!(other[0].ty, other[1].ty);

        for (source, code, offending) in [
            (
                "extern int values[2]; int values[] = { 1, 2, 3 }; long after;",
                "E2201",
                "3",
            ),
            (
                "extern char text[2]; char text[] = \"abc\"; long after;",
                "E2204",
                "\"abc\"",
            ),
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, code);
            assert_eq!(diagnostic_text(source, &diagnostic), offending);
            let symbol = unit
                .symbols()
                .resolve(
                    unit.symbols().file_scope(),
                    if code == "E2201" { "values" } else { "text" },
                )
                .unwrap()
                .expect("the prior complete declaration remains");
            assert!(matches!(
                unit.types()
                    .get(unit.symbols().symbol(symbol).unwrap().ty())
                    .map(|ty| &ty.kind),
                Some(TypeKind::Array {
                    bound: ArrayBound::Fixed(2),
                    ..
                })
            ));
            assert!(unit
                .symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn initializer_plan_budget_counts_actions_and_keeps_string_stores_compact() {
        let arena = Arena::default();
        let unit = parse_unit(
            "char direct[1000000] = \"x\"; \
             struct S { char text[1000000]; int tail; }; \
             struct S nested = { \"x\", 1 }; \
             char rows[][1000000] = { \"x\" };",
            &arena,
        );
        let action_counts = unit
            .declarations()
            .iter()
            .filter_map(|declaration| match declaration {
                CHirDecl::Object(object) => object
                    .initializer
                    .as_ref()
                    .map(|initializer| initializer.plan().actions().len()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(action_counts, [1, 2, 1]);

        for source in [
            "int keep; int huge[65537] = { 1 }; long after;",
            "int keep; int huge[][18446744073709551615ULL] = { 1 }; long after;",
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, "E2207");
            assert_eq!(diagnostic.message, "initializer-plan-too-large");
            assert_eq!(diagnostic_text(source, &diagnostic), "{ 1 }");
            assert_eq!(
                unit.symbols().resolve(unit.symbols().file_scope(), "huge"),
                Ok(None)
            );
            assert!(unit
                .symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap()
                .is_some());
        }
    }

    #[test]
    fn record_completion_rejects_nonfinal_flexible_and_recursive_members() {
        for (source, offending) in [
            (
                "struct Bad { int values[]; int tail; }; long after;",
                "values[]",
            ),
            (
                "union Bad { int value; union Bad nested; }; long after;",
                "nested",
            ),
        ] {
            let arena = Arena::default();
            let (unit, diagnostic) = parse_rejected(source, &arena);
            assert_eq!(diagnostic.code, "E1005");
            assert_eq!(diagnostic.message, "invalid-declarator");
            assert_eq!(diagnostic_text(source, &diagnostic), offending);
            assert!(unit
                .symbols()
                .resolve(unit.symbols().file_scope(), "after")
                .unwrap()
                .is_some());
        }

        let arena = Arena::default();
        let unit = parse_unit(
            "struct Flexible { int values[]; }; \
             union GNUFlexible { int values[]; int scalar; }; \
             struct Clean { int; int value; }; struct Clean clean = { 7 };",
            &arena,
        );
        let clean = unit
            .declarations()
            .iter()
            .find_map(|declaration| match declaration {
                CHirDecl::Object(object) if object.name.spelling == "clean" => Some(object),
                _ => None,
            })
            .unwrap();
        let TypeKind::Struct(tag) = unit.types().get(clean.ty).unwrap().kind else {
            panic!("expected struct object")
        };
        let TagState::Complete(TagDefinition::Struct(fields)) =
            &unit.types().tag(tag).unwrap().state
        else {
            panic!("expected complete struct")
        };
        assert_eq!(
            fields.len(),
            1,
            "empty builtin member declaration leaked a field"
        );
        assert_eq!(
            clean.initializer.as_ref().unwrap().plan().actions()[0]
                .path()
                .steps(),
            [SubobjectStep::Field { field_index: 0 }]
        );

        let arena = Arena::default();
        let source = "union Flexible { int values[]; int scalar; }; union Flexible bad = { 1 };";
        let (_, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2706");
        assert_eq!(diagnostic_text(source, &diagnostic), "1");
    }

    #[test]
    fn atomic_character_arrays_do_not_take_the_string_special_case() {
        let arena = Arena::default();
        let source = "int keep; _Atomic char bad[2] = \"x\"; long after;";
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2202");
        assert_eq!(diagnostic.message, "invalid-initializer-type");
        assert_eq!(diagnostic_text(source, &diagnostic), "\"x\"");
        assert_eq!(
            unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
            Ok(None)
        );

        let arena = Arena::default();
        let unit = parse_unit("_Atomic char values[2] = { 'x' };", &arena);
        let CHirDecl::Object(object) = unit.declarations()[0] else {
            panic!("expected atomic character array")
        };
        assert_eq!(
            object.initializer.as_ref().unwrap().plan().actions().len(),
            2
        );
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
            }) => match init.written().expression().expect("expression initializer") {
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
        let unit = parse_unit("int sink; void f(void) { sink = 5; }", &arena);
        let CHirDecl::Function(function) = unit.declarations()[1] else {
            panic!("expected function")
        };
        let CHirStmt::Compound(items, _) = function.body.expect("expected function body") else {
            panic!("expected compound body")
        };
        let CHirBlockItem::Statement(CHirStmt::Expr(expr, _)) = items[0] else {
            panic!("expected assignment expression statement")
        };
        match expr {
            CHirExpr::Assign {
                op,
                target,
                value,
                facts,
                ..
            } => {
                assert_eq!(*op, AssignmentOp::Simple);
                assert!(matches!(*target, CHirExpr::Ident { .. }));
                assert!(matches!(*value, CHirExpr::Lit(_, _)));
                assert_eq!(facts.category(), evocc_chir::ExprCategory::Value);
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

    #[test]
    fn failed_declaration_rolls_back_enumerator_bindings() {
        let source = "enum { MUST_NOT_LEAK = 7 } broken trailing;";
        let mut lexer = Lexer::new(FileId::new(99), source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            tokens.push(token);
        }
        let arena = Arena::default();
        let mut parser = Parser::new(tokens, FileId::new(99), source);
        let unit = parser.parse_translation_unit_in(&arena);

        assert!(parser.had_error());
        assert!(unit.is_empty());
        assert!(
            unit.symbols()
                .resolve(unit.symbols().file_scope(), "MUST_NOT_LEAK")
                .unwrap()
                .is_none(),
            "a rejected declaration must roll back ordinary-name bindings"
        );
    }

    #[test]
    fn span_composition_never_mixes_file_coordinate_systems() {
        let parser = Parser::new(Vec::new(), FileId::new(99), "");
        let header_start = SourceSpan::new(FileId::new(701), 4, 9, 2, 3);
        let header_end = SourceSpan::new(FileId::new(701), 20, 24, 3, 7);
        assert_eq!(
            parser.merge_span(header_start, header_end),
            SourceSpan::new(FileId::new(701), 4, 24, 2, 3)
        );

        let main_end = SourceSpan::new(FileId::new(702), 40, 44, 5, 2);
        assert_eq!(
            parser.merge_span(header_start, main_end),
            header_start,
            "cross-file endpoints must select the start token as primary origin"
        );
    }
}
