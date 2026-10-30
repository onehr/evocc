use super::*;

/// Assembly immediates are link-time operands, not runtime address loads.
/// Keep their pure definition closure for resolution without creating GPRs or
/// otherwise introducing references absent from the emitted template. Any
/// ordinary use, including an edge argument, still needs its original value.
pub(crate) fn assembly_literal_definitions(block: &Block) -> HashSet<u32> {
    let instructions = || block.insts.iter().chain(std::iter::once(&block.terminator));
    let mut immediates = Vec::new();
    for inst in instructions() {
        let (Inst::Asm(body) | Inst::AsmGoto { assembly: body, .. }) = inst else {
            continue;
        };
        for input in &body.inputs {
            if matches!(input.constraint.as_str(), "I" | "J" | "K" | "L" | "i" | "n") {
                immediates.push(input.value.id);
            }
        }
    }
    let mut literals = HashSet::new();
    if immediates.is_empty() {
        return literals;
    }
    let mut uses: HashMap<u32, usize> = HashMap::new();
    for value in instructions().flat_map(inst_uses) {
        *uses.entry(value.id).or_default() += 1;
    }
    for value in immediates {
        *uses.get_mut(&value).expect("counted assembly use") -= 1;
    }
    for inst in block.insts.iter().rev() {
        let result = match inst {
            Inst::Const(value, _) if matches!(value.ty, EvoIRType::Bits(1..=64)) => value,
            Inst::GlobalAddr { result, .. }
            | Inst::FuncAddr { result, .. }
            | Inst::BlockAddr { result, .. }
            | Inst::PointerCast { result, .. } => result,
            Inst::Add(value, ..) | Inst::Sub(value, ..) if value.ty == EvoIRType::Bits(64) => value,
            _ => continue,
        };
        if uses.get(&result.id).copied().unwrap_or(0) == 0 {
            literals.insert(result.id);
            for value in inst_uses(inst) {
                *uses.get_mut(&value.id).expect("counted definition use") -= 1;
            }
        }
    }
    literals
}

/// Whether every constraint and clobber of one `asm` is in the closed
/// vocabulary EvoMIR carries.
///
/// EvoIR has already refused anything outside its own table (`E3021`); this is
/// the machine side of the same closure, so a constraint EvoMIR cannot spell
/// is named rather than approximated.
pub(crate) fn assembly_vocabulary(assembly: &InlineAssembly) -> Result<(), String> {
    for output in &assembly.outputs {
        if AsmConstraint::parse(evocc_evoir::assembly_register_binding(&output.constraint)?.0)
            .is_none()
        {
            return Err(format!(
                "an assembly output constraint '{}'",
                output.constraint
            ));
        }
    }
    for input in &assembly.inputs {
        if AsmConstraint::parse(evocc_evoir::assembly_register_binding(&input.constraint)?.0)
            .is_none()
        {
            return Err(format!(
                "an assembly input constraint '{}'",
                input.constraint
            ));
        }
    }
    for clobber in &assembly.clobbers {
        match AsmClobber::parse(clobber) {
            None => return Err(format!("an assembly clobber '{clobber}'")),
            // Section 6.7: the frame record and every frame address use the
            // frame register after the prologue, so no `asm` may write it.
            Some(AsmClobber::Register(number))
                if number == crate::isa::aarch64::regfile::FRAME_POINTER =>
            {
                return Err(format!(
                    "an assembly clobber '{clobber}' of the frame register"
                ));
            }
            Some(_) => {}
        }
    }
    Ok(())
}

/// Carry one EvoIR `asm` into its EvoMIR body.
///
/// EvoIR Section 3.11: "Each register output defines one fresh SSA result, in
/// output-list order. A memory output instead carries its place's pointer as
/// the incoming value and its type is that pointer's type; it defines no
/// scalar result". Inputs and outputs have distinct SSA identities, even when
/// the template needs one physical register. Read-write outputs use appended
/// tied inputs. Original template operand numbers therefore do not change.
/// Allocation inserts the two-address copies after SSA destruction.
pub(crate) fn lower_assembly(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    assembly: &InlineAssembly,
) -> Result<AsmBody, String> {
    let named = |name: &str| -> Option<String> { (!name.is_empty()).then(|| name.to_owned()) };
    let mut outputs: Vec<AsmOperand> = Vec::new();
    let mut read_write_inputs = Vec::new();
    let mut results = assembly.results.iter();
    for output in &assembly.outputs {
        let (base, fixed_register) = evocc_evoir::assembly_register_binding(&output.constraint)?;
        let mut constraint = AsmConstraint::parse(base)
            .ok_or_else(|| format!("an assembly output constraint '{}'", output.constraint))?;
        if let AsmConstraint::Memory { .. } = constraint {
            let place = output
                .incoming
                .as_ref()
                .ok_or_else(|| "a memory output with no place".to_owned())?;
            outputs.push(AsmOperand {
                constraint,
                fixed_register,
                name: named(&output.name),
                value: AsmValue::Reg(lowerer.reg(place)?),
            });
            continue;
        }
        let result = results
            .next()
            .ok_or_else(|| "a register assembly output with no result".to_owned())?;
        let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
        let register = lowerer.fresh(class);
        if let Some(incoming) = &output.incoming {
            let src = lowerer.reg(incoming)?;
            read_write_inputs.push(AsmOperand {
                constraint: AsmConstraint::Tied(
                    u8::try_from(outputs.len()).map_err(|_| "too many assembly outputs")?,
                ),
                fixed_register,
                name: None,
                value: AsmValue::Reg(src),
            });
            constraint = match constraint {
                AsmConstraint::Register(_) => AsmConstraint::Register(AsmDirection::Write),
                AsmConstraint::EarlyClobber(_) => AsmConstraint::EarlyClobber(AsmDirection::Write),
                AsmConstraint::Simd(_) => AsmConstraint::Simd(AsmDirection::Write),
                AsmConstraint::SimdEarlyClobber(_) => {
                    AsmConstraint::SimdEarlyClobber(AsmDirection::Write)
                }
                _ => return Err("a read-write output without a register constraint".into()),
            };
        }
        lowerer.bind(result, register);
        outputs.push(AsmOperand {
            constraint,
            fixed_register,
            name: named(&output.name),
            value: AsmValue::Reg(register),
        });
    }

    let mut inputs: Vec<AsmOperand> = Vec::new();
    for input in &assembly.inputs {
        let (base, fixed_register) = evocc_evoir::assembly_register_binding(&input.constraint)?;
        let constraint = AsmConstraint::parse(base)
            .ok_or_else(|| format!("an assembly input constraint '{}'", input.constraint))?;
        let value = match constraint {
            AsmConstraint::Immediate(_) => {
                if base == "i" {
                    if let Some((name, addend)) = lowerer.symbolic_addresses.get(&input.value.id) {
                        inputs.push(AsmOperand {
                            constraint,
                            fixed_register,
                            name: named(&input.name),
                            value: AsmValue::Symbol {
                                name: name.clone(),
                                addend: *addend,
                            },
                        });
                        continue;
                    }
                }
                let literal = lowerer
                    .constants
                    .get(&input.value.id)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "an immediate assembly operand whose value v{} is not a constant",
                            input.value.id
                        )
                    })?;
                AsmValue::Imm(literal as i64)
            }
            AsmConstraint::RegisterOrZero => {
                // "a general register, or the zero register when the operand
                // is the constant zero".
                if lowerer.constants.get(&input.value.id) == Some(&0) {
                    let class = class_of(&input.value.ty).map_err(|(code, _)| code.to_owned())?;
                    AsmValue::Zero(class)
                } else {
                    AsmValue::Reg(lowerer.reg(&input.value)?)
                }
            }
            AsmConstraint::Tied(digit) => {
                let output = outputs.get(usize::from(digit)).ok_or_else(|| {
                    format!("a tied operand names output {digit}, which is absent")
                })?;
                let output_register = output.register().ok_or_else(|| {
                    format!("a tied operand names output {digit}, which holds no register")
                })?;
                let src = lowerer.reg(&input.value)?;
                if src.class == output_register.class {
                    AsmValue::Reg(src)
                } else {
                    // The source has already selected the output width. Keep
                    // its existing low-part conversion as a separate SSA def.
                    let converted = lowerer.fresh(output_register.class);
                    insts.push(MInst::Mov {
                        dst: converted,
                        src,
                    });
                    AsmValue::Reg(converted)
                }
            }
            _ => AsmValue::Reg(lowerer.reg(&input.value)?),
        };
        inputs.push(AsmOperand {
            constraint,
            fixed_register,
            name: named(&input.name),
            value,
        });
    }
    inputs.extend(read_write_inputs);

    let mut clobbers = Vec::new();
    for clobber in &assembly.clobbers {
        clobbers.push(
            AsmClobber::parse(clobber).ok_or_else(|| format!("an assembly clobber '{clobber}'"))?,
        );
    }

    Ok(AsmBody {
        template: assembly.template.clone(),
        volatile: assembly.volatile,
        outputs,
        inputs,
        clobbers,
    })
}
