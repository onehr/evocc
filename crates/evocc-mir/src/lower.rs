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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
mod quad;

use evocc_evoir::{
    call_argument_marks, inst_results, inst_uses, BitOp, Block, CmpOp, Function, InlineAssembly,
    Inst, Module, TrapReason, UbReason, Value, VariadicOp,
};
use evocc_types::{param_mark, EvoIRType, ParamMark, RecordShape};

use crate::isa::aarch64::abi::{
    general_bank, place, vector_bank, Aarch64Abi, ProfileFeature, KERNEL,
};
use crate::{
    inst_defs, AsmBody, AsmClobber, AsmConstraint, AsmOperand, AsmValue, BinOp, CarryOp, Cond,
    ExtOp, FlagOp, FramePlace, LoadKind, MBlock, MFunction, MInst, MModule, MTerm, RegClass,
    StoreKind, SymbolKind, Target, UnaryOp, VReg, WideMul, Width,
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
const E4002: (&str, &str) = ("E4002", "not-lowered: vector");
const E4003: (&str, &str) = ("E4003", "not-lowered: wide-integer");
const E4004: (&str, &str) = ("E4004", "not-lowered: tuple");
const E4009: (&str, &str) = ("E4009", "not-lowered: unsupported-form");
const E4010: (&str, &str) = ("E4010", "abi: too-many-register-arguments");
const E4013: (&str, &str) = ("E4013", "abi: too-many-results");

// Section 6's argument and result register counts, the stack-argument rule
// and the record, pair and variadic facts come from the target's generated
// ABI table (`targets/aarch64/abi-*`): "Integer and pointer arguments of at
// most 64 bits go in `x0` to `x7` in order", and "an EvoMIR function returns
// up to eight integer or pointer results in `x0` to `x7` in order ... A ninth
// result is `E4013`."

/// Native-only obligation lowering. Neither choice changes EvoIR or C semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UbCheckProfile {
    #[default]
    Checked,
    Unchecked,
}

impl UbCheckProfile {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        match value {
            "checked" => Ok(Self::Checked),
            "unchecked" => Ok(Self::Unchecked),
            _ => Err("UB-check profile must be checked or unchecked"),
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Checked => "checked",
            Self::Unchecked => "unchecked",
        }
    }
}

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
    lower_module_for_profile(target, module, external, UbCheckProfile::Checked)
}

/// The native profile is applied here, after proof replay, without editing IR.
pub fn lower_module_for_profile(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
    ub_checks: UbCheckProfile,
) -> Result<MModule, Vec<Refusal>> {
    lower_module_with_guard_control(target, module, external, ub_checks, &BTreeMap::new())
}

/// UNVERIFIED_DIAGNOSTIC, not a certified lowering or a driver profile.
/// Callers may measure object size only; artifacts cannot be installed or PASS.
pub fn lower_module_for_guard_attribution(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
    true_predicates: &BTreeMap<String, BTreeSet<u32>>,
) -> Result<MModule, Vec<Refusal>> {
    lower_module_with_guard_control(
        target,
        module,
        external,
        UbCheckProfile::Checked,
        true_predicates,
    )
}

fn lower_module_with_guard_control(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
    ub_checks: UbCheckProfile,
    true_predicates: &BTreeMap<String, BTreeSet<u32>>,
) -> Result<MModule, Vec<Refusal>> {
    let mut refusals = Vec::new();
    for function in &module.functions {
        scan_function(target, module, function, &mut refusals);
        if let Some(claimed) = true_predicates.get(&function.name) {
            let actual: BTreeSet<_> = function
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter(|i| {
                    matches!(
                        i,
                        Inst::DivisorNonZero(..)
                            | Inst::SignedDivRemNoOverflow(..)
                            | Inst::SignedAddDefined(..)
                            | Inst::SignedSubDefined(..)
                            | Inst::SignedMulDefined(..)
                            | Inst::ShiftCountDefined { .. }
                    )
                })
                .flat_map(evocc_evoir::inst_results)
                .map(|v| v.id)
                .collect();
            if !claimed.is_subset(&actual) {
                refusals.push(Refusal {
                    function: function.name.clone(),
                    block: function.blocks.first().map_or(0, |b| b.id),
                    index: 0,
                    code: E4009.0,
                    name: E4009.1,
                    detail: "guard attribution contains a non-predicate result".into(),
                });
            }
        }
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
    let crosses = |name: &str| external(module.symbol_target(name)) || declared.contains(name);
    let signatures = Signatures::of(module);

    let mut functions = Vec::new();
    for function in &module.functions {
        match lower_function(
            target,
            &signatures,
            function,
            &crosses,
            ub_checks,
            true_predicates
                .get(&function.name)
                .cloned()
                .unwrap_or_default(),
        ) {
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
        omit_unused_private_arguments(module, &mut functions);
        Ok(MModule {
            globals: module.globals.clone(),
            functions,
        })
    } else {
        Err(refusals)
    }
}

fn omit_unused_private_arguments(module: &Module, functions: &mut [MFunction]) {
    let unused: HashMap<_, _> = module
        .functions
        .iter()
        .zip(functions.iter())
        .filter(|(source, _)| {
            source.visibility == evocc_evoir::GlobalVisibility::Internal
                && !source.variadic
                && source
                    .param_marks
                    .iter()
                    .all(|mark| *mark == ParamMark::None)
        })
        .map(|(source, machine)| {
            (
                source.name.as_str(),
                crate::regalloc::unused_parameters(machine),
            )
        })
        .filter(|(_, mask)| mask.iter().any(|unused| *unused))
        .collect();
    if unused.is_empty() {
        return;
    }
    for function in functions {
        let mut next = function
            .blocks
            .iter()
            .flat_map(|b| {
                b.args
                    .iter()
                    .copied()
                    .chain(b.insts.iter().flat_map(crate::inst_defs))
            })
            .map(|v| v.id)
            .max()
            .unwrap_or(0)
            + 1;
        for block in &mut function.blocks {
            let mut rewritten = Vec::with_capacity(block.insts.len());
            for mut inst in std::mem::take(&mut block.insts) {
                if let MInst::Bl { symbol, args, .. } = &mut inst {
                    if let Some(mask) = unused
                        .get(symbol.as_str())
                        .filter(|mask| mask.len() == args.len())
                    {
                        for (arg, unused) in args.iter_mut().zip(mask) {
                            if *unused {
                                let value = VReg::new(next, arg.class);
                                next += 1;
                                rewritten.push(MInst::MovImm { dst: value, imm: 0 });
                                *arg = value;
                            }
                        }
                    }
                }
                rewritten.push(inst);
            }
            block.insts = rewritten;
        }
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
        EvoIRType::F32 => Ok(RegClass::Simd32),
        EvoIRType::F64 => Ok(RegClass::Simd64),
        EvoIRType::F128 => Ok(RegClass::Simd128),
        EvoIRType::Vec { .. } => match vector_bytes(ty) {
            Some(1 | 2 | 4) => Ok(RegClass::Gpr32),
            Some(8) => Ok(RegClass::Simd64),
            Some(16) => Ok(RegClass::Simd128),
            _ => Err(E4002),
        },
        EvoIRType::Tuple(_) => Err(E4004),
        _ => Err(E4009),
    }
}

/// Whether a type is the one Section 2 represents as a register pair.
fn is_pair(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::Bits(128))
}

fn vector_bytes(ty: &EvoIRType) -> Option<u64> {
    let (lanes, bits) = evocc_evoir::integer_vector_shape(ty)?;
    Some(u64::from(lanes) * u64::from(bits) / 8)
}

fn short_vector_bytes(ty: &EvoIRType) -> Option<u64> {
    vector_bytes(ty).filter(|bytes| matches!(bytes, 8 | 16))
}

fn simd_argument_bytes(ty: &EvoIRType) -> Option<u64> {
    match ty {
        EvoIRType::F32 => Some(4),
        EvoIRType::F64 => Some(8),
        EvoIRType::F128 => Some(16),
        _ => short_vector_bytes(ty),
    }
}

fn simd_class(bytes: u64) -> RegClass {
    match bytes {
        4 => RegClass::Simd32,
        8 => RegClass::Simd64,
        16 => RegClass::Simd128,
        _ => unreachable!("validated SIMD size"),
    }
}
fn simd_load(bytes: u64) -> LoadKind {
    match bytes {
        4 => LoadKind::S,
        8 => LoadKind::D,
        16 => LoadKind::Q,
        _ => unreachable!("validated SIMD size"),
    }
}
fn simd_store(bytes: u64) -> StoreKind {
    match bytes {
        4 => StoreKind::S,
        8 => StoreKind::D,
        16 => StoreKind::Q,
        _ => unreachable!("validated SIMD size"),
    }
}

/// An HFA is packed as an object on the stack, not one eight-byte slot per
/// float member. These are private ABI transport pieces, not a new C type.
fn simd_argument_pieces(shape: RecordShape, stack: bool) -> Vec<(u64, u64)> {
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

/// The number of significant bits a value carries: Section 3's `N`.
fn significant_bits(ty: &EvoIRType) -> u16 {
    match ty {
        EvoIRType::Bits(width) => *width,
        EvoIRType::F32 => 32,
        EvoIRType::F128 => 128,
        EvoIRType::Vec { .. } => vector_bytes(ty).map_or(64, |bytes| bytes as u16 * 8),
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
    match reason {
        TrapReason::BuiltinTrap => 1000,
        _ => reason_immediate(reason.diagnostic()),
    }
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
    // Section 6 (revision 0.18): a record spends the registers its chunks take
    // and a record above sixteen bytes spends one, so the count that matters
    // is the convention's, not the parameter list's.
    let declared: Vec<EvoIRType> = function
        .params
        .iter()
        .map(|param| param.ty.clone())
        .collect();
    let marked = marked_types(&declared, &function.param_marks);
    if target != Target::Linux
        && (declared
            .iter()
            .chain(&function.ret_tys)
            .any(|t| *t == EvoIRType::F128)
            || function.param_marks.iter().any(|m| match m {
                ParamMark::ByVal(s) | ParamMark::Sret(s) => s
                    .floating
                    .is_some_and(|f| f.kind == evocc_types::FloatKind::F128),
                _ => false,
            }))
    {
        report(
            E4009,
            entry_block,
            0,
            "binary128 ABI is qualified only for Linux".into(),
        );
    }
    let plan = record_plan(target, &marked, true);
    // Section 10's `E4010` is "more than eight integer arguments on Darwin
    // (Linux lowers stack arguments, revision 0.10)": the ABI table's
    // `stack-arguments` fact.
    if plan.positions > target.abi().general_arguments() && !target.abi().facts.stack_arguments {
        report(
            E4010,
            entry_block,
            0,
            format!(
                "@{} takes {} register arguments",
                function.name, plan.positions
            ),
        );
    }
    // Section 6 puts the destination pointer of a record result above sixteen
    // bytes in `x8`, which the machine reaches through the call's last
    // argument, and EvoIR 10.1.3 already puts it "after every declared
    // parameter". A mark anywhere else is a form this revision does not lower.
    if let Some(position) = marked
        .iter()
        .position(|(_, mark)| mark.is_sret())
        .filter(|position| *position + 1 != marked.len())
    {
        report(
            E4009,
            entry_block,
            0,
            format!(
                "@{} names its record return destination at parameter {position} of {}",
                function.name,
                marked.len()
            ),
        );
    }
    for ty in &params {
        if matches!(vector_bytes(ty), Some(1 | 2 | 4)) {
            report(
                E4002,
                entry_block,
                0,
                "sub-8-byte vector ABI differs between the sealed compilers".into(),
            );
        }
        if let Err(code) = class_of(ty) {
            report(code, entry_block, 0, format!("parameter of type {ty:?}"));
        }
    }
    let results = ordinary_types(&function.ret_tys);
    if results.len() > target.abi().general_results() {
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
        if matches!(vector_bytes(ty), Some(1 | 2 | 4)) {
            report(
                E4002,
                entry_block,
                0,
                "sub-8-byte vector ABI differs between the sealed compilers".into(),
            );
        }
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

fn scan_inst(
    target: Target,
    module: &Module,
    inst: &Inst,
    block: u32,
    index: usize,
    report: &mut impl FnMut((&'static str, &'static str), u32, usize, String),
) {
    if target != Target::Linux {
        if inst_results(inst)
            .into_iter()
            .chain(inst_uses(inst))
            .any(|v| v.ty == EvoIRType::F128)
        {
            report(
                E4009,
                block,
                index,
                "binary128 is qualified only for Linux".into(),
            );
        }
        if let Inst::StrictFloating(op) = inst {
            if matches!(
                op.instruction.op,
                evocc_evoir::FloatingOp::FromSigned
                    | evocc_evoir::FloatingOp::FromUnsigned
                    | evocc_evoir::FloatingOp::ToSigned
                    | evocc_evoir::FloatingOp::ToUnsigned
            ) && op
                .instruction
                .arguments
                .iter()
                .chain(std::iter::once(&op.instruction.result))
                .any(|v| is_pair(&v.ty))
            {
                report(E4009,block,index,"strict integer128 floating helpers are qualified only for the Linux hosted runtime".into());
            }
        }
    }
    if let Inst::Call { args, results, .. } | Inst::IndirectCall { args, results, .. } = inst {
        if args
            .iter()
            .chain(results)
            .any(|value| matches!(vector_bytes(&value.ty), Some(1 | 2 | 4)))
        {
            report(
                E4002,
                block,
                index,
                "sub-8-byte vector ABI differs between the sealed compilers".into(),
            );
        }
    }
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
        Inst::Vector(_) => {}
        Inst::Floating(_) | Inst::StrictFloating(_) => {}
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
                if matches!(vector_bytes(&result.ty), Some(1 | 2 | 4)) {
                    report(
                        E4002,
                        block,
                        index,
                        "sub-8-byte vector ABI differs between the sealed compilers".into(),
                    );
                }
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
        // Section 4 carries both assembly forms through the same vocabulary.
        Inst::Asm(assembly) | Inst::AsmGoto { assembly, .. } => {
            if let Err(detail) = assembly_vocabulary(assembly) {
                report(E4009, block, index, detail);
            }
        }
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
        | Inst::DynamicAlloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. }
        | Inst::Br(..)
        | Inst::IndirectBr { .. }
        | Inst::CondBr(..)
        | Inst::Ret(_)
        | Inst::Trap(_)
        | Inst::UbAssume(..) => {}
        Inst::Atomic(access) => {
            if let Err(detail) = access.validate() {
                report(E4009, block, index, detail);
            }
        }
        Inst::TlsAddr(access) => {
            if let Err(detail) = access.validate() {
                report(E4009, block, index, detail);
            }
        }
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
            arg_marks,
            ..
        } => {
            let marks = module
                .functions
                .iter()
                .find(|function| &function.name == callee)
                .map(|function| function.param_marks.as_slice())
                .or_else(|| {
                    module
                        .declares
                        .iter()
                        .find(|declaration| &declaration.name == callee)
                        .map(|declaration| declaration.param_marks.as_slice())
                })
                .unwrap_or(&[]);
            let marks = call_argument_marks(marks, arg_marks, args.len());
            let positions =
                record_plan(target, &marked_types(&arg_types(args), &marks), true).positions;
            if positions > target.abi().general_arguments() && !target.abi().facts.stack_arguments {
                report(
                    E4010,
                    block,
                    index,
                    format!("a call to @{callee} passes {positions} arguments"),
                );
            }
            if results.len() > target.abi().general_results() {
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
                .any(|function| function.name == module.symbol_target(callee))
                && !module
                    .declares
                    .iter()
                    .any(|declaration| &declaration.name == callee)
            {
                report(E4009, block, index, format!("a call to absent @{callee}"));
            }
        }
        Inst::IndirectCall {
            results,
            args,
            callee,
            arg_marks,
            ..
        } => {
            let marks: Vec<ParamMark> = match &callee.ty {
                EvoIRType::FnPtr { marks, .. } => marks.clone(),
                _ => Vec::new(),
            };
            let marks = call_argument_marks(&marks, arg_marks, args.len());
            let positions =
                record_plan(target, &marked_types(&arg_types(args), &marks), true).positions;
            if positions > target.abi().general_arguments() && !target.abi().facts.stack_arguments {
                report(
                    E4010,
                    block,
                    index,
                    format!("an indirect call passes {positions} arguments"),
                );
            }
            if results.len() > target.abi().general_results() {
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
        EvoIRType::Vec { .. } if matches!(vector_bytes(ty), Some(1 | 2 | 4 | 8 | 16)) => None,
        EvoIRType::Ptr(_)
        | EvoIRType::FnPtr { .. }
        | EvoIRType::F32
        | EvoIRType::F64
        | EvoIRType::F128 => None,
        // Sixteen is the pair Section 5 accesses with `ldp`/`stp`.
        EvoIRType::Bits(width) if matches!(width.div_ceil(8), 1 | 2 | 4 | 8 | 16) => None,
        other => Some(format!("an access of type {other:?} has no matching width")),
    }
}

// ---------------------------------------------------------------------------
// Lowering
// ---------------------------------------------------------------------------

struct Lowerer {
    /// The selected operating system's generated ABI table.
    abi: &'static Aarch64Abi,
    ub_checks: UbCheckProfile,
    diagnostic_true_predicates: BTreeSet<u32>,
    dynamic_addresses: HashSet<u32>,
    access_alignments: HashMap<u32, u32>,
    function_name: String,
    next_vreg: u32,
    next_block: u32,
    map: HashMap<u32, VReg>,
    /// The high half of every value Section 2 represents as a pair; `map`
    /// holds the low half, which is the half every ordinary row already reads.
    high: HashMap<u32, VReg>,
    blocks: Vec<MBlock>,
    traps: Vec<(u16, u32)>,
    /// Named argument register count and incoming stack extent in bytes.
    /// `None` in a fixed function; both are from the call placement plan.
    named_arguments: Option<(u8, u8, u64)>,
    /// Every `const` this function has lowered, by value id.
    ///
    /// EvoIR Section 3.11 admits immediate assembly operands (`I`, `J`, `K`,
    /// `L`, `i`, `n`), which emission "writes literally"; the constant is a
    /// prior definition in the same function, and this is where an assembly
    /// operand finds it.
    constants: HashMap<u32, u64>,
    /// Direct `ptr.null` definitions only, not inferred integer addresses.
    null_pointers: HashSet<u32>,
    symbolic_addresses: HashMap<u32, (String, i64)>,
    literal_only: HashSet<u32>,
    /// Section 6 (revision 0.18): the shape of the record result this function
    /// returns in `x0`/`x1`, and the frame slot the entry block gave it.
    destination: Option<(RecordShape, VReg)>,
    /// The register each block's `ret` reads the record result out of, which
    /// `thread_destination` turns into that block's extra argument.
    destination_readers: HashMap<u32, VReg>,
    /// How many record frame slots this function has taken an origin for.
    record_origins: u64,
    /// Only pure address preparation already executed in the current machine
    /// block. Clear on every block boundary, including guard/varargs splits;
    /// these temporaries are not implicit inputs to a continuation.
    address_preparation: AddressPreparation,
}

#[derive(Default)]
struct AddressPreparation {
    extended: HashMap<(VReg, u16, evocc_evoir::IndexSignedness), VReg>,
    scaled: HashMap<(VReg, u64), VReg>,
    factors: HashMap<u64, VReg>,
    memory: HashMap<u32, (VReg, i64, usize)>,
}

impl Lowerer {
    fn immediate_pointer_delta(
        &self,
        index: &Value,
        direction: evocc_evoir::OffsetDirection,
        signedness: evocc_evoir::IndexSignedness,
        scale: u64,
    ) -> Option<i64> {
        let EvoIRType::Bits(bits @ 1..=64) = index.ty else {
            return None;
        };
        let raw = *self.constants.get(&index.id)? & (u64::MAX >> (64 - bits));
        let value = match signedness {
            evocc_evoir::IndexSignedness::Signed => {
                i128::from(((raw << (64 - bits)) as i64) >> (64 - bits))
            }
            evocc_evoir::IndexSignedness::Unsigned => i128::from(raw),
        };
        let delta = value.checked_mul(i128::from(scale))?;
        i64::try_from(match direction {
            evocc_evoir::OffsetDirection::Add => delta,
            evocc_evoir::OffsetDirection::Subtract => delta.checked_neg()?,
        })
        .ok()
    }

    fn memory_address(
        &self,
        insts: &[MInst],
        address: &Value,
        size: u64,
        align: u32,
    ) -> Result<(VReg, i64), String> {
        let original = self.reg(address)?;
        let Some(&(base, offset, after)) = self.address_preparation.memory.get(&address.id) else {
            return Ok((original, 0));
        };
        // Select one already-computed ptr.offset, not an arbitrary integer
        // expression or a chain of offsets. Do not move pointer formation over
        // a call, access, asm, guard, or other potentially failing computation.
        // AddressPreparation is also cleared on every machine-block boundary.
        let uninterrupted = insts[after..]
            .iter()
            .all(|inst| matches!(inst, MInst::MovImm { .. }));
        let encodable = (-256..=255).contains(&offset)
            || (offset >= 0
                && (offset as u64).is_multiple_of(size)
                && offset as u64 / size <= 4095);
        // Strict-align emission may expand the access to byte instructions.
        // Every byte offset must fit too, even if the wide scaled offset fits.
        let byte_safe = u64::from(align) >= size
            || (offset >= -256
                && offset
                    .checked_add(size as i64 - 1)
                    .is_some_and(|end| end <= 4095));
        Ok(if uninterrupted && encodable && byte_safe {
            (base, offset)
        } else {
            (original, 0)
        })
    }

    fn pointer_delta(
        &mut self,
        insts: &mut Vec<MInst>,
        index: &Value,
        signedness: evocc_evoir::IndexSignedness,
        scale: u64,
    ) -> Result<VReg, String> {
        // Address arithmetic is modulo the machine pointer width. The EvoIR
        // interpreter checks the full-width delta before access.
        let bits = significant_bits(&index.ty).min(64);
        let raw = if index.ty == EvoIRType::Bits(128) {
            self.pair(index)?.0
        } else {
            self.reg(index)?
        };
        let key = (raw, bits, signedness);
        let extended = if let Some(reg) = self.address_preparation.extended.get(&key) {
            *reg
        } else {
            // Decode the index according to its own width and signedness,
            // not the base pointer or another use of the same raw bits.
            let reg = match signedness {
                evocc_evoir::IndexSignedness::Signed => {
                    self.sign_extend(insts, raw, bits, Width::X)
                }
                evocc_evoir::IndexSignedness::Unsigned => {
                    self.zero_extend(insts, raw, bits, Width::X)
                }
            };
            let reg = if reg.class == RegClass::Gpr64 {
                reg
            } else {
                let dst = self.fresh(RegClass::Gpr64);
                insts.push(MInst::Mov { dst, src: reg });
                dst
            };
            self.address_preparation.extended.insert(key, reg);
            reg
        };
        if scale == 1 {
            return Ok(extended);
        }
        if let Some(delta) = self.address_preparation.scaled.get(&(extended, scale)) {
            return Ok(*delta);
        }
        // Section 5: shift for powers of two, multiply otherwise. Reusing a
        // previous delta never reuses the base, a load, or an access check.
        let delta = self.fresh(RegClass::Gpr64);
        if scale.is_power_of_two() {
            insts.push(MInst::BinaryImm {
                op: BinOp::Lsl,
                dst: delta,
                lhs: extended,
                imm: u64::from(scale.trailing_zeros()),
                width: Width::X,
            });
        } else {
            let factor = if let Some(factor) = self.address_preparation.factors.get(&scale) {
                *factor
            } else {
                let factor = self.fresh(RegClass::Gpr64);
                insts.push(MInst::MovImm {
                    dst: factor,
                    imm: scale,
                });
                self.address_preparation.factors.insert(scale, factor);
                factor
            };
            insts.push(MInst::Binary {
                op: BinOp::Mul,
                dst: delta,
                lhs: extended,
                rhs: factor,
                width: Width::X,
            });
        }
        self.address_preparation
            .scaled
            .insert((extended, scale), delta);
        Ok(delta)
    }

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
        if is_pair(&amount.ty) {
            // A high half of zero is necessary for every legal count,
            // regardless of whether the full source value is signed.
            let (low, high) = self.pair(amount)?;
            let flags = self.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: low,
                imm: u64::from(bound),
                width: Width::X,
            });
            let below = self.fresh(RegClass::Gpr32);
            insts.push(MInst::Cset {
                dst: below,
                flags,
                cond: Cond::Lo,
            });
            let zero = self.word(insts, 0);
            let high_zero = self.compare_into(insts, high, zero, Width::X, Cond::Eq);
            return Ok(self.both(insts, below, high_zero));
        }
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

    fn shift(
        &mut self,
        insts: &mut Vec<MInst>,
        op: BinOp,
        result: &Value,
        value: VReg,
        amount: &Value,
    ) -> Result<(), String> {
        let bits = significant_bits(&amount.ty);
        // Decode the count at its own width. Never use the machine's modulo
        // shift masking to justify selection or bypass the source UB guard.
        // Pair shifts and counts remain on their existing lowering path.
        let immediate = self
            .constants
            .get(&amount.id)
            .copied()
            .filter(|_| bits <= 64)
            .map(|raw| {
                if bits == 64 {
                    raw
                } else {
                    raw & ((1_u64 << bits) - 1)
                }
            })
            .filter(|count| *count < u64::from(significant_bits(&result.ty)));
        let dst = self.fresh(value.class);
        let width = width_of(value.class);
        if let Some(imm) = immediate {
            insts.push(MInst::BinaryImm {
                op,
                dst,
                lhs: value,
                imm,
                width,
            });
        } else {
            let rhs = self.shift_amount(insts, amount)?;
            insts.push(MInst::Binary {
                op,
                dst,
                lhs: value,
                rhs,
                width,
            });
        }
        self.bind(result, dst);
        Ok(())
    }
}

/// Must-originate analysis for automatic addresses passed through local SSA
/// edges. Unknown pointers and mixed fixed/dynamic joins are not reclaimed.
fn dynamic_addresses(function: &Function) -> HashSet<u32> {
    let mut roots = HashSet::new();
    let mut incoming: HashMap<u32, Vec<u32>> = HashMap::new();
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<HashMap<_, _>>();
    for block in &function.blocks {
        for inst in &block.insts {
            match inst {
                Inst::DynamicAlloca { result, .. } => {
                    roots.insert(result.id);
                }
                Inst::PointerCast { result, operand }
                    if matches!(result.ty, EvoIRType::Ptr(_))
                        && matches!(operand.ty, EvoIRType::Ptr(_)) =>
                {
                    incoming.entry(result.id).or_default().push(operand.id);
                }
                _ => {}
            }
        }
        let mut edge = |target, args: &[Value]| {
            if let Some(target) = blocks.get(&target) {
                for (param, arg) in target.args.iter().zip(args) {
                    if matches!(param.value.ty, EvoIRType::Ptr(_)) {
                        incoming.entry(param.value.id).or_default().push(arg.id);
                    }
                }
            }
        };
        match &block.terminator {
            Inst::Br(target, args) => edge(*target, args),
            Inst::CondBr(_, left, a, right, b) => {
                edge(*left, a);
                edge(*right, b);
            }
            Inst::IndirectBr { edges, .. } => {
                for e in edges {
                    edge(e.target, &e.arguments);
                }
            }
            Inst::AsmGoto {
                labels,
                fallthrough,
                ..
            } => {
                for label in labels {
                    edge(label.edge.target, &label.edge.arguments);
                }
                edge(fallthrough.target, &fallthrough.arguments);
            }
            _ => {}
        }
    }
    let mut possible = roots.clone();
    possible.extend(incoming.keys().copied());
    loop {
        let before = possible.len();
        let remove = incoming
            .iter()
            .filter_map(|(id, args)| {
                (!roots.contains(id) && args.iter().any(|arg| !possible.contains(arg)))
                    .then_some(*id)
            })
            .collect::<Vec<_>>();
        for id in remove {
            possible.remove(&id);
        }
        if possible.len() == before {
            return possible;
        }
    }
}

fn lower_function(
    target: Target,
    signatures: &Signatures<'_>,
    function: &Function,
    external: &dyn Fn(&str) -> bool,
    ub_checks: UbCheckProfile,
    diagnostic_true_predicates: BTreeSet<u32>,
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
        abi: target.abi(),
        ub_checks,
        diagnostic_true_predicates,
        dynamic_addresses: dynamic_addresses(function),
        access_alignments: HashMap::new(),
        function_name: function.name.clone(),
        next_vreg: 0,
        next_block: u32::try_from(function.blocks.len()).unwrap_or(u32::MAX),
        map: HashMap::new(),
        high: HashMap::new(),
        blocks: Vec::new(),
        traps: Vec::new(),
        named_arguments: function
            .variadic
            .then(|| named_argument_extent(target, function)),
        constants: HashMap::new(),
        null_pointers: HashSet::new(),
        symbolic_addresses: HashMap::new(),
        literal_only: HashSet::new(),
        destination: None,
        destination_readers: HashMap::new(),
        record_origins: 0,
        address_preparation: AddressPreparation::default(),
    };

    for (index, block) in function.blocks.iter().enumerate() {
        let id = ids[&block.id];
        lower_block(
            target,
            signatures,
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
    // Section 6 (revision 0.18): a record result of at most sixteen bytes is
    // loaded into `x0`/`x1` "by the callee ... before `ret`", out of the frame
    // slot the entry block gave the destination. Every block that returns
    // needs that address, so it is threaded through the block arguments the
    // Section 2 discipline requires.
    if let Some((_, slot)) = lowerer.destination {
        let readers = lowerer.destination_readers.clone();
        let mut next = lowerer.next_vreg;
        thread_destination(&mut lowerer.blocks, slot, &readers, &mut next);
        lowerer.next_vreg = next;
    }
    Ok(MFunction {
        name: function.name.clone(),
        blocks: lowerer.blocks,
    })
}

/// The record marks of every callee this module can name.
pub(crate) struct Signatures<'a> {
    tls: &'a [evocc_evoir::TlsSymbol],
    functions: HashMap<&'a str, &'a [ParamMark]>,
}

impl<'a> Signatures<'a> {
    fn of(module: &'a Module) -> Self {
        let mut functions: HashMap<&'a str, &'a [ParamMark]> = HashMap::new();
        for function in &module.functions {
            functions.insert(function.name.as_str(), function.param_marks.as_slice());
        }
        for declaration in &module.declares {
            functions.insert(
                declaration.name.as_str(),
                declaration.param_marks.as_slice(),
            );
        }
        for alias in &module.aliases {
            if let Some(marks) = functions.get(alias.target.as_str()).copied() {
                functions.insert(alias.name.as_str(), marks);
            }
        }
        Self {
            functions,
            tls: &module.tls,
        }
    }

    fn marks(&self, callee: &str) -> &[ParamMark] {
        self.functions.get(callee).copied().unwrap_or(&[])
    }
}

/// The register-typed values an edge passes.
fn edge_args(lowerer: &Lowerer, values: &[Value]) -> Result<Vec<VReg>, String> {
    let mut registers = Vec::new();
    for value in values.iter().filter(|value| is_register_type(&value.ty)) {
        registers.extend(lowerer.registers(value)?);
    }
    Ok(registers)
}

#[allow(clippy::too_many_arguments)]
fn lower_block(
    target: Target,
    signatures: &Signatures<'_>,
    lowerer: &mut Lowerer,
    function: &Function,
    block: &Block,
    id: u32,
    ids: &HashMap<u32, u32>,
    external: &dyn Fn(&str) -> bool,
    entry: bool,
) -> Result<(), String> {
    lowerer.address_preparation = AddressPreparation::default();
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
        .map(|marked| record_plan(target, marked, true));
    let mut args = Vec::new();
    let mut prologue: Vec<MInst> = Vec::new();
    let mut indirect_result = false;
    let mut ordinary = 0_usize;
    for (index, arg) in block.args.iter().enumerate() {
        if !is_register_type(&arg.value.ty) {
            continue;
        }
        let placement = plan.as_ref().map(|plan| plan.items[ordinary].clone());
        ordinary += 1;
        if let Some(Placed { fillers, passing }) = placement {
            for _ in 0..fillers {
                args.push(lowerer.fresh(RegClass::Gpr64));
            }
            match passing {
                // Section 6: "the callee stores them into a frame slot that
                // becomes the parameter's place".
                Passing::Chunks(chunks) => {
                    let slot = lowerer.fresh(RegClass::Gpr64);
                    let shape = param_mark(&function.param_marks, index)
                        .shape()
                        .expect("a chunked argument is a marked record");
                    prologue.push(MInst::FrameSlot {
                        dst: slot,
                        origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
                        size: shape.size,
                        align: shape.align,
                    });
                    lowerer.record_origins += 1;
                    let mut offset = 0;
                    for size in chunks {
                        let chunk = lowerer.fresh(RegClass::Gpr64);
                        args.push(chunk);
                        store_chunk(lowerer, &mut prologue, chunk, slot, offset, size);
                        offset += size;
                    }
                    lowerer.bind(&arg.value, slot);
                    continue;
                }
                Passing::Vectors {
                    shape,
                    skipped,
                    stack,
                } => {
                    for _ in 0..skipped {
                        args.push(lowerer.fresh(RegClass::Simd64));
                    }
                    let slot = lowerer.fresh(RegClass::Gpr64);
                    prologue.push(MInst::FrameSlot {
                        dst: slot,
                        origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
                        size: shape.size,
                        align: shape.align,
                    });
                    lowerer.record_origins += 1;
                    for (offset, size) in simd_argument_pieces(shape, stack) {
                        let reg = lowerer.fresh(simd_class(size));
                        args.push(reg);
                        prologue.push(MInst::Store {
                            align: shape.align.min(size as u32),
                            src: reg,
                            base: slot,
                            offset: offset as i64,
                            kind: simd_store(size),
                            volatile: false,
                        });
                    }
                    lowerer.bind(&arg.value, slot);
                    continue;
                }
                // Section 6: at most sixteen bytes the destination is the
                // callee's own, and `ret` loads the result out of it; above
                // sixteen the caller's pointer arrives in `x8`, which the
                // marker names and allocation pins.
                Passing::Destination(shape) => {
                    if shape.size <= lowerer.abi.record_register_bytes
                        || shape.simd_members().is_some()
                    {
                        let slot = lowerer.fresh(RegClass::Gpr64);
                        prologue.push(MInst::FrameSlot {
                            dst: slot,
                            origin: RECORD_ORIGIN_BASE + lowerer.record_origins,
                            size: shape.size,
                            align: shape.align,
                        });
                        lowerer.record_origins += 1;
                        lowerer.bind(&arg.value, slot);
                        lowerer.destination = Some((shape, slot));
                        continue;
                    }
                    indirect_result = true;
                    let reg = lowerer.fresh(RegClass::Gpr64);
                    lowerer.bind(&arg.value, reg);
                    args.push(reg);
                    continue;
                }
                Passing::Pointer | Passing::Value => {}
            }
        }
        if is_pair(&arg.value.ty) {
            // Section 6 rounds the next register number up to even before a
            // pair. The skipped position is a real argument nothing reads, so
            // that Section 7's positional pinning gives the halves the numbers
            // the convention names. Inside the function the rule has no
            // meaning, so only the entry block carries it.
            if entry && plan.is_none() && args.len() % 2 == 1 {
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
    // Section 6 (revision 0.18): the marker allocation reads is the first
    // thing in the entry block, before the register save area.
    if indirect_result {
        insts.push(MInst::IndirectResult);
    }
    // Section 6: the register save area is filled "in the prologue", before
    // anything writes an argument register. The marker is the first thing in
    // the entry block, which is where the frame layout finds it.
    if entry {
        if let Some((named, _, _)) = lowerer.named_arguments {
            insts.push(MInst::VaSaveArea { named });
        }
    }
    insts.extend(prologue);
    // Set when an `icmp` was fused into the block's `cond_br`.
    let mut fused: Option<(VReg, Cond)> = None;
    // A checked full-width add/sub already produces its arithmetic value.
    // Its use after the guard still enters through an explicit block argument.
    let mut computed_result = None;
    // A matched pure selection chain is emitted at its comparison. It contains
    // no obligation, effect or externally used intermediate to skip.
    let mut selected_until = 0;
    lowerer.literal_only = assembly_literal_definitions(block);
    // Count once, including all edge operands: checking each guard must not
    // rescan a large source block and make instruction selection quadratic.
    let use_counts = block_use_counts(block);
    // Shared A/B baseline: only direct, canonical bits<1> true is discharged.
    // A fact about a predicate, an alias, or a masked integer is not enough.
    let canonical_true: HashSet<_> = block
        .insts
        .iter()
        .filter_map(|inst| match inst {
            Inst::Const(value, 1) if value.ty == EvoIRType::Bits(1) => Some(value.id),
            _ => None,
        })
        .collect();

    for (index, inst) in block.insts.iter().enumerate() {
        if index < selected_until {
            continue;
        }
        if computed_result == Some(index) {
            computed_result = None;
            continue;
        }
        if let Inst::UbAssume(condition, reason) = inst {
            if canonical_true.contains(&condition.id) {
                continue;
            }
            if lowerer.diagnostic_true_predicates.contains(&condition.id) {
                continue;
            }
            if lowerer.ub_checks == UbCheckProfile::Unchecked {
                continue;
            }
            // Section 5: the guard still branches to its reason's trap. An
            // adjacent, single-use CSET can give its flags directly to that
            // branch; otherwise test the materialized boolean's low bit.
            // Instructions after the obligation enter a continuation through
            // explicit block arguments, whichever branch spelling is selected.
            let checked_flags =
                take_checked_flags(&mut insts, lowerer, block, &use_counts, index, condition)?;
            let condition = lowerer.reg(condition)?;
            let imm = ub_immediate(*reason);
            let trap = lowerer.trap_block(imm);
            // If the next pure arithmetic instruction has already been
            // computed by its predicate, transport its result, not operands
            // used only by that otherwise-repeated instruction.
            let from = index + 1 + usize::from(computed_result == Some(index + 1));
            let carried = live_across(lowerer, block, from);
            let continuation = lowerer.fresh_block();
            let mut continuation_args = Vec::new();
            let mut rebind = HashMap::new();
            for reg in &carried {
                let fresh = lowerer.fresh(reg.class);
                continuation_args.push(fresh);
                rebind.insert(reg.id, fresh);
            }
            let term = if let Some((flags, valid_when)) = checked_flags {
                MTerm::BCond {
                    flags,
                    cond: valid_when.inverted(),
                    taken: trap,
                    taken_args: Vec::new(),
                    untaken: continuation,
                    untaken_args: carried,
                }
            } else {
                MTerm::Tb {
                    nonzero: false,
                    src: condition,
                    bit: 0,
                    taken: trap,
                    taken_args: Vec::new(),
                    untaken: continuation,
                    untaken_args: carried,
                }
            };
            lowerer.blocks.push(MBlock {
                id: current_id,
                args: std::mem::take(&mut current_args),
                insts: std::mem::take(&mut insts),
                term,
            });
            // Both limbs are local SSA values. A checked operation splits
            // the block, so the continuation must use both new bindings.
            for reg in lowerer.map.values_mut().chain(lowerer.high.values_mut()) {
                if let Some(fresh) = rebind.get(&reg.id) {
                    *reg = *fresh;
                }
            }
            current_id = continuation;
            current_args = continuation_args;
            lowerer.address_preparation = AddressPreparation::default();
            continue;
        }

        // Section 6's `va_arg`: "the register-then-stack procedure of Section 6
        // for `T`'s size and alignment, ending in a load of `T` from the
        // computed address". The procedure branches, so it is written here,
        // where a block can be split, exactly as the `ub.assume` row above is.
        if let Inst::VaAccess(access) = inst {
            if let Some((slot, alignment)) = variadic_access_slot(&access.op) {
                let vector_group = match &access.op {
                    VariadicOp::Arg(result) => {
                        simd_argument_bytes(&result.ty).map(|bytes| (1, bytes))
                    }
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
                let register_slot =
                    vector_group.map_or(slot, |(count, _)| count * vector_save_bytes);
                let ap = lowerer.reg(&access.list)?;
                let mut carried = live_across(lowerer, block, index + 1);
                // The destination is an input of this instruction, so a last
                // use still has to cross the register/stack selection blocks.
                let destination_index = if let VariadicOp::ArgRecord { destination, .. } =
                    &access.op
                {
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

                // `offs = gr_offs; if offs < 0:` -- the field is a signed
                // thirty-two-bit integer, so it is read sign-extended.
                let offs = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Load {
                    align: 0,
                    dst: offs,
                    base: ap,
                    offset: offsets_field,
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
                let aligned = if alignment == 16 && vector_group.is_none() {
                    round_to_sixteen(lowerer, &mut body, offset)
                } else {
                    offset
                };
                let next =
                    lowerer.binary_imm(&mut body, BinOp::Add, aligned, register_slot, Width::X);
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
                    align: 0,
                    src: next,
                    base: list,
                    offset: offsets_field,
                    kind: StoreKind::W,
                    volatile: false,
                });
                let top = lowerer.fresh(RegClass::Gpr64);
                body.push(MInst::Load {
                    align: 0,
                    dst: top,
                    base: list,
                    offset: top_field,
                    kind: LoadKind::P,
                    volatile: false,
                });
                let address = lowerer.binary(&mut body, BinOp::Add, top, aligned, Width::X);
                if let (Some((count, bytes)), Some(destination)) = (vector_group, destination_index)
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
                        });
                        body.push(MInst::Store {
                            src: v,
                            base: params[destination],
                            offset: (i * bytes) as i64,
                            kind: simd_store(bytes),
                            volatile: false,
                            align: bytes as u32,
                        });
                    }
                }
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
                        align: 0,
                        src: zero,
                        base: list,
                        offset: offsets_field,
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
                    align: 0,
                    dst: raw,
                    base: list,
                    offset: va_list.stack,
                    kind: LoadKind::P,
                    volatile: false,
                });
                let address = if alignment == 16 {
                    round_to_sixteen(lowerer, &mut body, raw)
                } else {
                    raw
                };
                let bumped = lowerer.binary_imm(&mut body, BinOp::Add, address, slot, Width::X);
                if let (Some(_), Some(destination), VariadicOp::ArgRecord { shape, .. }) =
                    (vector_group, destination_index, &access.op)
                {
                    body.push(MInst::MemCopy {
                        dst: params[destination],
                        src: address,
                        size: shape.size,
                        align: shape.align.min(alignment as u32),
                        volatile: evocc_memory::CopyVolatility::None,
                    });
                }
                body.push(MInst::Store {
                    align: 0,
                    src: bumped,
                    base: list,
                    offset: va_list.stack,
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
                if let VariadicOp::ArgRecord { shape, .. } = &access.op {
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
                            });
                            src
                        } else {
                            address
                        };
                        body.push(MInst::MemCopy {
                            dst: params[destination_index.expect("record destination is carried")],
                            src,
                            size: shape.size,
                            align: shape.align.min(8),
                            volatile: evocc_memory::CopyVolatility::None,
                        });
                    }
                } else if let VariadicOp::Arg(result) = &access.op {
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
                        });
                        lowerer.bind(result, dst);
                    }
                }
                let mut rebind: HashMap<u32, VReg> = HashMap::new();
                for (old, fresh) in carried.iter().zip(params.iter()) {
                    rebind.insert(old.id, *fresh);
                }
                for reg in lowerer.map.values_mut().chain(lowerer.high.values_mut()) {
                    if let Some(fresh) = rebind.get(&reg.id) {
                        *reg = *fresh;
                    }
                }
                current_id = merge;
                current_args = params;
                insts = body;
                lowerer.address_preparation = AddressPreparation::default();
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
                    if condition.id == result.id && use_counts.get(&result.id) == Some(&1) {
                        let (flags, cond) =
                            lower_compare(lowerer, &mut insts, *operation, lhs, rhs)?;
                        fused = Some((flags, cond));
                        continue;
                    }
                }
            }
        }

        if let Some(value) = evocc_evoir::inst_results(inst)
            .first()
            .filter(|v| lowerer.diagnostic_true_predicates.contains(&v.id))
        {
            // Diagnostic only: retain every other data user of the predicate.
            let result = lowerer.fresh(RegClass::Gpr32);
            lowerer.bind(value, result);
            insts.push(MInst::MovImm {
                dst: result,
                imm: 1,
            });
        } else if let Some(length) =
            lower_compared_selection(lowerer, &mut insts, block, &use_counts, index)?
        {
            selected_until = index + length;
        } else if lowerer.ub_checks == UbCheckProfile::Checked
            && lower_checked_result(lowerer, &mut insts, block, index)?
        {
            computed_result = Some(index + 2);
        } else {
            lower_inst(target, signatures, lowerer, &mut insts, inst, external)?;
        }
    }

    let term = lower_terminator(lowerer, &mut insts, function, block, current_id, ids, fused)?;
    lowerer.blocks.push(MBlock {
        id: current_id,
        args: current_args,
        insts,
        term,
    });
    Ok(())
}

/// b ^ ((a ^ b) & -zext(icmp(x, y))) is exactly icmp(x, y) ? a : b.
/// Match only the contiguous, full-register-width chain over the compared
/// values; this is instruction selection, not speculative evaluation or DCE.
/// In particular no guard/effect moves, and the flags have one adjacent user.
fn lower_compared_selection(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    block: &Block,
    use_counts: &HashMap<u32, usize>,
    index: usize,
) -> Result<Option<usize>, String> {
    let remaining = &block.insts[index..];
    let [Inst::ICmp(condition, op, lhs, rhs), Inst::ZExt(extended, input, _), tail @ ..] =
        remaining
    else {
        return Ok(None);
    };
    if condition != input || !matches!(extended.ty, EvoIRType::Bits(32 | 64)) {
        return Ok(None);
    }
    let (local_zero, tail) = match tail {
        [Inst::Const(value, 0), tail @ ..] => (Some(value), tail),
        _ => (None, tail),
    };
    let [Inst::Sub(mask, zero, amount), Inst::Xor(difference, a, b), Inst::And(masked, diff, bits), Inst::Xor(result, fallback, chosen), ..] =
        tail
    else {
        return Ok(None);
    };
    if amount != extended
        || diff != difference
        || bits != mask
        || fallback != b
        || chosen != masked
        || !((a == lhs && b == rhs) || (a == rhs && b == lhs))
        || [lhs, rhs, zero, mask, difference, masked, result]
            .iter()
            .any(|value| value.ty != extended.ty)
        || [condition, extended, mask, difference, masked]
            .iter()
            .any(|value| use_counts.get(&value.id) != Some(&1))
        || [lhs, rhs]
            .iter()
            .any(|value| lowerer.symbolic_addresses.contains_key(&value.id))
    {
        return Ok(None);
    }
    if let Some(local_zero) = local_zero {
        if zero != local_zero || use_counts.get(&zero.id) != Some(&1) {
            return Ok(None);
        }
    } else if lowerer.constants.get(&zero.id) != Some(&0) {
        return Ok(None);
    }
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let selected = lowerer.fresh(class);
    let taken = lowerer.reg(a)?;
    let untaken = lowerer.reg(b)?;
    let (flags, cond) = lower_compare(lowerer, insts, *op, lhs, rhs)?;
    insts.push(MInst::Csel {
        dst: selected,
        flags,
        lhs: taken,
        rhs: untaken,
        cond,
        width: width_of(class),
    });
    lowerer.bind(result, selected);
    Ok(Some(6 + usize::from(local_zero.is_some())))
}

/// Select the existing full-width `adds/subs; cset vc` sequence once for
/// predicate / checked obligation / identical arithmetic. Nothing effectful
/// moves across the obligation, and it retains its ordinary checked branch.
fn lower_checked_result(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    block: &Block,
    index: usize,
) -> Result<bool, String> {
    let Some([predicate, Inst::UbAssume(condition, UbReason::SignedOverflow), arithmetic]) =
        block.insts.get(index..index + 3)
    else {
        return Ok(false);
    };
    let (check, result, lhs, rhs, a, b, op) = match (predicate, arithmetic) {
        (Inst::SignedAddDefined(check, lhs, rhs), Inst::Add(result, a, b)) => {
            (check, result, lhs, rhs, a, b, FlagOp::Adds)
        }
        (Inst::SignedSubDefined(check, lhs, rhs), Inst::Sub(result, a, b)) => {
            (check, result, lhs, rhs, a, b, FlagOp::Subs)
        }
        _ => return Ok(false),
    };
    if check != condition
        || lhs != a
        || rhs != b
        || result.ty != lhs.ty
        || !matches!(result.ty, EvoIRType::Bits(32 | 64))
        || lowerer.literal_only.contains(&result.id)
        || lowerer.symbolic_addresses.contains_key(&lhs.id)
        || lowerer.symbolic_addresses.contains_key(&rhs.id)
    {
        return Ok(false);
    }
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let value = lowerer.fresh(class);
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::FlagBinary {
        op,
        dst: value,
        flags,
        lhs: lowerer.reg(lhs)?,
        rhs: lowerer.reg(rhs)?,
        width: width_of(class),
    });
    let valid = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::Cset {
        dst: valid,
        flags,
        cond: Cond::Vc,
    });
    lowerer.bind(check, valid);
    lowerer.bind(result, value);
    Ok(true)
}

/// How many times each value is read in this block, terminator included.
fn block_use_counts(block: &Block) -> HashMap<u32, usize> {
    let mut counts = HashMap::new();
    for value in block
        .insts
        .iter()
        .chain(std::iter::once(&block.terminator))
        .flat_map(inst_uses)
    {
        *counts.entry(value.id).or_default() += 1;
    }
    counts
}

/// Fuse only the condition produced immediately before this obligation. The
/// boolean must have no other consumer (including an edge argument), and its
/// CSET must be adjacent to the flags producer. No effect or instruction moves.
fn take_checked_flags(
    insts: &mut Vec<MInst>,
    lowerer: &Lowerer,
    block: &Block,
    use_counts: &HashMap<u32, usize>,
    index: usize,
    condition: &Value,
) -> Result<Option<(VReg, Cond)>, String> {
    if use_counts.get(&condition.id) != Some(&1)
        || !index.checked_sub(1).is_some_and(|previous| {
            inst_results(&block.insts[previous])
                .iter()
                .any(|result| result.id == condition.id)
        })
    {
        return Ok(None);
    }
    let [.., producer, MInst::Cset { dst, flags, cond }] = insts.as_slice() else {
        return Ok(None);
    };
    if *dst != lowerer.reg(condition)? || !inst_defs(producer).contains(flags) {
        return Ok(None);
    }
    let selected = (*flags, *cond);
    insts.pop();
    Ok(Some(selected))
}

/// Assembly immediates are link-time operands, not runtime address loads.
/// Keep their pure definition closure for resolution without creating GPRs or
/// otherwise introducing references absent from the emitted template. Any
/// ordinary use, including an edge argument, still needs its original value.
fn assembly_literal_definitions(block: &Block) -> HashSet<u32> {
    let instructions = || block.insts.iter().chain(std::iter::once(&block.terminator));
    let mut immediates = Vec::new();
    for inst in instructions() {
        let (Inst::Asm(body) | Inst::AsmGoto { assembly: body, .. }) = inst else {
            continue;
        };
        for input in &body.inputs {
            if matches!(input.constraint.as_str(), "I" | "J" | "K" | "L" | "i" | "n") {
                immediates.push(input.value.id);
            }
        }
    }
    let mut literals = HashSet::new();
    if immediates.is_empty() {
        return literals;
    }
    let mut uses: HashMap<u32, usize> = HashMap::new();
    for value in instructions().flat_map(inst_uses) {
        *uses.entry(value.id).or_default() += 1;
    }
    for value in immediates {
        *uses.get_mut(&value).expect("counted assembly use") -= 1;
    }
    for inst in block.insts.iter().rev() {
        let result = match inst {
            Inst::Const(value, _) if matches!(value.ty, EvoIRType::Bits(1..=64)) => value,
            Inst::GlobalAddr { result, .. }
            | Inst::FuncAddr { result, .. }
            | Inst::BlockAddr { result, .. }
            | Inst::PointerCast { result, .. } => result,
            Inst::Add(value, ..) | Inst::Sub(value, ..) if value.ty == EvoIRType::Bits(64) => value,
            _ => continue,
        };
        if uses.get(&result.id).copied().unwrap_or(0) == 0 {
            literals.insert(result.id);
            for value in inst_uses(inst) {
                *uses.get_mut(&value.id).expect("counted definition use") -= 1;
            }
        }
    }
    literals
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
    // Null's target representation is zero, while the other operand keeps
    // its pointer identity in the MIR interpreter. No address exposure or
    // provenance inference is involved. EQ/NE are symmetric, so either
    // operand may be the directly defined null.
    if matches!(operation, CmpOp::Eq | CmpOp::Ne)
        && matches!(lhs.ty, EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. })
    {
        let selected = if lowerer.null_pointers.contains(&rhs.id) {
            Some(lhs)
        } else if lowerer.null_pointers.contains(&lhs.id) {
            Some(rhs)
        } else {
            None
        };
        if let Some(value) = selected {
            let register = lowerer.reg(value)?;
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: register,
                imm: 0,
                width: Width::X,
            });
            return Ok((flags, condition_of(operation)));
        }
    }
    // The admitted immediate is nonnegative in both full-width signed and
    // unsigned interpretations. Swapping already evaluated SSA operands needs
    // the reversed relation, not its logical negation. Narrow/pair and remaining
    // pointer comparisons retain their established extension/provenance paths.
    if matches!(lhs.ty, EvoIRType::Bits(32 | 64)) {
        let literal = |value: &Value| {
            lowerer
                .constants
                .get(&value.id)
                .copied()
                .filter(|v| *v <= 4095)
        };
        let selected = if let Some(imm) = literal(rhs) {
            Some((lhs, imm, condition_of(operation)))
        } else {
            literal(lhs).map(|imm| {
                let reversed = match operation {
                    CmpOp::Eq => Cond::Eq,
                    CmpOp::Ne => Cond::Ne,
                    CmpOp::SLt => Cond::Gt,
                    CmpOp::SLe => Cond::Ge,
                    CmpOp::ULt => Cond::Hi,
                    CmpOp::ULe => Cond::Hs,
                };
                (rhs, imm, reversed)
            })
        };
        if let Some((value, imm, condition)) = selected {
            let register = lowerer.reg(value)?;
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::CmpImm {
                dst: flags,
                lhs: register,
                imm,
                width: width_of(register.class),
            });
            return Ok((flags, condition));
        }
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
    current_id: u32,
    ids: &HashMap<u32, u32>,
    fused: Option<(VReg, Cond)>,
) -> Result<MTerm, String> {
    match &block.terminator {
        Inst::IndirectBr { target, edges } => Ok(MTerm::Indirect {
            target: lowerer.reg(target)?,
            edges: edges
                .iter()
                .map(|edge| {
                    Ok((
                        *ids.get(&edge.target)
                            .ok_or("indirect branch has a missing block")?,
                        edge_args(lowerer, &edge.arguments)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        }),
        Inst::AsmGoto {
            assembly,
            labels,
            fallthrough,
        } => {
            let mut body = lower_assembly(lowerer, insts, assembly)?;
            // EvoIR labels may be named or numbered after explicit operands;
            // EvoMIR uses declaration-order label positions. Escaped %% is
            // never a reference.
            let mut template = String::new();
            let mut rest = body.template.as_str();
            while let Some(at) = rest.find('%') {
                template.push_str(&rest[..at]);
                rest = &rest[at..];
                if rest.starts_with("%%") {
                    template.push_str("%%");
                    rest = &rest[2..];
                } else if let Some(reference) = rest.strip_prefix("%l[") {
                    let end = reference.find(']').ok_or("unterminated assembly label")?;
                    let name = &reference[..end];
                    let position = labels
                        .iter()
                        .position(|label| label.name == name)
                        .ok_or_else(|| format!("unknown assembly label {name}"))?;
                    template.push_str(&format!("%l[{position}]"));
                    rest = &reference[end + 1..];
                } else if let Some(reference) = rest.strip_prefix("%l").filter(|reference| {
                    reference.as_bytes().first().is_some_and(u8::is_ascii_digit)
                }) {
                    let end = reference.bytes().take_while(u8::is_ascii_digit).count();
                    let position = reference[..end]
                        .parse::<usize>()
                        .ok()
                        .and_then(|index| {
                            index.checked_sub(assembly.outputs.len() + assembly.inputs.len())
                        })
                        .filter(|index| *index < labels.len())
                        .ok_or("assembly label index outside label range")?;
                    template.push_str(&format!("%l[{position}]"));
                    rest = &reference[end..];
                } else {
                    template.push('%');
                    rest = &rest[1..];
                }
            }
            template.push_str(rest);
            body.template = template;
            let target = |id: &u32| {
                ids.get(id)
                    .copied()
                    .ok_or_else(|| format!("assembly branch to missing block {id}"))
            };
            // Bind the assembly results before translating either edge.
            // Like every machine edge, effect tokens have no registers.
            let labels = labels
                .iter()
                .map(|label| {
                    Ok((
                        target(&label.edge.target)?,
                        edge_args(lowerer, &label.edge.arguments)?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(MTerm::AsmGoto {
                body,
                labels,
                fallthrough: target(&fallthrough.target)?,
                fallthrough_args: edge_args(lowerer, &fallthrough.arguments)?,
            })
        }
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
            // Section 6 (revision 0.18): "An `sret` result of at most 16
            // bytes is loaded by the callee into `x0` and `x1` before `ret`".
            // The address comes in as this block's threaded argument.
            if let Some((shape, _)) = lowerer.destination {
                let base = lowerer.fresh(RegClass::Gpr64);
                lowerer.destination_readers.insert(current_id, base);
                if let Some((count, size)) = shape.simd_members() {
                    for member in 0..count {
                        let chunk = lowerer.fresh(simd_class(size));
                        insts.push(MInst::Load {
                            align: shape.align.min(size as u32),
                            dst: chunk,
                            base,
                            offset: i64::from(member) * size as i64,
                            kind: simd_load(size),
                            volatile: false,
                        });
                        results.push(chunk);
                    }
                    return Ok(MTerm::Ret { values: results });
                }
                let mut offset = 0;
                for size in record_chunks(shape.size) {
                    let chunk = load_chunk(lowerer, insts, base, offset, size);
                    results.push(chunk);
                    offset += size;
                }
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
    target: Target,
    signatures: &Signatures<'_>,
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    inst: &Inst,
    external: &dyn Fn(&str) -> bool,
) -> Result<(), String> {
    let symbolic = match inst {
        Inst::GlobalAddr { result, symbol } | Inst::FuncAddr { result, symbol } => {
            Some((result.id, (symbol.clone(), 0)))
        }
        Inst::BlockAddr { result, block } => Some((
            result.id,
            (format!("{}^b{block}", lowerer.function_name), 0),
        )),
        Inst::PointerCast { result, operand } => lowerer
            .symbolic_addresses
            .get(&operand.id)
            .cloned()
            .map(|address| (result.id, address)),
        Inst::Add(result, lhs, rhs) | Inst::Sub(result, lhs, rhs)
            if result.ty == EvoIRType::Bits(64) =>
        {
            let (base, offset, subtract) = if lowerer.symbolic_addresses.contains_key(&lhs.id) {
                (lhs, rhs, matches!(inst, Inst::Sub(..)))
            } else if matches!(inst, Inst::Add(..)) {
                (rhs, lhs, false)
            } else {
                (lhs, rhs, true)
            };
            lowerer
                .symbolic_addresses
                .get(&base.id)
                .and_then(|(name, addend)| {
                    let offset = *lowerer.constants.get(&offset.id)? as i64;
                    let addend = if subtract {
                        addend.checked_sub(offset)
                    } else {
                        addend.checked_add(offset)
                    }?;
                    Some((result.id, (name.clone(), addend)))
                })
        }
        _ => None,
    };
    if let Some((id, symbolic)) = symbolic {
        lowerer.symbolic_addresses.insert(id, symbolic);
    }
    if let Inst::Const(result, bits) = inst {
        lowerer.constants.insert(result.id, *bits);
    }
    if !lowerer.literal_only.is_empty()
        && inst_results(inst)
            .first()
            .is_some_and(|result| lowerer.literal_only.contains(&result.id))
    {
        return Ok(());
    }
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
        }
        Inst::TlsAddr(a) => {
            if !KERNEL.admits(target, ProfileFeature::Tls) {
                return Err("TLS requires AArch64 ELF".into());
            }
            let model = signatures
                .tls
                .iter()
                .find(|s| s.symbol == a.symbol)
                .ok_or("missing TLS symbol")?
                .model;
            let name = &a.symbol;
            let (template, clobbers) = match model {
                evocc_evoir::TlsModel::LocalExec => (format!("mrs %0, tpidr_el0\nadd %0, %0, #:tprel_hi12:{name}\nadd %0, %0, #:tprel_lo12_nc:{name}"), vec![]),
                evocc_evoir::TlsModel::InitialExec => (format!("adrp x0, :gottprel:{name}\nldr x0, [x0, #:gottprel_lo12:{name}]\nmrs %0, tpidr_el0\nadd %0, %0, x0"), vec![AsmClobber::Register(0)]),
                evocc_evoir::TlsModel::GeneralDynamic | evocc_evoir::TlsModel::LocalDynamic => (format!("adrp x0, :tlsdesc:{name}\nldr x1, [x0, #:tlsdesc_lo12:{name}]\nadd x0, x0, #:tlsdesc_lo12:{name}\n.tlsdesccall {name}\nblr x1\nmrs %0, tpidr_el0\nadd %0, %0, x0"), vec![AsmClobber::Register(0),AsmClobber::Register(1),AsmClobber::Register(30),AsmClobber::Flags,AsmClobber::Memory]),
            };
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::Asm(AsmBody {
                template,
                volatile: true,
                outputs: vec![AsmOperand::reg(
                    AsmConstraint::EarlyClobber(crate::AsmDirection::Write),
                    dst,
                )],
                inputs: vec![],
                clobbers,
            }));
            lowerer.bind(&a.result, dst);
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
            let value = lowerer.reg(lhs)?;
            lowerer.shift(insts, BinOp::Lsl, result, value, rhs)?;
        }
        Inst::LShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.zero_extend(insts, value, bits, width);
            lowerer.shift(insts, BinOp::Lsr, result, value, rhs)?;
        }
        Inst::AShr(result, lhs, rhs) => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let width = width_of(class);
            let bits = significant_bits(&lhs.ty);
            let value = lowerer.reg(lhs)?;
            let value = lowerer.sign_extend(insts, value, bits, width);
            lowerer.shift(insts, BinOp::Asr, result, value, rhs)?;
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
            lowerer.null_pointers.insert(result.id);
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::MovImm { dst, imm: 0 });
            lowerer.bind(result, dst);
        }
        // Section 5's `va_start` and `va_end` rows. `va_arg` needs branches,
        // so it splits the block and is lowered where the blocks are built.
        Inst::VaAccess(access) => match &access.op {
            VariadicOp::Start => lower_va_start(lowerer, insts, &access.list)?,
            VariadicOp::End => {}
            VariadicOp::Arg(_) | VariadicOp::ArgRecord { .. } => {
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
        Inst::Vector(vector) => lower_vector(lowerer, insts, vector)?,
        Inst::Floating(operation) => lower_floating(lowerer, insts, operation, false)?,
        Inst::StrictFloating(operation) => {
            lower_floating(lowerer, insts, &operation.instruction, true)?
        }
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
        Inst::FrameAddress { result, depth } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            insts.push(MInst::FrameAddress {
                dst,
                place: std::num::NonZeroU16::new(*depth)
                    .map_or(FramePlace::FramePointer, FramePlace::AncestorFrame),
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
            let delta = lowerer.pointer_delta(insts, index, *signedness, *scale)?;
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
            if let Some(offset) =
                lowerer.immediate_pointer_delta(index, *direction, *signedness, *scale)
            {
                lowerer
                    .address_preparation
                    .memory
                    .insert(result.id, (base, offset, insts.len()));
            }
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
        Inst::DynamicAlloca {
            result,
            origin,
            size,
            align,
            ..
        } => {
            let dst = lowerer.fresh(RegClass::Gpr64);
            let size = lowerer.reg(size)?;
            insts.push(MInst::StackAlloc {
                dst,
                size,
                origin: *origin,
                align: *align,
            });
            lowerer.bind(result, dst);
        }
        // A dynamic object can release the native stack allocation. Fixed
        // frame slots keep their reserved frame positions; access guards
        // themselves still have no machine effect.
        Inst::LifetimeEnd { pointer, .. } => {
            if lowerer.dynamic_addresses.contains(&pointer.id) {
                insts.push(MInst::StackRelease {
                    address: lowerer.reg(pointer)?,
                });
            }
        }
        Inst::PointerValid { .. } => {}
        Inst::AccessInBounds {
            mem_out, layout, ..
        } => {
            lowerer
                .access_alignments
                .insert(mem_out.id, layout.align_bytes);
        }
        Inst::Atomic(access) => {
            if let evocc_evoir::AtomicOp::Rmw {
                result,
                operation,
                value,
                expected,
            } = &access.op
            {
                let dst = lowerer.fresh(class_of(&result.ty).map_err(|(code, _)| code.to_owned())?);
                insts.push(MInst::AtomicRmw {
                    dst,
                    base: lowerer.reg(&access.address)?,
                    value: lowerer.reg(value)?,
                    expected: expected
                        .as_ref()
                        .map(|value| lowerer.reg(value))
                        .transpose()?,
                    operation: *operation,
                    kind: load_kind(&result.ty)?,
                    order: access.order,
                    volatile: access.volatile,
                });
                lowerer.bind(result, dst);
                return Ok(());
            }
            let load = access.is_load();
            let kind = load_kind(&access.value().ty)?;
            let base = lowerer.reg(&access.address)?;
            let value = if load {
                lowerer.fresh(class_of(&access.value().ty).map_err(|(code, _)| code.to_owned())?)
            } else {
                lowerer.reg(access.value())?
            };
            insts.push(MInst::Atomic {
                load,
                value,
                base,
                kind,
                order: access.order,
                volatile: access.volatile,
            });
            if load {
                lowerer.bind(access.value(), value);
            }
        }
        Inst::Load {
            result,
            address,
            mem_in,
            volatile,
            ..
        } => {
            let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
            let kind = load_kind(&result.ty)?;
            let dst = lowerer.fresh(class);
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            let (base, offset) =
                lowerer.memory_address(insts, address, kind.size_bytes(), align)?;
            insts.push(MInst::Load {
                align: if u64::from(align) == kind.size_bytes() {
                    0
                } else {
                    align
                },
                dst,
                base,
                offset,
                kind,
                volatile: *volatile,
            });
            lowerer.bind(result, dst);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            volatile,
            ..
        } => {
            let kind = store_kind(&value.ty)?;
            let src = lowerer.reg(value)?;
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            let (base, offset) =
                lowerer.memory_address(insts, address, kind.size_bytes(), align)?;
            insts.push(MInst::Store {
                align: if u64::from(align) == kind.size_bytes() {
                    0
                } else {
                    align
                },
                src,
                base,
                offset,
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
            volatile,
            ..
        } => {
            let dst = lowerer.reg(destination)?;
            let src = lowerer.reg(source)?;
            insts.push(MInst::MemCopy {
                dst,
                src,
                size: layout.size_bytes,
                align: layout.align_bytes,
                volatile: *volatile,
            });
        }
        Inst::Call {
            results,
            callee,
            args,
            arg_marks,
            ..
        } => {
            let marks = call_argument_marks(signatures.marks(callee), arg_marks, args.len());
            let plan = record_plan(target, &marked_types(&arg_types(args), &marks), true);
            let (mut arguments, returned) = call_arguments(lowerer, insts, args, &marks, &plan)?;
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
                let ordinary = args.iter().filter(|value| is_register_type(&value.ty));
                for (value, placed) in ordinary.zip(&plan.items) {
                    slot += placed.fillers;
                    match placed.passing {
                        Passing::Chunks(ref chunks) => slot += chunks.len(),
                        Passing::Vectors {
                            shape,
                            skipped,
                            stack,
                        } => slot += simd_argument_pieces(shape, stack).len() + skipped,
                        Passing::Pointer => slot += 1,
                        Passing::Destination(_) => {}
                        Passing::Value => {
                            if is_pair(&value.ty) {
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
                }
            }
            let mut outputs = call_results(lowerer, results)?;
            let after = call_record_result(lowerer, &mut outputs, returned);
            if plan.indirect_result {
                insts.push(MInst::IndirectResult);
            }
            insts.push(MInst::Bl {
                symbol: callee.clone(),
                args: arguments,
                results: outputs,
                fp_mode: None,
            });
            insts.extend(after);
        }
        Inst::IndirectCall {
            results,
            callee,
            args,
            arg_marks,
            ..
        } => {
            // Revision 1.9.73 puts the marks inside the pointer's own type,
            // "part of the signature for equality", so an indirect call reads
            // them where it reads everything else about its callee.
            let marks: Vec<ParamMark> = match &callee.ty {
                EvoIRType::FnPtr { marks, .. } => marks.clone(),
                _ => Vec::new(),
            };
            let marks = call_argument_marks(&marks, arg_marks, args.len());
            let plan = record_plan(target, &marked_types(&arg_types(args), &marks), true);
            let pointer = lowerer.reg(callee)?;
            let (arguments, returned) = call_arguments(lowerer, insts, args, &marks, &plan)?;
            let mut outputs = call_results(lowerer, results)?;
            let after = call_record_result(lowerer, &mut outputs, returned);
            if plan.indirect_result {
                insts.push(MInst::IndirectResult);
            }
            insts.push(MInst::Blr {
                target: pointer,
                args: arguments,
                results: outputs,
            });
            insts.extend(after);
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
    if outputs.len() > lowerer.abi.general_results() {
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
        EvoIRType::F32 => LoadKind::S,
        EvoIRType::F64 => LoadKind::D,
        EvoIRType::F128 => LoadKind::Q,
        EvoIRType::Vec { .. } => match vector_bytes(ty) {
            Some(1) => LoadKind::B,
            Some(2) => LoadKind::H,
            Some(4) => LoadKind::W,
            Some(8) => LoadKind::D,
            Some(16) => LoadKind::Q,
            _ => return Err("unsupported vector load width".into()),
        },
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
        EvoIRType::F32 => StoreKind::S,
        EvoIRType::F64 => StoreKind::D,
        EvoIRType::F128 => StoreKind::Q,
        EvoIRType::Vec { .. } => match vector_bytes(ty) {
            Some(1) => StoreKind::B,
            Some(2) => StoreKind::H,
            Some(4) => StoreKind::W,
            Some(8) => StoreKind::D,
            Some(16) => StoreKind::Q,
            _ => return Err("unsupported vector store width".into()),
        },
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

/// Floating helpers remain ordinary calls, with visible ABI/clobbers. Raw
/// FP-to-integer operations are total; C lowering supplies the UB obligation.
fn lower_floating(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    operation: &evocc_evoir::FloatingInst,
    strict: bool,
) -> Result<(), String> {
    use evocc_evoir::FloatingOp::*;
    use evocc_types::FloatKind;
    let result = &operation.result;
    let op = operation.op;
    if operation
        .arguments
        .iter()
        .chain(std::iter::once(result))
        .any(|v| v.ty == EvoIRType::F128)
    {
        return quad::lower(lowerer, insts, operation, strict);
    }
    let kind = operation
        .arguments
        .first()
        .and_then(|value| evocc_evoir::floating_kind(&value.ty))
        .or_else(|| evocc_evoir::floating_kind(&result.ty))
        .ok_or("no floating format")?;
    if let Constant(bits) = op {
        let raw = lowerer.fresh(if kind == FloatKind::F32 {
            RegClass::Gpr32
        } else {
            RegClass::Gpr64
        });
        insts.push(MInst::MovImm {
            dst: raw,
            imm: bits as u64,
        });
        let dst = lowerer.fresh(crate::floating::class(kind));
        insts.push(MInst::Floating {
            strict: false,
            integer_bits: 0,
            dst,
            op: FromBits,
            kind,
            args: vec![raw],
        });
        lowerer.bind(result, dst);
        return Ok(());
    }
    let from_pair = matches!(op, FromSigned | FromUnsigned) && is_pair(&operation.arguments[0].ty);
    let to_pair = matches!(op, ToSigned | ToUnsigned) && is_pair(&result.ty);
    if from_pair || to_pair {
        insts.push(MInst::FpEnvCheck { strict });
        let mut args = call_args(lowerer, insts, &operation.arguments)?;
        if to_pair {
            // Do not call a C runtime helper outside its defined conversion
            // domain. Invalid raw IR converts a selected +0 instead.
            let input = args[0];
            let defined = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Floating {
                strict: false,
                integer_bits: 0,
                dst: defined,
                op: ToIntDefined {
                    width: 128,
                    signed: op == ToSigned,
                },
                kind,
                args: vec![input],
            });
            if strict {
                // Raw IR conversion is total: invalid input yields zero and
                // INVALID. The runtime helper only receives the selected zero;
                // a strict NaN->integer conversion raises the missing flag.
                let flags = lowerer.fresh(RegClass::Flags);
                insts.push(MInst::CmpImm {
                    dst: flags,
                    lhs: defined,
                    imm: 0,
                    width: Width::W,
                });
                let invalid = lowerer.fresh(RegClass::Gpr32);
                insts.push(MInst::Cset {
                    dst: invalid,
                    flags,
                    cond: Cond::Eq,
                });
                let nan = lowerer.fresh(if kind == FloatKind::F32 {
                    RegClass::Gpr32
                } else {
                    RegClass::Gpr64
                });
                insts.push(MInst::MovImm {
                    dst: nan,
                    imm: if kind == FloatKind::F32 {
                        0x7fc00000
                    } else {
                        0x7ff8000000000000
                    },
                });
                let bits = floating_select_zero(lowerer, insts, nan, invalid);
                let input = lowerer.fresh(crate::floating::class(kind));
                insts.push(MInst::Floating {
                    strict: false,
                    integer_bits: 0,
                    dst: input,
                    op: FromBits,
                    kind,
                    args: vec![bits],
                });
                let ignored = lowerer.fresh(RegClass::Gpr64);
                insts.push(MInst::Floating {
                    strict: true,
                    integer_bits: 64,
                    dst: ignored,
                    op: ToSigned,
                    kind,
                    args: vec![input],
                });
            }
            let raw = lowerer.fresh(if kind == FloatKind::F32 {
                RegClass::Gpr32
            } else {
                RegClass::Gpr64
            });
            insts.push(MInst::Floating {
                strict: false,
                integer_bits: 0,
                dst: raw,
                op: ToBits,
                kind,
                args: vec![input],
            });
            let selected = floating_select_zero(lowerer, insts, raw, defined);
            let input = lowerer.fresh(crate::floating::class(kind));
            insts.push(MInst::Floating {
                strict: false,
                integer_bits: 0,
                dst: input,
                op: FromBits,
                kind,
                args: vec![selected],
            });
            args = vec![input];
        }
        let suffix = if kind == FloatKind::F32 { "sf" } else { "df" };
        let symbol = match op {
            FromSigned => format!("__floatti{suffix}"),
            FromUnsigned => format!("__floatunti{suffix}"),
            ToSigned => format!("__fix{suffix}ti"),
            ToUnsigned => format!("__fixuns{suffix}ti"),
            _ => unreachable!(),
        };
        let results = call_results(lowerer, std::slice::from_ref(result))?;
        insts.push(MInst::Bl {
            symbol,
            args,
            results,
            fp_mode: Some(if strict {
                crate::FpCallMode::Strict
            } else {
                crate::FpCallMode::Fixed
            }),
        });
        return Ok(());
    }
    let mut args = operation
        .arguments
        .iter()
        .map(|value| lowerer.reg(value))
        .collect::<Result<Vec<_>, _>>()?;
    if matches!(op, FromSigned | FromUnsigned) {
        let width = width_of(args[0].class);
        let bits = significant_bits(&operation.arguments[0].ty);
        args[0] = if op == FromSigned {
            lowerer.sign_extend(insts, args[0], bits, width)
        } else {
            lowerer.zero_extend(insts, args[0], bits, width)
        };
    }
    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
    let mut dst = lowerer.fresh(class);
    insts.push(MInst::Floating {
        strict,
        integer_bits: if strict && matches!(op, ToSigned | ToUnsigned) {
            significant_bits(&result.ty)
        } else {
            0
        },
        dst,
        op,
        kind,
        args: args.clone(),
    });
    if matches!(op, ToSigned | ToUnsigned) {
        let bits = significant_bits(&result.ty);
        if bits < class.bit_width().unwrap() && !strict {
            let defined = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Floating {
                strict: false,
                integer_bits: 0,
                dst: defined,
                op: ToIntDefined {
                    width: bits,
                    signed: op == ToSigned,
                },
                kind,
                args,
            });
            dst = floating_select_zero(lowerer, insts, dst, defined);
            dst = lowerer.zero_extend(insts, dst, bits, width_of(class));
        }
        if strict && bits < class.bit_width().unwrap() {
            dst = lowerer.zero_extend(insts, dst, bits, width_of(class));
        }
    }
    lowerer.bind(result, dst);
    Ok(())
}

fn floating_select_zero(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: VReg,
    predicate: VReg,
) -> VReg {
    let zero = lowerer.fresh(value.class);
    insts.push(MInst::MovImm { dst: zero, imm: 0 });
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::CmpImm {
        dst: flags,
        lhs: predicate,
        imm: 0,
        width: Width::W,
    });
    let dst = lowerer.fresh(value.class);
    insts.push(MInst::Csel {
        dst,
        flags,
        lhs: value,
        rhs: zero,
        cond: Cond::Ne,
        width: width_of(value.class),
    });
    dst
}

fn lower_vector(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    vector: &evocc_evoir::VectorInst,
) -> Result<(), String> {
    use evocc_evoir::VectorOp;
    vector.validate()?;
    let result = &vector.result;
    let args = &vector.arguments;
    let bytes = vector_bytes(&result.ty)
        .or_else(|| vector_bytes(&args[0].ty))
        .ok_or("invalid vector representation size")? as u8;
    match vector.op {
        VectorOp::Aes(op) => {
            let dst = lowerer.fresh(RegClass::Simd128);
            insts.push(MInst::Aes {
                dst,
                op,
                args: args
                    .iter()
                    .map(|v| lowerer.reg(v))
                    .collect::<Result<_, _>>()?,
            });
            lowerer.bind(result, dst);
        }
        VectorOp::Pack => {
            let (_, lane_bits) =
                evocc_evoir::integer_vector_shape(&result.ty).ok_or("invalid vector shape")?;
            let dst = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
            let (lane_bits, arguments) = if lane_bits == 128 {
                (
                    64,
                    args.iter()
                        .map(|v| lowerer.registers(v))
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                )
            } else {
                (
                    lane_bits,
                    args.iter()
                        .map(|v| lowerer.reg(v))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            insts.push(MInst::Vector {
                dst,
                extract: false,
                lane_bits,
                bytes,
                args: arguments,
            });
            lowerer.bind(result, dst);
        }
        VectorOp::Extract => {
            let (_, lane_bits) =
                evocc_evoir::integer_vector_shape(&args[0].ty).ok_or("invalid vector shape")?;
            let source = lowerer.reg(&args[0])?;
            let index = vector_index(
                lowerer,
                insts,
                &args[1],
                u64::from(bytes) * 8 / u64::from(lane_bits),
            )?;
            if lane_bits == 128 {
                // There is one 128-bit lane in a short vector. Keep the
                // original index check by extracting its first 64-bit half.
                let twice = lowerer.word_binary_imm(insts, BinOp::Lsl, index, 1);
                let second = lowerer.word_binary_imm(insts, BinOp::Add, twice, 1);
                let (low, high) = lowerer.fresh_pair();
                insts.push(MInst::Vector {
                    dst: low,
                    extract: true,
                    lane_bits: 64,
                    bytes,
                    args: vec![source, twice],
                });
                insts.push(MInst::Vector {
                    dst: high,
                    extract: true,
                    lane_bits: 64,
                    bytes,
                    args: vec![source, second],
                });
                lowerer.bind_pair(result, low, high);
            } else {
                let dst = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
                insts.push(MInst::Vector {
                    dst,
                    extract: true,
                    lane_bits,
                    bytes,
                    args: vec![source, index],
                });
                lowerer.bind(result, dst);
            }
        }
        VectorOp::Bitcast => {
            if vector_bytes(&result.ty).is_some() && vector_bytes(&args[0].ty).is_some() {
                lowerer.bind(result, lowerer.reg(&args[0])?);
            } else if vector_bytes(&result.ty).is_some() {
                let dst = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
                insts.push(MInst::Vector {
                    dst,
                    extract: false,
                    lane_bits: u16::from(bytes).min(8) * 8,
                    bytes,
                    args: lowerer.registers(&args[0])?,
                });
                lowerer.bind(result, dst);
            } else {
                let source = lowerer.reg(&args[0])?;
                let low = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
                let zero = lowerer.word(insts, 0);
                insts.push(MInst::Vector {
                    dst: low,
                    extract: true,
                    lane_bits: u16::from(bytes).min(8) * 8,
                    bytes,
                    args: vec![source, zero],
                });
                if is_pair(&result.ty) {
                    let high = lowerer.fresh(RegClass::Gpr64);
                    let one = lowerer.word(insts, 1);
                    insts.push(MInst::Vector {
                        dst: high,
                        extract: true,
                        lane_bits: 64,
                        bytes,
                        args: vec![source, one],
                    });
                    lowerer.bind_pair(result, low, high);
                } else {
                    lowerer.bind(result, low);
                }
            }
        }
    }
    Ok(())
}

/// Preserve an invalid full-width index as an out-of-range sentinel before
/// selecting machine-width lanes. In particular, do not truncate bits<128>
/// or multiply a large index before checking it.
fn vector_index(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    value: &Value,
    lanes: u64,
) -> Result<VReg, String> {
    let parts = lowerer.registers(value)?;
    let low = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::Mov {
        dst: low,
        src: parts[0],
    });
    let limit = lowerer.word(insts, lanes);
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::Cmp {
        dst: flags,
        lhs: low,
        rhs: limit,
        width: Width::X,
    });
    let bounded = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::Csel {
        dst: bounded,
        flags,
        lhs: low,
        rhs: limit,
        cond: Cond::Lo,
        width: Width::X,
    });
    if let Some(high) = parts.get(1) {
        let zero = lowerer.word(insts, 0);
        let flags = lowerer.fresh(RegClass::Flags);
        insts.push(MInst::Cmp {
            dst: flags,
            lhs: *high,
            rhs: zero,
            width: Width::X,
        });
        let result = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::Csel {
            dst: result,
            flags,
            lhs: bounded,
            rhs: limit,
            cond: Cond::Eq,
            width: Width::X,
        });
        Ok(result)
    } else {
        Ok(bounded)
    }
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
    let width = width_of(class);
    // Select an existing unsigned 12-bit immediate, never fold away the
    // source's checked-arithmetic predicate. Commuting ADD only exchanges
    // already-evaluated SSA operands; SUB must keep its minuend/subtrahend.
    // Narrow and pair values keep their established representation paths.
    let small = |value: &Value| {
        lowerer
            .constants
            .get(&value.id)
            .map(|raw| raw & width.mask())
            .filter(|raw| *raw <= 4095)
    };
    let logical = |value: &Value| {
        lowerer
            .constants
            .get(&value.id)
            .map(|raw| raw & width.mask())
            .filter(|raw| crate::immediate::logical_mask(*raw, width))
    };
    let immediate = if matches!(result.ty, EvoIRType::Bits(32 | 64)) {
        match op {
            BinOp::Add => small(rhs)
                .map(|imm| (left, imm))
                .or_else(|| small(lhs).map(|imm| (right, imm))),
            BinOp::Sub => small(rhs).map(|imm| (left, imm)),
            BinOp::And | BinOp::Orr | BinOp::Eor => logical(rhs)
                .map(|imm| (left, imm))
                .or_else(|| logical(lhs).map(|imm| (right, imm))),
            _ => None,
        }
    } else {
        None
    };
    if let Some((lhs, imm)) = immediate {
        insts.push(MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width,
        });
    } else {
        insts.push(MInst::Binary {
            op,
            dst,
            lhs: left,
            rhs: right,
            width,
        });
    }
    // Retain exact facts for total, full-width bitwise constant expressions
    // (including the XOR-with-all-ones spelling of C's complement). The
    // computation still exists until normal dead-instruction removal; effects
    // and source obligations are neither folded nor moved by this table.
    if matches!(result.ty, EvoIRType::Bits(32 | 64)) {
        if let (Some(a), Some(b)) = (
            lowerer.constants.get(&lhs.id),
            lowerer.constants.get(&rhs.id),
        ) {
            let known = match op {
                BinOp::And => Some(a & b),
                BinOp::Orr => Some(a | b),
                BinOp::Eor => Some(a ^ b),
                _ => None,
            };
            if let Some(raw) = known {
                lowerer.constants.insert(result.id, raw & width.mask());
            }
        }
    }
    lowerer.bind(result, dst);
    Ok(())
}

/// Scalar division uses `sdiv`/`udiv` and `msub`; the hosted 128-bit form
/// uses ordinary calls so allocation sees argument/result pairs and clobbers.
fn divide(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    is_signed: bool,
    remainder: bool,
    result: &Value,
    lhs: &Value,
    rhs: &Value,
) -> Result<(), String> {
    if is_pair(&result.ty) {
        let symbol = match (is_signed, remainder) {
            (true, false) => "__divti3",
            (false, false) => "__udivti3",
            (true, true) => "__modti3",
            (false, true) => "__umodti3",
        };
        let args = call_args(lowerer, insts, &[lhs.clone(), rhs.clone()])?;
        let results = call_results(lowerer, std::slice::from_ref(result))?;
        insts.push(MInst::Bl {
            symbol: symbol.to_owned(),
            args,
            results,
            fp_mode: None,
        });
        return Ok(());
    }
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
        if AsmConstraint::parse(evocc_evoir::assembly_register_binding(&output.constraint)?.0)
            .is_none()
        {
            return Err(format!(
                "an assembly output constraint '{}'",
                output.constraint
            ));
        }
    }
    for input in &assembly.inputs {
        if AsmConstraint::parse(evocc_evoir::assembly_register_binding(&input.constraint)?.0)
            .is_none()
        {
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
        let (base, fixed_register) = evocc_evoir::assembly_register_binding(&output.constraint)?;
        let constraint = AsmConstraint::parse(base)
            .ok_or_else(|| format!("an assembly output constraint '{}'", output.constraint))?;
        if let AsmConstraint::Memory { .. } = constraint {
            let place = output
                .incoming
                .as_ref()
                .ok_or_else(|| "a memory output with no place".to_owned())?;
            outputs.push(AsmOperand {
                constraint,
                fixed_register,
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
            fixed_register,
            name: named(&output.name),
            value: AsmValue::Reg(register),
        });
    }

    let mut inputs: Vec<AsmOperand> = Vec::new();
    for input in &assembly.inputs {
        let (base, fixed_register) = evocc_evoir::assembly_register_binding(&input.constraint)?;
        let constraint = AsmConstraint::parse(base)
            .ok_or_else(|| format!("an assembly input constraint '{}'", input.constraint))?;
        let value = match constraint {
            AsmConstraint::Immediate(_) => {
                if base == "i" {
                    if let Some((name, addend)) = lowerer.symbolic_addresses.get(&input.value.id) {
                        inputs.push(AsmOperand {
                            constraint,
                            fixed_register,
                            name: named(&input.name),
                            value: AsmValue::Symbol {
                                name: name.clone(),
                                addend: *addend,
                            },
                        });
                        continue;
                    }
                }
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
            fixed_register,
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

/// The same placement determines both saved registers and the first
/// anonymous stack slot, including pair/record alignment padding.
fn named_argument_extent(target: Target, function: &Function) -> (u8, u8, u64) {
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
    );
    let marked = marked_types(&declared, &function.param_marks);
    let abi = target.abi();
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
        cursor.used(general_bank()),
        cursor.used(vector_bank()),
        cursor.stack_bytes(abi),
    )
}

/// The size rounded up to eight (`S8`) and the alignment (`A`) of one
/// `va_arg` type, or nothing when the type has no Section 6 procedure.
fn variadic_access_slot(op: &VariadicOp) -> Option<(u64, u64)> {
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

fn variadic_slot(ty: &EvoIRType) -> Option<(u64, u64)> {
    if let Some(bytes) = vector_bytes(ty) {
        return Some((bytes.max(8), bytes));
    }
    match ty {
        EvoIRType::Ptr(_) | EvoIRType::FnPtr { .. } | EvoIRType::F32 | EvoIRType::F64 => {
            Some((8, 8))
        }
        EvoIRType::Bits(128) | EvoIRType::F128 => Some((16, 16)),
        EvoIRType::Bits(width) if *width <= 64 => Some((8, 8)),
        _ => None,
    }
}

/// `va_start`: skips named incoming stack slots, then stores
/// `gr_top = save area + 64`, `vr_top = gr_top + 128`, `gr_offs =
/// -8 * (8 - k)` and `vr_offs = -128` (Section 6, revision 0.28). The field
/// offsets, register counts and save widths are the ABI table's.
fn lower_va_start(
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
    });

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
        Inst::SignedAddDefined(result, lhs, rhs) | Inst::SignedSubDefined(result, lhs, rhs)
            if is_pair(&lhs.ty) =>
        {
            let (a_low, a_high) = lowerer.pair(lhs)?;
            let (b_low, b_high) = lowerer.pair(rhs)?;
            let subtract = matches!(inst, Inst::SignedSubDefined(..));
            let scratch = lowerer.fresh(RegClass::Gpr64);
            let carry = lowerer.fresh(RegClass::Flags);
            insts.push(MInst::FlagBinary {
                op: if subtract { FlagOp::Subs } else { FlagOp::Adds },
                dst: scratch,
                flags: carry,
                lhs: a_low,
                rhs: b_low,
                width: Width::X,
            });
            let flags = lowerer.fresh(RegClass::Flags);
            insts.push(if subtract {
                MInst::SubCarryFlags {
                    dst: flags,
                    flags_in: carry,
                    lhs: a_high,
                    rhs: b_high,
                    width: Width::X,
                }
            } else {
                MInst::AddCarryFlags {
                    dst: flags,
                    flags_in: carry,
                    lhs: a_high,
                    rhs: b_high,
                    width: Width::X,
                }
            });
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::Cset {
                dst,
                flags,
                cond: Cond::Vc,
            });
            lowerer.bind(result, dst);
        }
        Inst::SignedMulDefined(result, lhs, rhs) if is_pair(&lhs.ty) => {
            let left = lowerer.pair(lhs)?;
            let right = lowerer.pair(rhs)?;
            let dst = pair_product_defined(lowerer, insts, left, right);
            lowerer.bind(result, dst);
        }
        Inst::DivisorNonZero(result, divisor) if is_pair(&divisor.ty) => {
            let (low, high) = lowerer.pair(divisor)?;
            let bits = lowerer.word_binary(insts, BinOp::Orr, low, high);
            let zero = lowerer.word(insts, 0);
            let dst = lowerer.compare_into(insts, bits, zero, Width::X, Cond::Ne);
            lowerer.bind(result, dst);
        }
        Inst::SignedDivRemNoOverflow(result, lhs, rhs) if is_pair(&lhs.ty) => {
            let (a_low, a_high) = lowerer.pair(lhs)?;
            let (b_low, b_high) = lowerer.pair(rhs)?;
            // OR the four differences from (MIN, -1), then make one boolean.
            let high_difference = lowerer.word_binary_imm(insts, BinOp::Eor, a_high, 1 << 63);
            let left = lowerer.word_binary(insts, BinOp::Orr, a_low, high_difference);
            // All-ones is not an AArch64 logical immediate.
            let ones = lowerer.word(insts, u64::MAX);
            let low_difference = lowerer.word_binary(insts, BinOp::Eor, b_low, ones);
            let high_difference = lowerer.word_binary(insts, BinOp::Eor, b_high, ones);
            let right = lowerer.word_binary(insts, BinOp::Orr, low_difference, high_difference);
            let difference = lowerer.word_binary(insts, BinOp::Orr, left, right);
            let zero = lowerer.word(insts, 0);
            let dst = lowerer.compare_into(insts, difference, zero, Width::X, Cond::Ne);
            lowerer.bind(result, dst);
        }
        Inst::SignedShlDefined {
            result,
            shifted,
            amount,
            amount_signed,
        } if is_pair(&shifted.ty) => {
            let count_ok = lowerer.count_defined(insts, shifted, amount, *amount_signed)?;
            let value = lowerer.pair(shifted)?;
            let zero = lowerer.word(insts, 0);
            let nonnegative = lowerer.compare_into(insts, value.1, zero, Width::X, Cond::Ge);
            let count = lowerer.shift_amount(insts, amount)?;
            let left = pair_shift(lowerer, insts, value, count, PairShift::Left);
            let back = pair_shift(lowerer, insts, left, count, PairShift::Arithmetic);
            let low = lowerer.word_binary(insts, BinOp::Eor, back.0, value.0);
            let high = lowerer.word_binary(insts, BinOp::Eor, back.1, value.1);
            let difference = lowerer.word_binary(insts, BinOp::Orr, low, high);
            let same = lowerer.compare_into(insts, difference, zero, Width::X, Cond::Eq);
            let dst = lowerer.both(insts, count_ok, nonnegative);
            let dst = lowerer.both(insts, dst, same);
            lowerer.bind(result, dst);
        }
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
        // "`load`/`store`: `ldp`/`stp` of the halves at the address, which is
        // not single-copy atomic, exactly as both oracles emit for a
        // `volatile u128`."
        Inst::Load {
            result,
            address,
            mem_in,
            volatile,
            ..
        } if is_pair(&result.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.fresh_pair();
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            if align < 8 {
                for (dst, offset) in [(low, 0), (high, 8)] {
                    insts.push(MInst::Load {
                        align,
                        dst,
                        base,
                        offset,
                        kind: LoadKind::X,
                        volatile: *volatile,
                    });
                }
            } else {
                insts.push(MInst::Ldp {
                    rt1: low,
                    rt2: high,
                    base,
                    offset: 0,
                    writeback: false,
                    volatile: *volatile,
                });
            }
            lowerer.bind_pair(result, low, high);
        }
        Inst::Store {
            address,
            value,
            mem_in,
            volatile,
            ..
        } if is_pair(&value.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.pair(value)?;
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            if align < 8 {
                for (src, offset) in [(low, 0), (high, 8)] {
                    insts.push(MInst::Store {
                        align,
                        src,
                        base,
                        offset,
                        kind: StoreKind::X,
                        volatile: *volatile,
                    });
                }
            } else {
                insts.push(MInst::Stp {
                    rt1: low,
                    rt2: high,
                    base,
                    offset: 0,
                    writeback: false,
                    volatile: *volatile,
                });
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Magnitude without signed overflow, including the signed minimum.
fn pair_magnitude(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    (low, high): (VReg, VReg),
) -> ((VReg, VReg), VReg) {
    let sign = lowerer.word_binary_imm(insts, BinOp::Asr, high, 63);
    let low = lowerer.word_binary(insts, BinOp::Eor, low, sign);
    let high = lowerer.word_binary(insts, BinOp::Eor, high, sign);
    let magnitude_low = lowerer.fresh(RegClass::Gpr64);
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::FlagBinary {
        op: FlagOp::Subs,
        dst: magnitude_low,
        flags,
        lhs: low,
        rhs: sign,
        width: Width::X,
    });
    let magnitude_high = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::CarryBinary {
        op: CarryOp::Sbc,
        dst: magnitude_high,
        flags,
        lhs: high,
        rhs: sign,
        width: Width::X,
    });
    ((magnitude_low, magnitude_high), sign)
}

/// A limb addition and its carry materialized before any later flags producer.
fn limb_add(lowerer: &mut Lowerer, insts: &mut Vec<MInst>, lhs: VReg, rhs: VReg) -> (VReg, VReg) {
    let dst = lowerer.fresh(RegClass::Gpr64);
    let flags = lowerer.fresh(RegClass::Flags);
    insts.push(MInst::FlagBinary {
        op: FlagOp::Adds,
        dst,
        flags,
        lhs,
        rhs,
        width: Width::X,
    });
    let carry = lowerer.fresh(RegClass::Gpr64);
    insts.push(MInst::Cset {
        dst: carry,
        flags,
        cond: Cond::Hs,
    });
    (dst, carry)
}

/// The exact unsigned 128-by-128 product, accumulated into four 64-bit limbs.
fn pair_product_defined(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    left: (VReg, VReg),
    right: (VReg, VReg),
) -> VReg {
    let ((a_low, a_high), a_sign) = pair_magnitude(lowerer, insts, left);
    let ((b_low, b_high), b_sign) = pair_magnitude(lowerer, insts, right);
    let mut products = Vec::with_capacity(4);
    for (lhs, rhs) in [
        (a_low, b_low),
        (a_low, b_high),
        (a_high, b_low),
        (a_high, b_high),
    ] {
        let low = lowerer.word_binary(insts, BinOp::Mul, lhs, rhs);
        let high = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::WideMultiply {
            op: WideMul::Umulh,
            dst: high,
            lhs,
            rhs,
        });
        products.push((low, high));
    }
    let low = products[0].0;
    let (middle, carry_a) = limb_add(lowerer, insts, products[0].1, products[1].0);
    let (middle, carry_b) = limb_add(lowerer, insts, middle, products[2].0);
    let carry = lowerer.word_binary(insts, BinOp::Add, carry_a, carry_b);
    let (upper, carry_a) = limb_add(lowerer, insts, products[1].1, products[2].1);
    let (upper, carry_b) = limb_add(lowerer, insts, upper, products[3].0);
    let (upper, carry_c) = limb_add(lowerer, insts, upper, carry);
    let top = lowerer.word_binary(insts, BinOp::Add, products[3].1, carry_a);
    let top = lowerer.word_binary(insts, BinOp::Add, top, carry_b);
    let top = lowerer.word_binary(insts, BinOp::Add, top, carry_c);
    let upper = lowerer.word_binary(insts, BinOp::Orr, upper, top);
    let zero = lowerer.word(insts, 0);
    let upper_zero = lowerer.compare_into(insts, upper, zero, Width::X, Cond::Eq);

    // Below 2^127 fits either sign. Exactly 2^127 fits only a negative result.
    let boundary = lowerer.word(insts, 1 << 63);
    let below = lowerer.compare_into(insts, middle, boundary, Width::X, Cond::Lo);
    let at_boundary = lowerer.compare_into(insts, middle, boundary, Width::X, Cond::Eq);
    let low_zero = lowerer.compare_into(insts, low, zero, Width::X, Cond::Eq);
    let different_signs = lowerer.compare_into(insts, a_sign, b_sign, Width::X, Cond::Ne);
    let at_boundary = lowerer.both(insts, at_boundary, low_zero);
    let at_boundary = lowerer.both(insts, at_boundary, different_signs);
    let within = lowerer.fresh(RegClass::Gpr32);
    insts.push(MInst::Binary {
        op: BinOp::Orr,
        dst: within,
        lhs: below,
        rhs: at_boundary,
        width: Width::W,
    });
    lowerer.both(insts, within, upper_zero)
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
    let pair = lowerer.pair(value)?;
    let count = lowerer.shift_amount(insts, amount)?;
    Ok(pair_shift(lowerer, insts, pair, count, kind))
}

fn pair_shift(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    (low, high): (VReg, VReg),
    count: VReg,
    kind: PairShift,
) -> (VReg, VReg) {
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
        return (result_low, result_high);
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
    (result_low, result_high)
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

// ---------------------------------------------------------------------------
// Section 6: records (revision 0.18)
// ---------------------------------------------------------------------------

/// The origins the machine gives the frame slots the record convention needs.
///
/// EvoIR's `alloca` origins come from the unit; these are the machine's own and
/// are taken from the top of the ordinal space so that the two never meet.
const RECORD_ORIGIN_BASE: u64 = 1 << 62;

/// The eight-byte chunks a record of `size` bytes travels in.
fn record_chunks(size: u64) -> Vec<u64> {
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
/// pairs of descending power-of-two widths.
///
/// A record's size is not a multiple of eight in general (`struct { int a, b,
/// c; }` is twelve), and the oracles read exactly the bytes the record has:
/// `pass4` moves a word, `pass12` a doubleword and a word
/// (`.evidence/builtin-probes/20260906-b10/b10-callsites-linux-gcc14.s`). Every
/// other size follows from the same rule, so a chunk of three bytes is a
/// halfword and a byte and nothing is read past the object.
fn chunk_pieces(size: u64) -> Vec<(u64, u64)> {
    let mut pieces = Vec::new();
    let mut offset = 0;
    let mut left = size;
    for width in [8_u64, 4, 2, 1] {
        while left >= width {
            pieces.push((offset, width));
            offset += width;
            left -= width;
        }
    }
    pieces
}

fn piece_load(width: u64) -> LoadKind {
    match width {
        1 => LoadKind::B,
        2 => LoadKind::H,
        4 => LoadKind::W,
        _ => LoadKind::X,
    }
}

fn piece_store(width: u64) -> StoreKind {
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
/// the width its member has.
fn load_chunk(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    base: VReg,
    offset: u64,
    size: u64,
) -> VReg {
    let mut accumulated: Option<VReg> = None;
    for (piece, width) in chunk_pieces(size) {
        let dst = lowerer.fresh(RegClass::Gpr64);
        insts.push(MInst::Load {
            align: 0,
            dst,
            base,
            offset: i64::try_from(offset + piece).unwrap_or(0),
            kind: piece_load(width),
            volatile: false,
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

/// Store one chunk of `size` bytes held in `src` at `offset` from `base`.
fn store_chunk(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    src: VReg,
    base: VReg,
    offset: u64,
    size: u64,
) {
    for (piece, width) in chunk_pieces(size) {
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
        });
    }
}

/// What the convention does with one argument.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Passing {
    /// An ordinary value: one register, or two for a `bits<128>`.
    Value,
    /// The eight-byte chunks a `byval` record of at most sixteen bytes travels
    /// in, each entry that chunk's own byte count.
    Chunks(Vec<u64>),
    Vectors {
        shape: RecordShape,
        skipped: usize,
        stack: bool,
    },
    /// A `byval` record above sixteen bytes: a pointer to the copy.
    Pointer,
    /// The destination of a record result. It takes no argument position: at
    /// most sixteen bytes the result comes back in `x0`/`x1`, and above that
    /// the pointer travels in `x8`.
    Destination(RecordShape),
}

/// One argument's placement.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Placed {
    /// Positions skipped before this argument: Section 6's even-register
    /// rounding, and the rule that sends a whole record to the stack rather
    /// than splitting it across the last register and the stack.
    fillers: usize,
    passing: Passing,
}

/// Section 6 applied to one argument list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordPlan {
    items: Vec<Placed>,
    /// Positions consumed, fillers included. A position at or above eight is a
    /// stack slot, which Darwin refuses (`E4010`).
    positions: usize,
    /// Whether the list ends with a destination pointer that goes to `x8`.
    indirect_result: bool,
}

/// Where each ordinary argument of a signature goes.
///
/// `pair_rounding` is the `bits<128>` rule as the two call sites already apply
/// it: the entry block rounds only there, because "inside the function the rule
/// has no meaning", and a call site always does.
fn record_plan(
    target: Target,
    arguments: &[(EvoIRType, ParamMark)],
    pair_rounding: bool,
) -> RecordPlan {
    let abi = target.abi();
    let general = abi.general_arguments();
    let vectors = abi.vector_arguments();
    let mut items = Vec::new();
    let mut next = 0_usize;
    let mut next_vector = 0_usize;
    let mut cursor = abi.argument_cursor();
    let mut indirect_result = false;
    for (ty, mark) in arguments {
        match mark {
            ParamMark::Sret(shape) => {
                if shape.size > abi.record_register_bytes && shape.simd_members().is_none() {
                    indirect_result = true;
                }
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Destination(*shape),
                });
            }
            ParamMark::ByVal(shape) if shape.simd_members().is_some() => {
                let count = usize::from(shape.simd_members().unwrap().0);
                let stack = next_vector + count > vectors;
                let mut skipped = if next_vector < vectors && next_vector + count > vectors {
                    vectors - next_vector
                } else {
                    0
                };
                // Once SIMD registers are exhausted, one additional private
                // filler aligns the shared stack cursor for an HFA whose
                // members require 16-byte alignment. Object over-alignment
                // alone must not request this padding.
                if stack
                    && shape.floating.is_some_and(|f| f.abi_align == 16)
                    && !cursor.stack_bytes(abi).is_multiple_of(16)
                {
                    skipped += 1;
                }
                next_vector += skipped + count;
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Vectors {
                        shape: *shape,
                        skipped,
                        stack,
                    },
                });
            }
            ParamMark::ByVal(shape) if shape.size <= abi.record_register_bytes => {
                let chunks = record_chunks(shape.size);
                let mut fillers = 0;
                // Section 6: "a record whose alignment is 16 starts at an even
                // register on Linux, as a `bits<128>` does"; Apple does not
                // round (measured: `after_one_16a16` takes `x1`:`x2` under
                // clang and `x2`:`x3` under GCC-14,
                // `.evidence/builtin-probes/20260906-b10/`).
                if shape.size != 0
                    && abi.aligned_record_pair_rounding
                    && shape.align == 16
                    && cursor.pair_padding(general_bank()) != 0
                {
                    fillers += 1;
                }
                // AAPCS64 never splits a record between the last register and
                // the stack: when the chunks do not all fit, the whole record
                // goes to the stack and the remaining register is unused
                // (measured: `after_seven_16` reads `ldp ..., [sp]` and leaves
                // `x7` alone under both oracles).
                if next + fillers + chunks.len() > general && next + fillers < general {
                    fillers = general - next;
                    if abi.aligned_record_pair_rounding
                        && shape.align == 16
                        && (next + fillers) % 2 == 1
                    {
                        fillers += 1;
                    }
                }
                next += fillers + chunks.len();
                items.push(Placed {
                    fillers,
                    passing: Passing::Chunks(chunks),
                });
            }
            ParamMark::ByVal(_) => {
                items.push(Placed {
                    fillers: 0,
                    passing: Passing::Pointer,
                });
                next += 1;
            }
            ParamMark::None => {
                if simd_argument_bytes(ty).is_some() {
                    next_vector += 1;
                    items.push(Placed {
                        fillers: 0,
                        passing: Passing::Value,
                    });
                } else {
                    let count = usize::from(is_pair(ty)) + 1;
                    let fillers = if count == 2 && pair_rounding {
                        cursor.pair_padding(general_bank())
                    } else {
                        0
                    };
                    next += fillers + count;
                    items.push(Placed {
                        fillers,
                        passing: Passing::Value,
                    });
                }
            }
        }
        let placed = items.last().expect("each source argument has a placement");
        for _ in 0..placed.fillers {
            let _ = place(&mut cursor, RegClass::Gpr64);
        }
        let (class, count) = match &placed.passing {
            Passing::Chunks(chunks) => (RegClass::Gpr64, chunks.len()),
            Passing::Vectors {
                shape,
                skipped,
                stack,
            } => {
                for _ in 0..*skipped {
                    let _ = place(&mut cursor, RegClass::Simd64);
                }
                for (_, size) in simd_argument_pieces(*shape, *stack) {
                    let _ = place(&mut cursor, simd_class(size));
                }
                continue;
            }
            Passing::Pointer => (RegClass::Gpr64, 1),
            Passing::Value if is_pair(ty) => (RegClass::Gpr64, 2),
            Passing::Value => (class_of(ty).unwrap_or(RegClass::Gpr64), 1),
            Passing::Destination(_) => (RegClass::Gpr64, 0),
        };
        for _ in 0..count {
            let _ = place(&mut cursor, class);
        }
    }
    RecordPlan {
        items,
        positions: next,
        indirect_result,
    }
}

/// The ordinary arguments of a call or an entry block, paired with their marks.
fn marked_types(types: &[EvoIRType], marks: &[ParamMark]) -> Vec<(EvoIRType, ParamMark)> {
    types
        .iter()
        .enumerate()
        .filter(|(_, ty)| is_register_type(ty))
        .map(|(index, ty)| (ty.clone(), param_mark(marks, index)))
        .collect()
}

/// Section 2's block-argument discipline applied to the record destination:
/// give every block the address of the frame slot the result is written to.
///
/// The slot is allocated once, in the entry block, and a `ret` that must read
/// the record out of it is in some other block, whose "arguments are its only
/// inputs". Threading it as one more argument of every block and one more
/// value on every edge is what the discipline admits.
fn thread_destination(
    blocks: &mut [MBlock],
    entry_slot: VReg,
    readers: &HashMap<u32, VReg>,
    next_vreg: &mut u32,
) {
    let mut available: HashMap<u32, VReg> = HashMap::new();
    for (index, block) in blocks.iter_mut().enumerate() {
        if index == 0 {
            available.insert(block.id, entry_slot);
            if let Some(reader) = readers.get(&block.id) {
                // The entry block reads the slot it defines, so the copy goes
                // where the definition is, not where the loads are.
                let at = block
                    .insts
                    .iter()
                    .position(|inst| inst_defs(inst).contains(&entry_slot))
                    .map_or(0, |position| position + 1);
                block.insts.insert(
                    at,
                    MInst::Mov {
                        dst: *reader,
                        src: entry_slot,
                    },
                );
            }
            continue;
        }
        let reg = readers.get(&block.id).copied().unwrap_or_else(|| {
            let reg = VReg::new(*next_vreg, RegClass::Gpr64);
            *next_vreg += 1;
            reg
        });
        block.args.push(reg);
        available.insert(block.id, reg);
    }
    for block in blocks.iter_mut() {
        let source = available[&block.id];
        for args in term_edge_args(&mut block.term) {
            args.push(source);
        }
    }
}

/// Mutable access to the argument list of every edge a terminator has.
fn term_edge_args(term: &mut MTerm) -> Vec<&mut Vec<VReg>> {
    match term {
        MTerm::Indirect { edges, .. } => edges.iter_mut().map(|(_, args)| args).collect(),
        MTerm::B { args, .. } => vec![args],
        MTerm::BCond {
            taken_args,
            untaken_args,
            ..
        }
        | MTerm::Cb {
            taken_args,
            untaken_args,
            ..
        }
        | MTerm::Tb {
            taken_args,
            untaken_args,
            ..
        } => vec![taken_args, untaken_args],
        MTerm::AsmGoto {
            labels,
            fallthrough_args,
            ..
        } => {
            let mut slots: Vec<&mut Vec<VReg>> = labels.iter_mut().map(|(_, args)| args).collect();
            slots.push(fallthrough_args);
            slots
        }
        MTerm::Ret { .. } | MTerm::Brk { .. } => Vec::new(),
    }
}

/// The types of a call's arguments, in order.
fn arg_types(args: &[Value]) -> Vec<EvoIRType> {
    args.iter().map(|value| value.ty.clone()).collect()
}

/// The registers a call passes and, when it has one, the destination a record
/// result of at most sixteen bytes is stored into when the call returns.
type CallArguments = (Vec<VReg>, Option<(RecordShape, VReg)>);

/// The registers a call passes, with the record convention applied.
///
/// Returns the argument registers and, when the callee writes its result
/// through a destination of at most sixteen bytes, that destination's shape and
/// pointer: the caller passes no argument for it and stores the result into it
/// when the call returns.
fn call_arguments(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    args: &[Value],
    marks: &[ParamMark],
    plan: &RecordPlan,
) -> Result<CallArguments, String> {
    let _ = marks;
    let mut registers: Vec<VReg> = Vec::new();
    let mut returned = None;
    let mut indirect_destination = None;
    let ordinary = args.iter().filter(|value| is_register_type(&value.ty));
    for (value, placed) in ordinary.zip(&plan.items) {
        for _ in 0..placed.fillers {
            // The skipped register is passed as zero rather than left
            // undefined, so that two lowerings of one unit agree byte for byte.
            let filler = lowerer.word(insts, 0);
            registers.push(filler);
        }
        match &placed.passing {
            // Section 6: "loaded from the copy into the next one or two
            // general registers as 8-byte chunks".
            Passing::Chunks(chunks) => {
                let base = lowerer.reg(value)?;
                let mut offset = 0;
                for size in chunks {
                    let chunk = load_chunk(lowerer, insts, base, offset, *size);
                    registers.push(chunk);
                    offset += size;
                }
            }
            Passing::Vectors {
                shape,
                skipped,
                stack,
            } => {
                for _ in 0..*skipped {
                    let filler = lowerer.fresh(RegClass::Simd64);
                    insts.push(MInst::MovImm {
                        dst: filler,
                        imm: 0,
                    });
                    registers.push(filler);
                }
                let base = lowerer.reg(value)?;
                for (offset, size) in simd_argument_pieces(*shape, *stack) {
                    let chunk = lowerer.fresh(simd_class(size));
                    insts.push(MInst::Load {
                        align: shape.align.min(size as u32),
                        dst: chunk,
                        base,
                        offset: offset as i64,
                        kind: simd_load(size),
                        volatile: false,
                    });
                    registers.push(chunk);
                }
            }
            // Above sixteen bytes the pointer to the caller's copy is the
            // argument, "which is what AAPCS64 does too".
            Passing::Pointer => registers.push(lowerer.reg(value)?),
            Passing::Destination(shape) => {
                if shape.size > lowerer.abi.record_register_bytes && shape.simd_members().is_none()
                {
                    // The IR's fixed sret slot can precede anonymous
                    // arguments. Keep it out of their ordinary placement;
                    // the machine call's final slot always names x8.
                    indirect_destination = Some(lowerer.reg(value)?);
                } else {
                    returned = Some((*shape, lowerer.reg(value)?));
                }
            }
            Passing::Value => registers.extend(lowerer.registers(value)?),
        }
    }
    if let Some(destination) = indirect_destination {
        registers.push(destination);
    }
    Ok((registers, returned))
}

/// Section 6: a record result of at most sixteen bytes comes back in `x0` and
/// `x1`, and "stored by the caller into its destination".
fn call_record_result(
    lowerer: &mut Lowerer,
    outputs: &mut Vec<VReg>,
    returned: Option<(RecordShape, VReg)>,
) -> Vec<MInst> {
    let Some((shape, destination)) = returned else {
        return Vec::new();
    };
    let mut after = Vec::new();
    if let Some((count, size)) = shape.simd_members() {
        for member in 0..count {
            let chunk = lowerer.fresh(simd_class(size));
            outputs.push(chunk);
            after.push(MInst::Store {
                align: shape.align.min(size as u32),
                src: chunk,
                base: destination,
                offset: i64::from(member) * size as i64,
                kind: simd_store(size),
                volatile: false,
            });
        }
        return after;
    }
    let mut offset = 0;
    for size in record_chunks(shape.size) {
        let chunk = lowerer.fresh(RegClass::Gpr64);
        outputs.push(chunk);
        store_chunk(lowerer, &mut after, chunk, destination, offset, size);
        offset += size;
    }
    after
}
