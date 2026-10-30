//! Lowering assembly.

use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn bound_assembly_constraint(
        &self,
        constraint: &str,
        expression: &CHirExpr<'_>,
        span: SourceSpan,
    ) -> Result<String, LowerError> {
        let CHirExpr::Ident { symbol, .. } = expression else {
            return Ok(constraint.to_owned());
        };
        let Some(register) = self.local_registers.get(symbol) else {
            return Ok(constraint.to_owned());
        };
        // Memory/immediate constraints do not request the object's register.
        if !(matches!(
            constraint,
            "r" | "rZ" | "=r" | "+r" | "=&r" | "+&r" | "w" | "=w" | "+w" | "=&w" | "+&w"
        ) || (constraint.len() == 1 && constraint.as_bytes()[0].is_ascii_digit()))
        {
            return Ok(constraint.to_owned());
        }
        let register = if let Some(n) = register.strip_prefix(['d', 's']) {
            format!("v{n}")
        } else {
            register
                .strip_prefix(['w', 'r'])
                .map_or_else(|| register.clone(), |n| format!("x{n}"))
        };
        let base = if constraint == "rZ" { "r" } else { constraint };
        let bound = format!("{base}@{register}");
        evocc_evoir::assembly_register_binding(&bound)
            .map_err(|reason| LowerError::AsmOperand { span, reason })?;
        Ok(bound)
    }

    pub(super) fn assembly_constant(
        &self,
        expression: &CHirExpr<'_>,
        constraint: &str,
    ) -> Result<AssemblyConstant, LowerError> {
        let refuse = || LowerError::AsmOperand {
            span: expression.span(),
            reason: constraint.to_owned(),
        };
        if evocc_chir::builtin_constant_p(
            expression,
            self.unit.types(),
            self.unit.symbols(),
            self.target,
        ) {
            if let Ok(integer) = evocc_chir::interp::interpret_static_integer_expression(
                self.unit,
                expression,
                self.target,
            ) {
                if integer.ty().bits() > 64 {
                    return Err(LowerError::WideInteger {
                        span: expression.span(),
                    });
                }
                return Ok(AssemblyConstant::Integer(integer));
            }
        }
        if constraint != "i" {
            return Err(refuse());
        }
        if null_pointer_constant(self.unit, self.target, expression) {
            return Ok(AssemblyConstant::Integer(CInteger::new(
                0,
                CIntegerType::ULONG,
            )));
        }
        let mut references = FunctionReferences::default();
        collect_references_expr(expression, &mut references);
        let labels = references
            .labels
            .into_iter()
            .map(|(expression, name)| {
                self.address_labels
                    .get(&name)
                    .map(|block| (expression, format!("{}^b{block}", self.info.name)))
                    .ok_or_else(refuse)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let (symbol, addend, _) = symbolic_address(
            self.unit,
            self.target,
            expression,
            false,
            self.allocation_origins,
            &self.string_origins,
            &labels,
        )
        .map_err(|_| refuse())?;
        Ok(AssemblyConstant::Address(symbol, addend))
    }

    pub(super) fn emit_assembly_constant(
        &mut self,
        constant: AssemblyConstant,
        span: SourceSpan,
        state: &State,
    ) -> Result<Value, LowerError> {
        let (symbol, addend) = match constant {
            AssemblyConstant::Integer(integer) => {
                return self
                    .constant(state, integer.ty(), integer.raw_bits() as u64)
                    .map(|value| value.value);
            }
            AssemblyConstant::Address(symbol, addend) => (symbol, addend),
        };
        let address = if let Some((_, block)) = symbol.split_once("^b") {
            let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            let block = block.parse().map_err(|_| LowerError::AsmOperand {
                span,
                reason: "i label identity".to_owned(),
            })?;
            self.emit(
                state,
                Inst::BlockAddr {
                    result: result.clone(),
                    block,
                },
            )?;
            result
        } else if let Some(id) = self
            .functions
            .iter()
            .find_map(|(id, function)| (function.name == symbol).then_some(*id))
        {
            self.function_address(id, span, state)?.value
        } else {
            let result = self.fresh_value(EvoIRType::Ptr(AddressSpace::Generic));
            self.emit(
                state,
                Inst::GlobalAddr {
                    result: result.clone(),
                    symbol,
                },
            )?;
            result
        };
        let integer = self.fresh_value(EvoIRType::Bits(64));
        self.emit(
            state,
            Inst::PointerCast {
                result: integer.clone(),
                operand: address,
            },
        )?;
        if addend == 0 {
            return Ok(integer);
        }
        let offset = self
            .constant(state, CIntegerType::ULONG, addend as u64)?
            .value;
        let result = self.fresh_value(EvoIRType::Bits(64));
        self.emit(state, Inst::Add(result.clone(), integer, offset))?;
        Ok(result)
    }

    pub(super) fn lower_assembly(
        &mut self,
        source: &'a AsmStmt<'a>,
        mut state: State,
        live_after: &BTreeSet<SymbolId>,
    ) -> Result<Flow, LowerError> {
        let refuse = |reason: String| LowerError::AsmOperand {
            span: source.span,
            reason,
        };
        let mut outputs = Vec::new();
        let mut inputs = Vec::new();
        let mut result_types = Vec::new();
        let mut writebacks = Vec::new();
        let mut temporary_keys = Vec::new();
        let mut incoming_keys = Vec::new();
        let mut pending_inputs = Vec::new();
        for operand in &source.outputs {
            // Select the register alternative before binding the place. The
            // source keeps its written constraint; IR has no alternatives.
            let constraint = match operand.constraint.as_str() {
                "=rm" | "=mr" | "=g" => "=r",
                "+rm" | "+mr" | "+g" => "+r",
                "=&rm" | "=&mr" | "=&g" => "=&r",
                "+&rm" | "+&mr" | "+&g" => "+&r",
                "=m" => "=Q",
                "+m" => "+Q",
                constraint => constraint,
            };
            if !matches!(
                constraint,
                "=r" | "+r" | "=&r" | "+&r" | "=w" | "+w" | "=&w" | "+&w" | "=Q" | "+Q"
            ) {
                return Err(refuse(operand.constraint.clone()));
            }
            let local = match operand.expression {
                CHirExpr::Ident { symbol, facts, .. }
                    if !evocc_evoir::assembly_memory_constraint(constraint)
                        && !self.memory_symbols.contains(symbol)
                        && !self.global_registers.contains_key(symbol) =>
                {
                    Some((*symbol, facts.ty()))
                }
                _ => None,
            };
            let incoming = if let Some((symbol, ty)) = local {
                let (ir, integer) = self.assembly_register_value_type(ty, operand.span)?;
                result_types.push(ir);
                writebacks.push((AssemblyWriteback::Local(symbol), integer));
                if constraint.starts_with('+') {
                    Some(self.local(&mut state, symbol, operand.span)?.value)
                } else {
                    None
                }
            } else {
                let address = self
                    .lower_unary(
                        UnaryOp::AddrOf,
                        operand.expression,
                        operand.span,
                        &mut state,
                    )?
                    .value;
                if evocc_evoir::assembly_memory_constraint(constraint) {
                    Some(address)
                } else {
                    let ty = match operand.expression {
                        CHirExpr::Ident { facts, .. } | CHirExpr::MemoryRead { facts, .. } => {
                            facts.ty()
                        }
                        CHirExpr::Unary(UnaryOp::Deref, pointer, span) => {
                            self.pointer_pointee_type(pointer, *span)?
                        }
                        _ => return Err(refuse(format!("{} output place", operand.constraint))),
                    };
                    let (ir, integer) = self.assembly_register_value_type(ty, operand.span)?;
                    let layout = match operand.expression {
                        CHirExpr::MemoryRead { place, .. } => {
                            self.place_access_layout(place, operand.span)?
                        }
                        _ => self.access_layout(ty, operand.span)?,
                    };
                    let volatile = self
                        .unit
                        .types()
                        .get(ty)
                        .is_some_and(|ty| ty.qualifiers.is_volatile);
                    let address_key = self.retain_temporary(address.clone(), &mut state);
                    temporary_keys.push(address_key);
                    result_types.push(ir.clone());
                    writebacks.push((
                        AssemblyWriteback::Memory {
                            address: address_key,
                            layout,
                            volatile,
                        },
                        integer,
                    ));
                    if constraint.starts_with('+') {
                        Some(
                            self.emit_typed_memory_load(
                                address, ir, integer, volatile, &mut state, layout,
                            )?
                            .value,
                        )
                    } else {
                        None
                    }
                }
            };
            let ty = if evocc_evoir::assembly_memory_constraint(constraint) {
                incoming.as_ref().expect("memory address").ty.clone()
            } else {
                result_types.last().expect("register output").clone()
            };
            let key = incoming
                .as_ref()
                .map(|value| self.retain_temporary(value.clone(), &mut state));
            temporary_keys.extend(key);
            incoming_keys.push(key);
            outputs.push(AssemblyOutput {
                constraint: self.bound_assembly_constraint(
                    constraint,
                    operand.expression,
                    operand.span,
                )?,
                name: operand.symbolic.clone().unwrap_or_default(),
                ty,
                incoming,
            });
        }
        for operand in &source.inputs {
            if operand.constraint.contains('@') {
                return Err(refuse(operand.constraint.clone()));
            }
            let immediate = matches!(
                operand.constraint.as_str(),
                "I" | "J" | "K" | "L" | "i" | "n"
            );
            if immediate {
                let constant = match self.assembly_constant(operand.expression, &operand.constraint)
                {
                    Ok(constant) => constant,
                    Err(error) => {
                        let Some(value) = self.result_after_evaluation(operand.expression) else {
                            return Err(error);
                        };
                        if value.ty().bits() > 64 {
                            return Err(LowerError::WideInteger { span: operand.span });
                        }
                        // A unique result need not be a source constant. Keep
                        // the original evaluation, including calls, volatile
                        // reads and UB obligations, before using its known bits.
                        self.lower_expr(operand.expression, &mut state)?;
                        AssemblyConstant::Integer(value)
                    }
                };
                pending_inputs.push(AssemblyInputSource::Constant(constant));
                continue;
            }
            let value = if evocc_evoir::assembly_memory_constraint(&operand.constraint) {
                self.lower_unary(
                    UnaryOp::AddrOf,
                    operand.expression,
                    operand.span,
                    &mut state,
                )?
                .value
            } else if let Some(ty) = self.record_value_type(operand.expression) {
                let (ir, integer) = self.assembly_register_value_type(ty, operand.span)?;
                let address = self.lower_expr(operand.expression, &mut state)?.value;
                let layout = self.access_layout(ty, operand.span)?;
                self.emit_typed_memory_load(
                    address,
                    ir,
                    integer,
                    self.aggregate_source_is_volatile(operand.expression),
                    &mut state,
                    layout,
                )?
                .value
            } else {
                self.lower_expr(operand.expression, &mut state)?.value
            };
            let key = self.retain_temporary(value.clone(), &mut state);
            temporary_keys.push(key);
            pending_inputs.push(AssemblyInputSource::Temporary(key));
        }
        for (operand, pending) in source.inputs.iter().zip(pending_inputs) {
            // Immediate expression effects have already executed. Define their
            // values here, after other operands may have split the block, so
            // machine lowering need not guess constants through edge arguments.
            let mut value = match pending {
                AssemblyInputSource::Constant(constant) => {
                    self.emit_assembly_constant(constant, operand.span, &state)?
                }
                AssemblyInputSource::Temporary(key) => self.token(&state, key)?.value,
            };
            // A tie names a physical location, not a C arithmetic conversion.
            // RELOC_HIDE ties a pointer to an unsigned long output. Make that
            // shared address representation explicit before any truncation,
            // without weakening the IR validator's equal-type requirement.
            // A wider input supplies the low part of a narrower output
            // location; never guess high bits for the reverse case.
            if let [digit @ b'0'..=b'9'] = operand.constraint.as_bytes() {
                if let Some(output) = outputs.get(usize::from(*digit - b'0')) {
                    if matches!(output.ty, EvoIRType::Bits(_))
                        && value.ty != EvoIRType::Bits(64)
                        && evocc_evoir::assembly_address_type(&value.ty)
                    {
                        let result = self.fresh_value(EvoIRType::Bits(64));
                        self.emit(
                            &state,
                            Inst::PointerCast {
                                result: result.clone(),
                                operand: value,
                            },
                        )?;
                        value = result;
                    }
                    if let (EvoIRType::Bits(destination), EvoIRType::Bits(source)) =
                        (&output.ty, &value.ty)
                    {
                        if destination < source {
                            let result = self.fresh_value(output.ty.clone());
                            self.emit(
                                &state,
                                Inst::Trunc(result.clone(), value, output.ty.clone()),
                            )?;
                            value = result;
                        }
                    }
                    if !evocc_evoir::assembly_memory_constraint(&output.constraint)
                        && output.ty != value.ty
                        && evocc_evoir::assembly_address_type(&output.ty)
                        && evocc_evoir::assembly_address_type(&value.ty)
                    {
                        if value.ty != EvoIRType::Bits(64) {
                            let result = self.fresh_value(EvoIRType::Bits(64));
                            self.emit(
                                &state,
                                Inst::PointerCast {
                                    result: result.clone(),
                                    operand: value,
                                },
                            )?;
                            value = result;
                        }
                        if output.ty != EvoIRType::Bits(64) {
                            let result = self.fresh_value(output.ty.clone());
                            self.emit(
                                &state,
                                Inst::PointerCast {
                                    result: result.clone(),
                                    operand: value,
                                },
                            )?;
                            value = result;
                        }
                    }
                }
            }
            // An admitted immediate letter paired with r permits a register
            // for every value. Select it after the single evaluation, leaving
            // no alternatives for downstream consumers to resolve.
            let register_alternative = matches!(
                operand.constraint.as_bytes(),
                [b'I' | b'J' | b'K' | b'L' | b'i' | b'n' | b'm', b'r']
                    | [b'r', b'I' | b'J' | b'K' | b'L' | b'i' | b'n' | b'm']
            );
            let constraint = if operand.constraint == "p" {
                if !evocc_evoir::assembly_address_type(&value.ty) {
                    return Err(refuse(operand.constraint.clone()));
                }
                "r".to_owned()
            } else if register_alternative || operand.constraint == "g" {
                if !evocc_evoir::assembly_register_type(&value.ty) {
                    return Err(refuse(operand.constraint.clone()));
                }
                "r".to_owned()
            } else if operand.constraint == "m" {
                "Q".to_owned()
            } else {
                operand.constraint.clone()
            };
            inputs.push(AssemblyInput {
                constraint: self.bound_assembly_constraint(
                    &constraint,
                    operand.expression,
                    operand.span,
                )?,
                name: operand.symbolic.clone().unwrap_or_default(),
                value,
            });
        }
        for (output, key) in outputs.iter_mut().zip(incoming_keys) {
            if let Some(key) = key {
                output.incoming = Some(self.token(&state, key)?.value);
            }
        }
        let results = result_types
            .into_iter()
            .map(|ty| self.fresh_value(ty))
            .collect::<Vec<_>>();
        let assembly = InlineAssembly {
            template: source_assembly_template(source)?,
            volatile: source.volatile_span.is_some(),
            outputs,
            inputs,
            clobbers: source.clobbers.clone(),
            results,
            mem_in: self.token(&state, EnvKey::Memory)?.value,
            world_in: self.token(&state, EnvKey::World)?.value,
            mem_out: self.fresh_value(EvoIRType::Mem(Region::All)),
            world_out: self.fresh_value(EvoIRType::World),
        };
        let labels = source
            .labels
            .iter()
            .map(|label| AssemblyLabel {
                name: label.spelling.clone(),
                edge: AssemblyEdge {
                    target: 0,
                    arguments: vec![],
                },
            })
            .collect::<Vec<_>>();
        assembly.validate(&labels).map_err(refuse)?;
        self.replace_effect_tokens(
            &mut state,
            assembly.mem_out.clone(),
            Some(assembly.world_out.clone()),
        );
        let mut result_keys = Vec::new();
        for result in &assembly.results {
            let key = self.retain_temporary(result.clone(), &mut state);
            temporary_keys.push(key);
            result_keys.push(key);
        }
        let mut successors = Vec::new();
        if source.goto_span.is_some() {
            let keys = self.keys_for(&state, &state.locals, &[]);
            let mut labels = Vec::new();
            for label in &source.labels {
                let successor = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
                labels.push(AssemblyLabel {
                    name: label.spelling.clone(),
                    edge: AssemblyEdge {
                        target: successor.block,
                        arguments: self.edge_values(&state, &keys)?,
                    },
                });
                successors.push((Some(label.spelling.clone()), successor));
            }
            let fallthrough = self.new_block_from(&state, &keys, BlockRole::Ordinary)?;
            let edge = AssemblyEdge {
                target: fallthrough.block,
                arguments: self.edge_values(&state, &keys)?,
            };
            self.terminate(
                &state,
                Inst::AsmGoto {
                    assembly,
                    labels,
                    fallthrough: edge,
                },
            )?;
            successors.push((None, fallthrough));
        } else {
            self.emit(&state, Inst::Asm(assembly))?;
            successors.push((None, state));
        }
        let mut normal = Vec::new();
        for (label, mut successor) in successors {
            for ((destination, integer), result) in writebacks.iter().zip(&result_keys) {
                let value = ExprValue {
                    value: self.token(&successor, *result)?.value,
                    integer: *integer,
                };
                match destination {
                    AssemblyWriteback::Local(symbol) => {
                        self.store_local(*symbol, value, source.span, &mut successor)?;
                    }
                    AssemblyWriteback::Memory {
                        address,
                        layout,
                        volatile,
                    } => {
                        let address = self.token(&successor, *address)?.value;
                        self.emit_memory_store_with_layout(
                            address,
                            value,
                            *layout,
                            *volatile,
                            &mut successor,
                        )?;
                    }
                }
            }
            for key in &temporary_keys {
                successor.env.remove(key);
            }
            if let Some(label) = label {
                if let Some(target) = self.labels.get(&label).cloned() {
                    self.branch(&successor, target.id, &target.keys)?;
                } else {
                    self.pending_gotos.entry(label).or_default().push(successor);
                }
            } else {
                normal.push(successor);
            }
        }
        let _ = live_after;
        Ok(Flow { normal })
    }

    pub(super) fn register_assembly(
        &mut self,
        symbol: SymbolId,
        assigned: Option<ExprValue>,
        span: SourceSpan,
        state: &mut State,
    ) -> Result<ExprValue, LowerError> {
        let register = &self.global_registers[&symbol];
        let declaration = self
            .unit
            .symbols()
            .symbol(symbol)
            .map_err(|_| LowerError::UnknownSymbol { symbol, span })?;
        let (ty, integer) = self.c_value_type(declaration.ty(), span)?;
        let (template, outputs, inputs, results, value) = if let Some(value) = assigned {
            (
                format!("mov {register}, %0"),
                vec![],
                vec![AssemblyInput {
                    constraint: "r".into(),
                    name: String::new(),
                    value: value.value.clone(),
                }],
                vec![],
                value,
            )
        } else {
            let result = self.fresh_value(ty.clone());
            (
                format!("mov %0, {register}"),
                vec![AssemblyOutput {
                    constraint: "=r".into(),
                    name: String::new(),
                    ty,
                    incoming: None,
                }],
                vec![],
                vec![result.clone()],
                ExprValue {
                    value: result,
                    integer,
                },
            )
        };
        let assembly = InlineAssembly {
            template,
            volatile: true,
            outputs,
            inputs,
            results,
            clobbers: vec![],
            mem_in: self.token(state, EnvKey::Memory)?.value,
            world_in: self.token(state, EnvKey::World)?.value,
            mem_out: self.fresh_value(EvoIRType::Mem(Region::All)),
            world_out: self.fresh_value(EvoIRType::World),
        };
        assembly
            .validate(&[])
            .map_err(|reason| LowerError::AsmOperand { span, reason })?;
        self.replace_effect_tokens(
            state,
            assembly.mem_out.clone(),
            Some(assembly.world_out.clone()),
        );
        self.emit(state, Inst::Asm(assembly))?;
        Ok(value)
    }
}
