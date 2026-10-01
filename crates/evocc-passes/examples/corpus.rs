//! Compact N02/N05 consumer of the existing generator and interpreters.
//! Does not write captures or install products. `small` retains numeric/hash
//! rows; `full` emits only aggregate counts and hashes, never module text.
use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_evoir::{render_module, Module, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_passes::{
    measure_execution, run_pipeline, ExecutionInput, IdentityPass, ValidationError,
};
use evocc_proof::sha256_hex;
use evocc_randgate::{entry_arguments, generate_module, GeneratorConfig, Observation};
use evocc_types::TargetDataLayout;

#[path = "support/source_inventory.rs"]
mod source_inventory;

fn main() -> Result<(), String> {
    let preset = std::env::args()
        .nth(1)
        .ok_or("usage: corpus small|full|native")?;
    if preset == "native" {
        return native();
    }
    if preset == "preflight-full" {
        return preflight_full();
    }
    if preset == "preflight-source" {
        return preflight_source();
    }
    let ranges: &[(u64, u64)] = match preset.as_str() {
        "small" => &[(0, 256)],
        "full" => &[(0x5EED_0001, 1000), (0x5EED_9001, 20_000)],
        _ => return Err("usage: corpus small|full|native".into()),
    };
    let mut rows = Vec::new();
    let mut identity = String::new();
    let mut modules = 0;
    let mut admitted = 0;
    let mut opaque = 0;
    let mut defined = 0;
    let mut diagnostic = 0;
    let mut steps = 0_u64;
    let mut consume = |module: Module,
                       input: ExecutionInput,
                       source_bridge: &str|
     -> Result<(), String> {
        let before =
            measure_execution(&module, &input).map_err(|e| format!("{}: {e}", input.id))?;
        let (after, validation) =
            match run_pipeline(&module, &[&IdentityPass], std::slice::from_ref(&input)) {
                Ok(output) => {
                    admitted += 1;
                    (
                        measure_execution(&output.module, &input).map_err(|e| e.to_string())?,
                        "VALIDATED_ON_INPUT",
                    )
                }
                Err(ValidationError::Ineligible(_)) => {
                    opaque += 1;
                    (
                        measure_execution(&module, &input).map_err(|e| e.to_string())?,
                        "NOT_ADMITTED_OPAQUE",
                    )
                }
                Err(error) => return Err(format!("{}: {error}", input.id)),
            };
        if before.observation != after.observation || before.steps != after.steps {
            return Err(format!("{}: identity observation changed", input.id));
        }
        let outcome = if matches!(before.observation, Observation::Defined(_)) {
            defined += 1;
            "DEFINED_MATCH"
        } else {
            diagnostic += 1;
            "DIAGNOSTIC_MATCH"
        };
        modules += 1;
        steps = steps
            .checked_add(before.steps.unwrap_or(0))
            .ok_or("step sum overflow")?;
        let module_hash = sha256_hex(render_module(&module).as_bytes());
        identity.push_str(&format!(
            "{}:{module_hash}:{}:{}\n",
            input.id, before.input_sha256, before.outcome_sha256
        ));
        if preset == "small" {
            let count = before
                .steps
                .map_or_else(|| "null".into(), |n| n.to_string());
            rows.push(format!("{{\"input_id\":\"{}\",\"input_sha256\":\"{}\",\"module_sha256\":\"{module_hash}\",\"before_steps\":{count},\"after_steps\":{count},\"before_outcome_sha256\":\"{}\",\"after_outcome_sha256\":\"{}\",\"outcome\":\"{outcome}\",\"validation\":\"{validation}\",\"source_bridge\":\"{source_bridge}\"}}", input.id, before.input_sha256, before.outcome_sha256, after.outcome_sha256));
        }
        Ok(())
    };
    for &(start, count) in ranges {
        for seed in start..start + count {
            let module = generate_module(seed, GeneratorConfig::default());
            let input = ExecutionInput::main(
                format!("seed-{seed}"),
                entry_arguments(&module, seed),
                100_000,
            );
            consume(module, input, "NOT_RUN")?;
        }
    }
    let mut source_bridge = "NOT_RUN";
    let mut source_bridge_detail = "null".to_string();
    if preset == "small" {
        let (module, arguments, bridge) = integer_mix()?;
        source_bridge = bridge.status;
        source_bridge_detail = bridge.to_json();
        consume(
            module,
            ExecutionInput::main("integer-mix", arguments, 100_000),
            source_bridge,
        )?;
    }
    println!("{{\"schema\":\"evocc-pass-corpus-v1\",\"preset\":\"{preset}\",\"modules\":{modules},\"identity_tv_admitted\":{admitted},\"identity_tv_ineligible_opaque\":{opaque},\"defined_measurements\":{defined},\"diagnostic_measurements\":{diagnostic},\"total_before_steps\":{steps},\"total_after_steps\":{steps},\"observations_sha256\":\"{}\",\"native_validation\":\"NOT_RUN\",\"source_bridge\":\"{source_bridge}\",\"source_bridge_detail\":{source_bridge_detail},\"cases\":[{}]}}", sha256_hex(identity.as_bytes()), rows.join(","));
    Ok(())
}

fn preflight_full() -> Result<(), String> {
    use evocc_passes::preflight::{inspect, OpportunityCounts};
    use std::collections::BTreeMap;
    let mut inline = OpportunityCounts::default();
    let mut edge_reuse = OpportunityCounts::default();
    let mut admissions = BTreeMap::<&str, u64>::new();
    let mut identity = String::new();
    let mut executed_inputs = 0;
    for (start, count) in [(0x5EED_0001, 1000), (0x5EED_9001, 20_000)] {
        for seed in start..start + count {
            let module = generate_module(seed, GeneratorConfig::default());
            let before = render_module(&module);
            let input = ExecutionInput::main(
                format!("seed-{seed}"),
                entry_arguments(&module, seed),
                100_000,
            );
            // Generated EvoIR has no C declaration chain. Never manufacture
            // source permission from the absence of a barrier in raw IR.
            let report =
                inspect(&module, &BTreeMap::new(), &[input]).map_err(|e| format!("{seed}: {e}"))?;
            if before != render_module(&module) {
                return Err(format!("preflight mutated seed {seed}"));
            }
            inline.accumulate(&report.inline);
            edge_reuse.accumulate(&report.edge_reuse);
            *admissions.entry(report.module_admission).or_default() += 1;
            executed_inputs += report.executed_inputs;
            identity.push_str(&format!(
                "{seed}:{}:{}\n",
                sha256_hex(before.as_bytes()),
                sha256_hex(report.render_json().as_bytes())
            ));
        }
    }
    let admissions = admissions
        .iter()
        .map(|(name, count)| format!("\"{name}\":{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("{{\"schema\":\"evocc-opportunity-population-v1\",\"population\":\"frozen-generated-21000\",\"modules\":21000,\"transforms\":0,\"source_permission\":\"UNAVAILABLE_RAW_IR\",\"residual_B\":\"NOT_RUN_GENERATED_POPULATION\",\"module_admissions\":{{{admissions}}},\"executed_inputs\":{executed_inputs},\"inline\":{},\"edge_reuse\":{},\"observations_sha256\":\"{}\"}}", inline.render_json(), edge_reuse.render_json(), sha256_hex(identity.as_bytes()));
    Ok(())
}

fn preflight_source() -> Result<(), String> {
    use evocc_passes::preflight::inspect;
    let args: Vec<_> = std::env::args().skip(2).collect();
    if args.len() != 3 {
        return Err("usage: corpus preflight-source input.i original-A.evo current-B.evo".into());
    }
    let source = std::fs::read_to_string(&args[0]).map_err(|e| e.to_string())?;
    let (module, permissions) = source_inventory::lower_preprocessed(&source)?;
    let a_bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    if render_module(&module).as_bytes() != a_bytes {
        return Err(format!(
            "source inventory lowering differs from original driver A: {} != {}",
            sha256_hex(render_module(&module).as_bytes()),
            sha256_hex(&a_bytes)
        ));
    }
    let b_bytes = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    let b = evocc_evoir::parse_section9_module(&b_bytes).map_err(|e| e.to_string())?;
    // Only the existing no-ordinary-argument main consumer is authorized.
    // A library TU is not turned into a synthetic function-scoped TV case.
    let inputs = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .filter(|f| {
            f.params.iter().all(|p| {
                matches!(
                    p.ty,
                    evocc_types::EvoIRType::Mem(_) | evocc_types::EvoIRType::World
                )
            })
        })
        .map(|f| {
            vec![ExecutionInput::main(
                "original-main",
                f.params
                    .iter()
                    .map(|p| RuntimeValue::new(p.ty.clone(), 0))
                    .collect(),
                100_000,
            )]
        })
        .unwrap_or_default();
    let a_report = inspect(&module, &permissions, &inputs).map_err(|e| e.to_string())?;
    let b_report = inspect(&b, &permissions, &inputs).map_err(|e| e.to_string())?;
    if render_module(&module).as_bytes() != a_bytes || render_module(&b).as_bytes() != b_bytes {
        return Err("preflight modified original modules or B is noncanonical".into());
    }
    let barriers = permissions
        .values()
        .filter(|p| **p == evocc_passes::preflight::SourcePermission::Barrier)
        .count();
    println!("{{\"schema\":\"evocc-opportunity-source-v1\",\"transforms\":0,\"preprocessed_sha256\":\"{}\",\"A_sha256\":\"{}\",\"B_sha256\":\"{}\",\"source_permission_symbols\":{},\"source_barrier_symbols\":{barriers},\"source_inventory\":\"ORIGINAL_A_BYTES_MATCH\",\"A\":{},\"residual_B\":{}}}", sha256_hex(source.as_bytes()), sha256_hex(&a_bytes), sha256_hex(&b_bytes), permissions.len(), a_report.render_json(), b_report.render_json());
    Ok(())
}

fn native() -> Result<(), String> {
    let mut admitted = 0;
    let mut opaque = 0;
    let mut machine = 0;
    let mut native = 0;
    let mut skipped = std::collections::BTreeMap::new();
    // The existing native-main population, not a newly sampled seed space.
    for seed in 0x4E41_5400_0001..0x4E41_5400_0001 + 256 {
        let module = generate_module(seed, GeneratorConfig::native_main());
        let args = entry_arguments(&module, seed);
        let inputs = [ExecutionInput::main(
            format!("native-{seed}"),
            args.clone(),
            100_000,
        )];
        let output = match run_pipeline(&module, &[&IdentityPass], &inputs) {
            Ok(output) => output,
            Err(ValidationError::Ineligible(_)) => {
                opaque += 1;
                continue;
            }
            Err(error) => return Err(format!("{seed}: {error}")),
        };
        admitted += 1;
        let result = evocc_randgate::differential::validate_lowered_transformation(
            &module,
            &output.module,
            &args,
            100_000,
            Some(seed),
        )?;
        machine += u64::from(result.machine_compared);
        native += u64::from(result.native_run);
        if let Some(reason) = result.native_skipped {
            *skipped.entry(reason).or_insert(0) += 1;
        }
    }
    let skipped = skipped
        .iter()
        .map(|(reason, count)| format!("\"{reason}\":{count}"))
        .collect::<Vec<_>>()
        .join(",");
    println!("{{\"schema\":\"evocc-pass-native-v1\",\"programs\":256,\"identity_tv_admitted\":{admitted},\"identity_tv_ineligible_opaque\":{opaque},\"machine_compared\":{machine},\"native_compared\":{native},\"native_skipped\":{{{skipped}}},\"claim\":\"IDENTITY_PIPELINE_INTEGRATION_ONLY\"}}");
    if admitted == 0 || machine != admitted || native == 0 {
        return Err(
            "native identity integration did not complete its admitted execution coverage".into(),
        );
    }
    Ok(())
}

struct SourceBridge {
    status: &'static str,
    chir_events: usize,
    evoir_events: usize,
    chir_return: u128,
    evoir_return: u128,
    first_difference: Option<usize>,
}

impl SourceBridge {
    fn to_json(&self) -> String {
        let first_difference = self
            .first_difference
            .map_or_else(|| "null".to_string(), |index| index.to_string());
        format!(
            "{{\"input_id\":\"integer-mix\",\"chir_events\":{},\"evoir_events\":{},\"chir_return\":{},\"evoir_return\":{},\"first_difference\":{first_difference}}}",
            self.chir_events, self.evoir_events, self.chir_return, self.evoir_return,
        )
    }
}

fn integer_mix() -> Result<(Module, Vec<RuntimeValue>, SourceBridge), String> {
    let source = include_str!("../../../tests/fixtures/eqsat-performance/integer-mix.c");
    let arena = Arena::default();
    let file = FileId::new(60202);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() {
        return Err(format!("integer-mix parse: {:?}", parser.diagnostic()));
    }
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .ok_or("integer-mix has no main")?;
    let chir =
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(100_000))
            .map_err(|e| e.to_string())?;
    let module = evocc_lower::lower_unit_with_optimization(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        true,
    )
    .map_err(|e| e.to_string())?;
    let arguments = module
        .functions
        .iter()
        .find(|f| f.name == "main")
        .ok_or("lowering lost main")?
        .params
        .iter()
        .map(|p| RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let observation = measure_execution(
        &module,
        &ExecutionInput::main("integer-mix", arguments.clone(), 100_000),
    )
    .map_err(|e| e.to_string())?;
    let Observation::Defined(run) = observation.observation else {
        return Err("integer-mix not defined".into());
    };
    let chir_return = chir.return_value.ok_or("integer-mix no return")?.raw_bits;
    let evoir_return = run.returns[0].value();
    if chir_return != evoir_return {
        return Err(format!(
            "integer-mix C-HIR/EvoIR returns differ: {chir_return}/{evoir_return}"
        ));
    }
    let chir_trace = chir.world.volatile_trace();
    let first_difference = chir_trace
        .iter()
        .zip(&run.volatile)
        .position(|(left, right)| left != right)
        .or_else(|| {
            (chir_trace.len() != run.volatile.len())
                .then_some(chir_trace.len().min(run.volatile.len()))
        });
    // This is source-bridge evidence, not TV admission. In particular, the
    // observed local volatile initialization discrepancy must remain visible;
    // unchanged EvoIR measurement is not two-interpreter agreement.
    let bridge = SourceBridge {
        status: if first_difference.is_some() {
            "MISMATCH_NOT_VALIDATED"
        } else {
            "OBSERVATIONS_MATCH"
        },
        chir_events: chir_trace.len(),
        evoir_events: run.volatile.len(),
        chir_return,
        evoir_return,
        first_difference,
    };
    Ok((module, arguments, bridge))
}
