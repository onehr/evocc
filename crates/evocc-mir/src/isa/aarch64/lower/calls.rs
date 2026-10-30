use super::*;

/// The registers a call passes, in the positions Section 6 assigns them.
pub(crate) fn call_args(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    args: &[Value],
) -> Result<Vec<VReg>, String> {
    let ordinary: Vec<&Value> = args
        .iter()
        .filter(|value| is_register_type(&value.ty))
        .collect();
    let mut registers: Vec<VReg> = Vec::new();
    for value in ordinary {
        if is_pair(&value.ty) && registers.len() % 2 == 1 {
            // The skipped register is passed as zero rather than left
            // undefined, so that two lowerings of one unit agree byte for byte.
            let filler = lowerer.word(insts, 0);
            registers.push(filler);
        }
        registers.extend(lowerer.registers(value)?);
    }
    Ok(registers)
}

/// Section 6.3: a half value crosses a call in the `h` view of a vector
/// register. Its `gpr32` representation moves to a `simd32` transport
/// register, whose bits above the half are unspecified, as AAPCS64 C.1
/// permits.
pub(crate) fn half_to_transport(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: VReg,
) -> VReg {
    let transport = lowerer.fresh(RegClass::Simd32);
    insts.push(MInst::Floating {
        strict: false,
        integer_bits: 0,
        dst: transport,
        op: evocc_evoir::FloatingOp::FromBits,
        kind: evocc_types::FloatKind::F32,
        args: vec![value],
    });
    transport
}

/// The `gpr32` representation of a half value that arrived in a transport
/// register; its bits above the half stay unspecified.
pub(crate) fn half_from_transport(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    transport: VReg,
) -> VReg {
    let value = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::Floating {
        strict: false,
        integer_bits: 0,
        dst: value,
        op: evocc_evoir::FloatingOp::ToBits,
        kind: evocc_types::FloatKind::F32,
        args: vec![transport],
    });
    value
}

/// Section 6.3: a call reads a half result from the transport register.
pub(crate) fn half_call_results(
    lowerer: &mut Lowerer,
    outputs: &mut [VReg],
    results: &[Value],
) -> Vec<MInst> {
    let mut after = Vec::new();
    let mut slot = 0;
    for value in results.iter().filter(|value| is_register_type(&value.ty)) {
        if is_pair(&value.ty) {
            slot += 2;
            continue;
        }
        if evocc_evoir::is_half_type(&value.ty) {
            let transport = lowerer.fresh(RegClass::Simd32);
            outputs[slot] = transport;
            let half = half_from_transport(lowerer, &mut after, transport);
            lowerer.bind(value, half);
        }
        slot += 1;
    }
    after
}

/// Section 6.2: under the Apple convention a call reads a tiny-vector result
/// from `v0` and narrows its widened lanes after the call.
pub(crate) fn apple_call_results(
    lowerer: &mut Lowerer,
    outputs: &mut [VReg],
    results: &[Value],
) -> Vec<MInst> {
    let mut after = Vec::new();
    if !lowerer.anonymous_on_stack {
        return after;
    }
    let mut slot = 0;
    for value in results.iter().filter(|value| is_register_type(&value.ty)) {
        if is_pair(&value.ty) {
            slot += 2;
            continue;
        }
        if let Some(shape) = tiny_vector_shape(&value.ty) {
            let wide = lowerer.fresh(RegClass::Simd64);
            outputs[slot] = wide;
            let narrow = narrow_apple_result(lowerer, &mut after, wide, shape);
            lowerer.bind(value, narrow);
        }
        slot += 1;
    }
    after
}

pub(crate) fn call_results(lowerer: &mut Lowerer, results: &[Value]) -> Result<Vec<VReg>, String> {
    let mut outputs = Vec::new();
    for value in results.iter().filter(|value| is_register_type(&value.ty)) {
        if is_pair(&value.ty) {
            // Section 6: "A `bits<128>` result is `x0`:`x1`, low half in
            // `x0`."
            let (low, high) = lowerer.fresh_pair();
            lowerer.bind_pair(value, low, high);
            outputs.push(low);
            outputs.push(high);
            continue;
        }
        let class = class_of(&value.ty).map_err(|(code, _)| code.to_owned())?;
        let reg = lowerer.fresh(class);
        lowerer.bind(value, reg);
        outputs.push(reg);
    }
    if outputs.len() > lowerer.abi.general_results() {
        return Err("a call with more than eight ordinary results".to_owned());
    }
    Ok(outputs)
}

/// Read one named argument from the Apple named stack area at `offset` from
/// `base`, the incoming stack address, with the access of its natural size
/// (Section 6.6). A record is copied into a frame slot, as when it arrives
/// in registers.
pub(crate) fn read_apple_stack_argument(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: &Value,
    mark: &ParamMark,
    passing: &Passing,
    base: VReg,
    offset: u64,
) -> Result<(), String> {
    let (pieces, _) = apple_pieces(&value.ty, mark, passing)
        .ok_or_else(|| format!("v{} has no Apple stack placement", value.id))?;
    // An offset that does not fit is an error, never offset 0.
    let beyond = || format!("v{} lies beyond the incoming stack area", value.id);
    let at = |piece: &ApplePiece| -> Result<i64, String> {
        offset
            .checked_add(piece.offset)
            .and_then(|offset| i64::try_from(offset).ok())
            .ok_or_else(beyond)
    };
    let slot = |lowerer: &mut Lowerer, insts: &mut Vec<MInst>, shape: RecordShape, align| {
        let slot = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::FrameSlot {
            dst: slot,
            origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
            size: shape.size,
            align,
        });
        lowerer.record_origins += 1;
        slot
    };
    match passing {
        Passing::Value if is_pair(&value.ty) => {
            let (low, high) = lowerer.fresh_pair();
            for (dst, piece) in [low, high].into_iter().zip(&pieces) {
                insts.push(MInst::Load {
                    align: 8,
                    dst,
                    base,
                    offset: at(piece)?,
                    kind: LoadKind::X,
                    volatile: false,
                    mask: None,
                    transport: false,
                });
            }
            lowerer.bind_pair(value, low, high);
        }
        Passing::Value | Passing::Pointer => {
            let piece = pieces[0];
            let dst = lowerer.fresh(piece.class);
            insts.push(MInst::Load {
                align: piece.size as u32,
                dst,
                base,
                offset: at(&piece)?,
                kind: if piece.class.is_simd() {
                    simd_load(piece.size)
                } else {
                    piece_load(piece.size)
                },
                volatile: false,
                mask: None,
                transport: false,
            });
            lowerer.bind(value, dst);
        }
        Passing::Chunks(chunks) => {
            let shape = mark
                .shape()
                .ok_or("a chunked argument is a marked record")?;
            let align = lowerer.record_slot_alignment(shape);
            let destination = slot(lowerer, insts, shape, align);
            // The incoming area starts at `x29 + 16`, which the ABI keeps
            // aligned to the stack alignment.
            let incoming = lowerer.abi.facts.stack_alignment_bytes;
            let mut within = 0;
            for ((chunk, size), piece) in chunks.iter().enumerate().zip(&pieces) {
                // `load_chunk` reads up to eight bytes from its offset, so the
                // whole chunk must lie inside the addressable range.
                let at = at(piece)?;
                at.checked_add(8).ok_or_else(beyond)?;
                debug_assert_eq!(piece.offset, 8 * chunk as u64);
                let word = load_chunk(lowerer, insts, base, incoming, at as u64, *size);
                let base_align = u64::from(align);
                store_chunk(lowerer, insts, word, destination, base_align, within, *size);
                within += size;
            }
            lowerer.bind(value, destination);
        }
        Passing::Vectors { shape, .. } => {
            let destination = slot(lowerer, insts, *shape, shape.align);
            for piece in &pieces {
                let member = lowerer.fresh(piece.class);
                insts.push(MInst::Load {
                    align: shape.align.min(piece.size as u32),
                    dst: member,
                    base,
                    offset: at(piece)?,
                    kind: simd_load(piece.size),
                    volatile: false,
                    mask: None,
                    transport: false,
                });
                insts.push(MInst::Store {
                    align: shape.align.min(piece.size as u32),
                    src: member,
                    base: destination,
                    offset: i64::try_from(piece.offset).map_err(|_| beyond())?,
                    kind: simd_store(piece.size),
                    volatile: false,
                    mask: None,
                    transport: false,
                });
            }
            lowerer.bind(value, destination);
        }
        Passing::Destination(_) => return Err("a record destination is never on the stack".into()),
    }
    Ok(())
}

/// The value of one Apple stack piece, zero-extended into a general
/// register of 64 bits.
pub(crate) fn apple_word_piece(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    reg: VReg,
    piece: ApplePiece,
) -> VReg {
    let general = if reg.class.is_simd() {
        // `fmov wN, sN`: the bits of a 32-bit floating piece.
        let bits = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::Floating {
            strict: false,
            integer_bits: 0,
            dst: bits,
            op: evocc_evoir::FloatingOp::ToBits,
            kind: evocc_types::FloatKind::F32,
            args: vec![reg],
        });
        bits
    } else {
        reg
    };
    let extended = lowerer.zero_extend(insts, general, (piece.size * 8) as u16, Width::X);
    if extended.class == RegClass::Gpr64 {
        extended
    } else {
        let dst = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::Mov { dst, src: extended });
        dst
    }
}

/// The caller's side of the Apple named stack area: the zero fillers that
/// exhaust the banks used by its words, then the area, one eight-byte word (or one
/// sixteen-byte vector) per element, so that each element takes the next
/// stack slot (Section 6.6).
pub(crate) fn push_apple_stack_words(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    registers: &mut Vec<VReg>,
    plan: &RecordPlan,
    arguments: &[(EvoIRType, ParamMark)],
    pieces: &HashMap<usize, Vec<VReg>>,
) {
    let words = apple_layout(&plan.items, arguments, plan.apple_stack_bytes);
    if words.is_empty() {
        return;
    }
    for _ in 0..plan.apple_fill.0 {
        let filler = lowerer.word(insts, 0);
        registers.push(filler);
    }
    for _ in 0..plan.apple_fill.1 {
        let filler = lowerer.fresh(RegClass::Simd64);
        insts.push(MInst::MovImm {
            dst: filler,
            imm: 0,
        });
        registers.push(filler);
    }
    let piece_reg = |index: usize, piece: &ApplePiece| -> VReg {
        let (all, _) = apple_pieces(
            &arguments[index].0,
            &arguments[index].1,
            &plan.items[index].passing,
        )
        .expect("a placed argument has pieces");
        let position = all
            .iter()
            .position(|candidate| candidate == piece)
            .expect("the piece belongs to its argument");
        pieces[&index][position]
    };
    for word in words {
        match word {
            AppleWord::Whole(index, piece) => registers.push(piece_reg(index, &piece)),
            AppleWord::Composed(inside) => {
                let mut accumulated: Option<VReg> = None;
                for (index, piece, within) in inside {
                    let reg = piece_reg(index, &piece);
                    let value = apple_word_piece(lowerer, insts, reg, piece);
                    let placed = if within == 0 {
                        value
                    } else {
                        lowerer.word_binary_imm(insts, BinOp::Lsl, value, within * 8)
                    };
                    accumulated = Some(match accumulated {
                        None => placed,
                        Some(low) => lowerer.word_binary(insts, BinOp::Orr, low, placed),
                    });
                }
                let word = accumulated.unwrap_or_else(|| lowerer.word(insts, 0));
                registers.push(word);
            }
        }
    }
}

/// The registers a call passes, with the record convention applied.
///
/// Returns the argument registers and, when the callee writes its result
/// through a destination of at most sixteen bytes, that destination's shape and
/// pointer: the caller passes no argument for it and stores the result into it
/// when the call returns.
pub(crate) fn call_arguments(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    args: &[Value],
    marks: &[ParamMark],
    plan: &RecordPlan,
    extend: bool,
) -> Result<CallArguments, String> {
    let marked = marked_types(&arg_types(args), marks);
    let mut registers: Vec<VReg> = Vec::new();
    let mut returned = None;
    let mut indirect_destination = None;
    // The Apple convention: the registers that hold each named stack
    // argument's pieces, until the words of the area are composed.
    let mut stacked: HashMap<usize, Vec<VReg>> = HashMap::new();
    let mut flushed = false;
    // The narrow arguments to extend, as (register index, bits, signed).
    let mut narrow: Vec<(usize, u16, bool)> = Vec::new();
    let ordinary = args.iter().filter(|value| is_register_type(&value.ty));
    for (position, (value, placed)) in ordinary.zip(&plan.items).enumerate() {
        // The Apple variadic convention: zero the argument registers that the
        // named arguments leave, so that the anonymous arguments go to the
        // stack. The callee reads none of these registers.
        if let Some(start) = plan.anonymous.filter(|start| start.item == position) {
            push_apple_stack_words(lowerer, insts, &mut registers, plan, &marked, &stacked);
            flushed = true;
            for _ in 0..start.general {
                let filler = lowerer.word(insts, 0);
                registers.push(filler);
            }
            for _ in 0..start.vector {
                let filler = lowerer.fresh(RegClass::Simd64);
                insts.push(MInst::MovImm {
                    dst: filler,
                    imm: 0,
                });
                registers.push(filler);
            }
        }
        if placed.apple_stack.is_some() {
            let pieces = match &placed.passing {
                Passing::Value => lowerer.registers(value)?,
                Passing::Pointer => vec![lowerer.reg(value)?],
                Passing::Chunks(chunks) => {
                    let base = lowerer.reg(value)?;
                    let base_align = copy_alignment(&marked, position);
                    let mut offset = 0;
                    let mut words = Vec::new();
                    for size in chunks {
                        words.push(load_chunk(lowerer, insts, base, base_align, offset, *size));
                        offset += size;
                    }
                    words
                }
                Passing::Vectors { shape, .. } => {
                    let base = lowerer.reg(value)?;
                    let (count, size) = shape.simd_members().ok_or("an HFA or HVA has members")?;
                    let mut members = Vec::new();
                    for member in 0..u64::from(count) {
                        let reg = lowerer.fresh(simd_class(size));
                        let offset = member
                            .checked_mul(size)
                            .and_then(|offset| i64::try_from(offset).ok())
                            .ok_or("an HFA or HVA member lies beyond its record")?;
                        insts.push(MInst::Load {
                            align: shape.align.min(size as u32),
                            dst: reg,
                            base,
                            offset,
                            kind: simd_load(size),
                            volatile: false,
                            mask: None,
                            transport: false,
                        });
                        members.push(reg);
                    }
                    members
                }
                Passing::Destination(_) => {
                    return Err("a record destination is never on the stack".into())
                }
            };
            stacked.insert(position, pieces);
            continue;
        }
        for _ in 0..placed.fillers {
            // The skipped register is passed as zero rather than left
            // undefined, so that two lowerings of one unit agree byte for byte.
            let filler = lowerer.word(insts, 0);
            registers.push(filler);
        }
        match &placed.passing {
            // Section 6: "loaded from the copy into the next one or two
            // general registers as 8-byte chunks".
            Passing::Chunks(chunks) => {
                let base = lowerer.reg(value)?;
                let base_align = copy_alignment(&marked, position);
                let mut offset = 0;
                for size in chunks {
                    let chunk = load_chunk(lowerer, insts, base, base_align, offset, *size);
                    registers.push(chunk);
                    offset += size;
                }
            }
            Passing::Vectors {
                shape,
                skipped,
                stack,
            } => {
                for _ in 0..*skipped {
                    let filler = lowerer.fresh(RegClass::Simd64);
                    insts.push(MInst::MovImm {
                        dst: filler,
                        imm: 0,
                    });
                    registers.push(filler);
                }
                let base = lowerer.reg(value)?;
                for (offset, size) in simd_argument_pieces(*shape, *stack) {
                    let chunk = lowerer.fresh(simd_class(size));
                    insts.push(MInst::Load {
                        align: shape.align.min(size as u32),
                        dst: chunk,
                        base,
                        offset: offset as i64,
                        kind: simd_load(size),
                        volatile: false,
                        mask: None,
                        transport: false,
                    });
                    registers.push(chunk);
                }
            }
            // Above sixteen bytes the pointer to the caller's copy is the
            // argument, "which is what AAPCS64 does too".
            Passing::Pointer => registers.push(lowerer.reg(value)?),
            Passing::Destination(shape) => {
                if shape.size > lowerer.abi.record_register_bytes && shape.simd_members().is_none()
                {
                    // The IR's fixed sret slot can precede anonymous
                    // arguments. Keep it out of their ordinary placement;
                    // the machine call's final slot always names x8.
                    indirect_destination = Some(lowerer.reg(value)?);
                } else {
                    returned = Some((*shape, lowerer.reg(value)?));
                }
            }
            Passing::Value => {
                if let EvoIRType::Bits(bits) = value.ty {
                    if extend && bits < 32 {
                        let signed = marked.get(position).map(|(_, mark)| mark)
                            == Some(&ParamMark::SignExtend);
                        narrow.push((registers.len(), bits, signed));
                    }
                }
                if evocc_evoir::is_half_type(&value.ty) {
                    let half = lowerer.reg(value)?;
                    registers.push(half_to_transport(lowerer, insts, half));
                    continue;
                }
                registers.extend(lowerer.registers(value)?);
            }
        }
    }
    if !flushed {
        push_apple_stack_words(lowerer, insts, &mut registers, plan, &marked, &stacked);
    }
    // Section 6.2: a caller that crosses the compiler boundary extends an
    // argument narrower than 32 bits to 32 bits: by its sign when the
    // signature marks it `sext`, otherwise by zeros.
    for (slot, bits, signed) in narrow {
        registers[slot] = if signed {
            lowerer.sign_extend(insts, registers[slot], bits, Width::W)
        } else {
            lowerer.zero_extend(insts, registers[slot], bits, Width::W)
        };
    }
    if let Some(destination) = indirect_destination {
        registers.push(destination);
    }
    Ok((registers, returned))
}

/// Section 6: a record result of at most sixteen bytes comes back in `x0` and
/// `x1`, and "stored by the caller into its destination".
pub(crate) fn call_record_result(
    lowerer: &mut Lowerer,
    outputs: &mut Vec<VReg>,
    returned: Option<(RecordShape, VReg)>,
) -> Vec<MInst> {
    let Some((shape, destination)) = returned else {
        return Vec::new();
    };
    let mut after = Vec::new();
    if let Some((count, size)) = shape.simd_members() {
        for member in 0..count {
            let chunk = lowerer.fresh(simd_class(size));
            outputs.push(chunk);
            after.push(MInst::Store {
                align: shape.align.min(size as u32),
                src: chunk,
                base: destination,
                offset: i64::from(member) * size as i64,
                kind: simd_store(size),
                volatile: false,
                mask: None,
                transport: false,
            });
        }
        return after;
    }
    // The destination is the caller's copy, at the record's alignment.
    let base_align = u64::from(shape.align);
    let mut offset = 0;
    for size in record_chunks(shape.size) {
        let chunk = lowerer.fresh(RegClass::Gpr64);
        outputs.push(chunk);
        store_chunk(
            lowerer,
            &mut after,
            chunk,
            destination,
            base_align,
            offset,
            size,
        );
        offset += size;
    }
    after
}

/// The alignment of the caller's copy of the by-value record at `position`:
/// the alignment of its `byval` mark (EvoIR Section 10.1.3), one byte for an
/// argument without a record mark.
pub(crate) fn copy_alignment(marked: &[(EvoIRType, ParamMark)], position: usize) -> u64 {
    marked
        .get(position)
        .and_then(|(_, mark)| mark.shape())
        .map_or(1, |shape| u64::from(shape.align))
}

/// The registers a call passes and, when it has one, the destination a record
/// result of at most sixteen bytes is stored into when the call returns.
pub(crate) type CallArguments = (Vec<VReg>, Option<(RecordShape, VReg)>);
