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
    entry_arguments, generate_module, observe, run_campaign, run_seed, shrink_module, GateBudget,
    GeneratorConfig, Observation, ProofChecker, Rng, Verdict, WIDTHS,
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
        Inst::Asm(_) => "asm",
        Inst::AsmGoto { .. } => "asm.goto",
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

/// Structural shapes a family name cannot express.
///
/// A pointer block argument that nothing reads, a loop whose limit is written
/// into its own entry edge as a constant, or a second return value no function
/// ever declares would all leave the family list satisfied while the shape the
/// gate is supposed to exercise was gone.
#[derive(Default)]
struct Shapes {
    pointer_crosses_an_edge: bool,
    several_ordinary_returns: bool,
    loop_limit_is_a_block_argument: bool,
    loop_limit_is_data_dependent: bool,
    recursion: bool,
    pointer_call_argument: bool,
    pointer_result: bool,
    copy_between_distinct_objects: bool,
}

impl Shapes {
    fn observe(&mut self, module: &Module) {
        for function in &module.functions {
            if function.ret_tys.len() > 3 {
                self.several_ordinary_returns = true;
            }
            if function
                .ret_tys
                .iter()
                .any(|ty| matches!(ty, evocc_types::EvoIRType::Ptr(_)))
            {
                self.pointer_result = true;
            }
            for block in &function.blocks {
                let allocated = block
                    .insts
                    .iter()
                    .filter_map(|inst| match inst {
                        Inst::Alloca { result, .. } => Some(result.id),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>();
                for inst in &block.insts {
                    match inst {
                        Inst::Call { callee, args, .. } => {
                            if callee == &function.name {
                                self.recursion = true;
                            }
                            if args
                                .iter()
                                .any(|arg| matches!(arg.ty, evocc_types::EvoIRType::Ptr(_)))
                            {
                                self.pointer_call_argument = true;
                            }
                        }
                        Inst::IndirectCall { args, .. } => {
                            if args
                                .iter()
                                .any(|arg| matches!(arg.ty, evocc_types::EvoIRType::Ptr(_)))
                            {
                                self.pointer_call_argument = true;
                            }
                        }
                        Inst::Copy {
                            destination,
                            source,
                            ..
                        } if destination.id != source.id
                            && allocated.contains(&destination.id)
                            && allocated.contains(&source.id) =>
                        {
                            self.copy_between_distinct_objects = true;
                        }
                        _ => {}
                    }
                }
                let pointers = block
                    .args
                    .iter()
                    .filter(|arg| matches!(arg.value.ty, evocc_types::EvoIRType::Ptr(_)))
                    .map(|arg| arg.value.id)
                    .collect::<BTreeSet<_>>();
                for inst in &block.insts {
                    let reads_a_carried_pointer = matches!(
                        inst,
                        Inst::AccessInBounds { .. }
                            | Inst::PointerValid { .. }
                            | Inst::Load { .. }
                            | Inst::Store { .. }
                            | Inst::Copy { .. }
                            | Inst::PtrOffset { .. }
                    ) && evocc_evoir::inst_uses(inst)
                        .iter()
                        .any(|used| pointers.contains(&used.id));
                    if reads_a_carried_pointer {
                        self.pointer_crosses_an_edge = true;
                    }
                }
                self.observe_loop(function, block);
            }
        }
    }

    /// A loop header compares its counter against a limit. Both are block
    /// arguments once the limit is threaded, and the value the entry edge puts
    /// in the limit position says whether the bound was a constant or was
    /// derived from the program.
    fn observe_loop(&mut self, function: &evocc_evoir::Function, block: &evocc_evoir::Block) {
        let Inst::CondBr(condition, ..) = &block.terminator else {
            return;
        };
        let Some(Inst::ICmp(_, evocc_evoir::CmpOp::ULt, counter, limit)) =
            block.insts.iter().find(|inst| {
                inst_results(inst)
                    .first()
                    .is_some_and(|r| r.id == condition.id)
            })
        else {
            return;
        };
        let position = |id: u32| block.args.iter().position(|arg| arg.value.id == id);
        let (Some(_), Some(limit_position)) = (position(counter.id), position(limit.id)) else {
            return;
        };
        self.loop_limit_is_a_block_argument = true;

        for source in &function.blocks {
            let arguments = match &source.terminator {
                Inst::Br(target, arguments) if *target == block.id => arguments,
                _ => continue,
            };
            let Some(incoming) = arguments.get(limit_position) else {
                continue;
            };
            let constant = source
                .insts
                .iter()
                .any(|inst| matches!(inst, Inst::Const(result, _) if result.id == incoming.id));
            if !constant {
                self.loop_limit_is_data_dependent = true;
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
    let mut shapes = Shapes::default();

    for seed in 0..512_u64 {
        let module = generate_module(seed, config());
        seen.extend(families(&module));
        shapes.observe(&module);
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
        "copy",
        "divisor_nonzero",
        "func_addr",
        "global_addr",
        "icmp",
        "indirect_call",
        "lifetime.end",
        "load",
        "load.volatile",
        "lshr",
        "mul",
        "or",
        "pointer_valid",
        "ptr.null",
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
    assert!(
        shapes.pointer_crosses_an_edge,
        "no pointer created in one block was used through a block argument in another"
    );
    assert!(
        shapes.several_ordinary_returns,
        "every generated function returned exactly one ordinary value"
    );
    assert!(
        shapes.loop_limit_is_a_block_argument,
        "no loop compared its counter against a threaded limit"
    );
    assert!(
        shapes.loop_limit_is_data_dependent,
        "every loop bound was a constant written into the loop entry edge"
    );
    assert!(shapes.recursion, "no generated function called itself");
    assert!(
        shapes.pointer_call_argument,
        "no pointer was ever passed to a call"
    );
    assert!(shapes.pointer_result, "no function ever returned a pointer");
    assert!(
        shapes.copy_between_distinct_objects,
        "every copy stayed inside one object"
    );

    // The exact set, not a subset: a `ub.assume` may only be emitted for a
    // condition the surrounding shape has already made true, so this equality
    // is simultaneously the coverage claim and the record of what the gate
    // reaches. All five reasons are now emitted.
    assert_eq!(
        reasons,
        [
            "division_by_zero",
            "invalid_shift_count",
            "invalid_signed_left_shift",
            "remainder_by_zero",
            "signed_overflow",
        ]
        .into_iter()
        .collect::<BTreeSet<_>>(),
        "the undefined-behaviour obligations the generator emits changed"
    );
}

/// EvoMIR Section 9's second mode keeps "otherwise the same instruction
/// coverage as the standard mode where the shape allows": only `@main`'s
/// interface is fixed, so every family the pinned mode reaches must still be
/// reached, and the native corpus must not reach a family the pinned one never
/// generated either.
#[test]
fn the_native_mode_reaches_the_families_the_pinned_mode_reaches() {
    let mut standard = BTreeSet::new();
    let mut corpus = BTreeSet::new();
    for seed in 0..512_u64 {
        standard.extend(families(&generate_module(seed, config())));
        corpus.extend(families(&generate_module(
            seed,
            GeneratorConfig::native_main(),
        )));
    }
    println!("pinnedFamilies={}", standard.len());
    println!("nativeFamilies={}", corpus.len());
    let missing: Vec<&&str> = standard.difference(&corpus).collect();
    let extra: Vec<&&str> = corpus.difference(&standard).collect();
    assert!(
        missing.is_empty(),
        "the native corpus never reaches {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "the native corpus reaches {extra:?} alone"
    );
}

/// The native corpus is generated by the same shapes, so it is free of
/// undefined behaviour for the same reason.
#[test]
fn native_corpus_programs_are_free_of_undefined_behaviour_by_construction() {
    let mut codes: BTreeMap<&'static str, usize> = BTreeMap::new();
    for seed in 0..512_u64 {
        let module = generate_module(seed, GeneratorConfig::native_main());
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
        "the native corpus must not emit undefined programs: {codes:?}"
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
        report.certificates_replayed, report.transactions,
        "every transaction of every run must replay its certificate through evo-proofcheck"
    );
    assert!(
        report.optimized * 2 >= GATE_PROGRAMS,
        "only {} of {GATE_PROGRAMS} programs were optimized at all; the gate would be a no-op",
        report.optimized
    );
    assert!(report.regions > 0);
    // Residual regions are a quality metric of the registry rather than a
    // failure (11.3), so the campaign counts them and says so instead of
    // leaving a reader to rerun the optimizer to find out.
    assert!(
        report.render().contains("residualRegions="),
        "the measurement table must carry the residual count:\n{}",
        report.render()
    );
    // A run is a sequence of transactions (8.11), so its length is telemetry
    // the campaign reports rather than a number a reader has to reconstruct.
    assert!(
        report.render().contains("runLengths="),
        "the measurement table must carry the run-length distribution:\n{}",
        report.render()
    );
    // A program with nothing to optimize keeps no transaction, so the run
    // length is zero and the total is bounded below by the optimized count
    // rather than by the compared count.
    assert!(
        report.transactions >= report.optimized,
        "{} transactions across {} optimized programs",
        report.transactions,
        report.optimized
    );
    // A run stops when a transaction admits no region, so a run that stopped
    // before the ceiling has nothing left to admit. Only a run cut off at
    // `max_transactions` may leave one behind.
    if report.runs_at_ceiling == 0 {
        assert_eq!(
            report.residual_regions, 0,
            "a run that stopped short of the ceiling left {} regions unadmitted",
            report.residual_regions
        );
    }
}

/// Digest chaining is what makes a sequence of certificates one run.
///
/// Section 8.11: "the run is verified by digest chaining, the `output` digest
/// of one certificate being the `input` digest of the next, and a chain that
/// breaks anywhere fails the run." Every run has at least two certificates --
/// the transactions that admitted something, and the one that admitted nothing
/// and ended it -- so every run has a chain to walk. The seed that needs a
/// second productive transaction is found rather than written down, because
/// which seed that is moves whenever the generator does.
#[test]
fn a_run_chains_each_certificate_to_the_next_by_digest() {
    let budget = evocc_eqopt::ScalarEqBudget::new(20_000, 256);
    let mut longest = 0_usize;
    for offset in 0..256_u64 {
        let seed = GATE_SEED.wrapping_add(offset);
        let module = generate_module(seed, config());
        let mut current = module.clone();
        let mut chained_from = evocc_proof::sha256_hex(render_module(&module).as_bytes());
        let mut productive = 0_usize;

        loop {
            let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(&current, budget);
            assert_eq!(
                outcome.certificate.input_digest, chained_from,
                "seed {seed} broke the chain at transaction {productive}"
            );
            if outcome.certificate.regions.is_empty() {
                assert_eq!(
                    outcome.certificate.output_digest, outcome.certificate.input_digest,
                    "a transaction that admits nothing must change nothing"
                );
                break;
            }
            chained_from = outcome.certificate.output_digest.clone();
            current = outcome.module;
            productive += 1;
            assert!(productive <= 8, "seed {seed} never reached the ceiling");
        }
        longest = longest.max(productive);
    }
    println!("longestRunInSample={longest}");
    assert!(longest >= 1, "no seed in the sample optimized at all");
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

/// Every seed whose certificate a campaign has refused.
///
/// Twelve come from the first campaign, one from the second and eight from the
/// long run over the fact-driven rules. The first
/// twelve no longer name the modules they named then -- the generator has
/// grown since -- so they are a spot check rather than a pin; the shapes
/// themselves are pinned by the hand-written reproducers in
/// `certificate_binding_refusal.rs`, which no generator change can move.
/// Rediscovering any of them by chance is not a regression test either way.
const REFUSED_SEEDS: [u64; 26] = [
    1_592_591_013,
    1_592_627_955,
    1_592_629_865,
    1_592_631_625,
    1_592_632_596,
    1_592_639_902,
    1_592_640_996,
    1_592_641_595,
    1_592_645_031,
    1_592_645_302,
    1_592_645_787,
    1_592_646_750,
    1_592_636_988,
    // The eight the long run refused once the double-negation rule could fire
    // through a folded inner negation: one nested left pattern resolved
    // against the wrong node of a merged class.
    1_592_629_328,
    1_592_631_555,
    1_592_635_798,
    1_592_641_195,
    1_592_644_260,
    1_592_644_319,
    1_592_645_150,
    1_592_645_621,
    // The five the long run refused once a block argument carried a fact: the
    // sweep read an identity comparison off its own rewritten stream, where an
    // earlier alias had already replaced one operand by the other, and the
    // region could not join the two classes.
    1_592_634_837,
    1_592_638_363,
    1_592_642_422,
    1_592_642_867,
    1_592_644_670,
];

#[test]
fn the_seeds_the_first_campaign_refused_now_replay() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let mut refused = Vec::new();
    for seed in REFUSED_SEEDS {
        match run_seed(seed, config(), budget(), &checker) {
            Verdict::Compared { .. } => {}
            other => refused.push((seed, other)),
        }
    }
    assert!(
        refused.is_empty(),
        "seeds still refused: {:#?}",
        refused.first()
    );
}

/// Fuel is a limit of the host, not a property of the program (10.4). A trial
/// that runs out of it is counted on its own and is never folded into the
/// undefined-behaviour discards or the mismatches, whichever side stopped.
#[test]
fn exhausted_fuel_is_counted_on_its_own() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let starved = GateBudget {
        step_budget: 1,
        ..budget()
    };
    let report = run_campaign(GATE_SEED, 32, config(), starved, &checker);

    assert_eq!(
        report.interpreter_budget_exhausted,
        32,
        "{}",
        report.render()
    );
    assert_eq!(report.discarded_undefined, 0, "{}", report.render());
    assert_eq!(report.mismatched, 0, "{}", report.render());
    assert_eq!(report.compared, 0, "{}", report.render());
}

/// Which rules of the reviewed registry the campaign actually reaches.
///
/// A binding is resolved from the node stream by a path, and `p0_009_double_neg`
/// is the only rule whose path passes through a second node. A gate that never
/// fired it would leave that traversal unmeasured while every certificate still
/// replayed.
#[test]
fn the_campaign_reaches_the_registry_rules_it_baits() {
    let mut fired = BTreeSet::new();
    for seed in 0..256_u64 {
        let module = generate_module(seed, config());
        let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(
            &module,
            evocc_eqopt::ScalarEqBudget::new(20_000, 256),
        );
        for region in &outcome.certificate.regions {
            for event in &region.events {
                if let evocc_proof::EvpEvent::Rewrite(rewrite) = event {
                    fired.insert(rewrite.rule.clone());
                }
            }
        }
    }
    println!(
        "rulesFired={}",
        fired.iter().cloned().collect::<Vec<_>>().join(",")
    );

    // The complete earlier registry, every rule of it: each rule's binding is
    // resolved by the same path mechanism, and a rule that stopped firing would
    // silently take its path out of the campaign. The rules the revision adds
    // have their own assertion below, so this one is a containment rather than
    // an equality.
    let registry = [
        "p0_001_add_zero_rhs",
        "p0_002_add_zero_lhs",
        "p0_003_mul_one_rhs",
        "p0_004_mul_one_lhs",
        "p0_005_and_zero_rhs",
        "p0_006_and_zero_lhs",
        "p0_007_or_zero_rhs",
        "p0_008_or_zero_lhs",
        "p0_009_double_neg",
        "p0_010_icmp_eq_self",
        "p0_011_icmp_ne_self",
        "p0_012_icmp_slt_self",
        "p0_013_icmp_sle_self",
        "p0_014_icmp_ult_self",
        "p0_015_icmp_ule_self",
        "p0_100_fold_add",
        "p0_101_fold_sub",
        "p0_102_fold_mul",
        "p0_103_fold_and",
        "p0_104_fold_or",
        "p0_105_fold_xor",
        "p0_106_fold_icmp_eq",
        "p0_107_fold_icmp_ne",
        "p0_108_fold_icmp_slt",
        "p0_109_fold_icmp_sle",
        "p0_110_fold_icmp_ult",
        "p0_111_fold_icmp_ule",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<BTreeSet<_>>();
    let missing = registry.difference(&fired).cloned().collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "the campaign stopped reaching {}",
        missing.join(",")
    );
}

/// The stop reason a transaction records must be the one its own scheduler
/// earned.
///
/// Section 8.11, restored at revision 1.9.38: `saturated` means no rule fired
/// in the transaction's last epoch, and says nothing about a fresh transaction
/// over the output. A fresh transaction may still admit *residual regions*,
/// because Section 11.6 lets a left-hand `const_like` match only a constant
/// already in the before witness closure and never one an earlier epoch
/// derived. Those are a quality metric of the registry, not a false record, so
/// they are measured and reported here and they do not change the stop reason.
#[test]
fn a_transaction_that_ran_out_of_matches_records_saturation() {
    let budget = evocc_eqopt::ScalarEqBudget::new(20_000, 256);
    let mut residual_regions = 0_u64;
    let mut residual_programs = 0_u64;
    for offset in 0..GATE_PROGRAMS {
        let seed = GATE_SEED.wrapping_add(offset);
        let module = generate_module(seed, config());
        let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(&module, budget);
        let recorded = outcome
            .certificate
            .budget
            .expect("an evp2 certificate carries a budget line")
            .stop;
        assert_eq!(
            recorded,
            evocc_proof::EvpStop::Saturated,
            "seed {seed} stopped with nothing left to fire and must record saturation"
        );

        let fresh = evocc_eqopt::optimize_scalar_module_with_evp1(&outcome.module, budget);
        let admitted = fresh.certificate.regions.len() as u64;
        if admitted != 0 {
            residual_programs += 1;
            residual_regions += admitted;
        }
    }
    println!("residualPrograms={residual_programs}");
    println!("residualRegions={residual_regions}");
}

/// How deep the campaign's schedules actually run.
///
/// An epoch count is only worth recording if it varies, so this measures the
/// distribution rather than asserting a constant, and requires only that more
/// than one epoch is genuinely reached.
#[test]
fn the_campaign_runs_more_than_one_epoch() {
    let budget = evocc_eqopt::ScalarEqBudget::new(20_000, 256);
    let mut multi = 0_u64;
    let mut deepest = 0_u64;
    let mut optimized = 0_u64;
    for offset in 0..GATE_PROGRAMS {
        let seed = GATE_SEED.wrapping_add(offset);
        let module = generate_module(seed, config());
        let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(&module, budget);
        if outcome.certificate.regions.is_empty() {
            continue;
        }
        optimized += 1;
        let epochs = outcome
            .certificate
            .budget
            .expect("an evp2 certificate carries a budget line")
            .epochs
            .consumed;
        deepest = deepest.max(epochs);
        if epochs > 1 {
            multi += 1;
        }
    }
    println!("optimizedPrograms={optimized}");
    println!("multiEpochPrograms={multi}");
    println!("deepestEpoch={deepest}");
    assert!(
        multi > 0,
        "no program ran more than one epoch, so the tiering is untested"
    );
}

/// The fact-driven rules the revisions added: twenty-one reached, three
/// dominated.
///
/// A rule the campaign never fires is a rule whose premise recording, whose
/// side condition and whose builder are all unmeasured, however carefully they
/// are written. Section 11.6 exempts exactly three: `fully_known` on `xor`,
/// `add` and `sub` holds only when both operands are fully known, which is
/// when `all_const` holds, and registry order puts the premise-free fold
/// first. Those three are asserted absent rather than present, so a producer
/// that started firing them would fail this test rather than pass it quietly.
#[test]
fn the_campaign_reaches_the_tier_b_rules() {
    let mut fired = BTreeSet::new();
    for seed in 0..256_u64 {
        let module = generate_module(seed, config());
        let outcome = evocc_eqopt::optimize_scalar_module_with_evp1(
            &module,
            evocc_eqopt::ScalarEqBudget::new(20_000, 256),
        );
        for region in &outcome.certificate.regions {
            for event in &region.events {
                if let evocc_proof::EvpEvent::Rewrite(rewrite) = event {
                    fired.insert(rewrite.rule.clone());
                }
            }
        }
    }
    for rule in [
        "p1_200_known_and",
        "p1_201_known_or",
        "p1_205_known_mul",
        "p1_210_known_icmp_eq",
        "p1_211_known_icmp_ne",
        "p1_212_known_icmp_slt",
        "p1_213_known_icmp_sle",
        "p1_214_known_icmp_ult",
        "p1_215_known_icmp_ule",
        "p1_220_and_cover_rhs",
        "p1_221_and_cover_lhs",
        "p1_222_or_cover_rhs",
        "p1_223_or_cover_lhs",
        "p1_224_xor_known_zero_rhs",
        "p1_225_xor_known_zero_lhs",
        "p1_206_known_shl",
        "p1_207_known_lshr",
        "p1_208_known_ashr",
        "p1_230_known_zext",
        "p1_231_known_sext",
        "p1_232_known_trunc",
    ] {
        assert!(fired.contains(rule), "{rule} never fired");
    }
    // Revision 1.9.47: these three stopped being dominated when a block
    // argument gained a fact of its own. `fully_known` on `xor`, `add` and
    // `sub` used to hold exactly when `all_const` did, so the fold always came
    // first in registry order and the fact rule never fired; a seed that is
    // fully known without any `const` node standing for it separates the two,
    // and a coverage claim counts them as reached when such an input exists.
    for reached in ["p1_202_known_xor", "p1_203_known_add", "p1_204_known_sub"] {
        assert!(
            fired.contains(reached),
            "{reached} fires where the operands are known without being constants"
        );
    }
}

/// The third lane of the EvoMIR specification's Section 9.1: EvoIR before,
/// EvoIR after optimization, EvoMIR after lowering.
///
/// A refusal is counted, not compared. Every optimized module therefore either
/// lowered and was judged, or carries the Section 10 codes that stopped it, and
/// the two accounts add up to the programs the gate compared.
#[test]
fn the_third_lane_lowers_or_refuses_every_compared_program() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let report = run_campaign(GATE_SEED, 256, config(), budget(), &checker);

    assert_eq!(
        report.lowered + report.refused,
        report.compared,
        "{}",
        report.render()
    );
    assert!(
        report.machine_compared + report.machine_budget_exhausted <= report.lowered,
        "{}",
        report.render()
    );
    assert_eq!(report.mismatched, 0, "{}", report.render());
    for code in report.refusal_codes.keys() {
        assert!(
            matches!(
                *code,
                "E4001"
                    | "E4002"
                    | "E4003"
                    | "E4004"
                    | "E4005"
                    | "E4006"
                    | "E4007"
                    | "E4008"
                    | "E4009"
                    | "E4010"
                    | "E4011"
                    | "E4012"
                    | "E4013"
            ),
            "{code} is not a Section 10 diagnostic"
        );
    }
}

/// How much of the corpus the lowering table reaches, measured.
///
/// Revision 0.1 refused nineteen of every twenty programs, all of them for two
/// rows it did not have. Revision 0.2 has them, so the table is expected to
/// reach the whole corpus and any refusal that remains names a form Sections 5
/// and 10 still exclude on purpose.
#[test]
fn the_lowering_table_reaches_the_corpus() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let report = run_campaign(GATE_SEED, 256, config(), budget(), &checker);

    assert!(report.compared > 0, "{}", report.render());
    assert_eq!(report.refused, 0, "{}", report.render());
    assert_eq!(report.lowered, report.compared, "{}", report.render());
    assert_eq!(
        report.machine_compared,
        report.lowered,
        "{}",
        report.render()
    );
    assert_eq!(report.machine_budget_exhausted, 0, "{}", report.render());
    assert_eq!(report.mismatched, 0, "{}", report.render());
}

/// Which forms the table still excludes, over the generated corpus.
///
/// The generator writes no floating point, no vector, no integer wider than
/// sixty-four bits, no tuple and no function of nine arguments or nine
/// results, so the count is expected to be zero and is measured rather than
/// assumed: a generator that grew one of those shapes would show up here as a
/// refusal rather than as a silently unjudged program.
#[test]
fn the_forms_the_table_still_excludes_are_measured() {
    const CORPUS: u64 = 2_000;
    let mut refused = 0_u64;
    let mut codes: std::collections::BTreeMap<&'static str, u64> =
        std::collections::BTreeMap::new();

    for offset in 0..CORPUS {
        let module = generate_module(GATE_SEED.wrapping_add(offset), config());
        let Err(refusals) = evocc_mir::lower_module(&module) else {
            continue;
        };
        refused += 1;
        for refusal in &refusals {
            *codes.entry(refusal.code).or_default() += 1;
        }
    }

    println!("corpus={CORPUS}");
    println!("refused={refused}");
    for (code, count) in &codes {
        println!("{code}={count}");
    }
    assert_eq!(refused, 0, "{codes:?}");
}
