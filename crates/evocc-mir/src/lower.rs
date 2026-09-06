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
    inst_results, inst_uses, BitOp, Block, CmpOp, Function, InlineAssembly, Inst, Module,
    TrapReason, UbReason, Value, VariadicOp,
};
use evocc_types::EvoIRType;

use crate::{
    AsmBody, AsmClobber, AsmConstraint, AsmOperand, AsmValue, BinOp, CarryOp, Cond, ExtOp, FlagOp,
    FramePlace, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, RegClass, StoreKind,
    SymbolKind, Target, UnaryOp, VReg, WideMul, Width,
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
    lower_module_with(module, &|_| false)
}

/// Lower a module in which `external` names the symbols something other than
/// this module defines.
///
/// EvoMIR Section 6 (revision 0.5): "Across the host boundary the platform rule
/// governs instead: a caller extends an argument narrower than 32 bits to 32
/// bits ... A cross-boundary call therefore inserts the extension the platform
/// requires; a call between EvoCC functions does not."
///
/// Which calls those are is not an EvoIR question: EvoIR 1.9.56 refuses a call
/// to a function the module does not define ("call to missing function"), so
/// every call is between EvoCC functions until emission is told to leave a
/// definition out and let the linker bind the symbol elsewhere. That filter is
/// the only construct in this codebase that makes a boundary, so it is what
/// `external` reports. Recorded gap: the specification states the rule without
/// saying which calls it governs, because the representation cannot yet spell
/// one.
pub fn lower_module_with(
    module: &Module,
    external: &dyn Fn(&str) -> bool,
) -> Result<MModule, Vec<Refusal>> {
    lower_module_for(Target::Darwin, module, external)
}

/// The same, for a named target.
///
/// Section 6 (revision 0.10): on Linux "an argument that finds no register
/// goes to the stack in an 8-byte slot", while "on Darwin the platform packs
/// stack arguments by natural alignment instead; that rule is still
/// unmeasured, so more than eight integer arguments stay `E4010` there". The
/// target is the only thing the two lowerings differ by.
pub fn lower_module_for(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
) -> Result<MModule, Vec<Refusal>> {
    let mut refusals = Vec::new();
    for function in &module.functions {
        scan_function(target, module, function, &mut refusals);
    }
    if !refusals.is_empty() {
        return Err(refusals);
    }
    // Section 8, revision 0.11: "Every such call is cross-boundary in the
    // sense of Section 6, so the platform's extension rule applies to its
    // narrow arguments." A `declare` is exactly a callee whose definition is
    // not emitted beside its caller, which is what Section 6 makes the
    // boundary, so the two ways of naming one are joined here.
    let declared: HashSet<&str> = module
        .declares
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect();
    let crosses = |name: &str| external(name) || declared.contains(name);

    let mut functions = Vec::new();
    for function in &module.functions {
        match lower_function(function, &crosses) {
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
        // Section 2: "A `bits<128>` EvoIR value has no class of its own:
        // lowering represents it as two `gpr64` virtual registers, the low
        // half first." The class named here is each half's.
        EvoIRType::Bits(width) => match width {
            1..=32 => Ok(RegClass::Gpr32),
            33..=64 | 128 => Ok(RegClass::Gpr64),
            _ => Err(E4003),
        },
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => Ok(RegClass::Gpr64),
        EvoIRType::F32 | EvoIRType::F64 => Err(E4001),
        EvoIRType::Vec { .. } => Err(E4002),
        EvoIRType::Tuple(_) => Err(E4004),
        _ => Err(E4009),
    }
}

/// Whether a type is the one Section 2 represents as a register pair.
fn is_pair(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::Bits(128))
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

fn scan_function(
    target: Target,
    module: &Module,
    function: &Function,
    refusals: &mut Vec<Refusal>,
) {
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
    if params.len() > MAX_REGISTER_ARGUMENTS && refuses_stack_arguments(target) {
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
            scan_inst(target, module, inst, block.id, index, &mut report);
        }
        scan_inst(
            target,
            module,
            &block.terminator,
            block.id,
            block.insts.len(),
            &mut report,
        );
    }
}

/// Whether more than eight register arguments are refused rather than lowered.
///
/// Section 10: `E4010` is "more than eight integer arguments on Darwin (Linux
/// lowers stack arguments, revision 0.10)".
const fn refuses_stack_arguments(target: Target) -> bool {
    matches!(target, Target::Darwin)
}

fn scan_inst(
    target: Target,
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

    // Section 10: "at `N = 128`, division, remainder and the signed UB
    // predicates" have no row. The cohort has none of them, and EvoIR keeps
    // division `E3033`.
    if let Some(detail) = unlowered_at_the_widest_scalar(inst) {
        report(E4003, block, index, detail);
    }

    match inst {
        // The Section 5 table, row by row.
        // Section 5's bit-counting and byte-swap row (revision 0.14). EvoIR
        // 3.4 fixes `bswap` at 16, 32 or 64; the counting forms take any `N`
        // the register classes hold, and width 128 has no row.
        Inst::BitOperation { result, op, .. } => {
            if is_pair(&result.ty) {
                report(
                    E4003,
                    block,
                    index,
                    format!("{} at bits<128>", op.mnemonic()),
                );
            } else if *op == BitOp::ByteSwap && !matches!(result.ty, EvoIRType::Bits(16 | 32 | 64))
            {
                report(
                    E4009,
                    block,
                    index,
                    format!("bswap of {:?}, which is not 16, 32 or 64", result.ty),
                );
            }
        }
        // Section 5's three variadic rows (revision 0.10). Only the promoted
        // scalar types of EvoIR 3.10 reach `va_arg`, and the ones the machine
        // has no class for are refused by the operand scan above.
        Inst::VaAccess(access) => {
            if let VariadicOp::Arg(result) = &access.op {
                if variadic_slot(&result.ty).is_none() {
                    report(
                        E4009,
                        block,
                        index,
                        format!("a va_arg of {:?} has no Section 6 procedure", result.ty),
                    );
                }
            }
        }
        // Section 4 carries an `asm` through unchanged. `asm.goto` is a
        // terminator whose successors are its labels, and no unit of the
        // cohort this revision serves reaches one, so it stays refused.
        Inst::Asm(assembly) => {
            if let Err(detail) = assembly_vocabulary(assembly) {
                report(E4009, block, index, detail);
            }
        }
        Inst::AsmGoto { .. } => report(
            E4009,
            block,
            index,
            "an `asm.goto` terminator has no row in Section 5".into(),
        ),
        // Section 5 (revision 0.12): "`ptr.to_int`, `ptr.from_int.*` |
        // nothing: the value is the same 64-bit register".
        Inst::PointerCast { .. }
        | Inst::Const(..)
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
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. }
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
            if args.len() > MAX_REGISTER_ARGUMENTS && refuses_stack_arguments(target) {
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
            // Section 8, revision 0.11: "A `call @f` whose `@f` is an EvoIR
            // `declare` ... is emitted with exactly the spellings above for a
            // defined symbol ... the object file carries an undefined symbol
            // reference and the linker resolves it." A callee that names
            // neither table is still absent.
            if !module
                .functions
                .iter()
                .any(|function| &function.name == callee)
                && !module
                    .declares
                    .iter()
                    .any(|declaration| &declaration.name == callee)
            {
                report(E4009, block, index, format!("a call to absent @{callee}"));
            }
        }
        Inst::IndirectCall { results, args, .. } => {
            if args.len() > MAX_REGISTER_ARGUMENTS && refuses_stack_arguments(target) {
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
        // Sixteen is the pair Section 5 accesses with `ldp`/`stp`.
        EvoIRType::Bits(width) if matches!(width.div_ceil(8), 1 | 2 | 4 | 8 | 16) => None,
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
    /// The high half of every value Section 2 represents as a pair; `map`
    /// holds the low half, which is the half every ordinary row already reads.
    high: HashMap<u32, VReg>,
    blocks: Vec<MBlock>,
    traps: Vec<(u16, u32)>,
    /// Section 6's `k` for a variadic callee: "the number of argument
    /// registers its named parameters consumed". `None` in a fixed function.
    named_registers: Option<u8>,
    /// Every `const` this function has lowered, by value id.
    ///
    /// EvoIR Section 3.11 admits immediate assembly operands (`I`, `J`, `K`,
    /// `L`, `i`, `n`), which emission "writes literally"; the constant is a
    /// prior definition in the same function, and this is where an assembly
    /// operand finds it.
    constants: HashMap<u32, u64>,
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

    /// Both halves of a pair, low first.
    fn pair(&self, value: &Value) -> Result<(VReg, VReg), String> {
        let low = self.reg(value)?;
        let high = self
            .high
            .get(&value.id)
            .copied()
            .ok_or_else(|| format!("v{} has no high half", value.id))?;
        Ok((low, high))
    }

    fn bind_pair(&mut self, value: &Value, low: VReg, high: VReg) {
        self.map.insert(value.id, low);
        self.high.insert(value.id, high);
    }

    /// Every register a value occupies, low half first.
    fn registers(&self, value: &Value) -> Result<Vec<VReg>, String> {
        if is_pair(&value.ty) {
            let (low, high) = self.pair(value)?;
            Ok(vec![low, high])
        } else {
            Ok(vec![self.reg(value)?])
        }
    }

    fn fresh_pair(&mut self) -> (VReg, VReg) {
        (self.fresh(RegClass::Gpr64), self.fresh(RegClass::Gpr64))
    }

    fn word(&mut self, insts: &mut Vec<MInst>, imm: u64) -> VReg {
        let dst = self.fresh(RegClass::Gpr64);
        insts.push(MInst::MovImm { dst, imm });
        dst
    }

    /// One 64-bit register-register form.
    fn word_binary(&mut self, insts: &mut Vec<MInst>, op: BinOp, lhs: VReg, rhs: VReg) -> VReg {
        let dst = self.fresh(RegClass::Gpr64);
        insts.push(MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width: Width::X,
        });
        dst
    }

    /// One 64-bit register-immediate form.
    fn word_binary_imm(&mut self, insts: &mut Vec<MInst>, op: BinOp, lhs: VReg, imm: u64) -> VReg {
        let dst = self.fresh(RegClass::Gpr64);
        insts.push(MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width: Width::X,
        });
        dst
    }

    /// One register-register form at `width`.
    fn binary(
        &mut self,
        insts: &mut Vec<MInst>,
        op: BinOp,
        lhs: VReg,
        rhs: VReg,
        width: Width,
    ) -> VReg {
        let dst = self.fresh(width.class());
        insts.push(MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width,
        });
        dst
    }

    /// One register-immediate form at `width`.
    fn binary_imm(
        &mut self,
        insts: &mut Vec<MInst>,
        op: BinOp,
        lhs: VReg,
        imm: u64,
        width: Width,
    ) -> VReg {
        let dst = self.fresh(width.class());
        insts.push(MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        });
        dst
    }

    /// One of Section 5's one-operand bit forms at `width`.
    fn unary(&mut self, insts: &mut Vec<MInst>, op: UnaryOp, src: VReg, width: Width) -> VReg {
        let dst = self.fresh(width.class());
        insts.push(MInst::Unary {
            op,
            dst,
            src,
            width,
        });
        dst
    }

    /// `tst s, #64` then `csel dst, taken, untaken, eq`.
    ///
    /// Section 5 selects the 64-or-more case with "two `csel` on `tst s,
    /// #64`". Section 2 admits a `flags` value only in the instruction that
    /// immediately follows its producer, so the test is written once per
    /// select rather than once per shift. It reads one register and writes
    /// only flags, so recomputing it is the same program.
    fn select_on_wide_amount(
        &mut self,
        insts: &mut Vec<MInst>,
        amount: VReg,
        taken: VReg,
        untaken: VReg,
    ) -> VReg {
        let flags = self.fresh(RegClass::Flags);
        insts.push(MInst::TstImm {
            dst: flags,
            lhs: amount,
            imm: 64,
            width: Width::X,
        });
        let dst = self.fresh(RegClass::Gpr64);
        insts.push(MInst::Csel {
            dst,
            flags,
            lhs: taken,
            rhs: untaken,
            cond: Cond::Eq,
            width: Width::X,
        });
        dst
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

fn lower_function(
    function: &Function,
    external: &dyn Fn(&str) -> bool,
) -> Result<MFunction, String> {
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
        high: HashMap::new(),
        blocks: Vec::new(),
        traps: Vec::new(),
        named_registers: function
            .variadic
            .then(|| named_argument_registers(&function.params)),
        constants: HashMap::new(),
    };

    for (index, block) in function.blocks.iter().enumerate() {
        let id = ids[&block.id];
        lower_block(
            &mut lowerer,
            function,
            block,
            id,
            &ids,
            external,
            index == 0,
        )?;
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
    let mut registers = Vec::new();
    for value in values.iter().filter(|value| is_register_type(&value.ty)) {
        registers.extend(lowerer.registers(value)?);
    }
    Ok(registers)
}

fn lower_block(
    lowerer: &mut Lowerer,
    function: &Function,
    block: &Block,
    id: u32,
    ids: &HashMap<u32, u32>,
    external: &dyn Fn(&str) -> bool,
    entry: bool,
) -> Result<(), String> {
    let mut args = Vec::new();
    for arg in &block.args {
        if !is_register_type(&arg.value.ty) {
            continue;
        }
        if is_pair(&arg.value.ty) {
            // Section 6 rounds the next register number up to even before a
            // pair. The skipped position is a real argument nothing reads, so
            // that Section 7's positional pinning gives the halves the numbers
            // the convention names. Inside the function the rule has no
            // meaning, so only the entry block carries it.
            if entry && args.len() % 2 == 1 {
                args.push(lowerer.fresh(RegClass::Gpr64));
            }
            let (low, high) = lowerer.fresh_pair();
            lowerer.bind_pair(&arg.value, low, high);
            args.push(low);
            args.push(high);
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
    // Section 6: the register save area is filled "in the prologue", before
    // anything writes an argument register. The marker is the first thing in
    // the entry block, which is where the frame layout finds it.
    if entry {
        if let Some(named) = lowerer.named_registers {
            insts.push(MInst::VaSaveArea { named });
        }
    }
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

        // Section 6's `va_arg`: "the register-then-stack procedure of Section 6
        // for `T`'s size and alignment, ending in a load of `T` from the
        // computed address". The procedure branches, so it is written here,
        // where a block can be split, exactly as the `ub.assume` row above is.
        if let Inst::VaAccess(access) = inst {
            if let VariadicOp::Arg(result) = &access.op {
                let (slot, alignment) = variadic_slot(&result.ty)
                    .ok_or_else(|| format!("a va_arg of {:?}", result.ty))?;
                let ap = lowerer.reg(&access.list)?;
                let carried = live_across(lowerer, block, index + 1);
                let width = carried.len();

                let from_registers = lowerer.fresh_block();
                let taken_from_registers = lowerer.fresh_block();
                let to_the_stack = lowerer.fresh_block();
                let from_the_stack = lowerer.fresh_block();
                let merge = lowerer.fresh_block();

                // `offs = gr_offs; if offs < 0:` -- the field is a signed
                // thirty-two-bit integer, so it is read sign-extended.
                let offs = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Load {
                    dst: offs,
                    base: ap,
                    offset: VA_GR_OFFS,
                    kind: LoadKind::Sw,
                    volatile: false,
                });
                let flags = lowerer.fresh(RegClass::Flags);
                insts.push(MInst::CmpImm {
                    dst: flags,
                    lhs: offs,
                    imm: 0,
                    width: Width::X,
                });
                let mut with_offset = carried.clone();
                with_offset.push(ap);
                with_offset.push(offs);
                let mut with_list = carried.clone();
                with_list.push(ap);
                lowerer.blocks.push(MBlock {
                    id: current_id,
                    args: std::mem::take(&mut current_args),
                    insts: std::mem::take(&mut insts),
                    term: MTerm::BCond {
                        flags,
                        cond: Cond::Lt,
                        taken: from_registers,
                        taken_args: with_offset.clone(),
                        untaken: from_the_stack,
                        untaken_args: with_list.clone(),
                    },
                });

                // `if A == 16: offs = (offs + 15) & ~15; next = offs + S8;
                //  if next <= 0: ...`
                let params = fresh_like(lowerer, &with_offset);
                let list = params[width];
                let offset = params[width + 1];
                let mut body = Vec::new();
                let aligned = if alignment == 16 {
                    round_to_sixteen(lowerer, &mut body, offset)
                } else {
                    offset
                };
                let next = lowerer.binary_imm(&mut body, BinOp::Add, aligned, slot, Width::X);
                let decided = lowerer.fresh(RegClass::Flags);
                body.push(MInst::CmpImm {
                    dst: decided,
                    lhs: next,
                    imm: 0,
                    width: Width::X,
                });
                let mut inside = params[..width].to_vec();
                inside.push(list);
                inside.push(aligned);
                inside.push(next);
                let mut outside = params[..width].to_vec();
                outside.push(list);
                lowerer.blocks.push(MBlock {
                    id: from_registers,
                    args: params,
                    insts: body,
                    term: MTerm::BCond {
                        flags: decided,
                        cond: Cond::Le,
                        taken: taken_from_registers,
                        taken_args: inside.clone(),
                        untaken: to_the_stack,
                        untaken_args: outside,
                    },
                });

                // `gr_offs = next; address = gr_top + offs; done`
                let params = fresh_like(lowerer, &inside);
                let list = params[width];
                let aligned = params[width + 1];
                let next = params[width + 2];
                let mut body = Vec::new();
                body.push(MInst::Store {
                    src: next,
                    base: list,
                    offset: VA_GR_OFFS,
                    kind: StoreKind::W,
                    volatile: false,
                });
                let top = lowerer.fresh(RegClass::Gpr64);
                body.push(MInst::Load {
                    dst: top,
                    base: list,
                    offset: VA_GR_TOP,
                    kind: LoadKind::P,
                    volatile: false,
                });
                let address = lowerer.binary(&mut body, BinOp::Add, top, aligned, Width::X);
                let mut reached = params[..width].to_vec();
                reached.push(address);
                lowerer.blocks.push(MBlock {
                    id: taken_from_registers,
                    args: params,
                    insts: body,
                    term: MTerm::B {
                        target: merge,
                        args: reached,
                    },
                });

                // `gr_offs = 0`, then the stack path.
                let params = fresh_like(lowerer, &with_list);
                let list = params[width];
                let zero = lowerer.fresh(RegClass::Gpr32);
                let mut body = vec![
                    MInst::MovImm { dst: zero, imm: 0 },
                    MInst::Store {
                        src: zero,
                        base: list,
                        offset: VA_GR_OFFS,
                        kind: StoreKind::W,
                        volatile: false,
                    },
                ];
                let mut onward = params[..width].to_vec();
                onward.push(list);
                let _ = &mut body;
                lowerer.blocks.push(MBlock {
                    id: to_the_stack,
                    args: params,
                    insts: body,
                    term: MTerm::B {
                        target: from_the_stack,
                        args: onward,
                    },
                });

                // `address = stack; if A == 16: address = (address + 15) &
                //  ~15; stack = address + S8`
                let params = fresh_like(lowerer, &with_list);
                let list = params[width];
                let mut body = Vec::new();
                let raw = lowerer.fresh(RegClass::Gpr64);
                body.push(MInst::Load {
                    dst: raw,
                    base: list,
                    offset: VA_STACK,
                    kind: LoadKind::P,
                    volatile: false,
                });
                let address = if alignment == 16 {
                    round_to_sixteen(lowerer, &mut body, raw)
                } else {
                    raw
                };
                let bumped = lowerer.binary_imm(&mut body, BinOp::Add, address, slot, Width::X);
                body.push(MInst::Store {
                    src: bumped,
                    base: list,
                    offset: VA_STACK,
                    kind: StoreKind::P,
                    volatile: false,
                });
                let mut reached = params[..width].to_vec();
                reached.push(address);
                lowerer.blocks.push(MBlock {
                    id: from_the_stack,
                    args: params,
                    insts: body,
                    term: MTerm::B {
                        target: merge,
                        args: reached,
                    },
                });

                // "then loads `T` from `address`."
                let mut shape = carried.clone();
                shape.push(VReg::new(0, RegClass::Gpr64));
                let params = fresh_like(lowerer, &shape);
                let address = params[width];
                let mut body = Vec::new();
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
                        dst,
                        base: address,
                        offset: 0,
                        kind,
                        volatile: false,
                    });
                    lowerer.bind(result, dst);
                }
                let mut rebind: HashMap<u32, VReg> = HashMap::new();
                for (old, fresh) in carried.iter().zip(params.iter()) {
                    rebind.insert(old.id, *fresh);
                }
                for reg in lowerer.map.values_mut() {
                    if let Some(fresh) = rebind.get(&reg.id) {
                        *reg = *fresh;
                    }
                }
                for reg in lowerer.high.values_mut() {
                    if let Some(fresh) = rebind.get(&reg.id) {
                        *reg = *fresh;
                    }
                }
                current_id = merge;
                current_args = params;
                insts = body;
                continue;
            }
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

        lower_inst(lowerer, &mut insts, inst, external)?;
    }

    let term = lower_terminator(lowerer, &mut insts, function, block, ids, fused)?;
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
        if let Some(reg) = lowerer.high.get(&value) {
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
    if is_pair(&lhs.ty) {
        return lower_pair_compare(lowerer, insts, operation, lhs, rhs);
    }
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
    insts: &mut Vec<MInst>,
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
            let mut results = edge_args(lowerer, values)?;
            // Section 6: Apple's variant extends a result narrower than 32
            // bits, and the probe of Section 9 measured it: a host caller
            // reading a `bits<8>` result at `-O2` takes the whole register and
            // its own build of the same function had left the bits above the
            // width clear. A callee cannot know whether its caller is one this
            // compiler emitted, so it extends on every return; the internal
            // rule leaves those bits unspecified, so specifying them breaks
            // nothing. Zero, because EvoIR carries no result signedness, which
            // is the rule revision 0.7 already ruled for arguments.
            // A pair occupies two result registers, so the walk counts
            // registers rather than values; neither half of a `bits<128>` is
            // narrow, so neither is extended.
            let mut slot = 0;
            for value in values.iter().filter(|value| is_register_type(&value.ty)) {
                if is_pair(&value.ty) {
                    slot += 2;
                    continue;
                }
                if let EvoIRType::Bits(bits) = value.ty {
                    if bits < 32 {
                        results[slot] = lowerer.zero_extend(insts, results[slot], bits, Width::W);
                    }
                }
                slot += 1;
            }
            Ok(MTerm::Ret { values: results })
        }
        Inst::Trap(reason) => Ok(MTerm::Brk {
            imm: trap_immediate(*reason),
        }),
        other => Err(format!("{other:?} is not a terminator")),
    }
}

#[allow(clippy::too_many_lines)]
fn lower_inst(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
    external: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    // Section 5's `bits<128>` row expands each form over the halves, so it is
    // consulted before the rows that assume one register per value.
    if lower_pair(lowerer, insts, inst)? {
        return Ok(());
    }
    match inst {
        Inst::Const(result, bits) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::MovImm { dst, imm: *bits });
            lowerer.bind(result, dst);
            lowerer.constants.insert(result.id, *bits);
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
        // Section 5's `va_start` and `va_end` rows. `va_arg` needs branches,
        // so it splits the block and is lowered where the blocks are built.
        Inst::VaAccess(access) => match &access.op {
            VariadicOp::Start => lower_va_start(lowerer, insts, &access.list)?,
            VariadicOp::End => {}
            VariadicOp::Arg(_) => {
                return Err("a va_arg is lowered with its block".to_owned());
            }
        },
        // Section 4: the `asm` of EvoIR Section 3.11, "carried through
        // unchanged: the template, its operands with their constraints, its
        // clobbers".
        Inst::Asm(assembly) => {
            let body = lower_assembly(lowerer, insts, assembly)?;
            insts.push(MInst::Asm(body));
        }
        // Section 5's bit-counting and byte-swap row (revision 0.14).
        Inst::BitOperation {
            result,
            operand,
            op,
        } => {
            lower_bit_operation(lowerer, insts, result, operand, *op)?;
        }
        // Section 5 (revision 0.12): "`ptr.to_int`, `ptr.from_int.*` |
        // nothing: the value is the same 64-bit register". A pointer and a
        // `bits<64>` are both `gpr64`, so the conversion is an alias and emits
        // no instruction at all.
        Inst::PointerCast { result, operand } => {
            let src = lowerer.reg(operand)?;
            lowerer.bind(result, src);
        }
        Inst::GlobalAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Global);
        }
        Inst::FuncAddr { result, symbol } => {
            address_of(lowerer, insts, result, symbol, SymbolKind::Function);
        }
        // Section 5 (revision 0.15): "`block_addr ^b` | `adr rd, <label of
        // block b>`". One instruction, not a page pair: `adr` reaches a
        // mebibyte either way, which covers any function this backend emits.
        Inst::BlockAddr { result, block } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Adr { dst, block: *block });
            lowerer.bind(result, dst);
        }
        // Section 5 (revision 0.17): "`return_address` | `ldr rd, [x29, #8]`:
        // the saved link register of this frame record, valid anywhere in the
        // body because every function keeps a frame pointer" and
        // "`frame_address` | `mov rd, x29`". Both places belong to the frame
        // layout, so both are the `frame.address` pseudo-form emission expands.
        Inst::ReturnAddress { result } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameAddress {
                dst,
                place: FramePlace::SavedLinkRegister,
            });
            lowerer.bind(result, dst);
        }
        Inst::FrameAddress { result } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameAddress {
                dst,
                place: FramePlace::FramePointer,
            });
            lowerer.bind(result, dst);
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
            let mut arguments = call_args(lowerer, insts, args)?;
            if external(callee) {
                // Section 6, revision 0.5: across the host boundary "a caller
                // extends an argument narrower than 32 bits to 32 bits". The
                // value of a `bits<N>` is its low `N` bits, so the extension
                // that preserves it is the zero one; sign-extending would need
                // the source type's signedness, which lives above the
                // extraction boundary and which EvoIR does not carry. Recorded
                // gap: a signed host parameter narrower than 32 bits is not
                // reachable from EvoIR alone.
                let mut slot = 0;
                for value in args.iter().filter(|value| is_register_type(&value.ty)) {
                    if is_pair(&value.ty) {
                        if slot % 2 == 1 {
                            slot += 1;
                        }
                        slot += 2;
                        continue;
                    }
                    if let EvoIRType::Bits(bits) = value.ty {
                        if bits < 32 {
                            arguments[slot] =
                                lowerer.zero_extend(insts, arguments[slot], bits, Width::W);
                        }
                    }
                    slot += 1;
                }
            }
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
            let arguments = call_args(lowerer, insts, args)?;
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

/// The registers a call passes, in the positions Section 6 assigns them.
fn call_args(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    args: &[Value],
) -> Result<Vec<VReg>, String> {
    let ordinary: Vec<&Value> = args
        .iter()
        .filter(|value| is_register_type(&value.ty))
        .collect();
    let mut registers: Vec<VReg> = Vec::new();
    for value in ordinary {
        if is_pair(&value.ty) && registers.len() % 2 == 1 {
            // The skipped register is passed as zero rather than left
            // undefined, so that two lowerings of one unit agree byte for byte.
            let filler = lowerer.word(insts, 0);
            registers.push(filler);
        }
        registers.extend(lowerer.registers(value)?);
    }
    Ok(registers)
}

fn call_results(lowerer: &mut Lowerer, results: &[Value]) -> Result<Vec<VReg>, String> {
    let mut outputs = Vec::new();
    for value in results.iter().filter(|value| is_register_type(&value.ty)) {
        if is_pair(&value.ty) {
            // Section 6: "A `bits<128>` result is `x0`:`x1`, low half in
            // `x0`."
            let (low, high) = lowerer.fresh_pair();
            lowerer.bind_pair(value, low, high);
            outputs.push(low);
            outputs.push(high);
            continue;
        }
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

// ---------------------------------------------------------------------------
// Section 5: bit counting and byte swap (revision 0.14)
// ---------------------------------------------------------------------------

/// The row, sequence by sequence.
///
/// "`ctlz`: zero-extend per Section 3, `clz` at the register width, then
/// subtract `32 - N` or `64 - N` for a narrower `N`; `cttz`: `rbit` then
/// `clz`, and for a narrower `N` the result capped at `N` with `cmp`/`csel`,
/// since the unspecified upper bits may hold a set bit; `ctpop`: the
/// general-register sequence both oracles use without SIMD (`p39.out`,
/// Clang-19; GCC-14 calls `__popcountdi2`, which the kernel does not provide)
/// ... after zero extension; `bswap`: `rev16` (16, upper bits unspecified per
/// Section 3), `rev` at 32 or 64."
fn lower_bit_operation(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    result: &Value,
    operand: &Value,
    op: BitOp,
) -> Result<(), String> {
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let width = width_of(class);
    let bits = significant_bits(&result.ty);
    let src = lowerer.reg(operand)?;

    let value = match op {
        BitOp::LeadingZeros => {
            let extended = lowerer.zero_extend(insts, src, bits, width);
            let counted = lowerer.unary(insts, UnaryOp::Clz, extended, width);
            if bits < width.bits() {
                lowerer.binary_imm(
                    insts,
                    BinOp::Sub,
                    counted,
                    u64::from(width.bits() - bits),
                    width,
                )
            } else {
                counted
            }
        }
        BitOp::TrailingZeros => {
            // The operand is not extended: `rbit` then `clz` answers the
            // trailing zero count of whatever the register holds, and only the
            // all-zero low `N` bits need the cap, which is what a garbage bit
            // above `N` would otherwise shorten.
            let reversed = lowerer.unary(insts, UnaryOp::Rbit, src, width);
            let counted = lowerer.unary(insts, UnaryOp::Clz, reversed, width);
            if bits < width.bits() {
                let cap = lowerer.fresh(class);
                insts.push(MInst::MovImm {
                    dst: cap,
                    imm: u64::from(bits),
                });
                let flags = lowerer.fresh(RegClass::Flags);
                insts.push(MInst::CmpImm {
                    dst: flags,
                    lhs: counted,
                    imm: u64::from(bits),
                    width,
                });
                let capped = lowerer.fresh(class);
                insts.push(MInst::Csel {
                    dst: capped,
                    flags,
                    lhs: cap,
                    rhs: counted,
                    cond: Cond::Hs,
                    width,
                });
                capped
            } else {
                counted
            }
        }
        BitOp::Population => {
            let extended = lowerer.zero_extend(insts, src, bits, width);
            population_count(lowerer, insts, extended, width)
        }
        BitOp::ByteSwap => match bits {
            16 => lowerer.unary(insts, UnaryOp::Rev16, src, width),
            32 | 64 => lowerer.unary(insts, UnaryOp::Rev, src, width),
            other => return Err(format!("bswap at bits<{other}>")),
        },
    };
    lowerer.bind(result, value);
    Ok(())
}

/// The SIMD-free population count both oracles emit (`p39.out`, Clang-19).
///
/// `x - ((x >> 1) & 0x5555...)`, `(x & 0x3333...) + ((x >> 2) & 0x3333...)`,
/// `(x + (x >> 4)) & 0x0f0f...`, multiply by `0x0101...` and `lsr` by the
/// register width minus 8.
fn population_count(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: VReg,
    width: Width,
) -> VReg {
    let mask = |pattern: u64| pattern & width.mask();
    let ones = mask(0x5555_5555_5555_5555);
    let twos = mask(0x3333_3333_3333_3333);
    let nibbles = mask(0x0f0f_0f0f_0f0f_0f0f);
    let bytes = mask(0x0101_0101_0101_0101);

    let halved = lowerer.binary_imm(insts, BinOp::Lsr, value, 1, width);
    let odd = lowerer.binary_imm(insts, BinOp::And, halved, ones, width);
    let pairs = lowerer.binary(insts, BinOp::Sub, value, odd, width);

    let low = lowerer.binary_imm(insts, BinOp::And, pairs, twos, width);
    let shifted = lowerer.binary_imm(insts, BinOp::Lsr, pairs, 2, width);
    let high = lowerer.binary_imm(insts, BinOp::And, shifted, twos, width);
    let quads = lowerer.binary(insts, BinOp::Add, low, high, width);

    let folded = lowerer.binary_imm(insts, BinOp::Lsr, quads, 4, width);
    let summed = lowerer.binary(insts, BinOp::Add, quads, folded, width);
    let octets = lowerer.binary_imm(insts, BinOp::And, summed, nibbles, width);

    let factor = lowerer.fresh(width.class());
    insts.push(MInst::MovImm {
        dst: factor,
        imm: bytes,
    });
    let product = lowerer.binary(insts, BinOp::Mul, octets, factor, width);
    lowerer.binary_imm(
        insts,
        BinOp::Lsr,
        product,
        u64::from(width.bits() - 8),
        width,
    )
}

// ---------------------------------------------------------------------------
// Section 4: inline assembly (EvoIR Section 3.11)
// ---------------------------------------------------------------------------

/// Whether every constraint and clobber of one `asm` is in the closed
/// vocabulary EvoMIR carries.
///
/// EvoIR has already refused anything outside its own table (`E3021`); this is
/// the machine side of the same closure, so a constraint EvoMIR cannot spell
/// is named rather than approximated.
fn assembly_vocabulary(assembly: &InlineAssembly) -> Result<(), String> {
    for output in &assembly.outputs {
        if AsmConstraint::parse(&output.constraint).is_none() {
            return Err(format!(
                "an assembly output constraint '{}'",
                output.constraint
            ));
        }
    }
    for input in &assembly.inputs {
        if AsmConstraint::parse(&input.constraint).is_none() {
            return Err(format!(
                "an assembly input constraint '{}'",
                input.constraint
            ));
        }
    }
    for clobber in &assembly.clobbers {
        if AsmClobber::parse(clobber).is_none() {
            return Err(format!("an assembly clobber '{clobber}'"));
        }
    }
    Ok(())
}

/// Carry one EvoIR `asm` into its EvoMIR body.
///
/// EvoIR Section 3.11: "Each register output defines one fresh SSA result, in
/// output-list order. A memory output instead carries its place's pointer as
/// the incoming value and its type is that pointer's type; it defines no
/// scalar result". A read-write output and a tied input each name one
/// register the template both reads and writes, so the incoming value is moved
/// into it before the assembly, which is the form Section 4's own fixtures
/// use.
fn lower_assembly(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    assembly: &InlineAssembly,
) -> Result<AsmBody, String> {
    let named = |name: &str| -> Option<String> { (!name.is_empty()).then(|| name.to_owned()) };
    let mut outputs: Vec<AsmOperand> = Vec::new();
    let mut results = assembly.results.iter();
    for output in &assembly.outputs {
        let constraint = AsmConstraint::parse(&output.constraint)
            .ok_or_else(|| format!("an assembly output constraint '{}'", output.constraint))?;
        if let AsmConstraint::Memory { .. } = constraint {
            let place = output
                .incoming
                .as_ref()
                .ok_or_else(|| "a memory output with no place".to_owned())?;
            outputs.push(AsmOperand {
                constraint,
                name: named(&output.name),
                value: AsmValue::Reg(lowerer.reg(place)?),
            });
            continue;
        }
        let result = results
            .next()
            .ok_or_else(|| "a register assembly output with no result".to_owned())?;
        let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
        let register = lowerer.fresh(class);
        if let Some(incoming) = &output.incoming {
            let src = lowerer.reg(incoming)?;
            insts.push(MInst::Mov { dst: register, src });
        }
        lowerer.bind(result, register);
        outputs.push(AsmOperand {
            constraint,
            name: named(&output.name),
            value: AsmValue::Reg(register),
        });
    }

    let mut inputs: Vec<AsmOperand> = Vec::new();
    for input in &assembly.inputs {
        let constraint = AsmConstraint::parse(&input.constraint)
            .ok_or_else(|| format!("an assembly input constraint '{}'", input.constraint))?;
        let value = match constraint {
            AsmConstraint::Immediate(_) => {
                let literal = lowerer
                    .constants
                    .get(&input.value.id)
                    .copied()
                    .ok_or_else(|| {
                        format!(
                            "an immediate assembly operand whose value v{} is not a constant",
                            input.value.id
                        )
                    })?;
                AsmValue::Imm(literal as i64)
            }
            AsmConstraint::RegisterOrZero => {
                // "a general register, or the zero register when the operand
                // is the constant zero".
                if lowerer.constants.get(&input.value.id) == Some(&0) {
                    let class = class_of(&input.value.ty).map_err(|(code, _)| code.to_owned())?;
                    AsmValue::Zero(class)
                } else {
                    AsmValue::Reg(lowerer.reg(&input.value)?)
                }
            }
            AsmConstraint::Tied(digit) => {
                let output = outputs.get(usize::from(digit)).ok_or_else(|| {
                    format!("a tied operand names output {digit}, which is absent")
                })?;
                let register = output.register().ok_or_else(|| {
                    format!("a tied operand names output {digit}, which holds no register")
                })?;
                let src = lowerer.reg(&input.value)?;
                insts.push(MInst::Mov { dst: register, src });
                AsmValue::Reg(register)
            }
            _ => AsmValue::Reg(lowerer.reg(&input.value)?),
        };
        inputs.push(AsmOperand {
            constraint,
            name: named(&input.name),
            value,
        });
    }

    let mut clobbers = Vec::new();
    for clobber in &assembly.clobbers {
        clobbers.push(
            AsmClobber::parse(clobber).ok_or_else(|| format!("an assembly clobber '{clobber}'"))?,
        );
    }

    Ok(AsmBody {
        template: assembly.template.clone(),
        volatile: assembly.volatile,
        outputs,
        inputs,
        clobbers,
    })
}

// ---------------------------------------------------------------------------
// Section 6: variadic functions (revision 0.10)
// ---------------------------------------------------------------------------

/// Section 6: "the number of argument registers its named parameters
/// consumed (a `bits<128>` parameter consumes two and the rounding above)".
fn named_argument_registers(params: &[evocc_evoir::FunctionParam]) -> u8 {
    let mut used = 0_usize;
    for param in params.iter().filter(|param| is_register_type(&param.ty)) {
        if is_pair(&param.ty) {
            if used % 2 == 1 {
                used += 1;
            }
            used += 2;
        } else {
            used += 1;
        }
    }
    u8::try_from(used.min(MAX_REGISTER_ARGUMENTS)).unwrap_or(8)
}

/// The size rounded up to eight (`S8`) and the alignment (`A`) of one
/// `va_arg` type, or nothing when the type has no Section 6 procedure.
fn variadic_slot(ty: &EvoIRType) -> Option<(u64, u64)> {
    match ty {
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } => Some((8, 8)),
        EvoIRType::Bits(128) => Some((16, 16)),
        EvoIRType::Bits(width) if *width <= 64 => Some((8, 8)),
        _ => None,
    }
}

/// The offsets of the list object's five fields (Section 6).
const VA_STACK: i64 = 0;
const VA_GR_TOP: i64 = 8;
const VA_VR_TOP: i64 = 16;
const VA_GR_OFFS: i64 = 24;
const VA_VR_OFFS: i64 = 28;

/// `va_start`: "stores `stack = x29 + 16` (the first incoming stack slot),
/// `gr_top = save area + 64`, `vr_top = gr_top` (never read ...), `gr_offs =
/// -8 * (8 - k)` and `vr_offs = 0`".
fn lower_va_start(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    list: &Value,
) -> Result<(), String> {
    let named = lowerer
        .named_registers
        .ok_or_else(|| "a va_start outside a variadic function".to_owned())?;
    let ap = lowerer.reg(list)?;

    let stack = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::FrameAddress {
        dst: stack,
        place: FramePlace::IncomingStack,
    });
    insts.push(MInst::Store {
        src: stack,
        base: ap,
        offset: VA_STACK,
        kind: StoreKind::P,
        volatile: false,
    });

    let top = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::FrameAddress {
        dst: top,
        place: FramePlace::RegisterSaveTop,
    });
    for offset in [VA_GR_TOP, VA_VR_TOP] {
        insts.push(MInst::Store {
            src: top,
            base: ap,
            offset,
            kind: StoreKind::P,
            volatile: false,
        });
    }

    let offs = -8 * (8 - i64::from(named));
    let gr_offs = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::MovImm {
        dst: gr_offs,
        imm: u64::from(offs as u32),
    });
    insts.push(MInst::Store {
        src: gr_offs,
        base: ap,
        offset: VA_GR_OFFS,
        kind: StoreKind::W,
        volatile: false,
    });
    let vr_offs = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::MovImm {
        dst: vr_offs,
        imm: 0,
    });
    insts.push(MInst::Store {
        src: vr_offs,
        base: ap,
        offset: VA_VR_OFFS,
        kind: StoreKind::W,
        volatile: false,
    });
    Ok(())
}

/// One fresh register per entry of `shape`, in the same classes.
fn fresh_like(lowerer: &mut Lowerer, shape: &[VReg]) -> Vec<VReg> {
    shape.iter().map(|reg| lowerer.fresh(reg.class)).collect()
}

/// Round a 64-bit value up to sixteen: `(value + 15) & ~15`.
fn round_to_sixteen(lowerer: &mut Lowerer, insts: &mut Vec<MInst>, value: VReg) -> VReg {
    let raised = lowerer.binary_imm(insts, BinOp::Add, value, 15, Width::X);
    lowerer.binary_imm(insts, BinOp::And, raised, !15_u64, Width::X)
}

/// The forms Section 10 keeps refused at width 128.
///
/// Every one of them needs an arithmetic wider than the value to answer, which
/// the machine has no register pair for: a 128-bit quotient, and the signed
/// predicates whose definition compares a product or a sum against the range
/// of the type it was computed in.
fn unlowered_at_the_widest_scalar(inst: &Inst) -> Option<String> {
    let (name, ty) = match inst {
        Inst::SDiv(_, lhs, _) => ("sdiv", &lhs.ty),
        Inst::UDiv(_, lhs, _) => ("udiv", &lhs.ty),
        Inst::SRem(_, lhs, _) => ("srem", &lhs.ty),
        Inst::URem(_, lhs, _) => ("urem", &lhs.ty),
        Inst::SignedAddDefined(_, lhs, _) => ("signed_add_defined", &lhs.ty),
        Inst::SignedSubDefined(_, lhs, _) => ("signed_sub_defined", &lhs.ty),
        Inst::SignedMulDefined(_, lhs, _) => ("signed_mul_defined", &lhs.ty),
        Inst::SignedDivRemNoOverflow(_, lhs, _) => ("signed_div_rem_no_overflow", &lhs.ty),
        Inst::SignedShlDefined { shifted, .. } => ("signed_shl_defined", &shifted.ty),
        _ => return None,
    };
    is_pair(ty).then(|| format!("{name} at bits<128>"))
}

// ---------------------------------------------------------------------------
// Section 5: the `bits<128>` forms
// ---------------------------------------------------------------------------

/// Lower one form over a register pair, answering whether it did.
///
/// Every sequence here is the one both sealed oracles emit for the same C,
/// recorded in `.evidence/builtin-probes/20260902-2c23684c/p34-p35.out` and
/// `.evidence/builtin-probes/20260905-b6/b6-probes.out`.
fn lower_pair(lowerer: &mut Lowerer, insts: &mut Vec<MInst>, inst: &Inst) -> Result<bool, String> {
    match inst {
        // A `bits<128>` constant carries its immediate in the same 64-bit
        // field every other constant does, so the high half is zero.
        Inst::Const(result, bits) if is_pair(&result.ty) => {
            let low = lowerer.word(insts, *bits);
            let high = lowerer.word(insts, 0);
            lowerer.bind_pair(result, low, high);
        }
        // Section 5: "any value is permitted; zero is chosen so the output is
        // deterministic."
        Inst::Uninitialized(result) if is_pair(&result.ty) => {
            let low = lowerer.word(insts, 0);
            let high = lowerer.word(insts, 0);
            lowerer.bind_pair(result, low, high);
        }
        // "add: `adds lo` then `adc hi`; sub: `subs lo` then `sbc hi`."
        Inst::Add(result, lhs, rhs) | Inst::Sub(result, lhs, rhs) if is_pair(&result.ty) => {
            let (a_low, a_high) = lowerer.pair(lhs)?;
            let (b_low, b_high) = lowerer.pair(rhs)?;
            let subtract = matches!(inst, Inst::Sub(..));
            let low = lowerer.fresh(RegClass::Gpr64);
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::FlagBinary {
                op: if subtract { FlagOp::Subs } else { FlagOp::Adds },
                dst: low,
                flags,
                lhs: a_low,
                rhs: b_low,
                width: Width::X,
            });
            let high = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::CarryBinary {
                op: if subtract { CarryOp::Sbc } else { CarryOp::Adc },
                dst: high,
                flags,
                lhs: a_high,
                rhs: b_high,
                width: Width::X,
            });
            lowerer.bind_pair(result, low, high);
        }
        // "mul: `mul lo, alo, blo`, `umulh hi, alo, blo`,
        // `madd hi, alo, bhi, hi`, `madd hi, ahi, blo, hi`."
        Inst::Mul(result, lhs, rhs) if is_pair(&result.ty) => {
            let (a_low, a_high) = lowerer.pair(lhs)?;
            let (b_low, b_high) = lowerer.pair(rhs)?;
            let low = lowerer.word_binary(insts, BinOp::Mul, a_low, b_low);
            let carried = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::WideMultiply {
                op: WideMul::Umulh,
                dst: carried,
                lhs: a_low,
                rhs: b_low,
            });
            let first = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Madd {
                dst: first,
                n: a_low,
                m: b_high,
                a: carried,
                width: Width::X,
            });
            let high = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Madd {
                dst: high,
                n: a_high,
                m: b_low,
                a: first,
                width: Width::X,
            });
            lowerer.bind_pair(result, low, high);
        }
        // "`and`, `or`, `xor`: halfwise."
        Inst::And(result, lhs, rhs) | Inst::Or(result, lhs, rhs) | Inst::Xor(result, lhs, rhs)
            if is_pair(&result.ty) =>
        {
            let op = match inst {
                Inst::And(..) => BinOp::And,
                Inst::Or(..) => BinOp::Orr,
                _ => BinOp::Eor,
            };
            let (a_low, a_high) = lowerer.pair(lhs)?;
            let (b_low, b_high) = lowerer.pair(rhs)?;
            let low = lowerer.word_binary(insts, op, a_low, b_low);
            let high = lowerer.word_binary(insts, op, a_high, b_high);
            lowerer.bind_pair(result, low, high);
        }
        Inst::Shl(result, lhs, rhs) if is_pair(&result.ty) => {
            let pair = lower_pair_shift(lowerer, insts, lhs, rhs, PairShift::Left)?;
            lowerer.bind_pair(result, pair.0, pair.1);
        }
        Inst::LShr(result, lhs, rhs) if is_pair(&result.ty) => {
            let pair = lower_pair_shift(lowerer, insts, lhs, rhs, PairShift::Logical)?;
            lowerer.bind_pair(result, pair.0, pair.1);
        }
        Inst::AShr(result, lhs, rhs) if is_pair(&result.ty) => {
            let pair = lower_pair_shift(lowerer, insts, lhs, rhs, PairShift::Arithmetic)?;
            lowerer.bind_pair(result, pair.0, pair.1);
        }
        // "zext to 128: `hi = 0`; sext to 128: `hi = asr lo, #63`."
        Inst::ZExt(result, source, _) | Inst::SExt(result, source, _) if is_pair(&result.ty) => {
            let signed = matches!(inst, Inst::SExt(..));
            let (low, high) = if is_pair(&source.ty) {
                lowerer.pair(source)?
            } else {
                let bits = significant_bits(&source.ty);
                let src = lowerer.reg(source)?;
                let low = if signed {
                    lowerer.sign_extend(insts, src, bits, Width::X)
                } else {
                    lowerer.zero_extend(insts, src, bits, Width::X)
                };
                let low = if low.class == RegClass::Gpr64 {
                    low
                } else {
                    let dst = lowerer.fresh(RegClass::Gpr64);
                    insts.push(MInst::Mov { dst, src: low });
                    dst
                };
                let high = if signed {
                    lowerer.word_binary_imm(insts, BinOp::Asr, low, 63)
                } else {
                    lowerer.word(insts, 0)
                };
                (low, high)
            };
            lowerer.bind_pair(result, low, high);
        }
        // A widening that is no widening still names a pair.
        Inst::Trunc(result, source, _) if is_pair(&result.ty) => {
            let (low, high) = lowerer.pair(source)?;
            lowerer.bind_pair(result, low, high);
        }
        // "`shift_count_defined` compares the amount with `#128`."
        Inst::ShiftCountDefined {
            result,
            shifted,
            amount,
            ..
        } if is_pair(&shifted.ty) => {
            let amount = lowerer.reg(amount)?;
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: amount,
                imm: 128,
                width: Width::X,
            });
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let dst = lowerer.fresh(class);
            insts.push(MInst::Cset {
                dst,
                flags,
                cond: Cond::Lo,
            });
            lowerer.bind(result, dst);
        }
        // "`load`/`store`: `ldp`/`stp` of the halves at the address, which is
        // not single-copy atomic, exactly as both oracles emit for a
        // `volatile u128`."
        Inst::Load {
            result,
            address,
            volatile,
            ..
        } if is_pair(&result.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.fresh_pair();
            insts.push(MInst::Ldp {
                rt1: low,
                rt2: high,
                base,
                offset: 0,
                writeback: false,
                volatile: *volatile,
            });
            lowerer.bind_pair(result, low, high);
        }
        Inst::Store {
            address,
            value,
            volatile,
            ..
        } if is_pair(&value.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.pair(value)?;
            insts.push(MInst::Stp {
                rt1: low,
                rt2: high,
                base,
                offset: 0,
                writeback: false,
                volatile: *volatile,
            });
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Which of the three shifts a pair is being given.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PairShift {
    Left,
    Logical,
    Arithmetic,
}

/// Section 5's branch-free variable shift of a pair.
///
/// "the 64-bit shifts of both halves, the cross term `lsl`/`lsr` of the other
/// half by `63 - s` after a one-bit pre-shift, combined by `orr`, and two
/// `csel` on `tst s, #64` selecting the 64-or-more case". The front end has
/// asserted `s < 128`, so only bit 6 of the amount distinguishes the two
/// cases and the machine's own masking supplies `s mod 64`.
///
/// The arithmetic form's 64-or-more case takes the sign fill rather than zero,
/// which GCC-14 spells with a different comparison; the sequence written here
/// was measured against the C operator over 8,192 operand and count pairs
/// under both oracles (`b6-cmp-sbcs.c`).
fn lower_pair_shift(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: &Value,
    amount: &Value,
    kind: PairShift,
) -> Result<(VReg, VReg), String> {
    let (low, high) = lowerer.pair(value)?;
    let count = lowerer.reg(amount)?;
    let bound = lowerer.word(insts, 63);
    let complement = lowerer.word_binary(insts, BinOp::Sub, bound, count);

    if kind == PairShift::Left {
        let pre = lowerer.word_binary_imm(insts, BinOp::Lsr, low, 1);
        let cross = lowerer.word_binary(insts, BinOp::Lsr, pre, complement);
        let shifted_high = lowerer.word_binary(insts, BinOp::Lsl, high, count);
        let combined = lowerer.word_binary(insts, BinOp::Orr, cross, shifted_high);
        let shifted_low = lowerer.word_binary(insts, BinOp::Lsl, low, count);
        let zero = lowerer.word(insts, 0);
        let result_high = lowerer.select_on_wide_amount(insts, count, combined, shifted_low);
        let result_low = lowerer.select_on_wide_amount(insts, count, shifted_low, zero);
        return Ok((result_low, result_high));
    }

    let arithmetic = kind == PairShift::Arithmetic;
    let pre = lowerer.word_binary_imm(insts, BinOp::Lsl, high, 1);
    let cross = lowerer.word_binary(insts, BinOp::Lsl, pre, complement);
    let shifted_low = lowerer.word_binary(insts, BinOp::Lsr, low, count);
    let combined = lowerer.word_binary(insts, BinOp::Orr, cross, shifted_low);
    let shifted_high = lowerer.word_binary(
        insts,
        if arithmetic { BinOp::Asr } else { BinOp::Lsr },
        high,
        count,
    );
    let beyond = if arithmetic {
        lowerer.word_binary_imm(insts, BinOp::Asr, high, 63)
    } else {
        lowerer.word(insts, 0)
    };
    let result_low = lowerer.select_on_wide_amount(insts, count, combined, shifted_high);
    let result_high = lowerer.select_on_wide_amount(insts, count, shifted_high, beyond);
    Ok((result_low, result_high))
}

/// Section 5's comparison of two pairs.
///
/// "`icmp.eq`/`ne`: `eor` each half pair, `orr`, compare with zero; ordered
/// `icmp`: `cmp alo, blo` then `sbcs xzr, ahi, bhi` and `cset` with the 64-bit
/// form's condition (`lo` for `ult`, `lt` for `slt`; `ule` and `sle` compute
/// the swapped subtraction and take `hs` and `ge`)."
fn lower_pair_compare(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    operation: CmpOp,
    lhs: &Value,
    rhs: &Value,
) -> Result<(VReg, Cond), String> {
    let (a_low, a_high) = lowerer.pair(lhs)?;
    let (b_low, b_high) = lowerer.pair(rhs)?;
    match operation {
        CmpOp::Eq | CmpOp::Ne => {
            let low = lowerer.word_binary(insts, BinOp::Eor, a_low, b_low);
            let high = lowerer.word_binary(insts, BinOp::Eor, a_high, b_high);
            let difference = lowerer.word_binary(insts, BinOp::Orr, low, high);
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: difference,
                imm: 0,
                width: Width::X,
            });
            Ok((flags, condition_of(operation)))
        }
        // The two "or equal" forms subtract the other way and read the
        // complementary condition, because the machine has no borrow-aware
        // `ls` or `le` over a pair.
        CmpOp::ULe | CmpOp::SLe => {
            let (low, high, other_low, other_high) = (b_low, b_high, a_low, a_high);
            let borrow = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::Cmp {
                dst: borrow,
                lhs: low,
                rhs: other_low,
                width: Width::X,
            });
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::SubCarryFlags {
                dst: flags,
                flags_in: borrow,
                lhs: high,
                rhs: other_high,
                width: Width::X,
            });
            Ok((
                flags,
                if operation == CmpOp::ULe {
                    Cond::Hs
                } else {
                    Cond::Ge
                },
            ))
        }
        CmpOp::ULt | CmpOp::SLt => {
            let borrow = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::Cmp {
                dst: borrow,
                lhs: a_low,
                rhs: b_low,
                width: Width::X,
            });
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::SubCarryFlags {
                dst: flags,
                flags_in: borrow,
                lhs: a_high,
                rhs: b_high,
                width: Width::X,
            });
            Ok((
                flags,
                if operation == CmpOp::ULt {
                    Cond::Lo
                } else {
                    Cond::Lt
                },
            ))
        }
    }
}
