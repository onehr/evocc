//! Typed semantic facts shared by C-HIR construction and later lowering.
//!
//! This module deliberately contains no name-resolution pass.  It provides
//! the deterministic identities and checked value objects that such a pass
//! can populate without changing the existing C-HIR node ABI.

use std::{collections::HashMap, fmt, sync::Arc};

use evocc_common::SourceSpan;
use evocc_types::{TagId, TypeId, TypeInterner, TypeKind};

use crate::{BinaryOp, StorageClass};

/// Deterministic, table-local identity of one ordinary C identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SymbolId(u32);

impl SymbolId {
    /// Return the table-local numeric identity.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// Deterministic, table-local identity of one lexical scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ScopeId(u32);

impl ScopeId {
    /// Return the table-local numeric identity.
    #[must_use]
    pub fn raw(self) -> u32 {
        self.0
    }
}

/// C lexical scope kinds relevant to ordinary identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScopeKind {
    File,
    Function,
    Block,
    For,
    FunctionPrototype,
}

/// Kinds sharing C's ordinary-identifier namespace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Object,
    Function,
    Parameter,
    Typedef,
    Enumerator,
}

/// Kind-specific semantic payload that must survive name resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolData {
    Object { storage: StorageClass },
    Function { storage: StorageClass },
    Parameter { storage: StorageClass },
    Typedef,
    Enumerator { value: i128 },
}

impl SymbolData {
    #[must_use]
    pub fn kind(self) -> SymbolKind {
        match self {
            Self::Object { .. } => SymbolKind::Object,
            Self::Function { .. } => SymbolKind::Function,
            Self::Parameter { .. } => SymbolKind::Parameter,
            Self::Typedef => SymbolKind::Typedef,
            Self::Enumerator { .. } => SymbolKind::Enumerator,
        }
    }

    #[must_use]
    pub fn storage_class(self) -> Option<StorageClass> {
        match self {
            Self::Object { storage } | Self::Function { storage } | Self::Parameter { storage } => {
                Some(storage)
            }
            Self::Typedef | Self::Enumerator { .. } => None,
        }
    }

    #[must_use]
    pub fn enumerator_value(self) -> Option<i128> {
        match self {
            Self::Enumerator { value } => Some(value),
            _ => None,
        }
    }
}

/// One source declaration with a stable semantic identity.
#[derive(Debug, Clone, PartialEq)]
pub struct Symbol {
    id: SymbolId,
    scope: ScopeId,
    spelling: String,
    data: SymbolData,
    ty: TypeId,
    declared_alignment: u32,
    span: SourceSpan,
}

impl Symbol {
    #[must_use]
    pub fn id(&self) -> SymbolId {
        self.id
    }

    #[must_use]
    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    #[must_use]
    pub fn kind(&self) -> SymbolKind {
        self.data.kind()
    }

    #[must_use]
    pub fn data(&self) -> SymbolData {
        self.data
    }

    #[must_use]
    pub fn storage_class(&self) -> Option<StorageClass> {
        self.data.storage_class()
    }

    #[must_use]
    pub fn enumerator_value(&self) -> Option<i128> {
        self.data.enumerator_value()
    }

    #[must_use]
    pub fn ty(&self) -> TypeId {
        self.ty
    }

    /// Maximum alignment written on this object's declarations, independent of its type.
    #[must_use]
    pub fn declared_alignment(&self) -> u32 {
        self.declared_alignment
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        self.span
    }
}

/// One declaration in C's independent tag namespace.
#[derive(Debug, Clone, PartialEq)]
pub struct TagBinding {
    scope: ScopeId,
    spelling: String,
    tag: TagId,
    span: SourceSpan,
}

impl TagBinding {
    #[must_use]
    pub fn scope(&self) -> ScopeId {
        self.scope
    }

    #[must_use]
    pub fn spelling(&self) -> &str {
        &self.spelling
    }

    #[must_use]
    pub fn tag(&self) -> TagId {
        self.tag
    }

    #[must_use]
    pub fn span(&self) -> SourceSpan {
        self.span
    }
}

/// One lexical scope and its declarations in exact source order.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    id: ScopeId,
    parent: Option<ScopeId>,
    kind: ScopeKind,
    span: Option<SourceSpan>,
    function_owner: Option<SymbolId>,
    symbols: Vec<SymbolId>,
    tags: Vec<TagId>,
    tag_names: HashMap<String, TagId>,
}

impl Scope {
    #[must_use]
    pub fn id(&self) -> ScopeId {
        self.id
    }

    #[must_use]
    pub fn parent(&self) -> Option<ScopeId> {
        self.parent
    }

    #[must_use]
    pub fn kind(&self) -> ScopeKind {
        self.kind
    }

    #[must_use]
    pub fn span(&self) -> Option<SourceSpan> {
        self.span
    }

    /// Semantic owner of a definition's parameter scope. Source spans are not
    /// identities: one macro invocation may produce several function bodies.
    #[must_use]
    pub fn function_owner(&self) -> Option<SymbolId> {
        self.function_owner
    }

    #[must_use]
    pub fn symbols(&self) -> &[SymbolId] {
        &self.symbols
    }

    #[must_use]
    pub fn tags(&self) -> &[TagId] {
        &self.tags
    }
}

/// A structural failure while extending or querying a [`SymbolTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolTableError {
    UnknownScope(ScopeId),
    UnknownSymbol(SymbolId),
    BuiltinLexicalBinding(SymbolId),
    InvalidFunctionScope {
        scope: ScopeId,
        function: SymbolId,
    },
    DuplicateInScope {
        scope: ScopeId,
        existing: SymbolId,
        spelling: String,
    },
    DuplicateTagInScope {
        scope: ScopeId,
        existing: TagId,
        spelling: String,
    },
    InvalidCheckpoint,
    ScopeTableExhausted,
    SymbolTableExhausted,
}

impl fmt::Display for SymbolTableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownScope(scope) => write!(formatter, "unknown scope ID {}", scope.raw()),
            Self::UnknownSymbol(symbol) => {
                write!(formatter, "unknown symbol ID {}", symbol.raw())
            }
            Self::BuiltinLexicalBinding(symbol) => {
                write!(
                    formatter,
                    "builtin symbol {} cannot bind a lexical scope",
                    symbol.raw()
                )
            }
            Self::InvalidFunctionScope { scope, function } => write!(
                formatter,
                "scope {} cannot belong to function {}",
                scope.raw(),
                function.raw()
            ),
            Self::DuplicateInScope {
                scope,
                existing,
                spelling,
            } => write!(
                formatter,
                "ordinary identifier {spelling:?} duplicates symbol {} in scope {}",
                existing.raw(),
                scope.raw()
            ),
            Self::DuplicateTagInScope {
                scope,
                existing,
                spelling,
            } => write!(
                formatter,
                "tag {spelling:?} duplicates tag {} in scope {}",
                existing.raw(),
                scope.raw()
            ),
            Self::InvalidCheckpoint => {
                formatter.write_str("checkpoint does not describe this symbol-table state")
            }
            Self::ScopeTableExhausted => formatter.write_str("scope table exhausted u32 IDs"),
            Self::SymbolTableExhausted => formatter.write_str("symbol table exhausted u32 IDs"),
        }
    }
}

impl std::error::Error for SymbolTableError {}

#[derive(Debug)]
struct SymbolTableIdentity(u8);

/// Exact table snapshot used to roll back speculative parser work.
#[derive(Debug, Clone)]
pub struct SymbolTableCheckpoint {
    identity: Arc<SymbolTableIdentity>,
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
    tags: Vec<TagBinding>,
    builtin_functions: HashMap<String, SymbolId>,
}

/// One table-local, last-in-first-out speculative mutation boundary.
///
/// Unlike [`SymbolTableCheckpoint`], this token does not own an independently
/// restorable branch. It is the cheap parser path: nested transactions must be
/// committed or rolled back in reverse order.
#[derive(Debug)]
pub struct SymbolTableTransaction {
    identity: Arc<SymbolTableIdentity>,
    undo_marker: usize,
    depth: usize,
}

#[derive(Debug)]
enum SymbolTableUndo {
    ScopeCreated,
    BuiltinDeclared,
    SymbolDeclared {
        scope: ScopeId,
    },
    ExistingBound {
        scope: ScopeId,
    },
    SymbolTypeUpdated {
        symbol: SymbolId,
        previous: TypeId,
    },
    SymbolAlignmentUpdated {
        symbol: SymbolId,
        previous: u32,
    },
    FunctionScopeBound {
        scope: ScopeId,
        previous: Option<SymbolId>,
        previous_kind: ScopeKind,
    },
    TagDeclared {
        scope: ScopeId,
    },
}

/// Deterministic lexical-scope and ordinary-identifier table.
///
/// Vectors, not hash-table iteration, are authoritative for IDs and source
/// order.  A declaration is appended only after all checks have succeeded, so
/// a failed declaration leaves both the global and per-scope sequences intact.
#[derive(Debug)]
pub struct SymbolTable {
    identity: Arc<SymbolTableIdentity>,
    scopes: Vec<Scope>,
    symbols: Vec<Symbol>,
    tags: Vec<TagBinding>,
    builtin_functions: HashMap<String, SymbolId>,
    undo_log: Vec<SymbolTableUndo>,
    transaction_markers: Vec<usize>,
}

impl SymbolTable {
    #[must_use]
    pub fn new() -> Self {
        Self {
            identity: Arc::new(SymbolTableIdentity(0)),
            scopes: vec![Scope {
                id: ScopeId(0),
                parent: None,
                kind: ScopeKind::File,
                span: None,
                function_owner: None,
                symbols: Vec::new(),
                tags: Vec::new(),
                tag_names: HashMap::new(),
            }],
            symbols: Vec::new(),
            tags: Vec::new(),
            builtin_functions: HashMap::new(),
            undo_log: Vec::new(),
            transaction_markers: Vec::new(),
        }
    }

    #[must_use]
    pub fn file_scope(&self) -> ScopeId {
        ScopeId(0)
    }

    pub fn create_scope(
        &mut self,
        parent: ScopeId,
        kind: ScopeKind,
        span: Option<SourceSpan>,
    ) -> Result<ScopeId, SymbolTableError> {
        self.scope_index(parent)?;
        let raw =
            u32::try_from(self.scopes.len()).map_err(|_| SymbolTableError::ScopeTableExhausted)?;
        let id = ScopeId(raw);
        self.scopes.push(Scope {
            id,
            parent: Some(parent),
            kind,
            span,
            function_owner: None,
            symbols: Vec::new(),
            tags: Vec::new(),
            tag_names: HashMap::new(),
        });
        self.record_undo(SymbolTableUndo::ScopeCreated);
        Ok(id)
    }

    /// Bind a definition's lexical scope without interpreting source offsets.
    pub fn bind_function_scope(
        &mut self,
        scope: ScopeId,
        function: SymbolId,
    ) -> Result<(), SymbolTableError> {
        let index = self.scope_index(scope)?;
        let symbol = self.symbol(function)?;
        let entry = &self.scopes[index];
        if !matches!(
            entry.kind,
            ScopeKind::Function | ScopeKind::FunctionPrototype
        ) || entry.parent != Some(self.file_scope())
            || symbol.kind() != SymbolKind::Function
            || self.is_builtin_function(function)
            || entry.function_owner.is_some_and(|owner| owner != function)
        {
            return Err(SymbolTableError::InvalidFunctionScope { scope, function });
        }
        let previous = entry.function_owner;
        let previous_kind = entry.kind;
        // A declarator is parsed before trailing attributes or outer layers
        // reveal the body. Binding its definition completes that provisional
        // prototype scope, without changing its symbols or identity.
        self.scopes[index].kind = ScopeKind::Function;
        self.scopes[index].function_owner = Some(function);
        self.record_undo(SymbolTableUndo::FunctionScopeBound {
            scope,
            previous,
            previous_kind,
        });
        Ok(())
    }

    pub fn declare(
        &mut self,
        scope: ScopeId,
        spelling: impl Into<String>,
        data: SymbolData,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<SymbolId, SymbolTableError> {
        let spelling = spelling.into();
        let scope_index = self.scope_index(scope)?;

        if let Some(existing) = self.scopes[scope_index]
            .symbols
            .iter()
            .copied()
            .find(|id| self.symbols[id.raw() as usize].spelling == spelling)
        {
            return Err(SymbolTableError::DuplicateInScope {
                scope,
                existing,
                spelling,
            });
        }

        let raw = u32::try_from(self.symbols.len())
            .map_err(|_| SymbolTableError::SymbolTableExhausted)?;
        let id = SymbolId(raw);
        self.symbols.push(Symbol {
            id,
            scope,
            spelling,
            data,
            ty,
            declared_alignment: 1,
            span,
        });
        self.scopes[scope_index].symbols.push(id);
        self.record_undo(SymbolTableUndo::SymbolDeclared { scope });
        Ok(id)
    }

    /// Intern a library prototype in the unit's separate builtin namespace.
    /// The file scope ID is only its unit anchor: no lexical binding is added.
    pub fn declare_builtin_function(
        &mut self,
        spelling: &str,
        ty: TypeId,
        span: SourceSpan,
    ) -> Result<(SymbolId, bool), SymbolTableError> {
        if let Some(&symbol) = self.builtin_functions.get(spelling) {
            return Ok((symbol, false));
        }
        let raw = u32::try_from(self.symbols.len())
            .map_err(|_| SymbolTableError::SymbolTableExhausted)?;
        let id = SymbolId(raw);
        self.symbols.push(Symbol {
            id,
            scope: self.file_scope(),
            spelling: spelling.to_owned(),
            data: SymbolData::Function {
                storage: StorageClass::Extern,
            },
            ty,
            declared_alignment: 1,
            span,
        });
        self.builtin_functions.insert(spelling.to_owned(), id);
        self.record_undo(SymbolTableUndo::BuiltinDeclared);
        Ok((id, true))
    }

    #[must_use]
    pub fn is_builtin_function(&self, symbol: SymbolId) -> bool {
        self.symbol(symbol)
            .is_ok_and(|entry| self.builtin_functions.get(entry.spelling()) == Some(&symbol))
    }

    /// Make an already-declared linked entity visible in another lexical
    /// scope without allocating a second semantic identity.
    ///
    /// C block-scope `extern` declarations and block-scope function
    /// declarations use this operation to join the file-scope entity while
    /// retaining normal lexical lookup in the block.
    pub fn bind_existing(
        &mut self,
        scope: ScopeId,
        symbol: SymbolId,
    ) -> Result<SymbolId, SymbolTableError> {
        let scope_index = self.scope_index(scope)?;
        let declaration = self.symbol(symbol)?;
        if self.is_builtin_function(symbol) {
            return Err(SymbolTableError::BuiltinLexicalBinding(symbol));
        }
        let spelling = declaration.spelling.clone();

        if let Some(existing) = self.scopes[scope_index]
            .symbols
            .iter()
            .copied()
            .find(|id| self.symbols[id.raw() as usize].spelling == spelling)
        {
            return if existing == symbol {
                Ok(symbol)
            } else {
                Err(SymbolTableError::DuplicateInScope {
                    scope,
                    existing,
                    spelling,
                })
            };
        }

        self.scopes[scope_index].symbols.push(symbol);
        self.record_undo(SymbolTableUndo::ExistingBound { scope });
        Ok(symbol)
    }

    /// Replace the canonical composite type associated with an existing C
    /// entity while preserving its stable [`SymbolId`].
    pub fn update_symbol_type(
        &mut self,
        symbol: SymbolId,
        ty: TypeId,
    ) -> Result<(), SymbolTableError> {
        let previous = {
            let declaration = self
                .symbols
                .get_mut(symbol.raw() as usize)
                .ok_or(SymbolTableError::UnknownSymbol(symbol))?;
            let previous = declaration.ty;
            declaration.ty = ty;
            previous
        };
        self.record_undo(SymbolTableUndo::SymbolTypeUpdated { symbol, previous });
        Ok(())
    }

    /// Accumulate object or function alignment without changing type identity.
    pub fn raise_declared_alignment(
        &mut self,
        symbol: SymbolId,
        alignment: u32,
    ) -> Result<(), SymbolTableError> {
        let declaration = self
            .symbols
            .get_mut(symbol.raw() as usize)
            .ok_or(SymbolTableError::UnknownSymbol(symbol))?;
        let previous = declaration.declared_alignment;
        if alignment > previous {
            declaration.declared_alignment = alignment;
            self.record_undo(SymbolTableUndo::SymbolAlignmentUpdated { symbol, previous });
        }
        Ok(())
    }

    /// Bind a nominal tag name in C's independent tag namespace.
    pub fn declare_tag(
        &mut self,
        scope: ScopeId,
        spelling: impl Into<String>,
        tag: TagId,
        span: SourceSpan,
    ) -> Result<TagId, SymbolTableError> {
        let spelling = spelling.into();
        let scope_index = self.scope_index(scope)?;
        if let Some(existing) = self.scopes[scope_index].tag_names.get(&spelling).copied() {
            return Err(SymbolTableError::DuplicateTagInScope {
                scope,
                existing,
                spelling,
            });
        }

        self.tags.push(TagBinding {
            scope,
            spelling: spelling.clone(),
            tag,
            span,
        });
        self.scopes[scope_index].tags.push(tag);
        self.scopes[scope_index].tag_names.insert(spelling, tag);
        self.record_undo(SymbolTableUndo::TagDeclared { scope });
        Ok(tag)
    }

    /// Resolve the nearest declaration of `spelling` in the lexical parent
    /// chain.  Invalid scope IDs are errors rather than implicit misses.
    pub fn resolve(
        &self,
        scope: ScopeId,
        spelling: &str,
    ) -> Result<Option<SymbolId>, SymbolTableError> {
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            let scope = self.scope(scope_id)?;
            if let Some(symbol) = scope
                .symbols
                .iter()
                .rev()
                .copied()
                .find(|id| self.symbols[id.raw() as usize].spelling == spelling)
            {
                return Ok(Some(symbol));
            }
            current = scope.parent;
        }
        Ok(None)
    }

    /// Resolve the nearest tag declaration independently of ordinary names.
    pub fn resolve_tag(
        &self,
        scope: ScopeId,
        spelling: &str,
    ) -> Result<Option<TagId>, SymbolTableError> {
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            let scope = self.scope(scope_id)?;
            if let Some(tag) = scope.tag_names.get(spelling).copied() {
                return Ok(Some(tag));
            }
            current = scope.parent;
        }
        Ok(None)
    }

    /// Capture a table-local rollback point for speculative parsing.
    #[must_use]
    pub fn checkpoint(&self) -> SymbolTableCheckpoint {
        SymbolTableCheckpoint {
            identity: Arc::clone(&self.identity),
            scopes: self.scopes.clone(),
            symbols: self.symbols.clone(),
            tags: self.tags.clone(),
            builtin_functions: self.builtin_functions.clone(),
        }
    }

    /// Begin a cheap, table-local speculative transaction.
    ///
    /// Transactions may nest, but their tokens are linear and must be closed
    /// in last-in-first-out order. Use [`Self::checkpoint`] when a branch must
    /// remain independently restorable after another branch is selected.
    #[must_use]
    pub fn begin_transaction(&mut self) -> SymbolTableTransaction {
        let undo_marker = self.undo_log.len();
        self.transaction_markers.push(undo_marker);
        SymbolTableTransaction {
            identity: Arc::clone(&self.identity),
            undo_marker,
            depth: self.transaction_markers.len(),
        }
    }

    /// Keep all mutations since the matching transaction began.
    pub fn commit_transaction(
        &mut self,
        transaction: &SymbolTableTransaction,
    ) -> Result<(), SymbolTableError> {
        self.validate_transaction(transaction)?;
        self.transaction_markers.pop();
        if self.transaction_markers.is_empty() {
            self.undo_log.clear();
        }
        Ok(())
    }

    /// Undo all mutations since the matching transaction began.
    pub fn rollback_transaction(
        &mut self,
        transaction: &SymbolTableTransaction,
    ) -> Result<(), SymbolTableError> {
        self.validate_transaction(transaction)?;
        while self.undo_log.len() > transaction.undo_marker {
            let undo = self
                .undo_log
                .pop()
                .expect("transaction marker is bounded by the undo log");
            self.apply_undo(undo);
        }
        self.transaction_markers.pop();
        if self.transaction_markers.is_empty() {
            self.undo_log.clear();
        }
        Ok(())
    }

    /// Restore exact scope and declaration order to a prior checkpoint.
    pub fn rollback(&mut self, checkpoint: SymbolTableCheckpoint) -> Result<(), SymbolTableError> {
        if !Arc::ptr_eq(&self.identity, &checkpoint.identity)
            || !self.transaction_markers.is_empty()
        {
            return Err(SymbolTableError::InvalidCheckpoint);
        }
        self.scopes = checkpoint.scopes;
        self.symbols = checkpoint.symbols;
        self.tags = checkpoint.tags;
        self.builtin_functions = checkpoint.builtin_functions;
        Ok(())
    }

    pub fn scope(&self, id: ScopeId) -> Result<&Scope, SymbolTableError> {
        self.scopes
            .get(id.raw() as usize)
            .ok_or(SymbolTableError::UnknownScope(id))
    }

    pub fn symbol(&self, id: SymbolId) -> Result<&Symbol, SymbolTableError> {
        self.symbols
            .get(id.raw() as usize)
            .ok_or(SymbolTableError::UnknownSymbol(id))
    }

    pub fn scope_symbols(&self, id: ScopeId) -> Result<&[SymbolId], SymbolTableError> {
        Ok(self.scope(id)?.symbols())
    }

    /// Whether this exact scope binds the symbol, without searching parents.
    /// A linked entity may be bound in several scopes; `Symbol::scope` records
    /// its first declaration, not every subsequent binding.
    pub fn is_bound_in_scope(
        &self,
        symbol: SymbolId,
        scope: ScopeId,
    ) -> Result<bool, SymbolTableError> {
        self.symbol(symbol)?;
        Ok(self.scope_symbols(scope)?.contains(&symbol))
    }

    pub fn scope_tags(&self, id: ScopeId) -> Result<&[TagId], SymbolTableError> {
        Ok(self.scope(id)?.tags())
    }

    #[must_use]
    pub fn scopes_in_source_order(&self) -> &[Scope] {
        &self.scopes
    }

    #[must_use]
    pub fn symbols_in_source_order(&self) -> &[Symbol] {
        &self.symbols
    }

    #[must_use]
    pub fn tags_in_source_order(&self) -> &[TagBinding] {
        &self.tags
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    #[must_use]
    pub fn tag_len(&self) -> usize {
        self.tags.len()
    }

    fn scope_index(&self, id: ScopeId) -> Result<usize, SymbolTableError> {
        let index = id.raw() as usize;
        if index < self.scopes.len() {
            Ok(index)
        } else {
            Err(SymbolTableError::UnknownScope(id))
        }
    }

    fn record_undo(&mut self, undo: SymbolTableUndo) {
        if !self.transaction_markers.is_empty() {
            self.undo_log.push(undo);
        }
    }

    fn validate_transaction(
        &self,
        transaction: &SymbolTableTransaction,
    ) -> Result<(), SymbolTableError> {
        let is_top = Arc::ptr_eq(&self.identity, &transaction.identity)
            && self.transaction_markers.len() == transaction.depth
            && self.transaction_markers.last().copied() == Some(transaction.undo_marker)
            && self.undo_log.len() >= transaction.undo_marker;
        if is_top {
            Ok(())
        } else {
            Err(SymbolTableError::InvalidCheckpoint)
        }
    }

    fn apply_undo(&mut self, undo: SymbolTableUndo) {
        match undo {
            SymbolTableUndo::BuiltinDeclared => {
                let symbol = self.symbols.pop().expect("declared builtin remains last");
                self.builtin_functions.remove(&symbol.spelling);
            }
            SymbolTableUndo::ScopeCreated => {
                let scope = self.scopes.pop().expect("created scope remains last");
                debug_assert!(scope.symbols.is_empty());
                debug_assert!(scope.tags.is_empty());
            }
            SymbolTableUndo::SymbolDeclared { scope } => {
                let symbol = self.symbols.pop().expect("declared symbol remains last");
                debug_assert_eq!(symbol.scope, scope);
                let bound = self.scopes[scope.raw() as usize]
                    .symbols
                    .pop()
                    .expect("declared symbol remains bound in its scope");
                debug_assert_eq!(bound, symbol.id);
            }
            SymbolTableUndo::ExistingBound { scope } => {
                self.scopes[scope.raw() as usize]
                    .symbols
                    .pop()
                    .expect("existing symbol binding remains last");
            }
            SymbolTableUndo::SymbolTypeUpdated { symbol, previous } => {
                self.symbols[symbol.raw() as usize].ty = previous;
            }
            SymbolTableUndo::SymbolAlignmentUpdated { symbol, previous } => {
                self.symbols[symbol.raw() as usize].declared_alignment = previous;
            }
            SymbolTableUndo::FunctionScopeBound {
                scope,
                previous,
                previous_kind,
            } => {
                self.scopes[scope.raw() as usize].function_owner = previous;
                self.scopes[scope.raw() as usize].kind = previous_kind;
            }
            SymbolTableUndo::TagDeclared { scope } => {
                let binding = self.tags.pop().expect("declared tag remains last");
                debug_assert_eq!(binding.scope, scope);
                let tag = self.scopes[scope.raw() as usize]
                    .tags
                    .pop()
                    .expect("declared tag remains bound in its scope");
                debug_assert_eq!(tag, binding.tag);
                let named = self.scopes[scope.raw() as usize]
                    .tag_names
                    .remove(&binding.spelling)
                    .expect("declared tag remains indexed in its scope");
                debug_assert_eq!(named, binding.tag);
            }
        }
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for SymbolTable {
    /// Clone semantic contents into an independent table lineage.  Checkpoints
    /// never cross between the original and the clone.
    fn clone(&self) -> Self {
        Self {
            identity: Arc::new(SymbolTableIdentity(self.identity.0)),
            scopes: self.scopes.clone(),
            symbols: self.symbols.clone(),
            tags: self.tags.clone(),
            builtin_functions: self.builtin_functions.clone(),
            undo_log: Vec::new(),
            transaction_markers: Vec::new(),
        }
    }
}

/// C expression value category retained after semantic analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExprCategory {
    Value,
    LValue,
    FunctionDesignator,
}

/// An internally inconsistent set of expression facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExprFactsError {
    ModifiableNonLvalue,
    BitFieldNonLvalue,
    AddressableBitField,
    AddressableValue,
}

impl fmt::Display for ExprFactsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ModifiableNonLvalue => "only an lvalue can be modifiable",
            Self::BitFieldNonLvalue => "a bit-field expression must be an lvalue",
            Self::AddressableBitField => "a bit-field expression cannot be addressable",
            Self::AddressableValue => "a value expression cannot be addressable",
        })
    }
}

impl std::error::Error for ExprFactsError {}

/// Checked semantic facts attached to a typed expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExprFacts {
    ty: TypeId,
    category: ExprCategory,
    modifiable_lvalue: bool,
    bit_field: bool,
    addressable: bool,
}

impl ExprFacts {
    pub(crate) const fn with_type(self, ty: TypeId) -> Self {
        Self { ty, ..self }
    }

    pub fn try_new(
        ty: TypeId,
        category: ExprCategory,
        modifiable_lvalue: bool,
        bit_field: bool,
        addressable: bool,
    ) -> Result<Self, ExprFactsError> {
        if modifiable_lvalue && category != ExprCategory::LValue {
            return Err(ExprFactsError::ModifiableNonLvalue);
        }
        if bit_field && category != ExprCategory::LValue {
            return Err(ExprFactsError::BitFieldNonLvalue);
        }
        if bit_field && addressable {
            return Err(ExprFactsError::AddressableBitField);
        }
        if addressable && category == ExprCategory::Value {
            return Err(ExprFactsError::AddressableValue);
        }
        Ok(Self {
            ty,
            category,
            modifiable_lvalue,
            bit_field,
            addressable,
        })
    }

    #[must_use]
    pub fn value(ty: TypeId) -> Self {
        Self {
            ty,
            category: ExprCategory::Value,
            modifiable_lvalue: false,
            bit_field: false,
            addressable: false,
        }
    }

    pub fn lvalue(ty: TypeId, modifiable: bool, addressable: bool) -> Result<Self, ExprFactsError> {
        Self::try_new(ty, ExprCategory::LValue, modifiable, false, addressable)
    }

    #[must_use]
    pub fn bit_field(ty: TypeId, modifiable: bool) -> Self {
        Self {
            ty,
            category: ExprCategory::LValue,
            modifiable_lvalue: modifiable,
            bit_field: true,
            addressable: false,
        }
    }

    #[must_use]
    pub fn function_designator(ty: TypeId) -> Self {
        Self {
            ty,
            category: ExprCategory::FunctionDesignator,
            modifiable_lvalue: false,
            bit_field: false,
            addressable: true,
        }
    }

    #[must_use]
    pub fn ty(self) -> TypeId {
        self.ty
    }

    #[must_use]
    pub fn category(self) -> ExprCategory {
        self.category
    }

    #[must_use]
    pub fn is_modifiable_lvalue(self) -> bool {
        self.modifiable_lvalue
    }

    #[must_use]
    pub fn is_bit_field(self) -> bool {
        self.bit_field
    }

    #[must_use]
    pub fn is_addressable(self) -> bool {
        self.addressable
    }
}

/// Assignment operators separated from non-assignment binary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssignmentOp {
    Simple,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    ShiftLeft,
    ShiftRight,
    BitAnd,
    BitOr,
    BitXor,
}

/// Prefix/postfix scalar updates kept distinct from ordinary unary operators.
///
/// Each operation mutates exactly one storage place. Keeping this semantic
/// operator in C-HIR avoids desugaring `x++` into an expression that mentions
/// or evaluates `x` more than once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateOp {
    PreIncrement,
    PreDecrement,
    PostIncrement,
    PostDecrement,
}

impl AssignmentOp {
    #[must_use]
    pub fn is_compound(self) -> bool {
        self != Self::Simple
    }
}

/// A [`BinaryOp`] that is not one of C's assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotAssignmentOperator(pub BinaryOp);

impl fmt::Display for NotAssignmentOperator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?} is not an assignment operator", self.0)
    }
}

impl std::error::Error for NotAssignmentOperator {}

impl TryFrom<BinaryOp> for AssignmentOp {
    type Error = NotAssignmentOperator;

    fn try_from(operator: BinaryOp) -> Result<Self, Self::Error> {
        match operator {
            BinaryOp::Assign => Ok(Self::Simple),
            BinaryOp::AddAssign => Ok(Self::Add),
            BinaryOp::SubAssign => Ok(Self::Subtract),
            BinaryOp::MulAssign => Ok(Self::Multiply),
            BinaryOp::DivAssign => Ok(Self::Divide),
            BinaryOp::ModAssign => Ok(Self::Remainder),
            BinaryOp::ShlAssign => Ok(Self::ShiftLeft),
            BinaryOp::ShrAssign => Ok(Self::ShiftRight),
            BinaryOp::BitAndAssign => Ok(Self::BitAnd),
            BinaryOp::BitOrAssign => Ok(Self::BitOr),
            BinaryOp::BitXorAssign => Ok(Self::BitXor),
            other => Err(NotAssignmentOperator(other)),
        }
    }
}

impl From<AssignmentOp> for BinaryOp {
    fn from(operator: AssignmentOp) -> Self {
        match operator {
            AssignmentOp::Simple => Self::Assign,
            AssignmentOp::Add => Self::AddAssign,
            AssignmentOp::Subtract => Self::SubAssign,
            AssignmentOp::Multiply => Self::MulAssign,
            AssignmentOp::Divide => Self::DivAssign,
            AssignmentOp::Remainder => Self::ModAssign,
            AssignmentOp::ShiftLeft => Self::ShlAssign,
            AssignmentOp::ShiftRight => Self::ShrAssign,
            AssignmentOp::BitAnd => Self::BitAndAssign,
            AssignmentOp::BitOr => Self::BitOrAssign,
            AssignmentOp::BitXor => Self::BitXorAssign,
        }
    }
}

/// A direct named object location used by the scalar semantic/interpreter path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScalarPlace {
    symbol: SymbolId,
    ty: TypeId,
}

impl ScalarPlace {
    pub fn bind(
        table: &SymbolTable,
        types: &TypeInterner,
        symbol: SymbolId,
    ) -> Result<Self, ScalarPlaceError> {
        let declaration = table
            .symbol(symbol)
            .map_err(|_| ScalarPlaceError::UnknownSymbol(symbol))?;
        let kind = declaration.kind();
        if !matches!(kind, SymbolKind::Object | SymbolKind::Parameter) {
            return Err(ScalarPlaceError::NotScalarObject { symbol, kind });
        }
        let ty = declaration.ty();
        let canonical = types
            .get(ty)
            .ok_or(ScalarPlaceError::UnknownType { symbol, ty })?;
        if !matches!(
            canonical.kind,
            TypeKind::Int(_)
                | TypeKind::Float(_)
                | TypeKind::Complex(_)
                | TypeKind::Pointer(_)
                | TypeKind::Enum(_)
                | TypeKind::GnuInt128(_)
        ) && !matches!(canonical.kind, TypeKind::TargetBuiltin(kind) if kind.polynomial_bits().is_some())
        {
            return Err(ScalarPlaceError::NonScalarType { symbol, ty });
        }
        Ok(Self { symbol, ty })
    }

    #[must_use]
    pub fn symbol(self) -> SymbolId {
        self.symbol
    }

    #[must_use]
    pub fn ty(self) -> TypeId {
        self.ty
    }
}

/// The bounded scalar update plan consumed by the future evaluator.
///
/// This is deliberately a plan, not an interpreter: S3 proves the number of
/// storage effects without introducing Day-7 memory or G3 execution state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdatePlan {
    Assignment {
        place: ScalarPlace,
        op: AssignmentOp,
    },
    Update {
        place: ScalarPlace,
        op: UpdateOp,
    },
}

impl UpdatePlan {
    #[must_use]
    pub const fn assignment(place: ScalarPlace, op: AssignmentOp) -> Self {
        Self::Assignment { place, op }
    }

    #[must_use]
    pub const fn update(place: ScalarPlace, op: UpdateOp) -> Self {
        Self::Update { place, op }
    }

    #[must_use]
    pub const fn place(self) -> ScalarPlace {
        match self {
            Self::Assignment { place, .. } | Self::Update { place, .. } => place,
        }
    }

    #[must_use]
    pub const fn effect(self) -> UpdateEffect {
        let loads = match self {
            Self::Assignment {
                op: AssignmentOp::Simple,
                ..
            } => 0,
            Self::Assignment { .. } | Self::Update { .. } => 1,
        };
        UpdateEffect {
            place_lookups: 1,
            loads,
            stores: 1,
        }
    }
}

/// Exact storage-operation counts for one [`UpdatePlan`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UpdateEffect {
    place_lookups: u8,
    loads: u8,
    stores: u8,
}

impl UpdateEffect {
    #[must_use]
    pub const fn place_lookups(self) -> u8 {
        self.place_lookups
    }

    #[must_use]
    pub const fn loads(self) -> u8 {
        self.loads
    }

    #[must_use]
    pub const fn stores(self) -> u8 {
        self.stores
    }
}

/// Why a symbol cannot identify a direct scalar storage place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarPlaceError {
    UnknownSymbol(SymbolId),
    UnknownType { symbol: SymbolId, ty: TypeId },
    NotScalarObject { symbol: SymbolId, kind: SymbolKind },
    NonScalarType { symbol: SymbolId, ty: TypeId },
}

impl fmt::Display for ScalarPlaceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownSymbol(symbol) => write!(formatter, "unknown symbol ID {}", symbol.raw()),
            Self::UnknownType { symbol, ty } => write!(
                formatter,
                "symbol {} refers to unknown type {}",
                symbol.raw(),
                ty.raw()
            ),
            Self::NotScalarObject { symbol, kind } => write!(
                formatter,
                "symbol {} has kind {kind:?}, not object storage",
                symbol.raw()
            ),
            Self::NonScalarType { symbol, ty } => write!(
                formatter,
                "symbol {} has non-scalar type {}",
                symbol.raw(),
                ty.raw()
            ),
        }
    }
}

impl std::error::Error for ScalarPlaceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_common::{FileId, SourceSpan};
    use evocc_types::{CanonicalType, IntRank, Qualifiers};

    fn scalar_place() -> ScalarPlace {
        let mut types = TypeInterner::new();
        let ty = types.intern(CanonicalType {
            kind: TypeKind::Int(IntRank::Int),
            qualifiers: Qualifiers::default(),
        });
        let mut symbols = SymbolTable::new();
        let symbol = symbols
            .declare(
                symbols.file_scope(),
                "value",
                SymbolData::Object {
                    storage: StorageClass::None,
                },
                ty,
                SourceSpan::new(FileId::new(1), 0, 5, 1, 1),
            )
            .unwrap();
        ScalarPlace::bind(&symbols, &types, symbol).unwrap()
    }

    #[test]
    fn scalar_update_plans_witness_exactly_one_place_and_store() {
        let place = scalar_place();
        let simple = UpdatePlan::assignment(place, AssignmentOp::Simple);
        assert_eq!(simple.place(), place);
        assert_eq!(simple.effect().place_lookups(), 1);
        assert_eq!(simple.effect().loads(), 0);
        assert_eq!(simple.effect().stores(), 1);

        for op in [
            AssignmentOp::Add,
            AssignmentOp::Subtract,
            AssignmentOp::Multiply,
            AssignmentOp::Divide,
            AssignmentOp::Remainder,
            AssignmentOp::ShiftLeft,
            AssignmentOp::ShiftRight,
            AssignmentOp::BitAnd,
            AssignmentOp::BitOr,
            AssignmentOp::BitXor,
        ] {
            let plan = UpdatePlan::assignment(place, op);
            assert_eq!(plan.place(), place);
            assert_eq!(plan.effect().place_lookups(), 1);
            assert_eq!(plan.effect().loads(), 1);
            assert_eq!(plan.effect().stores(), 1);
        }

        for op in [
            UpdateOp::PreIncrement,
            UpdateOp::PreDecrement,
            UpdateOp::PostIncrement,
            UpdateOp::PostDecrement,
        ] {
            let plan = UpdatePlan::update(place, op);
            assert_eq!(plan.place(), place);
            assert_eq!(plan.effect().place_lookups(), 1);
            assert_eq!(plan.effect().loads(), 1);
            assert_eq!(plan.effect().stores(), 1);
        }
    }
}
