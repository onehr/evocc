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
    // Nothing between a producer and its consumer may write flags: with
    // adjacency already established, that means no two consecutive writers.
    for pair in block.insts.windows(2) {
        if writes_flags(&pair[0]) && writes_flags(&pair[1]) {
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
    if reg.class == RegClass::Flags {
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
    match inst {
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
        MInst::WideMultiply { dst, lhs, rhs, .. } => {
            if dst.class != RegClass::Gpr64 {
                return Err(format!(
                    "@{name} block {block}: a wide multiply writes a gpr64 value"
                ));
            }
            expect_general(name, block, *lhs, "a wide multiply operand")?;
            expect_general(name, block, *rhs, "a wide multiply operand")
        }
        MInst::MemCopy { dst, src, .. } => {
            if dst.class != RegClass::Gpr64 || src.class != RegClass::Gpr64 {
                return Err(format!(
                    "@{name} block {block}: a mem.copy addresses two gpr64 values"
                ));
            }
            Ok(())
        }
        MInst::Adrp { dst, .. } | MInst::FrameSlot { dst, .. } => {
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
        MTerm::BCond { flags, .. } => expect_flags(name, block, *flags, "a b.cond condition"),
        MTerm::Cb { src, .. } | MTerm::Tb { src, .. } => {
            expect_general(name, block, *src, "a branch operand")
        }
        MTerm::Ret { values } => {
            // Section 6: "an EvoMIR function returns up to eight integer or
            // pointer results in x0 to x7 in order."
            if values.len() > 8 {
                return Err(format!(
                    "@{name} block {block}: {} results exceed x0 to x7",
                    values.len()
                ));
            }
            for value in values {
                expect_general(name, block, *value, "a result")?;
            }
            Ok(())
        }
        MTerm::B { .. } | MTerm::Brk { .. } => Ok(()),
    }
}
