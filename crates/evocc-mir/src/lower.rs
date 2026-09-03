//! The Section 5 lowering from extracted EvoIR to pre-allocation EvoMIR.
//!
//! Section 5 gives one table and one rule for everything outside it: "A form
//! not in the table is refused with the diagnostic named in Section 10." The
//! refusal is therefore not a fallback; it is the lowering of every row that
//! does not exist, and Section 1 makes it fail closed: "the driver produces no
//! assembly for that function."
//!
//! Section 1 also makes the lowering a function of its input: "two runs over
//! the same EvoIR text produce the same EvoMIR text, byte for byte." Nothing
//! below reads a hash map in iteration order, a clock, or an address.

use std::collections::{BTreeSet, HashMap, HashSet};

use evocc_evoir::{
    inst_results, inst_uses, Block, CmpOp, Function, Inst, Module, TrapReason, UbReason, Value,
};
use evocc_types::EvoIRType;

use crate::{
    BinOp, Cond, ExtOp, FlagOp, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, RegClass,
    StoreKind, SymbolKind, VReg, WideMul, Width,
};

/// One EvoIR form this revision does not lower, reported at the instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub function: String,
    pub block: u32,
    pub index: usize,
    pub code: &'static str,
    pub name: &'static str,
    pub detail: String,
}

/// The Section 10 table.
const E4001: (&str, &str) = ("E4001", "not-lowered: floating-point");
const E4002: (&str, &str) = ("E4002", "not-lowered: vector");
const E4003: (&str, &str) = ("E4003", "not-lowered: wide-integer");
const E4004: (&str, &str) = ("E4004", "not-lowered: tuple");
const E4009: (&str, &str) = ("E4009", "not-lowered: unsupported-form");
const E4010: (&str, &str) = ("E4010", "abi: too-many-register-arguments");
const E4013: (&str, &str) = ("E4013", "abi: too-many-results");

/// Section 6: "Integer and pointer arguments of at most 64 bits go in `x0` to
/// `x7` in order ... A ninth integer argument is refused."
const MAX_REGISTER_ARGUMENTS: usize = 8;

/// Section 6: "an EvoMIR function returns up to eight integer or pointer
/// results in `x0` to `x7` in order ... A ninth result is `E4013`."
const MAX_REGISTER_RESULTS: usize = 8;

/// Lower a whole module, or report every refusal it contains.
pub fn lower_module(module: &Module) -> Result<MModule, Vec<Refusal>> {
    let mut refusals = Vec::new();
    for function in &module.functions {
        scan_function(module, function, &mut refusals);
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let mut functions = Vec::new();
    for function in &module.functions {
        match lower_function(function) {
            Ok(lowered) => functions.push(lowered),
            Err(detail) => refusals.push(Refusal {
                function: function.name.clone(),
                block: function.blocks.first().map_or(0, |block| block.id),
                index: 0,
                code: E4009.0,
                name: E4009.1,
                detail,
            }),
        }
    }
    if refusals.is_empty() {
        Ok(MModule {
            globals: module.globals.clone(),
            functions,
        })
    } else {
        Err(refusals)
    }
}

// ---------------------------------------------------------------------------
// Types and classes
// ---------------------------------------------------------------------------

/// Whether a type occupies a machine register.
fn is_register_type(ty: &EvoIRType) -> bool {
    !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World)
}

/// Section 2's class table, and the Section 10 code for a type it has no row
/// for.
fn class_of(ty: &EvoIRType) -> Result<RegClass, (&'static str, &'static str)> {
    match ty {
        EvoIRType::Bits(width) => match width {
            1..=32 => Ok(RegClass::Gpr32),
            33..=64 => Ok(RegClass::Gpr64),
            _ => Err(E4003),
        },
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => Ok(RegClass::Gpr64),
        EvoIRType::F32 | EvoIRType::F64 => Err(E4001),
        EvoIRType::Vec { .. } => Err(E4002),
        EvoIRType::Tuple(_) => Err(E4004),
        _ => Err(E4009),
    }
}

/// The number of significant bits a value carries: Section 3's `N`.
fn significant_bits(ty: &EvoIRType) -> u16 {
    match ty {
        EvoIRType::Bits(width) => *width,
        _ => 64,
    }
}

fn width_of(class: RegClass) -> Width {
    match class {
        RegClass::Gpr64 => Width::X,
        _ => Width::W,
    }
}

/// The `brk` immediate for an EvoIR reason.
///
/// Section 4 says the immediate "names the EvoIR trap or assumption reason",
/// and Section 5 says `brk #imm` carries "the reason's number", but revision
/// 0.1 never numbers the reasons. The number used here is the decimal part of
/// the EvoIR diagnostic the reason already owns (Section 6.1 of the EvoIR
/// specification), which is an authority both sides already share and fits the
/// sixteen bits the form has. Recorded gap.
fn reason_immediate(code: &str) -> u16 {
    code.trim_start_matches('E').parse().unwrap_or(0)
}

fn ub_immediate(reason: UbReason) -> u16 {
    reason_immediate(reason.diagnostic())
}

fn trap_immediate(reason: TrapReason) -> u16 {
    reason_immediate(reason.diagnostic())
}

// ---------------------------------------------------------------------------
// Refusal scan
// ---------------------------------------------------------------------------

fn ordinary_types(types: &[EvoIRType]) -> Vec<&EvoIRType> {
    types.iter().filter(|ty| is_register_type(ty)).collect()
}

fn scan_function(module: &Module, function: &Function, refusals: &mut Vec<Refusal>) {
    let entry_block = function.blocks.first().map_or(0, |block| block.id);
    let mut report =
        |code: (&'static str, &'static str), block: u32, index: usize, detail: String| {
            refusals.push(Refusal {
                function: function.name.clone(),
                block,
                index,
                code: code.0,
                name: code.1,
                detail,
            });
        };

    let params: Vec<&EvoIRType> = function
        .params
        .iter()
        .map(|param| &param.ty)
        .filter(|ty| is_register_type(ty))
        .collect();
    if params.len() > MAX_REGISTER_ARGUMENTS {
        report(
            E4010,
            entry_block,
            0,
            format!(
                "@{} takes {} register arguments",
                function.name,
                params.len()
            ),
        );
    }
    for ty in &params {
        if let Err(code) = class_of(ty) {
            report(code, entry_block, 0, format!("parameter of type {ty:?}"));
        }
    }
    let results = ordinary_types(&function.ret_tys);
    if results.len() > MAX_REGISTER_RESULTS {
        report(
            E4013,
            entry_block,
            0,
            format!(
                "@{} returns {} ordinary results",
                function.name,
                results.len()
            ),
        );
    }
    for ty in &results {
        if let Err(code) = class_of(ty) {
            report(code, entry_block, 0, format!("result of type {ty:?}"));
        }
    }

    for block in &function.blocks {
        for arg in &block.args {
            if is_register_type(&arg.value.ty) {
                if let Err(code) = class_of(&arg.value.ty) {
                    report(
                        code,
                        block.id,
                        0,
                        format!("block argument of type {:?}", arg.value.ty),
                    );
                }
            }
        }
        for (index, inst) in block.insts.iter().enumerate() {
            scan_inst(module, inst, block.id, index, &mut report);
        }
        scan_inst(
            module,
            &block.terminator,
            block.id,
            block.insts.len(),
            &mut report,
        );
    }
}

fn scan_inst(
    module: &Module,
    inst: &Inst,
    block: u32,
    index: usize,
    report: &mut impl FnMut((&'static str, &'static str), u32, usize, String),
) {
    // Every operand and result must have a class before the form is judged, so
    // that a floating-point add is E4001 rather than E4009.
    for value in inst_results(inst).into_iter().chain(inst_uses(inst)) {
        if is_register_type(&value.ty) {
            if let Err(code) = class_of(&value.ty) {
                report(
                    code,
                    block,
                    index,
                    format!("operand of type {:?}", value.ty),
                );
            }
        }
    }

    match inst {
        // The Section 5 table, row by row.
        Inst::Const(..)
        | Inst::Uninitialized(_)
        | Inst::Add(..)
        | Inst::Sub(..)
        | Inst::Mul(..)
        | Inst::And(..)
        | Inst::Or(..)
        | Inst::Xor(..)
        | Inst::Shl(..)
        | Inst::LShr(..)
        | Inst::AShr(..)
        | Inst::SDiv(..)
        | Inst::UDiv(..)
        | Inst::SRem(..)
        | Inst::URem(..)
        | Inst::SExt(..)
        | Inst::ZExt(..)
        | Inst::Trunc(..)
        | Inst::ICmp(..)
        | Inst::PtrNull(_)
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::PtrOffset { .. }
        | Inst::Alloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. }
        | Inst::Br(..)
        | Inst::CondBr(..)
        | Inst::Ret(_)
        | Inst::Trap(_)
        | Inst::UbAssume(..) => {}
        Inst::Load { result, .. } => {
            if let Some(detail) = unsupported_access(&result.ty) {
                report(E4009, block, index, detail);
            }
        }
        Inst::Store { value, .. } => {
            if let Some(detail) = unsupported_access(&value.ty) {
                report(E4009, block, index, detail);
            }
        }
        Inst::Copy { .. } => {}
        Inst::Call {
            results,
            args,
            callee,
            ..
        } => {
            if args.len() > MAX_REGISTER_ARGUMENTS {
                report(
                    E4010,
                    block,
                    index,
                    format!("a call to @{callee} passes {} arguments", args.len()),
                );
            }
            if results.len() > MAX_REGISTER_RESULTS {
                report(
                    E4013,
                    block,
                    index,
                    format!("a call to @{callee} takes {} results", results.len()),
                );
            }
            if !module
                .functions
                .iter()
                .any(|function| &function.name == callee)
            {
                report(E4009, block, index, format!("a call to absent @{callee}"));
            }
        }
        Inst::IndirectCall { results, args, .. } => {
            if args.len() > MAX_REGISTER_ARGUMENTS {
                report(
                    E4010,
                    block,
                    index,
                    format!("an indirect call passes {} arguments", args.len()),
                );
            }
            if results.len() > MAX_REGISTER_RESULTS {
                report(
                    E4013,
                    block,
                    index,
                    format!("an indirect call takes {} results", results.len()),
                );
            }
        }
        // Revision 0.2 gives every undefined-behaviour predicate a row, and
        // they "lower whether or not a `ub.assume` consumes them".
        Inst::DivisorNonZero(..)
        | Inst::SignedDivRemNoOverflow(..)
        | Inst::SignedAddDefined(..)
        | Inst::SignedSubDefined(..)
        | Inst::SignedMulDefined(..)
        | Inst::ShiftCountDefined { .. }
        | Inst::SignedShlDefined { .. } => {}
    }
}

/// Section 5 lowers a load or a store to "`ldr`/`str` of the matching width".
/// A scalar whose representation is not one of the machine's four widths has
/// no matching width.
fn unsupported_access(ty: &EvoIRType) -> Option<String> {
    match ty {
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => None,
        EvoIRType::Bits(width) if matches!(width.div_ceil(8), 1 | 2 | 4 | 8) => None,
        other => Some(format!("an access of type {other:?} has no matching width")),
    }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

struct Lowerer {
    next_vreg: u32,
    next_block: u32,
    map: HashMap<u32, VReg>,
    blocks: Vec<MBlock>,
    traps: Vec<(u16, u32)>,
}

impl Lowerer {
    fn fresh(&mut self, class: RegClass) -> VReg {
        let reg = VReg::new(self.next_vreg, class);
        self.next_vreg += 1;
        reg
    }

    fn fresh_block(&mut self) -> u32 {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    fn reg(&self, value: &Value) -> Result<VReg, String> {
        self.map
            .get(&value.id)
            .copied()
            .ok_or_else(|| format!("v{} has no register", value.id))
    }

    fn bind(&mut self, value: &Value, reg: VReg) {
        self.map.insert(value.id, reg);
    }

    /// The block holding `brk` for one reason, created on first use.
    fn trap_block(&mut self, imm: u16) -> u32 {
        if let Some((_, id)) = self.traps.iter().find(|(reason, _)| *reason == imm) {
            return *id;
        }
        let id = self.fresh_block();
        self.traps.push((imm, id));
        id
    }

    /// Section 3: bring a value of `bits` significant bits into a register of
    /// `width` zero-extended, "`and` with the mask `2^N - 1` (`uxtb`/`uxth`
    /// when `N` is 8 or 16)".
    fn zero_extend(&mut self, insts: &mut Vec<MInst>, src: VReg, bits: u16, width: Width) -> VReg {
        if bits >= width.bits() {
            return src;
        }
        let class = if width == Width::X {
            RegClass::Gpr64
        } else {
            RegClass::Gpr32
        };
        let dst = self.fresh(class);
        insts.push(match bits {
            8 => MInst::Extend {
                op: ExtOp::Uxtb,
                dst,
                src,
            },
            16 => MInst::Extend {
                op: ExtOp::Uxth,
                dst,
                src,
            },
            _ => MInst::BinaryImm {
                op: BinOp::And,
                dst,
                lhs: src,
                imm: (1_u64 << bits) - 1,
                width,
            },
        });
        dst
    }

    /// Section 3: the same, sign-extended, "`sbfx` from bit `N - 1`
    /// (`sxtb`/`sxth` when `N` is 8 or 16)".
    fn sign_extend(&mut self, insts: &mut Vec<MInst>, src: VReg, bits: u16, width: Width) -> VReg {
        if bits >= width.bits() {
            return src;
        }
        let class = if width == Width::X {
            RegClass::Gpr64
        } else {
            RegClass::Gpr32
        };
        let dst = self.fresh(class);
        insts.push(match (bits, width) {
            (8, _) => MInst::Extend {
                op: ExtOp::Sxtb,
                dst,
                src,
            },
            (16, _) => MInst::Extend {
                op: ExtOp::Sxth,
                dst,
                src,
            },
            (32, Width::X) => MInst::Extend {
                op: ExtOp::Sxtw,
                dst,
                src,
            },
            _ => MInst::Bfx {
                signed: true,
                dst,
                src,
                lsb: 0,
                bits,
                width,
            },
        });
        dst
    }

    /// `cmp` of two registers followed by the `cset` its condition names, the
    /// shape every boolean row of Section 5 ends in.
    fn compare_into(
        &mut self,
        insts: &mut Vec<MInst>,
        lhs: VReg,
        rhs: VReg,
        width: Width,
        cond: Cond,
    ) -> VReg {
        let flags = self.fresh(RegClass::Flags);
        insts.push(MInst::Cmp {
            dst: flags,
            lhs,
            rhs,
            width,
        });
        let dst = self.fresh(RegClass::Gpr32);
        insts.push(MInst::Cset { dst, flags, cond });
        dst
    }

    /// The conjunction of two `bits<1>` values, each of which `cset` wrote as
    /// zero or one into a whole register.
    fn both(&mut self, insts: &mut Vec<MInst>, lhs: VReg, rhs: VReg) -> VReg {
        let dst = self.fresh(RegClass::Gpr32);
        insts.push(MInst::Binary {
            op: BinOp::And,
            dst,
            lhs,
            rhs,
            width: Width::W,
        });
        dst
    }

    /// Whether a register holding an exact signed value is representable in
    /// `bits`: its low `bits` bits, sign-extended, are the value itself.
    fn representable_in(
        &mut self,
        insts: &mut Vec<MInst>,
        value: VReg,
        bits: u16,
        width: Width,
    ) -> VReg {
        if bits >= width.bits() {
            let one = self.fresh(RegClass::Gpr32);
            insts.push(MInst::MovImm { dst: one, imm: 1 });
            return one;
        }
        let class = if width == Width::X {
            RegClass::Gpr64
        } else {
            RegClass::Gpr32
        };
        let truncated = self.fresh(class);
        insts.push(MInst::Bfx {
            signed: true,
            dst: truncated,
            src: value,
            lsb: 0,
            bits,
            width,
        });
        self.compare_into(insts, value, truncated, width, Cond::Eq)
    }

    /// Section 5: "zero-extend `%amt` per Section 3, `cmp` with `#N`, `cset
    /// lo`", with the extension taken from the declared signedness.
    ///
    /// EvoIR Section 3.3.1 decodes the amount "at width `M` with the declared
    /// source signedness" and is true only for counts in `[0, N)`. A negative
    /// signed amount of width `M` below the register width is not negative once
    /// zero-extended, so a one-bit signed amount of 1, which EvoIR decodes as
    /// -1, would compare below `N` and answer true. Extending by the declared
    /// signedness makes every negative amount compare above `N` unsigned.
    /// Recorded gap.
    fn count_defined(
        &mut self,
        insts: &mut Vec<MInst>,
        shifted: &Value,
        amount: &Value,
        amount_signed: bool,
    ) -> Result<VReg, String> {
        let bound = significant_bits(&shifted.ty);
        let bits = significant_bits(&amount.ty);
        let class = class_of(&amount.ty).map_err(|(code, _)| code.to_owned())?;
        let width = width_of(class);
        let raw = self.reg(amount)?;
        let extended = if amount_signed {
            self.sign_extend(insts, raw, bits, width)
        } else {
            self.zero_extend(insts, raw, bits, width)
        };
        let flags = self.fresh(RegClass::Flags);
        insts.push(MInst::CmpImm {
            dst: flags,
            lhs: extended,
            imm: u64::from(bound),
            width,
        });
        let dst = self.fresh(RegClass::Gpr32);
        insts.push(MInst::Cset {
            dst,
            flags,
            cond: Cond::Lo,
        });
        Ok(dst)
    }

    /// Section 3: "shift amount operand `bits<M>` | the low `M` bits are the
    /// amount | `and` with `2^M - 1` when `M` is below the register width".
    fn shift_amount(&mut self, insts: &mut Vec<MInst>, amount: &Value) -> Result<VReg, String> {
        let reg = self.reg(amount)?;
        let bits = significant_bits(&amount.ty);
        if bits >= width_of(reg.class).bits() {
            return Ok(reg);
        }
        let dst = self.fresh(reg.class);
        insts.push(MInst::BinaryImm {
            op: BinOp::And,
            dst,
            lhs: reg,
            imm: (1_u64 << bits) - 1,
            width: width_of(reg.class),
        });
        Ok(dst)
    }
}

fn lower_function(function: &Function) -> Result<MFunction, String> {
    let entry = function
        .blocks
        .first()
        .ok_or_else(|| format!("@{} has no entry block", function.name))?;
    let order: Vec<u32> = function.blocks.iter().map(|block| block.id).collect();
    let ids: HashMap<u32, u32> = order
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, u32::try_from(index).unwrap_or(u32::MAX)))
        .collect();
    let _ = entry;

    let mut lowerer = Lowerer {
        next_vreg: 0,
        next_block: u32::try_from(function.blocks.len()).unwrap_or(u32::MAX),
        map: HashMap::new(),
        blocks: Vec::new(),
        traps: Vec::new(),
    };

    for block in &function.blocks {
        let id = ids[&block.id];
        lower_block(&mut lowerer, function, block, id, &ids)?;
    }
    for (imm, id) in lowerer.traps.clone() {
        lowerer.blocks.push(MBlock {
            id,
            args: Vec::new(),
            insts: Vec::new(),
            term: MTerm::Brk { imm },
        });
    }
    Ok(MFunction {
        name: function.name.clone(),
        blocks: lowerer.blocks,
    })
}

/// The register-typed values an edge passes.
fn edge_args(lowerer: &Lowerer, values: &[Value]) -> Result<Vec<VReg>, String> {
    values
        .iter()
        .filter(|value| is_register_type(&value.ty))
        .map(|value| lowerer.reg(value))
        .collect()
}

fn lower_block(
    lowerer: &mut Lowerer,
    function: &Function,
    block: &Block,
    id: u32,
    ids: &HashMap<u32, u32>,
) -> Result<(), String> {
    let mut args = Vec::new();
    for arg in &block.args {
        if !is_register_type(&arg.value.ty) {
            continue;
        }
        let class = class_of(&arg.value.ty).map_err(|(code, _)| code.to_owned())?;
        let reg = lowerer.fresh(class);
        lowerer.bind(&arg.value, reg);
        args.push(reg);
    }

    let mut current_id = id;
    let mut current_args = args;
    let mut insts: Vec<MInst> = Vec::new();
    // Set when an `icmp` was fused into the block's `cond_br`.
    let mut fused: Option<(VReg, Cond)> = None;

    for (index, inst) in block.insts.iter().enumerate() {
        if let Inst::UbAssume(condition, reason) = inst {
            // Section 5, checked profile: "tbz %c, #0, trap_label where the
            // label holds brk #imm for the reason". The instructions after the
            // obligation become a continuation block, because a terminator ends
            // a block and a block's arguments are its only inputs.
            let condition = lowerer.reg(condition)?;
            let imm = ub_immediate(*reason);
            let trap = lowerer.trap_block(imm);
            let carried = live_across(lowerer, block, index + 1);
            let continuation = lowerer.fresh_block();
            let mut continuation_args = Vec::new();
            let mut rebind = HashMap::new();
            for reg in &carried {
                let fresh = lowerer.fresh(reg.class);
                continuation_args.push(fresh);
                rebind.insert(reg.id, fresh);
            }
            lowerer.blocks.push(MBlock {
                id: current_id,
                args: std::mem::take(&mut current_args),
                insts: std::mem::take(&mut insts),
                term: MTerm::Tb {
                    nonzero: false,
                    src: condition,
                    bit: 0,
                    taken: trap,
                    taken_args: Vec::new(),
                    untaken: continuation,
                    untaken_args: carried,
                },
            });
            for reg in lowerer.map.values_mut() {
                if let Some(fresh) = rebind.get(&reg.id) {
                    *reg = *fresh;
                }
            }
            current_id = continuation;
            current_args = continuation_args;
            continue;
        }

        // Section 5: an `icmp` "when the only consumer is a cond_br in the same
        // block" becomes `cmp` then `b.cond` with no `cset`. The fusion is
        // taken only when the comparison is the block's last instruction,
        // because Section 2 requires the `cmp` and its consumer to be adjacent.
        if let Inst::ICmp(result, operation, lhs, rhs) = inst {
            if index + 1 == block.insts.len() {
                if let Inst::CondBr(condition, ..) = &block.terminator {
                    if condition.id == result.id && uses_of(block, result.id) == 1 {
                        let (flags, cond) =
                            lower_compare(lowerer, &mut insts, *operation, lhs, rhs)?;
                        fused = Some((flags, cond));
                        continue;
                    }
                }
            }
        }

        lower_inst(lowerer, &mut insts, inst)?;
    }

    let term = lower_terminator(lowerer, function, block, ids, fused)?;
    lowerer.blocks.push(MBlock {
        id: current_id,
        args: current_args,
        insts,
        term,
    });
    Ok(())
}

/// How many times `value` is read in `block`, terminator included.
fn uses_of(block: &Block, value: u32) -> usize {
    block
        .insts
        .iter()
        .chain(std::iter::once(&block.terminator))
        .flat_map(inst_uses)
        .filter(|used| used.id == value)
        .count()
}

/// The registers a continuation block must receive: everything already bound
/// that instruction `from` onwards still reads.
fn live_across(lowerer: &Lowerer, block: &Block, from: usize) -> Vec<VReg> {
    let mut needed: HashSet<u32> = HashSet::new();
    for inst in block.insts[from..]
        .iter()
        .chain(std::iter::once(&block.terminator))
    {
        for value in inst_uses(inst) {
            needed.insert(value.id);
        }
    }
    let mut carried: BTreeSet<VReg> = BTreeSet::new();
    for value in needed {
        if let Some(reg) = lowerer.map.get(&value) {
            carried.insert(*reg);
        }
    }
    carried.into_iter().collect()
}

fn condition_of(operation: CmpOp) -> Cond {
    match operation {
        CmpOp::Eq => Cond::Eq,
        CmpOp::Ne => Cond::Ne,
        CmpOp::SLt => Cond::Lt,
        CmpOp::SLe => Cond::Le,
        CmpOp::ULt => Cond::Lo,
        CmpOp::ULe => Cond::Ls,
    }
}

/// Emit the `cmp` of an EvoIR comparison and name the condition it sets.
///
/// Section 3's table puts `icmp.eq` and `icmp.ne` in the row that needs no
/// inserted extension, "the low `N` bits of the result depend only on the low
/// `N` bits of the operands". That sentence is true of `add` and false of a
/// comparison: `trunc` inserts nothing, so a `bits<3>` may carry any bits above
/// bit 2, and a 32-bit `cmp` reads all of them. The two readings are
/// distinguished by comparing a truncated value with a constant of the same
/// low bits. The reading implemented here is the one under which the EvoMIR
/// "faithfully implements the abstract semantics of that EClass for all defined
/// executions" (EvoIR Section 10.2, which EvoMIR Section 1 restates and which
/// wins where the two meet): equality is given the same zero-extension as
/// `icmp.ult`. Recorded gap.
fn lower_compare(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    operation: CmpOp,
    lhs: &Value,
    rhs: &Value,
) -> Result<(VReg, Cond), String> {
    let left = lowerer.reg(lhs)?;
    let right = lowerer.reg(rhs)?;
    let cond = condition_of(operation);
    let (left, right, width) = if matches!(lhs.ty, EvoIRType::Bits(_)) {
        let bits = significant_bits(&lhs.ty);
        let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
        let width = width_of(class);
        let (left, right) = match operation {
            CmpOp::SLt | CmpOp::SLe => (
                lowerer.sign_extend(insts, left, bits, width),
                lowerer.sign_extend(insts, right, bits, width),
            ),
            _ => (
                lowerer.zero_extend(insts, left, bits, width),
                lowerer.zero_extend(insts, right, bits, width),
            ),
        };
        (left, right, width)
    } else {
        (left, right, Width::X)
    };
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::Cmp {
        dst: flags,
        lhs: left,
        rhs: right,
        width,
    });
    Ok((flags, cond))
}

fn lower_terminator(
    lowerer: &mut Lowerer,
    function: &Function,
    block: &Block,
    ids: &HashMap<u32, u32>,
    fused: Option<(VReg, Cond)>,
) -> Result<MTerm, String> {
    match &block.terminator {
        Inst::Br(target, args) => Ok(MTerm::B {
            target: *ids
                .get(target)
                .ok_or_else(|| format!("branch to missing block {target}"))?,
            args: edge_args(lowerer, args)?,
        }),
        Inst::CondBr(condition, taken, taken_args, untaken, untaken_args) => {
            let taken_id = *ids
                .get(taken)
                .ok_or_else(|| format!("branch to missing block {taken}"))?;
            let untaken_id = *ids
                .get(untaken)
                .ok_or_else(|| format!("branch to missing block {untaken}"))?;
            let taken_args = edge_args(lowerer, taken_args)?;
            let untaken_args = edge_args(lowerer, untaken_args)?;
            if let Some((flags, cond)) = fused {
                return Ok(MTerm::BCond {
                    flags,
                    cond,
                    taken: taken_id,
                    taken_args,
                    untaken: untaken_id,
                    untaken_args,
                });
            }
            // Section 3: "cond_br on bits<1> ... nothing: tbnz/tbz on bit 0".
            Ok(MTerm::Tb {
                nonzero: true,
                src: lowerer.reg(condition)?,
                bit: 0,
                taken: taken_id,
                taken_args,
                untaken: untaken_id,
                untaken_args,
            })
        }
        Inst::Ret(values) => {
            let _ = function;
            Ok(MTerm::Ret {
                values: edge_args(lowerer, values)?,
            })
        }
        Inst::Trap(reason) => Ok(MTerm::Brk {
            imm: trap_immediate(*reason),
        }),
        other => Err(format!("{other:?} is not a terminator")),
    }
}

#[allow(clippy::too_many_lines)]
fn lower_inst(lowerer: &mut Lowerer, insts: &mut Vec<MInst>, inst: &Inst) -> Result<(), String> {
    match inst {
        Inst::Const(result, bits) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst, imm: *bits });
            lowerer.bind(result, dst);
        }
        // Section 5: "any value is permitted; zero is chosen so the output is
        // deterministic".
        Inst::Uninitialized(result) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst, imm: 0 });
            lowerer.bind(result, dst);
        }
        Inst::Add(result, lhs, rhs) => simple(lowerer, insts, BinOp::Add, result, lhs, rhs)?,
        Inst::Sub(result, lhs, rhs) => simple(lowerer, insts, BinOp::Sub, result, lhs, rhs)?,
        Inst::Mul(result, lhs, rhs) => simple(lowerer, insts, BinOp::Mul, result, lhs, rhs)?,
        Inst::And(result, lhs, rhs) => simple(lowerer, insts, BinOp::And, result, lhs, rhs)?,
        Inst::Or(result, lhs, rhs) => simple(lowerer, insts, BinOp::Orr, result, lhs, rhs)?,
        Inst::Xor(result, lhs, rhs) => simple(lowerer, insts, BinOp::Eor, result, lhs, rhs)?,
        Inst::Shl(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let value = lowerer.reg(lhs)?;
            let amount = lowerer.shift_amount(insts, rhs)?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Lsl,
                dst,
                lhs: value,
                rhs: amount,
                width,
            });
            lowerer.bind(result, dst);
        }
        Inst::LShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.zero_extend(insts, value, bits, width);
            let amount = lowerer.shift_amount(insts, rhs)?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Lsr,
                dst,
                lhs: value,
                rhs: amount,
                width,
            });
            lowerer.bind(result, dst);
        }
        Inst::AShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.sign_extend(insts, value, bits, width);
            let amount = lowerer.shift_amount(insts, rhs)?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Asr,
                dst,
                lhs: value,
                rhs: amount,
                width,
            });
            lowerer.bind(result, dst);
        }
        Inst::SDiv(result, lhs, rhs) => divide(lowerer, insts, true, false, result, lhs, rhs)?,
        Inst::UDiv(result, lhs, rhs) => divide(lowerer, insts, false, false, result, lhs, rhs)?,
        Inst::SRem(result, lhs, rhs) => divide(lowerer, insts, true, true, result, lhs, rhs)?,
        Inst::URem(result, lhs, rhs) => divide(lowerer, insts, false, true, result, lhs, rhs)?,
        Inst::SExt(result, source, _) | Inst::ZExt(result, source, _) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&source.ty);
            let src = lowerer.reg(source)?;
            let extended = if matches!(inst, Inst::SExt(..)) {
                lowerer.sign_extend(insts, src, bits, width)
            } else {
                lowerer.zero_extend(insts, src, bits, width)
            };
            let extended = if extended.class == class {
                extended
            } else {
                let dst = lowerer.fresh(class);
                insts.push(MInst::Mov { dst, src: extended });
                dst
            };
            lowerer.bind(result, extended);
        }
        // Section 5: "the extension of Section 3, or nothing for `trunc`". A
        // narrowing that also changes class needs a register of the other
        // class, which on the machine is the same register named at the other
        // width; the move is that renaming and nothing else.
        Inst::Trunc(result, source, _) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let src = lowerer.reg(source)?;
            if src.class == class {
                lowerer.bind(result, src);
            } else {
                let dst = lowerer.fresh(class);
                insts.push(MInst::Mov { dst, src });
                lowerer.bind(result, dst);
            }
        }
        Inst::ICmp(result, operation, lhs, rhs) => {
            let (flags, cond) = lower_compare(lowerer, insts, *operation, lhs, rhs)?;
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Cset { dst, flags, cond });
            lowerer.bind(result, dst);
        }
        Inst::PtrNull(result) => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::MovImm { dst, imm: 0 });
            lowerer.bind(result, dst);
        }
        Inst::GlobalAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Global);
        }
        Inst::FuncAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Function);
        }
        Inst::PtrOffset {
            result,
            base,
            index,
            direction,
            signedness,
            scale,
        } => {
            let base = lowerer.reg(base)?;
            let bits = significant_bits(&index.ty);
            let raw = lowerer.reg(index)?;
            // EvoIR Section 3.7 "decodes %index according to the mnemonic",
            // so the extension is the one the signedness names.
            let extended = match signedness {
                evocc_evoir::IndexSignedness::Signed => {
                    lowerer.sign_extend(insts, raw, bits, Width::X)
                }
                evocc_evoir::IndexSignedness::Unsigned => {
                    lowerer.zero_extend(insts, raw, bits, Width::X)
                }
            };
            let extended = if extended.class == RegClass::Gpr64 {
                extended
            } else {
                let dst = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Mov { dst, src: extended });
                dst
            };
            // Section 5: "scale by `lsl` first when the scale is a power of
            // two, `mul` otherwise".
            let delta = if *scale == 1 {
                extended
            } else if scale.is_power_of_two() {
                let dst = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::BinaryImm {
                    op: BinOp::Lsl,
                    dst,
                    lhs: extended,
                    imm: u64::from(scale.trailing_zeros()),
                    width: Width::X,
                });
                dst
            } else {
                let factor = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::MovImm {
                    dst: factor,
                    imm: *scale,
                });
                let dst = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Binary {
                    op: BinOp::Mul,
                    dst,
                    lhs: extended,
                    rhs: factor,
                    width: Width::X,
                });
                dst
            };
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Binary {
                op: match direction {
                    evocc_evoir::OffsetDirection::Add => BinOp::Add,
                    evocc_evoir::OffsetDirection::Subtract => BinOp::Sub,
                },
                dst,
                lhs: base,
                rhs: delta,
                width: Width::X,
            });
            lowerer.bind(result, dst);
        }
        Inst::Alloca {
            result,
            origin,
            layout,
            ..
        } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameSlot {
                dst,
                origin: *origin,
                size: layout.size_bytes,
                align: layout.align_bytes,
            });
            lowerer.bind(result, dst);
        }
        // Section 5: `lifetime.end` lowers to nothing, and `pointer_valid` and
        // `access_in_bounds` are "the guards the front end already lowered into
        // ub.assume obligations; the instruction itself has no machine effect".
        Inst::LifetimeEnd { .. } | Inst::PointerValid { .. } | Inst::AccessInBounds { .. } => {}
        Inst::Load {
            result,
            address,
            volatile,
            ..
        } => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let kind = load_kind(&result.ty)?;
            let base = lowerer.reg(address)?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Load {
                dst,
                base,
                offset: 0,
                kind,
                volatile: *volatile,
            });
            lowerer.bind(result, dst);
        }
        Inst::Store {
            address,
            value,
            volatile,
            ..
        } => {
            let kind = store_kind(&value.ty)?;
            let base = lowerer.reg(address)?;
            let src = lowerer.reg(value)?;
            insts.push(MInst::Store {
                src,
                base,
                offset: 0,
                kind,
                volatile: *volatile,
            });
        }
        // Section 5: `copy` with static size lowers to `mem.copy`, the
        // pseudo-form that transfers per-byte initializedness and pointer
        // fragments; emission expands it.
        Inst::Copy {
            destination,
            source,
            layout,
            ..
        } => {
            let dst = lowerer.reg(destination)?;
            let src = lowerer.reg(source)?;
            insts.push(MInst::MemCopy {
                dst,
                src,
                size: layout.size_bytes,
                align: layout.align_bytes,
            });
        }
        Inst::Call {
            results,
            callee,
            args,
            ..
        } => {
            let arguments = edge_args(lowerer, args)?;
            let outputs = call_results(lowerer, results)?;
            insts.push(MInst::Bl {
                symbol: callee.clone(),
                args: arguments,
                results: outputs,
            });
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            ..
        } => {
            let target = lowerer.reg(callee)?;
            let arguments = edge_args(lowerer, args)?;
            let outputs = call_results(lowerer, results)?;
            insts.push(MInst::Blr {
                target,
                args: arguments,
                results: outputs,
            });
        }
        // --- the undefined-behaviour predicates of Section 5 ----------------
        //
        // Every row below is the specification's, with one generalization the
        // specification's own technique dictates: a row written for a value at
        // the register width tests representability in the register, and EvoIR
        // asks for representability in `bits<N>`. Where `N` is narrower the
        // check is closed with the bitfield comparison Section 5 already uses
        // in the `signed_mul_defined` row ("compare with the sign extension of
        // its low 32 bits"), read at `N` instead of 32. Two further departures
        // are recorded with the gap they answer, at the row that makes them.
        Inst::DivisorNonZero(result, divisor) => {
            let bits = significant_bits(&divisor.ty);
            let class = class_of(&divisor.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let source = lowerer.reg(divisor)?;
            // `cmp` reads the whole register, so Section 3's zero-extension is
            // required here exactly as it is for `icmp.eq`. Section 5's row
            // does not say so; recorded gap.
            let source = lowerer.zero_extend(insts, source, bits, width);
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: source,
                imm: 0,
                width,
            });
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Cset {
                dst,
                flags,
                cond: Cond::Ne,
            });
            lowerer.bind(result, dst);
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) => {
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            // Section 5: "sign-extend both per Section 3; `cmp %a, #INT_MIN_N`
            // (the minimum of `bits<N>`, materialized), `cset ne` into `%p`;
            // `cmp %b, #-1`, `cset ne` into `%q`; `orr %p, %q`".
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let minimum = lowerer.fresh(class);
            insts.push(MInst::MovImm {
                dst: minimum,
                imm: (1_u64 << (bits - 1)).wrapping_neg() & width.mask(),
            });
            let low = lowerer.compare_into(insts, left, minimum, width, Cond::Ne);
            let ones = lowerer.fresh(class);
            insts.push(MInst::MovImm {
                dst: ones,
                imm: width.mask(),
            });
            let high = lowerer.compare_into(insts, right, ones, width, Cond::Ne);
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Binary {
                op: BinOp::Orr,
                dst,
                lhs: low,
                rhs: high,
                width: Width::W,
            });
            lowerer.bind(result, dst);
        }
        Inst::SignedAddDefined(result, lhs, rhs) | Inst::SignedSubDefined(result, lhs, rhs) => {
            let subtract = matches!(inst, Inst::SignedSubDefined(..));
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let dst = if bits == width.bits() {
                // Section 5: "`adds`/`subs` into a scratch, then `cset vc`".
                let scratch = lowerer.fresh(class);
                let flags = lowerer.fresh(RegClass::Flags);
                insts.push(MInst::FlagBinary {
                    op: if subtract { FlagOp::Subs } else { FlagOp::Adds },
                    dst: scratch,
                    flags,
                    lhs: left,
                    rhs: right,
                    width,
                });
                let dst = lowerer.fresh(RegClass::Gpr32);
                insts.push(MInst::Cset {
                    dst,
                    flags,
                    cond: Cond::Vc,
                });
                dst
            } else {
                // The `v` flag reports overflow of the register, not of
                // `bits<N>`: sign-extended narrow operands never set it, so
                // the row answers "defined" where EvoIR answers "not". The sum
                // is exact at the register width, and representability in `N`
                // bits is the bitfield comparison. Recorded gap.
                let sum = lowerer.fresh(class);
                insts.push(MInst::Binary {
                    op: if subtract { BinOp::Sub } else { BinOp::Add },
                    dst: sum,
                    lhs: left,
                    rhs: right,
                    width,
                });
                lowerer.representable_in(insts, sum, bits, width)
            };
            lowerer.bind(result, dst);
        }
        Inst::SignedMulDefined(result, lhs, rhs) => {
            let bits = significant_bits(&lhs.ty);
            let class = class_of(&lhs.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let left = lowerer.reg(lhs)?;
            let right = lowerer.reg(rhs)?;
            let left = lowerer.sign_extend(insts, left, bits, width);
            let right = lowerer.sign_extend(insts, right, bits, width);
            let dst = if width == Width::W {
                // Section 5, 32-bit: "`smull` the sign-extended operands,
                // compare with the sign extension of its low 32 bits". Read at
                // `N` rather than at 32, which is the same instruction and the
                // same comparison; the product of two values of at most 32
                // significant bits is exact in the 64-bit result.
                let product = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::WideMultiply {
                    op: WideMul::Smull,
                    dst: product,
                    lhs: left,
                    rhs: right,
                });
                lowerer.representable_in(insts, product, bits, Width::X)
            } else {
                // Section 5, 64-bit: "`mul` and `smulh`, compare the high word
                // with `asr` of the low word by 63".
                let low = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Binary {
                    op: BinOp::Mul,
                    dst: low,
                    lhs: left,
                    rhs: right,
                    width: Width::X,
                });
                let high = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::WideMultiply {
                    op: WideMul::Smulh,
                    dst: high,
                    lhs: left,
                    rhs: right,
                });
                let sign = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::BinaryImm {
                    op: BinOp::Asr,
                    dst: sign,
                    lhs: low,
                    imm: 63,
                    width: Width::X,
                });
                let exact = lowerer.compare_into(insts, high, sign, Width::X, Cond::Eq);
                if bits == 64 {
                    exact
                } else {
                    let fits = lowerer.representable_in(insts, low, bits, Width::X);
                    lowerer.both(insts, exact, fits)
                }
            };
            lowerer.bind(result, dst);
        }
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let dst = lowerer.count_defined(insts, shifted, amount, *amount_signed)?;
            lowerer.bind(result, dst);
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } => {
            let count_ok = lowerer.count_defined(insts, shifted, amount, *amount_signed)?;
            let bits = significant_bits(&shifted.ty);
            let class = class_of(&shifted.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let value = lowerer.reg(shifted)?;
            let value = lowerer.sign_extend(insts, value, bits, width);
            // EvoIR Section 3.3.1 makes the predicate "false for an invalid
            // count, a negative left operand, or a shifted result not
            // representable in signed bits<N>". Section 5's row states the
            // count and the round trip and omits the negative operand;
            // recorded gap.
            let zero = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst: zero, imm: 0 });
            let non_negative = lowerer.compare_into(insts, value, zero, width, Cond::Ge);
            let amount_reg = lowerer.shift_amount(insts, amount)?;
            let shifted_left = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Lsl,
                dst: shifted_left,
                lhs: value,
                rhs: amount_reg,
                width,
            });
            let back = lowerer.fresh(class);
            insts.push(MInst::Binary {
                op: BinOp::Asr,
                dst: back,
                lhs: shifted_left,
                rhs: amount_reg,
                width,
            });
            // Section 5: "(x << amt) >> amt == x with an arithmetic right
            // shift".
            let round_trip = lowerer.compare_into(insts, back, value, width, Cond::Eq);
            let mut dst = lowerer.both(insts, count_ok, non_negative);
            dst = lowerer.both(insts, dst, round_trip);
            if bits < width.bits() {
                // The round trip tests representability in the register, not
                // in bits<N>; the same recorded gap as the additive rows.
                let fits = lowerer.representable_in(insts, shifted_left, bits, width);
                dst = lowerer.both(insts, dst, fits);
            }
            lowerer.bind(result, dst);
        }
        other => return Err(format!("{other:?} has no row in Section 5")),
    }
    Ok(())
}

fn call_results(lowerer: &mut Lowerer, results: &[Value]) -> Result<Vec<VReg>, String> {
    let mut outputs = Vec::new();
    for value in results.iter().filter(|value| is_register_type(&value.ty)) {
        let class = class_of(&value.ty).map_err(|(code, _)| code.to_owned())?;
        let reg = lowerer.fresh(class);
        lowerer.bind(value, reg);
        outputs.push(reg);
    }
    if outputs.len() > MAX_REGISTER_RESULTS {
        return Err("a call with more than eight ordinary results".to_owned());
    }
    Ok(outputs)
}

fn address_of(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    result: &Value,
    symbol: &str,
    kind: SymbolKind,
) {
    let page = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::Adrp {
        dst: page,
        symbol: symbol.to_owned(),
        kind,
    });
    let dst = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::AddPageOff {
        dst,
        src: page,
        symbol: symbol.to_owned(),
        kind,
    });
    lowerer.bind(result, dst);
}

fn load_kind(ty: &EvoIRType) -> Result<LoadKind, String> {
    Ok(match ty {
        EvoIRType::Ptr(_) => LoadKind::P,
        EvoIRType::FnPtr { .. } => LoadKind::Pf,
        EvoIRType::Bits(width) => match width.div_ceil(8) {
            1 => LoadKind::B,
            2 => LoadKind::H,
            4 => LoadKind::W,
            8 => LoadKind::X,
            _ => return Err("a load with no matching width".to_owned()),
        },
        other => return Err(format!("a load of {other:?}")),
    })
}

fn store_kind(ty: &EvoIRType) -> Result<StoreKind, String> {
    Ok(match ty {
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => StoreKind::P,
        EvoIRType::Bits(width) => match width.div_ceil(8) {
            1 => StoreKind::B,
            2 => StoreKind::H,
            4 => StoreKind::W,
            8 => StoreKind::X,
            _ => return Err("a store with no matching width".to_owned()),
        },
        other => return Err(format!("a store of {other:?}")),
    })
}

/// Section 3's first row: "none: the low `N` bits of the result depend only on
/// the low `N` bits of the operands".
fn simple(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    op: BinOp,
    result: &Value,
    lhs: &Value,
    rhs: &Value,
) -> Result<(), String> {
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let left = lowerer.reg(lhs)?;
    let right = lowerer.reg(rhs)?;
    let dst = lowerer.fresh(class);
    insts.push(MInst::Binary {
        op,
        dst,
        lhs: left,
        rhs: right,
        width: width_of(class),
    });
    lowerer.bind(result, dst);
    Ok(())
}

/// Section 5: "`sdiv`/`udiv`, remainders by `msub`".
fn divide(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    is_signed: bool,
    remainder: bool,
    result: &Value,
    lhs: &Value,
    rhs: &Value,
) -> Result<(), String> {
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let width = width_of(class);
    let bits = significant_bits(&result.ty);
    let left = lowerer.reg(lhs)?;
    let right = lowerer.reg(rhs)?;
    let (left, right) = if is_signed {
        (
            lowerer.sign_extend(insts, left, bits, width),
            lowerer.sign_extend(insts, right, bits, width),
        )
    } else {
        (
            lowerer.zero_extend(insts, left, bits, width),
            lowerer.zero_extend(insts, right, bits, width),
        )
    };
    let quotient = lowerer.fresh(class);
    insts.push(MInst::Binary {
        op: if is_signed { BinOp::SDiv } else { BinOp::UDiv },
        dst: quotient,
        lhs: left,
        rhs: right,
        width,
    });
    if remainder {
        let dst = lowerer.fresh(class);
        insts.push(MInst::Msub {
            dst,
            n: quotient,
            m: right,
            a: left,
            width,
        });
        lowerer.bind(result, dst);
    } else {
        lowerer.bind(result, quotient);
    }
    Ok(())
}
