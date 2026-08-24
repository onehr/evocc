use std::fs;
use std::path::Path;
use std::process::ExitCode;

use evocc_proof::sha256_hex;

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
    let certificate = match Certificate::parse(&proof) {
        Ok(certificate) => certificate,
        Err(ParseOutcome::Malformed) => return ExitCode::from(2),
        Err(ParseOutcome::UnsupportedRegion) => return ExitCode::from(3),
    };

    if certificate.input != sha256_hex(&before) {
        return ExitCode::from(3);
    }
    let reassembled = match certificate.claim {
        Claim::Zero => before.clone(),
        Claim::AddZeroRhs(claim) => {
            match reassemble_reviewed_add_zero(&before, &before_module, claim) {
                Some(module) => module,
                None => return ExitCode::from(3),
            }
        }
    };
    if certificate.output != sha256_hex(&reassembled) || reassembled != after {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn read(path: &impl AsRef<Path>) -> Result<Vec<u8>, ()> {
    fs::read(path).map_err(|_| ())
}

struct Certificate<'a> {
    input: &'a str,
    output: &'a str,
    claim: Claim,
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

enum ParseOutcome {
    Malformed,
    UnsupportedRegion,
}

impl<'a> Certificate<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, ParseOutcome> {
        let text = std::str::from_utf8(bytes).map_err(|_| ParseOutcome::Malformed)?;
        if !text.ends_with('\n') || text.ends_with("\n\n") || text.contains('\r') {
            return Err(ParseOutcome::Malformed);
        }
        let lines = text.lines().collect::<Vec<_>>();
        if lines.first() != Some(&"evp1") {
            return Err(ParseOutcome::Malformed);
        }
        if lines.len() != 4 && lines.len() != 14 {
            return Err(ParseOutcome::Malformed);
        }
        let registry = lines[1]
            .strip_prefix("registry ")
            .and_then(|line| line.split_once(' '))
            .ok_or(ParseOutcome::Malformed)?;
        if registry != (REGISTRY_NAME, REGISTRY_SHA256) {
            return Err(ParseOutcome::UnsupportedRegion);
        }
        let input = lines[2]
            .strip_prefix("input ")
            .ok_or(ParseOutcome::Malformed)?;
        let output_index = lines.len() - 1;
        let output = lines[output_index]
            .strip_prefix("output ")
            .ok_or(ParseOutcome::Malformed)?;
        if !is_digest(input) || !is_digest(output) {
            return Err(ParseOutcome::Malformed);
        }
        let claim = if lines.len() == 4 {
            Claim::Zero
        } else {
            Claim::AddZeroRhs(parse_add_zero_claim(&lines[3..13])?)
        };
        Ok(Self {
            input,
            output,
            claim,
        })
    }
}

fn parse_add_zero_claim(lines: &[&str]) -> Result<AddZeroRhs, ParseOutcome> {
    if lines.len() != 10 {
        return Err(ParseOutcome::Malformed);
    }
    let region = lines[0].split(' ').collect::<Vec<_>>();
    if region.len() != 8
        || region[0] != "region"
        || region[2] != "root"
        || region[4..] != ["witnesses", "2", "seeds", "1"]
    {
        return Err(ParseOutcome::Malformed);
    }
    let (function, block, root_index) = parse_region_identity(region[1])?;
    let root = parse_value_id(region[3])?;
    if lines[1] != format!("witness 0 {function}/{block}/0")
        || lines[2] != format!("witness 1 {function}/{block}/{root_index}")
    {
        return Err(ParseOutcome::UnsupportedRegion);
    }
    let seed = lines[3].split(' ').collect::<Vec<_>>();
    if seed.len() != 6
        || seed[0] != "seed"
        || seed[1] != "0"
        || seed[3] != "bits<32>"
        || seed[4] != "input"
        || seed[5] != "c0"
    {
        return Err(ParseOutcome::UnsupportedRegion);
    }
    let seed = parse_value_id(seed[2])?;
    if lines[4]
        != "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-"
        || lines[5]
            != "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1"
        || lines[6] != "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0"
        || lines[7] != "extract 0 c0 seed:0"
        || lines[8] != "root c0"
        || lines[9] != "endregion"
    {
        return Err(ParseOutcome::UnsupportedRegion);
    }
    Ok(AddZeroRhs {
        function: function.to_owned(),
        block: block.to_owned(),
        root_index,
        root,
        seed,
    })
}

fn parse_region_identity(text: &str) -> Result<(&str, &str, usize), ParseOutcome> {
    let (prefix, index) = text.rsplit_once('/').ok_or(ParseOutcome::Malformed)?;
    let (function, block) = prefix.split_once('/').ok_or(ParseOutcome::Malformed)?;
    if function.is_empty()
        || block.is_empty()
        || prefix.matches('/').count() != 1
        || (index.len() > 1 && index.starts_with('0'))
    {
        return Err(ParseOutcome::Malformed);
    }
    let index = index.parse().map_err(|_| ParseOutcome::Malformed)?;
    Ok((function, block, index))
}

fn parse_value_id(text: &str) -> Result<u32, ParseOutcome> {
    let digits = text.strip_prefix("%v").ok_or(ParseOutcome::Malformed)?;
    if digits.is_empty()
        || (digits.len() > 1 && digits.starts_with('0'))
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(ParseOutcome::Malformed);
    }
    digits.parse().map_err(|_| ParseOutcome::Malformed)
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

fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
