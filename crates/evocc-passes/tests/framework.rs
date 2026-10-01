use evocc_evoir::{parse_section9_module, render_module, Inst, Module, Value};
use evocc_passes::{
    run_pipeline, ExecutionInput, IdentityPass, ModulePass, Opportunities, PassClass, PassOutput,
    ValidationError,
};
use evocc_randgate::{entry_arguments, generate_module, GeneratorConfig};
use evocc_types::EvoIRType;

fn simple() -> Module {
    parse_section9_module(b"func @main() -> (bits<32>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    %v0 = const.bits<32> 7\n    ret %v0\n}\n").unwrap()
}

struct WrongReturn;
impl ModulePass for WrongReturn {
    fn name(&self) -> &'static str {
        "wrong-return"
    }
    fn version(&self) -> u32 {
        1
    }
    fn apply(&self, module: &Module) -> Result<PassOutput, String> {
        let mut module = module.clone();
        let mut sites = 0;
        for function in &mut module.functions {
            if function.name != "main" {
                continue;
            }
            for block in &mut function.blocks {
                if let Inst::Ret(values) = &mut block.terminator {
                    if let Some(value) = values
                        .iter_mut()
                        .find(|v| matches!(v.ty, EvoIRType::Bits(_)))
                    {
                        let id = block
                            .insts
                            .iter()
                            .flat_map(evocc_evoir::inst_results)
                            .chain(block.args.iter().map(|a| &a.value))
                            .map(|v| v.id)
                            .max()
                            .unwrap_or(0)
                            + 1;
                        let one = Value {
                            id,
                            ty: value.ty.clone(),
                        };
                        let new = Value {
                            id: id + 1,
                            ty: value.ty.clone(),
                        };
                        block.insts.push(Inst::Const(one.clone(), 1));
                        block.insts.push(Inst::Xor(new.clone(), value.clone(), one));
                        *value = new;
                        sites += 1;
                    }
                }
            }
        }
        Ok(PassOutput {
            module,
            opportunities: Opportunities {
                eligible_sites: sites,
                taken_sites: sites,
                ..Opportunities::default()
            },
        })
    }
}

#[test]
fn empty_pipeline_preserves_exact_module_and_does_not_execute() {
    let module = simple();
    let output = run_pipeline(&module, &[], &[]).unwrap();
    assert_eq!(render_module(&output.module), render_module(&module));
    assert!(output.report.passes.is_empty());
}

#[test]
fn wrong_pass_is_rejected_without_mutating_the_input() {
    let module = simple();
    let before = render_module(&module);
    assert!(matches!(
        run_pipeline(
            &module,
            &[&WrongReturn],
            &[ExecutionInput::main("seven", vec![], 100)]
        ),
        Err(ValidationError::Mismatch {
            observation: "returns",
            ..
        })
    ));
    assert_eq!(before, render_module(&module));
}

#[test]
fn each_intermediate_pass_is_validated() {
    let module = simple();
    // Applying bitwise-not twice repairs the result, but cannot repair a rejected first pass.
    assert!(matches!(
        run_pipeline(
            &module,
            &[&WrongReturn, &WrongReturn],
            &[ExecutionInput::main("seven", vec![], 100)]
        ),
        Err(ValidationError::Mismatch { .. })
    ));
}

#[test]
fn identity_reports_input_hashes_steps_and_validated_not_verified() {
    let output = run_pipeline(
        &simple(),
        &[&IdentityPass],
        &[ExecutionInput::main("seven", vec![], 100)],
    )
    .unwrap();
    let row = &output.report.passes[0];
    assert_eq!(row.class, PassClass::Validated);
    assert_eq!(row.before_ir_sha256, row.after_ir_sha256);
    let execution = &row.executions[0];
    assert_eq!(execution.before_steps, execution.after_steps);
    assert!(execution.before_steps.unwrap() > 0);
    assert_eq!(execution.input_sha256.len(), 64);
    assert_eq!(
        execution.before_outcome_sha256,
        execution.after_outcome_sha256
    );
    let json = output.report.render_json();
    assert!(json.contains("\"actual_bytes\":null"));
    assert!(json.contains("\"class\":\"VALIDATED\""));
}

struct InvalidTelemetry;
impl ModulePass for InvalidTelemetry {
    fn name(&self) -> &'static str {
        "bad-counter"
    }
    fn version(&self) -> u32 {
        1
    }
    fn apply(&self, module: &Module) -> Result<PassOutput, String> {
        Ok(PassOutput {
            module: module.clone(),
            opportunities: Opportunities {
                eligible_sites: 1,
                ..Opportunities::default()
            },
        })
    }
}

#[test]
fn invalid_telemetry_is_rejected() {
    assert!(matches!(
        run_pipeline(
            &simple(),
            &[&InvalidTelemetry],
            &[ExecutionInput::main("seven", vec![], 100)]
        ),
        Err(ValidationError::InvalidTelemetry(_))
    ));
}

#[test]
fn no_inputs_and_exhausted_fuel_are_not_passes() {
    assert_eq!(
        run_pipeline(&simple(), &[&IdentityPass], &[]).unwrap_err(),
        ValidationError::NoInputs
    );
    assert!(matches!(
        run_pipeline(
            &simple(),
            &[&IdentityPass],
            &[ExecutionInput::main("fuel", vec![], 0)]
        ),
        Err(ValidationError::Ineligible(_))
    ));
}

#[test]
fn generated_red_corpus_detects_wrong_return() {
    let mut detected = 0;
    for seed in 0..256 {
        let module = generate_module(seed, GeneratorConfig::default());
        let arguments = entry_arguments(&module, seed);
        let cases = [ExecutionInput::main(
            format!("seed-{seed}"),
            arguments,
            100_000,
        )];
        match run_pipeline(&module, &[&IdentityPass], &cases) {
            Ok(_) => {
                assert!(
                    matches!(
                        run_pipeline(&module, &[&WrongReturn], &cases),
                        Err(ValidationError::Mismatch {
                            observation: "returns",
                            ..
                        })
                    ),
                    "seed {seed}"
                );
                detected += 1;
            }
            Err(ValidationError::Ineligible(_)) => {}
            other => panic!("seed {seed}: {other:?}"),
        }
    }
    assert!(
        detected > 0,
        "corpus must actually execute wrong-pass injections"
    );
}

struct Mutate(fn(&mut Module));
impl ModulePass for Mutate {
    fn name(&self) -> &'static str {
        "mutation"
    }
    fn version(&self) -> u32 {
        1
    }
    fn apply(&self, module: &Module) -> Result<PassOutput, String> {
        let mut module = module.clone();
        (self.0)(&mut module);
        module.recompute_effect_summaries();
        Ok(PassOutput {
            module,
            opportunities: Opportunities {
                eligible_sites: 1,
                taken_sites: 1,
                ..Opportunities::default()
            },
        })
    }
}

#[test]
fn introduced_diagnostic_and_malformed_output_are_not_validated() {
    let poison = Mutate(|module| {
        let Inst::Const(value, _) = &module.functions[0].blocks[0].insts[0] else {
            panic!()
        };
        module.functions[0].blocks[0].insts[0] = Inst::Uninitialized(value.clone());
    });
    let inputs = [ExecutionInput::main("seven", vec![], 100)];
    assert!(matches!(
        run_pipeline(&simple(), &[&poison], &inputs),
        Err(ValidationError::Mismatch {
            observation: "diagnostics",
            ..
        })
    ));
    let malformed = Mutate(|module| {
        module.functions[0].blocks[0].insts.clear();
    });
    assert!(matches!(
        run_pipeline(&simple(), &[&malformed], &inputs),
        Err(ValidationError::InvalidOutput(_))
    ));
}

#[test]
fn surviving_memory_changes_are_observable_even_with_unchanged_return() {
    let text = format!("global @state origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\n\n{}", render_module(&simple()));
    let module = parse_section9_module(text.as_bytes()).unwrap();
    let change_memory = Mutate(|module| module.globals[0].bytes[0] = 8);
    assert!(matches!(
        run_pipeline(
            &module,
            &[&change_memory],
            &[ExecutionInput::main("memory", vec![], 100)]
        ),
        Err(ValidationError::Mismatch {
            observation: "static-memory",
            ..
        })
    ));
}

#[test]
fn opaque_module_and_effect_forgery_are_rejected() {
    let mut opaque = simple();
    opaque.assembly.push("nop".into());
    assert!(matches!(
        run_pipeline(
            &opaque,
            &[&IdentityPass],
            &[ExecutionInput::main("asm", vec![], 100)]
        ),
        Err(ValidationError::Ineligible(_))
    ));
    let mut forged = simple();
    forged.functions[0].effects.volatile = true;
    assert!(matches!(
        run_pipeline(
            &forged,
            &[&IdentityPass],
            &[ExecutionInput::main("forged", vec![], 100)]
        ),
        Err(ValidationError::InvalidInput(_))
    ));
}

#[test]
fn repeated_diagnostic_is_separate_from_defined_execution() {
    let mut poison = simple();
    let Inst::Const(value, _) = &poison.functions[0].blocks[0].insts[0] else {
        panic!()
    };
    poison.functions[0].blocks[0].insts[0] = Inst::Uninitialized(value.clone());
    let output = run_pipeline(
        &poison,
        &[&IdentityPass],
        &[ExecutionInput::main("poison", vec![], 100)],
    )
    .unwrap();
    let case = &output.report.passes[0].executions[0];
    assert_eq!(case.outcome, "DIAGNOSTIC_MATCH:E3001");
    assert_eq!(case.before_steps, None);
    assert_eq!(case.after_steps, None);
}

#[test]
fn duplicate_input_labels_are_not_coverage() {
    let input = ExecutionInput::main("same", vec![], 100);
    assert!(matches!(
        run_pipeline(&simple(), &[&IdentityPass], &[input.clone(), input]),
        Err(ValidationError::InvalidInput(_))
    ));
}

#[test]
fn unsupported_diagnostic_cannot_be_counted_as_a_match() {
    let mut module = simple();
    module.functions[0].blocks[0].insts.insert(
        0,
        Inst::FrameAddress {
            result: Value {
                id: 1,
                ty: EvoIRType::Ptr(evocc_types::AddressSpace::Generic),
            },
            depth: 1,
        },
    );
    module.recompute_effect_summaries();
    assert!(
        matches!(run_pipeline(&module, &[&IdentityPass], &[ExecutionInput::main("unsupported-frame", vec![], 100)]), Err(ValidationError::Ineligible(message)) if message.contains("E3031"))
    );
}

#[test]
fn existing_machine_lane_accepts_identity_and_rejects_changed_observations() {
    use evocc_randgate::differential::validate_lowered_transformation;
    let module = simple();
    let input = ExecutionInput::main("seven", vec![], 100);
    let output = run_pipeline(&module, &[&IdentityPass], &[input]).unwrap();
    let report = validate_lowered_transformation(&module, &output.module, &[], 100, None).unwrap();
    assert!(report.machine_compared);
    assert!(!report.native_run);
    assert_eq!(report.native_skipped, Some("notTheNativeCorpus"));
    let wrong = WrongReturn.apply(&module).unwrap().module;
    assert!(
        validate_lowered_transformation(&module, &wrong, &[], 100, None)
            .unwrap_err()
            .contains("ReturnValues")
    );
}
