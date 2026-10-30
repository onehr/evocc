//! Size-only adapter. Never produces an installable or PASS-eligible artifact.
use std::collections::BTreeMap;
use std::io::Write;
use std::process::ExitCode;

fn emit(args: &[String]) -> Result<String, String> {
    if args.len() != 2 {
        return Err(
            "usage: evo-guard-size SHA256 INPUT.evo (UNVERIFIED_DIAGNOSTIC, object-size only)"
                .into(),
        );
    }
    let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    if evocc_proof::sha256_hex(&bytes) != args[0] {
        return Err("guard-attribution input digest mismatch".into());
    }
    let module = evocc_evoir::parse_section9_module(&bytes).map_err(|e| e.to_string())?;
    evocc_evoir::validate_module(&module).map_err(|e| e.to_string())?;
    if evocc_evoir::render_module(&module).as_bytes() != bytes {
        return Err("guard-attribution input is not canonical EvoIR".into());
    }
    let keep = evocc_mir::emitted_functions(&module);
    let mut claims = BTreeMap::new();
    let (mut predicates, mut assumptions, mut folded, mut removed) = (0, 0, 0, 0);
    let mut kinds: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for f in module.functions.iter().filter(|f| keep.contains(&f.name)) {
        let facts = evocc_eqopt::guard_size_attribution(f);
        predicates += facts.total_predicates;
        assumptions += facts.total_assumptions;
        folded += facts.true_predicates.len();
        removed += facts.removable_assumptions;
        for (kind, count) in &facts.predicates_by_kind {
            let entry = kinds.entry(kind).or_default();
            entry.0 += count;
            entry.1 += facts.true_by_kind.get(kind).copied().unwrap_or(0);
        }
        claims.insert(f.name.clone(), facts.true_predicates);
    }
    let assembly = evocc_mir::native::compile_module_for_guard_attribution(&module, &claims)
        .map_err(|e| format!("{e:?}"))?;
    let kind_json = kinds
        .iter()
        .map(|(name, (total, truth))| {
            format!("\"{name}\":{{\"total\":{total},\"folded\":{truth}}}")
        })
        .collect::<Vec<_>>()
        .join(",");
    eprintln!("{{\"schema\":\"evocc-guard-size-attribution-v1\",\"status\":\"UNVERIFIED_DIAGNOSTIC\",\"installable\":false,\"pass_eligible\":false,\"predicates\":{predicates},\"folded_predicates\":{folded},\"assumptions\":{assumptions},\"removed_assumptions\":{removed},\"by_kind\":{{{kind_json}}}}}");
    Ok(assembly)
}

fn main() -> ExitCode {
    match emit(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(s) => match std::io::stdout().lock().write_all(s.as_bytes()) {
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
    #[test]
    fn attribution_artifact_keeps_non_installable_marker_and_unproved_trap() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/eqsat-size/guard-attribution.evo");
        let bytes = std::fs::read(&path).unwrap();
        let assembly = super::emit(&[
            evocc_proof::sha256_hex(&bytes),
            path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(assembly.contains("UNVERIFIED_DIAGNOSTIC_NON_INSTALLABLE"));
        assert!(assembly.contains(".note.evocc.unverified"));
        assert!(assembly.contains("brk #3004"));
    }
}
