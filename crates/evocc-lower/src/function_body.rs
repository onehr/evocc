//! Lowering function body.

use super::*;

impl<'a> FunctionLowerer<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        unit: &'a CHirUnit<'a>,
        target: &'a TargetDataLayout,
        functions: &'a BTreeMap<SymbolId, FunctionInfo>,
        info: &'a FunctionInfo,
        memory_symbols: &'a BTreeSet<SymbolId>,
        size_symbols: &'a BTreeSet<SymbolId>,
        global_registers: &'a BTreeMap<SymbolId, String>,
        allocation_origins: &'a BTreeMap<SymbolId, u64>,
        compound_origins: &BTreeMap<usize, (u64, bool)>,
        dynamic_origins: &BTreeMap<usize, u64>,
        string_origins: &BTreeMap<usize, u64>,
        immutable_globals: &'a BTreeMap<String, GlobalObject>,
    ) -> Self {
        Self {
            unit,
            target,
            functions,
            info,
            memory_symbols,
            global_registers,
            local_registers: BTreeMap::new(),
            allocation_origins,
            compound_origins: compound_origins.clone(),
            dynamic_origins: dynamic_origins.clone(),
            compound_scopes: BTreeMap::new(),
            string_origins: string_origins.clone(),
            immutable_globals,
            blocks: Vec::new(),
            next_value: 0,
            next_block: 0,
            next_temporary: 0,
            next_synthetic_origin: 0,
            next_placeholder: BREAK_TARGET_BASE,
            goto_names: BTreeSet::new(),
            has_computed_goto: false,
            indirect_branches: Vec::new(),
            goto_counts: BTreeMap::new(),
            labels: BTreeMap::new(),
            case_labels: BTreeMap::new(),
            address_labels: BTreeMap::new(),
            pending_gotos: BTreeMap::new(),
            object_declarations: BTreeMap::new(),
            constant_objects: BTreeSet::new(),
            unstable_objects: BTreeSet::new(),
            optimize: false,
            size_symbols,
            active_control: ControlTargets::default(),
            fenv_access: false,
            source_span: info.span,
            source_values: None,
        }
    }

    pub(super) fn lower(
        mut self,
        declaration: &FunctionDecl<'a>,
        optimize: bool,
        locations: &mut Option<SourceLocations>,
    ) -> Result<(Function, BTreeMap<usize, String>), LowerError> {
        self.optimize = optimize;
        self.source_values = locations.as_ref().map(|_| BTreeMap::new());
        let body = declaration
            .body
            .ok_or_else(|| LowerError::MalformedFunction {
                function: declaration.name.spelling.clone(),
                span: declaration.span,
                reason: "attempted to lower a declaration without a body",
            })?;
        let mut references = FunctionReferences::default();
        collect_references_stmt(body, &mut references);
        self.unstable_objects = std::mem::take(&mut references.unstable_objects);
        let stack_protect = self.unit.stack_protector()
            && !declaration
                .attributes
                .iter()
                .any(|a| a.semantic_name() == Some("no_stack_protector"))
            && (references.objects.iter().any(|(symbol, ty)| {
                references.frame_addresses.contains(symbol)
                    || self.type_contains_array(*ty, &mut BTreeSet::new())
            }) || references.addressed_compound
                || references.compound_scopes.values().any(|sites| {
                    sites
                        .values()
                        .any(|(ty, _)| self.type_contains_array(*ty, &mut BTreeSet::new()))
                })
                || self
                    .info
                    .parameter_symbols
                    .iter()
                    .flatten()
                    .any(|symbol| references.frame_addresses.contains(symbol))
                || references.direct.iter().any(|symbol| {
                    self.unit.symbols().is_builtin_function(*symbol)
                        && self
                            .unit
                            .symbols()
                            .symbol(*symbol)
                            .is_ok_and(|entry| entry.spelling() == "alloca")
                }));
        self.has_computed_goto = references.has_computed_goto;
        self.goto_names = std::mem::take(&mut references.goto_names);
        self.goto_counts = std::mem::take(&mut references.goto_counts);
        self.local_registers = std::mem::take(&mut references.local_registers);
        self.compound_scopes = std::mem::take(&mut references.compound_scopes);
        for name in references.labels.values().collect::<BTreeSet<_>>() {
            let id = self.fresh_placeholder();
            self.address_labels.insert(name.clone(), id);
            self.goto_names.insert(name.clone());
        }
        let mut params = Vec::new();
        let mut env = Environment::new();
        let mut entry_args = Vec::new();
        for (index, (symbol, value_type)) in self
            .info
            .parameter_symbols
            .iter()
            .copied()
            .zip(self.info.parameter_types.iter())
            .enumerate()
        {
            let ty = value_type.ir.clone();
            params.push(FunctionParam {
                name: format!("arg{index}"),
                ty: ty.clone(),
            });
            let value = self.fresh_value(ty);
            let key = match symbol {
                Some(symbol) if self.is_aggregate_type(value_type.source) => {
                    EnvKey::Address(symbol)
                }
                Some(symbol) => EnvKey::Local(symbol),
                None => {
                    let key = EnvKey::Temporary(self.next_temporary);
                    self.next_temporary += 1;
                    key
                }
            };
            entry_args.push((
                key,
                BlockArg {
                    name: format!("param{index}"),
                    value: value.clone(),
                },
            ));
            if symbol.is_some() {
                env.insert(
                    key,
                    BoundValue {
                        value,
                        integer: (!self.is_aggregate_type(value_type.source))
                            .then_some(value_type.integer),
                    },
                );
            }
        }
        if self
            .info
            .return_type
            .as_ref()
            .is_some_and(|returned| self.is_aggregate_type(returned.source))
        {
            let index = params.len();
            let ty = EvoIRType::Ptr(AddressSpace::Generic);
            params.push(FunctionParam {
                name: format!("arg{index}"),
                ty: ty.clone(),
            });
            let value = self.fresh_value(ty);
            entry_args.push((
                EnvKey::ReturnDestination,
                BlockArg {
                    name: "return_destination".into(),
                    value: value.clone(),
                },
            ));
            env.insert(
                EnvKey::ReturnDestination,
                BoundValue {
                    value,
                    integer: None,
                },
            );
        }
        if self.info.participant {
            let mem = EvoIRType::Mem(Region::All);
            let world = EvoIRType::World;
            let mem_index = params.len();
            params.push(FunctionParam {
                name: format!("arg{mem_index}"),
                ty: mem.clone(),
            });
            params.push(FunctionParam {
                name: format!("arg{}", mem_index + 1),
                ty: world.clone(),
            });
            let mem_value = self.fresh_value(mem);
            let world_value = self.fresh_value(world);
            entry_args.push((
                EnvKey::Memory,
                BlockArg {
                    name: "memory".into(),
                    value: mem_value.clone(),
                },
            ));
            entry_args.push((
                EnvKey::World,
                BlockArg {
                    name: "world".into(),
                    value: world_value.clone(),
                },
            ));
            env.insert(
                EnvKey::Memory,
                BoundValue {
                    value: mem_value,
                    integer: None,
                },
            );
            env.insert(
                EnvKey::World,
                BoundValue {
                    value: world_value,
                    integer: None,
                },
            );
        }
        self.blocks.push(DraftBlock {
            id: 0,
            args: entry_args,
            insts: Vec::new(),
            terminator: None,
            role: BlockRole::Ordinary,
        });
        self.next_block = 1;

        let locals = self
            .info
            .parameter_symbols
            .iter()
            .copied()
            .flatten()
            .collect();
        let mut entry = State {
            block: 0,
            env,
            locals,
            cleanups: Vec::new(),
        };
        // The call interface supplies scalar values, but an addressed parameter
        // is an automatic object owned by this invocation, just like a local.
        for symbol in self.info.parameter_symbols.iter().copied().flatten() {
            let declared = self
                .unit
                .symbols()
                .symbol(symbol)
                .expect("parameter identity was validated")
                .ty();
            let value_type =
                function_value_type(self.unit.types(), self.target, declared, declaration.span)?;
            if let Some(incoming) = entry.env.get(&EnvKey::Local(symbol)).cloned() {
                if matches!(incoming.value.ty, EvoIRType::Bits(_)) {
                    let converted = self.cast(
                        ExprValue {
                            value: incoming.value,
                            integer: incoming.integer.expect("integer parameter"),
                        },
                        value_type.integer,
                        &mut entry,
                    )?;
                    entry.env.insert(
                        EnvKey::Local(symbol),
                        BoundValue {
                            value: converted.value,
                            integer: Some(converted.integer),
                        },
                    );
                }
            }
            if !self.memory_symbols.contains(&symbol) || self.is_aggregate_type(value_type.source) {
                continue;
            }
            let value = entry.env.remove(&EnvKey::Local(symbol)).ok_or(
                LowerError::InvalidGeneratedSsa {
                    message: format!("parameter {} has no incoming scalar value", symbol.raw()),
                },
            )?;
            let origin =
                *self
                    .allocation_origins
                    .get(&symbol)
                    .ok_or(LowerError::InvalidGeneratedSsa {
                        message: format!("parameter {} has no allocation origin", symbol.raw()),
                    })?;
            let address =
                self.emit_alloca(value_type.source, declaration.span, origin, &mut entry)?;
            self.emit_memory_store(
                address.clone(),
                ExprValue {
                    value: value.value,
                    integer: value_type.integer,
                },
                value_type.source,
                self.unit
                    .types()
                    .get(declared)
                    .is_some_and(|ty| ty.qualifiers.is_volatile),
                declaration.span,
                &mut entry,
            )?;
            entry.env.insert(
                EnvKey::Address(symbol),
                BoundValue {
                    value: address,
                    integer: None,
                },
            );
        }
        let flow = self.lower_stmt(body, entry, &BTreeSet::new(), &ControlTargets::default())?;
        if !self.pending_gotos.is_empty() {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "goto targets a label that was not lowered",
            });
        }
        for state in &flow.normal {
            if self.info.return_type.is_none() || self.info.name == "main" {
                let mut returned = Vec::new();
                if let Some(value_type) = self.info.return_type.as_ref() {
                    if matches!(value_type.ir, EvoIRType::Ptr(_)) {
                        let value = self.fresh_value(value_type.ir.clone());
                        self.emit(state, Inst::PtrNull(value.clone()))?;
                        returned.push(value);
                    } else if value_type.ir == bits(value_type.integer) {
                        returned.push(self.constant(state, value_type.integer, 0)?.value);
                    } else {
                        returned.push(self.constant(state, CIntegerType::INT, 0)?.value);
                    }
                }
                if self.info.participant {
                    returned.push(self.token(state, EnvKey::Memory)?.value);
                    returned.push(self.token(state, EnvKey::World)?.value);
                }
                self.terminate(state, Inst::Ret(returned))?;
            } else {
                let value_type = self.info.return_type.as_ref().unwrap();
                let mut returned = Vec::new();
                // Aggregates return through caller-owned uninitialized storage;
                // falling off does not initialize it or add a scalar result.
                if !self.is_aggregate_type(value_type.source) {
                    let value = self.fresh_value(value_type.ir.clone());
                    self.emit(state, Inst::Uninitialized(value.clone()))?;
                    returned.push(value);
                }
                if self.info.participant {
                    returned.push(self.token(state, EnvKey::Memory)?.value);
                    returned.push(self.token(state, EnvKey::World)?.value);
                }
                self.terminate(state, Inst::Ret(returned))?;
            }
        }
        self.finish_indirect_branches()?;
        let mut address_remap = BTreeMap::new();
        for (name, placeholder) in &self.address_labels {
            let target = self
                .labels
                .get(name)
                .ok_or(LowerError::InvalidControlFlow {
                    span: self.info.span,
                    reason: "addressed label was not lowered",
                })?;
            address_remap.insert(*placeholder, target.id);
        }
        for block in &mut self.blocks {
            for instruction in &mut block.insts {
                if let Inst::BlockAddr { block, .. } = instruction {
                    *block = address_remap[block];
                }
            }
        }
        if optimize {
            constants::prune_constant_edges(
                &mut self.blocks,
                &self
                    .address_labels
                    .keys()
                    .filter_map(|name| self.labels.get(name).map(|label| label.id))
                    .collect(),
                self.immutable_globals,
                self.target.byte_order(),
            );
        }
        self.prune_unreachable_blocks()?;
        if self.blocks.iter().any(|block| block.terminator.is_none()) {
            return Err(LowerError::InvalidControlFlow {
                span: self.info.span,
                reason: "lowered block has no terminator",
            });
        }
        self.eliminate_switch_forwarders()?;
        if optimize {
            cfg::join_linear_blocks(
                &mut self.blocks,
                &self.labels.values().map(|label| label.id).collect(),
            )?;
        }
        let label_addresses = references
            .labels
            .into_iter()
            .map(|(expression, name)| {
                let id = self.labels[&name].id;
                let position = self.blocks.iter().position(|block| block.id == id).ok_or(
                    LowerError::InvalidControlFlow {
                        span: self.info.span,
                        reason: "addressed block was removed",
                    },
                )?;
                Ok((expression, format!("{}^b{position}", self.info.name)))
            })
            .collect::<Result<BTreeMap<_, _>, LowerError>>()?;
        let param_marks = record_signature_marks(
            self.unit.types(),
            self.target,
            &self
                .info
                .parameter_types
                .iter()
                .map(|value| value.source)
                .collect::<Vec<_>>(),
            self.info.return_type.as_ref().map(|value| value.source),
            params.len(),
            self.info.span,
        )?;
        let source_span = self.info.span;
        let source_values = self.source_values.take();
        let mut function = Function {
            section: self.info.section.clone(),
            patchable_entry: self.info.patchable_entry,
            stack_protect,
            alignment: self
                .unit
                .symbols()
                .symbol(declaration.symbol)
                .map_err(|_| LowerError::UnknownSymbol {
                    symbol: declaration.symbol,
                    span: declaration.span,
                })?
                .declared_alignment()
                .max(self.unit.minimum_function_alignment()),
            name: self.info.name.clone(),
            variadic: self.info.variadic,
            params,
            ret_tys: self.return_types(),
            effects: Default::default(),
            linkage: evocc_evoir::Linkage::Strong,
            visibility: if declaration.has_internal_linkage(self.unit.symbols()) {
                evocc_evoir::GlobalVisibility::Internal
            } else {
                evocc_evoir::GlobalVisibility::External
            },
            blocks: self.finish_blocks()?,
            param_marks,
        };
        allocation::share_allocation_sites(&mut function)?;
        if optimize {
            reuse::reuse_pure_block_values(&mut function);
            private_memory::promote(&mut function);
        }
        let value_map = renumber_function(&mut function)?;
        if let (Some(locations), Some(values)) = (locations, source_values) {
            let values = values
                .into_iter()
                .filter_map(|(id, span)| value_map.get(&id).map(|new| (*new, span)))
                .collect();
            locations
                .functions
                .insert(function.name.clone(), (source_span, values));
        }
        Ok((function, label_addresses))
    }

    pub(super) fn type_contains_array(&self, ty: TypeId, visiting: &mut BTreeSet<TypeId>) -> bool {
        if !visiting.insert(ty) {
            return false;
        }
        match self.unit.types().get(ty).map(|ty| &ty.kind) {
            Some(TypeKind::Array { .. }) => true,
            Some(TypeKind::Struct(tag) | TypeKind::Union(tag)) => self
                .unit
                .types()
                .tag(*tag)
                .is_some_and(|tag| match &tag.state {
                    TagState::Complete(
                        TagDefinition::Struct(fields) | TagDefinition::Union(fields),
                    ) => fields
                        .iter()
                        .any(|field| self.type_contains_array(field.ty, visiting)),
                    _ => false,
                }),
            _ => false,
        }
    }

    pub(super) fn return_types(&self) -> Vec<EvoIRType> {
        let mut returns = self
            .info
            .return_type
            .as_ref()
            .filter(|value_type| !self.is_aggregate_type(value_type.source))
            .map(|value_type| vec![value_type.ir.clone()])
            .unwrap_or_default();
        if self.info.participant {
            returns.push(EvoIRType::Mem(Region::All));
            returns.push(EvoIRType::World);
        }
        returns
    }
}
