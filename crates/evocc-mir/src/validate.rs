//! The Section 2 verifier.
//!
//! Section 2 puts the pre-allocation form in "block-argument SSA form, the same
//! discipline as EvoIR Section 2, so that a block's arguments are its only
//! inputs and every value has one definition", and states the `flags` rule as
//! "a legality rule the verifier of Section 9 checks, not a hint".

use std::collections::{HashMap, HashSet};

use crate::{
    inst_defs, inst_uses, term_edges, term_uses, writes_flags, MBlock, MFunction, MInst, MModule,
    MTerm, RegClass, VReg,
};

/// Verify every function of a module.
pub fn validate_module(module: &MModule) -> Result<(), String> {
    let mut symbols = HashSet::new();
    for function in &module.functions {
        if !symbols.insert(function.name.as_str()) {
            return Err(format!("duplicate function symbol @{}", function.name));
        }
    }
    for function in &module.functions {
        validate_function(function)?;
    }
    Ok(())
}

/// Verify one function.
pub fn validate_function(function: &MFunction) -> Result<(), String> {
    let name = &function.name;
    let Some(entry) = function.blocks.first() else {
        return Err(format!("function @{name} has no entry block"));
    };
    // The entry block's identifier is not spelled in the textual form (its
    // label is `^entry` regardless), so it is fixed here rather than left to
    // round-trip by luck.
    if entry.id != 0 {
        return Err(format!("function @{name} entry block is not block 0"));
    }

    let mut blocks: HashMap<u32, &MBlock> = HashMap::new();
    for block in &function.blocks {
        if blocks.insert(block.id, block).is_some() {
            return Err(format!("function @{name} defines block {} twice", block.id));
        }
    }

    let mut defined: HashSet<u32> = HashSet::new();
    for block in &function.blocks {
        for reg in &block.args {
            if !defined.insert(reg.id) {
                return Err(format!("@{name}: %v{} is defined twice", reg.id));
            }
        }
        for inst in &block.insts {
            for reg in inst_defs(inst) {
                if !defined.insert(reg.id) {
                    return Err(format!("@{name}: %v{} is defined twice", reg.id));
                }
            }
        }
    }

    for block in &function.blocks {
        validate_block(name, block, &blocks)?;
    }

    let mut reachable: HashSet<u32> = HashSet::new();
    let mut work = vec![entry.id];
    reachable.insert(entry.id);
    while let Some(id) = work.pop() {
        let block = blocks[&id];
        for (target, _) in term_edges(&block.term) {
            if reachable.insert(target) {
                work.push(target);
            }
        }
    }
    for block in &function.blocks {
        if !reachable.contains(&block.id) {
            return Err(format!("@{name}: block {} is unreachable", block.id));
        }
    }
    Ok(())
}

fn validate_block(
    name: &str,
    block: &MBlock,
    blocks: &HashMap<u32, &MBlock>,
) -> Result<(), String> {
    let id = block.id;
    let mut live: HashMap<u32, RegClass> = HashMap::new();
    for reg in &block.args {
        if reg.class == RegClass::Flags {
            return Err(format!(
                "@{name} block {id}: a flags value cannot be a block argument"
            ));
        }
        live.insert(reg.id, reg.class);
    }

    let check = |live: &HashMap<u32, RegClass>, reg: VReg| -> Result<(), String> {
        match live.get(&reg.id) {
            Some(class) if *class == reg.class => Ok(()),
            Some(class) => Err(format!(
                "@{name} block {id}: %v{} is {} at its definition and {} at this use",
                reg.id,
                class.mnemonic(),
                reg.class.mnemonic()
            )),
            None => Err(format!(
                "@{name} block {id}: %v{} is used without a definition in this block",
                reg.id
            )),
        }
    };

    for (index, inst) in block.insts.iter().enumerate() {
        for reg in inst_uses(inst) {
            check(&live, reg)?;
        }
        validate_shape(name, id, inst)?;
        for reg in inst_defs(inst) {
            live.insert(reg.id, reg.class);
        }
        // Section 2: a flags value is consumed only by the instruction that
        // immediately follows its producer, and nothing between them writes
        // flags. Adjacency is checked from the producer's side, because a
        // producer whose result is never consumed is equally illegal.
        if let Some(flags) = inst_defs(inst)
            .into_iter()
            .find(|reg| reg.class == RegClass::Flags)
        {
            let consumers: Vec<VReg> = match block.insts.get(index + 1) {
                Some(next) => inst_uses(next),
                None => term_uses(&block.term),
            };
            if !consumers.contains(&flags) {
                return Err(format!(
                    "@{name} block {id}: %v{} is not consumed by the instruction that follows it",
                    flags.id
                ));
            }
        }
    }
    for reg in term_uses(&block.term) {
        check(&live, reg)?;
    }
    validate_term_shape(name, id, &block.term)?;

    // Every other flags use must be the one adjacency admits.
    let mut flags_uses: HashMap<u32, usize> = HashMap::new();
    for inst in &block.insts {
        for reg in inst_uses(inst) {
            if reg.class == RegClass::Flags {
                *flags_uses.entry(reg.id).or_default() += 1;
            }
        }
    }
    for reg in term_uses(&block.term) {
        if reg.class == RegClass::Flags {
            *flags_uses.entry(reg.id).or_default() += 1;
        }
    }
    for (value, count) in flags_uses {
        if count != 1 {
            return Err(format!(
                "@{name} block {id}: %v{value} is consumed {count} times; a flags value has one consumer"
            ));
        }
    }
    // Nothing between a producer and its consumer may write flags. With
    // adjacency already established, that means no two consecutive writers,
    // unless the second one is the first one's consumer: Section 5's
    // `sbcs` (revision 0.10) reads the borrow the preceding `cmp` left and
    // sets the flags the following `cset` reads, so a producer standing
    // immediately before its own consumer is the sequence the rule admits,
    // not a violation of it.
    for pair in block.insts.windows(2) {
        if !writes_flags(&pair[0]) || !writes_flags(&pair[1]) {
            continue;
        }
        let produced: Vec<u32> = inst_defs(&pair[0])
            .into_iter()
            .filter(|reg| reg.class == RegClass::Flags)
            .map(|reg| reg.id)
            .collect();
        let consumes = inst_uses(&pair[1])
            .into_iter()
            .any(|reg| reg.class == RegClass::Flags && produced.contains(&reg.id));
        if !consumes {
            return Err(format!(
                "@{name} block {id}: a second flags writer stands between a producer and its consumer"
            ));
        }
    }

    for (target, args) in term_edges(&block.term) {
        let Some(destination) = blocks.get(&target) else {
            return Err(format!(
                "@{name} block {id}: edge to missing block {target}"
            ));
        };
        if destination.args.len() != args.len() {
            return Err(format!(
                "@{name} block {id}: edge to block {target} passes {} values for {} arguments",
                args.len(),
                destination.args.len()
            ));
        }
        for (passed, expected) in args.iter().zip(&destination.args) {
            if passed.class != expected.class {
                return Err(format!(
                    "@{name} block {id}: edge to block {target} passes {} for {}",
                    passed.class.mnemonic(),
                    expected.class.mnemonic()
                ));
            }
        }
    }
    Ok(())
}

fn expect_general(name: &str, block: u32, reg: VReg, role: &str) -> Result<(), String> {
    if matches!(
        reg.class,
        RegClass::Flags | RegClass::Simd32 | RegClass::Simd64 | RegClass::Simd128
    ) {
        return Err(format!(
            "@{name} block {block}: {role} is a flags value, which no general form reads"
        ));
    }
    Ok(())
}

fn expect_flags(name: &str, block: u32, reg: VReg, role: &str) -> Result<(), String> {
    if reg.class != RegClass::Flags {
        return Err(format!(
            "@{name} block {block}: {role} must be a flags value"
        ));
    }
    Ok(())
}

fn validate_shape(name: &str, block: u32, inst: &MInst) -> Result<(), String> {
    if let MInst::Load { align, .. } | MInst::Store { align, .. } = inst {
        if *align != 0 && !align.is_power_of_two() {
            return Err(format!(
                "@{name} block {block}: memory alignment must be a power of two"
            ));
        }
    }
    match inst {
        MInst::Floating {
            strict,
            integer_bits,
            dst,
            op,
            kind,
            args,
        } => {
            if *integer_bits != 0 && !strict {
                return Err("integer width metadata requires strict conversion".into());
            }
            let instruction = crate::floating::operation(*dst, *op, *kind, args, *integer_bits)?;
            if *strict {
                evocc_evoir::StrictFloatingInst {
                    instruction,
                    world_in: evocc_evoir::Value {
                        id: u32::MAX,
                        ty: evocc_evoir::EvoIRType::World,
                    },
                    world_out: evocc_evoir::Value {
                        id: u32::MAX - 1,
                        ty: evocc_evoir::EvoIRType::World,
                    },
                }
                .validate()?;
            }
            Ok(())
        }
        MInst::Asm(body) => validate_assembly_classes(body),
        MInst::Aes { dst, op, args } => {
            if dst.class != RegClass::Simd128
                || args.len() != op.arity()
                || args.iter().any(|arg| arg.class != RegClass::Simd128)
            {
                return Err("invalid AES operand types or arity".into());
            }
            Ok(())
        }
        MInst::Vector {
            dst,
            extract,
            lane_bits,
            bytes,
            args,
        } => {
            let vector_class = match bytes {
                1 | 2 | 4 => Some(RegClass::Gpr32),
                8 => Some(RegClass::Simd64),
                16 => Some(RegClass::Simd128),
                _ => None,
            };
            let valid = matches!(lane_bits, 8 | 16 | 32 | 64)
                && u16::from(*bytes) * 8 >= *lane_bits
                && if *extract {
                    args.len() == 2
                        && Some(args[0].class) == vector_class
                        && matches!(args[1].class, RegClass::Gpr32 | RegClass::Gpr64)
                        && dst.class
                            == if *lane_bits == 64 {
                                RegClass::Gpr64
                            } else {
                                RegClass::Gpr32
                            }
                } else {
                    Some(dst.class) == vector_class
                        && args.len() * usize::from(*lane_bits) == usize::from(*bytes) * 8
                        && args.iter().all(|arg| {
                            arg.class
                                == if *lane_bits == 64 {
                                    RegClass::Gpr64
                                } else {
                                    RegClass::Gpr32
                                }
                        })
                };
            if valid {
                Ok(())
            } else {
                Err(format!(
                    "@{name} block {block}: invalid vector representation operation"
                ))
            }
        }
        MInst::Mov { dst, src } if dst.class.is_simd() || src.class.is_simd() => {
            if dst.class == src.class {
                Ok(())
            } else {
                Err("a SIMD move cannot change register bank or width".into())
            }
        }
        MInst::MovImm { dst, imm } if dst.class.is_simd() => {
            if *imm == 0 {
                Ok(())
            } else {
                Err("SIMD immediate is only the zero representation".into())
            }
        }
        MInst::Load {
            dst, base, kind, ..
        } if dst.class.is_simd() || matches!(kind, crate::LoadKind::D | crate::LoadKind::Q) => {
            if base.class == RegClass::Gpr64
                && matches!(
                    (kind, dst.class),
                    (crate::LoadKind::S, RegClass::Simd32)
                        | (crate::LoadKind::D, RegClass::Simd64)
                        | (crate::LoadKind::Q, RegClass::Simd128)
                )
            {
                Ok(())
            } else {
                Err("invalid SIMD load shape".into())
            }
        }
        MInst::Store {
            src, base, kind, ..
        } if src.class.is_simd() || matches!(kind, crate::StoreKind::D | crate::StoreKind::Q) => {
            if base.class == RegClass::Gpr64
                && matches!(
                    (kind, src.class),
                    (crate::StoreKind::S, RegClass::Simd32)
                        | (crate::StoreKind::D, RegClass::Simd64)
                        | (crate::StoreKind::Q, RegClass::Simd128)
                )
            {
                Ok(())
            } else {
                Err("invalid SIMD store shape".into())
            }
        }
        MInst::Bl {
            symbol,
            args,
            results,
            fp_mode: Some(_),
        } => {
            let helper = crate::floating::helpers::qualified(symbol)
                .ok_or("strict FP call requires a qualified numeric helper")?;
            let (inputs, outputs) = helper.classes();
            if args.iter().map(|r| r.class).collect::<Vec<_>>() == inputs
                && results.iter().map(|r| r.class).collect::<Vec<_>>() == outputs
            {
                Ok(())
            } else {
                Err("strict FP helper signature mismatch".into())
            }
        }
        MInst::Bl { args, results, .. } | MInst::Blr { args, results, .. } => {
            if args
                .iter()
                .chain(results)
                .any(|r| r.class == RegClass::Flags)
            {
                Err("flags cannot cross a call".into())
            } else {
                Ok(())
            }
        }
        MInst::SpillStore { .. }
        | MInst::SpillReload { .. }
        | MInst::IncomingArg { .. }
        | MInst::OutgoingArg { .. } => Ok(()),
        MInst::Atomic {
            load,
            value,
            base,
            kind,
            order,
            ..
        } => {
            if !order.valid_for(*load)
                || !matches!(
                    kind,
                    crate::LoadKind::B
                        | crate::LoadKind::H
                        | crate::LoadKind::W
                        | crate::LoadKind::X
                        | crate::LoadKind::P
                        | crate::LoadKind::Pf
                )
                || base.class != crate::RegClass::Gpr64
                || value.class
                    != if kind.size_bytes() <= 4 {
                        crate::RegClass::Gpr32
                    } else {
                        crate::RegClass::Gpr64
                    }
            {
                return Err(format!(
                    "@{name} block {block}: invalid atomic access shape/order"
                ));
            }
            Ok(())
        }
        MInst::Cmp { dst, lhs, rhs, .. } => {
            expect_flags(name, block, *dst, "a cmp result")?;
            expect_general(name, block, *lhs, "a cmp operand")?;
            expect_general(name, block, *rhs, "a cmp operand")
        }
        MInst::CmpImm { dst, lhs, .. } => {
            expect_flags(name, block, *dst, "a cmp result")?;
            expect_general(name, block, *lhs, "a cmp operand")
        }
        MInst::Cset { dst, flags, .. } => {
            expect_general(name, block, *dst, "a cset result")?;
            expect_flags(name, block, *flags, "a cset condition source")
        }
        MInst::Csel {
            dst,
            flags,
            lhs,
            rhs,
            ..
        } => {
            expect_general(name, block, *dst, "a csel result")?;
            expect_flags(name, block, *flags, "a csel condition source")?;
            expect_general(name, block, *lhs, "a csel operand")?;
            expect_general(name, block, *rhs, "a csel operand")
        }
        MInst::FlagBinary {
            dst,
            flags,
            lhs,
            rhs,
            ..
        } => {
            expect_general(name, block, *dst, "an adds/subs result")?;
            expect_flags(name, block, *flags, "an adds/subs flags result")?;
            expect_general(name, block, *lhs, "an adds/subs operand")?;
            expect_general(name, block, *rhs, "an adds/subs operand")
        }
        // Section 5's carry-propagating forms read the `nzcv` the preceding
        // flag-setting form produced, which the Section 2 adjacency rule
        // already constrains.
        MInst::CarryBinary {
            dst,
            flags,
            lhs,
            rhs,
            ..
        } => {
            expect_general(name, block, *dst, "an adc/sbc result")?;
            expect_flags(name, block, *flags, "an adc/sbc carry source")?;
            expect_general(name, block, *lhs, "an adc/sbc operand")?;
            expect_general(name, block, *rhs, "an adc/sbc operand")
        }
        MInst::AddCarryFlags {
            dst,
            flags_in,
            lhs,
            rhs,
            ..
        }
        | MInst::SubCarryFlags {
            dst,
            flags_in,
            lhs,
            rhs,
            ..
        } => {
            expect_flags(name, block, *dst, "an adcs/sbcs flags result")?;
            expect_flags(name, block, *flags_in, "an adcs/sbcs carry source")?;
            expect_general(name, block, *lhs, "an adcs/sbcs operand")?;
            expect_general(name, block, *rhs, "an adcs/sbcs operand")
        }
        MInst::TstImm { dst, lhs, .. } => {
            expect_flags(name, block, *dst, "a tst result")?;
            expect_general(name, block, *lhs, "a tst operand")
        }
        MInst::WideMultiply { dst, lhs, rhs, .. } => {
            if dst.class != RegClass::Gpr64 {
                return Err(format!(
                    "@{name} block {block}: a wide multiply writes a gpr64 value"
                ));
            }
            expect_general(name, block, *lhs, "a wide multiply operand")?;
            expect_general(name, block, *rhs, "a wide multiply operand")
        }
        MInst::MemCopy {
            dst,
            src,
            size,
            align,
            volatile,
        } => {
            if dst.class != RegClass::Gpr64 || src.class != RegClass::Gpr64 {
                return Err(format!(
                    "@{name} block {block}: a mem.copy addresses two gpr64 values"
                ));
            }
            if !align.is_power_of_two()
                || !volatile.supports(evocc_memory::AccessLayout::new(*size, *align))
            {
                return Err(format!("@{name} block {block}: volatile mem.copy requires one naturally aligned 1/2/4/8-byte access"));
            }
            Ok(())
        }
        MInst::AtomicRmw {
            dst,
            base,
            value,
            expected,
            operation,
            kind,
            ..
        } => {
            if base.class != RegClass::Gpr64
                || dst.class != value.class
                || dst.class
                    != if kind.size_bytes() == 8 {
                        RegClass::Gpr64
                    } else {
                        RegClass::Gpr32
                    }
                || !matches!(kind.size_bytes(), 1 | 2 | 4 | 8)
                || expected.as_ref().is_some_and(|e| e.class != dst.class)
                || (*operation == evocc_memory::AtomicRmwOp::CompareExchange) != expected.is_some()
            {
                return Err(format!(
                    "@{name} block {block}: invalid atomic RMW operands"
                ));
            }
            Ok(())
        }
        MInst::StackAlloc {
            dst, size, align, ..
        } => {
            if dst.class != RegClass::Gpr64
                || size.class != RegClass::Gpr64
                || !align.is_power_of_two()
            {
                return Err(format!("@{name} block {block}: stack.alloc needs gpr64 address/size and power-of-two alignment"));
            }
            Ok(())
        }
        MInst::Adrp { dst, .. }
        | MInst::Adr { dst, .. }
        | MInst::FrameSlot { dst, .. }
        | MInst::StackRelease { address: dst } => {
            if dst.class != RegClass::Gpr64 {
                return Err(format!(
                    "@{name} block {block}: an address is a gpr64 value"
                ));
            }
            Ok(())
        }
        other => {
            for reg in inst_defs(other).into_iter().chain(inst_uses(other)) {
                expect_general(name, block, reg, "an operand")?;
            }
            Ok(())
        }
    }
}

fn validate_term_shape(name: &str, block: u32, term: &MTerm) -> Result<(), String> {
    match term {
        MTerm::Indirect { target, edges } => {
            if target.class != RegClass::Gpr64 || edges.is_empty() {
                return Err(format!(
                    "{name}: indirect branch in block {block} needs a gpr64 target and successors"
                ));
            }
            if edges.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err("indirect successors must be unique and sorted".into());
            }
            Ok(())
        }
        MTerm::AsmGoto { body, .. } => validate_assembly_classes(body),
        MTerm::BCond { flags, .. } => expect_flags(name, block, *flags, "a b.cond condition"),
        MTerm::Cb { src, .. } | MTerm::Tb { src, .. } => {
            expect_general(name, block, *src, "a branch operand")
        }
        MTerm::Ret { values } => {
            // Section 6: "an EvoMIR function returns up to eight integer or
            // pointer results in x0 to x7 in order."
            if values.iter().filter(|r| r.class.is_simd()).count() > 8
                || values.iter().filter(|r| !r.class.is_simd()).count() > 8
            {
                return Err(format!(
                    "@{name} block {block}: {} results exceed x0 to x7",
                    values.len()
                ));
            }
            for value in values {
                if value.class == RegClass::Flags {
                    return Err("flags cannot be returned".into());
                }
            }
            Ok(())
        }
        MTerm::B { .. } | MTerm::Brk { .. } => Ok(()),
    }
}

fn validate_assembly_classes(body: &crate::AsmBody) -> Result<(), String> {
    use crate::{AsmConstraint, AsmDirection, AsmValue};
    for (index, operand) in body.numbered().into_iter().enumerate() {
        let output = index < body.outputs.len();
        if output == (operand.constraint.direction() == AsmDirection::Read) {
            return Err("assembly constraint direction does not match operand role".into());
        }
        let constraint = if let AsmConstraint::Tied(digit) = operand.constraint {
            let output = body
                .outputs
                .get(usize::from(digit))
                .ok_or("assembly tie names a missing output")?;
            if output.register().map(|reg| reg.class) != operand.register().map(|reg| reg.class) {
                return Err("assembly tied operands have different classes".into());
            }
            output.constraint
        } else {
            operand.constraint
        };
        if let AsmValue::Reg(reg) = operand.value {
            if constraint.simd() != reg.class.is_simd() || reg.class == RegClass::Flags {
                return Err("assembly operand has the wrong register class".into());
            }
            if matches!(
                constraint,
                AsmConstraint::Memory { .. } | AsmConstraint::Address
            ) && reg.class != RegClass::Gpr64
            {
                return Err("assembly address requires gpr64".into());
            }
        }
    }
    Ok(())
}
