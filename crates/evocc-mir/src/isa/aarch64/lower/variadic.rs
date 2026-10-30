//! Register and stack paths of variadic access.

use super::*;
use crate::isa::aarch64::abi::VaListLayout;

struct VaArgSelection<'a> {
    access: &'a evocc_evoir::VariadicAccess,
    slot: u64,
    alignment: u64,
    vector_group: Option<(u64, u64)>,
    va_list: VaListLayout,
    offsets_field: i64,
    top_field: i64,
    register_slot: u64,
    ap: VReg,
    carried: Vec<VReg>,
    destination_index: Option<usize>,
    width: usize,
    from_registers: u32,
    taken_from_registers: u32,
    to_the_stack: u32,
    from_the_stack: u32,
    merge: u32,
    with_offset: Vec<VReg>,
    with_list: Vec<VReg>,
    inside: Vec<VReg>,
}

impl<'a> VaArgSelection<'a> {
    fn new(
        lowerer: &mut Lowerer,
        access: &'a evocc_evoir::VariadicAccess,
        block: &Block,
        index: usize,
        slot: u64,
        alignment: u64,
    ) -> Result<Self, String> {
        let vector_group = match &access.op {
            VariadicOp::Arg(result) => simd_argument_bytes(&result.ty).map(|bytes| (1, bytes)),
            VariadicOp::ArgRecord { shape, .. } => shape
                .simd_members()
                .map(|(count, size)| (u64::from(count), size)),
            _ => None,
        };
        let va_list = lowerer.abi.va_list;
        let offsets_field = if vector_group.is_some() {
            va_list.vector_offset
        } else {
            va_list.general_offset
        };
        let top_field = if vector_group.is_some() {
            va_list.vector_top
        } else {
            va_list.general_top
        };
        let vector_save_bytes = lowerer.abi.vector_save_bytes();
        let register_slot = vector_group.map_or(slot, |(count, _)| count * vector_save_bytes);
        let ap = lowerer.reg(&access.list)?;
        let mut carried = live_across(lowerer, block, index + 1);
        // The destination is an input of this instruction, so a last
        // use still has to cross the register/stack selection blocks.
        let destination_index = if let VariadicOp::ArgRecord { destination, .. } = &access.op {
            let destination = lowerer.reg(destination)?;
            Some(
                if let Some(index) = carried.iter().position(|reg| *reg == destination) {
                    index
                } else {
                    carried.push(destination);
                    carried.len() - 1
                },
            )
        } else {
            None
        };
        let width = carried.len();

        let from_registers = lowerer.fresh_block();
        let taken_from_registers = lowerer.fresh_block();
        let to_the_stack = lowerer.fresh_block();
        let from_the_stack = lowerer.fresh_block();
        let merge = lowerer.fresh_block();
        Ok(Self {
            access,
            slot,
            alignment,
            vector_group,
            va_list,
            offsets_field,
            top_field,
            register_slot,
            ap,
            carried,
            destination_index,
            width,
            from_registers,
            taken_from_registers,
            to_the_stack,
            from_the_stack,
            merge,
            with_offset: Vec::new(),
            with_list: Vec::new(),
            inside: Vec::new(),
        })
    }

    fn start(&mut self, lowerer: &mut Lowerer, cursor: &mut blocks::BlockCursor) {
        // `offs = gr_offs; if offs < 0:` -- the field is a signed
        // thirty-two-bit integer, so it is read sign-extended.
        let offs = lowerer.fresh(RegClass::Gpr64);
        cursor.insts.push(MInst::Load {
            align: 0,
            dst: offs,
            base: self.ap,
            offset: self.offsets_field,
            kind: LoadKind::Sw,
            volatile: false,
            mask: None,
            transport: false,
        });
        let flags = lowerer.fresh(RegClass::Flags);
        cursor.insts.push(MInst::CmpImm {
            dst: flags,
            lhs: offs,
            imm: 0,
            width: Width::X,
        });
        let mut with_offset = self.carried.clone();
        with_offset.push(self.ap);
        with_offset.push(offs);
        let mut with_list = self.carried.clone();
        with_list.push(self.ap);
        lowerer.blocks.push(MBlock {
            id: cursor.id,
            args: std::mem::take(&mut cursor.args),
            insts: std::mem::take(&mut cursor.insts),
            term: MTerm::BCond {
                flags,
                cond: Cond::Lt,
                taken: self.from_registers,
                taken_args: with_offset.clone(),
                untaken: self.from_the_stack,
                untaken_args: with_list.clone(),
            },
        });
        self.with_offset = with_offset;
        self.with_list = with_list;
    }

    fn check_registers(&mut self, lowerer: &mut Lowerer) {
        // `if A == 16: offs = (offs + 15) & ~15; next = offs + S8;
        //  if next <= 0: ...`
        let params = fresh_like(lowerer, &self.with_offset);
        let list = params[self.width];
        let offset = params[self.width + 1];
        let mut body = Vec::new();
        let aligned = if self.alignment == 16 && self.vector_group.is_none() {
            round_to_sixteen(lowerer, &mut body, offset)
        } else {
            offset
        };
        let next = lowerer.binary_imm(&mut body, BinOp::Add, aligned, self.register_slot, Width::X);
        let decided = lowerer.fresh(RegClass::Flags);
        body.push(MInst::CmpImm {
            dst: decided,
            lhs: next,
            imm: 0,
            width: Width::X,
        });
        let mut inside = params[..self.width].to_vec();
        inside.push(list);
        inside.push(aligned);
        inside.push(next);
        let mut outside = params[..self.width].to_vec();
        outside.push(list);
        lowerer.blocks.push(MBlock {
            id: self.from_registers,
            args: params,
            insts: body,
            term: MTerm::BCond {
                flags: decided,
                cond: Cond::Le,
                taken: self.taken_from_registers,
                taken_args: inside.clone(),
                untaken: self.to_the_stack,
                untaken_args: outside,
            },
        });
        self.inside = inside;
    }

    fn read_registers(&self, lowerer: &mut Lowerer) {
        // `gr_offs = next; address = gr_top + offs; done`
        let params = fresh_like(lowerer, &self.inside);
        let list = params[self.width];
        let aligned = params[self.width + 1];
        let next = params[self.width + 2];
        let mut body = Vec::new();
        body.push(MInst::Store {
            align: 0,
            src: next,
            base: list,
            offset: self.offsets_field,
            kind: StoreKind::W,
            volatile: false,
            mask: None,
            transport: false,
        });
        let top = lowerer.fresh(RegClass::Gpr64);
        body.push(MInst::Load {
            align: 0,
            dst: top,
            base: list,
            offset: self.top_field,
            kind: LoadKind::P,
            volatile: false,
            mask: None,
            transport: false,
        });
        let address = lowerer.binary(&mut body, BinOp::Add, top, aligned, Width::X);
        if let (Some((count, bytes)), Some(destination)) =
            (self.vector_group, self.destination_index)
        {
            for i in 0..count {
                let v = lowerer.fresh(simd_class(bytes));
                body.push(MInst::Load {
                    dst: v,
                    base: address,
                    offset: (i * 16) as i64,
                    kind: simd_load(bytes),
                    volatile: false,
                    align: bytes as u32,
                    mask: None,
                    transport: false,
                });
                body.push(MInst::Store {
                    src: v,
                    base: params[destination],
                    offset: (i * bytes) as i64,
                    kind: simd_store(bytes),
                    volatile: false,
                    align: bytes as u32,
                    mask: None,
                    transport: false,
                });
            }
        }
        let mut reached = params[..self.width].to_vec();
        reached.push(address);
        lowerer.blocks.push(MBlock {
            id: self.taken_from_registers,
            args: params,
            insts: body,
            term: MTerm::B {
                target: self.merge,
                args: reached,
            },
        });
    }

    fn exhaust_registers(&self, lowerer: &mut Lowerer) {
        // `gr_offs = 0`, then the stack path.
        let params = fresh_like(lowerer, &self.with_list);
        let list = params[self.width];
        let zero = lowerer.fresh(RegClass::Gpr32);
        let mut body = vec![
            MInst::MovImm { dst: zero, imm: 0 },
            MInst::Store {
                align: 0,
                src: zero,
                base: list,
                offset: self.offsets_field,
                kind: StoreKind::W,
                volatile: false,
                mask: None,
                transport: false,
            },
        ];
        let mut onward = params[..self.width].to_vec();
        onward.push(list);
        let _ = &mut body;
        lowerer.blocks.push(MBlock {
            id: self.to_the_stack,
            args: params,
            insts: body,
            term: MTerm::B {
                target: self.from_the_stack,
                args: onward,
            },
        });
    }

    fn read_stack(&self, lowerer: &mut Lowerer) {
        // `address = stack; if A == 16: address = (address + 15) &
        //  ~15; stack = address + S8`
        let params = fresh_like(lowerer, &self.with_list);
        let list = params[self.width];
        let mut body = Vec::new();
        let raw = lowerer.fresh(RegClass::Gpr64);
        body.push(MInst::Load {
            align: 0,
            dst: raw,
            base: list,
            offset: self.va_list.stack,
            kind: LoadKind::P,
            volatile: false,
            mask: None,
            transport: false,
        });
        let address = if self.alignment == 16 {
            round_to_sixteen(lowerer, &mut body, raw)
        } else {
            raw
        };
        let bumped = lowerer.binary_imm(&mut body, BinOp::Add, address, self.slot, Width::X);
        if let (Some(_), Some(destination), VariadicOp::ArgRecord { shape, .. }) =
            (self.vector_group, self.destination_index, &self.access.op)
        {
            body.push(MInst::MemCopy {
                dst: params[destination],
                src: address,
                size: shape.size,
                align: shape.align.min(self.alignment as u32),
                volatile: evocc_memory::CopyVolatility::None,
            });
        }
        body.push(MInst::Store {
            align: 0,
            src: bumped,
            base: list,
            offset: self.va_list.stack,
            kind: StoreKind::P,
            volatile: false,
            mask: None,
            transport: false,
        });
        let mut reached = params[..self.width].to_vec();
        reached.push(address);
        lowerer.blocks.push(MBlock {
            id: self.from_the_stack,
            args: params,
            insts: body,
            term: MTerm::B {
                target: self.merge,
                args: reached,
            },
        });
    }

    fn merge(&self, lowerer: &mut Lowerer, cursor: &mut blocks::BlockCursor) -> Result<(), String> {
        // "then loads `T` from `address`."
        let mut shape = self.carried.clone();
        shape.push(VReg::new(0, RegClass::Gpr64));
        let params = fresh_like(lowerer, &shape);
        let address = params[self.width];
        let mut body = Vec::new();
        if let VariadicOp::ArgRecord { shape, .. } = &self.access.op {
            if shape.simd_members().is_none() {
                let src = if shape.size > 16 {
                    let src = lowerer.fresh(RegClass::Gpr64);
                    body.push(MInst::Load {
                        align: 0,
                        dst: src,
                        base: address,
                        offset: 0,
                        kind: LoadKind::P,
                        volatile: false,
                        mask: None,
                        transport: false,
                    });
                    src
                } else {
                    address
                };
                body.push(MInst::MemCopy {
                    dst: params[self
                        .destination_index
                        .expect("record destination is carried")],
                    src,
                    size: shape.size,
                    align: shape.align.min(8),
                    volatile: evocc_memory::CopyVolatility::None,
                });
            }
        } else if let VariadicOp::Arg(result) = &self.access.op {
            if is_pair(&result.ty) {
                let (low, high) = lowerer.fresh_pair();
                body.push(MInst::Ldp {
                    rt1: low,
                    rt2: high,
                    base: address,
                    offset: 0,
                    writeback: false,
                    volatile: false,
                });
                lowerer.bind_pair(result, low, high);
            } else {
                let kind = load_kind(&result.ty)?;
                let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
                let dst = lowerer.fresh(class);
                body.push(MInst::Load {
                    align: 0,
                    dst,
                    base: address,
                    offset: 0,
                    kind,
                    volatile: false,
                    mask: None,
                    transport: false,
                });
                lowerer.bind(result, dst);
            }
        }
        let mut rebind: HashMap<u32, VReg> = HashMap::new();
        for (old, fresh) in self.carried.iter().zip(params.iter()) {
            rebind.insert(old.id, *fresh);
        }
        for reg in lowerer.map.values_mut().chain(lowerer.high.values_mut()) {
            if let Some(fresh) = rebind.get(&reg.id) {
                *reg = *fresh;
            }
        }
        cursor.id = self.merge;
        cursor.args = params;
        cursor.insts = body;
        lowerer.address_preparation = AddressPreparation::default();
        Ok(())
    }
}

pub(crate) fn lower_access(
    lowerer: &mut Lowerer,
    cursor: &mut blocks::BlockCursor,
    block: &Block,
    index: usize,
    inst: &Inst,
) -> Result<bool, String> {
    // Section 6's `va_arg`: "the register-then-stack procedure of Section 6
    // for `T`'s size and alignment, ending in a load of `T` from the
    // computed address". The procedure branches, so it is written here,
    // where a block can be split, exactly as the `ub.assume` row above is.
    if let Inst::VaAccess(access) = inst {
        if lowerer.anonymous_on_stack {
            if let Some((slot, alignment)) = variadic_access_slot(&access.op) {
                lower_stack_va_arg(lowerer, &mut cursor.insts, access, slot, alignment)?;
                lowerer.address_preparation = AddressPreparation::default();
                return Ok(true);
            }
        }
        if let Some((slot, alignment)) = variadic_access_slot(&access.op) {
            let mut selection =
                VaArgSelection::new(lowerer, access, block, index, slot, alignment)?;
            selection.start(lowerer, cursor);
            selection.check_registers(lowerer);
            selection.read_registers(lowerer);
            selection.exhaust_registers(lowerer);
            selection.read_stack(lowerer);
            selection.merge(lowerer, cursor)?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// The same placement determines both saved registers and the first
/// anonymous stack slot, including pair/record alignment padding.
pub(crate) fn named_argument_extent(
    target: Target,
    function: &Function,
    apple: bool,
) -> (u8, u8, u64) {
    // Revision 0.18: a record spends the registers its chunks take, so the
    // count is the convention's, which is what the plan already answers.
    let declared: Vec<EvoIRType> = function
        .params
        .iter()
        .map(|param| param.ty.clone())
        .collect();
    let plan = record_plan(
        target,
        &marked_types(&declared, &function.param_marks),
        true,
        None,
        apple,
    );
    // The Apple convention: the anonymous arguments start after the named
    // stack area, rounded up to eight (Section 6.12).
    if apple {
        let mut cursor = target.aarch64_abi().argument_cursor();
        for (placed, (ty, _)) in plan
            .items
            .iter()
            .zip(marked_types(&declared, &function.param_marks).iter())
        {
            if placed.apple_stack.is_some() {
                continue;
            }
            let classes: Vec<RegClass> = match &placed.passing {
                Passing::Value if is_pair(ty) => vec![RegClass::Gpr64; 2],
                Passing::Value => vec![class_of(ty).expect("validated argument class")],
                Passing::Pointer => vec![RegClass::Gpr64],
                Passing::Chunks(chunks) => vec![RegClass::Gpr64; chunks.len()],
                Passing::Vectors { shape, .. } => simd_argument_pieces(*shape, false)
                    .into_iter()
                    .map(|(_, bytes)| simd_class(bytes))
                    .collect(),
                Passing::Destination(_) => Vec::new(),
            };
            for class in classes {
                let _ = place(&mut cursor, class);
            }
        }
        return (
            cursor.used(general_bank()) as u8,
            cursor.used(vector_bank()) as u8,
            plan.apple_stack_bytes.next_multiple_of(8),
        );
    }
    let marked = marked_types(&declared, &function.param_marks);
    let abi = target.aarch64_abi();
    let mut cursor = abi.argument_cursor();
    for ((ty, _), placed) in marked.iter().zip(&plan.items) {
        let mut classes = vec![RegClass::Gpr64; placed.fillers];
        match &placed.passing {
            Passing::Value if is_pair(ty) => classes.extend([RegClass::Gpr64; 2]),
            Passing::Value => classes.push(class_of(ty).expect("validated argument class")),
            Passing::Pointer => classes.push(RegClass::Gpr64),
            Passing::Chunks(chunks) => {
                classes.extend(std::iter::repeat_n(RegClass::Gpr64, chunks.len()))
            }
            Passing::Vectors {
                shape,
                skipped,
                stack,
            } => {
                classes.extend(std::iter::repeat_n(RegClass::Simd64, *skipped));
                classes.extend(
                    simd_argument_pieces(*shape, *stack)
                        .into_iter()
                        .map(|(_, bytes)| simd_class(bytes)),
                );
            }
            Passing::Destination(_) => {}
        }
        for class in classes {
            let _ = place(&mut cursor, class);
        }
    }
    (
        cursor.used(general_bank()) as u8,
        cursor.used(vector_bank()) as u8,
        cursor.stack_bytes(abi),
    )
}

/// The size rounded up to eight (`S8`) and the alignment (`A`) of one
/// `va_arg` type, or nothing when the type has no Section 6 procedure.
pub(crate) fn variadic_access_slot(op: &VariadicOp) -> Option<(u64, u64)> {
    match op {
        VariadicOp::Arg(result) => variadic_slot(&result.ty),
        VariadicOp::ArgRecord { shape, .. }
            if shape.size > 16 && shape.simd_members().is_none() =>
        {
            Some((8, 8))
        }
        VariadicOp::ArgRecord { shape, .. } => Some((
            shape.size.div_ceil(8) * 8,
            shape
                .floating
                .map_or(u64::from(shape.align).clamp(8, 16), |f| {
                    u64::from(f.abi_align)
                }),
        )),
        VariadicOp::Start | VariadicOp::End => None,
    }
}

pub(crate) fn variadic_slot(ty: &EvoIRType) -> Option<(u64, u64)> {
    if let Some(bytes) = vector_bytes(ty) {
        return Some((bytes.max(8), bytes));
    }
    match ty {
        EvoIRType::Ptr(_)
        | EvoIRType::FnPtr { .. }
        | EvoIRType::F16
        | EvoIRType::BF16
        | EvoIRType::F32
        | EvoIRType::F64 => Some((8, 8)),
        EvoIRType::Bits(128) | EvoIRType::F128 => Some((16, 16)),
        EvoIRType::Bits(width) if *width <= 64 => Some((8, 8)),
        _ => None,
    }
}

/// `va_start`: skips named incoming stack slots, then stores
/// `gr_top = save area + 64`, `vr_top = gr_top + 128`, `gr_offs =
/// -8 * (8 - k)` and `vr_offs = -128` (Section 6, revision 0.28). The field
/// offsets, register counts and save widths are the ABI table's.
pub(crate) fn lower_va_start(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    list: &Value,
) -> Result<(), String> {
    let (named, named_vectors, stack_bytes) = lowerer
        .named_arguments
        .ok_or_else(|| "a va_start outside a variadic function".to_owned())?;
    let ap = lowerer.reg(list)?;
    let abi = lowerer.abi;
    let va_list = abi.va_list;

    let stack = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::FrameAddress {
        dst: stack,
        place: FramePlace::IncomingStack,
    });
    let stack = if stack_bytes == 0 {
        stack
    } else {
        lowerer.binary_imm(insts, BinOp::Add, stack, stack_bytes, Width::X)
    };
    insts.push(MInst::Store {
        align: 0,
        src: stack,
        base: ap,
        offset: va_list.stack,
        kind: StoreKind::P,
        volatile: false,
        mask: None,
        transport: false,
    });
    // The Apple variadic convention: the list is this one pointer.
    if lowerer.anonymous_on_stack {
        return Ok(());
    }

    let top = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::FrameAddress {
        dst: top,
        place: FramePlace::RegisterSaveTop,
    });
    let vector_top = lowerer.binary_imm(
        insts,
        BinOp::Add,
        top,
        abi.vector_save_area_bytes(),
        Width::X,
    );
    for (offset, address) in [(va_list.general_top, top), (va_list.vector_top, vector_top)] {
        insts.push(MInst::Store {
            align: 0,
            src: address,
            base: ap,
            offset,
            kind: StoreKind::P,
            volatile: false,
            mask: None,
            transport: false,
        });
    }

    let general_save = abi.general_save_bytes() as i64;
    let offs = -general_save * (abi.general_arguments() as i64 - i64::from(named));
    let gr_offs = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::MovImm {
        dst: gr_offs,
        imm: u64::from(offs as u32),
    });
    insts.push(MInst::Store {
        align: 0,
        src: gr_offs,
        base: ap,
        offset: va_list.general_offset,
        kind: StoreKind::W,
        volatile: false,
        mask: None,
        transport: false,
    });
    let vr_offs = lowerer.fresh(RegClass::Gpr32);
    let vector_save = abi.vector_save_bytes() as i32;
    let vector_count = abi.vector_arguments() as i32;
    insts.push(MInst::MovImm {
        dst: vr_offs,
        imm: u64::from((-vector_save * (vector_count - i32::from(named_vectors))) as u32),
    });
    insts.push(MInst::Store {
        align: 0,
        src: vr_offs,
        base: ap,
        offset: va_list.vector_offset,
        kind: StoreKind::W,
        volatile: false,
        mask: None,
        transport: false,
    });
    Ok(())
}

/// `va_arg` under the Apple variadic convention: the list is one pointer to
/// the next anonymous argument on the caller's stack. Align it to 16 for a
/// 16-byte type, read the argument there, and advance the pointer by the slot.
pub(crate) fn lower_stack_va_arg(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    access: &evocc_evoir::VariadicAccess,
    slot: u64,
    alignment: u64,
) -> Result<(), String> {
    let field = lowerer.abi.va_list.stack;
    let ap = lowerer.reg(&access.list)?;
    let raw = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::Load {
        align: 0,
        dst: raw,
        base: ap,
        offset: field,
        kind: LoadKind::P,
        volatile: false,
        mask: None,
        transport: false,
    });
    let address = if alignment == 16 {
        round_to_sixteen(lowerer, insts, raw)
    } else {
        raw
    };
    let bumped = lowerer.binary_imm(insts, BinOp::Add, address, slot, Width::X);
    insts.push(MInst::Store {
        align: 0,
        src: bumped,
        base: ap,
        offset: field,
        kind: StoreKind::P,
        volatile: false,
        mask: None,
        transport: false,
    });
    match &access.op {
        VariadicOp::ArgRecord { destination, shape } => {
            let destination = lowerer.reg(destination)?;
            // A record above sixteen bytes that is not a vector group travels
            // as a pointer to the caller's copy.
            let source = if shape.size > 16 && shape.simd_members().is_none() {
                let source = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Load {
                    align: 0,
                    dst: source,
                    base: address,
                    offset: 0,
                    kind: LoadKind::P,
                    volatile: false,
                    mask: None,
                    transport: false,
                });
                source
            } else {
                address
            };
            insts.push(MInst::MemCopy {
                dst: destination,
                src: source,
                size: shape.size,
                align: shape.align.min(8),
                volatile: evocc_memory::CopyVolatility::None,
            });
        }
        VariadicOp::Arg(result) if is_pair(&result.ty) => {
            let (low, high) = lowerer.fresh_pair();
            insts.push(MInst::Ldp {
                rt1: low,
                rt2: high,
                base: address,
                offset: 0,
                writeback: false,
                volatile: false,
            });
            lowerer.bind_pair(result, low, high);
        }
        VariadicOp::Arg(result) => {
            let kind = load_kind(&result.ty)?;
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Load {
                align: 0,
                dst,
                base: address,
                offset: 0,
                kind,
                volatile: false,
                mask: None,
                transport: false,
            });
            lowerer.bind(result, dst);
        }
        VariadicOp::Start | VariadicOp::End => {}
    }
    Ok(())
}
