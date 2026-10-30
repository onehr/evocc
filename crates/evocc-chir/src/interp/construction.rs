//! C-HIR interpreter construction.

use super::*;

impl<'unit, 'arena> Interpreter<'unit, 'arena> {
    pub(super) fn new(
        unit: &'unit CHirUnit<'arena>,
        target: &TargetDataLayout,
        limits: InterpreterLimits,
        memory_mode: bool,
    ) -> Result<Self, InterpreterError> {
        let memory_limits = MemoryLimits {
            max_objects: MAX_MEMORY_OBJECTS,
            max_object_bytes: MAX_MEMORY_OBJECT_BYTES,
            max_total_bytes: MAX_MEMORY_TOTAL_BYTES,
        };
        let alignment = unit
            .symbols()
            .symbols_in_source_order()
            .iter()
            .filter(|symbol| symbol.kind() == SymbolKind::Function)
            .map(|symbol| symbol.declared_alignment())
            .max()
            .unwrap_or(4)
            .max(unit.minimum_function_alignment());
        let config = MemoryConfig::for_target(target, memory_limits)
            .and_then(|config| config.with_function_alignment(alignment))
            .map_err(|error| InterpreterError::MemorySetup { error })?;
        let memory =
            MemoryState::new(config).map_err(|error| InterpreterError::MemorySetup { error })?;
        let world = WorldState::new(WorldLimits {
            max_volatile_events: MAX_VOLATILE_EVENTS,
        });
        Ok(Self {
            program: false,
            termination: None,
            linkage: unit.symbol_linkage().map_err(InterpreterError::Linkage)?,
            argument_lists: evocc_memory::ArgumentLists::default(),
            unit,
            target: *target,
            slots: HashMap::new(),
            manifest: None,
            static_bindings: HashMap::new(),
            static_compound_bindings: HashMap::new(),
            initializer_values: HashMap::new(),
            string_bindings: HashMap::new(),
            symbol_order: OnceCell::new(),
            memory_frame: None,
            memory_mode,
            memory,
            world,
            fuel_limit: limits.fuel(),
            consumed: 0,
            fuel_anchor: None,
            nested_mutation_depth: 0,
            static_integer_evaluation: false,
            fenv_access: false,
            integer_expression_types: HashMap::new(),
            temporary_origin_site: 0,
        })
    }

    /// Derive the canonical allocation-site manifest for the whole unit.
    ///
    /// This is whole-unit work, so it runs only after the requested function has
    /// been validated; otherwise a defect anywhere in the unit would mask the
    /// diagnostic that actually describes the function under execution.
    pub(super) fn derive_allocation_manifest(&mut self) -> Result<(), InterpreterError> {
        let manifest = AllocationSiteManifest::for_target(self.unit, &self.target)
            .map_err(|error| InterpreterError::InvalidAllocationManifest { error })?;
        self.temporary_origin_site = u64::try_from(manifest.sites().len())
            .map_err(|_| InterpreterError::MissingAllocationManifest)?;
        self.manifest = Some(manifest);
        Ok(())
    }
}
