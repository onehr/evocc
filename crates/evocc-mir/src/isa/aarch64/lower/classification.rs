use super::*;

/// What the convention does with one argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Passing {
    /// An ordinary value: one register, or two for a `bits<128>`.
    Value,
    /// The eight-byte chunks a `byval` record of at most sixteen bytes travels
    /// in, each entry that chunk's own byte count.
    Chunks(Vec<u64>),
    Vectors {
        shape: RecordShape,
        skipped: usize,
        stack: bool,
    },
    /// A `byval` record above sixteen bytes: a pointer to the copy.
    Pointer,
    /// The destination of a record result. It takes no argument position: at
    /// most sixteen bytes the result comes back in `x0`/`x1`, and above that
    /// the pointer travels in `x8`.
    Destination(RecordShape),
}

/// One argument's placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Placed {
    /// Positions skipped before this argument: Section 6's even-register
    /// rounding, and the rule that sends a whole record to the stack rather
    /// than splitting it across the last register and the stack.
    pub(crate) fillers: usize,
    pub(crate) passing: Passing,
    /// The Apple convention: the byte offset of a named argument that finds
    /// no register, in the named stack area that the caller packs by natural
    /// size and alignment (Section 6.6).
    pub(crate) apple_stack: Option<u64>,
}

/// The largest named stack area that the Apple convention places, in bytes.
/// A larger one is refused (`E4010`); below it, every offset and slot fits.
pub(crate) const APPLE_STACK_AREA_LIMIT: u64 = 1 << 20;

/// One piece of a named stack argument under the Apple convention: its byte
/// offset inside the argument, its size and the class that holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ApplePiece {
    pub(crate) offset: u64,
    pub(crate) size: u64,
    pub(crate) class: RegClass,
}

/// Section 6 applied to one argument list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecordPlan {
    pub(crate) items: Vec<Placed>,
    /// Positions consumed, fillers included. A position at or above eight is a
    /// stack slot, which Darwin refuses (`E4010`). Under the Apple variadic
    /// convention only the named arguments count.
    pub(crate) positions: usize,
    /// Whether the list ends with a destination pointer that goes to `x8`.
    pub(crate) indirect_result: bool,
    /// The Apple variadic convention: the first anonymous argument and the
    /// registers that stay unused before it.
    pub(crate) anonymous: Option<AnonymousStart>,
    /// The Apple convention: the bytes of the named stack area, before the
    /// final rounding to eight.
    pub(crate) apple_stack_bytes: u64,
    /// The Apple convention: the free general and vector registers that zero
    /// fillers take before the named stack words, so that each word finds a
    /// stack slot.
    pub(crate) apple_fill: (usize, usize),
    /// A named argument that the Apple convention cannot place (`E4010`).
    pub(crate) refused: Option<String>,
}

/// Where the anonymous arguments of a variadic call begin when they all go to
/// the stack. Zero fillers take the `general` and `vector` argument registers
/// that the named arguments leave, so each anonymous piece finds a stack slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AnonymousStart {
    pub(crate) item: usize,
    pub(crate) general: usize,
    pub(crate) vector: usize,
}

/// The ordinary position of the first anonymous argument of a call to a
/// callee with this signature, when the callee is variadic.
pub(crate) fn anonymous_start(variadic: bool, params: &[EvoIRType]) -> Option<usize> {
    variadic.then(|| params.iter().filter(|ty| is_register_type(ty)).count())
}

/// Whether an argument travels in vector registers, not general registers.
pub(crate) fn uses_vector_bank(ty: &EvoIRType, mark: &ParamMark) -> bool {
    match mark {
        ParamMark::ByVal(shape) => shape.simd_members().is_some(),
        ParamMark::None | ParamMark::SignExtend => simd_argument_bytes(ty).is_some(),
        ParamMark::Sret(_) => false,
    }
}

/// The pieces and the alignment of one named stack argument under the Apple
/// convention (Section 6.6). A scalar takes its natural size and alignment.
/// A record of at most sixteen bytes takes its eight-byte chunks, aligned to
/// eight or, for a record aligned to sixteen, to sixteen. An HFA or HVA takes
/// its members contiguously, aligned to the record's alignment up to sixteen.
/// A larger record is the pointer to the caller's copy.
pub(crate) fn apple_pieces(
    ty: &EvoIRType,
    mark: &ParamMark,
    passing: &Passing,
) -> Option<(Vec<ApplePiece>, u64)> {
    let piece = |offset, size, class| ApplePiece {
        offset,
        size,
        class,
    };
    match passing {
        Passing::Value if is_pair(ty) => Some((
            vec![piece(0, 8, RegClass::Gpr64), piece(8, 8, RegClass::Gpr64)],
            16,
        )),
        // A half value or a binary16 HFA has no Apple stack placement in this
        // revision (Section 6.3).
        Passing::Value if evocc_evoir::is_half_type(ty) => None,
        Passing::Vectors { shape, .. } if half_aggregate(*shape) => None,
        Passing::Value => {
            if let Some(bytes) = simd_argument_bytes(ty) {
                if bytes == 16 && !matches!(ty, EvoIRType::Vec { .. }) {
                    // binary128 has no Darwin placement.
                    return None;
                }
                return Some((vec![piece(0, bytes, simd_class(bytes))], bytes));
            }
            let class = class_of(ty).ok()?;
            let bytes = match ty {
                EvoIRType::Bits(width) if *width <= 64 => {
                    u64::from(width.div_ceil(8)).next_power_of_two()
                }
                EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => 8,
                // Apple clang passes a tiny vector as its 4-byte integer
                // image, so it takes a 4-byte slot (Section 6.6).
                EvoIRType::Vec { .. } if tiny_vector_shape(ty).is_some() => 4,
                _ => return None,
            };
            Some((vec![piece(0, bytes, class)], bytes))
        }
        Passing::Chunks(chunks) => Some((
            (0..chunks.len() as u64)
                .map(|chunk| piece(chunk * 8, 8, RegClass::Gpr64))
                .collect(),
            if mark.shape().is_some_and(|shape| shape.align >= 16) {
                16
            } else {
                8
            },
        )),
        Passing::Pointer => Some((vec![piece(0, 8, RegClass::Gpr64)], 8)),
        Passing::Vectors { shape, .. } => {
            let (count, size) = shape.simd_members()?;
            Some((
                (0..u64::from(count))
                    .map(|member| piece(member * size, size, simd_class(size)))
                    .collect(),
                u64::from(shape.align).clamp(size.min(16), 16),
            ))
        }
        Passing::Destination(_) => None,
    }
}

/// Where each ordinary argument of a signature goes.
///
/// `pair_rounding` is the `bits<128>` rule as the two call sites already apply
/// it: the entry block rounds only there, because "inside the function the rule
/// has no meaning", and a call site always does. The ABI table's
/// `aligned-record-pair-rounding` fact decides it for a `bits<128>` as for a
/// record aligned to sixteen: the Darwin platform rounds neither (Section 6.5).
///
/// `apple` selects the Apple convention (`CodegenOptions::anonymous_on_stack`):
/// a named argument that finds no register goes to the named stack area at
/// its natural size and alignment (Section 6.6).
pub(crate) fn record_plan(
    target: Target,
    arguments: &[(EvoIRType, ParamMark)],
    pair_rounding: bool,
    anonymous_from: Option<usize>,
    apple: bool,
) -> RecordPlan {
    let abi = target.aarch64_abi();
    let general = abi.general_arguments();
    let vectors = abi.vector_arguments();
    let mut items = Vec::new();
    let mut next = 0_usize;
    let mut next_vector = 0_usize;
    let mut cursor = abi.argument_cursor();
    let mut indirect_result = false;
    let mut anonymous = None;
    let mut named_positions = None;
    // The Apple convention: the named stack area and its words.
    let mut apple_stack_bytes = 0_u64;
    let mut apple_fill = (0, 0);
    let mut apple_flushed = false;
    let mut refused = None;
    let flush = |cursor: &mut crate::generic::abi::ArgumentCursor<'_>,
                 words: &[RegClass],
                 fill: &mut (usize, usize),
                 flushed: &mut bool| {
        if *flushed {
            return;
        }
        *flushed = true;
        if words.is_empty() {
            return;
        }
        // Only a bank used by the packed words must be exhausted to place
        // those words on the stack. Filling the other bank creates a false
        // dependency on SIMD for integer-only calls. Anonymous arguments
        // later exhaust their own banks from this updated cursor.
        fill.0 = if words.iter().any(|class| !class.is_simd()) {
            general.saturating_sub(cursor.used(general_bank()))
        } else {
            0
        };
        fill.1 = if words.iter().any(|class| class.is_simd()) {
            vectors.saturating_sub(cursor.used(vector_bank()))
        } else {
            0
        };
        for _ in 0..fill.0 {
            let _ = place(cursor, RegClass::Gpr64);
        }
        for _ in 0..fill.1 {
            let _ = place(cursor, RegClass::Simd64);
        }
        for class in words {
            let _ = place(cursor, *class);
        }
    };
    for (position, (ty, mark)) in arguments.iter().enumerate() {
        let named = anonymous_from.is_none_or(|start| position < start);
        // The Apple convention: decide each named argument's bank or the
        // named stack area, as AAPCS64 decides it, and keep the register
        // pieces in order. A bank that a named argument does not fit stays
        // exhausted for the later named arguments.
        if apple && named {
            let passing = match mark {
                ParamMark::Sret(shape) => {
                    if shape.size > abi.record_register_bytes && shape.simd_members().is_none() {
                        indirect_result = true;
                    }
                    items.push(Placed {
                        fillers: 0,
                        passing: Passing::Destination(*shape),
                        apple_stack: None,
                    });
                    continue;
                }
                ParamMark::ByVal(shape) if shape.simd_members().is_some() => {
                    let count = usize::from(shape.simd_members().unwrap().0);
                    let stack = next_vector + count > vectors;
                    Passing::Vectors {
                        shape: *shape,
                        skipped: 0,
                        stack,
                    }
                }
                ParamMark::ByVal(shape) if shape.size <= abi.record_register_bytes => {
                    Passing::Chunks(record_chunks(shape.size))
                }
                ParamMark::ByVal(_) => Passing::Pointer,
                ParamMark::None | ParamMark::SignExtend => Passing::Value,
            };
            let vector = matches!(passing, Passing::Vectors { .. })
                || (matches!(passing, Passing::Value) && simd_argument_bytes(ty).is_some());
            let (count, classes): (usize, Vec<RegClass>) = match &passing {
                Passing::Value if is_pair(ty) => (2, vec![RegClass::Gpr64; 2]),
                Passing::Value => (1, vec![class_of(ty).unwrap_or(RegClass::Gpr64)]),
                Passing::Chunks(chunks) => (chunks.len(), vec![RegClass::Gpr64; chunks.len()]),
                Passing::Pointer => (1, vec![RegClass::Gpr64]),
                Passing::Vectors { shape, .. } => {
                    let (members, size) = shape.simd_members().unwrap();
                    (
                        usize::from(members),
                        vec![simd_class(size); usize::from(members)],
                    )
                }
                Passing::Destination(_) => unreachable!("handled above"),
            };
            let fits = if vector {
                next_vector + count <= vectors
            } else {
                next + count <= general
            };
            if fits {
                if vector {
                    next_vector += count;
                } else {
                    next += count;
                }
                for class in classes {
                    let _ = place(&mut cursor, class);
                }
                items.push(Placed {
                    fillers: 0,
                    passing,
                    apple_stack: None,
                });
                continue;
            }
            // AAPCS64: an argument that does not fit takes no register, and
            // its bank stays exhausted for the later named arguments.
            if vector {
                next_vector = vectors;
            } else {
                next = general;
            }
            let Some((pieces, align)) = apple_pieces(ty, mark, &passing) else {
                refused.get_or_insert_with(|| {
                    format!("argument {position} of type {ty:?} has no Apple stack placement")
                });
                items.push(Placed {
                    fillers: 0,
                    passing,
                    apple_stack: None,
                });
                continue;
            };
            let size = pieces.iter().map(|piece| piece.size).sum::<u64>();
            // The area stays far below the range of a slot number, so that
            // every later offset, word and slot of it fits.
            let Some((offset, end)) = apple_stack_bytes
                .checked_next_multiple_of(align)
                .and_then(|offset| Some((offset, offset.checked_add(size)?)))
                .filter(|(_, end)| *end <= APPLE_STACK_AREA_LIMIT)
            else {
                refused.get_or_insert_with(|| {
                    format!("argument {position} ends beyond the named stack area")
                });
                items.push(Placed {
                    fillers: 0,
                    passing,
                    apple_stack: None,
                });
                continue;
            };
            apple_stack_bytes = end;
            items.push(Placed {
                fillers: 0,
                passing,
                apple_stack: Some(offset),
            });
            continue;
        }
        // The Apple variadic convention: the named arguments keep their
        // registers, and the anonymous ones start on the stack. A bank that
        // no anonymous argument uses keeps its free registers.
        if anonymous_from == Some(position) {
            if apple {
                let words = apple_word_classes(&items, arguments, apple_stack_bytes);
                flush(&mut cursor, &words, &mut apple_fill, &mut apple_flushed);
            }
            let rest = &arguments[position..];
            let fill_general = rest
                .iter()
                .any(|(ty, mark)| !uses_vector_bank(ty, mark) && !mark.is_sret());
            let fill_vector = rest.iter().any(|(ty, mark)| uses_vector_bank(ty, mark));
            let general_left = if fill_general {
                general.saturating_sub(cursor.used(general_bank()))
            } else {
                0
            };
            let vector_left = if fill_vector {
                vectors.saturating_sub(cursor.used(vector_bank()))
            } else {
                0
            };
            for _ in 0..general_left {
                let _ = place(&mut cursor, RegClass::Gpr64);
            }
            for _ in 0..vector_left {
                let _ = place(&mut cursor, RegClass::Simd64);
            }
            named_positions = Some(next);
            if fill_general {
                next = next.max(general);
            }
            if fill_vector {
                next_vector = next_vector.max(vectors);
            }
            anonymous = Some(AnonymousStart {
                item: position,
                general: general_left,
                vector: vector_left,
            });
        }
        match mark {
            ParamMark::Sret(shape) => {
                if shape.size > abi.record_register_bytes && shape.simd_members().is_none() {
                    indirect_result = true;
                }
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Destination(*shape),
                    apple_stack: None,
                });
            }
            ParamMark::ByVal(shape) if shape.simd_members().is_some() => {
                let count = usize::from(shape.simd_members().unwrap().0);
                let stack = next_vector + count > vectors;
                // Section 6.3: a binary16 HFA has no stack placement in this
                // revision.
                if stack && half_aggregate(*shape) {
                    refused.get_or_insert_with(|| {
                        format!("argument {position}, a binary16 HFA, has no stack placement")
                    });
                }
                let mut skipped = if next_vector < vectors && next_vector + count > vectors {
                    vectors - next_vector
                } else {
                    0
                };
                // Once SIMD registers are exhausted, one additional private
                // filler aligns the shared stack cursor for an HFA whose
                // members require 16-byte alignment. Object over-alignment
                // alone must not request this padding.
                if stack
                    && shape.floating.is_some_and(|f| f.abi_align == 16)
                    && !cursor.stack_bytes(abi).is_multiple_of(16)
                {
                    skipped += 1;
                }
                next_vector += skipped + count;
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Vectors {
                        shape: *shape,
                        skipped,
                        stack,
                    },
                    apple_stack: None,
                });
            }
            ParamMark::ByVal(shape) if shape.size <= abi.record_register_bytes => {
                let chunks = record_chunks(shape.size);
                let mut fillers = 0;
                // Section 6: "a record whose alignment is 16 starts at an even
                // register on Linux, as a `bits<128>` does"; Apple does not
                // round (measured: `after_one_16a16` takes `x1`:`x2` under
                // clang and `x2`:`x3` under GCC-14,
                // `.evidence/builtin-probes/20260906-b10/`).
                if shape.size != 0
                    && (abi.aligned_record_pair_rounding || anonymous.is_some())
                    && shape.align == 16
                    && cursor.pair_padding(general_bank()) != 0
                {
                    fillers += 1;
                }
                // AAPCS64 never splits a record between the last register and
                // the stack: when the chunks do not all fit, the whole record
                // goes to the stack and the remaining register is unused
                // (measured: `after_seven_16` reads `ldp ..., [sp]` and leaves
                // `x7` alone under both oracles).
                if next + fillers + chunks.len() > general && next + fillers < general {
                    fillers = general - next;
                    if abi.aligned_record_pair_rounding
                        && shape.align == 16
                        && (next + fillers) % 2 == 1
                    {
                        fillers += 1;
                    }
                }
                next += fillers + chunks.len();
                items.push(Placed {
                    fillers,
                    passing: Passing::Chunks(chunks),
                    apple_stack: None,
                });
            }
            ParamMark::ByVal(_) => {
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Pointer,
                    apple_stack: None,
                });
                next += 1;
            }
            ParamMark::None | ParamMark::SignExtend => {
                if simd_argument_bytes(ty).is_some() {
                    next_vector += 1;
                    items.push(Placed {
                        fillers: 0,
                        passing: Passing::Value,
                        apple_stack: None,
                    });
                } else {
                    let count = usize::from(is_pair(ty)) + 1;
                    // Section 6.5: a `bits<128>` starts at an even register
                    // where the ABI table rounds a 16-byte-aligned argument;
                    // an anonymous one starts at a 16-byte-aligned slot.
                    let fillers = if count == 2
                        && pair_rounding
                        && (abi.aligned_record_pair_rounding || anonymous.is_some())
                    {
                        cursor.pair_padding(general_bank())
                    } else {
                        0
                    };
                    next += fillers + count;
                    items.push(Placed {
                        fillers,
                        passing: Passing::Value,
                        apple_stack: None,
                    });
                }
            }
        }
        let placed = items.last().expect("each source argument has a placement");
        for _ in 0..placed.fillers {
            let _ = place(&mut cursor, RegClass::Gpr64);
        }
        let (class, count) = match &placed.passing {
            Passing::Chunks(chunks) => (RegClass::Gpr64, chunks.len()),
            Passing::Vectors {
                shape,
                skipped,
                stack,
            } => {
                for _ in 0..*skipped {
                    let _ = place(&mut cursor, RegClass::Simd64);
                }
                for (_, size) in simd_argument_pieces(*shape, *stack) {
                    let _ = place(&mut cursor, simd_class(size));
                }
                continue;
            }
            Passing::Pointer => (RegClass::Gpr64, 1),
            Passing::Value if is_pair(ty) => (RegClass::Gpr64, 2),
            Passing::Value => (class_of(ty).unwrap_or(RegClass::Gpr64), 1),
            Passing::Destination(_) => (RegClass::Gpr64, 0),
        };
        for _ in 0..count {
            let _ = place(&mut cursor, class);
        }
    }
    if apple && !apple_flushed {
        let words = apple_word_classes(&items, arguments, apple_stack_bytes);
        flush(&mut cursor, &words, &mut apple_fill, &mut apple_flushed);
    }
    RecordPlan {
        items,
        positions: named_positions.unwrap_or(next),
        indirect_result,
        anonymous,
        apple_stack_bytes,
        apple_fill,
        refused,
    }
}

/// The classes of the eight-byte words of the Apple named stack area, in
/// address order. A word that holds one piece of eight bytes keeps that
/// piece's class, and a sixteen-byte vector piece is one two-word element;
/// every other word is composed in a general register.
pub(crate) fn apple_word_classes(
    items: &[Placed],
    arguments: &[(EvoIRType, ParamMark)],
    bytes: u64,
) -> Vec<RegClass> {
    apple_layout(items, arguments, bytes)
        .into_iter()
        .map(|word| match word {
            AppleWord::Whole(_, piece) => piece.class,
            AppleWord::Composed(_) => RegClass::Gpr64,
        })
        .collect()
}

/// The named stack area, word by word.
pub(crate) fn apple_layout(
    items: &[Placed],
    arguments: &[(EvoIRType, ParamMark)],
    bytes: u64,
) -> Vec<AppleWord> {
    let mut placed: Vec<(u64, usize, ApplePiece)> = Vec::new();
    for (index, (item, (ty, mark))) in items.iter().zip(arguments).enumerate() {
        let Some(base) = item.apple_stack else {
            continue;
        };
        let (pieces, _) =
            apple_pieces(ty, mark, &item.passing).expect("a placed argument has pieces");
        for piece in pieces {
            placed.push((base + piece.offset, index, piece));
        }
    }
    placed.sort_by_key(|(offset, _, _)| *offset);
    let mut words = Vec::new();
    let mut start = 0_u64;
    let total = bytes.next_multiple_of(8);
    while start < total {
        let inside: Vec<_> = placed
            .iter()
            .filter(|(offset, _, _)| *offset >= start && *offset < start + 8)
            .collect();
        match inside.as_slice() {
            [(offset, index, piece)] if *offset == start && piece.size >= 8 => {
                words.push(AppleWord::Whole(*index, *piece));
                start += piece.size;
            }
            _ => {
                words.push(AppleWord::Composed(
                    inside
                        .iter()
                        .map(|(offset, index, piece)| (*index, *piece, offset - start))
                        .collect(),
                ));
                start += 8;
            }
        }
    }
    words
}

/// The ordinary arguments of a call or an entry block, paired with their marks.
pub(crate) fn marked_types(
    types: &[EvoIRType],
    marks: &[ParamMark],
) -> Vec<(EvoIRType, ParamMark)> {
    types
        .iter()
        .enumerate()
        .filter(|(_, ty)| is_register_type(ty))
        .map(|(index, ty)| (ty.clone(), param_mark(marks, index)))
        .collect()
}
