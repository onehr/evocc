//! The randomized semantic gate.
//!
//! These tests are the gate itself, not a harness around it: a rewrite family
//! may only ship while every one of them is green.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;
use std::time::Instant;

use evocc_evoir::{
    inst_results, parse_section9_module, render_module, validate_module, Inst, Module,
};
use evocc_randgate::{
    entry_arguments, generate_module, observe, run_campaign, shrink_module, GateBudget,
    GeneratorConfig, Observation, ProofChecker, Rng, WIDTHS,
};

/// The seed the standing gate runs from. Changing it changes which thousand
/// programs the gate covers, so it is written down rather than derived.
const GATE_SEED: u64 = 0x5EED_0001;
const GATE_PROGRAMS: u64 = 1_000;
/// The independent seed the long run uses, so the two campaigns never overlap.
const LONG_SEED: u64 = 0x5EED_9001;
const LONG_PROGRAMS: u64 = 20_000;

fn config() -> GeneratorConfig {
    GeneratorConfig::default()
}

fn budget() -> GateBudget {
    GateBudget::default()
}

fn family(inst: &Inst) -> &'static str {
    match inst {
        Inst::Const(..) => "const",
        Inst::Uninitialized(..) => "uninitialized",
        Inst::Add(..) => "add",
        Inst::Sub(..) => "sub",
        Inst::Mul(..) => "mul",
        Inst::SDiv(..) => "sdiv",
        Inst::UDiv(..) => "udiv",
        Inst::SRem(..) => "srem",
        Inst::URem(..) => "urem",
        Inst::Shl(..) => "shl",
        Inst::LShr(..) => "lshr",
        Inst::AShr(..) => "ashr",
        Inst::And(..) => "and",
        Inst::Or(..) => "or",
        Inst::Xor(..) => "xor",
        Inst::SignedAddDefined(..) => "signed_add_defined",
        Inst::SignedSubDefined(..) => "signed_sub_defined",
        Inst::SignedMulDefined(..) => "signed_mul_defined",
        Inst::DivisorNonZero(..) => "divisor_nonzero",
        Inst::SignedDivRemNoOverflow(..) => "signed_div_rem_no_overflow",
        Inst::ShiftCountDefined { .. } => "shift_count_defined",
        Inst::SignedShlDefined { .. } => "signed_shl_defined",
        Inst::UbAssume(..) => "ub.assume",
        Inst::ICmp(..) => "icmp",
        Inst::SExt(..) => "sext",
        Inst::ZExt(..) => "zext",
        Inst::Trunc(..) => "trunc",
        Inst::PtrNull(..) => "ptr.null",
        Inst::GlobalAddr { .. } => "global_addr",
        Inst::FuncAddr { .. } => "func_addr",
        Inst::Alloca { .. } => "alloca",
        Inst::LifetimeEnd { .. } => "lifetime.end",
        Inst::PointerValid { .. } => "pointer_valid",
        Inst::AccessInBounds { .. } => "access_in_bounds",
        Inst::Load {
            volatile: false, ..
        } => "load",
        Inst::Load { volatile: true, .. } => "load.volatile",
        Inst::Store {
            volatile: false, ..
        } => "store",
        Inst::Store { volatile: true, .. } => "store.volatile",
        Inst::Copy { .. } => "copy",
        Inst::PtrOffset { .. } => "ptr.offset",
        Inst::Call { .. } => "call",
        Inst::IndirectCall { .. } => "indirect_call",
        Inst::Br(..) => "br",
        Inst::CondBr(..) => "cond_br",
        Inst::Ret(..) => "ret",
        Inst::Trap(..) => "trap",
    }
}

fn families(module: &Module) -> BTreeSet<&'static str> {
    let mut seen = BTreeSet::new();
    for function in &module.functions {
        for block in &function.blocks {
            for inst in &block.insts {
                seen.insert(family(inst));
            }
            seen.insert(family(&block.terminator));
        }
    }
    seen
}

fn instruction_count(module: &Module) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .map(|block| block.insts.len() + 1)
        .sum()
}

#[test]
fn a_seed_names_one_module_forever() {
    for seed in [0_u64, 1, 7, 42, 1_000, u64::MAX] {
        let first = render_module(&generate_module(seed, config()));
        let second = render_module(&generate_module(seed, config()));
        assert_eq!(first, second, "seed {seed} produced two different modules");
    }

    let distinct = (0..64_u64)
        .map(|seed| render_module(&generate_module(seed, config())))
        .collect::<BTreeSet<_>>();
    assert!(
        distinct.len() >= 60,
        "64 seeds produced only {} distinct modules",
        distinct.len()
    );
}

#[test]
fn the_bit_source_is_reproducible_and_spreads() {
    let mut first = Rng::from_seed(12_345);
    let mut second = Rng::from_seed(12_345);
    let a = (0..16).map(|_| first.next_u64()).collect::<Vec<_>>();
    let b = (0..16).map(|_| second.next_u64()).collect::<Vec<_>>();
    assert_eq!(a, b);
    assert_eq!(a.iter().collect::<BTreeSet<_>>().len(), a.len());

    let mut rng = Rng::from_seed(9);
    let mut seen = BTreeSet::new();
    for _ in 0..512 {
        seen.insert(rng.below(7));
    }
    assert_eq!(seen, (0..7).collect::<BTreeSet<_>>());
}

#[test]
fn generated_modules_validate_and_render_canonically() {
    for seed in 0..256_u64 {
        let module = generate_module(seed, config());
        validate_module(&module).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        let text = render_module(&module);
        let reparsed = parse_section9_module(text.as_bytes())
            .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        assert_eq!(
            render_module(&reparsed),
            text,
            "seed {seed} is not canonical"
        );
    }
}

#[test]
fn every_value_definition_in_a_module_is_unique_and_block_local() {
    for seed in 0..128_u64 {
        let module = generate_module(seed, config());
        for function in &module.functions {
            let mut defined = BTreeSet::new();
            for block in &function.blocks {
                for arg in &block.args {
                    assert!(
                        defined.insert(arg.value.id),
                        "seed {seed}: v{} defined twice",
                        arg.value.id
                    );
                }
                for inst in &block.insts {
                    for result in inst_results(inst) {
                        assert!(
                            defined.insert(result.id),
                            "seed {seed}: v{} defined twice",
                            result.id
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn generated_programs_cover_the_declared_instruction_families() {
    let mut seen = BTreeSet::new();
    let mut widths = BTreeSet::new();
    let mut block_argument_counts = BTreeSet::new();
    let mut reasons = BTreeSet::new();
    let mut back_edges = 0_usize;

    for seed in 0..512_u64 {
        let module = generate_module(seed, config());
        seen.extend(families(&module));
        for function in &module.functions {
            for block in &function.blocks {
                block_argument_counts.insert(block.args.len());
                for inst in &block.insts {
                    if let Inst::UbAssume(_, reason) = inst {
                        reasons.insert(reason.mnemonic());
                    }
                }
                for value in block.args.iter().map(|arg| arg.value.clone()).chain(
                    block.insts.iter().flat_map(|inst| {
                        inst_results(inst).into_iter().cloned().collect::<Vec<_>>()
                    }),
                ) {
                    if let evocc_types::EvoIRType::Bits(width) = value.ty {
                        widths.insert(width);
                    }
                }
                let targets: Vec<u32> = match &block.terminator {
                    Inst::Br(target, _) => vec![*target],
                    Inst::CondBr(_, a, _, b, _) => vec![*a, *b],
                    _ => Vec::new(),
                };
                back_edges += targets.iter().filter(|target| **target <= block.id).count();
            }
        }
    }

    let required = [
        "access_in_bounds",
        "add",
        "alloca",
        "and",
        "ashr",
        "br",
        "call",
        "cond_br",
        "const",
        "divisor_nonzero",
        "global_addr",
        "icmp",
        "load",
        "load.volatile",
        "lshr",
        "mul",
        "or",
        "pointer_valid",
        "ptr.offset",
        "ret",
        "sdiv",
        "sext",
        "shift_count_defined",
        "shl",
        "signed_add_defined",
        "signed_div_rem_no_overflow",
        "signed_mul_defined",
        "signed_shl_defined",
        "signed_sub_defined",
        "srem",
        "store",
        "store.volatile",
        "sub",
        "trunc",
        "ub.assume",
        "udiv",
        "urem",
        "xor",
        "zext",
    ];
    let missing = required
        .iter()
        .filter(|name| !seen.contains(*name))
        .collect::<Vec<_>>();
    assert!(missing.is_empty(), "families never generated: {missing:?}");

    let missing_widths = WIDTHS
        .iter()
        .filter(|width| !widths.contains(*width))
        .collect::<Vec<_>>();
    assert!(
        missing_widths.is_empty(),
        "bit widths never generated: {missing_widths:?}"
    );

    assert!(
        block_argument_counts.iter().any(|count| *count > 2),
        "no block ever carried an ordinary block argument"
    );
    assert!(back_edges > 0, "no generated function contained a loop");

    // The exact set, not a subset: a `ub.assume` may only be emitted for a
    // condition the surrounding shape has already made true, so this equality
    // is simultaneously the coverage claim and the record of what the gate does
    // not yet reach. `invalid_signed_left_shift` is absent because no shape here
    // makes `signed_shl_defined` true by construction.
    assert_eq!(
        reasons,
        [
            "division_by_zero",
            "invalid_shift_count",
            "remainder_by_zero",
            "signed_overflow",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "the undefined-behaviour obligations the generator emits changed"
    );
}

#[test]
fn generated_programs_are_free_of_undefined_behaviour_by_construction() {
    let mut codes: BTreeMap<&'static str, usize> = BTreeMap::new();
    for seed in 0..512_u64 {
        let module = generate_module(seed, config());
        let arguments = entry_arguments(&module, seed);
        if let Observation::Undefined { code, detail } =
            observe(&module, &arguments, budget().step_budget)
        {
            *codes.entry(code).or_default() += 1;
            if codes.len() == 1 {
                eprintln!("seed {seed}: {code} {detail}");
            }
        }
    }
    assert!(
        codes.is_empty(),
        "the generator must not emit undefined programs: {codes:?}"
    );
}

#[test]
fn shrinking_preserves_the_refusal_and_reduces_the_module() {
    let seed = (0..512_u64)
        .find(|seed| {
            let module = generate_module(*seed, config());
            families(&module).contains("xor") && instruction_count(&module) > 40
        })
        .expect("a seed with a large enough module containing xor");
    let module = generate_module(seed, config());

    let mut refuses = |candidate: &Module| families(candidate).contains("xor");
    let reduced = shrink_module(&module, &mut refuses, 32);

    validate_module(&reduced).expect("a reduction must still validate");
    assert!(families(&reduced).contains("xor"), "the refusal was lost");
    assert!(
        instruction_count(&reduced) < instruction_count(&module),
        "shrinking removed nothing: {} instructions",
        instruction_count(&reduced)
    );
}

#[test]
fn the_gate_finds_no_mismatch_in_one_thousand_random_programs() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let started = Instant::now();
    let report = run_campaign(GATE_SEED, GATE_PROGRAMS, config(), budget(), &checker);
    let elapsed = started.elapsed();

    println!("{}", report.render());
    println!("wallTimeSeconds={:.3}", elapsed.as_secs_f64());

    assert_eq!(report.generated, GATE_PROGRAMS);
    assert_eq!(
        report.mismatched,
        0,
        "mismatches: {:#?}",
        report.mismatches.first()
    );
    assert_eq!(report.discarded_undefined, 0);
    assert_eq!(report.interpreter_budget_exhausted, 0);
    assert_eq!(report.compared, GATE_PROGRAMS);
    assert_eq!(
        report.certificates_replayed, GATE_PROGRAMS,
        "every trial must replay its certificate through evo-proofcheck"
    );
    assert!(
        report.optimized * 2 >= GATE_PROGRAMS,
        "only {} of {GATE_PROGRAMS} programs were optimized at all; the gate would be a no-op",
        report.optimized
    );
    assert!(report.regions > 0);
}

#[test]
fn the_command_line_gate_reports_a_measurement_table() {
    let output = Command::new(env!("CARGO_BIN_EXE_evo-randgate"))
        .args(["--seed", "31337", "--count", "16"])
        .output()
        .expect("run evo-randgate");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={stdout}, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("generated=16"), "stdout={stdout}");
    assert!(stdout.contains("mismatched=0"), "stdout={stdout}");
    assert!(stdout.contains("firstSeed=31337"), "stdout={stdout}");
}

#[test]
#[ignore = "long run: reserved for the standing campaign, not for every commit"]
fn the_extended_campaign_finds_no_mismatch() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let started = Instant::now();
    let report = run_campaign(LONG_SEED, LONG_PROGRAMS, config(), budget(), &checker);
    println!("{}", report.render());
    println!("wallTimeSeconds={:.3}", started.elapsed().as_secs_f64());
    assert_eq!(report.mismatched, 0, "{:#?}", report.mismatches.first());
    assert_eq!(report.generated, LONG_PROGRAMS);
}
