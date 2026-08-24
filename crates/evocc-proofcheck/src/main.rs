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
    if evocc_evoir::validate_section9_bytes(&before).is_err()
        || evocc_evoir::validate_section9_bytes(&after).is_err()
    {
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
        Claim::AddZeroRhs => match reassemble_reviewed_add_zero(&before) {
            Some(module) => module,
            None => return ExitCode::from(3),
        },
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
    AddZeroRhs,
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
        } else if lines[3..13]
            == [
                "region main/entry/1 root %v2 witnesses 2 seeds 1",
                "witness 0 main/entry/0",
                "witness 1 main/entry/1",
                "seed 0 %v0 bits<32> input c0",
                "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-",
                "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1",
                "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0",
                "extract 0 c0 seed:0",
                "root c0",
                "endregion",
            ]
        {
            Claim::AddZeroRhs
        } else {
            return Err(ParseOutcome::UnsupportedRegion);
        };
        Ok(Self {
            input,
            output,
            claim,
        })
    }
}

fn reassemble_reviewed_add_zero(before: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(before).ok()?;
    let required = [
        "func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }",
        "{",
        "^entry(%v0: bits<32>):",
        "    %v1 = const.bits<32> 0",
        "    %v2 = add.bits<32> %v0, %v1",
        "    ret %v2",
        "}",
    ];
    if text.lines().ne(required) {
        return None;
    }
    Some(
        concat!(
            "func @main(%arg0: bits<32>) -> (bits<32>) cc = target_default\n",
            "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
            "{\n",
            "^entry(%v0: bits<32>):\n",
            "    %v1 = const.bits<32> 0\n",
            "    ret %v0\n",
            "}\n",
        )
        .as_bytes()
        .to_vec(),
    )
}

fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
