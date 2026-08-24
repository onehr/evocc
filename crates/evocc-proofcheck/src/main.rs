use std::fs;
use std::path::Path;
use std::process::ExitCode;

use evocc_proof::{parse_evp1, sha256_hex, EvpCertificate};

mod eligibility;
mod reassemble;
mod replay;

use eligibility::derive_eligible_regions;
use reassemble::reassemble;
use replay::{replay_region, RegionReplacement};

const REGISTRY_NAME: &str = "p0.scalar.v1";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";

fn main() -> ExitCode {
    let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
    if arguments.len() == 1 && arguments[0] == "--probe" {
        return ExitCode::SUCCESS;
    }
    if arguments.len() != 3 {
        return ExitCode::from(2);
    }

    let Ok(before) = read(&arguments[0]) else {
        return ExitCode::from(2);
    };
    let Ok(after) = read(&arguments[1]) else {
        return ExitCode::from(2);
    };
    let Ok(proof) = read(&arguments[2]) else {
        return ExitCode::from(2);
    };
    let before_module = match evocc_evoir::parse_section9_module(&before) {
        Ok(module) => module,
        Err(_) => return ExitCode::from(2),
    };
    if evocc_evoir::parse_section9_module(&after).is_err() {
        return ExitCode::from(2);
    }
    let certificate = match parse_evp1(&proof) {
        Ok(certificate) => certificate,
        Err(_) => return ExitCode::from(2),
    };

    if certificate.registry_name != REGISTRY_NAME
        || certificate.registry_digest != REGISTRY_SHA256
        || certificate.input_digest != sha256_hex(&before)
    {
        return ExitCode::from(3);
    }
    let eligible = derive_eligible_regions(&before_module);
    let reassembled_module = match claim(&certificate, &before_module, &eligible) {
        Ok(Claim::Zero) => before.clone(),
        Ok(Claim::Regions(replacements)) => match reassemble(&before_module, replacements) {
            Ok(module) => evocc_evoir::render_module(&module).into_bytes(),
            Err(()) => return ExitCode::from(3),
        },
        Err(()) => return ExitCode::from(3),
    };
    if certificate.output_digest != sha256_hex(&reassembled_module) || reassembled_module != after {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn read(path: &impl AsRef<Path>) -> Result<Vec<u8>, ()> {
    fs::read(path).map_err(|_| ())
}

enum Claim {
    Zero,
    Regions(Vec<RegionReplacement>),
}

fn claim(
    certificate: &EvpCertificate,
    module: &evocc_evoir::Module,
    eligible: &[eligibility::EligibleRegion],
) -> Result<Claim, ()> {
    if certificate.regions.is_empty() {
        return Ok(Claim::Zero);
    }
    let mut replacements = Vec::new();
    let mut previous = None;
    for region in &certificate.regions {
        let index = eligible
            .iter()
            .position(|candidate| candidate.id == region.id)
            .ok_or(())?;
        if previous.is_some_and(|previous| index <= previous) {
            return Err(());
        }
        previous = Some(index);
        replacements.push(replay_region(module, region, &eligible[index])?);
    }
    Ok(Claim::Regions(replacements))
}
