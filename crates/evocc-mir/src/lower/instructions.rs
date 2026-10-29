//! EvoIR instruction selection by form family.

use super::*;

pub(super) fn record_symbolic_definition(lowerer: &mut Lowerer, inst: &Inst) {
    let symbolic = match inst {
        Inst::GlobalAddr { result, symbol } | Inst::FuncAddr { result, symbol } => {
            Some((result.id, (symbol.clone(), 0)))
        }
        Inst::BlockAddr { result, block } => Some((
            result.id,
            (format!("{}^b{block}", lowerer.function_name), 0),
        )),
        Inst::PointerCast { result, operand } => lowerer
            .symbolic_addresses
            .get(&operand.id)
            .cloned()
            .map(|address| (result.id, address)),
        Inst::Add(result, lhs, rhs) | Inst::Sub(result, lhs, rhs)
            if result.ty == EvoIRType::Bits(64) =>
        {
            let (base, offset, subtract) = if lowerer.symbolic_addresses.contains_key(&lhs.id) {
                (lhs, rhs, matches!(inst, Inst::Sub(..)))
            } else if matches!(inst, Inst::Add(..)) {
                (rhs, lhs, false)
            } else {
                (lhs, rhs, true)
            };
            lowerer
                .symbolic_addresses
                .get(&base.id)
                .and_then(|(name, addend)| {
                    let offset = *lowerer.constants.get(&offset.id)? as i64;
                    let addend = if subtract {
                        addend.checked_sub(offset)
                    } else {
                        addend.checked_add(offset)
                    }?;
                    Some((result.id, (name.clone(), addend)))
                })
        }
        _ => None,
    };
    if let Some((id, symbolic)) = symbolic {
        lowerer.symbolic_addresses.insert(id, symbolic);
    }
    if let Inst::Const(result, bits) = inst {
        lowerer.constants.insert(result.id, *bits);
    }
}

pub(super) fn lower_literal_or_pair(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<bool, String> {
    if let Some((result, bits)) = literal_integer_cast(inst, &lowerer.constants) {
        let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
        let dst = lowerer.fresh(class);
        insts.push(MInst::MovImm { dst, imm: bits });
        lowerer.bind(result, dst);
        lowerer.constants.insert(result.id, bits);
        return Ok(true);
    }
    if !lowerer.literal_only.is_empty()
        && inst_results(inst)
            .first()
            .is_some_and(|result| lowerer.literal_only.contains(&result.id))
    {
        return Ok(true);
    }
    // Section 5's `bits<128>` row expands each form over the halves, so it is
    // consulted before the rows that assume one register per value.
    if lower_pair(lowerer, insts, inst)? {
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn lower_basic(
    target: Target,
    signatures: &Signatures<'_>,
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::Const(result, bits) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst, imm: *bits });
            lowerer.bind(result, dst);
        }
        Inst::TlsAddr(a) => {
            if !KERNEL.admits(target, ProfileFeature::Tls) {
                return Err("TLS requires AArch64 ELF".into());
            }
            let model = signatures
                .tls
                .iter()
                .find(|s| s.symbol == a.symbol)
                .ok_or("missing TLS symbol")?
                .model;
            let name = &a.symbol;
            let (template, clobbers) = match model {
                evocc_evoir::TlsModel::LocalExec => (format!("mrs %0, tpidr_el0\nadd %0, %0, #:tprel_hi12:{name}\nadd %0, %0, #:tprel_lo12_nc:{name}"), vec![]),
                evocc_evoir::TlsModel::InitialExec => (format!("adrp x0, :gottprel:{name}\nldr x0, [x0, #:gottprel_lo12:{name}]\nmrs %0, tpidr_el0\nadd %0, %0, x0"), vec![AsmClobber::Register(0)]),
                evocc_evoir::TlsModel::GeneralDynamic | evocc_evoir::TlsModel::LocalDynamic => (format!("adrp x0, :tlsdesc:{name}\nldr x1, [x0, #:tlsdesc_lo12:{name}]\nadd x0, x0, #:tlsdesc_lo12:{name}\n.tlsdesccall {name}\nblr x1\nmrs %0, tpidr_el0\nadd %0, %0, x0"), vec![AsmClobber::Register(0),AsmClobber::Register(1),AsmClobber::Register(30),AsmClobber::Flags,AsmClobber::Memory]),
            };
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Asm(AsmBody {
                template,
                volatile: true,
                outputs: vec![AsmOperand::reg(
                    AsmConstraint::EarlyClobber(crate::AsmDirection::Write),
                    dst,
                )],
                inputs: vec![],
                clobbers,
            }));
            lowerer.bind(&a.result, dst);
        }
        // Section 5: "any value is permitted; zero is chosen so the output is
        // deterministic".
        Inst::Uninitialized(result) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst, imm: 0 });
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_integer_operations(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::Add(result, lhs, rhs) => simple(lowerer, insts, BinOp::Add, result, lhs, rhs)?,
        Inst::Sub(result, lhs, rhs) => simple(lowerer, insts, BinOp::Sub, result, lhs, rhs)?,
        Inst::Mul(result, lhs, rhs) => simple(lowerer, insts, BinOp::Mul, result, lhs, rhs)?,
        Inst::And(result, lhs, rhs) => simple(lowerer, insts, BinOp::And, result, lhs, rhs)?,
        Inst::Or(result, lhs, rhs) => simple(lowerer, insts, BinOp::Orr, result, lhs, rhs)?,
        Inst::Xor(result, lhs, rhs) => simple(lowerer, insts, BinOp::Eor, result, lhs, rhs)?,
        Inst::Shl(result, lhs, rhs) => {
            let value = lowerer.reg(lhs)?;
            lowerer.shift(insts, BinOp::Lsl, result, value, rhs)?;
        }
        Inst::LShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.zero_extend(insts, value, bits, width);
            lowerer.shift(insts, BinOp::Lsr, result, value, rhs)?;
        }
        Inst::AShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.sign_extend(insts, value, bits, width);
            lowerer.shift(insts, BinOp::Asr, result, value, rhs)?;
        }
        Inst::SDiv(result, lhs, rhs) => divide(lowerer, insts, true, false, result, lhs, rhs)?,
        Inst::UDiv(result, lhs, rhs) => divide(lowerer, insts, false, false, result, lhs, rhs)?,
        Inst::SRem(result, lhs, rhs) => divide(lowerer, insts, true, true, result, lhs, rhs)?,
        Inst::URem(result, lhs, rhs) => divide(lowerer, insts, false, true, result, lhs, rhs)?,
        Inst::SExt(result, source, _) | Inst::ZExt(result, source, _) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&source.ty);
            let src = lowerer.reg(source)?;
            let extended = if matches!(inst, Inst::SExt(..)) {
                lowerer.sign_extend(insts, src, bits, width)
            } else {
                lowerer.zero_extend(insts, src, bits, width)
            };
            let extended = if extended.class == class {
                extended
            } else {
                let dst = lowerer.fresh(class);
                insts.push(MInst::Mov { dst, src: extended });
                dst
            };
            lowerer.bind(result, extended);
        }
        // Section 5: "the extension of Section 3, or nothing for `trunc`". A
        // narrowing that also changes class needs a register of the other
        // class, which on the machine is the same register named at the other
        // width; the move is that renaming and nothing else.
        Inst::Trunc(result, source, _) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let src = lowerer.reg(source)?;
            if src.class == class {
                lowerer.bind(result, src);
            } else {
                let dst = lowerer.fresh(class);
                insts.push(MInst::Mov { dst, src });
                lowerer.bind(result, dst);
            }
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            let (flags, cond) = lower_compare(lowerer, insts, *operation, lhs, rhs)?;
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Cset { dst, flags, cond });
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_special_values(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::PtrNull(result) => {
            lowerer.null_pointers.insert(result.id);
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::MovImm { dst, imm: 0 });
            lowerer.bind(result, dst);
        }
        // Section 5's `va_start` and `va_end` rows. `va_arg` needs branches,
        // so it splits the block and is lowered where the blocks are built.
        Inst::VaAccess(access) => match &access.op {
            VariadicOp::Start => lower_va_start(lowerer, insts, &access.list)?,
            VariadicOp::End => {}
            VariadicOp::Arg(_) | VariadicOp::ArgRecord { .. } => {
                return Err("a va_arg is lowered with its block".to_owned());
            }
        },
        // Section 4: the `asm` of EvoIR Section 3.11, "carried through
        // unchanged: the template, its operands with their constraints, its
        // clobbers".
        Inst::Asm(assembly) => {
            let body = lower_assembly(lowerer, insts, assembly)?;
            insts.push(MInst::Asm(body));
        }
        // Section 5's bit-counting and byte-swap row (revision 0.14).
        Inst::Vector(vector) => lower_vector(lowerer, insts, vector)?,
        Inst::Floating(operation) => lower_floating(lowerer, insts, operation, false)?,
        Inst::StrictFloating(operation) => {
            lower_floating(lowerer, insts, &operation.instruction, true)?
        }
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            lower_bit_operation(lowerer, insts, result, operand, *op)?;
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_addresses(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        // Section 5 (revision 0.12): "`ptr.to_int`, `ptr.from_int.*` |
        // nothing: the value is the same 64-bit register". A pointer and a
        // `bits<64>` are both `gpr64`, so the conversion is an alias and emits
        // no instruction at all.
        Inst::PointerCast { result, operand } => {
            let src = lowerer.reg(operand)?;
            lowerer.bind(result, src);
        }
        Inst::GlobalAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Global);
        }
        Inst::FuncAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Function);
        }
        // Section 5 (revision 0.15): "`block_addr ^b` | `adr rd, <label of
        // block b>`". One instruction, not a page pair: `adr` reaches a
        // mebibyte either way, which covers any function this backend emits.
        Inst::BlockAddr { result, block } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Adr { dst, block: *block });
            lowerer.bind(result, dst);
        }
        // Section 5 (revision 0.17): "`return_address` | `ldr rd, [x29, #8]`:
        // the saved link register of this frame record, valid anywhere in the
        // body because every function keeps a frame pointer" and
        // "`frame_address` | `mov rd, x29`". Both places belong to the frame
        // layout, so both are the `frame.address` pseudo-form emission expands.
        Inst::ReturnAddress { result } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameAddress {
                dst,
                place: FramePlace::SavedLinkRegister,
            });
            lowerer.bind(result, dst);
        }
        Inst::FrameAddress { result, depth } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameAddress {
                dst,
                place: std::num::NonZeroU16::new(*depth)
                    .map_or(FramePlace::FramePointer, FramePlace::AncestorFrame),
            });
            lowerer.bind(result, dst);
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => {
            let base = lowerer.reg(base)?;
            let offset = lowerer.immediate_pointer_delta(index, *direction, *signedness, *scale);
            // Decode at the index's width before applying scale and direction.
            // This selects the same machine address; it does not remove the
            // following EvoIR pointer_valid obligation or move any access.
            let dst = if offset == Some(0) {
                base
            } else if let Some(offset) = offset.filter(|offset| offset.unsigned_abs() <= 4095) {
                let dst = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::BinaryImm {
                    op: if offset < 0 { BinOp::Sub } else { BinOp::Add },
                    dst,
                    lhs: base,
                    imm: offset.unsigned_abs(),
                    width: Width::X,
                });
                dst
            } else {
                let delta = lowerer.pointer_delta(insts, index, *signedness, *scale)?;
                let dst = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Binary {
                    op: match direction {
                        evocc_evoir::OffsetDirection::Add => BinOp::Add,
                        evocc_evoir::OffsetDirection::Subtract => BinOp::Sub,
                    },
                    dst,
                    lhs: base,
                    rhs: delta,
                    width: Width::X,
                });
                dst
            };
            if let Some(offset) = offset {
                lowerer
                    .address_preparation
                    .memory
                    .insert(result.id, (base, offset, insts.len()));
            }
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_storage(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::Alloca {
            result,
            origin,
            layout,
            ..
        } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameSlot {
                dst,
                origin: *origin,
                size: layout.size_bytes,
                align: layout.align_bytes,
            });
            lowerer.bind(result, dst);
        }
        Inst::DynamicAlloca {
            result,
            origin,
            size,
            align,
            ..
        } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            let size = lowerer.reg(size)?;
            insts.push(MInst::StackAlloc {
                dst,
                size,
                origin: *origin,
                align: *align,
            });
            lowerer.bind(result, dst);
        }
        // A dynamic object can release the native stack allocation. Fixed
        // frame slots keep their reserved frame positions; access guards
        // themselves still have no machine effect.
        Inst::LifetimeEnd { pointer, .. } => {
            if lowerer.dynamic_addresses.contains(&pointer.id) {
                insts.push(MInst::StackRelease {
                    address: lowerer.reg(pointer)?,
                });
            }
        }
        Inst::PointerValid { .. } => {}
        Inst::AccessInBounds {
            mem_out, layout, ..
        } => {
            lowerer
                .access_alignments
                .insert(mem_out.id, layout.align_bytes);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_atomics(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::Atomic(access) => {
            if let evocc_evoir::AtomicOp::Copy {
                op,
                buffers,
                result,
                layout,
            } = &access.op
            {
                lower_atomic_copy(
                    lowerer,
                    insts,
                    access,
                    *op,
                    buffers,
                    result.as_ref(),
                    *layout,
                )?;
                return Ok(());
            }
            if let evocc_evoir::AtomicOp::Rmw {
                result,
                operation,
                value,
                expected,
            } = &access.op
            {
                let dst = lowerer.fresh(class_of(&result.ty).map_err(|(code, _)| code.to_owned())?);
                insts.push(MInst::AtomicRmw {
                    dst,
                    base: lowerer.reg(&access.address)?,
                    value: lowerer.reg(value)?,
                    expected: expected
                        .as_ref()
                        .map(|value| lowerer.reg(value))
                        .transpose()?,
                    operation: *operation,
                    kind: load_kind(&result.ty)?,
                    order: access.order,
                    volatile: access.volatile,
                });
                lowerer.bind(result, dst);
                return Ok(());
            }
            let load = access.is_load();
            let scalar = access
                .value()
                .ok_or("atomic access without a scalar value")?;
            let kind = load_kind(&scalar.ty)?;
            let base = lowerer.reg(&access.address)?;
            let value = if load {
                lowerer.fresh(class_of(&scalar.ty).map_err(|(code, _)| code.to_owned())?)
            } else {
                lowerer.reg(scalar)?
            };
            insts.push(MInst::Atomic {
                load,
                value,
                base,
                kind,
                order: access.order,
                volatile: access.volatile,
            });
            if load {
                lowerer.bind(scalar, value);
            }
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_memory(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::Load {
            result,
            address,
            mem_in,
            volatile,
            mask,
            ..
        } => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let kind = load_kind(&result.ty)?;
            let dst = lowerer.fresh(class);
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            let (base, offset) =
                lowerer.memory_address(insts, address, kind.size_bytes(), align)?;
            insts.push(MInst::Load {
                align: if u64::from(align) == kind.size_bytes() {
                    0
                } else {
                    align
                },
                dst,
                base,
                offset,
                kind,
                volatile: *volatile,
                mask: register_mask(*mask)?,
                transport: false,
            });
            lowerer.bind(result, dst);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            volatile,
            mask,
            ..
        } => {
            let kind = store_kind(&value.ty)?;
            let src = lowerer.reg(value)?;
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            let (base, offset) =
                lowerer.memory_address(insts, address, kind.size_bytes(), align)?;
            insts.push(MInst::Store {
                align: if u64::from(align) == kind.size_bytes() {
                    0
                } else {
                    align
                },
                src,
                base,
                offset,
                kind,
                volatile: *volatile,
                mask: register_mask(*mask)?,
                transport: false,
            });
        }
        // Section 5: `copy` with static size lowers to `mem.copy`, the
        // pseudo-form that transfers per-byte initializedness and pointer
        // fragments; emission expands it.
        Inst::Copy {
            destination,
            source,
            layout,
            volatile,
            ..
        } => {
            let dst = lowerer.reg(destination)?;
            let src = lowerer.reg(source)?;
            insts.push(MInst::MemCopy {
                dst,
                src,
                size: layout.size_bytes,
                align: layout.align_bytes,
                volatile: *volatile,
            });
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_calls(
    target: Target,
    signatures: &Signatures<'_>,
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
    external: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    match inst {
        Inst::Call {
            results,
            callee,
            args,
            arg_marks,
            returns_twice,
            ..
        } => {
            let marks = call_argument_marks(signatures.marks(callee), arg_marks, args.len());
            let anonymous = if lowerer.anonymous_on_stack {
                signatures.anonymous_start(callee)
            } else {
                None
            };
            let plan = record_plan(
                target,
                &marked_types(&arg_types(args), &marks),
                true,
                anonymous,
                lowerer.anonymous_on_stack,
            );
            // Section 6.2: a cross-boundary call extends its narrow arguments.
            let (arguments, returned) =
                call_arguments(lowerer, insts, args, &marks, &plan, external(callee))?;
            let mut outputs = call_results(lowerer, results)?;
            let narrowed = apple_call_results(lowerer, &mut outputs, results);
            let halves = half_call_results(lowerer, &mut outputs, results);
            let after = call_record_result(lowerer, &mut outputs, returned);
            if plan.indirect_result {
                insts.push(MInst::IndirectResult);
            }
            insts.push(MInst::Bl {
                symbol: callee.clone(),
                args: arguments,
                results: outputs,
                fp_mode: None,
                returns_twice: *returns_twice,
            });
            insts.extend(narrowed);
            insts.extend(halves);
            insts.extend(after);
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            arg_marks,
            ..
        } => {
            // Revision 1.9.73 puts the marks inside the pointer's own type,
            // "part of the signature for equality", so an indirect call reads
            // them where it reads everything else about its callee.
            let marks: Vec<ParamMark> = match &callee.ty {
                EvoIRType::FnPtr { marks, .. } => marks.clone(),
                _ => Vec::new(),
            };
            let marks = call_argument_marks(&marks, arg_marks, args.len());
            let anonymous = match &callee.ty {
                EvoIRType::FnPtr {
                    params, variadic, ..
                } if lowerer.anonymous_on_stack => anonymous_start(*variadic, params),
                _ => None,
            };
            let plan = record_plan(
                target,
                &marked_types(&arg_types(args), &marks),
                true,
                anonymous,
                lowerer.anonymous_on_stack,
            );
            let pointer = lowerer.reg(callee)?;
            // Under the Apple convention an indirect call can reach code that
            // another compiler built, so it extends its narrow arguments too.
            let (arguments, returned) = call_arguments(
                lowerer,
                insts,
                args,
                &marks,
                &plan,
                lowerer.anonymous_on_stack,
            )?;
            let mut outputs = call_results(lowerer, results)?;
            let narrowed = apple_call_results(lowerer, &mut outputs, results);
            let halves = half_call_results(lowerer, &mut outputs, results);
            let after = call_record_result(lowerer, &mut outputs, returned);
            if plan.indirect_result {
                insts.push(MInst::IndirectResult);
            }
            insts.push(MInst::Blr {
                target: pointer,
                args: arguments,
                results: outputs,
            });
            insts.extend(narrowed);
            insts.extend(halves);
            insts.extend(after);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_division_predicates(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        // --- the undefined-behaviour predicates of Section 5 ----------------
        //
        // Every row below is the specification's, with one generalization the
        // specification's own technique dictates: a row written for a value at
        // the register width tests representability in the register, and EvoIR
        // asks for representability in `bits<N>`. Where `N` is narrower the
        // check is closed with the bitfield comparison Section 5 already uses
        // in the `signed_mul_defined` row ("compare with the sign extension of
        // its low 32 bits"), read at `N` instead of 32. Two further departures
        // are recorded with the gap they answer, at the row that makes them.
        Inst::DivisorNonZero(result, divisor) => {
            let bits = significant_bits(&divisor.ty);
            let class = class_of(&divisor.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let source = lowerer.reg(divisor)?;
            // `cmp` reads the whole register, so Section 3's zero-extension is
            // required here exactly as it is for `icmp.eq`. Section 5's row
            // does not say so; recorded gap.
            let source = lowerer.zero_extend(insts, source, bits, width);
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: source,
                imm: 0,
                width,
            });
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Cset {
                dst,
                flags,
                cond: Cond::Ne,
            });
            lowerer.bind(result, dst);
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            // Section 5: "sign-extend both per Section 3; `cmp %a, #INT_MIN_N`
            // (the minimum of `bits<N>`, materialized), `cset ne` into `%p`;
            // `cmp %b, #-1`, `cset ne` into `%q`; `orr %p, %q`".
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let minimum = lowerer.fresh(class);
            insts.push(MInst::MovImm {
                dst: minimum,
                imm: (1_u64 << (bits - 1)).wrapping_neg() & width.mask(),
            });
            let low = lowerer.compare_into(insts, left, minimum, width, Cond::Ne);
            let ones = lowerer.fresh(class);
            insts.push(MInst::MovImm {
                dst: ones,
                imm: width.mask(),
            });
            let high = lowerer.compare_into(insts, right, ones, width, Cond::Ne);
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Binary {
                op: BinOp::Orr,
                dst,
                lhs: low,
                rhs: high,
                width: Width::W,
            });
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_arithmetic_predicates(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::SignedAddDefined(result, lhs, rhs) | Inst::SignedSubDefined(result, lhs, rhs) => {
            let subtract = matches!(inst, Inst::SignedSubDefined(..));
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let dst = if bits == width.bits() {
                // Section 5: "`adds`/`subs` into a scratch, then `cset vc`".
                let scratch = lowerer.fresh(class);
                let flags = lowerer.fresh(RegClass::Flags);
                insts.push(MInst::FlagBinary {
                    op: if subtract { FlagOp::Subs } else { FlagOp::Adds },
                    dst: scratch,
                    flags,
                    lhs: left,
                    rhs: right,
                    width,
                });
                let dst = lowerer.fresh(RegClass::Gpr32);
                insts.push(MInst::Cset {
                    dst,
                    flags,
                    cond: Cond::Vc,
                });
                dst
            } else {
                // The `v` flag reports overflow of the register, not of
                // `bits<N>`: sign-extended narrow operands never set it, so
                // the row answers "defined" where EvoIR answers "not". The sum
                // is exact at the register width, and representability in `N`
                // bits is the bitfield comparison. Recorded gap.
                let sum = lowerer.fresh(class);
                insts.push(MInst::Binary {
                    op: if subtract { BinOp::Sub } else { BinOp::Add },
                    dst: sum,
                    lhs: left,
                    rhs: right,
                    width,
                });
                lowerer.representable_in(insts, sum, bits, width)
            };
            lowerer.bind(result, dst);
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let dst = if width == Width::W {
                // Section 5, 32-bit: "`smull` the sign-extended operands,
                // compare with the sign extension of its low 32 bits". Read at
                // `N` rather than at 32, which is the same instruction and the
                // same comparison; the product of two values of at most 32
                // significant bits is exact in the 64-bit result.
                let product = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::WideMultiply {
                    op: WideMul::Smull,
                    dst: product,
                    lhs: left,
                    rhs: right,
                });
                lowerer.representable_in(insts, product, bits, Width::X)
            } else {
                // Section 5, 64-bit: "`mul` and `smulh`, compare the high word
                // with `asr` of the low word by 63".
                let low = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Binary {
                    op: BinOp::Mul,
                    dst: low,
                    lhs: left,
                    rhs: right,
                    width: Width::X,
                });
                let high = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::WideMultiply {
                    op: WideMul::Smulh,
                    dst: high,
                    lhs: left,
                    rhs: right,
                });
                let sign = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::BinaryImm {
                    op: BinOp::Asr,
                    dst: sign,
                    lhs: low,
                    imm: 63,
                    width: Width::X,
                });
                let exact = lowerer.compare_into(insts, high, sign, Width::X, Cond::Eq);
                if bits == 64 {
                    exact
                } else {
                    let fits = lowerer.representable_in(insts, low, bits, Width::X);
                    lowerer.both(insts, exact, fits)
                }
            };
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}

pub(super) fn lower_shift_predicates(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
) -> Result<(), String> {
    match inst {
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let dst = lowerer.count_defined(insts, shifted, amount, *amount_signed)?;
            lowerer.bind(result, dst);
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let count_ok = lowerer.count_defined(insts, shifted, amount, *amount_signed)?;
            let bits = significant_bits(&shifted.ty);
            let class = class_of(&shifted.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let value = lowerer.reg(shifted)?;
            let value = lowerer.sign_extend(insts, value, bits, width);
            // EvoIR Section 3.3.1 makes the predicate "false for an invalid
            // count, a negative left operand, or a shifted result not
            // representable in signed bits<N>". Section 5's row states the
            // count and the round trip and omits the negative operand;
            // recorded gap.
            let zero = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst: zero, imm: 0 });
            let non_negative = lowerer.compare_into(insts, value, zero, width, Cond::Ge);
            let amount_reg = lowerer.shift_amount(insts, amount)?;
            let shifted_left = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Lsl,
                dst: shifted_left,
                lhs: value,
                rhs: amount_reg,
                width,
            });
            let back = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Asr,
                dst: back,
                lhs: shifted_left,
                rhs: amount_reg,
                width,
            });
            // Section 5: "(x << amt) >> amt == x with an arithmetic right
            // shift".
            let round_trip = lowerer.compare_into(insts, back, value, width, Cond::Eq);
            let mut dst = lowerer.both(insts, count_ok, non_negative);
            dst = lowerer.both(insts, dst, round_trip);
            if bits < width.bits() {
                // The round trip tests representability in the register, not
                // in bits<N>; the same recorded gap as the additive rows.
                let fits = lowerer.representable_in(insts, shifted_left, bits, width);
                dst = lowerer.both(insts, dst, fits);
            }
            lowerer.bind(result, dst);
        }
        _ => unreachable!(),
    }
    Ok(())
}
