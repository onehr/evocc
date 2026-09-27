//! Canonical allocation-site identities shared by C-HIR execution and EvoIR
//! lowering.
//!
//! The manifest is deliberately derived from semantic objects rather than
//! materialized allocations.  That keeps origins stable when a scalar local
//! remains in SSA, and makes the two interpreters agree on cross-IR volatile
//! observations.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

use evocc_common::SourceSpan;
use evocc_types::{
    ArrayBound, ComplexBase, FloatKind, FunctionParameters, IntRank, TypeId, TypeInterner, TypeKind,
};

use crate::{
    CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, InitAction, ParameterSyntax, ScopeId,
    ScopeKind, StorageClass, SymbolId, SymbolKind, TagMembers, WrittenDeclaratorLayer,
    WrittenInitializer,
};

/// Zero-based module-wide identity of one allocation-bearing semantic site.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AllocationSiteOrdinal(u64);

impl AllocationSiteOrdinal {
    /// Return the stable numeric identity written into `ObjectOrigin.site`.
    #[must_use]
    pub fn raw(self) -> u64 {
        self.0
    }
}

/// The semantic source represented by one allocation site.
#[derive(Debug, Clone, Copy)]
pub enum AllocationSiteKind<'arena> {
    /// A merged C object or a definition parameter, keyed by semantic identity.
    Object(SymbolId),
    /// One exact string-literal expression node, not its spelling or span.
    StringLiteral(&'arena CHirExpr<'arena>),
    /// One compiler-owned dynamic stack allocation call.
    DynamicStack(&'arena CHirExpr<'arena>),
    /// One exact compound-literal expression and its C storage duration.
    CompoundLiteral {
        expression: &'arena CHirExpr<'arena>,
        static_storage: bool,
    },
}

impl<'arena> AllocationSiteKind<'arena> {
    #[must_use]
    pub fn object_symbol(self) -> Option<SymbolId> {
        match self {
            Self::Object(symbol) => Some(symbol),
            Self::StringLiteral(_) | Self::DynamicStack(_) | Self::CompoundLiteral { .. } => None,
        }
    }

    #[must_use]
    pub fn string_literal(self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::Object(_) | Self::DynamicStack(_) => None,
            Self::StringLiteral(expression) => Some(expression),
            Self::CompoundLiteral { .. } => None,
        }
    }

    #[must_use]
    pub fn compound_literal(self) -> Option<(&'arena CHirExpr<'arena>, bool)> {
        match self {
            Self::CompoundLiteral {
                expression,
                static_storage,
            } => Some((expression, static_storage)),
            Self::Object(_) | Self::StringLiteral(_) | Self::DynamicStack(_) => None,
        }
    }

    #[must_use]
    pub fn dynamic_stack(self) -> Option<&'arena CHirExpr<'arena>> {
        match self {
            Self::DynamicStack(expression) => Some(expression),
            _ => None,
        }
    }
}

/// One canonical allocation site after source-order ordinal assignment.
#[derive(Debug, Clone, Copy)]
pub struct AllocationSite<'arena> {
    ordinal: AllocationSiteOrdinal,
    kind: AllocationSiteKind<'arena>,
    anchor: SourceSpan,
}

impl<'arena> AllocationSite<'arena> {
    #[must_use]
    pub fn ordinal(&self) -> AllocationSiteOrdinal {
        self.ordinal
    }

    #[must_use]
    pub fn kind(&self) -> AllocationSiteKind<'arena> {
        self.kind
    }

    #[must_use]
    pub fn anchor(&self) -> SourceSpan {
        self.anchor
    }
}

/// Structural C-HIR failure or ordinal exhaustion during manifest derivation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AllocationSiteManifestError {
    OrdinalSpaceExhausted,
    UnknownSymbol {
        symbol: SymbolId,
        span: SourceSpan,
    },
    SymbolKindMismatch {
        symbol: SymbolId,
        expected: SymbolKind,
        actual: SymbolKind,
        span: SourceSpan,
    },
    SymbolSpellingMismatch {
        symbol: SymbolId,
        span: SourceSpan,
    },
    SymbolTypeMismatch {
        symbol: SymbolId,
        declaration: TypeId,
        canonical: TypeId,
        span: SourceSpan,
    },
    FileScopeMismatch {
        symbol: SymbolId,
        span: SourceSpan,
    },
    LocalScopeMismatch {
        symbol: SymbolId,
        span: SourceSpan,
    },
    ObjectStorageMismatch {
        symbol: SymbolId,
        declaration: StorageClass,
        canonical: Option<StorageClass>,
        span: SourceSpan,
    },
    FunctionStorageMismatch {
        symbol: SymbolId,
        declaration: StorageClass,
        canonical: Option<StorageClass>,
        span: SourceSpan,
    },
    DuplicateLocalObject {
        symbol: SymbolId,
        first: SourceSpan,
        duplicate: SourceSpan,
    },
    DuplicateFunctionDefinition {
        function: SymbolId,
        first: SourceSpan,
        duplicate: SourceSpan,
    },
    DeclarationNodeReused {
        span: SourceSpan,
    },
    StatementNodeReused {
        first_function: SymbolId,
        duplicate_function: SymbolId,
        span: SourceSpan,
    },
    StatementExpressionOutsideFunction {
        span: SourceSpan,
    },
    FunctionScopeMismatch {
        function: SymbolId,
        span: SourceSpan,
    },
    FunctionScopeOwnedByAnotherFunction {
        scope: ScopeId,
        owner: SymbolId,
        contender: SymbolId,
        span: SourceSpan,
    },
    DefinitionParameterIdentityMismatch {
        function: SymbolId,
        span: SourceSpan,
    },
    DuplicateDefinitionParameter {
        function: SymbolId,
        symbol: SymbolId,
        span: SourceSpan,
    },
    ParameterStorageMismatch {
        function: SymbolId,
        symbol: SymbolId,
        canonical: Option<StorageClass>,
        span: SourceSpan,
    },
    ParameterScopeMismatch {
        function: SymbolId,
        symbol: SymbolId,
        span: SourceSpan,
    },
    ParameterOwnedByAnotherFunction {
        symbol: SymbolId,
        owner: SymbolId,
        contender: SymbolId,
        span: SourceSpan,
    },
    InvalidMemoryPlace {
        span: SourceSpan,
    },
}

impl fmt::Display for AllocationSiteManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OrdinalSpaceExhausted => {
                formatter.write_str("allocation-site manifest exhausted u64 ordinals")
            }
            Self::UnknownSymbol { symbol, .. } => {
                write!(
                    formatter,
                    "allocation declaration names unknown symbol {}",
                    symbol.raw()
                )
            }
            Self::SymbolKindMismatch {
                symbol,
                expected,
                actual,
                ..
            } => write!(
                formatter,
                "allocation symbol {} has kind {actual:?}, expected {expected:?}",
                symbol.raw()
            ),
            Self::SymbolSpellingMismatch { symbol, .. } => write!(
                formatter,
                "allocation declaration spelling disagrees with symbol {}",
                symbol.raw()
            ),
            Self::SymbolTypeMismatch { symbol, .. } => write!(
                formatter,
                "allocation declaration type disagrees with symbol {}",
                symbol.raw()
            ),
            Self::FileScopeMismatch { symbol, .. } => write!(
                formatter,
                "allocation symbol {} is not bound at file scope",
                symbol.raw()
            ),
            Self::LocalScopeMismatch { symbol, .. } => write!(
                formatter,
                "allocation symbol {} is not coherently bound in a local scope",
                symbol.raw()
            ),
            Self::ObjectStorageMismatch { symbol, .. } => write!(
                formatter,
                "object storage disagrees with symbol {}",
                symbol.raw()
            ),
            Self::FunctionStorageMismatch { symbol, .. } => write!(
                formatter,
                "function storage disagrees with symbol {}",
                symbol.raw()
            ),
            Self::DuplicateLocalObject { symbol, .. } => write!(
                formatter,
                "local object symbol {} has multiple storage declarations",
                symbol.raw()
            ),
            Self::DuplicateFunctionDefinition { function, .. } => write!(
                formatter,
                "function {} has multiple definitions",
                function.raw()
            ),
            Self::DeclarationNodeReused { .. } => {
                formatter.write_str("one C-HIR declaration node has multiple structural parents")
            }
            Self::StatementNodeReused {
                first_function,
                duplicate_function,
                ..
            } => write!(
                formatter,
                "one C-HIR statement node belongs to both function {} and function {}",
                first_function.raw(),
                duplicate_function.raw()
            ),
            Self::StatementExpressionOutsideFunction { .. } => {
                formatter.write_str("statement expression has no enclosing function authority")
            }
            Self::FunctionScopeMismatch { function, .. } => write!(
                formatter,
                "function {} has no unique declarator-anchored function scope",
                function.raw()
            ),
            Self::FunctionScopeOwnedByAnotherFunction {
                scope,
                owner,
                contender,
                ..
            } => write!(
                formatter,
                "function scope {} belongs to function {}, not function {}",
                scope.raw(),
                owner.raw(),
                contender.raw()
            ),
            Self::DefinitionParameterIdentityMismatch { function, .. } => write!(
                formatter,
                "function {} has a malformed definition parameter identity",
                function.raw()
            ),
            Self::DuplicateDefinitionParameter {
                function, symbol, ..
            } => write!(
                formatter,
                "function {} repeats parameter symbol {}",
                function.raw(),
                symbol.raw()
            ),
            Self::ParameterStorageMismatch {
                function, symbol, ..
            } => write!(
                formatter,
                "function {} parameter {} has incoherent storage",
                function.raw(),
                symbol.raw()
            ),
            Self::ParameterScopeMismatch {
                function, symbol, ..
            } => write!(
                formatter,
                "function {} parameter {} is not owned by one function scope",
                function.raw(),
                symbol.raw()
            ),
            Self::ParameterOwnedByAnotherFunction {
                symbol,
                owner,
                contender,
                ..
            } => write!(
                formatter,
                "parameter {} belongs to function {}, not function {}",
                symbol.raw(),
                owner.raw(),
                contender.raw()
            ),
            Self::InvalidMemoryPlace { .. } => {
                formatter.write_str("memory place is not authoritative in this C-HIR unit")
            }
        }
    }
}

impl std::error::Error for AllocationSiteManifestError {}

/// Canonical allocation-site inventory for one C-HIR translation unit.
#[derive(Debug)]
pub struct AllocationSiteManifest<'arena> {
    sites: Vec<AllocationSite<'arena>>,
    function_static_objects: Vec<&'arena crate::ObjectDecl<'arena>>,
    object_sites: HashMap<SymbolId, usize>,
    string_sites: HashMap<usize, usize>,
    compound_sites: HashMap<usize, usize>,
    dynamic_sites: HashMap<usize, usize>,
    string_spans: BTreeMap<SpanKey, Vec<AllocationSiteOrdinal>>,
}

impl<'arena> AllocationSiteManifest<'arena> {
    /// Derive the complete manifest without recursively walking host stack.
    ///
    /// The walk covers both written initializers and normalized scalar-plan
    /// expressions. Expression and initializer identities act as
    /// cycle/shared-node guards; declarations and statements instead undergo
    /// occurrence validation, so mounting either node twice is a structural
    /// error. Merged file objects select the initializer-bearing definition,
    /// or the first tentative definition when no initializer exists, then use
    /// that selected anchor's canonical traversal encounter to assign
    /// module-wide order. `FileId` allocation order is never treated as
    /// translation-unit source order.
    pub fn derive(unit: &crate::CHirUnit<'arena>) -> Result<Self, AllocationSiteManifestError> {
        Deriver::new(unit).derive()
    }

    #[must_use]
    pub fn sites(&self) -> &[AllocationSite<'arena>] {
        &self.sites
    }

    /// Declarations discovered by the same validated, expression-aware walk
    /// that assigns allocation identities. Consumers must not rescan only
    /// statement bodies and lose statics nested in statement expressions.
    #[must_use]
    pub fn function_static_objects(&self) -> &[&'arena crate::ObjectDecl<'arena>] {
        &self.function_static_objects
    }

    #[must_use]
    pub fn site(&self, ordinal: AllocationSiteOrdinal) -> Option<&AllocationSite<'arena>> {
        usize::try_from(ordinal.raw())
            .ok()
            .and_then(|index| self.sites.get(index))
    }

    #[must_use]
    pub fn object_site(&self, symbol: SymbolId) -> Option<&AllocationSite<'arena>> {
        self.object_sites
            .get(&symbol)
            .and_then(|index| self.sites.get(*index))
    }

    /// Look up a string site by exact C-HIR node identity.
    #[must_use]
    pub fn string_literal_site(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Option<&AllocationSite<'arena>> {
        if !matches!(expression, CHirExpr::StringLit(_, _)) {
            return None;
        }
        self.string_sites
            .get(&node_identity(expression))
            .and_then(|index| self.sites.get(*index))
    }

    /// Look up a compound-literal site by exact C-HIR node identity.
    #[must_use]
    pub fn compound_literal_site(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Option<&AllocationSite<'arena>> {
        if !matches!(expression, CHirExpr::CompoundLit(_, _, _)) {
            return None;
        }
        self.compound_sites
            .get(&node_identity(expression))
            .and_then(|index| self.sites.get(*index))
    }

    #[must_use]
    pub fn dynamic_stack_site(
        &self,
        expression: &CHirExpr<'arena>,
    ) -> Option<&AllocationSite<'arena>> {
        self.dynamic_sites
            .get(&node_identity(expression))
            .and_then(|index| self.sites.get(*index))
    }

    /// Return every string site at the exact primary source location.
    ///
    /// A source span is not assumed unique: macro expansion or deliberately
    /// constructed C-HIR can place multiple distinct expression nodes at the
    /// same file/byte range.  Line and column are derived metadata and do not
    /// participate in the lookup key.
    #[must_use]
    pub fn string_literal_ordinals_at(&self, span: SourceSpan) -> &[AllocationSiteOrdinal] {
        self.string_spans
            .get(&SpanKey::from(span))
            .map_or(&[], Vec::as_slice)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct SpanKey {
    file: u32,
    byte_start: u32,
    byte_end: u32,
}

impl From<SourceSpan> for SpanKey {
    fn from(span: SourceSpan) -> Self {
        Self {
            file: span.file.raw(),
            byte_start: span.byte_start,
            byte_end: span.byte_end,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum AnchorStrength {
    Tentative,
    Definition,
}

#[derive(Debug, Clone, Copy)]
struct PendingSite<'arena> {
    kind: AllocationSiteKind<'arena>,
    anchor: SourceSpan,
    encounter: u64,
}

#[derive(Debug, Clone, Copy)]
struct PendingObject<'arena> {
    site: PendingSite<'arena>,
    strength: AnchorStrength,
}

#[derive(Debug, Clone, Copy)]
struct ValidatedSymbol {
    scope: ScopeId,
    ty: TypeId,
    storage: Option<StorageClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FunctionAuthority {
    symbol: SymbolId,
    scope: ScopeId,
}

#[derive(Debug, Clone, Copy)]
enum WorkItem<'arena> {
    Declaration {
        declaration: &'arena CHirDecl<'arena>,
        file_scope: bool,
        current_function: Option<FunctionAuthority>,
    },
    Statement {
        statement: &'arena CHirStmt<'arena>,
        current_function: FunctionAuthority,
    },
    Expression {
        expression: &'arena CHirExpr<'arena>,
        current_function: Option<FunctionAuthority>,
    },
    WrittenInitializer {
        initializer: &'arena WrittenInitializer<'arena>,
        current_function: Option<FunctionAuthority>,
    },
}

struct Deriver<'unit, 'arena> {
    unit: &'unit crate::CHirUnit<'arena>,
    next_encounter: u64,
    objects: HashMap<SymbolId, PendingObject<'arena>>,
    strings: Vec<PendingSite<'arena>>,
    local_object_declarations: HashMap<SymbolId, SourceSpan>,
    function_static_objects: Vec<&'arena crate::ObjectDecl<'arena>>,
    parameter_owners: HashMap<SymbolId, SymbolId>,
    function_scope_owners: HashMap<ScopeId, SymbolId>,
    definition_function_scopes: HashSet<(SymbolId, ScopeId)>,
    declaration_occurrences: HashMap<usize, SourceSpan>,
    statement_owners: HashMap<usize, FunctionAuthority>,
    visited_expressions: HashSet<usize>,
    visited_initializers: HashSet<usize>,
}

impl<'unit, 'arena> Deriver<'unit, 'arena> {
    fn new(unit: &'unit crate::CHirUnit<'arena>) -> Self {
        Self {
            unit,
            next_encounter: 0,
            objects: HashMap::new(),
            strings: Vec::new(),
            local_object_declarations: HashMap::new(),
            function_static_objects: Vec::new(),
            parameter_owners: HashMap::new(),
            function_scope_owners: HashMap::new(),
            definition_function_scopes: HashSet::new(),
            declaration_occurrences: HashMap::new(),
            statement_owners: HashMap::new(),
            visited_expressions: HashSet::new(),
            visited_initializers: HashSet::new(),
        }
    }

    fn derive(mut self) -> Result<AllocationSiteManifest<'arena>, AllocationSiteManifestError> {
        crate::function_definitions::FunctionDefinitions::derive(self.unit).map_err(|e| {
            AllocationSiteManifestError::DuplicateFunctionDefinition {
                function: e.symbol,
                first: e.first,
                duplicate: e.duplicate,
            }
        })?;
        let mut work = Vec::new();
        for declaration in self.unit.declarations().iter().rev().copied() {
            work.push(WorkItem::Declaration {
                declaration,
                file_scope: true,
                current_function: None,
            });
        }

        while let Some(item) = work.pop() {
            match item {
                WorkItem::Declaration {
                    declaration,
                    file_scope,
                    current_function,
                } => {
                    self.visit_declaration(declaration, file_scope, current_function, &mut work)?
                }
                WorkItem::Statement {
                    statement,
                    current_function,
                } => self.visit_statement(statement, current_function, &mut work)?,
                WorkItem::Expression {
                    expression,
                    current_function,
                } => self.visit_expression(expression, current_function, &mut work)?,
                WorkItem::WrittenInitializer {
                    initializer,
                    current_function,
                } => {
                    self.visit_written_initializer(initializer, current_function, &mut work);
                }
            }
        }

        let mut pending = self.strings;
        pending.extend(self.objects.into_values().map(|object| object.site));
        pending.sort_by_key(|site| site.encounter);

        let mut sites = Vec::with_capacity(pending.len());
        let mut object_sites = HashMap::new();
        let mut string_sites = HashMap::new();
        let mut compound_sites = HashMap::new();
        let mut dynamic_sites = HashMap::new();
        let mut string_spans = BTreeMap::<SpanKey, Vec<AllocationSiteOrdinal>>::new();
        for (index, pending) in pending.into_iter().enumerate() {
            let ordinal = AllocationSiteOrdinal(
                u64::try_from(index)
                    .map_err(|_| AllocationSiteManifestError::OrdinalSpaceExhausted)?,
            );
            let site = AllocationSite {
                ordinal,
                kind: pending.kind,
                anchor: pending.anchor,
            };
            match pending.kind {
                AllocationSiteKind::Object(symbol) => {
                    object_sites.insert(symbol, index);
                }
                AllocationSiteKind::StringLiteral(expression) => {
                    string_sites.insert(node_identity(expression), index);
                    string_spans
                        .entry(SpanKey::from(pending.anchor))
                        .or_default()
                        .push(ordinal);
                }
                AllocationSiteKind::CompoundLiteral { expression, .. } => {
                    compound_sites.insert(node_identity(expression), index);
                }
                AllocationSiteKind::DynamicStack(expression) => {
                    dynamic_sites.insert(node_identity(expression), index);
                }
            }
            sites.push(site);
        }

        Ok(AllocationSiteManifest {
            sites,
            function_static_objects: self.function_static_objects,
            object_sites,
            string_sites,
            compound_sites,
            dynamic_sites,
            string_spans,
        })
    }

    fn encounter(&mut self) -> Result<u64, AllocationSiteManifestError> {
        let encounter = self.next_encounter;
        self.next_encounter = self
            .next_encounter
            .checked_add(1)
            .ok_or(AllocationSiteManifestError::OrdinalSpaceExhausted)?;
        Ok(encounter)
    }

    fn record_object(
        &mut self,
        symbol: SymbolId,
        anchor: SourceSpan,
        strength: AnchorStrength,
    ) -> Result<(), AllocationSiteManifestError> {
        let candidate = PendingObject {
            site: PendingSite {
                kind: AllocationSiteKind::Object(symbol),
                anchor,
                encounter: self.encounter()?,
            },
            strength,
        };
        match self.objects.get_mut(&symbol) {
            Some(existing) if strength > existing.strength => *existing = candidate,
            Some(_) => {}
            None => {
                self.objects.insert(symbol, candidate);
            }
        }
        Ok(())
    }

    fn validate_symbol(
        &self,
        symbol: SymbolId,
        expected_kind: SymbolKind,
        spelling: &str,
        declaration_type: TypeId,
        span: SourceSpan,
    ) -> Result<ValidatedSymbol, AllocationSiteManifestError> {
        let canonical = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| AllocationSiteManifestError::UnknownSymbol { symbol, span })?;
        if canonical.kind() != expected_kind {
            return Err(AllocationSiteManifestError::SymbolKindMismatch {
                symbol,
                expected: expected_kind,
                actual: canonical.kind(),
                span,
            });
        }
        if canonical.spelling() != spelling {
            return Err(AllocationSiteManifestError::SymbolSpellingMismatch { symbol, span });
        }
        if !types_compatible(self.unit.types(), canonical.ty(), declaration_type) {
            return Err(AllocationSiteManifestError::SymbolTypeMismatch {
                symbol,
                declaration: declaration_type,
                canonical: canonical.ty(),
                span,
            });
        }
        Ok(ValidatedSymbol {
            scope: canonical.scope(),
            ty: canonical.ty(),
            storage: canonical.storage_class(),
        })
    }

    fn symbol_is_bound_in_scope(&self, symbol: SymbolId, scope: ScopeId) -> bool {
        self.unit
            .symbols()
            .is_bound_in_scope(symbol, scope)
            .unwrap_or(false)
    }

    fn function_scope_ancestor(&self, mut scope: ScopeId) -> Option<ScopeId> {
        loop {
            let current = self.unit.symbols().scope(scope).ok()?;
            if current.kind() == ScopeKind::Function {
                return Some(scope);
            }
            scope = current.parent()?;
        }
    }

    fn symbol_is_bound_under_function(
        &self,
        symbol: SymbolId,
        authority: FunctionAuthority,
    ) -> bool {
        if !self
            .definition_function_scopes
            .contains(&(authority.symbol, authority.scope))
            || self.function_scope_owners.get(&authority.scope) != Some(&authority.symbol)
        {
            return false;
        }
        self.unit
            .symbols()
            .scopes_in_source_order()
            .iter()
            .any(|scope| {
                scope.symbols().contains(&symbol)
                    && self.function_scope_ancestor(scope.id()) == Some(authority.scope)
            })
    }

    fn definition_function_authority(
        &mut self,
        function: &crate::FunctionDecl<'arena>,
    ) -> Result<FunctionAuthority, AllocationSiteManifestError> {
        let Some(WrittenDeclaratorLayer::Function {
            span: declarator_span,
            ..
        }) = function.written.layers.first()
        else {
            return Err(AllocationSiteManifestError::FunctionScopeMismatch {
                function: function.symbol,
                span: function.span,
            });
        };
        let has_bound_scope = self
            .unit
            .symbols()
            .scopes_in_source_order()
            .iter()
            .any(|scope| scope.function_owner() == Some(function.symbol));
        let matches = self
            .unit
            .symbols()
            .scopes_in_source_order()
            .iter()
            .filter(|scope| {
                scope.kind() == ScopeKind::Function
                    && scope.parent() == Some(self.unit.symbols().file_scope())
                    && match scope.function_owner() {
                        Some(owner) => {
                            owner == function.symbol
                                && scope.span().is_some_and(|span| {
                                    span.file == declarator_span.file
                                        && span.byte_start == declarator_span.byte_start
                                })
                        }
                        // Legacy, manually constructed C-HIR may omit the
                        // binding. Accept only an unambiguous location match.
                        None => {
                            !has_bound_scope
                                && scope.span().is_some_and(|scope_span| {
                                    scope_span.file == declarator_span.file
                                        && scope_span.byte_start == declarator_span.byte_start
                                })
                        }
                    }
            })
            .map(crate::Scope::id)
            .collect::<Vec<_>>();
        let [scope] = matches.as_slice() else {
            return Err(AllocationSiteManifestError::FunctionScopeMismatch {
                function: function.symbol,
                span: function.span,
            });
        };
        self.definition_function_scopes
            .insert((function.symbol, *scope));
        if let Some(owner) = self.function_scope_owners.insert(*scope, function.symbol) {
            if owner != function.symbol {
                return Err(
                    AllocationSiteManifestError::FunctionScopeOwnedByAnotherFunction {
                        scope: *scope,
                        owner,
                        contender: function.symbol,
                        span: function.span,
                    },
                );
            }
        }
        Ok(FunctionAuthority {
            symbol: function.symbol,
            scope: *scope,
        })
    }

    fn validate_object_declaration(
        &mut self,
        object: &crate::ObjectDecl<'arena>,
        file_scope: bool,
        current_function: Option<FunctionAuthority>,
    ) -> Result<(), AllocationSiteManifestError> {
        let symbol = self.validate_symbol(
            object.symbol,
            SymbolKind::Object,
            &object.name.spelling,
            object.ty,
            object.span,
        )?;
        if file_scope {
            if !self.symbol_is_bound_in_scope(object.symbol, self.unit.symbols().file_scope()) {
                return Err(AllocationSiteManifestError::FileScopeMismatch {
                    symbol: object.symbol,
                    span: object.span,
                });
            }
            let global_register = object.storage == StorageClass::Register
                && object.asm_label.is_some()
                && symbol.storage == Some(StorageClass::Register);
            if !global_register && !file_object_storage_compatible(symbol.storage, object.storage) {
                return Err(AllocationSiteManifestError::ObjectStorageMismatch {
                    symbol: object.symbol,
                    declaration: object.storage,
                    canonical: symbol.storage,
                    span: object.span,
                });
            }
            return Ok(());
        }

        let Some(current_function) = current_function else {
            return Err(AllocationSiteManifestError::LocalScopeMismatch {
                symbol: object.symbol,
                span: object.span,
            });
        };
        if object.storage.base() == StorageClass::Extern {
            if !self.symbol_is_bound_under_function(object.symbol, current_function) {
                return Err(AllocationSiteManifestError::LocalScopeMismatch {
                    symbol: object.symbol,
                    span: object.span,
                });
            }
            if !block_extern_storage_compatible(symbol.storage) {
                return Err(AllocationSiteManifestError::ObjectStorageMismatch {
                    symbol: object.symbol,
                    declaration: object.storage,
                    canonical: symbol.storage,
                    span: object.span,
                });
            }
            return Ok(());
        }

        let primary_scope = self.unit.symbols().scope(symbol.scope).map_err(|_| {
            AllocationSiteManifestError::LocalScopeMismatch {
                symbol: object.symbol,
                span: object.span,
            }
        })?;
        if !matches!(
            primary_scope.kind(),
            ScopeKind::Function | ScopeKind::Block | ScopeKind::For
        ) || !primary_scope.symbols().contains(&object.symbol)
            || !self
                .definition_function_scopes
                .contains(&(current_function.symbol, current_function.scope))
            || self.function_scope_owners.get(&current_function.scope)
                != Some(&current_function.symbol)
            || self.function_scope_ancestor(symbol.scope) != Some(current_function.scope)
        {
            return Err(AllocationSiteManifestError::LocalScopeMismatch {
                symbol: object.symbol,
                span: object.span,
            });
        }
        if symbol.storage != Some(object.storage) {
            return Err(AllocationSiteManifestError::ObjectStorageMismatch {
                symbol: object.symbol,
                declaration: object.storage,
                canonical: symbol.storage,
                span: object.span,
            });
        }
        if let Some(first) = self
            .local_object_declarations
            .insert(object.symbol, object.span)
        {
            return Err(AllocationSiteManifestError::DuplicateLocalObject {
                symbol: object.symbol,
                first,
                duplicate: object.span,
            });
        }
        Ok(())
    }

    fn validate_function_declaration(
        &self,
        function: &crate::FunctionDecl<'arena>,
        file_scope: bool,
        current_function: Option<FunctionAuthority>,
    ) -> Result<(), AllocationSiteManifestError> {
        let symbol = self.validate_symbol(
            function.symbol,
            SymbolKind::Function,
            &function.name.spelling,
            function.ty,
            function.span,
        )?;
        let coherently_bound = if file_scope {
            self.symbol_is_bound_in_scope(function.symbol, self.unit.symbols().file_scope())
                || (self.unit.symbols().is_builtin_function(function.symbol)
                    && function.body.is_none())
        } else {
            current_function.is_some_and(|authority| {
                self.symbol_is_bound_under_function(function.symbol, authority)
            })
        };
        if !coherently_bound || (!file_scope && function.body.is_some()) {
            return Err(if file_scope {
                AllocationSiteManifestError::FileScopeMismatch {
                    symbol: function.symbol,
                    span: function.span,
                }
            } else {
                AllocationSiteManifestError::LocalScopeMismatch {
                    symbol: function.symbol,
                    span: function.span,
                }
            });
        }
        if !function_storage_compatible(symbol.storage, function.storage) {
            return Err(AllocationSiteManifestError::FunctionStorageMismatch {
                symbol: function.symbol,
                declaration: function.storage,
                canonical: symbol.storage,
                span: function.span,
            });
        }
        Ok(())
    }

    fn validate_definition_parameters(
        &mut self,
        function: &crate::FunctionDecl<'arena>,
        authority: FunctionAuthority,
    ) -> Result<(), AllocationSiteManifestError> {
        let (ParameterSyntax::Prototype { parameters, .. }
        | ParameterSyntax::IdentifierList { parameters }) = &function.parameters
        else {
            return Ok(());
        };
        let mut seen_symbols = HashSet::new();
        let mut seen_spellings = HashSet::new();
        for parameter in parameters {
            let (symbol, name) = match (parameter.symbol, &parameter.name) {
                (None, None) => continue,
                (Some(symbol), Some(name)) => (symbol, name),
                _ => {
                    return Err(
                        AllocationSiteManifestError::DefinitionParameterIdentityMismatch {
                            function: function.symbol,
                            span: parameter.span,
                        },
                    );
                }
            };
            let canonical = self.validate_symbol(
                symbol,
                SymbolKind::Parameter,
                &name.spelling,
                parameter.ty,
                parameter.span,
            )?;
            // `specifier.type_id()` is only the declaration-specifier base;
            // pointer/array layers live in `written` and are already reflected
            // in `parameter.ty`. Comparing the base directly to the completed
            // type rejects every derived parameter declarator.
            if canonical.ty != parameter.ty {
                return Err(AllocationSiteManifestError::SymbolTypeMismatch {
                    symbol,
                    declaration: parameter.ty,
                    canonical: canonical.ty,
                    span: parameter.span,
                });
            }
            if !matches!(
                canonical.storage,
                Some(StorageClass::None | StorageClass::Register)
            ) || canonical.storage != Some(parameter.storage)
            {
                return Err(AllocationSiteManifestError::ParameterStorageMismatch {
                    function: function.symbol,
                    symbol,
                    canonical: canonical.storage,
                    span: parameter.span,
                });
            }
            if let Some(owner) = self.parameter_owners.get(&symbol).copied() {
                if owner != function.symbol {
                    return Err(
                        AllocationSiteManifestError::ParameterOwnedByAnotherFunction {
                            symbol,
                            owner,
                            contender: function.symbol,
                            span: parameter.span,
                        },
                    );
                }
            }
            let scope = self.unit.symbols().scope(canonical.scope).map_err(|_| {
                AllocationSiteManifestError::ParameterScopeMismatch {
                    function: function.symbol,
                    symbol,
                    span: parameter.span,
                }
            })?;
            let coherent_scope = scope.kind() == ScopeKind::Function
                && scope.parent() == Some(self.unit.symbols().file_scope())
                && scope.symbols().contains(&symbol)
                && canonical.scope == authority.scope;
            if !coherent_scope {
                return Err(AllocationSiteManifestError::ParameterScopeMismatch {
                    function: function.symbol,
                    symbol,
                    span: parameter.span,
                });
            }
            if !seen_symbols.insert(symbol) || !seen_spellings.insert(name.spelling.as_str()) {
                return Err(AllocationSiteManifestError::DuplicateDefinitionParameter {
                    function: function.symbol,
                    symbol,
                    span: parameter.span,
                });
            }
            self.parameter_owners.insert(symbol, function.symbol);
            self.record_object(symbol, parameter.span, AnchorStrength::Definition)?;
        }
        Ok(())
    }

    fn visit_declaration(
        &mut self,
        declaration: &'arena CHirDecl<'arena>,
        file_scope: bool,
        current_function: Option<FunctionAuthority>,
        work: &mut Vec<WorkItem<'arena>>,
    ) -> Result<(), AllocationSiteManifestError> {
        if self
            .declaration_occurrences
            .insert(node_identity(declaration), declaration.span())
            .is_some()
        {
            return Err(AllocationSiteManifestError::DeclarationNodeReused {
                span: declaration.span(),
            });
        }

        match declaration {
            CHirDecl::Asm(assembly) => {
                for operand in assembly.outputs.iter().chain(&assembly.inputs).rev() {
                    work.push(WorkItem::Expression {
                        expression: operand.expression,
                        current_function,
                    });
                }
            }
            CHirDecl::Object(object) => {
                self.validate_object_declaration(object, file_scope, current_function)?;
                if let Some(expression) = object.cleanup {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
                if !file_scope && object.storage.base() == StorageClass::Static {
                    self.function_static_objects.push(object);
                }
                let strength = if file_scope {
                    if (object.storage == StorageClass::Register && object.asm_label.is_some())
                        || object
                            .attributes
                            .iter()
                            .any(|attribute| attribute.semantic_name() == Some("alias"))
                    {
                        None
                    } else if object.initializer.is_some() {
                        Some(AnchorStrength::Definition)
                    } else if object.storage.base() == StorageClass::Extern {
                        None
                    } else {
                        Some(AnchorStrength::Tentative)
                    }
                } else if object.storage.base() == StorageClass::Extern {
                    None
                } else {
                    Some(AnchorStrength::Definition)
                };
                if let Some(strength) = strength {
                    self.record_object(object.symbol, object.span, strength)?;
                }

                if let Some(initializer) = &object.initializer {
                    for action in initializer.plan().actions().iter().rev() {
                        match action {
                            InitAction::StoreScalar { value, .. }
                            | InitAction::CopyAggregate { value, .. } => {
                                work.push(WorkItem::Expression {
                                    expression: value,
                                    current_function,
                                });
                            }
                            // StoreString owns a normalized literal value, not
                            // an expression identity. Its authoritative source
                            // occurrence is visited through `written()` below.
                            InitAction::StoreString { .. } | InitAction::ZeroScalar { .. } => {}
                        }
                    }
                    work.push(WorkItem::WrittenInitializer {
                        initializer: initializer.written(),
                        current_function,
                    });
                }
            }
            CHirDecl::Function(function) => {
                self.validate_function_declaration(function, file_scope, current_function)?;
                if let Some(body) = function.body {
                    let authority = self.definition_function_authority(function)?;
                    self.validate_definition_parameters(function, authority)?;
                    work.push(WorkItem::Statement {
                        statement: body,
                        current_function: authority,
                    });
                }
            }
            CHirDecl::Tag(tag) => match &tag.members {
                TagMembers::Incomplete => {}
                TagMembers::Struct(members) | TagMembers::Union(members) => {
                    for member in members.iter().rev() {
                        if let Some(bit_width) = member.bit_width {
                            work.push(WorkItem::Expression {
                                expression: bit_width,
                                current_function,
                            });
                        }
                    }
                }
                TagMembers::Enum(enumerators) => {
                    for enumerator in enumerators.iter().rev() {
                        if let Some(value) = enumerator.written_value {
                            work.push(WorkItem::Expression {
                                expression: value,
                                current_function,
                            });
                        }
                    }
                }
            },
            CHirDecl::Typedef(_) => {}
        }
        Ok(())
    }

    fn visit_statement(
        &mut self,
        statement: &'arena CHirStmt<'arena>,
        current_function: FunctionAuthority,
        work: &mut Vec<WorkItem<'arena>>,
    ) -> Result<(), AllocationSiteManifestError> {
        if let Some(first_owner) = self
            .statement_owners
            .insert(node_identity(statement), current_function)
        {
            return Err(AllocationSiteManifestError::StatementNodeReused {
                first_function: first_owner.symbol,
                duplicate_function: current_function.symbol,
                span: statement.span(),
            });
        }

        match statement {
            CHirStmt::Asm(assembly) => {
                for operand in assembly.outputs.iter().chain(&assembly.inputs).rev() {
                    work.push(WorkItem::Expression {
                        expression: operand.expression,
                        current_function: Some(current_function),
                    });
                }
            }
            CHirStmt::Expr(expression, _) | CHirStmt::Return(Some(expression), _) => {
                work.push(WorkItem::Expression {
                    expression,
                    current_function: Some(current_function),
                });
            }
            CHirStmt::ComputedGoto(expression, _) => {
                work.push(WorkItem::Expression {
                    expression,
                    current_function: Some(current_function),
                });
            }
            CHirStmt::If(condition, then_statement, else_statement, _) => {
                if let Some(else_statement) = else_statement {
                    work.push(WorkItem::Statement {
                        statement: else_statement,
                        current_function,
                    });
                }
                work.push(WorkItem::Statement {
                    statement: then_statement,
                    current_function,
                });
                work.push(WorkItem::Expression {
                    expression: condition,
                    current_function: Some(current_function),
                });
            }
            CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
                work.push(WorkItem::Statement {
                    statement: body,
                    current_function,
                });
                work.push(WorkItem::Expression {
                    expression,
                    current_function: Some(current_function),
                });
            }
            CHirStmt::DoWhile(body, condition, _) => {
                work.push(WorkItem::Expression {
                    expression: condition,
                    current_function: Some(current_function),
                });
                work.push(WorkItem::Statement {
                    statement: body,
                    current_function,
                });
            }
            CHirStmt::For(initializer, condition, post, body, _) => {
                work.push(WorkItem::Statement {
                    statement: body,
                    current_function,
                });
                if let Some(post) = post {
                    work.push(WorkItem::Expression {
                        expression: post,
                        current_function: Some(current_function),
                    });
                }
                if let Some(condition) = condition {
                    work.push(WorkItem::Expression {
                        expression: condition,
                        current_function: Some(current_function),
                    });
                }
                match initializer {
                    CHirForInit::Empty => {}
                    CHirForInit::Expression(expression) => {
                        work.push(WorkItem::Expression {
                            expression,
                            current_function: Some(current_function),
                        });
                    }
                    CHirForInit::Declaration(declarations) => {
                        for declaration in declarations.iter().rev().copied() {
                            work.push(WorkItem::Declaration {
                                declaration,
                                file_scope: false,
                                current_function: Some(current_function),
                            });
                        }
                    }
                }
            }
            CHirStmt::Return(None, _)
            | CHirStmt::Break(_)
            | CHirStmt::Continue(_)
            | CHirStmt::Goto(_, _)
            | CHirStmt::AttributedNull(_, _)
            | CHirStmt::Null(_) => {}
            CHirStmt::Compound(items, _, _) => {
                for item in items.iter().rev() {
                    match item {
                        CHirBlockItem::Declaration(declaration) => {
                            work.push(WorkItem::Declaration {
                                declaration,
                                file_scope: false,
                                current_function: Some(current_function),
                            });
                        }
                        CHirBlockItem::Statement(statement) => {
                            work.push(WorkItem::Statement {
                                statement,
                                current_function,
                            });
                        }
                    }
                }
            }
            CHirStmt::Label(_, statement, _) | CHirStmt::Default(statement, _) => {
                work.push(WorkItem::Statement {
                    statement,
                    current_function,
                });
            }
            CHirStmt::Case(expression, statement, _) => {
                work.push(WorkItem::Statement {
                    statement,
                    current_function,
                });
                work.push(WorkItem::Expression {
                    expression,
                    current_function: Some(current_function),
                });
            }
            CHirStmt::CaseRange(first, last, statement, _) => {
                work.push(WorkItem::Statement {
                    statement,
                    current_function,
                });
                work.push(WorkItem::Expression {
                    expression: last,
                    current_function: Some(current_function),
                });
                work.push(WorkItem::Expression {
                    expression: first,
                    current_function: Some(current_function),
                });
            }
        }
        Ok(())
    }

    fn visit_expression(
        &mut self,
        expression: &'arena CHirExpr<'arena>,
        current_function: Option<FunctionAuthority>,
        work: &mut Vec<WorkItem<'arena>>,
    ) -> Result<(), AllocationSiteManifestError> {
        if !self.visited_expressions.insert(node_identity(expression)) {
            return Ok(());
        }

        match expression {
            CHirExpr::Atomic(access) => {
                for expression in access.operands().rev() {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::Floating(access) => {
                for &expression in access.operands.iter().rev() {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::Vector(access) => {
                for &expression in access.operands.iter().rev() {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::Complex(access) => {
                for &expression in access.operands.iter().rev() {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::StringLit(_, anchor) => {
                let encounter = self.encounter()?;
                self.strings.push(PendingSite {
                    kind: AllocationSiteKind::StringLiteral(expression),
                    anchor: *anchor,
                    encounter,
                });
            }
            CHirExpr::Unary(_, operand, _)
            | CHirExpr::EvaluateOnce(operand, _)
            | CHirExpr::Cast(_, operand, _)
            | CHirExpr::Member(operand, _, _)
            | CHirExpr::PtrMember(operand, _, _)
            | CHirExpr::BitBuiltin { operand, .. } => {
                work.push(WorkItem::Expression {
                    expression: operand,
                    current_function,
                });
            }
            CHirExpr::Binary(_, left, right, _)
            | CHirExpr::Index(left, right, _)
            | CHirExpr::Comma(left, right, _)
            | CHirExpr::Elvis(left, right, _)
            | CHirExpr::BuiltinExpect(left, right, _) => {
                work.push(WorkItem::Expression {
                    expression: right,
                    current_function,
                });
                work.push(WorkItem::Expression {
                    expression: left,
                    current_function,
                });
            }
            CHirExpr::Ternary(condition, then_expression, else_expression, _) => {
                for expression in [else_expression, then_expression, condition] {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::Call(callee, arguments, _) => {
                if self.unit.stack_allocation_size(expression).is_some() {
                    let encounter = self.encounter()?;
                    self.strings.push(PendingSite {
                        kind: AllocationSiteKind::DynamicStack(expression),
                        anchor: expression.span(),
                        encounter,
                    });
                }
                for argument in arguments.iter().rev().copied() {
                    work.push(WorkItem::Expression {
                        expression: argument,
                        current_function,
                    });
                }
                work.push(WorkItem::Expression {
                    expression: callee,
                    current_function,
                });
            }
            CHirExpr::CheckedArithmetic {
                lhs, rhs, target, ..
            } => {
                for expression in [target, rhs, lhs] {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::BuiltinVarArgs { first, second, .. } => {
                if let Some(second) = second {
                    work.push(WorkItem::Expression {
                        expression: second,
                        current_function,
                    });
                }
                work.push(WorkItem::Expression {
                    expression: first,
                    current_function,
                });
            }
            CHirExpr::Assign { target, value, .. } => {
                for expression in [value, target] {
                    work.push(WorkItem::Expression {
                        expression,
                        current_function,
                    });
                }
            }
            CHirExpr::MemoryRead {
                place, target_span, ..
            } => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| AllocationSiteManifestError::InvalidMemoryPlace {
                        span: *target_span,
                    })?;
                for source in place.source_expressions().rev() {
                    work.push(WorkItem::Expression {
                        expression: source,
                        current_function,
                    });
                }
            }
            CHirExpr::MemoryAssign {
                place,
                target_span,
                value,
                ..
            } => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| AllocationSiteManifestError::InvalidMemoryPlace {
                        span: *target_span,
                    })?;
                work.push(WorkItem::Expression {
                    expression: value,
                    current_function,
                });
                for source in place.source_expressions().rev() {
                    work.push(WorkItem::Expression {
                        expression: source,
                        current_function,
                    });
                }
            }
            CHirExpr::Update { target, .. } => {
                work.push(WorkItem::Expression {
                    expression: target,
                    current_function,
                });
            }
            CHirExpr::MemoryUpdate {
                place, target_span, ..
            } => {
                place
                    .validate_against(self.unit.symbols(), self.unit.types())
                    .map_err(|_| AllocationSiteManifestError::InvalidMemoryPlace {
                        span: *target_span,
                    })?;
                for source in place.source_expressions().rev() {
                    work.push(WorkItem::Expression {
                        expression: source,
                        current_function,
                    });
                }
            }
            CHirExpr::StatementExpr { body, span, .. } => {
                let Some(current_function) = current_function else {
                    return Err(
                        AllocationSiteManifestError::StatementExpressionOutsideFunction {
                            span: *span,
                        },
                    );
                };
                work.push(WorkItem::Statement {
                    statement: body,
                    current_function,
                });
            }
            CHirExpr::CompoundLit(_, initializer, anchor) => {
                let encounter = self.encounter()?;
                self.strings.push(PendingSite {
                    kind: AllocationSiteKind::CompoundLiteral {
                        expression,
                        static_storage: current_function.is_none(),
                    },
                    anchor: *anchor,
                    encounter,
                });
                work.push(WorkItem::WrittenInitializer {
                    initializer: initializer.written(),
                    current_function,
                });
            }
            CHirExpr::Lit(_, _)
            | CHirExpr::FloatLit(_, _)
            | CHirExpr::CharLit(_, _)
            | CHirExpr::Ident { .. }
            | CHirExpr::LabelAddress { .. }
            | CHirExpr::SizeOf(_, _)
            | CHirExpr::SizeOfExpr(_, _, _)
            | CHirExpr::BuiltinConstantP { .. }
            | CHirExpr::BuiltinObjectSize { .. }
            | CHirExpr::AlignOf(_, _)
            | CHirExpr::BuiltinTrap { .. }
            | CHirExpr::BuiltinUnreachable { .. }
            | CHirExpr::BuiltinReturnAddress { .. }
            | CHirExpr::BuiltinFrameAddress { .. } => {}
        }
        Ok(())
    }

    fn visit_written_initializer(
        &mut self,
        initializer: &'arena WrittenInitializer<'arena>,
        current_function: Option<FunctionAuthority>,
        work: &mut Vec<WorkItem<'arena>>,
    ) {
        if !self.visited_initializers.insert(node_identity(initializer)) {
            return;
        }
        match initializer {
            WrittenInitializer::Expression(expression) => {
                work.push(WorkItem::Expression {
                    expression,
                    current_function,
                });
            }
            WrittenInitializer::List {
                elements,
                designations,
                ..
            } => {
                for designation in designations.iter().rev().flatten() {
                    for designator in designation.designators.iter().rev() {
                        match designator {
                            crate::WrittenDesignator::Field(_) => {}
                            crate::WrittenDesignator::Index { expression, .. } => {
                                work.push(WorkItem::Expression {
                                    expression,
                                    current_function,
                                });
                            }
                            crate::WrittenDesignator::Range { first, last, .. } => {
                                work.push(WorkItem::Expression {
                                    expression: last,
                                    current_function,
                                });
                                work.push(WorkItem::Expression {
                                    expression: first,
                                    current_function,
                                });
                            }
                        }
                    }
                }
                for element in elements.iter().rev() {
                    work.push(WorkItem::WrittenInitializer {
                        initializer: element,
                        current_function,
                    });
                }
            }
        }
    }
}

fn file_object_storage_compatible(
    canonical: Option<StorageClass>,
    declaration: StorageClass,
) -> bool {
    if canonical.is_some_and(StorageClass::is_thread_local) != declaration.is_thread_local() {
        return false;
    }
    let declaration = declaration.base();
    let canonical = canonical.map(StorageClass::base);
    match canonical {
        Some(StorageClass::Static) => {
            matches!(declaration, StorageClass::Static | StorageClass::Extern)
        }
        Some(StorageClass::None | StorageClass::Extern) => {
            matches!(declaration, StorageClass::None | StorageClass::Extern)
        }
        Some(
            StorageClass::ThreadLocal
            | StorageClass::StaticThreadLocal
            | StorageClass::ExternThreadLocal,
        ) => false,
        Some(StorageClass::Typedef | StorageClass::Auto | StorageClass::Register) | None => false,
    }
}

fn block_extern_storage_compatible(canonical: Option<StorageClass>) -> bool {
    matches!(
        canonical.map(StorageClass::base),
        Some(StorageClass::None | StorageClass::Extern | StorageClass::Static)
    )
}

fn function_storage_compatible(canonical: Option<StorageClass>, declaration: StorageClass) -> bool {
    match canonical {
        Some(StorageClass::Static) => matches!(
            declaration,
            StorageClass::None | StorageClass::Extern | StorageClass::Static
        ),
        Some(StorageClass::None | StorageClass::Extern) => {
            matches!(declaration, StorageClass::None | StorageClass::Extern)
        }
        Some(
            StorageClass::Typedef
            | StorageClass::Auto
            | StorageClass::Register
            | StorageClass::ThreadLocal
            | StorageClass::StaticThreadLocal
            | StorageClass::ExternThreadLocal,
        )
        | None => false,
    }
}

/// Read-only equivalent of the parser's composite-type compatibility test.
///
/// This deliberately does not intern a composite type: manifest derivation is
/// observational and must not mutate the translation unit's type authority.
fn types_compatible(types: &TypeInterner, left: TypeId, right: TypeId) -> bool {
    let mut work = vec![(left, right, false)];
    let mut visited = HashSet::new();
    while let Some((left_id, right_id, ignore_top_qualifiers)) = work.pop() {
        if !visited.insert((left_id, right_id, ignore_top_qualifiers)) {
            continue;
        }
        let (Some(left), Some(right)) = (types.get(left_id), types.get(right_id)) else {
            return false;
        };
        if left_id == right_id {
            continue;
        }
        if !ignore_top_qualifiers
            && left.qualifiers.compatibility() != right.qualifiers.compatibility()
        {
            return false;
        }
        match (&left.kind, &right.kind) {
            (TypeKind::Void, TypeKind::Void)
            | (TypeKind::BuiltinVaList, TypeKind::BuiltinVaList) => {}
            (TypeKind::GnuInt128(left), TypeKind::GnuInt128(right)) if left == right => {}
            (TypeKind::TargetBuiltin(left), TypeKind::TargetBuiltin(right)) if left == right => {}
            (TypeKind::Int(left), TypeKind::Int(right)) if left == right => {}
            (TypeKind::Enum(tag), TypeKind::Int(rank))
            | (TypeKind::Int(rank), TypeKind::Enum(tag))
                if types.enum_representation(*tag) == Some(*rank) => {}
            (TypeKind::Float(left), TypeKind::Float(right)) if left == right => {}
            (TypeKind::Complex(left), TypeKind::Complex(right)) if left == right => {}
            (TypeKind::Pointer(left), TypeKind::Pointer(right)) => {
                work.push((*left, *right, false));
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
            ) if array_bounds_compatible(left_bound, right_bound) => {
                work.push((*left_element, *right_element, false));
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
                work.push((*left_return, *right_return, false));
                match (left_parameters, right_parameters) {
                    (FunctionParameters::Unspecified, FunctionParameters::Unspecified) => {}
                    (
                        FunctionParameters::Unspecified,
                        prototype @ FunctionParameters::Prototype { .. },
                    )
                    | (
                        prototype @ FunctionParameters::Prototype { .. },
                        FunctionParameters::Unspecified,
                    ) => {
                        if !prototype_survives_default_promotions(types, prototype) {
                            return false;
                        }
                    }
                    (
                        FunctionParameters::Prototype {
                            types: left_types,
                            variadic: left_variadic,
                        },
                        FunctionParameters::Prototype {
                            types: right_types,
                            variadic: right_variadic,
                        },
                    ) => {
                        if left_variadic != right_variadic || left_types.len() != right_types.len()
                        {
                            return false;
                        }
                        work.extend(
                            left_types
                                .iter()
                                .zip(right_types)
                                .map(|(left, right)| (*left, *right, true)),
                        );
                    }
                }
            }
            (TypeKind::Struct(left), TypeKind::Struct(right)) if left == right => {}
            (TypeKind::Union(left), TypeKind::Union(right)) if left == right => {}
            (TypeKind::Enum(left), TypeKind::Enum(right)) if left == right => {}
            _ => return false,
        }
    }
    true
}

fn array_bounds_compatible(left: &ArrayBound, right: &ArrayBound) -> bool {
    left.composite(right).is_some()
}

fn prototype_survives_default_promotions(
    types: &TypeInterner,
    parameters: &FunctionParameters,
) -> bool {
    let FunctionParameters::Prototype {
        types: parameters,
        variadic,
    } = parameters
    else {
        return true;
    };
    !variadic
        && parameters.iter().all(|parameter| {
            types
                .get(*parameter)
                .is_some_and(|parameter| match parameter.kind {
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
                    TypeKind::TargetBuiltin(_) => false,
                    TypeKind::Complex(ComplexBase::CF16 | ComplexBase::CF32) => false,
                    TypeKind::Enum(_) => false,
                    _ => true,
                })
        })
}

fn node_identity<T>(node: &T) -> usize {
    std::ptr::from_ref(node).cast::<()>() as usize
}

#[cfg(test)]
mod tests {
    use super::{AllocationSiteKind, AllocationSiteManifest, AllocationSiteManifestError};
    use crate::{
        AssignmentOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, ExprFacts,
        FunctionDecl, InitAction, InitPlan, IntBase, IntLit, IntSuffix, MemoryPlace, ObjectDecl,
        ObjectInitializer, ParameterDecl, ParameterSyntax, PlaceRoot, ScopeKind, SpannedName,
        StorageClass, StringLit, StringPrefix, SymbolData, SymbolId, SymbolKind, SymbolTable,
        TypeSpecifier, UnaryOp, WrittenDeclarator, WrittenDeclaratorLayer, WrittenInitializer,
    };
    use evocc_common::{Arena, FileId, SourceSpan};
    use evocc_types::{
        ArrayBound, CanonicalType, IntRank, Qualifiers, TypeId, TypeInterner, TypeKind,
    };

    fn span(byte_start: u32) -> SourceSpan {
        span_in(1, byte_start)
    }

    fn span_in(file: u32, byte_start: u32) -> SourceSpan {
        SourceSpan::new(
            FileId::new(file),
            byte_start,
            byte_start + 1,
            1,
            byte_start + 1,
        )
    }

    fn arena_alloc<'arena, T: 'arena>(arena: &'arena Arena, value: T) -> &'arena T {
        // Safety: these tests allocate only C-HIR values whose destructors do
        // not retain or access the Arena.
        unsafe { arena.alloc_shared(value) }
    }

    fn int_type(types: &mut TypeInterner) -> TypeId {
        types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        })
    }

    fn long_type(types: &mut TypeInterner) -> TypeId {
        types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Long),
            qualifiers: Qualifiers::default(),
        })
    }

    fn name(spelling: &str, at: u32) -> SpannedName {
        SpannedName {
            spelling: spelling.into(),
            span: span(at),
        }
    }

    fn specifier(ty: TypeId, at: u32) -> TypeSpecifier {
        TypeSpecifier::Builtin { ty, span: span(at) }
    }

    fn written(at: u32) -> WrittenDeclarator {
        WrittenDeclarator {
            layers: Vec::new(),
            span: span(at),
        }
    }

    fn integer<'arena>(arena: &'arena Arena, value: u64, at: u32) -> &'arena CHirExpr<'arena> {
        arena_alloc(
            arena,
            CHirExpr::Lit(
                IntLit {
                    value,
                    suffix: IntSuffix::None,
                    base: IntBase::Dec,
                },
                span(at),
            ),
        )
    }

    fn string<'arena>(arena: &'arena Arena, value: &str, at: u32) -> &'arena CHirExpr<'arena> {
        string_at(arena, value, span(at))
    }

    fn string_at<'arena>(
        arena: &'arena Arena,
        value: &str,
        source_span: SourceSpan,
    ) -> &'arena CHirExpr<'arena> {
        arena_alloc(
            arena,
            CHirExpr::StringLit(StringLit::single(value, StringPrefix::None), source_span),
        )
    }

    fn initializer<'arena>(
        ty: TypeId,
        expression: &'arena CHirExpr<'arena>,
    ) -> ObjectInitializer<'arena> {
        ObjectInitializer::new(
            WrittenInitializer::Expression(expression),
            InitPlan::new(
                ty,
                vec![InitAction::store_scalar(
                    crate::SubobjectPath::root(),
                    ty,
                    expression,
                    expression.span(),
                )],
            ),
        )
    }

    fn object<'arena>(
        symbol: SymbolId,
        spelling: &str,
        ty: TypeId,
        storage: StorageClass,
        initializer: Option<ObjectInitializer<'arena>>,
        at: u32,
    ) -> CHirDecl<'arena> {
        CHirDecl::Object(ObjectDecl {
            attributes: Vec::new(),
            cleanup: None,
            symbol,
            name: name(spelling, at),
            ty,
            specifier: specifier(ty, at),
            written: written(at),
            storage,
            initializer,
            asm_label: None,
            asm_label_span: None,
            gnu_extension_spans: Vec::new(),
            span: span(at),
        })
    }

    fn parameter(
        symbol: Option<SymbolId>,
        spelling: Option<&str>,
        ty: TypeId,
        at: u32,
    ) -> ParameterDecl {
        ParameterDecl {
            attributes: Vec::new(),
            symbol,
            name: spelling.map(|spelling| name(spelling, at)),
            ty,
            storage: StorageClass::None,
            specifier: specifier(ty, at),
            written: written(at),
            span: span(at),
        }
    }

    fn function<'arena>(
        symbol: SymbolId,
        spelling: &str,
        ty: TypeId,
        parameters: Vec<ParameterDecl>,
        body: Option<&'arena CHirStmt<'arena>>,
        at: u32,
    ) -> CHirDecl<'arena> {
        CHirDecl::Function(FunctionDecl {
            attributes: Vec::new(),
            symbol,
            name: name(spelling, at),
            ty,
            return_specifier: specifier(ty, at),
            written: WrittenDeclarator {
                layers: vec![WrittenDeclaratorLayer::Function {
                    parameters: None,
                    span: span(at + 1),
                }],
                span: span(at),
            },
            parameters: ParameterSyntax::Prototype {
                parameters,
                variadic: false,
            },
            storage: StorageClass::None,
            asm_label: None,
            asm_label_span: None,
            inline_spans: Vec::new(),
            gnu_extension_spans: Vec::new(),
            body,
            span: span(at),
        })
    }

    fn declare(
        symbols: &mut SymbolTable,
        scope: crate::ScopeId,
        spelling: &str,
        data: SymbolData,
        ty: TypeId,
        at: u32,
    ) -> SymbolId {
        symbols
            .declare(scope, spelling, data, ty, span(at))
            .expect("fixture declaration must be valid")
    }

    #[test]
    fn definition_anchor_replaces_first_tentative_and_extern_only_has_no_site() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let x = declare(
            &mut symbols,
            file,
            "x",
            SymbolData::Object {
                storage: StorageClass::Extern,
            },
            ty,
            1,
        );
        let y = declare(
            &mut symbols,
            file,
            "y",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            ty,
            2,
        );
        let z = declare(
            &mut symbols,
            file,
            "z",
            SymbolData::Object {
                storage: StorageClass::Extern,
            },
            ty,
            3,
        );
        let prototype_scope = symbols
            .create_scope(file, ScopeKind::FunctionPrototype, Some(span(4)))
            .unwrap();
        let q = declare(
            &mut symbols,
            prototype_scope,
            "q",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            ty,
            31,
        );
        let f = declare(
            &mut symbols,
            file,
            "f",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            4,
        );

        let declarations = vec![
            arena_alloc(&arena, object(x, "x", ty, StorageClass::Extern, None, 10)),
            arena_alloc(&arena, object(x, "x", ty, StorageClass::None, None, 20)),
            arena_alloc(
                &arena,
                function(
                    f,
                    "f",
                    ty,
                    vec![parameter(Some(q), Some("q"), ty, 31)],
                    None,
                    30,
                ),
            ),
            arena_alloc(&arena, object(y, "y", ty, StorageClass::None, None, 40)),
            arena_alloc(
                &arena,
                object(
                    x,
                    "x",
                    ty,
                    StorageClass::None,
                    Some(initializer(ty, integer(&arena, 1, 51))),
                    50,
                ),
            ),
            arena_alloc(&arena, object(z, "z", ty, StorageClass::Extern, None, 60)),
        ];
        let unit = crate::CHirUnit::new(declarations, types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert_eq!(manifest.sites().len(), 2);
        assert_eq!(manifest.object_site(y).unwrap().ordinal().raw(), 0);
        assert_eq!(manifest.object_site(y).unwrap().anchor(), span(40));
        assert_eq!(manifest.object_site(x).unwrap().ordinal().raw(), 1);
        assert_eq!(manifest.object_site(x).unwrap().anchor(), span(50));
        assert!(manifest.object_site(z).is_none());
        assert!(
            manifest.object_site(q).is_none(),
            "prototype parameters own no runtime object"
        );
    }

    #[test]
    fn compatible_incomplete_array_redeclaration_uses_the_composite_symbol_type() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let element = int_type(&mut types);
        let incomplete = types.intern(CanonicalType {
            kind: TypeKind::Array {
                element,
                bound: ArrayBound::Unbounded,
            },
            qualifiers: Qualifiers::default(),
        });
        let complete = types.intern(CanonicalType {
            kind: TypeKind::Array {
                element,
                bound: ArrayBound::Fixed(3),
            },
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let array = declare(
            &mut symbols,
            file,
            "array",
            SymbolData::Object {
                storage: StorageClass::Extern,
            },
            complete,
            1,
        );
        let declaration = arena_alloc(
            &arena,
            object(array, "array", incomplete, StorageClass::Extern, None, 10),
        );
        let definition = arena_alloc(
            &arena,
            object(array, "array", complete, StorageClass::None, None, 20),
        );
        let unit = crate::CHirUnit::new(vec![declaration, definition], types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert_eq!(manifest.sites().len(), 1);
        assert_eq!(manifest.object_site(array).unwrap().anchor(), span(20));
    }

    #[test]
    fn locals_definition_parameters_and_distinct_string_nodes_have_exact_lookups() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let f = declare(
            &mut symbols,
            file,
            "f",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let function_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let p = declare(
            &mut symbols,
            function_scope,
            "p",
            SymbolData::Parameter {
                storage: StorageClass::None,
            },
            ty,
            5,
        );
        let block = symbols
            .create_scope(function_scope, ScopeKind::Block, Some(span(10)))
            .unwrap();
        let local = declare(
            &mut symbols,
            block,
            "local",
            SymbolData::Object {
                storage: StorageClass::Auto,
            },
            ty,
            20,
        );
        let static_local = declare(
            &mut symbols,
            block,
            "saved",
            SymbolData::Object {
                storage: StorageClass::Static,
            },
            ty,
            40,
        );
        let linked = declare(
            &mut symbols,
            block,
            "linked",
            SymbolData::Object {
                storage: StorageClass::Extern,
            },
            ty,
            35,
        );
        let first = string(&arena, "first", 25);
        let same_span_a = string(&arena, "a", 50);
        let same_span_b = string(&arena, "b", 50);
        let non_string = integer(&arena, 0, 70);
        let local_decl = arena_alloc(
            &arena,
            object(
                local,
                "local",
                ty,
                StorageClass::Auto,
                Some(initializer(ty, first)),
                20,
            ),
        );
        let extern_decl = arena_alloc(
            &arena,
            object(linked, "linked", ty, StorageClass::Extern, None, 35),
        );
        let static_decl = arena_alloc(
            &arena,
            object(static_local, "saved", ty, StorageClass::Static, None, 40),
        );
        let body = arena_alloc(
            &arena,
            CHirStmt::Compound(
                vec![
                    CHirBlockItem::Declaration(local_decl),
                    CHirBlockItem::Declaration(extern_decl),
                    CHirBlockItem::Declaration(static_decl),
                    CHirBlockItem::Statement(arena_alloc(
                        &arena,
                        CHirStmt::Expr(same_span_a, span(50)),
                    )),
                    CHirBlockItem::Statement(arena_alloc(
                        &arena,
                        CHirStmt::Expr(same_span_b, span(50)),
                    )),
                    CHirBlockItem::Statement(arena_alloc(
                        &arena,
                        CHirStmt::Expr(non_string, span(70)),
                    )),
                ],
                span(10),
                None,
            ),
        );
        let definition = arena_alloc(
            &arena,
            function(
                f,
                "f",
                ty,
                vec![
                    parameter(Some(p), Some("p"), ty, 5),
                    parameter(None, None, ty, 7),
                ],
                Some(body),
                1,
            ),
        );
        let unit = crate::CHirUnit::new(vec![definition], types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert_eq!(manifest.object_site(p).unwrap().ordinal().raw(), 0);
        assert_eq!(manifest.object_site(local).unwrap().ordinal().raw(), 1);
        assert_eq!(
            manifest.string_literal_site(first).unwrap().ordinal().raw(),
            2
        );
        assert!(manifest.object_site(linked).is_none());
        assert_eq!(
            manifest.object_site(static_local).unwrap().ordinal().raw(),
            3
        );
        assert_eq!(
            manifest
                .string_literal_site(same_span_a)
                .unwrap()
                .ordinal()
                .raw(),
            4
        );
        assert_eq!(
            manifest
                .string_literal_site(same_span_b)
                .unwrap()
                .ordinal()
                .raw(),
            5
        );
        assert!(manifest.string_literal_site(non_string).is_none());
        assert_eq!(
            manifest
                .string_literal_ordinals_at(span(50))
                .iter()
                .map(|ordinal| ordinal.raw())
                .collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert!(matches!(
            manifest.string_literal_site(first).unwrap().kind(),
            AllocationSiteKind::StringLiteral(expression) if std::ptr::eq(expression, first)
        ));
    }

    #[test]
    fn traverses_initializer_plan_for_init_and_nested_expression_shapes() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let f = declare(
            &mut symbols,
            file,
            "f",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let function_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let block = symbols
            .create_scope(function_scope, ScopeKind::Block, Some(span(3)))
            .unwrap();
        let local = declare(
            &mut symbols,
            block,
            "i",
            SymbolData::Object {
                storage: StorageClass::Auto,
            },
            ty,
            10,
        );
        let plan_only = string(&arena, "plan", 15);
        let written_value = integer(&arena, 0, 14);
        let odd_initializer = ObjectInitializer::new(
            WrittenInitializer::List {
                elements: vec![WrittenInitializer::Expression(written_value)],
                designations: vec![None],
                span: span(13),
            },
            InitPlan::new(
                ty,
                vec![InitAction::store_scalar(
                    crate::SubobjectPath::root(),
                    ty,
                    plan_only,
                    span(15),
                )],
            ),
        );
        let local_decl = arena_alloc(
            &arena,
            object(
                local,
                "i",
                ty,
                StorageClass::Auto,
                Some(odd_initializer),
                10,
            ),
        );
        let deeply_nested = string(&arena, "nested", 30);
        let unary = arena_alloc(
            &arena,
            CHirExpr::Unary(UnaryOp::Plus, deeply_nested, span(29)),
        );
        let sizeof = arena_alloc(&arena, CHirExpr::SizeOfExpr(unary, ty, span(28)));
        let condition = arena_alloc(
            &arena,
            CHirExpr::Ternary(
                integer(&arena, 1, 20),
                sizeof,
                integer(&arena, 0, 31),
                span(20),
            ),
        );
        let loop_body = arena_alloc(&arena, CHirStmt::Null(span(40)));
        let body = arena_alloc(
            &arena,
            CHirStmt::For(
                CHirForInit::Declaration(vec![local_decl]),
                Some(condition),
                None,
                loop_body,
                span(5),
            ),
        );
        let definition = arena_alloc(&arena, function(f, "f", ty, Vec::new(), Some(body), 1));
        let unit = crate::CHirUnit::new(vec![definition], types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert!(manifest.string_literal_site(plan_only).is_some());
        assert!(
            manifest.string_literal_site(deeply_nested).is_none(),
            "an unevaluated sizeof operand owns no runtime allocation site"
        );
        assert_eq!(manifest.object_site(local).unwrap().anchor(), span(10));
    }

    #[test]
    fn deep_expression_nesting_is_walked_without_host_recursion() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let f = declare(
            &mut symbols,
            file,
            "deep",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let _function_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let literal = string(&arena, "bottom", 60);
        let mut expression = literal;
        for _ in 0..50_000 {
            expression = arena_alloc(&arena, CHirExpr::Unary(UnaryOp::Plus, expression, span(50)));
        }
        let body = arena_alloc(&arena, CHirStmt::Return(Some(expression), span(70)));
        let definition = arena_alloc(&arena, function(f, "deep", ty, Vec::new(), Some(body), 1));
        let unit = crate::CHirUnit::new(vec![definition], types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert_eq!(manifest.sites().len(), 1);
        assert!(manifest.string_literal_site(literal).is_some());
    }

    #[test]
    fn translation_unit_encounter_order_beats_unrelated_file_id_order() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let first_function = declare(
            &mut symbols,
            file,
            "first",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let second_function = declare(
            &mut symbols,
            file,
            "second",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            2,
        );
        let _first_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let _second_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(3)))
            .unwrap();
        let first_literal = string_at(&arena, "first", span_in(9, 90));
        let second_literal = string_at(&arena, "second", span_in(1, 1));
        let first_body = arena_alloc(&arena, CHirStmt::Return(Some(first_literal), span(10)));
        let second_body = arena_alloc(&arena, CHirStmt::Return(Some(second_literal), span(20)));
        let first = arena_alloc(
            &arena,
            function(first_function, "first", ty, Vec::new(), Some(first_body), 1),
        );
        let second = arena_alloc(
            &arena,
            function(
                second_function,
                "second",
                ty,
                Vec::new(),
                Some(second_body),
                2,
            ),
        );
        let unit = crate::CHirUnit::new(vec![first, second], types, symbols);

        let manifest = AllocationSiteManifest::derive(&unit).unwrap();
        assert_eq!(
            manifest
                .string_literal_site(first_literal)
                .unwrap()
                .ordinal()
                .raw(),
            0,
            "FileId allocation order is not translation-unit encounter order"
        );
        assert_eq!(
            manifest
                .string_literal_site(second_literal)
                .unwrap()
                .ordinal()
                .raw(),
            1
        );
    }

    #[test]
    fn forged_object_symbols_fail_closed_by_exact_coherence_class() {
        let arena = Arena::default();

        let unknown_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut foreign_symbols = SymbolTable::new();
            let foreign_file = foreign_symbols.file_scope();
            let _ = declare(
                &mut foreign_symbols,
                foreign_file,
                "padding",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let unknown = declare(
                &mut foreign_symbols,
                foreign_file,
                "unknown",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                2,
            );
            let declaration = arena_alloc(
                &arena,
                object(unknown, "unknown", ty, StorageClass::None, None, 10),
            );
            let unit = crate::CHirUnit::new(vec![declaration], types, SymbolTable::new());
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            unknown_error,
            AllocationSiteManifestError::UnknownSymbol { .. }
        ));

        let kind_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let wrong_kind = declare(
                &mut symbols,
                file,
                "x",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                10,
            );
            let declaration = arena_alloc(
                &arena,
                object(wrong_kind, "x", ty, StorageClass::None, None, 10),
            );
            let unit = crate::CHirUnit::new(vec![declaration], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            kind_error,
            AllocationSiteManifestError::SymbolKindMismatch {
                expected: SymbolKind::Object,
                actual: SymbolKind::Function,
                ..
            }
        ));

        let spelling_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let symbol = declare(
                &mut symbols,
                file,
                "canonical",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                10,
            );
            let declaration = arena_alloc(
                &arena,
                object(symbol, "forged", ty, StorageClass::None, None, 10),
            );
            let unit = crate::CHirUnit::new(vec![declaration], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            spelling_error,
            AllocationSiteManifestError::SymbolSpellingMismatch { .. }
        ));

        let type_error = {
            let mut types = TypeInterner::new();
            let int = int_type(&mut types);
            let long = long_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let symbol = declare(
                &mut symbols,
                file,
                "x",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                int,
                10,
            );
            let declaration = arena_alloc(
                &arena,
                object(symbol, "x", long, StorageClass::None, None, 10),
            );
            let unit = crate::CHirUnit::new(vec![declaration], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            type_error,
            AllocationSiteManifestError::SymbolTypeMismatch { .. }
        ));

        let scope_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let block = symbols
                .create_scope(file, ScopeKind::Block, Some(span(5)))
                .unwrap();
            let local = declare(
                &mut symbols,
                block,
                "x",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                10,
            );
            let declaration =
                arena_alloc(&arena, object(local, "x", ty, StorageClass::None, None, 10));
            let unit = crate::CHirUnit::new(vec![declaration], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            scope_error,
            AllocationSiteManifestError::FileScopeMismatch { .. }
        ));
    }

    #[test]
    fn forged_function_symbol_is_rejected_before_its_body_is_walked() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let wrong_kind = declare(
            &mut symbols,
            file,
            "f",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let unreachable_literal = string(&arena, "must-not-be-inventoried", 20);
        let body = arena_alloc(
            &arena,
            CHirStmt::Return(Some(unreachable_literal), span(30)),
        );
        let declaration = arena_alloc(
            &arena,
            function(wrong_kind, "f", ty, Vec::new(), Some(body), 1),
        );
        let unit = crate::CHirUnit::new(vec![declaration], types, symbols);

        assert!(matches!(
            AllocationSiteManifest::derive(&unit),
            Err(AllocationSiteManifestError::SymbolKindMismatch {
                expected: SymbolKind::Function,
                actual: SymbolKind::Object,
                ..
            })
        ));
    }

    #[test]
    fn forged_local_storage_and_duplicate_identity_cannot_collapse_sites() {
        let arena = Arena::default();

        let storage_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let function_symbol = declare(
                &mut symbols,
                file,
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let local = declare(
                &mut symbols,
                function_scope,
                "x",
                SymbolData::Object {
                    storage: StorageClass::Auto,
                },
                ty,
                10,
            );
            let local_decl = arena_alloc(
                &arena,
                object(local, "x", ty, StorageClass::Static, None, 10),
            );
            let body = arena_alloc(
                &arena,
                CHirStmt::Compound(vec![CHirBlockItem::Declaration(local_decl)], span(5), None),
            );
            let definition = arena_alloc(
                &arena,
                function(function_symbol, "f", ty, Vec::new(), Some(body), 1),
            );
            let unit = crate::CHirUnit::new(vec![definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            storage_error,
            AllocationSiteManifestError::ObjectStorageMismatch { .. }
        ));

        let duplicate_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let function_symbol = declare(
                &mut symbols,
                file,
                "g",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let local = declare(
                &mut symbols,
                function_scope,
                "x",
                SymbolData::Object {
                    storage: StorageClass::Auto,
                },
                ty,
                10,
            );
            let first = arena_alloc(&arena, object(local, "x", ty, StorageClass::Auto, None, 10));
            let duplicate =
                arena_alloc(&arena, object(local, "x", ty, StorageClass::Auto, None, 20));
            let body = arena_alloc(
                &arena,
                CHirStmt::Compound(
                    vec![
                        CHirBlockItem::Declaration(first),
                        CHirBlockItem::Declaration(duplicate),
                    ],
                    span(5),
                    None,
                ),
            );
            let definition = arena_alloc(
                &arena,
                function(function_symbol, "g", ty, Vec::new(), Some(body), 1),
            );
            let unit = crate::CHirUnit::new(vec![definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            duplicate_error,
            AllocationSiteManifestError::DuplicateLocalObject { .. }
        ));
    }

    #[test]
    fn definition_parameters_require_function_scope_storage_and_unique_owner() {
        let arena = Arena::default();

        let storage_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let f = declare(
                &mut symbols,
                file,
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let p = declare(
                &mut symbols,
                function_scope,
                "p",
                SymbolData::Parameter {
                    storage: StorageClass::Auto,
                },
                ty,
                5,
            );
            let body = arena_alloc(&arena, CHirStmt::Null(span(10)));
            let definition = arena_alloc(
                &arena,
                function(
                    f,
                    "f",
                    ty,
                    vec![parameter(Some(p), Some("p"), ty, 5)],
                    Some(body),
                    1,
                ),
            );
            let unit = crate::CHirUnit::new(vec![definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            storage_error,
            AllocationSiteManifestError::ParameterStorageMismatch { .. }
        ));

        let scope_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let f = declare(
                &mut symbols,
                file,
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let _function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let prototype_scope = symbols
                .create_scope(file, ScopeKind::FunctionPrototype, Some(span(2)))
                .unwrap();
            let p = declare(
                &mut symbols,
                prototype_scope,
                "p",
                SymbolData::Parameter {
                    storage: StorageClass::None,
                },
                ty,
                5,
            );
            let body = arena_alloc(&arena, CHirStmt::Null(span(10)));
            let definition = arena_alloc(
                &arena,
                function(
                    f,
                    "f",
                    ty,
                    vec![parameter(Some(p), Some("p"), ty, 5)],
                    Some(body),
                    1,
                ),
            );
            let unit = crate::CHirUnit::new(vec![definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            scope_error,
            AllocationSiteManifestError::ParameterScopeMismatch { .. }
        ));

        let owner_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let first_function = declare(
                &mut symbols,
                file,
                "first",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let second_function = declare(
                &mut symbols,
                file,
                "second",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                2,
            );
            let first_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let _second_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(3)))
                .unwrap();
            let p = declare(
                &mut symbols,
                first_scope,
                "p",
                SymbolData::Parameter {
                    storage: StorageClass::None,
                },
                ty,
                5,
            );
            let first_body = arena_alloc(&arena, CHirStmt::Null(span(10)));
            let second_body = arena_alloc(&arena, CHirStmt::Null(span(20)));
            let first = arena_alloc(
                &arena,
                function(
                    first_function,
                    "first",
                    ty,
                    vec![parameter(Some(p), Some("p"), ty, 5)],
                    Some(first_body),
                    1,
                ),
            );
            let second = arena_alloc(
                &arena,
                function(
                    second_function,
                    "second",
                    ty,
                    vec![parameter(Some(p), Some("p"), ty, 5)],
                    Some(second_body),
                    2,
                ),
            );
            let unit = crate::CHirUnit::new(vec![first, second], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            owner_error,
            AllocationSiteManifestError::ParameterOwnedByAnotherFunction { .. }
        ));
    }

    #[test]
    fn local_object_cannot_be_structurally_mounted_under_another_function() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let first_function = declare(
            &mut symbols,
            file,
            "first",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let second_function = declare(
            &mut symbols,
            file,
            "second",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            11,
        );
        let first_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let _second_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(12)))
            .unwrap();
        let first_local = declare(
            &mut symbols,
            first_scope,
            "x",
            SymbolData::Object {
                storage: StorageClass::Auto,
            },
            ty,
            20,
        );
        let misplaced = arena_alloc(
            &arena,
            object(first_local, "x", ty, StorageClass::Auto, None, 20),
        );
        let first_body = arena_alloc(&arena, CHirStmt::Null(span(5)));
        let second_body = arena_alloc(
            &arena,
            CHirStmt::Compound(vec![CHirBlockItem::Declaration(misplaced)], span(15), None),
        );
        let first = arena_alloc(
            &arena,
            function(first_function, "first", ty, Vec::new(), Some(first_body), 1),
        );
        let second = arena_alloc(
            &arena,
            function(
                second_function,
                "second",
                ty,
                Vec::new(),
                Some(second_body),
                11,
            ),
        );
        let unit = crate::CHirUnit::new(vec![first, second], types, symbols);

        assert!(matches!(
            AllocationSiteManifest::derive(&unit),
            Err(AllocationSiteManifestError::LocalScopeMismatch {
                symbol,
                ..
            }) if symbol == first_local
        ));
    }

    #[test]
    fn one_declaration_node_cannot_be_mounted_twice_in_any_context() {
        let arena = Arena::default();

        let file_alias_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let x = declare(
                &mut symbols,
                file,
                "x",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let declaration = arena_alloc(&arena, object(x, "x", ty, StorageClass::None, None, 1));
            let unit = crate::CHirUnit::new(vec![declaration, declaration], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            file_alias_error,
            AllocationSiteManifestError::DeclarationNodeReused { .. }
        ));

        let local_alias_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let f = declare(
                &mut symbols,
                file,
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let x = declare(
                &mut symbols,
                function_scope,
                "x",
                SymbolData::Object {
                    storage: StorageClass::Auto,
                },
                ty,
                5,
            );
            let declaration = arena_alloc(&arena, object(x, "x", ty, StorageClass::Auto, None, 5));
            let body = arena_alloc(
                &arena,
                CHirStmt::Compound(
                    vec![
                        CHirBlockItem::Declaration(declaration),
                        CHirBlockItem::Declaration(declaration),
                    ],
                    span(4),
                    None,
                ),
            );
            let definition = arena_alloc(&arena, function(f, "f", ty, Vec::new(), Some(body), 1));
            let unit = crate::CHirUnit::new(vec![definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            local_alias_error,
            AllocationSiteManifestError::DeclarationNodeReused { .. }
        ));

        let function_alias_error = {
            let mut types = TypeInterner::new();
            let ty = int_type(&mut types);
            let mut symbols = SymbolTable::new();
            let file = symbols.file_scope();
            let f = declare(
                &mut symbols,
                file,
                "f",
                SymbolData::Function {
                    storage: StorageClass::None,
                },
                ty,
                1,
            );
            let _function_scope = symbols
                .create_scope(file, ScopeKind::Function, Some(span(2)))
                .unwrap();
            let body = arena_alloc(&arena, CHirStmt::Null(span(5)));
            let definition = arena_alloc(&arena, function(f, "f", ty, Vec::new(), Some(body), 1));
            let unit = crate::CHirUnit::new(vec![definition, definition], types, symbols);
            AllocationSiteManifest::derive(&unit).unwrap_err()
        };
        assert!(matches!(
            function_alias_error,
            AllocationSiteManifestError::DeclarationNodeReused { .. }
        ));
    }

    #[test]
    fn one_function_symbol_cannot_have_two_distinct_definitions() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let f = declare(
            &mut symbols,
            file,
            "f",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let _function_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let first_body = arena_alloc(&arena, CHirStmt::Null(span(10)));
        let second_body = arena_alloc(&arena, CHirStmt::Null(span(20)));
        let first = arena_alloc(
            &arena,
            function(f, "f", ty, Vec::new(), Some(first_body), 1),
        );
        let second = arena_alloc(
            &arena,
            function(f, "f", ty, Vec::new(), Some(second_body), 1),
        );
        let unit = crate::CHirUnit::new(vec![first, second], types, symbols);

        assert!(matches!(
            AllocationSiteManifest::derive(&unit),
            Err(AllocationSiteManifestError::DuplicateFunctionDefinition {
                function,
                ..
            }) if function == f
        ));
    }

    #[test]
    fn one_statement_node_cannot_be_a_body_of_two_functions() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let ty = int_type(&mut types);
        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let first_function = declare(
            &mut symbols,
            file,
            "first",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            1,
        );
        let second_function = declare(
            &mut symbols,
            file,
            "second",
            SymbolData::Function {
                storage: StorageClass::None,
            },
            ty,
            11,
        );
        let first_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(2)))
            .unwrap();
        let _second_scope = symbols
            .create_scope(file, ScopeKind::Function, Some(span(12)))
            .unwrap();
        let local = declare(
            &mut symbols,
            first_scope,
            "x",
            SymbolData::Object {
                storage: StorageClass::Auto,
            },
            ty,
            20,
        );
        let local_declaration =
            arena_alloc(&arena, object(local, "x", ty, StorageClass::Auto, None, 20));
        let shared_body = arena_alloc(
            &arena,
            CHirStmt::Compound(
                vec![CHirBlockItem::Declaration(local_declaration)],
                span(19),
                None,
            ),
        );
        let first = arena_alloc(
            &arena,
            function(
                first_function,
                "first",
                ty,
                Vec::new(),
                Some(shared_body),
                1,
            ),
        );
        let second = arena_alloc(
            &arena,
            function(
                second_function,
                "second",
                ty,
                Vec::new(),
                Some(shared_body),
                11,
            ),
        );
        let unit = crate::CHirUnit::new(vec![first, second], types, symbols);

        assert!(matches!(
            AllocationSiteManifest::derive(&unit),
            Err(AllocationSiteManifestError::StatementNodeReused {
                first_function: first,
                duplicate_function: second,
                ..
            }) if first == first_function && second == second_function
        ));
    }

    #[test]
    fn foreign_memory_place_fails_before_allocation_traversal() {
        let arena = Arena::default();
        let mut types = TypeInterner::new();
        let int = int_type(&mut types);
        let long = long_type(&mut types);

        let mut foreign_symbols = SymbolTable::new();
        let foreign_file = foreign_symbols.file_scope();
        let foreign = declare(
            &mut foreign_symbols,
            foreign_file,
            "foreign",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            long,
            1,
        );
        let foreign_place = MemoryPlace::derive(
            &foreign_symbols,
            &types,
            PlaceRoot::Object { symbol: foreign },
            Vec::new(),
            long,
        )
        .unwrap();

        let mut symbols = SymbolTable::new();
        let file = symbols.file_scope();
        let host = declare(
            &mut symbols,
            file,
            "host",
            SymbolData::Object {
                storage: StorageClass::None,
            },
            int,
            10,
        );
        assert_eq!(foreign.raw(), host.raw(), "fixture requires colliding IDs");

        let value = integer(&arena, 1, 21);
        let forged = arena_alloc(
            &arena,
            CHirExpr::MemoryAssign {
                op: AssignmentOp::Simple,
                place: foreign_place,
                target_span: span(20),
                value,
                facts: ExprFacts::value(long),
                span: span(20),
            },
        );
        let declaration = arena_alloc(
            &arena,
            object(
                host,
                "host",
                int,
                StorageClass::None,
                Some(initializer(int, forged)),
                10,
            ),
        );
        let unit = crate::CHirUnit::new(vec![declaration], types, symbols);

        assert!(matches!(
            AllocationSiteManifest::derive(&unit),
            Err(AllocationSiteManifestError::InvalidMemoryPlace { span: actual })
                if actual == span(20)
        ));
    }
}
