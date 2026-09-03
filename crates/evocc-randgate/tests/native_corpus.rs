//! EvoMIR Section 9, "The native corpus".
//!
//! "The randomized generator's seed space is pinned by the middle-end
//! campaigns (a seed names one module forever), so the native lane does not
//! change it. It draws from a second generator mode with its own seed space,
//! `native-main`, whose `main` takes no arguments and returns one `bits<32>`,
//! and its programs are compared by exit status modulo 256, the eight bits a
//! process status carries."
//!
//! Three things are measured here: that the second mode is a second seed space
//! and not a change to the first, that every one of its programs survives
//! lowering, allocation, verification, emission, the host assembler, the host
//! linker and a run, and that the eight bits the host reports are the eight
//! bits the EvoIR interpreter computed.

use std::collections::BTreeMap;
use std::time::Instant;

use evocc_evoir::{render_module, validate_module, Module};
use evocc_mir::{compile_module, native_corpus_entry, NativeStop, Toolchain};
use evocc_proof::sha256_hex;
use evocc_randgate::{
    entry_arguments, generate_module, observe, run_campaign, GateBudget, GeneratorConfig,
    GeneratorMode, Observation, ProofChecker,
};
use evocc_types::EvoIRType;

/// The seed the native campaign runs from, in the `native-main` seed space.
const NATIVE_SEED: u64 = 0x4E41_5400_0001;
const NATIVE_PROGRAMS: u64 = 256;

/// The seeds the four-lane campaigns run from, disjoint from the campaign
/// above and from each other.
const NATIVE_GATE_SEED: u64 = 0x4E41_5400_1001;
const NATIVE_GATE_PROGRAMS: u64 = 1_000;
const NATIVE_LONG_SEED: u64 = 0x4E41_5400_9001;
const NATIVE_LONG_PROGRAMS: u64 = 20_000;

/// The interpreter fuel one native-corpus program is judged under, the same
/// figure the gate's own budget uses.
const STEP_BUDGET: u64 = 200_000;

fn native() -> GeneratorConfig {
    GeneratorConfig::native_main()
}

/// The eight bits the EvoIR interpreter's `bits<32>` result carries, or the
/// diagnostic that made the execution undefined.
fn interpreted(module: &Module, seed: u64) -> Result<u8, &'static str> {
    match observe(module, &entry_arguments(module, seed), STEP_BUDGET) {
        Observation::Defined(run) => {
            let value = run
                .returns
                .first()
                .expect("the native corpus returns one ordinary result");
            Ok(u8::try_from(value.raw_bits & 0xff).expect("eight bits"))
        }
        Observation::Undefined { code, .. } => Err(code),
    }
}

// ---------------------------------------------------------------------------
// The mode is a second seed space, not a change to the first
// ---------------------------------------------------------------------------

/// The digests of six modules of the pinned seed space, taken before the second
/// mode existed. Section 9: "a seed names one module forever".
const PINNED: [(u64, &str); 6] = [
    (
        0,
        "68cba24ed8b81a90814db347878541e0546f93150b6383e73ccd6c8ab63426e5",
    ),
    (
        1,
        "53512855e270958e88f57c87151a9fd74d2639b1c9aca0684e82fbbd9f3788c6",
    ),
    (
        1_592_591_013,
        "d79a2f3a06678c5c944f413a4f9a8b3113d6647631f9cc8476d50e5b71717bb0",
    ),
    (
        0x5EED_0001,
        "d546657935178e73625ecd6f1883023e8498c3884c81e4579a2d2b3b1fc7c050",
    ),
    (
        0x5EED_9001,
        "62ba531e1ca9791717d7619a4ae2796b3bff5ae8dcb614b854df8c70f542ee49",
    ),
    (
        31337,
        "21d92ba060d5a7e7b21f00eea9d2bea4138151927e8bbfeb08cc34eeba5ebe89",
    ),
];

#[test]
fn the_pinned_seed_space_still_names_the_modules_it_named() {
    for (seed, digest) in PINNED {
        let module = generate_module(seed, GeneratorConfig::default());
        assert_eq!(
            sha256_hex(render_module(&module).as_bytes()),
            digest,
            "seed {seed} no longer names the module it named"
        );
    }
}

#[test]
fn the_native_mode_is_a_second_seed_space() {
    assert_eq!(GeneratorConfig::default().mode, GeneratorMode::Standard);
    assert_eq!(native().mode, GeneratorMode::NativeMain);

    let mut differing = 0;
    for seed in 0..256_u64 {
        let standard = render_module(&generate_module(seed, GeneratorConfig::default()));
        let corpus = render_module(&generate_module(seed, native()));
        if standard != corpus {
            differing += 1;
        }
    }
    assert_eq!(
        differing, 256,
        "the two modes must name different modules under the same seed"
    );
}

#[test]
fn a_native_seed_names_one_module_forever() {
    for seed in 0..64_u64 {
        let first = render_module(&generate_module(seed, native()));
        let second = render_module(&generate_module(seed, native()));
        assert_eq!(first, second, "seed {seed} is not deterministic");
    }
}

#[test]
fn the_native_entry_takes_no_arguments_and_returns_one_word() {
    for seed in 0..512_u64 {
        let module = generate_module(seed, native());
        validate_module(&module).expect("a generated module validates");
        native_corpus_entry(&module).unwrap_or_else(|reason| {
            panic!("seed {seed} is not a native-corpus program: {reason}")
        });

        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .expect("every generated module defines @main");
        let ordinary: Vec<&EvoIRType> = main
            .params
            .iter()
            .map(|param| &param.ty)
            .filter(|ty| !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World))
            .collect();
        assert!(
            ordinary.is_empty(),
            "seed {seed}: @main takes {} ordinary arguments",
            ordinary.len()
        );
        let results: Vec<&EvoIRType> = main
            .ret_tys
            .iter()
            .filter(|ty| !matches!(ty, EvoIRType::Mem(_) | EvoIRType::World))
            .collect();
        assert_eq!(
            results,
            vec![&EvoIRType::Bits(32)],
            "seed {seed}: @main does not return one bits<32>"
        );
    }
}

// ---------------------------------------------------------------------------
// The campaign
// ---------------------------------------------------------------------------

#[test]
fn the_native_campaign_agrees_with_the_interpreter() {
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={}", toolchain.version());

    let started = Instant::now();
    let mut discarded: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut stopped: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut refusal_codes: BTreeMap<&'static str, u64> = BTreeMap::new();
    let mut assembled = 0_u64;
    let mut linked = 0_u64;
    let mut ran = 0_u64;
    let mut agreeing = 0_u64;
    let mut disagreements: Vec<(u64, u8, u8, String)> = Vec::new();

    for offset in 0..NATIVE_PROGRAMS {
        let seed = NATIVE_SEED.wrapping_add(offset);
        let module = generate_module(seed, native());
        let expected = match interpreted(&module, seed) {
            Ok(status) => status,
            Err(code) => {
                *discarded.entry(code).or_default() += 1;
                continue;
            }
        };

        let text = match compile_module(&module) {
            Ok(text) => text,
            Err(stop) => {
                *stopped.entry(stop.component()).or_default() += 1;
                if let NativeStop::Lowering(codes) = &stop {
                    for code in codes {
                        *refusal_codes.entry(*code).or_default() += 1;
                    }
                }
                continue;
            }
        };
        let name = format!("corpus{seed:x}");
        let object = match toolchain.assemble(&name, &text) {
            Ok(object) => object,
            Err(stop) => {
                *stopped.entry(stop.component()).or_default() += 1;
                panic!(
                    "seed {seed}: the assembler refused\n{}\n{text}",
                    stop.detail()
                );
            }
        };
        assembled += 1;
        let program = match toolchain.link(&name, &object) {
            Ok(program) => program,
            Err(stop) => panic!("seed {seed}: the linker refused\n{}", stop.detail()),
        };
        linked += 1;
        let observed = match toolchain.run(&program) {
            Ok(status) => status,
            Err(stop) => panic!("seed {seed}: {}", stop.detail()),
        };
        ran += 1;
        if observed == expected {
            agreeing += 1;
        } else {
            disagreements.push((seed, expected, observed, render_module(&module)));
        }
    }

    let codes = refusal_codes
        .iter()
        .map(|(code, count)| format!("{code}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let discards = discarded
        .iter()
        .map(|(code, count)| format!("{code}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    let stops = stopped
        .iter()
        .map(|(component, count)| format!("{component}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("firstSeed={NATIVE_SEED}");
    println!("programs={NATIVE_PROGRAMS}");
    println!("discardedUndefined={discards}");
    println!("stoppedByComponent={stops}");
    println!("refusedByCode={codes}");
    println!("assembled={assembled}");
    println!("linked={linked}");
    println!("ran={ran}");
    println!("exitStatusesAgreeing={agreeing}");
    println!("wallTimeSeconds={:.3}", started.elapsed().as_secs_f64());

    if let Some((seed, expected, observed, text)) = disagreements.first() {
        panic!("seed {seed}: the interpreter said {expected}, the host said {observed}\n{text}");
    }
    assert_eq!(assembled, linked);
    assert_eq!(linked, ran);
    assert_eq!(ran, agreeing);
    assert!(
        ran * 2 >= NATIVE_PROGRAMS,
        "only {ran} of {NATIVE_PROGRAMS} programs reached the host"
    );
}

// ---------------------------------------------------------------------------
// The fourth lane
// ---------------------------------------------------------------------------

#[test]
fn the_gate_runs_four_lanes_over_the_native_corpus() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let started = Instant::now();
    let report = run_campaign(
        NATIVE_GATE_SEED,
        NATIVE_GATE_PROGRAMS,
        native(),
        GateBudget::default(),
        &checker,
    );
    println!("{}", report.render());
    println!("wallTimeSeconds={:.3}", started.elapsed().as_secs_f64());

    assert_eq!(report.generated, NATIVE_GATE_PROGRAMS);
    assert_eq!(
        report.mismatched,
        0,
        "mismatches: {:#?}",
        report.mismatches.first()
    );
    assert_eq!(report.compared, NATIVE_GATE_PROGRAMS);
    assert_eq!(report.certificates_replayed, report.transactions);

    // The fourth lane is enabled when the host toolchain is present and skipped
    // with a counted reason otherwise; either way the report says which.
    if Toolchain::locate().is_ok() {
        assert!(
            report.native_run > 0,
            "the host toolchain is present but the fourth lane ran nothing:\n{}",
            report.render()
        );
        assert_eq!(
            report.native_agreeing,
            report.native_run,
            "the fourth lane disagreed on {} programs",
            report.native_run - report.native_agreeing
        );
    } else {
        assert_eq!(report.native_run, 0);
        assert!(
            !report.native_skipped.is_empty(),
            "a skipped lane must carry its reason"
        );
    }
}

#[test]
#[ignore = "long run: reserved for the standing campaign, not for every commit"]
fn the_extended_native_campaign_finds_no_mismatch() {
    let checker = ProofChecker::locate().expect("evo-proofcheck");
    let started = Instant::now();
    let report = run_campaign(
        NATIVE_LONG_SEED,
        NATIVE_LONG_PROGRAMS,
        native(),
        GateBudget::default(),
        &checker,
    );
    println!("{}", report.render());
    println!("wallTimeSeconds={:.3}", started.elapsed().as_secs_f64());
    assert_eq!(report.generated, NATIVE_LONG_PROGRAMS);
    assert_eq!(
        report.mismatched,
        0,
        "mismatches: {:#?}",
        report.mismatches.first()
    );
    assert_eq!(report.native_agreeing, report.native_run);
}
