//! Parser symbols.

use super::*;

impl<'src, 'arena> Parser<'src, 'arena> {
    pub(super) fn symbol_in_current_scope(&self, spelling: &str) -> Option<SymbolId> {
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

    pub(super) fn install_temporary_parameter_shadows(&mut self) {
        let spellings = self
            .temporary_parameter_shadows
            .iter()
            .map(|shadow| shadow.spelling.clone())
            .collect::<Vec<_>>();
        for spelling in spellings {
            let _ = self.resolve_expression_symbol(&spelling);
        }
    }

    pub(super) fn resolve_expression_symbol(&mut self, spelling: &str) -> Option<SymbolId> {
        let declared = self
            .symbols
            .resolve(self.current_scope, spelling)
            .expect("the parser current scope is always live");
        if self.temporary_parameter_shadows.is_empty() {
            return declared;
        }
        let declared_scope = declared.map(|symbol| {
            self.symbols
                .symbol(symbol)
                .expect("resolved symbol belongs to the same table")
                .scope()
        });
        let mut scope = Some(self.current_scope);
        while let Some(current) = scope {
            if declared_scope == Some(current) {
                return declared;
            }
            if let Some(shadow) = self
                .temporary_parameter_shadows
                .iter()
                .rev()
                .find(|shadow| shadow.scope == current && shadow.spelling == spelling)
                .cloned()
            {
                // A prior parameter already has scope, even though the full
                // function declarator is not bound yet. Materialize it only
                // when referenced, and let bind_parameters reuse its identity.
                let symbol = self
                    .symbols
                    .declare(
                        current,
                        spelling,
                        SymbolData::Parameter {
                            storage: shadow.storage,
                        },
                        shadow.ty,
                        shadow.span,
                    )
                    .expect("an unresolved parameter has no binding in its scope");
                self.add_entity_attributes(symbol, &shadow.attributes);
                return Some(symbol);
            }
            scope = self
                .symbols
                .scope(current)
                .expect("the parser current scope ancestry is always live")
                .parent();
        }
        declared
    }

    pub(super) fn linked_symbol_in_parent_scopes(&self, spelling: &str) -> Option<SymbolId> {
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

    pub(super) fn begin_declaration_transaction(&mut self) -> DeclarationTransaction {
        self.declaration_transaction_depth += 1;
        DeclarationTransaction {
            unprototyped_calls: self.unprototyped_calls.len(),
            old_style_definitions: self.old_style_definitions.len(),
            prototype_scopes: self.prototype_scopes.len(),
            standard_allocation_calls: self.standard_allocation_calls.len(),
            entity_undo: self.entity_undo.len(),
            external_entity_undo: self.external_entity_undo.len(),
            inherited_function_parameter_undo: self.inherited_function_parameter_undo.len(),
            entity_attribute_undo: self.entity_attribute_undo.len(),
            member_attribute_undo: self.member_attribute_undo.len(),
            initialized_const_undo: self.initialized_const_undo.len(),
            initialized_string_undo: self.initialized_string_undo.len(),
            string_constant_uses: self.string_constant_uses.len(),
            function_uses: self.function_uses.len(),
            function_definitions: self.function_definitions.len(),
            side_declarations: self.side_declarations.len(),
            builtin_declarations: self.builtin_declarations.len(),
        }
    }

    pub(super) fn commit_declaration_transaction(&mut self, transaction: &DeclarationTransaction) {
        debug_assert!(self.declaration_transaction_depth > 0);
        debug_assert!(self.entity_undo.len() >= transaction.entity_undo);
        debug_assert!(self.external_entity_undo.len() >= transaction.external_entity_undo);
        debug_assert!(
            self.inherited_function_parameter_undo.len()
                >= transaction.inherited_function_parameter_undo
        );
        debug_assert!(self.entity_attribute_undo.len() >= transaction.entity_attribute_undo);
        debug_assert!(self.member_attribute_undo.len() >= transaction.member_attribute_undo);
        self.declaration_transaction_depth -= 1;
        if self.declaration_transaction_depth == 0 {
            self.entity_undo.clear();
            self.external_entity_undo.clear();
            self.inherited_function_parameter_undo.clear();
            self.entity_attribute_undo.clear();
            self.member_attribute_undo.clear();
            self.initialized_const_undo.clear();
            self.initialized_string_undo.clear();
        }
    }

    pub(super) fn rollback_declaration_transaction(
        &mut self,
        transaction: &DeclarationTransaction,
    ) {
        self.string_constant_uses
            .truncate(transaction.string_constant_uses);
        self.function_uses.truncate(transaction.function_uses);
        self.function_definitions
            .truncate(transaction.function_definitions);
        while self.initialized_string_undo.len() > transaction.initialized_string_undo {
            let (symbol, previous) = self
                .initialized_string_undo
                .pop()
                .expect("bounded undo marker");
            if let Some(previous) = previous {
                self.initialized_strings.insert(symbol, previous);
            } else {
                self.initialized_strings.remove(&symbol);
            }
        }
        while self.initialized_const_undo.len() > transaction.initialized_const_undo {
            let (symbol, previous) = self
                .initialized_const_undo
                .pop()
                .expect("undo marker is bounded");
            if let Some(previous) = previous {
                self.initialized_const_integers.insert(symbol, previous);
            } else {
                self.initialized_const_integers.remove(&symbol);
            }
        }
        self.unprototyped_calls
            .truncate(transaction.unprototyped_calls);
        self.old_style_definitions
            .truncate(transaction.old_style_definitions);
        self.prototype_scopes.truncate(transaction.prototype_scopes);
        self.standard_allocation_calls
            .truncate(transaction.standard_allocation_calls);
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
        while self.entity_attribute_undo.len() > transaction.entity_attribute_undo {
            let (symbol, previous) = self
                .entity_attribute_undo
                .pop()
                .expect("undo marker is bounded");
            match previous {
                Some(previous) => {
                    self.entity_attributes.insert(symbol, previous);
                }
                None => {
                    self.entity_attributes.remove(&symbol);
                }
            }
        }
        while self.member_attribute_undo.len() > transaction.member_attribute_undo {
            let (field, previous) = self
                .member_attribute_undo
                .pop()
                .expect("undo marker is bounded");
            match previous {
                Some(previous) => {
                    self.member_attributes.insert(field, previous);
                }
                None => {
                    self.member_attributes.remove(&field);
                }
            }
        }
        self.side_declarations
            .truncate(transaction.side_declarations);
        self.builtin_declarations
            .truncate(transaction.builtin_declarations);
        self.declaration_transaction_depth -= 1;
        if self.declaration_transaction_depth == 0 {
            self.entity_undo.clear();
            self.external_entity_undo.clear();
            self.inherited_function_parameter_undo.clear();
            self.entity_attribute_undo.clear();
            self.member_attribute_undo.clear();
            self.initialized_const_undo.clear();
            self.initialized_string_undo.clear();
        }
    }

    pub(super) fn set_entity_state(&mut self, symbol: SymbolId, state: EntityState) {
        let previous = self.entities.insert(symbol, state);
        if self.declaration_transaction_depth > 0 {
            self.entity_undo.push((symbol, previous));
        }
    }

    pub(super) fn set_external_entity(&mut self, spelling: String, symbol: SymbolId) {
        let previous = self.external_entities.insert(spelling.clone(), symbol);
        if self.declaration_transaction_depth > 0 {
            self.external_entity_undo.push((spelling, previous));
        }
    }

    pub(super) fn set_inherited_function_parameters(
        &mut self,
        symbol: SymbolId,
        parameters: ParameterSyntax,
    ) {
        let previous = self
            .inherited_function_parameters
            .insert(symbol, parameters);
        if self.declaration_transaction_depth > 0 {
            self.inherited_function_parameter_undo
                .push((symbol, previous));
        }
    }

    /// Merge the availability, `unavailable` and `weak_import` attributes of
    /// one declaration into the state of its entity (EvoIR 10.1.9). A
    /// malformed `availability` attribute is `E2903`. Without a deployment
    /// target nothing is merged.
    pub(super) fn merged_availability(
        &mut self,
        mut state: DeclarationAvailability,
        attributes: &[Attribute],
    ) -> DeclarationAvailability {
        if self.availability_target.is_none() {
            return state;
        }
        for attribute in attributes {
            match attribute.semantic_name() {
                Some("availability") => {
                    let arguments = attribute
                        .arguments
                        .iter()
                        .map(|argument| argument.text.as_str())
                        .collect::<Vec<_>>();
                    match Availability::parse(&arguments) {
                        Ok(clause) => state.merged.merge(clause),
                        Err(index) => {
                            let span = attribute
                                .arguments
                                .get(index)
                                .map_or(attribute.span, |argument| argument.span);
                            self.report("E2903", "malformed-attribute", span);
                        }
                    }
                }
                Some("unavailable") if state.unavailable.is_none() => {
                    let message = match attribute.arguments.as_slice() {
                        [] => String::new(),
                        [argument] => match argument
                            .text
                            .strip_prefix('"')
                            .and_then(|text| text.strip_suffix('"'))
                        {
                            Some(message) => message.to_owned(),
                            None => {
                                self.report("E2903", "malformed-attribute", argument.span);
                                String::new()
                            }
                        },
                        [_, extra, ..] => {
                            self.report("E2903", "malformed-attribute", extra.span);
                            String::new()
                        }
                    };
                    state.unavailable = Some(message);
                }
                Some("weak_import") => state.weak_import = true,
                _ => {}
            }
        }
        state
    }

    /// Why a use of `symbol` is refused here: the symbol is unavailable for
    /// the deployment target.
    pub(super) fn symbol_unavailable(&self, symbol: SymbolId) -> Option<String> {
        let deployment = self.availability_target?;
        self.symbol_availability
            .get(&symbol)?
            .unavailable(deployment)
    }

    pub(super) fn add_entity_attributes(&mut self, symbol: SymbolId, attributes: &[Attribute]) {
        if self.availability_target.is_some() {
            let previous = self
                .symbol_availability
                .get(&symbol)
                .cloned()
                .unwrap_or_default();
            let merged = self.merged_availability(previous, attributes);
            self.symbol_availability.insert(symbol, merged);
        }
        for attribute in attributes {
            if attribute.semantic_name() == Some("patchable_function_entry")
                && (!self
                    .symbols
                    .symbol(symbol)
                    .is_ok_and(|entry| entry.kind() == SymbolKind::Function)
                    || evocc_chir::function_definitions::patchable_entry(attribute).is_err())
            {
                self.report("E2903", "malformed-attribute", attribute.span);
            }
        }
        if self.symbols.symbol(symbol).is_ok_and(|entry| {
            matches!(
                entry.kind(),
                SymbolKind::Object | SymbolKind::Parameter | SymbolKind::Function
            )
        }) {
            for attribute in attributes {
                if let Some(AppliedAttribute::Aligned(bytes)) = attribute.applied {
                    self.symbols
                        .raise_declared_alignment(symbol, bytes)
                        .expect("declared entity exists");
                }
            }
        }
        if attributes.is_empty() {
            return;
        }
        let previous = self.entity_attributes.get(&symbol).cloned();
        let names = self.entity_attributes.entry(symbol).or_default();
        names.extend(
            attributes
                .iter()
                .filter_map(|attribute| attribute.query_name().map(str::to_owned)),
        );
        if self.declaration_transaction_depth > 0 {
            self.entity_attribute_undo.push((symbol, previous));
        }
    }

    pub(super) fn set_member_attributes(
        &mut self,
        tag: TagId,
        field_index: usize,
        attributes: &[Attribute],
    ) {
        let key = (tag, field_index);
        if self.availability_target.is_some() {
            let merged = self.merged_availability(DeclarationAvailability::default(), attributes);
            self.member_availability.insert(key, merged);
        }
        let previous = self.member_attributes.get(&key).cloned();
        self.member_attributes.insert(
            key,
            attributes
                .iter()
                .filter_map(|attribute| attribute.query_name().map(str::to_owned))
                .collect(),
        );
        if self.declaration_transaction_depth > 0 {
            self.member_attribute_undo.push((key, previous));
        }
    }

    pub(super) fn declare_symbol(
        &mut self,
        name: &SpannedName,
        data: SymbolData,
        ty: TypeId,
    ) -> SymbolId {
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
                        thread_local: data
                            .storage_class()
                            .is_some_and(StorageClass::is_thread_local),
                        has_definition: false,
                        gnu_inline_body: false,
                        assembly_named: false,
                        old_style_definition: false,
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

    pub(super) fn report_redefinition(&mut self, name: &SpannedName) {
        let previous = self
            .symbols
            .resolve(self.current_scope, &name.spelling)
            .ok()
            .flatten()
            .and_then(|id| self.symbols.symbol(id).ok())
            .map(|symbol| symbol.span())
            .filter(|span| *span != name.span);
        self.report(
            "E2002",
            format!("incompatible-redeclaration: {}", name.spelling),
            name.span,
        );
        self.diagnostics.last_mut().unwrap().previous_declaration = previous;
    }

    pub(super) fn declaration_linkage(&self, data: SymbolData) -> EntityLinkage {
        let file_scope = self.current_scope == self.symbols.file_scope();
        match data {
            SymbolData::Function {
                storage: StorageClass::Static,
            } if file_scope => EntityLinkage::Internal,
            SymbolData::Function { .. } => EntityLinkage::External,
            SymbolData::Object {
                storage: StorageClass::Static | StorageClass::StaticThreadLocal,
            } if file_scope => EntityLinkage::Internal,
            SymbolData::Object {
                storage: StorageClass::Extern | StorageClass::ExternThreadLocal,
            } => EntityLinkage::External,
            SymbolData::Object { .. } if file_scope => EntityLinkage::External,
            SymbolData::Object { .. }
            | SymbolData::Parameter { .. }
            | SymbolData::Typedef
            | SymbolData::Enumerator { .. } => EntityLinkage::None,
        }
    }

    pub(super) fn merge_redeclaration(
        &mut self,
        symbol: SymbolId,
        data: SymbolData,
        ty: TypeId,
    ) -> bool {
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
        let new_thread_local = data
            .storage_class()
            .is_some_and(StorageClass::is_thread_local);
        if state.thread_local != new_thread_local {
            return false;
        }

        let new_linkage = self.declaration_linkage(data);
        let storage = data.storage_class().unwrap_or(StorageClass::None).base();
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

    /// Whether a function declaration puts a prototype in scope: its
    /// declarator, or the type that it inherits, has a parameter type list
    /// (C11 6.2.1p4, 6.7.6.3p14). `__typeof__` of an old-style definition
    /// does not, as in GCC 14.
    pub(super) fn declares_prototype(
        &self,
        declarator: &ParsedDeclarator<'arena>,
        specifier: &ParsedSpecifier<'arena>,
        declared: TypeId,
    ) -> bool {
        let written = declarator.derived.iter().find_map(|layer| match layer {
            DerivedDeclarator::Function { parameters, .. } => Some(parameters),
            _ => None,
        });
        match written.or(specifier.inherited_function_parameters.as_ref()) {
            Some(parameters) => matches!(parameters, ParameterSyntax::Prototype { .. }),
            None => matches!(
                self.types.get(declared).map(|ty| &ty.kind),
                Some(TypeKind::Function {
                    parameters: FunctionParameters::Prototype { .. },
                    ..
                })
            ),
        }
    }

    pub(super) fn record_prototype_scope(&mut self, symbol: SymbolId) {
        self.prototype_scopes.push((symbol, self.current_scope));
    }

    /// Whether a declaration of the function with a parameter type list is
    /// visible in the current scope. Block-scope declarations share the
    /// entity of the file-scope function, so its type cannot tell.
    pub(super) fn prototype_in_scope(&self, symbol: SymbolId) -> bool {
        self.prototype_scopes.iter().any(|(declared, scope)| {
            let mut current = Some(self.current_scope);
            *declared == symbol
                && loop {
                    match current {
                        Some(id) if id == *scope => break true,
                        Some(id) => {
                            current = self.symbols.scope(id).ok().and_then(|entry| entry.parent());
                        }
                        None => break false,
                    }
                }
        })
    }

    /// The promoted parameter types of an old-style definition, or `None`
    /// for a definition with a parameter type list (C11 6.9.1p7). Before
    /// C23, an empty list in a definition is the identifier-list form with
    /// no identifiers (C11 6.7.6.3p14), as in GCC 14 up to `-std=gnu17`.
    /// C23 makes it a prototype; EvoCC accepts no C23 mode, so the empty
    /// list is old-style in every accepted mode.
    pub(super) fn old_style_parameter_types(
        &mut self,
        parameters: &ParameterSyntax,
    ) -> Option<Vec<TypeId>> {
        match parameters {
            ParameterSyntax::Prototype { .. } => None,
            ParameterSyntax::Unspecified => Some(Vec::new()),
            ParameterSyntax::IdentifierList { parameters } => Some(
                parameters
                    .iter()
                    .map(|parameter| {
                        let kind =
                            evocc_chir::default_promoted_parameter_kind(&self.types, parameter.ty)
                                .expect("parsed parameter type exists");
                        self.intern(kind, Qualifiers::default())
                    })
                    .collect(),
            ),
        }
    }

    /// A call of a function with an old-style definition and no prototype
    /// in scope is a call without a prototype (C11 6.5.2.2p6), also after
    /// the definition gave the function its promoted interface. The
    /// designator gets the type without a prototype: the arguments then take
    /// the default argument promotions, and no parameter type checks them.
    pub(super) fn old_style_call_designator(
        &mut self,
        callee: &'arena CHirExpr<'arena>,
    ) -> Option<&'arena CHirExpr<'arena>> {
        let CHirExpr::Ident {
            symbol,
            spelling,
            facts,
            span,
        } = callee
        else {
            return None;
        };
        if facts.category() != ExprCategory::FunctionDesignator
            || !self
                .entities
                .get(symbol)
                .is_some_and(|state| state.old_style_definition)
            || self.prototype_in_scope(*symbol)
        {
            return None;
        }
        let ty = self.types.get(facts.ty())?;
        let TypeKind::Function {
            return_type,
            parameters: FunctionParameters::Prototype { .. },
        } = &ty.kind
        else {
            return None;
        };
        let (return_type, qualifiers) = (*return_type, ty.qualifiers);
        let unprototyped = self.intern(
            TypeKind::Function {
                return_type,
                parameters: FunctionParameters::Unspecified,
            },
            qualifiers,
        );
        Some(self.alloc_node(CHirExpr::Ident {
            symbol: *symbol,
            spelling: spelling.clone(),
            facts: ExprFacts::function_designator(unprototyped),
            span: *span,
        }))
    }

    pub(super) fn mark_definition(&mut self, symbol: SymbolId, name: &SpannedName) {
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

    pub(super) fn record_assembly_name(&mut self, symbol: SymbolId, name: &Option<String>) {
        if name.is_some() {
            if let Some(mut state) = self.entities.get(&symbol).copied() {
                state.assembly_named = true;
                self.set_entity_state(symbol, state);
            }
        }
    }

    pub(super) fn composite_type(
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
            (TypeKind::TargetBuiltin(left), TypeKind::TargetBuiltin(right)) if left == right => {
                TypeKind::TargetBuiltin(right)
            }
            (TypeKind::GnuInt128(left), TypeKind::GnuInt128(right)) if left == right => {
                TypeKind::GnuInt128(right)
            }
            (TypeKind::Int(left), TypeKind::Int(right)) if left == right => TypeKind::Int(right),
            (TypeKind::Enum(tag), TypeKind::Int(rank))
            | (TypeKind::Int(rank), TypeKind::Enum(tag))
                if self.types.enum_representation(tag) == Some(rank) =>
            {
                TypeKind::Int(rank)
            }
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
                let bound = left_bound.composite(&right_bound)?;
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

    pub(super) fn composite_function_parameters(
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

    pub(super) fn prototype_survives_default_promotions(
        &self,
        parameters: &FunctionParameters,
    ) -> bool {
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

    pub(super) fn create_scope(&mut self, kind: ScopeKind, span: Option<SourceSpan>) -> ScopeId {
        self.symbols
            .create_scope(self.current_scope, kind, span)
            .expect("the parser current scope is always live")
    }

    pub(super) fn tag_in_current_scope(&self, spelling: &str) -> Option<TagId> {
        self.symbols
            .tag_in_scope(self.current_scope, spelling)
            .expect("the parser current scope is always live")
    }

    pub(super) fn facts_for_symbol(&self, symbol: SymbolId) -> ExprFacts {
        let symbol = self
            .symbols
            .symbol(symbol)
            .expect("resolved symbol belongs to the same table");
        match symbol.kind() {
            SymbolKind::Function => ExprFacts::function_designator(symbol.ty()),
            SymbolKind::Enumerator => ExprFacts::value(symbol.ty()),
            SymbolKind::Object | SymbolKind::Parameter => {
                let modifiable = is_modifiable_object_type(&self.types, symbol.ty());
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
}
