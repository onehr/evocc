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

pub(super) fn lower_access(
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
