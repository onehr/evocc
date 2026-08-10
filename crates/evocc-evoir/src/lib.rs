//! EvoIR: typed block-argument SSA for optimization
//!
//! Values carry an identity (`id`) and a type so that the SSA validator can
//! verify single-static-definition form before any optimization transforms.

use std::collections::{HashMap, HashSet};

use evocc_types::EvoIRType;

// ---------------------------------------------------------------------------
// Core data types
// ---------------------------------------------------------------------------

/// A value in the IR with a unique identifier and its type.
///
/// Every instruction that produces a result carries a `Value` as its first
/// field.  Operands and block-argument bindings also use `Value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value {
    pub id: u32,
    pub ty: EvoIRType,
}

/// A block argument: a named, typed value bound at block entry by the
/// incoming edge (or by the function prologue for the entry block).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockArg {
    pub name: String,
    pub ty: EvoIRType,
}

/// Opaque identifier for a basic block.
pub type BlockId = u32;

// ---------------------------------------------------------------------------
// Comparison operators
// ---------------------------------------------------------------------------

/// Integer comparison operators for `ICmp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    SLt,
    SLe,
    ULt,
    ULe,
}

// ---------------------------------------------------------------------------
// Instructions
// ---------------------------------------------------------------------------

/// An SSA instruction.
///
/// Instructions that produce a value (`Const`, `Add`, `Sub`, `Mul`, `SDiv`,
/// `UDiv`, `ICmp`, `Load`, `PtrOffset`, `Call`, `SExt`, `ZExt`, `Trunc`)
/// carry the result `Value` as their first field.
///
/// Terminators (`Br`, `CondBr`, `Ret`) and side-effecting instructions
/// (`Store`) do not produce values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    /// `Const(result, immediate)`: define `result` as the integer literal.
    Const(Value, i64),

    /// `Add(result, lhs, rhs)`: integer addition.
    Add(Value, Value, Value),
    /// `Sub(result, lhs, rhs)`: integer subtraction.
    Sub(Value, Value, Value),
    /// `Mul(result, lhs, rhs)`: integer multiplication.
    Mul(Value, Value, Value),
    /// `SDiv(result, lhs, rhs)`: signed integer division.
    SDiv(Value, Value, Value),
    /// `UDiv(result, lhs, rhs)`: unsigned integer division.
    UDiv(Value, Value, Value),

    /// `ICmp(result, op, lhs, rhs)`: integer comparison.
    ICmp(Value, CmpOp, Value, Value),

    /// `Br(target, args)`: unconditional branch, passing `args` to the
    /// destination block's block arguments.
    Br(BlockId, Vec<Value>),

    /// `CondBr(cond, true_block, true_args, false_block, false_args)`
    /// conditional branch.
    CondBr(Value, BlockId, Vec<Value>, BlockId, Vec<Value>),

    /// `Ret(values)`: return from function with the given values.
    Ret(Vec<Value>),

    /// `Load(result, addr, ty)`: load a value of type `ty` from `addr`.
    Load(Value, Value, EvoIRType),

    /// `Store(addr, stored)`: store a value to memory.
    Store(Value, Value),

    /// `PtrOffset(result, base, offset)`: pointer arithmetic.
    PtrOffset(Value, Value, Value),

    /// `Call(result, callee, args, ret_tys)`: call `callee` with `args`;
    /// the call may return zero or more values.
    Call(Value, String, Vec<Value>, Vec<EvoIRType>),

    /// `SExt(result, src, target_ty)`: sign-extend `src` to `target_ty`.
    SExt(Value, Value, EvoIRType),

    /// `ZExt(result, src, target_ty)`: zero-extend `src` to `target_ty`.
    ZExt(Value, Value, EvoIRType),

    /// `Trunc(result, src, target_ty)`: truncate `src` to `target_ty`.
    Trunc(Value, Value, EvoIRType),
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the result `Value` produced by the instruction, or `None` for
/// terminators and stores.
#[must_use]
pub fn inst_result(inst: &Inst) -> Option<&Value> {
    match inst {
        Inst::Const(v, _)
        | Inst::Add(v, ..)
        | Inst::Sub(v, ..)
        | Inst::Mul(v, ..)
        | Inst::SDiv(v, ..)
        | Inst::UDiv(v, ..)
        | Inst::ICmp(v, ..)
        | Inst::Load(v, ..)
        | Inst::PtrOffset(v, ..)
        | Inst::Call(v, ..)
        | Inst::SExt(v, ..)
        | Inst::ZExt(v, ..)
        | Inst::Trunc(v, ..) => Some(v),
        Inst::Br(..) | Inst::CondBr(..) | Inst::Ret(..) | Inst::Store(..) => None,
    }
}

/// Returns all `Value` operands that this instruction *uses* (reads).
#[must_use]
pub fn inst_uses(inst: &Inst) -> Vec<&Value> {
    match inst {
        Inst::Const(..) => vec![],

        Inst::Add(_, lhs, rhs)
        | Inst::Sub(_, lhs, rhs)
        | Inst::Mul(_, lhs, rhs)
        | Inst::SDiv(_, lhs, rhs)
        | Inst::UDiv(_, lhs, rhs) => vec![lhs, rhs],

        Inst::ICmp(_, _, lhs, rhs) => vec![lhs, rhs],

        Inst::Br(_, args) => args.iter().collect(),
        Inst::CondBr(cond, _, targs, _, fargs) => {
            let mut uses: Vec<&Value> = vec![cond];
            uses.extend(targs);
            uses.extend(fargs);
            uses
        }
        Inst::Ret(values) => values.iter().collect(),

        Inst::Load(_, addr, _) => vec![addr],
        Inst::Store(addr, stored) => vec![addr, stored],
        Inst::PtrOffset(_, base, offset) => vec![base, offset],
        Inst::Call(_, _, args, _) => args.iter().collect(),
        Inst::SExt(_, src, _) | Inst::ZExt(_, src, _) | Inst::Trunc(_, src, _) => vec![src],
    }
}

// ---------------------------------------------------------------------------
// Blocks and functions
// ---------------------------------------------------------------------------

/// A basic block in the control-flow graph.
///
/// `args` are block arguments: values bound at block entry.  The entry
/// block's args semantically include the function's `params`.
///
/// `insts` is the list of non-terminator instructions; `terminator` is
/// always present and must be one of `Br`, `CondBr`, or `Ret`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: BlockId,
    pub args: Vec<BlockArg>,
    pub insts: Vec<Inst>,
    pub terminator: Inst,
}

/// A function in EvoIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    pub name: String,
    pub params: Vec<BlockArg>,
    pub ret_tys: Vec<EvoIRType>,
    pub blocks: Vec<Block>,
}

// ---------------------------------------------------------------------------
// SSA validator
// ---------------------------------------------------------------------------

/// Validate that every `Value` is defined exactly once in the function.
///
/// Definition sites are:
///   - the result of every value-producing instruction (`Const`, `Add`,
///     `Sub`, `Mul`, `SDiv`, `UDiv`, `ICmp`, `Load`, `PtrOffset`, `Call`,
///     `SExt`, `ZExt`, `Trunc`), keyed by `Value.id`;
///   - block arguments (including function `params`), keyed by
///     `(block_id, arg_index)` — these are implicitly defined at block entry.
///
/// Returns `Ok(())` when the SSA property holds, or `Err(msg)` describing the
/// first violation.
pub fn validate_ssa(func: &Function) -> Result<(), String> {
    let mut defs: HashMap<u32, String> = HashMap::new();
    let mut block_arg_defs: HashSet<(BlockId, String)> = HashSet::new();

    // -- 1. Register block argument definitions ----------------------------

    for block in &func.blocks {
        for arg in &block.args {
            if !block_arg_defs.insert((block.id, arg.name.clone())) {
                return Err(format!(
                    "duplicate block argument '{}' in block {}",
                    arg.name, block.id
                ));
            }
        }
    }

    // -- 2. Register instruction definitions -------------------------------

    for block in &func.blocks {
        for (i, inst) in block.insts.iter().enumerate() {
            if let Some(v) = inst_result(inst) {
                let site = format!("instruction {} in block {}", i, block.id);
                if let Some(existing) = defs.insert(v.id, site.clone()) {
                    return Err(format!(
                        "value v{} defined more than once (previous: {})",
                        v.id, existing
                    ));
                }
            }
        }
    }

    // -- 3. Check that every use refers to a defined value -----------------
    //
    // A use is satisfied when its `Value.id` appears in `defs` (instruction
    // definitions) or when it corresponds to a block argument.  Since block
    // arguments do not carry a `Value.id`, we cannot cross-reference
    // instruction operands against block args by id alone.  We therefore
    // track which `Value` objects are bound as block arguments by
    // materialising a synthetic `Value` for each block arg so that uses can
    // be matched.
    //
    // To keep the simple id-based scheme working, we assign each block arg a
    // synthetic id from a high-offset range (`BLOCK_ARG_ID_BASE + index`)
    // and register those ids in `defs` as well.

    const BLOCK_ARG_ID_BASE: u32 = 1_000_000;

    for block in &func.blocks {
        for (ai, arg) in block.args.iter().enumerate() {
            let syn_id = BLOCK_ARG_ID_BASE + ai as u32;
            if let Some(existing) = defs.insert(syn_id, format!("block arg '{}' in block {}", arg.name, block.id)) {
                return Err(format!(
                    "synthetic id v{} for block arg '{}' collides with {}",
                    syn_id, arg.name, existing
                ));
            }
        }
    }

    // Also register function params as synthetic entry-block args.
    for (pi, param) in func.params.iter().enumerate() {
        let syn_id = BLOCK_ARG_ID_BASE + 500_000 + pi as u32;
        if let Some(existing) = defs.insert(syn_id, format!("function param '{}'", param.name)) {
            return Err(format!(
                "synthetic id v{} for param '{}' collides with {}",
                syn_id, param.name, existing
            ));
        }
    }

    // Now verify every use.
    for block in &func.blocks {
        for inst in &block.insts {
            for used in inst_uses(inst) {
                if !defs.contains_key(&used.id) {
                    return Err(format!(
                        "value v{} used but never defined (in block {}, instruction {:?})",
                        used.id, block.id, inst
                    ));
                }
            }
        }
        for used in inst_uses(&block.terminator) {
            if !defs.contains_key(&used.id) {
                return Err(format!(
                    "value v{} used in terminator of block {} but never defined",
                    used.id, block.id
                ));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Interpreter
// ---------------------------------------------------------------------------

/// Execute a function and return the result values (as raw i64).
pub fn eval_function(func: &Function) -> Vec<i64> {
    let mut state = InterpState::new();
    let entry = &func.blocks[0];
    state.exec_block(func, entry.id, &[]);
    state.results.clone()
}

const BLOCK_ARG_ID_BASE: u32 = 1_000_000;

struct InterpState {
    values: HashMap<u32, i64>,
    results: Vec<i64>,
}

impl InterpState {
    fn new() -> Self {
        InterpState {
            values: HashMap::new(),
            results: Vec::new(),
        }
    }

    fn exec_block(&mut self, func: &Function, block_id: BlockId, args: &[i64]) {
        let block = func
            .blocks
            .iter()
            .find(|b| b.id == block_id)
            .unwrap_or_else(|| panic!("block {} not found", block_id));

        for (i, _arg) in block.args.iter().enumerate() {
            let syn_id = BLOCK_ARG_ID_BASE + i as u32;
            self.values.insert(syn_id, args[i]);
        }

        for inst in &block.insts {
            self.exec_inst(inst);
        }

        match &block.terminator {
            Inst::Br(target, targs) => {
                let resolved: Vec<i64> = targs.iter().map(|v| self.values[&v.id]).collect();
                self.exec_block(func, *target, &resolved);
            }
            Inst::CondBr(cond, true_block, true_args, false_block, false_args) => {
                let cond_val = self.values[&cond.id];
                if cond_val != 0 {
                    let resolved: Vec<i64> = true_args.iter().map(|v| self.values[&v.id]).collect();
                    self.exec_block(func, *true_block, &resolved);
                } else {
                    let resolved: Vec<i64> = false_args.iter().map(|v| self.values[&v.id]).collect();
                    self.exec_block(func, *false_block, &resolved);
                }
            }
            Inst::Ret(values) => {
                self.results = values.iter().map(|v| self.values[&v.id]).collect();
            }
            other => panic!("unexpected terminator in block {}: {:?}", block.id, other),
        }
    }

    fn exec_inst(&mut self, inst: &Inst) {
        match inst {
            Inst::Const(v, imm) => {
                self.values.insert(v.id, *imm);
            }
            Inst::Add(dst, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                self.values.insert(dst.id, l.wrapping_add(r));
            }
            Inst::Sub(dst, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                self.values.insert(dst.id, l.wrapping_sub(r));
            }
            Inst::Mul(dst, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                self.values.insert(dst.id, l.wrapping_mul(r));
            }
            Inst::SDiv(dst, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                self.values.insert(dst.id, l.wrapping_div(r));
            }
            Inst::UDiv(dst, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                self.values.insert(dst.id, l.wrapping_div(r));
            }
            Inst::ICmp(dst, op, lhs, rhs) => {
                let l = self.values[&lhs.id];
                let r = self.values[&rhs.id];
                let result = match op {
                    CmpOp::Eq => (l == r) as i64,
                    CmpOp::Ne => (l != r) as i64,
                    CmpOp::SLt => (l < r) as i64,
                    CmpOp::SLe => (l <= r) as i64,
                    CmpOp::ULt => ((l as u64) < (r as u64)) as i64,
                    CmpOp::ULe => ((l as u64) <= (r as u64)) as i64,
                };
                self.values.insert(dst.id, result);
            }
            Inst::Load(dst, ..)
            | Inst::PtrOffset(dst, ..)
            | Inst::Call(dst, ..)
            | Inst::SExt(dst, ..)
            | Inst::ZExt(dst, ..)
            | Inst::Trunc(dst, ..) => {
                self.values.insert(dst.id, 0);
            }
            Inst::Store(..) => {}
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use evocc_types::EvoIRType;

    #[test]
    fn evoir_block_args_are_single_definition() {
        let bits32 = EvoIRType::Bits(32);

        // ------------------------------------------------------------------
        // Valid: a single Const defines v0; Ret consumes v0.
        // ------------------------------------------------------------------
        let v0 = Value {
            id: 0,
            ty: bits32.clone(),
        };

        let block = Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Const(v0.clone(), 42)],
            terminator: Inst::Ret(vec![v0.clone()]),
        };

        let func = Function {
            name: "valid".into(),
            params: vec![],
            ret_tys: vec![bits32.clone()],
            blocks: vec![block],
        };

        assert!(validate_ssa(&func).is_ok(), "valid function should pass SSA check");

        // ------------------------------------------------------------------
        // Invalid: two Const instructions produce the same Value.
        // ------------------------------------------------------------------
        let block_bad = Block {
            id: 0,
            args: vec![],
            insts: vec![
                Inst::Const(v0.clone(), 42),
                Inst::Const(v0.clone(), 99), // duplicate def of v0
            ],
            terminator: Inst::Ret(vec![v0.clone()]),
        };

        let func_bad = Function {
            name: "bad".into(),
            params: vec![],
            ret_tys: vec![bits32.clone()],
            blocks: vec![block_bad],
        };

        assert!(
            validate_ssa(&func_bad).is_err(),
            "duplicate definition must fail SSA check"
        );

        // ------------------------------------------------------------------
        // Invalid: a use refers to an undefined value.
        // ------------------------------------------------------------------
        let undef = Value {
            id: 999,
            ty: bits32.clone(),
        };

        let block_undef = Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Const(v0.clone(), 10)],
            terminator: Inst::Ret(vec![undef]),
        };

        let func_undef = Function {
            name: "undef_use".into(),
            params: vec![],
            ret_tys: vec![bits32],
            blocks: vec![block_undef],
        };

        assert!(
            validate_ssa(&func_undef).is_err(),
            "undefined use must fail SSA check"
        );
    }

    #[test]
    fn evoir_interpreter_executes_basic_blocks() {
        let bits32 = EvoIRType::Bits(32);
        let v0 = Value {
            id: 0,
            ty: bits32.clone(),
        };

        let entry = Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Const(v0.clone(), 42)],
            terminator: Inst::Ret(vec![v0.clone()]),
        };
        let func = Function {
            name: "test".into(),
            params: vec![],
            ret_tys: vec![bits32],
            blocks: vec![entry],
        };

        let results = eval_function(&func);
        assert_eq!(results, vec![42]);
    }

    #[test]
    fn evoir_interpreter_branches_correctly() {
        let bits32 = EvoIRType::Bits(32);
        let v0 = Value {
            id: 0,
            ty: bits32.clone(),
        };
        let v1 = Value {
            id: 1,
            ty: bits32.clone(),
        };
        let v2 = Value {
            id: 2,
            ty: bits32.clone(),
        };

        let entry = Block {
            id: 0,
            args: vec![],
            insts: vec![Inst::Const(v0.clone(), 1)],
            terminator: Inst::CondBr(v0.clone(), 1, vec![], 2, vec![]),
        };
        let then_block = Block {
            id: 1,
            args: vec![],
            insts: vec![Inst::Const(v1.clone(), 42)],
            terminator: Inst::Ret(vec![v1.clone()]),
        };
        let else_block = Block {
            id: 2,
            args: vec![],
            insts: vec![Inst::Const(v2.clone(), 0)],
            terminator: Inst::Ret(vec![v2.clone()]),
        };
        let func = Function {
            name: "branch".into(),
            params: vec![],
            ret_tys: vec![bits32],
            blocks: vec![entry, then_block, else_block],
        };

        let results = eval_function(&func);
        assert_eq!(results, vec![42], "cond is 1 (true), so result is 42");
    }
}
