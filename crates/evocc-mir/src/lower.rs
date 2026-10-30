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

pub(crate) use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
pub(crate) mod blocks;
pub(crate) use crate::isa::aarch64::lower::*;
mod instructions;
mod integer_width;
mod quad;
mod zero_test;

pub(crate) use evocc_evoir::{
    call_argument_marks, inst_results, inst_uses, BitOp, Block, CmpOp, Function, InlineAssembly,
    Inst, Module, TrapReason, UbReason, Value, VariadicOp,
};
pub(crate) use evocc_types::{param_mark, EvoIRType, ParamMark, RecordShape};

pub(crate) use crate::isa::aarch64::abi::{
    general_bank, place, vector_bank, Aarch64Abi, ProfileFeature, KERNEL,
};
pub(crate) use crate::isa::aarch64::encodings;
pub(crate) use crate::{
    inst_defs, AsmBody, AsmClobber, AsmConstraint, AsmDirection, AsmOperand, AsmValue, BinOp,
    CarryOp, Cond, ExtOp, FlagOp, FramePlace, LoadKind, MBlock, MFunction, MInst, MModule, MTerm,
    RegClass, StoreKind, SymbolKind, Target, UnaryOp, VReg, WideMul, Width,
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

impl Refusal {
    /// Keep the machine location even when only the source function is known.
    pub fn message(&self) -> String {
        if self.function.is_empty() {
            format!("{}: {}", self.name, self.detail)
        } else {
            format!(
                "{}: @{} block {} instruction {}: {}",
                self.name, self.function, self.block, self.index, self.detail
            )
        }
    }
}

/// The Section 10 table.
pub(crate) const E4002: (&str, &str) = ("E4002", "not-lowered: vector");
pub(crate) const E4003: (&str, &str) = ("E4003", "not-lowered: wide-integer");
pub(crate) const E4004: (&str, &str) = ("E4004", "not-lowered: tuple");
pub(crate) const E4009: (&str, &str) = ("E4009", "not-lowered: unsupported-form");
pub(crate) const E4010: (&str, &str) = ("E4010", "abi: too-many-register-arguments");
pub(crate) const E4013: (&str, &str) = ("E4013", "abi: too-many-results");

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
/// Section 6.1: on Linux an argument that finds no register goes to the stack
/// in an 8-byte slot. On Darwin, only the Apple convention
/// (`lower_module_for_options`) packs named stack arguments by
/// natural size and alignment; this entry refuses them with `E4010`. The
/// target also decides whether a 16-byte-aligned argument starts at an even
/// register (Section 6.5).
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
    lower_module_with_guard_control(
        target,
        module,
        external,
        ub_checks,
        &BTreeMap::new(),
        false,
        false,
    )
}

/// The lowering that native code generation selects with `options`: its
/// UB-check profile; the Apple arm64 variadic convention, in which every
/// anonymous argument of a variadic call goes to the stack in 8-byte slots
/// and `va_list` is one pointer to the next anonymous argument
/// (`CodegenOptions::anonymous_on_stack`); and `-mstrict-align`, which
/// moves a record in general registers in aligned pieces (Section 8.9).
pub fn lower_module_for_options(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
    options: &crate::emit::CodegenOptions,
) -> Result<MModule, Vec<Refusal>> {
    lower_module_with_guard_control(
        target,
        module,
        external,
        options.ub_checks,
        &BTreeMap::new(),
        options.anonymous_on_stack,
        options.strict_alignment,
    )
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
        false,
        false,
    )
}

fn lower_module_with_guard_control(
    target: Target,
    module: &Module,
    external: &dyn Fn(&str) -> bool,
    ub_checks: UbCheckProfile,
    true_predicates: &BTreeMap<String, BTreeSet<u32>>,
    anonymous_on_stack: bool,
    strict_alignment: bool,
) -> Result<MModule, Vec<Refusal>> {
    let mut refusals = Vec::new();
    for function in &module.functions {
        scan_function(
            (
                target,
                match target.isa() {
                    crate::Isa::Aarch64 => target.aarch64_abi(),
                },
            ),
            module,
            function,
            anonymous_on_stack,
            &mut refusals,
        );
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
            anonymous_on_stack,
            strict_alignment,
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
pub(crate) fn is_register_type(ty: &EvoIRType) -> bool {
    !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World)
}

/// Section 2's class table, and the Section 10 code for a type it has no row
/// for.
pub(crate) fn class_of(ty: &EvoIRType) -> Result<RegClass, (&'static str, &'static str)> {
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
        // Section 2.6: a half value is its representation bits.
        EvoIRType::F16 | EvoIRType::BF16 => Ok(RegClass::Gpr32),
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
pub(crate) fn is_pair(ty: &EvoIRType) -> bool {
    matches!(ty, EvoIRType::Bits(128))
}

pub(crate) fn vector_bytes(ty: &EvoIRType) -> Option<u64> {
    let (lanes, bits) = evocc_evoir::vector_shape(ty)?;
    Some(u64::from(lanes) * u64::from(bits) / 8)
}

/// The number of significant bits a value carries: Section 3's `N`.
fn significant_bits(ty: &EvoIRType) -> u16 {
    match ty {
        EvoIRType::Bits(width) => *width,
        EvoIRType::F16 | EvoIRType::BF16 => 16,
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
    (target, abi): (Target, &Aarch64Abi),
    module: &Module,
    function: &Function,
    anonymous_on_stack: bool,
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
    if !target.is_linux()
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
    let plan = record_plan(target, &marked, true, None, anonymous_on_stack);
    if let Some(detail) = &plan.refused {
        report(
            E4010,
            entry_block,
            0,
            format!("@{}: {detail}", function.name),
        );
    }
    // Section 10's `E4010` is "more than eight integer arguments on Darwin
    // (Linux lowers stack arguments, revision 0.10)": the ABI table's
    // `stack-arguments` fact. The Apple convention places them instead.
    if plan.positions > abi.general_arguments() && !abi.facts.stack_arguments {
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
        if let Err(code) = class_of(ty) {
            report(code, entry_block, 0, format!("parameter of type {ty:?}"));
        }
        if tiny_floating_vector(ty) {
            report(
                E4002,
                entry_block,
                0,
                format!("a parameter of type {ty:?} is no AAPCS64 short vector"),
            );
        }
    }
    let results = ordinary_types(&function.ret_tys);
    if results.len() > abi.general_results() {
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
        if tiny_floating_vector(ty) {
            report(
                E4002,
                entry_block,
                0,
                format!("a result of type {ty:?} is no AAPCS64 short vector"),
            );
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
            scan_inst(
                (target, abi),
                module,
                anonymous_on_stack,
                inst,
                block.id,
                index,
                &mut report,
            );
        }
        scan_inst(
            (target, abi),
            module,
            anonymous_on_stack,
            &block.terminator,
            block.id,
            block.insts.len(),
            &mut report,
        );
    }
}

fn scan_inst(
    (target, abi): (Target, &Aarch64Abi),
    module: &Module,
    anonymous_on_stack: bool,
    inst: &Inst,
    block: u32,
    index: usize,
    report: &mut impl FnMut((&'static str, &'static str), u32, usize, String),
) {
    // Section 2.6: a half value converts from an integer of at most 32 bits.
    // C lowering reduces a wider integer first (EvoIR 3.13.6).
    let half_floating = match inst {
        Inst::Floating(operation) => Some(operation),
        Inst::StrictFloating(operation) => Some(&operation.instruction),
        _ => None,
    };
    if let Some(operation) = half_floating {
        if matches!(
            operation.op,
            evocc_evoir::FloatingOp::FromSigned | evocc_evoir::FloatingOp::FromUnsigned
        ) && evocc_evoir::is_half_type(&operation.result.ty)
            && significant_bits(&operation.arguments[0].ty) > 32
        {
            report(
                E4009,
                block,
                index,
                "a half value converts from an integer of at most 32 bits".into(),
            );
        }
    }
    if !target.is_linux() {
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

    // Section 6.2: a floating vector below eight bytes crosses no call
    // boundary, as an argument, a result or an anonymous argument.
    let boundary = match inst {
        Inst::Call { results, args, .. } | Inst::IndirectCall { results, args, .. } => results
            .iter()
            .chain(args)
            .map(|value| &value.ty)
            .find(|ty| tiny_floating_vector(ty)),
        Inst::VaAccess(access) => match &access.op {
            VariadicOp::Arg(result) => Some(&result.ty).filter(|ty| tiny_floating_vector(ty)),
            _ => None,
        },
        _ => None,
    };
    if let Some(ty) = boundary {
        report(
            E4002,
            block,
            index,
            format!("a call boundary of type {ty:?} is no AAPCS64 short vector"),
        );
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
            let anonymous = if anonymous_on_stack {
                module
                    .functions
                    .iter()
                    .find(|function| &function.name == callee)
                    .map(|function| {
                        let params: Vec<_> = function.params.iter().map(|p| p.ty.clone()).collect();
                        anonymous_start(function.variadic, &params)
                    })
                    .or_else(|| {
                        module
                            .declares
                            .iter()
                            .find(|declaration| &declaration.name == callee)
                            .map(|declaration| {
                                anonymous_start(declaration.variadic, &declaration.params)
                            })
                    })
                    .flatten()
            } else {
                None
            };
            let plan = record_plan(
                target,
                &marked_types(&arg_types(args), &marks),
                true,
                anonymous,
                anonymous_on_stack,
            );
            if let Some(detail) = &plan.refused {
                report(
                    E4010,
                    block,
                    index,
                    format!("a call to @{callee}: {detail}"),
                );
            }
            let positions = plan.positions;
            if positions > abi.general_arguments() && !abi.facts.stack_arguments {
                report(
                    E4010,
                    block,
                    index,
                    format!("a call to @{callee} passes {positions} arguments"),
                );
            }
            if results.len() > abi.general_results() {
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
            let anonymous = match &callee.ty {
                EvoIRType::FnPtr {
                    params, variadic, ..
                } if anonymous_on_stack => anonymous_start(*variadic, params),
                _ => None,
            };
            let plan = record_plan(
                target,
                &marked_types(&arg_types(args), &marks),
                true,
                anonymous,
                anonymous_on_stack,
            );
            if let Some(detail) = &plan.refused {
                report(E4010, block, index, format!("an indirect call: {detail}"));
            }
            let positions = plan.positions;
            if positions > abi.general_arguments() && !abi.facts.stack_arguments {
                report(
                    E4010,
                    block,
                    index,
                    format!("an indirect call passes {positions} arguments"),
                );
            }
            if results.len() > abi.general_results() {
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
        | EvoIRType::F16
        | EvoIRType::BF16
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

pub(crate) struct Lowerer {
    /// The selected operating system's generated ABI table.
    pub(crate) abi: &'static Aarch64Abi,
    pub(crate) ub_checks: UbCheckProfile,
    pub(crate) diagnostic_true_predicates: BTreeSet<u32>,
    pub(crate) dynamic_addresses: HashSet<u32>,
    pub(crate) access_alignments: HashMap<u32, u32>,
    pub(crate) function_name: String,
    pub(crate) next_vreg: u32,
    pub(crate) next_block: u32,
    pub(crate) map: HashMap<u32, VReg>,
    /// The high half of every value Section 2 represents as a pair; `map`
    /// holds the low half, which is the half every ordinary row already reads.
    pub(crate) high: HashMap<u32, VReg>,
    pub(crate) blocks: Vec<MBlock>,
    pub(crate) traps: Vec<(u16, u32)>,
    /// Named argument register count and incoming stack extent in bytes.
    /// `None` in a fixed function; both are from the call placement plan.
    pub(crate) named_arguments: Option<(u8, u8, u64)>,
    /// The Apple variadic convention (`CodegenOptions::anonymous_on_stack`).
    pub(crate) anonymous_on_stack: bool,
    /// `-mstrict-align` (`CodegenOptions::strict_alignment`): a record that
    /// moves through general registers is read and written in pieces no wider
    /// than the alignment of their addresses (Section 8.9).
    pub(crate) strict_alignment: bool,
    /// Literal definitions and their integer conversions, by value id.
    ///
    /// EvoIR Section 3.11 admits immediate assembly operands (`I`, `J`, `K`,
    /// `L`, `i`, `n`), which emission "writes literally"; the constant is a
    /// prior definition in the same function, and this is where an assembly
    /// operand finds it.
    pub(crate) constants: HashMap<u32, u64>,
    /// Direct `ptr.null` definitions only, not inferred integer addresses.
    pub(crate) null_pointers: HashSet<u32>,
    pub(crate) symbolic_addresses: HashMap<u32, (String, i64)>,
    pub(crate) literal_only: HashSet<u32>,
    /// Section 6 (revision 0.18): the shape of the record result this function
    /// returns in `x0`/`x1`, and the frame slot the entry block gave it.
    pub(crate) destination: Option<(RecordShape, VReg)>,
    /// The register each block's `ret` reads the record result out of, which
    /// `thread_destination` turns into that block's extra argument.
    pub(crate) destination_readers: HashMap<u32, VReg>,
    /// How many record frame slots this function has taken an origin for.
    pub(crate) record_origins: u64,
    /// Only pure address preparation already executed in the current machine
    /// block. Clear on every block boundary, including guard/varargs splits;
    /// these temporaries are not implicit inputs to a continuation.
    pub(crate) address_preparation: AddressPreparation,
}

#[derive(Default)]
pub(crate) struct AddressPreparation {
    pub(crate) extended: HashMap<(VReg, u16, evocc_evoir::IndexSignedness), VReg>,
    pub(crate) scaled: HashMap<(VReg, u64), VReg>,
    pub(crate) factors: HashMap<u64, VReg>,
    pub(crate) memory: HashMap<u32, (VReg, i64, usize)>,
}

impl Lowerer {
    /// The alignment of a frame slot that this lowering gives a record that
    /// moves through general registers: the record's own alignment, and under
    /// `-mstrict-align` at least the eight bytes of a chunk, so that each
    /// chunk moves as one aligned access (Section 8.9).
    pub(crate) fn record_slot_alignment(&self, shape: RecordShape) -> u32 {
        if self.strict_alignment && shape.simd_members().is_none() {
            shape.align.max(8)
        } else {
            shape.align
        }
    }

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
        // The first access must still form the pointer before any effect or
        // possibly failing computation. After that access, the same immutable
        // base and offset already formed successfully on this path. Later
        // accesses may use that address again; they retain their own memory
        // checks and volatile events. Loads/stores are never discarded by DCE.
        // No fact crosses a machine-block boundary, including a guard split.
        let following = &insts[after..];
        let first = following
            .iter()
            .find(|inst| !matches!(inst, MInst::MovImm { .. }));
        let formed = match first {
            None => true,
            Some(
                MInst::Load {
                    base: prior_base,
                    offset: prior_offset,
                    ..
                }
                | MInst::Store {
                    base: prior_base,
                    offset: prior_offset,
                    ..
                },
            ) => *prior_base == base && *prior_offset == offset,
            _ => false,
        };
        let encodable = encodings::memory_offset(offset, size);
        // Strict-align emission may expand the access to byte instructions.
        // Every byte offset must fit too, even if the wide scaled offset fits.
        let byte_safe = u64::from(align) >= size || encodings::memory_byte_expansion(offset, size);
        Ok(if formed && encodable && byte_safe {
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

    pub(crate) fn fresh(&mut self, class: RegClass) -> VReg {
        let reg = VReg::new(self.next_vreg, class);
        self.next_vreg += 1;
        reg
    }

    pub(crate) fn fresh_block(&mut self) -> u32 {
        let id = self.next_block;
        self.next_block += 1;
        id
    }

    pub(crate) fn reg(&self, value: &Value) -> Result<VReg, String> {
        self.map
            .get(&value.id)
            .copied()
            .ok_or_else(|| format!("v{} has no register", value.id))
    }

    pub(crate) fn bind(&mut self, value: &Value, reg: VReg) {
        self.map.insert(value.id, reg);
    }

    /// Both halves of a pair, low first.
    pub(crate) fn pair(&self, value: &Value) -> Result<(VReg, VReg), String> {
        let low = self.reg(value)?;
        let high = self
            .high
            .get(&value.id)
            .copied()
            .ok_or_else(|| format!("v{} has no high half", value.id))?;
        Ok((low, high))
    }

    pub(crate) fn bind_pair(&mut self, value: &Value, low: VReg, high: VReg) {
        self.map.insert(value.id, low);
        self.high.insert(value.id, high);
    }

    /// Every register a value occupies, low half first.
    pub(crate) fn registers(&self, value: &Value) -> Result<Vec<VReg>, String> {
        if is_pair(&value.ty) {
            let (low, high) = self.pair(value)?;
            Ok(vec![low, high])
        } else {
            Ok(vec![self.reg(value)?])
        }
    }

    pub(crate) fn fresh_pair(&mut self) -> (VReg, VReg) {
        (self.fresh(RegClass::Gpr64), self.fresh(RegClass::Gpr64))
    }

    pub(crate) fn word(&mut self, insts: &mut Vec<MInst>, imm: u64) -> VReg {
        let dst = self.fresh(RegClass::Gpr64);
        insts.push(MInst::MovImm { dst, imm });
        dst
    }

    /// One 64-bit register-register form.
    pub(crate) fn word_binary(
        &mut self,
        insts: &mut Vec<MInst>,
        op: BinOp,
        lhs: VReg,
        rhs: VReg,
    ) -> VReg {
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
    pub(crate) fn word_binary_imm(
        &mut self,
        insts: &mut Vec<MInst>,
        op: BinOp,
        lhs: VReg,
        imm: u64,
    ) -> VReg {
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
    pub(crate) fn binary(
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
    pub(crate) fn binary_imm(
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
    pub(crate) fn zero_extend(
        &mut self,
        insts: &mut Vec<MInst>,
        src: VReg,
        bits: u16,
        width: Width,
    ) -> VReg {
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
    pub(crate) fn sign_extend(
        &mut self,
        insts: &mut Vec<MInst>,
        src: VReg,
        bits: u16,
        width: Width,
    ) -> VReg {
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
    pub(crate) fn both(&mut self, insts: &mut Vec<MInst>, lhs: VReg, rhs: VReg) -> VReg {
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

#[allow(clippy::too_many_arguments)]
fn lower_function(
    target: Target,
    signatures: &Signatures<'_>,
    function: &Function,
    external: &dyn Fn(&str) -> bool,
    ub_checks: UbCheckProfile,
    diagnostic_true_predicates: BTreeSet<u32>,
    anonymous_on_stack: bool,
    strict_alignment: bool,
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
        abi: match target.isa() {
            crate::Isa::Aarch64 => target.aarch64_abi(),
        },
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
            .then(|| named_argument_extent(target, function, anonymous_on_stack)),
        anonymous_on_stack,
        strict_alignment,
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
    integer_width::select(&mut lowerer.blocks);
    zero_test::select(&mut lowerer.blocks);
    Ok(MFunction {
        name: function.name.clone(),
        blocks: lowerer.blocks,
    })
}

/// The record marks of every callee this module can name.
pub(crate) struct Signatures<'a> {
    pub(crate) tls: &'a [evocc_evoir::TlsSymbol],
    pub(crate) functions: HashMap<&'a str, &'a [ParamMark]>,
    /// The ordinary position of the first anonymous argument of each
    /// variadic callee.
    pub(crate) variadic: HashMap<&'a str, usize>,
}

impl<'a> Signatures<'a> {
    pub(crate) fn of(module: &'a Module) -> Self {
        let mut functions: HashMap<&'a str, &'a [ParamMark]> = HashMap::new();
        let mut variadic: HashMap<&'a str, usize> = HashMap::new();
        for function in &module.functions {
            functions.insert(function.name.as_str(), function.param_marks.as_slice());
            let params: Vec<_> = function.params.iter().map(|p| p.ty.clone()).collect();
            if let Some(start) = anonymous_start(function.variadic, &params) {
                variadic.insert(function.name.as_str(), start);
            }
        }
        for declaration in &module.declares {
            functions.insert(
                declaration.name.as_str(),
                declaration.param_marks.as_slice(),
            );
            if let Some(start) = anonymous_start(declaration.variadic, &declaration.params) {
                variadic.insert(declaration.name.as_str(), start);
            }
        }
        for alias in &module.aliases {
            if let Some(marks) = functions.get(alias.target.as_str()).copied() {
                functions.insert(alias.name.as_str(), marks);
            }
            if let Some(start) = variadic.get(alias.target.as_str()).copied() {
                variadic.insert(alias.name.as_str(), start);
            }
        }
        Self {
            functions,
            tls: &module.tls,
            variadic,
        }
    }

    pub(crate) fn marks(&self, callee: &str) -> &[ParamMark] {
        self.functions.get(callee).copied().unwrap_or(&[])
    }

    pub(crate) fn anonymous_start(&self, callee: &str) -> Option<usize> {
        self.variadic.get(callee).copied()
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
    let (args, prologue, indirect_result) =
        arguments::lower_arguments(target, lowerer, function, block, entry)?;

    let mut cursor = blocks::BlockCursor::new(lowerer, entry, id, args, prologue, indirect_result);
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
        if cursor.should_skip(index) {
            continue;
        }
        if cursor.lower_obligation(lowerer, block, &use_counts, index, inst, &canonical_true)? {
            continue;
        }
        if variadic::lower_access(lowerer, &mut cursor, block, index, inst)? {
            continue;
        }
        if cursor.select_darwin_product(target, lowerer, block, index)? {
            continue;
        }
        if cursor.fuse_compare(lowerer, block, &use_counts, index, inst)? {
            continue;
        }
        cursor.lower_selected(
            target,
            signatures,
            lowerer,
            block,
            &use_counts,
            index,
            inst,
            external,
        )?;
    }
    cursor.finish(lowerer, function, block, ids)
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

/// The registers a continuation block must receive: everything already bound
/// that instruction `from` onwards still reads.
pub(crate) fn live_across(lowerer: &Lowerer, block: &Block, from: usize) -> Vec<VReg> {
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
                .filter(|v| encodings::add_sub_immediate(*v))
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
                // Section 6.2: the Apple convention returns a tiny vector in
                // `v0` with widened lanes.
                if let Some(shape) =
                    tiny_vector_shape(&value.ty).filter(|_| lowerer.anonymous_on_stack)
                {
                    results[slot] = widen_apple_result(lowerer, insts, results[slot], shape);
                }
                // Section 6.3: a half result leaves in a transport register.
                if evocc_evoir::is_half_type(&value.ty) {
                    results[slot] = half_to_transport(lowerer, insts, results[slot]);
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
                            mask: None,
                            transport: false,
                        });
                        results.push(chunk);
                    }
                    return Ok(MTerm::Ret { values: results });
                }
                let base_align = u64::from(lowerer.record_slot_alignment(shape));
                let mut offset = 0;
                for size in record_chunks(shape.size) {
                    let chunk = load_chunk(lowerer, insts, base, base_align, offset, size);
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

/// Decode only literal integer conversions. Mask before sign extension:
/// a narrow EvoIR value does not acquire the spare bits of its register.
/// Block parameters, poison, symbolic addresses and 128-bit values do not
/// enter this path. The original EvoIR and its obligations are unchanged.
fn literal_integer_cast<'a>(
    inst: &'a Inst,
    constants: &HashMap<u32, u64>,
) -> Option<(&'a Value, u64)> {
    let (result, source, ty) = match inst {
        Inst::SExt(result, source, ty)
        | Inst::ZExt(result, source, ty)
        | Inst::Trunc(result, source, ty) => (result, source, ty),
        _ => return None,
    };
    let (EvoIRType::Bits(from @ 1..=64), EvoIRType::Bits(to @ 1..=64)) = (&source.ty, &result.ty)
    else {
        return None;
    };
    if ty != &result.ty {
        return None;
    }
    let mask = u64::MAX >> (64 - from);
    let mut raw = constants.get(&source.id)? & mask;
    if matches!(inst, Inst::SExt(..)) && raw & (1_u64 << (from - 1)) != 0 {
        raw |= !mask;
    }
    Some((result, raw & (u64::MAX >> (64 - to))))
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
    instructions::record_symbolic_definition(lowerer, inst);
    if instructions::lower_literal_or_pair(lowerer, insts, inst)? {
        return Ok(());
    }
    match inst {
        Inst::Const(..) | Inst::TlsAddr(..) | Inst::Uninitialized(..) => {
            instructions::lower_basic(target, signatures, lowerer, insts, inst)?
        }
        Inst::Add(..)
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
        | Inst::ICmp(..) => instructions::lower_integer_operations(lowerer, insts, inst)?,
        Inst::PtrNull(..)
        | Inst::VaAccess(..)
        | Inst::Asm(..)
        | Inst::Vector(..)
        | Inst::Floating(..)
        | Inst::StrictFloating(..)
        | Inst::BitOperation { .. } => instructions::lower_special_values(lowerer, insts, inst)?,
        Inst::PointerCast { .. }
        | Inst::GlobalAddr { .. }
        | Inst::FuncAddr { .. }
        | Inst::BlockAddr { .. }
        | Inst::ReturnAddress { .. }
        | Inst::FrameAddress { .. }
        | Inst::PtrOffset { .. } => instructions::lower_addresses(lowerer, insts, inst)?,
        Inst::Alloca { .. }
        | Inst::DynamicAlloca { .. }
        | Inst::LifetimeEnd { .. }
        | Inst::PointerValid { .. }
        | Inst::AccessInBounds { .. } => instructions::lower_storage(lowerer, insts, inst)?,
        Inst::Atomic(..) => instructions::lower_atomics(lowerer, insts, inst)?,
        Inst::Load { .. } | Inst::Store { .. } | Inst::Copy { .. } => {
            instructions::lower_memory(lowerer, insts, inst)?
        }
        Inst::Call { .. } | Inst::IndirectCall { .. } => {
            instructions::lower_calls(target, signatures, lowerer, insts, inst, external)?
        }
        Inst::DivisorNonZero(..) | Inst::SignedDivRemNoOverflow(..) => {
            instructions::lower_division_predicates(lowerer, insts, inst)?
        }
        Inst::SignedAddDefined(..) | Inst::SignedSubDefined(..) | Inst::SignedMulDefined(..) => {
            instructions::lower_arithmetic_predicates(lowerer, insts, inst)?
        }
        Inst::ShiftCountDefined { .. } | Inst::SignedShlDefined { .. } => {
            instructions::lower_shift_predicates(lowerer, insts, inst)?
        }
        other => return Err(format!("{other:?} has no row in Section 5")),
    }
    Ok(())
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

/// A generic form on memory (EvoIR 3.8): one exclusive or acquire/release
/// access of the whole object through a general register, and ordinary
/// accesses of the buffers. A compare-exchange stores the observed value in
/// its observed buffer and sets its result from the comparison.
#[allow(clippy::too_many_arguments)]
fn lower_atomic_copy(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    access: &evocc_evoir::AtomicAccess,
    op: evocc_evoir::AtomicCopyOp,
    buffers: &[Value],
    result: Option<&Value>,
    layout: evocc_evoir::AccessLayout,
) -> Result<(), String> {
    use evocc_evoir::AtomicCopyOp as C;
    let ty = EvoIRType::Bits(layout.size_bytes as u16 * 8);
    let class = class_of(&ty).map_err(|(code, _)| code.to_owned())?;
    let (load, store) = (load_kind(&ty)?, store_kind(&ty)?);
    let align = if u64::from(layout.align_bytes) == layout.size_bytes {
        0
    } else {
        layout.align_bytes
    };
    let object = lowerer.reg(&access.address)?;
    let buffers = buffers
        .iter()
        .map(|buffer| lowerer.reg(buffer))
        .collect::<Result<Vec<_>, _>>()?;
    let read = |lowerer: &mut Lowerer, insts: &mut Vec<MInst>, base: VReg| {
        let dst = lowerer.fresh(class);
        insts.push(MInst::Load {
            align,
            dst,
            base,
            offset: 0,
            kind: load,
            volatile: false,
            mask: None,
            transport: false,
        });
        dst
    };
    let write = |insts: &mut Vec<MInst>, base: VReg, src: VReg| {
        insts.push(MInst::Store {
            align,
            src,
            base,
            offset: 0,
            kind: store,
            volatile: false,
            mask: None,
            transport: false,
        });
    };
    match op {
        C::Load => {
            let value = lowerer.fresh(class);
            insts.push(MInst::Atomic {
                load: true,
                value,
                base: object,
                kind: load,
                order: access.order,
                volatile: access.volatile,
            });
            write(insts, buffers[0], value);
        }
        C::Store => {
            let value = read(lowerer, insts, buffers[0]);
            insts.push(MInst::Atomic {
                load: false,
                value,
                base: object,
                kind: load,
                order: access.order,
                volatile: access.volatile,
            });
        }
        C::Exchange | C::CompareExchange => {
            let compare = op == C::CompareExchange;
            let expected = compare.then(|| read(lowerer, insts, buffers[0]));
            let value = read(lowerer, insts, buffers[usize::from(compare)]);
            let old = lowerer.fresh(class);
            insts.push(MInst::AtomicRmw {
                dst: old,
                base: object,
                value,
                expected,
                operation: if compare {
                    evocc_memory::AtomicRmwOp::CompareExchange
                } else {
                    evocc_memory::AtomicRmwOp::Exchange
                },
                kind: load,
                order: access.order,
                volatile: access.volatile,
            });
            write(insts, buffers[if compare { 2 } else { 1 }], old);
            if let (Some(expected), Some(result)) = (expected, result) {
                let equal = lowerer.compare_into(insts, old, expected, width_of(class), Cond::Eq);
                lowerer.bind(result, equal);
            }
        }
    }
    Ok(())
}

pub(crate) fn load_kind(ty: &EvoIRType) -> Result<LoadKind, String> {
    Ok(match ty {
        EvoIRType::F16 | EvoIRType::BF16 => LoadKind::H,
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
        EvoIRType::F16 | EvoIRType::BF16 => StoreKind::H,
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
    if operation
        .arguments
        .iter()
        .chain(std::iter::once(result))
        .any(|v| evocc_evoir::is_half_type(&v.ty))
    {
        return lower_half(lowerer, insts, operation, strict);
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
            returns_twice: false,
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

/// A floating operation with a half value (Section 2.6). A constant is its
/// bits in a `gpr32`, and a representation move renames that register. A
/// conversion between the two half formats passes through the exact
/// binary32 value. An integer source of at most 32 bits is extended first.
fn lower_half(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    operation: &evocc_evoir::FloatingInst,
    strict: bool,
) -> Result<(), String> {
    use evocc_evoir::FloatingOp::*;
    let result = &operation.result;
    let half = |ty: &EvoIRType| evocc_evoir::floating_kind(ty).filter(|kind| kind.is_half());
    match operation.op {
        Constant(bits) => {
            let dst = lowerer.fresh(RegClass::Gpr32);
            insts.push(MInst::MovImm {
                dst,
                imm: bits as u64,
            });
            lowerer.bind(result, dst);
        }
        ToBits | FromBits => {
            let input = lowerer.reg(&operation.arguments[0])?;
            lowerer.bind(result, input);
        }
        Convert => {
            let source = &operation.arguments[0];
            let input = lowerer.reg(source)?;
            let dst = match (half(&source.ty), half(&result.ty)) {
                (Some(from), Some(to)) if from == to => input,
                (Some(from), Some(to)) => {
                    let wide = half_convert(
                        lowerer,
                        insts,
                        Convert,
                        from,
                        input,
                        RegClass::Simd32,
                        strict,
                    );
                    half_convert(lowerer, insts, Convert, to, wide, RegClass::Gpr32, strict)
                }
                (Some(from), None) => {
                    let class = class_of(&result.ty).map_err(|(code, _)| code.to_owned())?;
                    half_convert(lowerer, insts, Convert, from, input, class, strict)
                }
                (None, Some(to)) => {
                    half_convert(lowerer, insts, Convert, to, input, RegClass::Gpr32, strict)
                }
                (None, None) => unreachable!("a half operand or result"),
            };
            lowerer.bind(result, dst);
        }
        FromSigned | FromUnsigned => {
            let source = &operation.arguments[0];
            let bits = significant_bits(&source.ty);
            if bits > 32 {
                return Err("a half value converts from an integer of at most 32 bits".into());
            }
            let input = lowerer.reg(source)?;
            let input = if operation.op == FromSigned {
                lowerer.sign_extend(insts, input, bits, Width::W)
            } else {
                lowerer.zero_extend(insts, input, bits, Width::W)
            };
            let to = half(&result.ty).ok_or("a half result")?;
            let dst = half_convert(
                lowerer,
                insts,
                operation.op,
                to,
                input,
                RegClass::Gpr32,
                strict,
            );
            lowerer.bind(result, dst);
        }
        _ => return Err("a half format admits only conversions".into()),
    }
    Ok(())
}

/// One machine half conversion: `kind` is the half format, and `class` is
/// the class of the result.
fn half_convert(
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    op: evocc_evoir::FloatingOp,
    kind: evocc_types::FloatKind,
    input: VReg,
    class: RegClass,
    strict: bool,
) -> VReg {
    let dst = lowerer.fresh(class);
    insts.push(MInst::Floating {
        strict,
        integer_bits: 0,
        dst,
        op,
        kind,
        args: vec![input],
    });
    dst
}

/// The parts of a complex product or quotient (EvoIR 3.13.5) call the
/// target's runtime helper: libgcc's `__mul?c3` and `__div?c3` on Linux,
/// compiler-rt's on Darwin. Two adjacent parts with the same operands are one
/// call; the helper returns both parts. Helper names stay below EvoIR.
/// One part of a complex product or quotient, and whether it is strict.
fn complex_part(inst: &Inst) -> Option<(&evocc_evoir::FloatingInst, bool)> {
    match inst {
        Inst::Floating(operation) if operation.is_complex_helper() => Some((operation, false)),
        Inst::StrictFloating(operation) if operation.instruction.is_complex_helper() => {
            Some((&operation.instruction, true))
        }
        _ => None,
    }
}

/// Whether a complex part is a quotient, and whether it is the imaginary part.
fn complex_family(op: evocc_evoir::FloatingOp) -> (bool, bool) {
    match op {
        evocc_evoir::FloatingOp::ComplexMul { imaginary } => (false, imaginary),
        evocc_evoir::FloatingOp::ComplexDiv { imaginary } => (true, imaginary),
        _ => unreachable!("complex part"),
    }
}

/// The part at `index` and the other part, when it follows with the same
/// operands. Strict parts chain their world, so nothing runs between them.
fn complex_pair(
    block: &evocc_evoir::Block,
    index: usize,
) -> Option<(
    &evocc_evoir::FloatingInst,
    Option<&evocc_evoir::FloatingInst>,
    bool,
)> {
    let (first, strict) = complex_part(&block.insts[index])?;
    let (division, imaginary) = complex_family(first.op);
    let second = block
        .insts
        .get(index + 1)
        .and_then(complex_part)
        .filter(|(other, other_strict)| {
            *other_strict == strict
                && complex_family(other.op) == (division, !imaginary)
                && other.arguments == first.arguments
        })
        .map(|(other, _)| other);
    Some((first, second, strict))
}

/// Darwin: the parts of a complex product are Apple clang's inline form,
/// `(ac - bd) + (ad + bc)i`. Only a result whose two parts are both NaN
/// calls compiler-rt's helper, which computes the product again. A NaN
/// result of an arithmetic operation is quiet, so the two comparisons
/// raise no status.
fn lower_darwin_product(
    target: Target,
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    (current_id, current_args): (&mut u32, &mut Vec<VReg>),
    block: &evocc_evoir::Block,
    index: usize,
) -> Result<Option<usize>, String> {
    use evocc_evoir::FloatingOp::{Add, Mul, Sub, Une};
    if target.is_linux() {
        return Ok(None);
    }
    let Some((first, second, strict)) = complex_pair(block, index) else {
        return Ok(None);
    };
    if complex_family(first.op).0 {
        return Ok(None);
    }
    let kind = evocc_evoir::floating_kind(&first.result.ty).ok_or("complex part format")?;
    let suffix = match kind {
        evocc_types::FloatKind::F32 => "sc3",
        evocc_types::FloatKind::F64 => "dc3",
        _ => return Err("binary128 complex helpers are qualified only for Linux".into()),
    };
    let [a, b, c, d]: [VReg; 4] = first
        .arguments
        .iter()
        .map(|value| lowerer.reg(value))
        .collect::<Result<Vec<_>, _>>()?
        .try_into()
        .map_err(|_| "complex part arity")?;
    let class = crate::floating::class(kind);
    let mut floating = |lowerer: &mut Lowerer, op, args: Vec<VReg>, dst_class| {
        let dst = lowerer.fresh(dst_class);
        insts.push(MInst::Floating {
            strict,
            integer_bits: 0,
            dst,
            op,
            kind,
            args,
        });
        dst
    };
    let ac = floating(lowerer, Mul, vec![a, c], class);
    let bd = floating(lowerer, Mul, vec![b, d], class);
    let ad = floating(lowerer, Mul, vec![a, d], class);
    let bc = floating(lowerer, Mul, vec![b, c], class);
    let x = floating(lowerer, Sub, vec![ac, bd], class);
    let y = floating(lowerer, Add, vec![ad, bc], class);
    let x_nan = floating(lowerer, Une, vec![x, x], RegClass::Gpr32);
    let y_nan = floating(lowerer, Une, vec![y, y], RegClass::Gpr32);
    let both = lowerer.binary(insts, BinOp::And, x_nan, y_nan, Width::W);
    let length = 1 + usize::from(second.is_some());
    let carried = live_across(lowerer, block, index + length);
    let width = carried.len();
    let helper = lowerer.fresh_block();
    let merge = lowerer.fresh_block();
    let mut to_helper = carried.clone();
    to_helper.extend([a, b, c, d]);
    let mut to_merge = carried.clone();
    to_merge.extend([x, y]);
    lowerer.blocks.push(MBlock {
        id: *current_id,
        args: std::mem::take(current_args),
        insts: std::mem::take(insts),
        term: MTerm::Tb {
            nonzero: true,
            src: both,
            bit: 0,
            taken: helper,
            taken_args: to_helper.clone(),
            untaken: merge,
            untaken_args: to_merge.clone(),
        },
    });
    let params = fresh_like(lowerer, &to_helper);
    let results = vec![lowerer.fresh(class), lowerer.fresh(class)];
    let mut reached = params[..width].to_vec();
    reached.extend(results.iter().copied());
    lowerer.blocks.push(MBlock {
        id: helper,
        args: params.clone(),
        insts: vec![MInst::Bl {
            symbol: format!("__mul{suffix}"),
            args: params[width..].to_vec(),
            results,
            fp_mode: None,
            returns_twice: false,
        }],
        term: MTerm::B {
            target: merge,
            args: reached,
        },
    });
    let params = fresh_like(lowerer, &to_merge);
    let rebind: HashMap<u32, VReg> = carried
        .iter()
        .zip(params.iter())
        .map(|(old, fresh)| (old.id, *fresh))
        .collect();
    for reg in lowerer.map.values_mut().chain(lowerer.high.values_mut()) {
        if let Some(fresh) = rebind.get(&reg.id) {
            *reg = *fresh;
        }
    }
    for operation in std::iter::once(first).chain(second) {
        let imaginary = complex_family(operation.op).1;
        lowerer.bind(&operation.result, params[width + usize::from(imaginary)]);
    }
    *current_id = merge;
    *current_args = params;
    Ok(Some(length))
}

fn lower_complex_parts(
    target: Target,
    lowerer: &mut Lowerer,
    insts: &mut Vec<MInst>,
    block: &evocc_evoir::Block,
    index: usize,
) -> Result<Option<usize>, String> {
    let Some((first, second, strict)) = complex_pair(block, index) else {
        return Ok(None);
    };
    let division = complex_family(first.op).0;
    let kind = evocc_evoir::floating_kind(&first.result.ty).ok_or("complex part format")?;
    if !target.is_linux() && kind == evocc_types::FloatKind::F128 {
        return Err("binary128 complex helpers are qualified only for Linux".into());
    }
    let suffix = match kind {
        evocc_types::FloatKind::F32 => "sc3",
        evocc_types::FloatKind::F64 => "dc3",
        _ => "tc3",
    };
    let symbol = format!("__{}{suffix}", if division { "div" } else { "mul" });
    let args = first
        .arguments
        .iter()
        .map(|value| lowerer.reg(value))
        .collect::<Result<Vec<_>, _>>()?;
    let class = crate::floating::class(kind);
    let results = vec![lowerer.fresh(class), lowerer.fresh(class)];
    // The Linux helpers are the qualified hosted runtime; Darwin's are an
    // ordinary call into the platform runtime, which owns their status.
    let fp_mode = (target.is_linux()).then_some(if strict {
        crate::FpCallMode::Strict
    } else {
        crate::FpCallMode::Fixed
    });
    if fp_mode.is_some() {
        insts.push(MInst::FpEnvCheck { strict });
    }
    insts.push(MInst::Bl {
        symbol,
        args,
        results: results.clone(),
        fp_mode,
        returns_twice: false,
    });
    for operation in std::iter::once(first).chain(second) {
        lowerer.bind(
            &operation.result,
            results[usize::from(complex_family(operation.op).1)],
        );
    }
    Ok(Some(1 + usize::from(second.is_some())))
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
                evocc_evoir::vector_shape(&result.ty).ok_or("invalid vector shape")?;
            let dst = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
            let (lane_bits, arguments) =
                if let Some((_, kind)) = evocc_evoir::floating_vector_shape(&result.ty) {
                    // Section 2.7: a floating lane moves to a general register as
                    // its exact bits, and the integer lane move packs it.
                    let mut lanes = Vec::with_capacity(args.len());
                    for value in args {
                        let source = lowerer.reg(value)?;
                        let raw = lowerer.fresh(if lane_bits == 32 {
                            RegClass::Gpr32
                        } else {
                            RegClass::Gpr64
                        });
                        insts.push(MInst::Floating {
                            strict: false,
                            integer_bits: 0,
                            dst: raw,
                            op: evocc_evoir::FloatingOp::ToBits,
                            kind,
                            args: vec![source],
                        });
                        lanes.push(raw);
                    }
                    (lane_bits, lanes)
                } else if lane_bits == 128 {
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
                evocc_evoir::vector_shape(&args[0].ty).ok_or("invalid vector shape")?;
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
            } else if let Some((_, kind)) = evocc_evoir::floating_vector_shape(&args[0].ty) {
                // Section 2.7: the integer lane move reads the lane's bits,
                // and an exact bit move gives the floating value.
                let raw = lowerer.fresh(if lane_bits == 32 {
                    RegClass::Gpr32
                } else {
                    RegClass::Gpr64
                });
                insts.push(MInst::Vector {
                    dst: raw,
                    extract: true,
                    lane_bits,
                    bytes,
                    args: vec![source, index],
                });
                let dst = lowerer.fresh(class_of(&result.ty).map_err(|e| e.1)?);
                insts.push(MInst::Floating {
                    strict: false,
                    integer_bits: 0,
                    dst,
                    op: evocc_evoir::FloatingOp::FromBits,
                    kind,
                    args: vec![raw],
                });
                lowerer.bind(result, dst);
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
            .filter(|raw| encodings::add_sub_immediate(*raw))
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
            returns_twice: false,
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

// ---------------------------------------------------------------------------
// Section 6: variadic functions (revision 0.10)
// ---------------------------------------------------------------------------

/// One fresh register per entry of `shape`, in the same classes.
pub(crate) fn fresh_like(lowerer: &mut Lowerer, shape: &[VReg]) -> Vec<VReg> {
    shape.iter().map(|reg| lowerer.fresh(reg.class)).collect()
}

/// Round a 64-bit value up to sixteen: `(value + 15) & ~15`.
pub(crate) fn round_to_sixteen(lowerer: &mut Lowerer, insts: &mut Vec<MInst>, value: VReg) -> VReg {
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
        // A masked pair access always takes the two-access form, so that
        // each half keeps its half of the mask; only a bit-field access has a
        // mask, and its guard alignment is 1.
        Inst::Load {
            result,
            address,
            mem_in,
            volatile,
            mask,
            ..
        } if is_pair(&result.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.fresh_pair();
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            if align < 8 || mask.is_some() {
                for (dst, offset, half) in [
                    (low, 0, pair_mask(*mask, 0)),
                    (high, 8, pair_mask(*mask, 1)),
                ] {
                    insts.push(MInst::Load {
                        align: if align < 8 { align } else { 0 },
                        dst,
                        base,
                        offset,
                        kind: LoadKind::X,
                        volatile: *volatile,
                        mask: half,
                        transport: false,
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
            mask,
            ..
        } if is_pair(&value.ty) => {
            let base = lowerer.reg(address)?;
            let (low, high) = lowerer.pair(value)?;
            let align = lowerer
                .access_alignments
                .get(&mem_in.id)
                .copied()
                .unwrap_or(1);
            if align < 8 || mask.is_some() {
                for (src, offset, half) in [
                    (low, 0, pair_mask(*mask, 0)),
                    (high, 8, pair_mask(*mask, 1)),
                ] {
                    insts.push(MInst::Store {
                        align: if align < 8 { align } else { 0 },
                        src,
                        base,
                        offset,
                        kind: StoreKind::X,
                        volatile: *volatile,
                        mask: half,
                        transport: false,
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

/// The mask of a machine access of at most 64 bits (EvoIR 3.8).
fn register_mask(mask: Option<u128>) -> Result<Option<u64>, String> {
    mask.map(|mask| {
        u64::try_from(mask).map_err(|_| "a masked access wider than a register".to_owned())
    })
    .transpose()
}

/// The mask of one half of a masked `bits<128>` access, low half first, or
/// none when the half has every bit.
fn pair_mask(mask: Option<u128>, half: u32) -> Option<u64> {
    let bits = (mask? >> (64 * half)) as u64;
    (bits != u64::MAX).then_some(bits)
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
pub(crate) enum PairShift {
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
pub(crate) const RECORD_ORIGIN_BASE: u64 = 1 << 62;

/// One element of the Apple named stack area as the caller passes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AppleWord {
    /// One piece that fills the word, or the two words of a vector of
    /// sixteen bytes: the argument index and the piece.
    Whole(usize, ApplePiece),
    /// The pieces that share one word, as (argument index, piece, byte
    /// offset in the word). An empty list is padding.
    Composed(Vec<(usize, ApplePiece, u64)>),
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
pub(crate) fn arg_types(args: &[Value]) -> Vec<EvoIRType> {
    args.iter().map(|value| value.ty.clone()).collect()
}
