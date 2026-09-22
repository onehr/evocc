use super::*;

/// Resolve storage duration once, then thread lazy instance acquisition through
/// the same effect SSA as other memory operations. No access can use global_addr
/// for a template, including addresses synthesized for function-static objects.
pub(super) fn resolve(
    unit: &CHirUnit<'_>,
    origins: &BTreeMap<SymbolId, u64>,
    module: &mut Module,
) -> Result<(), LowerError> {
    let linkage = unit.symbol_linkage().map_err(LowerError::Linkage)?;
    let mut symbols = BTreeMap::new();
    evocc_chir::inline::for_each_declaration(unit, |decl| {
        let CHirDecl::Object(object) = decl else {
            return;
        };
        if !object.storage.is_thread_local() {
            return;
        }
        let file = unit
            .symbols()
            .is_bound_in_scope(object.symbol, unit.symbols().file_scope())
            .unwrap_or(false);
        let name = if file || object.storage.base() == StorageClass::Extern {
            object.name.spelling.clone()
        } else {
            format!("__evocc.static.{}", origins[&object.symbol])
        };
        let model = linkage
            .tls_models
            .get(&object.symbol)
            .copied()
            .unwrap_or(unit.tls_model());
        symbols.entry(name).or_insert((model, object.span));
    });
    module.tls = symbols
        .iter()
        .filter(|(name, _)| {
            module.globals.iter().any(|g| &g.symbol == *name)
                || module.extern_objects.iter().any(|g| &g.symbol == *name)
                || module.aliases.iter().any(|g| &g.name == *name)
        })
        .map(|(name, (model, _))| evocc_evoir::TlsSymbol {
            symbol: name.clone(),
            model: *model,
        })
        .collect();
    for function in &mut module.functions {
        let mut next = function
            .blocks
            .iter()
            .flat_map(|b| {
                b.args
                    .iter()
                    .map(|a| a.value.id)
                    .chain(b.insts.iter().flat_map(inst_results).map(|v| v.id))
            })
            .max()
            .unwrap_or(0)
            + 1;
        for block in &mut function.blocks {
            let mut mem = block
                .args
                .iter()
                .find(|a| a.value.ty == EvoIRType::Mem(Region::All))
                .map(|a| a.value.clone());
            let mut world = block
                .args
                .iter()
                .find(|a| a.value.ty == EvoIRType::World)
                .map(|a| a.value.clone());
            let mut remaps = HashMap::new();
            for inst in block.insts.iter_mut().chain([&mut block.terminator]) {
                evocc_evoir::remap_inst_uses(inst, &remaps);
                if let Inst::GlobalAddr { result, symbol } = inst {
                    if symbols.contains_key(symbol) {
                        let (Some(memory), Some(effect)) = (mem.as_ref(), world.as_ref()) else {
                            return Err(LowerError::InvalidGeneratedSsa {
                                message: "TLS address without effect parameters".into(),
                            });
                        };
                        let mem_out = Value {
                            id: next,
                            ty: memory.ty.clone(),
                        };
                        next += 1;
                        let world_out = Value {
                            id: next,
                            ty: EvoIRType::World,
                        };
                        next += 1;
                        remaps.insert(memory.id, mem_out.clone());
                        remaps.insert(effect.id, world_out.clone());
                        *inst = Inst::TlsAddr(evocc_evoir::TlsAddress {
                            result: result.clone(),
                            symbol: symbol.clone(),
                            mem_in: memory.clone(),
                            world_in: effect.clone(),
                            mem_out,
                            world_out,
                        });
                    }
                }
                for value in inst_results(inst) {
                    if value.ty == EvoIRType::Mem(Region::All) {
                        mem = Some(value.clone());
                    }
                    if value.ty == EvoIRType::World {
                        world = Some(value.clone());
                    }
                }
            }
        }
    }
    Ok(())
}
