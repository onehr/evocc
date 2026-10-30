//! EvoIR instruction types.

use super::*;

pub(super) fn validate_instruction_types(inst: &Inst, block: BlockId) -> Result<(), String> {
    match inst {
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            expect_bits(&result.ty, "bit operation result", block)?;
            if result.ty != operand.ty
                || (*op == BitOp::ByteSwap && !matches!(result.ty, EvoIRType::Bits(16 | 32 | 64)))
            {
                return Err(
                    "bit operation operand/result type mismatch or invalid byte-swap width".into(),
                );
            }
            Ok(())
        }
        Inst::Vector(operation) => operation.validate(),
        Inst::Floating(operation) => operation.validate(),
        Inst::StrictFloating(operation) => operation.validate(),
        Inst::VaAccess(access) => access.validate(),
        Inst::Atomic(access) => access.validate(),
        Inst::TlsAddr(access) => access.validate(),
        Inst::Asm(assembly) => assembly.validate(&[]),
        Inst::AsmGoto {
            assembly, labels, ..
        } => assembly.validate(labels),
        Inst::Const(result, bits) => {
            expect_bits(&result.ty, "const result", block)?;
            if !integer_constant_fits(&result.ty, *bits) {
                return Err(format!(
                    "constant {bits} does not fit {:?} in block {block}",
                    result.ty
                ));
            }
            Ok(())
        }
        Inst::Uninitialized(result) => expect_data_type(&result.ty, "uninitialized result", block),
        Inst::Add(result, lhs, rhs)
        | Inst::Sub(result, lhs, rhs)
        | Inst::Mul(result, lhs, rhs)
        | Inst::SRem(result, lhs, rhs)
        | Inst::URem(result, lhs, rhs)
        | Inst::And(result, lhs, rhs)
        | Inst::Or(result, lhs, rhs)
        | Inst::Xor(result, lhs, rhs)
        | Inst::SDiv(result, lhs, rhs)
        | Inst::UDiv(result, lhs, rhs) => {
            expect_same_bits(result, lhs, rhs, "integer arithmetic", block)
        }
        Inst::Shl(result, shifted, amount)
        | Inst::AShr(result, shifted, amount)
        | Inst::LShr(result, shifted, amount) => {
            expect_shift_bits(result, shifted, amount, "shift", block)
        }
        Inst::SignedAddDefined(result, lhs, rhs)
        | Inst::SignedSubDefined(result, lhs, rhs)
        | Inst::SignedMulDefined(result, lhs, rhs) => {
            expect_binary_bits_predicate(result, lhs, rhs, "signed-definedness", block)
        }
        Inst::DivisorNonZero(result, divisor) => {
            expect_unary_bits_predicate(result, divisor, "nonzero-divisor", block)
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            expect_binary_bits_predicate(result, lhs, rhs, "signed-div-rem-overflow", block)
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        } => expect_shift_predicate(result, shifted, amount, "shift-count", block),
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            ..
        } => expect_shift_predicate(
            result,
            shifted,
            amount,
            "signed-left-shift-definedness",
            block,
        ),
        Inst::UbAssume(condition, _) => {
            if condition.ty == EvoIRType::Bits(1) {
                Ok(())
            } else {
                Err(format!(
                    "ub.assume in block {} uses {:?}, expected bits<1>",
                    block, condition.ty
                ))
            }
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            if result.ty != EvoIRType::Bits(1) {
                return Err(format!(
                    "icmp result in block {} has type {:?}, expected bits<1>",
                    block, result.ty
                ));
            }
            if lhs.ty != rhs.ty {
                return Err(format!(
                    "icmp operands in block {} have different types {:?} and {:?}",
                    block, lhs.ty, rhs.ty
                ));
            }
            match &lhs.ty {
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }
                    if matches!(operation, CmpOp::Eq | CmpOp::Ne) =>
                {
                    Ok(())
                }
                _ => expect_bits(&lhs.ty, "icmp operand", block),
            }
        }
        Inst::PointerCast { result, operand } => {
            let valid = (result.ty == EvoIRType::Bits(64)
                && matches!(operand.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }))
                || (operand.ty == EvoIRType::Bits(64)
                    && matches!(result.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }));
            if valid {
                Ok(())
            } else {
                Err(format!(
                    "invalid pointer-integer conversion in block {block}"
                ))
            }
        }
        Inst::PtrNull(result) => expect_pointer_value(&result.ty, "null-pointer result", block),
        Inst::GlobalAddr { result, .. } => match result.ty {
            EvoIRType::Ptr(AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "global-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        Inst::FuncAddr { result, .. } => match result.ty {
            EvoIRType::FnPtr { .. } => Ok(()),
            _ => Err(format!(
                "function-address result in block {block} has type {:?}, expected fnptr<Sig>",
                result.ty
            )),
        },
        // Section 3.7: "`%p = block_addr ^label : ptr<generic>`". The one
        // address space is the form's, not a choice the writer has.
        Inst::BlockAddr { result, .. } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "block-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        // Section 3.7 (revision 1.9.72): "`%p = return_address : ptr<generic>`"
        // and "`%p = frame_address : ptr<generic>`". Neither takes an operand,
        // and the one address space is the form's.
        Inst::ReturnAddress { result } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "return-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        Inst::FrameAddress { result, .. } => match result.ty {
            EvoIRType::Ptr(evocc_types::AddressSpace::Generic) => Ok(()),
            _ => Err(format!(
                "frame-address result in block {block} has type {:?}, expected ptr<generic>",
                result.ty
            )),
        },
        Inst::Alloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            ..
        } => {
            expect_pointer(&result.ty, "alloca result", block)?;
            validate_layout(*layout, "alloca", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "alloca", block)
        }
        Inst::DynamicAlloca {
            result,
            mem_in,
            world_in,
            mem_out,
            world_out,
            size,
            align,
            ..
        } => {
            expect_pointer(&result.ty, "alloca.dynamic result", block)?;
            if size.ty != EvoIRType::Bits(64) {
                return Err(format!(
                    "alloca.dynamic size in block {block} must be bits<64>"
                ));
            }
            validate_layout(AccessLayout::new(0, *align), "alloca.dynamic", block)?;
            expect_day7_effect_tokens(
                mem_in,
                world_in,
                mem_out,
                world_out,
                "alloca.dynamic",
                block,
            )
        }
        Inst::LifetimeEnd {
            mem_in,
            world_in,
            pointer,
            mem_out,
            world_out,
        } => {
            expect_pointer(&pointer.ty, "lifetime.end pointer", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "lifetime.end", block)
        }
        Inst::PointerValid {
            mem_in,
            pointer,
            mem_out,
        } => {
            expect_pointer(&pointer.ty, "pointer_valid pointer", block)?;
            expect_day7_memory_tokens(mem_in, mem_out, "pointer_valid", block)
        }
        Inst::AccessInBounds {
            mem_in,
            pointer,
            mem_out,
            layout,
        } => {
            expect_pointer(&pointer.ty, "access_in_bounds pointer", block)?;
            if layout.size_bytes == 0 {
                return Err(format!(
                    "access_in_bounds in block {block} has invalid size/alignment {layout:?}"
                ));
            }
            validate_layout(*layout, "access_in_bounds", block)?;
            expect_day7_memory_tokens(mem_in, mem_out, "access_in_bounds", block)
        }
        Inst::Load {
            result,
            address,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile: _,
            mask,
        } => {
            expect_pointer(&address.ty, "load address", block)?;
            expect_data_type(&result.ty, "load result", block)?;
            validate_access_mask(*mask, &result.ty, "load", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "load", block)
        }
        Inst::Store {
            address,
            value,
            mem_in,
            world_in,
            mem_out,
            world_out,
            volatile: _,
            mask,
        } => {
            expect_pointer(&address.ty, "store address", block)?;
            expect_data_type(&value.ty, "stored value", block)?;
            validate_access_mask(*mask, &value.ty, "store", block)?;
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "store", block)
        }
        Inst::Copy {
            destination,
            source,
            mem_in,
            world_in,
            mem_out,
            world_out,
            layout,
            volatile,
        } => {
            expect_pointer(&destination.ty, "copy destination", block)?;
            expect_pointer(&source.ty, "copy source", block)?;
            if layout.align_bytes == 0 || !layout.align_bytes.is_power_of_two() {
                return Err(format!(
                    "copy in block {block} has invalid alignment {}",
                    layout.align_bytes
                ));
            }
            if !volatile.supports(*layout) {
                return Err(format!("volatile copy in block {block} requires one naturally aligned 1/2/4/8-byte access"));
            }
            expect_day7_effect_tokens(mem_in, world_in, mem_out, world_out, "copy", block)
        }
        Inst::Call {
            results,
            args,
            mem_in,
            world_in,
            mem_out,
            world_out,
            ..
        } => {
            for argument in args {
                expect_data_type(&argument.ty, "ordinary call argument", block)?;
            }
            for result in results {
                expect_data_type(&result.ty, "call result", block)?;
            }
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "call", block)
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            arg_marks,
            mem_in,
            world_in,
            mem_out,
            world_out,
        } => {
            let EvoIRType::FnPtr {
                params,
                results: returns,
                variadic,
                // The marks ride inside the type, so the equality below already
                // compares them; nothing else here reads one.
                marks: _,
            } = &callee.ty
            else {
                return Err(format!(
                    "indirect call callee in block {block} has type {:?}, expected fnptr<Sig>",
                    callee.ty
                ));
            };
            let mut actual_params = args
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>();
            actual_params.push(mem_in.ty.clone());
            actual_params.push(world_in.ty.clone());
            let mut actual_returns = results
                .iter()
                .map(|value| value.ty.clone())
                .collect::<Vec<_>>();
            actual_returns.push(mem_out.ty.clone());
            actual_returns.push(world_out.ty.clone());
            let fixed = params.len().saturating_sub(2);
            variadic::validate_anonymous_marks(args, arg_marks, fixed)?;
            let parameters_match = if *variadic {
                args.len() >= fixed
                    && actual_params[..fixed] == params[..fixed]
                    && actual_params[actual_params.len() - 2..] == params[fixed..]
            } else {
                actual_params == *params
            };
            if !parameters_match || actual_returns != *returns {
                return Err(format!(
                    "E3017: indirect-call-signature-mismatch in block {block}"
                ));
            }
            if *variadic
                && args[fixed..]
                    .iter()
                    .any(|argument| !variadic::promoted_type(&argument.ty))
            {
                return Err("anonymous argument is not a promoted scalar type".into());
            }
            expect_effect_tokens(mem_in, world_in, mem_out, world_out, "indirect call", block)
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            scale,
            ..
        } => {
            expect_pointer(&result.ty, "pointer-offset result", block)?;
            expect_pointer(&base.ty, "pointer-offset base", block)?;
            if result.ty != base.ty {
                return Err(format!(
                    "pointer-offset result and base in block {} have different types {:?} and {:?}",
                    block, result.ty, base.ty
                ));
            }
            if *scale == 0 {
                return Err(format!(
                    "pointer-offset scale in block {block} must be positive"
                ));
            }
            expect_bits(&index.ty, "pointer-offset index", block)
        }
        Inst::SExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::SignExtend, block)
        }
        Inst::ZExt(result, source, target) => {
            validate_conversion(result, source, target, Conversion::ZeroExtend, block)
        }
        Inst::Trunc(result, source, target) => {
            validate_conversion(result, source, target, Conversion::Truncate, block)
        }
        Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::IndirectBr { .. }
        | Inst::Ret(..)
        | Inst::Trap(..) => Err(format!(
            "terminator found in instruction list of block {}",
            block
        )),
    }
}

pub(super) fn validate_layout(
    layout: AccessLayout,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if layout.size_bytes > i64::MAX as u64 || !layout.align_bytes.is_power_of_two() {
        return Err(format!(
            "{operation} in block {block} has invalid size/alignment {layout:?}"
        ));
    }
    Ok(())
}

/// A mask belongs to an access of `bits<8>` to `bits<128>`, volatile or
/// not. It selects at least one bit fewer than the access has: an access of
/// every bit is written without a mask (3.8).
pub(super) fn validate_access_mask(
    mask: Option<u128>,
    ty: &EvoIRType,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    let Some(mask) = mask else {
        return Ok(());
    };
    let EvoIRType::Bits(width @ (8 | 16 | 32 | 64 | 128)) = ty else {
        return Err(format!(
            "{operation} in block {block} has a mask but is not an integer access"
        ));
    };
    if mask >= u128::MAX >> (128 - u32::from(*width)) {
        return Err(format!(
            "{operation} in block {block} has mask {mask}, which is not narrower than bits<{width}>"
        ));
    }
    Ok(())
}

pub(super) fn integer_constant_fits(ty: &EvoIRType, bits: u64) -> bool {
    matches!(ty, EvoIRType::Bits(width) if *width != 0 && (*width >= 64 || bits < (1_u64 << width)))
}

pub(super) fn expect_day7_memory_tokens(
    mem_in: &Value,
    mem_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if mem_in.ty == EvoIRType::Mem(Region::All) && mem_out.ty == EvoIRType::Mem(Region::All) {
        Ok(())
    } else {
        Err(format!("{operation} in block {block} must thread mem<all>"))
    }
}

pub(super) fn expect_day7_effect_tokens(
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    expect_effect_tokens(mem_in, world_in, mem_out, world_out, operation, block)?;
    expect_day7_memory_tokens(mem_in, mem_out, operation, block)
}

pub(super) fn expect_same_bits(
    result: &Value,
    lhs: &Value,
    rhs: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != lhs.ty || lhs.ty != rhs.ty {
        return Err(format!(
            "{} in block {} has inconsistent types: result {:?}, lhs {:?}, rhs {:?}",
            operation, block, result.ty, lhs.ty, rhs.ty
        ));
    }
    expect_bits(&result.ty, operation, block)
}

pub(super) fn expect_shift_bits(
    result: &Value,
    shifted: &Value,
    amount: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != shifted.ty {
        return Err(format!(
            "{} in block {} has different result and shifted types {:?} and {:?}",
            operation, block, result.ty, shifted.ty
        ));
    }
    expect_bits(&shifted.ty, &format!("{operation} shifted operand"), block)?;
    expect_bits(&amount.ty, &format!("{operation} amount"), block)
}

pub(super) fn expect_unary_bits_predicate(
    result: &Value,
    operand: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    expect_bits(&operand.ty, &format!("{operation} operand"), block)
}

pub(super) fn expect_binary_bits_predicate(
    result: &Value,
    lhs: &Value,
    rhs: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    if lhs.ty != rhs.ty {
        return Err(format!(
            "{} operands in block {} have different types {:?} and {:?}",
            operation, block, lhs.ty, rhs.ty
        ));
    }
    expect_bits(&lhs.ty, &format!("{operation} operand"), block)
}

pub(super) fn expect_shift_predicate(
    result: &Value,
    shifted: &Value,
    amount: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    if result.ty != EvoIRType::Bits(1) {
        return Err(format!(
            "{} result in block {} has type {:?}, expected bits<1>",
            operation, block, result.ty
        ));
    }
    expect_bits(&shifted.ty, &format!("{operation} shifted operand"), block)?;
    expect_bits(&amount.ty, &format!("{operation} amount"), block)
}

pub(super) fn expect_bits(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    match ty {
        EvoIRType::Bits(width) if *width > 0 => Ok(()),
        _ => Err(format!(
            "{} in block {} has type {:?}, expected non-empty bits<N>",
            role, block, ty
        )),
    }
}

pub(super) fn expect_pointer(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    if matches!(ty, EvoIRType::Ptr(_)) {
        Ok(())
    } else {
        Err(format!(
            "{} in block {} has type {:?}, expected pointer",
            role, block, ty
        ))
    }
}

pub(super) fn expect_pointer_value(
    ty: &EvoIRType,
    role: &str,
    block: BlockId,
) -> Result<(), String> {
    if matches!(ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. }) {
        Ok(())
    } else {
        Err(format!(
            "{} in block {} has type {:?}, expected pointer or fnptr",
            role, block, ty
        ))
    }
}

pub(super) fn expect_data_type(ty: &EvoIRType, role: &str, block: BlockId) -> Result<(), String> {
    ensure_value_type(ty, &format!("{role} in block {block}"))?;
    if contains_effect_token(ty)? {
        Err(format!(
            "{} in block {} contains a memory/world effect token",
            role, block
        ))
    } else {
        Ok(())
    }
}

/// Validate the recursive shape of one EvoIR type without recursive descent.
///
/// `Void` and the empty tuple are well-formed type spellings, although
/// [`validate_value_type`] rejects both when a concrete SSA value is required.
pub fn validate_type_well_formed(ty: &EvoIRType) -> Result<(), String> {
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Bits(0) => return Err("zero-width bits type is invalid".to_owned()),
            EvoIRType::Vec(0, _) => return Err("zero-lane vector type is invalid".to_owned()),
            EvoIRType::Vec(_, element) => {
                let forbidden_element = matches!(
                    element.as_ref(),
                    EvoIRType::Void | EvoIRType::Mem(_) | EvoIRType::World
                ) || matches!(element.as_ref(), EvoIRType::Tuple(elements) if elements.is_empty());
                if forbidden_element {
                    return Err("vector element is not a data value type".to_owned());
                }
                pending.push((element, depth + 1));
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::FnPtr {
                params, results, ..
            } => {
                if params.len() < 2 || results.len() < 2 {
                    return Err("fnptr signature lacks trailing mem<R>, world interface".to_owned());
                }
                let parameter_split = params.len() - 2;
                let result_split = results.len() - 2;
                let region = match (&params[parameter_split], &params[parameter_split + 1]) {
                    (EvoIRType::Mem(region), EvoIRType::World) => *region,
                    _ => return Err("fnptr parameters must end with mem<R>, world".to_owned()),
                };
                if !matches!(
                    (&results[result_split], &results[result_split + 1]),
                    (EvoIRType::Mem(output), EvoIRType::World) if *output == region
                ) {
                    return Err("fnptr results must end with the same mem<R>, world".to_owned());
                }
                if params[..parameter_split]
                    .iter()
                    .chain(&results[..result_split])
                    .any(|ty| matches!(ty, EvoIRType::Void) || contains_effect_token_unchecked(ty))
                {
                    return Err(
                        "fnptr ordinary parameters/results cannot be effect tokens or void"
                            .to_owned(),
                    );
                }
                pending.extend(
                    params
                        .iter()
                        .chain(results)
                        .map(|element| (element, depth + 1)),
                );
            }
            _ => {}
        }
    }
    Ok(())
}

/// Validate a type at a concrete SSA value/interface position.
pub fn validate_value_type(ty: &EvoIRType) -> Result<(), String> {
    validate_type_well_formed(ty)?;
    let mut pending = vec![(ty, 0_usize)];
    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_TYPE_NESTING {
            return Err(format!(
                "type nesting exceeds the limit of {MAX_TYPE_NESTING}"
            ));
        }
        match current {
            EvoIRType::Void => return Err("void is not an SSA value type".to_owned()),
            EvoIRType::Tuple(elements) if elements.is_empty() => {
                return Err(
                    "the empty tuple is void-equivalent and not an SSA value type".to_owned(),
                );
            }
            EvoIRType::Tuple(elements) => {
                pending.extend(elements.iter().map(|element| (element, depth + 1)));
            }
            EvoIRType::Vec(_, element) => pending.push((element, depth + 1)),
            EvoIRType::FnPtr { .. } => {}
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn ensure_value_type(ty: &EvoIRType, role: &str) -> Result<(), String> {
    validate_value_type(ty).map_err(|message| format!("{role}: {message}"))
}

pub(super) fn contains_effect_token(ty: &EvoIRType) -> Result<bool, String> {
    validate_type_well_formed(ty)?;
    Ok(contains_effect_token_unchecked(ty))
}

// Function pointers are ordinary values. Their own trailing effect interface
// is checked separately, not mistaken for a token inside the outer value.
pub(super) fn contains_effect_token_unchecked(ty: &EvoIRType) -> bool {
    let mut pending = vec![ty];
    while let Some(current) = pending.pop() {
        match current {
            EvoIRType::Mem(_) | EvoIRType::World => return true,
            EvoIRType::Tuple(elements) => {
                pending.extend(elements);
            }
            EvoIRType::Vec(_, element) => pending.push(element),
            _ => {}
        }
    }
    false
}

pub(super) fn expect_effect_tokens(
    mem_in: &Value,
    world_in: &Value,
    mem_out: &Value,
    world_out: &Value,
    operation: &str,
    block: BlockId,
) -> Result<(), String> {
    let input_region = match mem_in.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} input memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_in.ty
            ));
        }
    };
    let output_region = match mem_out.ty {
        EvoIRType::Mem(region) => region,
        _ => {
            return Err(format!(
                "{} output memory token in block {} has type {:?}, expected mem<R>",
                operation, block, mem_out.ty
            ));
        }
    };
    if input_region != output_region {
        return Err(format!(
            "{} in block {} changes memory region from {:?} to {:?}",
            operation, block, input_region, output_region
        ));
    }
    for (direction, token) in [("input", world_in), ("output", world_out)] {
        if token.ty != EvoIRType::World {
            return Err(format!(
                "{} {} world token in block {} has type {:?}, expected world",
                operation, direction, block, token.ty
            ));
        }
    }
    Ok(())
}

pub(super) fn validate_conversion(
    result: &Value,
    source: &Value,
    target: &EvoIRType,
    conversion: Conversion,
    block: BlockId,
) -> Result<(), String> {
    if &result.ty != target {
        return Err(format!(
            "conversion result in block {} has type {:?}, but target is {:?}",
            block, result.ty, target
        ));
    }
    let EvoIRType::Bits(source_width) = source.ty else {
        return Err(format!(
            "conversion source in block {} has non-integer type {:?}",
            block, source.ty
        ));
    };
    let EvoIRType::Bits(target_width) = target else {
        return Err(format!(
            "conversion target in block {} has non-integer type {:?}",
            block, target
        ));
    };
    let widths_valid = match conversion {
        Conversion::SignExtend | Conversion::ZeroExtend => target_width >= &source_width,
        Conversion::Truncate => target_width <= &source_width,
    };
    if !widths_valid {
        return Err(format!(
            "invalid conversion width in block {}: bits<{}> to bits<{}>",
            block, source_width, target_width
        ));
    }
    Ok(())
}
