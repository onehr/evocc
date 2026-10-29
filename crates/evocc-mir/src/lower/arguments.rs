//! Block argument bindings and entry transports.

use super::*;

struct EntryArguments {
    args: Vec<VReg>,
    prologue: Vec<MInst>,
    indirect_result: bool,
    // The incoming stack area is materialized once, when a named argument needs it.
    incoming: Option<VReg>,
}

impl EntryArguments {
    fn read_placed(
        &mut self,
        lowerer: &mut Lowerer,
        function: &Function,
        index: usize,
        arg: &evocc_evoir::BlockArg,
        placement: Option<Placed>,
    ) -> Result<bool, String> {
        if let Some(Placed {
            fillers,
            passing,
            apple_stack,
        }) = placement
        {
            // The Apple convention: a named argument that found no register
            // is read from the named stack area at its offset (Section 6.6).
            if let Some(offset) = apple_stack {
                let base = match self.incoming {
                    Some(base) => base,
                    None => {
                        let base = lowerer.fresh(RegClass::Gpr64);
                        self.prologue.push(MInst::FrameAddress {
                            dst: base,
                            place: FramePlace::IncomingStack,
                        });
                        self.incoming = Some(base);
                        base
                    }
                };
                let mark = param_mark(&function.param_marks, index);
                read_apple_stack_argument(
                    lowerer,
                    &mut self.prologue,
                    &arg.value,
                    &mark,
                    &passing,
                    base,
                    offset,
                )?;
                return Ok(true);
            }
            for _ in 0..fillers {
                self.args.push(lowerer.fresh(RegClass::Gpr64));
            }
            match passing {
                // Section 6: "the callee stores them into a frame slot that
                // becomes the parameter's place".
                Passing::Chunks(chunks) => {
                    self.read_chunks(lowerer, function, index, arg, chunks);
                    return Ok(true);
                }
                Passing::Vectors {
                    shape,
                    skipped,
                    stack,
                } => {
                    self.read_vectors(lowerer, arg, shape, skipped, stack);
                    return Ok(true);
                }
                // Section 6: at most sixteen bytes the destination is the
                // callee's own, and `ret` loads the result out of it; above
                // sixteen the caller's pointer arrives in `x8`, which the
                // marker names and allocation pins.
                Passing::Destination(shape) => {
                    self.read_destination(lowerer, arg, shape);
                    return Ok(true);
                }
                Passing::Pointer | Passing::Value => {}
            }
        }
        Ok(false)
    }

    fn read_chunks(
        &mut self,
        lowerer: &mut Lowerer,
        function: &Function,
        index: usize,
        arg: &evocc_evoir::BlockArg,
        chunks: Vec<u64>,
    ) {
        let slot = lowerer.fresh(RegClass::Gpr64);
        let shape = param_mark(&function.param_marks, index)
            .shape()
            .expect("a chunked argument is a marked record");
        let align = lowerer.record_slot_alignment(shape);
        self.prologue.push(MInst::FrameSlot {
            dst: slot,
            origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
            size: shape.size,
            align,
        });
        lowerer.record_origins += 1;
        let mut offset = 0;
        for size in chunks {
            let chunk = lowerer.fresh(RegClass::Gpr64);
            self.args.push(chunk);
            let base_align = u64::from(align);
            store_chunk(
                lowerer,
                &mut self.prologue,
                chunk,
                slot,
                base_align,
                offset,
                size,
            );
            offset += size;
        }
        lowerer.bind(&arg.value, slot);
    }

    fn read_vectors(
        &mut self,
        lowerer: &mut Lowerer,
        arg: &evocc_evoir::BlockArg,
        shape: RecordShape,
        skipped: usize,
        stack: bool,
    ) {
        for _ in 0..skipped {
            self.args.push(lowerer.fresh(RegClass::Simd64));
        }
        let slot = lowerer.fresh(RegClass::Gpr64);
        self.prologue.push(MInst::FrameSlot {
            dst: slot,
            origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
            size: shape.size,
            align: shape.align,
        });
        lowerer.record_origins += 1;
        for (offset, size) in simd_argument_pieces(shape, stack) {
            let reg = lowerer.fresh(simd_class(size));
            self.args.push(reg);
            self.prologue.push(MInst::Store {
                align: shape.align.min(size as u32),
                src: reg,
                base: slot,
                offset: offset as i64,
                kind: simd_store(size),
                volatile: false,
                mask: None,
                transport: false,
            });
        }
        lowerer.bind(&arg.value, slot);
    }

    fn read_destination(
        &mut self,
        lowerer: &mut Lowerer,
        arg: &evocc_evoir::BlockArg,
        shape: RecordShape,
    ) {
        if shape.size <= lowerer.abi.record_register_bytes || shape.simd_members().is_some() {
            let slot = lowerer.fresh(RegClass::Gpr64);
            self.prologue.push(MInst::FrameSlot {
                dst: slot,
                origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
                size: shape.size,
                align: lowerer.record_slot_alignment(shape),
            });
            lowerer.record_origins += 1;
            lowerer.bind(&arg.value, slot);
            lowerer.destination = Some((shape, slot));
            return;
        }
        self.indirect_result = true;
        let reg = lowerer.fresh(RegClass::Gpr64);
        lowerer.bind(&arg.value, reg);
        self.args.push(reg);
    }

    fn read_scalar(
        &mut self,
        lowerer: &mut Lowerer,
        arg: &evocc_evoir::BlockArg,
        entry: bool,
        plan_is_none: bool,
    ) -> Result<(), String> {
        if is_pair(&arg.value.ty) {
            // Section 6 rounds the next register number up to even before a
            // pair. The skipped position is a real argument nothing reads, so
            // that Section 7's positional pinning gives the halves the numbers
            // the convention names. Inside the function the rule has no
            // meaning, so only the entry block carries it.
            if entry && plan_is_none && self.args.len() % 2 == 1 {
                self.args.push(lowerer.fresh(RegClass::Gpr64));
            }
            let (low, high) = lowerer.fresh_pair();
            lowerer.bind_pair(&arg.value, low, high);
            self.args.push(low);
            self.args.push(high);
            return Ok(());
        }
        if entry && evocc_evoir::is_half_type(&arg.value.ty) {
            // Section 6.3: a half parameter arrives in a transport register.
            let transport = lowerer.fresh(RegClass::Simd32);
            self.args.push(transport);
            let half = half_from_transport(lowerer, &mut self.prologue, transport);
            lowerer.bind(&arg.value, half);
            return Ok(());
        }
        let class = class_of(&arg.value.ty).map_err(|(code, _)| code.to_owned())?;
        let reg = lowerer.fresh(class);
        lowerer.bind(&arg.value, reg);
        self.args.push(reg);
        Ok(())
    }
}

pub(super) fn lower_arguments(
    target: Target,
    lowerer: &mut Lowerer,
    function: &Function,
    block: &Block,
    entry: bool,
) -> Result<(Vec<VReg>, Vec<MInst>, bool), String> {
    // Section 6 (revision 0.18): the entry block's arguments are the
    // function's parameters "in the same order and with the same type", so the
    // marks apply to them position by position.
    let marked = entry.then(|| {
        marked_types(
            &block
                .args
                .iter()
                .map(|arg| arg.value.ty.clone())
                .collect::<Vec<_>>(),
            &function.param_marks,
        )
    });
    let plan = marked
        .as_ref()
        .map(|marked| record_plan(target, marked, true, None, lowerer.anonymous_on_stack));
    let mut output = EntryArguments {
        args: Vec::new(),
        prologue: Vec::new(),
        indirect_result: false,
        incoming: None,
    };
    let mut ordinary = 0_usize;
    for (index, arg) in block.args.iter().enumerate() {
        if !is_register_type(&arg.value.ty) {
            continue;
        }
        let placement = plan.as_ref().map(|plan| plan.items[ordinary].clone());
        ordinary += 1;
        if output.read_placed(lowerer, function, index, arg, placement)? {
            continue;
        }
        output.read_scalar(lowerer, arg, entry, plan.is_none())?;
    }
    Ok((output.args, output.prologue, output.indirect_result))
}
