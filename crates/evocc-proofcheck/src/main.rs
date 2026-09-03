use std::fs;
use std::path::Path;
use std::process::ExitCode;

use evocc_proof::{parse_evp1, sha256_hex, EvpCertificate, EvpEvent, EvpStop, EvpVersion};

mod eligibility;
mod facts;
mod reassemble;
mod replay;

use eligibility::derive_eligible_regions;
use reassemble::reassemble;
use replay::{replay_region, RegionReplacement, Registry};

const REGISTRY_V1: &str = "p0.scalar.v1";
/// The revision that adds the Tier B rules. Section 11.6 "Registries and
/// revisions": a revision includes every rule of the registry it revises,
/// unchanged and in order, so a rule id keeps its meaning across revisions.
const REGISTRY_V2: &str = "p0.scalar.v2";
const REGISTRY_V2_SHA256: &str = "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce";
/// The revision that admits the shifts and width changes.
const REGISTRY_V3: &str = "p0.scalar.v3";
const REGISTRY_V3_SHA256: &str = "aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15";
/// Section 8.11: the P0 profile's ceilings. A budget line that records anything
/// else is not describing this profile's scheduler.
const MAX_NODES: u64 = 100_000;
const MAX_EPOCHS: u64 = 1_024;
/// The floor of the per-rule fuel bound, `max(256, 8 * n)`.
const FUEL_FLOOR: u64 = 256;
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

    let Some(registry) = admitted_registry(&certificate) else {
        return ExitCode::from(3);
    };
    if certificate.input_digest != sha256_hex(&before) || !scheduler_record_holds(&certificate) {
        return ExitCode::from(3);
    }
    let eligible = derive_eligible_regions(&before_module);
    let reassembled_module = match claim(&certificate, &before_module, &eligible, registry) {
        // Section 11.3: a certificate with no region claims that after.evo is
        // the canonical rendering of the module parsed from before.evo, and
        // nothing else. Comparing against the raw input bytes instead would
        // refuse every gapped or otherwise non-canonical input the producer had
        // merely rendered.
        Ok(Claim::Zero) => evocc_evoir::render_module(&before_module).into_bytes(),
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

/// Section 11.3 "Budget": what a reader can check about a schedule without
/// re-running it.
///
/// The scheduler is not replayed. What is verified is that the box the producer
/// declared is this profile's box, that it stayed inside it, that the fuel bound
/// is the one its declared node count implies, and that an exhaustion it claims
/// is an exhaustion the numbers show. `saturated` and `no-new-best` are not
/// verified: deciding either would mean running the scheduler again, so they
/// are telemetry and are read but not judged.
fn scheduler_record_holds(certificate: &EvpCertificate) -> bool {
    let Some(budget) = certificate.budget else {
        // Only `evp2` carries a record, and the parser has already refused an
        // `evp2` without one and an `evp1` with one.
        return certificate.version == EvpVersion::Evp1;
    };
    if budget.nodes.bound != MAX_NODES || budget.epochs.bound != MAX_EPOCHS {
        return false;
    }
    if budget.nodes.consumed > budget.nodes.bound
        || budget.epochs.consumed > budget.epochs.bound
        || budget.fuel.consumed > budget.fuel.bound
    {
        return false;
    }
    if budget.fuel.bound != FUEL_FLOOR.max(budget.n.saturating_mul(8)) {
        return false;
    }
    let marker_beyond_the_epochs_that_ran = certificate
        .regions
        .iter()
        .flat_map(|region| &region.events)
        .any(|event| matches!(event, EvpEvent::Epoch(epoch) if *epoch > budget.epochs.consumed));
    if marker_beyond_the_epochs_that_ran {
        return false;
    }
    match budget.stop {
        EvpStop::Nodes => budget.nodes.consumed == budget.nodes.bound,
        EvpStop::Epochs => budget.epochs.consumed == budget.epochs.bound,
        EvpStop::Fuel => budget.fuel.consumed == budget.fuel.bound,
        EvpStop::Saturated | EvpStop::NoNewBest | EvpStop::Policy => true,
    }
}

/// Section 11.6: `evp1` accepts `p0.scalar.v1` only; `evp2` accepts
/// `p0.scalar.v1` and `p0.scalar.v2`.
///
/// The digest is checked with the name, because a certificate that named a
/// registry this checker knows while carrying another's digest would be
/// claiming rules neither side agreed on.
fn admitted_registry(certificate: &EvpCertificate) -> Option<Registry> {
    let named = (
        certificate.registry_name.as_str(),
        certificate.registry_digest.as_str(),
    );
    match (certificate.version, named) {
        (_, (REGISTRY_V1, REGISTRY_SHA256)) => Some(Registry::Reviewed),
        (EvpVersion::Evp2, (REGISTRY_V2, REGISTRY_V2_SHA256)) => Some(Registry::Revised),
        (EvpVersion::Evp2, (REGISTRY_V3, REGISTRY_V3_SHA256)) => Some(Registry::Widened),
        _ => None,
    }
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
    registry: Registry,
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
        replacements.push(replay_region(module, region, &eligible[index], registry)?);
    }
    Ok(Claim::Regions(replacements))
}
