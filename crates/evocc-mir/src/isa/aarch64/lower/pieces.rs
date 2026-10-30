use super::*;

/// Whether `ty` is a floating vector below eight bytes: a vector of one
/// `f32` lane. It is no AAPCS64 short vector, and GCC 14 and Clang 19 pass
/// it differently, so no call boundary carries it (Section 6.2).
pub(crate) fn tiny_floating_vector(ty: &EvoIRType) -> bool {
    evocc_evoir::floating_vector_shape(ty).is_some() && vector_bytes(ty).is_some_and(|b| b < 8)
}

pub(crate) fn short_vector_bytes(ty: &EvoIRType) -> Option<u64> {
    vector_bytes(ty).filter(|bytes| matches!(bytes, 8 | 16))
}

/// The lanes and lane width of an integer vector of 1, 2 or 4 bytes.
pub(crate) fn tiny_vector_shape(ty: &EvoIRType) -> Option<(u16, u16)> {
    evocc_evoir::integer_vector_shape(ty)
        .filter(|(lanes, bits)| matches!(u32::from(*lanes) * u32::from(*bits), 8 | 16 | 32))
}

/// The Apple convention returns a tiny vector in `v0` with each lane widened
/// to fill 64 bits: four 8-bit lanes as 16-bit lanes, two lanes as 32-bit
/// lanes, and one lane in lane 0 of two 32-bit lanes (Section 6.2).
pub(crate) fn apple_result_lane_bits(lanes: u16) -> u16 {
    if lanes == 4 {
        16
    } else {
        32
    }
}

/// Move a tiny vector from its general register into the Apple result layout.
pub(crate) fn widen_apple_result(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: VReg,
    (lanes, bits): (u16, u16),
) -> VReg {
    let bytes = (lanes * bits / 8) as u8;
    let wide = apple_result_lane_bits(lanes);
    let mut parts = Vec::new();
    for lane in 0..lanes {
        let index = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::MovImm {
            dst: index,
            imm: u64::from(lane),
        });
        let part = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::Vector {
            dst: part,
            extract: true,
            lane_bits: bits,
            bytes,
            args: vec![value, index],
        });
        parts.push(part);
    }
    while parts.len() * usize::from(wide) < 64 {
        let zero = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::MovImm { dst: zero, imm: 0 });
        parts.push(zero);
    }
    let dst = lowerer.fresh(RegClass::Simd64);
    insts.push(MInst::Vector {
        dst,
        extract: false,
        lane_bits: wide,
        bytes: 8,
        args: parts,
    });
    dst
}

/// Read a tiny vector back from the Apple result layout. Only the low bits of
/// each widened lane are significant.
pub(crate) fn narrow_apple_result(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: VReg,
    (lanes, bits): (u16, u16),
) -> VReg {
    let wide = apple_result_lane_bits(lanes);
    let mut parts = Vec::new();
    for lane in 0..lanes {
        let index = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::MovImm {
            dst: index,
            imm: u64::from(lane),
        });
        let part = lowerer.fresh(RegClass::Gpr32);
        insts.push(MInst::Vector {
            dst: part,
            extract: true,
            lane_bits: wide,
            bytes: 8,
            args: vec![value, index],
        });
        parts.push(part);
    }
    let dst = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::Vector {
        dst,
        extract: false,
        lane_bits: bits,
        bytes: (lanes * bits / 8) as u8,
        args: parts,
    });
    dst
}

/// The SIMD bytes an argument of type `ty` takes. A half value travels in
/// the low two bytes of a vector register (AAPCS64 C.1), through the `s`
/// view of a `simd32` transport register (Section 6.3).
pub(crate) fn simd_argument_bytes(ty: &EvoIRType) -> Option<u64> {
    match ty {
        EvoIRType::F16 | EvoIRType::BF16 => Some(2),
        EvoIRType::F32 => Some(4),
        EvoIRType::F64 => Some(8),
        EvoIRType::F128 => Some(16),
        _ => short_vector_bytes(ty),
    }
}

pub(crate) fn simd_class(bytes: u64) -> RegClass {
    match bytes {
        2 | 4 => RegClass::Simd32,
        8 => RegClass::Simd64,
        16 => RegClass::Simd128,
        _ => unreachable!("validated SIMD size"),
    }
}

pub(crate) fn simd_load(bytes: u64) -> LoadKind {
    match bytes {
        2 => LoadKind::Hv,
        4 => LoadKind::S,
        8 => LoadKind::D,
        16 => LoadKind::Q,
        _ => unreachable!("validated SIMD size"),
    }
}

pub(crate) fn simd_store(bytes: u64) -> StoreKind {
    match bytes {
        2 => StoreKind::Hv,
        4 => StoreKind::S,
        8 => StoreKind::D,
        16 => StoreKind::Q,
        _ => unreachable!("validated SIMD size"),
    }
}

/// Whether a record is a binary16 HFA, whose members are two bytes.
pub(crate) fn half_aggregate(shape: RecordShape) -> bool {
    shape
        .floating
        .is_some_and(|f| f.kind == evocc_types::FloatKind::F16)
}

/// An HFA is packed as an object on the stack, not one eight-byte slot per
/// float member. These are private ABI transport pieces, not a new C type.
pub(crate) fn simd_argument_pieces(shape: RecordShape, stack: bool) -> Vec<(u64, u64)> {
    if stack && shape.floating.is_some() {
        (0..shape.size)
            .step_by(8)
            .map(|offset| (offset, (shape.size - offset).min(8)))
            .collect()
    } else {
        let (count, size) = shape.simd_members().unwrap();
        (0..count)
            .map(|member| (u64::from(member) * size, size))
            .collect()
    }
}

/// The eight-byte chunks a record of `size` bytes travels in.
pub(crate) fn record_chunks(size: u64) -> Vec<u64> {
    let mut chunks = Vec::new();
    let mut left = size;
    while left > 0 {
        let take = left.min(8);
        chunks.push(take);
        left -= take;
    }
    chunks
}

/// The accesses one chunk of `size` bytes decomposes into, as `(offset, width)`
/// pairs of power-of-two widths.
///
/// A record's size is not a multiple of eight in general (`struct { int a, b,
/// c; }` is twelve), and the oracles read exactly the bytes the record has:
/// `pass4` moves a word, `pass12` a doubleword and a word
/// (`.evidence/builtin-probes/20260906-b10/b10-callsites-linux-gcc14.s`). Every
/// other size follows from the same rule, so a chunk of three bytes is a
/// halfword and a byte and nothing is read past the object.
///
/// `aligned` is the alignment of the chunk's first byte under `-mstrict-align`
/// (Section 8.9), and `None` without it. Each piece is then the widest that
/// both the bytes left and the alignment of its own address admit, so a chunk
/// of a 4-aligned record is two words. Without the option the widths only
/// descend.
pub(crate) fn chunk_pieces(size: u64, aligned: Option<u64>) -> Vec<(u64, u64)> {
    let mut pieces = Vec::new();
    let mut offset = 0;
    while offset < size {
        let left = size - offset;
        let width = [8_u64, 4, 2, 1]
            .into_iter()
            .find(|width| {
                *width <= left
                    && aligned.is_none_or(|align| *width <= align && offset.is_multiple_of(*width))
            })
            .unwrap_or(1);
        pieces.push((offset, width));
        offset += width;
    }
    pieces
}

/// The alignment of the chunk at `offset` from a base aligned to
/// `base_align`, when the lowering limits piece widths by it.
pub(crate) fn chunk_alignment(lowerer: &Lowerer, base_align: u64, offset: u64) -> Option<u64> {
    lowerer.strict_alignment.then(|| {
        if offset == 0 {
            base_align
        } else {
            base_align.min(1 << offset.trailing_zeros())
        }
    })
}

pub(crate) fn piece_load(width: u64) -> LoadKind {
    match width {
        1 => LoadKind::B,
        2 => LoadKind::H,
        4 => LoadKind::W,
        _ => LoadKind::X,
    }
}

pub(crate) fn piece_store(width: u64) -> StoreKind {
    match width {
        1 => StoreKind::B,
        2 => StoreKind::H,
        4 => StoreKind::W,
        _ => StoreKind::X,
    }
}

/// Load one chunk of `size` bytes at `offset` from `base` into one register.
///
/// The bytes a chunk does not have are unspecified in the convention: both
/// oracles leave whatever the register held there, and the callee reads only
/// the width its member has. Each piece is a transport load (Section 4.1):
/// padding and members that were never stored travel with their
/// initializedness, and a pointer member with its provenance.
pub(crate) fn load_chunk(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    base: VReg,
    base_align: u64,
    offset: u64,
    size: u64,
) -> VReg {
    let mut accumulated: Option<VReg> = None;
    let aligned = chunk_alignment(lowerer, base_align, offset);
    for (piece, width) in chunk_pieces(size, aligned) {
        let dst = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::Load {
            align: 0,
            dst,
            base,
            offset: i64::try_from(offset + piece).unwrap_or(0),
            kind: piece_load(width),
            volatile: false,
            mask: None,
            transport: true,
        });
        let placed = if piece == 0 {
            dst
        } else {
            lowerer.word_binary_imm(insts, BinOp::Lsl, dst, piece * 8)
        };
        accumulated = Some(match accumulated {
            None => placed,
            Some(low) => lowerer.word_binary(insts, BinOp::Orr, low, placed),
        });
    }
    accumulated.expect("a record chunk has at least one byte")
}

/// Store one chunk of `size` bytes held in `src` at `offset` from `base`, with
/// transport stores (Section 4.1).
pub(crate) fn store_chunk(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    src: VReg,
    base: VReg,
    base_align: u64,
    offset: u64,
    size: u64,
) {
    let aligned = chunk_alignment(lowerer, base_align, offset);
    for (piece, width) in chunk_pieces(size, aligned) {
        let value = if piece == 0 {
            src
        } else {
            lowerer.word_binary_imm(insts, BinOp::Lsr, src, piece * 8)
        };
        insts.push(MInst::Store {
            align: 0,
            src: value,
            base,
            offset: i64::try_from(offset + piece).unwrap_or(0),
            kind: piece_store(width),
            volatile: false,
            mask: None,
            transport: true,
        });
    }
}
