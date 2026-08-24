use std::fs;
use std::path::Path;
use std::process::ExitCode;

use evocc_proof::{
    parse_evp1, sha256_hex, EvpCertificate, EvpEffectClass, EvpEvent, EvpExtractionChoice,
    EvpNodeSource, EvpProofRef, EvpRegion, EvpSeedOrigin,
};

mod eligibility;

use eligibility::derive_eligible_regions;

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
    let reassembled = match claim(&certificate, &eligible) {
        Ok(Claim::Zero) => before.clone(),
        Ok(Claim::AddZeroRhs(claim)) => {
            match reassemble_reviewed_add_zero(&before, &before_module, claim) {
                Some(module) => module,
                None => return ExitCode::from(3),
            }
        }
        Err(()) => return ExitCode::from(3),
    };
    if certificate.output_digest != sha256_hex(&reassembled) || reassembled != after {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn read(path: &impl AsRef<Path>) -> Result<Vec<u8>, ()> {
    fs::read(path).map_err(|_| ())
}

enum Claim {
    Zero,
    AddZeroRhs(AddZeroRhs),
}

struct AddZeroRhs {
    function: String,
    block: String,
    root_index: usize,
    root: u32,
    seed: u32,
}

fn claim(
    certificate: &EvpCertificate,
    eligible: &[eligibility::EligibleRegion],
) -> Result<Claim, ()> {
    match certificate.regions.as_slice() {
        [] => Ok(Claim::Zero),
        [region] => {
            let derived = eligible
                .iter()
                .find(|candidate| candidate.id == region.id)
                .ok_or(())?;
            if derived.root_value != region.root_value
                || derived.witnesses != region.witnesses
                || derived.seeds != region.seeds
            {
                return Err(());
            }
            Ok(Claim::AddZeroRhs(parse_add_zero_claim(region)?))
        }
        _ => Err(()),
    }
}

fn parse_add_zero_claim(region: &EvpRegion) -> Result<AddZeroRhs, ()> {
    let function = &region.id.function;
    let block = &region.id.block;
    let root_index = usize::try_from(region.id.instruction).map_err(|_| ())?;
    if region.witnesses.len() != 2
        || region.witnesses[0].function != *function
        || region.witnesses[0].block != *block
        || region.witnesses[0].instruction != 0
        || region.witnesses[1] != region.id
        || region.seeds.len() != 1
        || region.seeds[0].sort != "bits<32>"
        || region.seeds[0].origin != EvpSeedOrigin::Input
        || region.seeds[0].class != 0
        || region.events.len() != 3
        || region.extractions.len() != 1
        || region.extractions[0].class != 0
        || region.extractions[0].choice != EvpExtractionChoice::Seed(0)
        || region.root_class != 0
    {
        return Err(());
    }
    let [EvpEvent::Node(constant), EvpEvent::Node(add), EvpEvent::Rewrite(rewrite)] =
        region.events.as_slice()
    else {
        return Err(());
    };
    if constant.node != 0
        || constant.owner != 1
        || constant.source != EvpNodeSource::Before
        || constant.operation != "const"
        || constant.sort != "bits<32>"
        || constant.effect != EvpEffectClass::Pure
        || constant.attributes != [("value".to_owned(), 0)]
        || !constant.children.is_empty()
        || add.node != 1
        || add.owner != 2
        || add.source != EvpNodeSource::Before
        || add.operation != "add"
        || add.sort != "bits<32>"
        || add.effect != EvpEffectClass::Pure
        || !add.attributes.is_empty()
        || add.children != [0, 1]
        || rewrite.rule != "p0_001_add_zero_rhs"
        || rewrite.lhs_pre != 2
        || rewrite.rhs_pre != 0
        || rewrite.result != 0
        || rewrite.lhs != EvpProofRef::Node(1)
        || rewrite.rhs != EvpProofRef::Class(0)
        || rewrite.bindings != [("x".to_owned(), 0)]
    {
        return Err(());
    }
    Ok(AddZeroRhs {
        function: function.clone(),
        block: block.clone(),
        root_index,
        root: region.root_value,
        seed: region.seeds[0].value,
    })
}

fn reassemble_reviewed_add_zero(
    before: &[u8],
    module: &evocc_evoir::Module,
    claim: AddZeroRhs,
) -> Option<Vec<u8>> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == claim.function)?;
    let block = if claim.block == "entry" {
        function.blocks.first()?
    } else {
        let id = claim.block.strip_prefix('b')?.parse().ok()?;
        function.blocks.iter().find(|block| block.id == id)?
    };
    let evocc_evoir::Inst::Add(result, left, right) = block.insts.get(claim.root_index)? else {
        return None;
    };
    if result.id != claim.root
        || left.id != claim.seed
        || result.ty != evocc_evoir::EvoIRType::Bits(32)
    {
        return None;
    }
    let const_index = claim.root_index.checked_sub(1)?;
    let evocc_evoir::Inst::Const(constant, immediate) = block.insts.get(const_index)? else {
        return None;
    };
    if *immediate != 0 || constant.id != right.id || constant.ty != result.ty {
        return None;
    }
    if !block
        .args
        .iter()
        .any(|argument| argument.value.id == claim.seed && argument.value.ty == result.ty)
    {
        return None;
    }

    let text = std::str::from_utf8(before).ok()?;
    let mut lines = text.lines().map(str::to_owned).collect::<Vec<_>>();
    let function_line = lines
        .iter()
        .position(|line| line.starts_with(&format!("func @{}(", claim.function)))?;
    let block_prefix = if claim.block == "entry" {
        "^entry(".to_owned()
    } else {
        format!("^{}(", claim.block)
    };
    let block_line = lines
        .iter()
        .enumerate()
        .skip(function_line + 3)
        .take_while(|(_, line)| line.as_str() != "}")
        .find_map(|(index, line)| line.starts_with(&block_prefix).then_some(index))?;
    let root_line = block_line.checked_add(1 + claim.root_index)?;
    if !lines
        .get(root_line)?
        .contains(&format!("%v{} = add.", claim.root))
    {
        return None;
    }
    lines.remove(root_line);
    for line in lines
        .iter_mut()
        .skip(root_line)
        .take_while(|line| !line.is_empty() && line.as_str() != "}" && !line.starts_with('^'))
    {
        *line = replace_value_token(line, claim.root, claim.seed);
    }
    let reassembled = format!("{}\n", lines.join("\n")).into_bytes();
    evocc_evoir::parse_section9_module(&reassembled).ok()?;
    Some(reassembled)
}

fn replace_value_token(line: &str, from: u32, to: u32) -> String {
    let needle = format!("%v{from}");
    let replacement = format!("%v{to}");
    let mut output = String::with_capacity(line.len());
    let mut remaining = line;
    while let Some(index) = remaining.find(&needle) {
        output.push_str(&remaining[..index]);
        let suffix = &remaining[index + needle.len()..];
        if suffix.as_bytes().first().is_some_and(u8::is_ascii_digit) {
            output.push_str(&needle);
        } else {
            output.push_str(&replacement);
        }
        remaining = suffix;
    }
    output.push_str(remaining);
    output
}
