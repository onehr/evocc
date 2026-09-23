//! Reuse identical, already executed pure computations inside one block.
//! This is not load forwarding, hoisting, algebraic rewriting or cross-block
//! CSE. Every guard, memory event and effect-token transition stays in place.

use std::collections::HashMap;

use evocc_evoir::{remap_inst_uses, Function, IndexSignedness, Inst, OffsetDirection, Value};
use evocc_types::EvoIRType;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Op {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,
    Zext,
    Sext,
    Trunc,
}

#[derive(PartialEq, Eq, Hash)]
enum Key {
    Const(EvoIRType, u64),
    Binary(Op, EvoIRType, Value, Value),
    Convert(Op, EvoIRType, Value, EvoIRType),
    Offset(
        EvoIRType,
        Value,
        Value,
        OffsetDirection,
        IndexSignedness,
        u64,
    ),
}

fn key(inst: &Inst) -> Option<(Key, &Value)> {
    let binary = |op, result: &Value, a: &Value, b: &Value| {
        Key::Binary(op, result.ty.clone(), a.clone(), b.clone())
    };
    Some(match inst {
        Inst::Const(result, raw) => (Key::Const(result.ty.clone(), *raw), result),
        Inst::Add(r, a, b) => (binary(Op::Add, r, a, b), r),
        Inst::Sub(r, a, b) => (binary(Op::Sub, r, a, b), r),
        Inst::Mul(r, a, b) => (binary(Op::Mul, r, a, b), r),
        Inst::And(r, a, b) => (binary(Op::And, r, a, b), r),
        Inst::Or(r, a, b) => (binary(Op::Or, r, a, b), r),
        Inst::Xor(r, a, b) => (binary(Op::Xor, r, a, b), r),
        Inst::Shl(r, a, b) => (binary(Op::Shl, r, a, b), r),
        Inst::LShr(r, a, b) => (binary(Op::LShr, r, a, b), r),
        Inst::AShr(r, a, b) => (binary(Op::AShr, r, a, b), r),
        Inst::ZExt(r, a, ty) => (
            Key::Convert(Op::Zext, r.ty.clone(), a.clone(), ty.clone()),
            r,
        ),
        Inst::SExt(r, a, ty) => (
            Key::Convert(Op::Sext, r.ty.clone(), a.clone(), ty.clone()),
            r,
        ),
        Inst::Trunc(r, a, ty) => (
            Key::Convert(Op::Trunc, r.ty.clone(), a.clone(), ty.clone()),
            r,
        ),
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => (
            Key::Offset(
                result.ty.clone(),
                base.clone(),
                index.clone(),
                *direction,
                *signedness,
                *scale,
            ),
            result,
        ),
        // In particular: no memory/provenance checks, loads, allocations,
        // calls, assembly, FP operations, atomics or UB obligations.
        _ => return None,
    })
}

pub(super) fn reuse_pure_block_values(function: &mut Function) {
    for block in &mut function.blocks {
        let mut available = HashMap::<Key, Value>::new();
        let mut aliases = HashMap::new();
        block.insts.retain_mut(|inst| {
            remap_inst_uses(inst, &aliases);
            if let Some((key, result)) = key(inst) {
                if let Some(previous) = available.get(&key) {
                    aliases.insert(result.id, previous.clone());
                    return false;
                }
                available.insert(key, result.clone());
            }
            true
        });
        remap_inst_uses(&mut block.terminator, &aliases);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_types::{AddressSpace, Region};
    use std::collections::HashSet;

    fn value(id: u32, ty: EvoIRType) -> Value {
        Value { id, ty }
    }

    #[test]
    fn offset_identity_includes_direction_signedness_scale_and_source() {
        let ptr = EvoIRType::Ptr(AddressSpace::Generic);
        let offset = |base, direction, signedness, scale| Inst::PtrOffset {
            result: value(10, ptr.clone()),
            base: value(base, ptr.clone()),
            index: value(2, EvoIRType::Bits(32)),
            direction,
            signedness,
            scale,
        };
        let mut keys = HashSet::new();
        for base in [0, 1] {
            for direction in [OffsetDirection::Add, OffsetDirection::Subtract] {
                for signedness in [IndexSignedness::Signed, IndexSignedness::Unsigned] {
                    for scale in [1, 4] {
                        keys.insert(key(&offset(base, direction, signedness, scale)).unwrap().0);
                    }
                }
            }
        }
        assert_eq!(keys.len(), 16);
    }

    #[test]
    fn pure_identity_is_typed_and_does_not_admit_tokens_or_poison_definitions() {
        let a = value(0, EvoIRType::Bits(32));
        let b = value(1, EvoIRType::Bits(32));
        let result = value(2, EvoIRType::Bits(64));
        let insts = [
            Inst::Const(a.clone(), 7),
            Inst::Const(result.clone(), 7),
            Inst::ZExt(result.clone(), a.clone(), EvoIRType::Bits(64)),
            Inst::SExt(result.clone(), a.clone(), EvoIRType::Bits(64)),
            Inst::Add(a.clone(), a.clone(), b.clone()),
            Inst::Add(a.clone(), b, a.clone()),
        ];
        let keys: HashSet<_> = insts.iter().map(|inst| key(inst).unwrap().0).collect();
        assert_eq!(keys.len(), insts.len());
        assert!(key(&Inst::Uninitialized(a)).is_none());
        assert!(key(&Inst::PointerValid {
            pointer: value(3, EvoIRType::Ptr(AddressSpace::Generic)),
            mem_in: value(4, EvoIRType::Mem(Region::All)),
            mem_out: value(5, EvoIRType::Mem(Region::All)),
        })
        .is_none());
    }
}
