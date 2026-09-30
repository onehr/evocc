//! Read-only P0 census and exact-IR policy experiment adapter.
//!
//! Census rows are producer diagnostics, not independently proved eligibility.
//! Reuse the producer fixed point; never feed this report to the checker.
#[allow(dead_code)]
#[path = "../src/eligibility.rs"]
mod eligibility;

use std::io::Write;
use std::process::ExitCode;

use evocc_eqopt::{
    run_scalar_chain_with_checker, CostFunction, EqualityState, ExtractionPolicy, OperatorWeights,
    SchedulePolicy,
};
use evocc_evoir::{inst_results, inst_uses, CmpOp, EvoIRType, Inst, Module};

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opcode(i: &Inst) -> &'static str {
    match i {
        Inst::Const(..) => "const",
        Inst::Add(..) => "add",
        Inst::Sub(..) => "sub",
        Inst::Mul(..) => "mul",
        Inst::And(..) => "and",
        Inst::Or(..) => "or",
        Inst::Xor(..) => "xor",
        Inst::Shl(..) => "shl",
        Inst::LShr(..) => "lshr",
        Inst::AShr(..) => "ashr",
        Inst::ZExt(..) => "zext",
        Inst::SExt(..) => "sext",
        Inst::Trunc(..) => "trunc",
        Inst::ICmp(_, op, ..) => match op {
            CmpOp::Eq => "icmp.eq",
            CmpOp::Ne => "icmp.ne",
            CmpOp::SLt => "icmp.slt",
            CmpOp::SLe => "icmp.sle",
            CmpOp::ULt => "icmp.ult",
            CmpOp::ULe => "icmp.ule",
        },
        _ => "unlisted",
    }
}

fn census(module: &Module, digest: &str) -> String {
    let mut rows = Vec::new();
    let mut excluded = [0_usize; 3];
    for (f, function) in module.functions.iter().enumerate() {
        let admission = eligibility::Admission::of(function);
        for block in &function.blocks {
            for instruction in &block.insts {
                let results = inst_results(instruction);
                let [result] = results.as_slice() else {
                    continue;
                };
                if !matches!(result.ty, EvoIRType::Bits(1..=64)) {
                    continue;
                }
                if admission.is_tainted(result.id) {
                    excluded[1] += 1;
                    continue;
                }
                if !eligibility::allowlisted(instruction) {
                    excluded[0] += 1;
                    continue;
                }
                if !admission.admits(result.id) {
                    excluded[2] += 1;
                    continue;
                }
                let EvoIRType::Bits(width) = result.ty else {
                    unreachable!()
                };
                let operands = inst_uses(instruction)
                    .into_iter()
                    .map(|v| {
                        let EvoIRType::Bits(w) = v.ty else {
                            unreachable!()
                        };
                        format!("{{\"id\":{},\"width\":{w}}}", v.id)
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                let constant = match instruction {
                    Inst::Const(_, bits) => format!("\"{bits:016x}\""),
                    _ => "null".into(),
                };
                rows.push(format!("{{\"function\":{f},\"name\":{},\"block\":{},\"root\":{},\"width\":{width},\"op\":{},\"operands\":[{operands}],\"constant\":{constant}}}", quote(&function.name),block.id,result.id,quote(opcode(instruction))));
            }
        }
    }
    format!("{{\"schema\":\"evocc-opportunity-census-v1\",\"authority\":\"PRODUCER_DIAGNOSTIC_NOT_PROOF\",\"input_digest\":\"{digest}\",\"eligible\":{},\"excluded\":{{\"unlisted\":{},\"taint\":{},\"dependency\":{}}},\"roots\":[{}]}}\n", rows.len(),excluded[0],excluded[1],excluded[2],rows.join(","))
}

fn run(args: &[String]) -> Result<String, String> {
    if args.len() != 3 && args.len() != 8 {
        return Err("usage: opportunity_census census SHA256 INPUT.evo | policy SHA256 INPUT.evo CHECKER CHECKER_SHA256 WEIGHTS TRANSACTIONS NEW_BUNDLE".into());
    }
    let bytes = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    if evocc_proof::sha256_hex(&bytes) != args[1] {
        return Err("input digest mismatch".into());
    }
    let module = evocc_evoir::parse_section9_module(&bytes).map_err(|e| e.to_string())?;
    evocc_evoir::validate_module(&module).map_err(|e| e.to_string())?;
    if evocc_evoir::render_module(&module).as_bytes() != bytes {
        return Err("noncanonical input".into());
    }
    if args[0] == "census" && args.len() == 3 {
        return Ok(census(&module, &args[1]));
    }
    if args[0] != "policy" || args.len() != 8 {
        return Err("unknown adapter mode".into());
    }
    let max_transactions = args[6].parse::<usize>().map_err(|e| e.to_string())?;
    if !(1..=8).contains(&max_transactions) {
        return Err("transaction count outside frozen native preflight bound".into());
    }
    let policy = ExtractionPolicy::default()
        .with_cost(CostFunction::Vector(OperatorWeights::parse(&args[5])?))
        .with_schedule(SchedulePolicy {
            max_transactions,
            ..SchedulePolicy::default()
        });
    let checker =
        evocc_proof::StandaloneProofChecker::new(args[3].clone().into(), std::env::temp_dir())
            .with_timeout(std::time::Duration::from_secs(300))
            .with_expected_digest(args[4].clone());
    let mut state = EqualityState::for_module(&module)?;
    match run_scalar_chain_with_checker(&module, &mut state, &policy, &checker) {
        Ok(run) => {
            run.write_bundle(std::path::Path::new(&args[7]))?;
            Ok(evocc_evoir::render_module(&run.module))
        }
        Err(error) => {
            let original = error.detail.clone();
            error
                .write_bundle(std::path::Path::new(&args[7]))
                .map_err(|e| format!("{original}; additionally {e}"))?;
            Err(original)
        }
    }
}

fn main() -> ExitCode {
    match run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(output) => match std::io::stdout().lock().write_all(output.as_bytes()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escaped_names_are_json_not_rust_debug_strings() {
        assert_eq!(quote("a\n\"\\\0"), "\"a\\u000a\\\"\\\\\\u0000\"");
    }

    #[test]
    fn bad_hash_never_reaches_the_producer() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/eqsat-chains/verified-chain/000.before.evo"
        );
        assert_eq!(
            run(&["census".into(), "0".repeat(64), path.into()]).unwrap_err(),
            "input digest mismatch"
        );
    }

    #[test]
    fn inventory_matches_the_existing_sealed_scalar_fixture() {
        let bytes =
            include_bytes!("../../../tests/fixtures/eqsat-chains/verified-chain/000.before.evo");
        let module = evocc_evoir::parse_section9_module(bytes).unwrap();
        let output = census(&module, &evocc_proof::sha256_hex(bytes));
        assert!(output.contains("\"eligible\":6"));
        assert!(output.contains("\"dependency\":2"));
    }
}
