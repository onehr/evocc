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
    let certificate = match ZeroRegionCertificate::parse(&proof) {
        Ok(certificate) => certificate,
        Err(ParseOutcome::Malformed) => return ExitCode::from(2),
        Err(ParseOutcome::UnsupportedRegion) => return ExitCode::from(3),
    };

    let before_digest = sha256_hex(&before);
    let after_digest = sha256_hex(&after);
    if certificate.input != before_digest || certificate.output != after_digest || before != after {
        return ExitCode::from(3);
    }
    ExitCode::SUCCESS
}

fn read(path: &impl AsRef<Path>) -> Result<Vec<u8>, ()> {
    fs::read(path).map_err(|_| ())
}

struct ZeroRegionCertificate<'a> {
    input: &'a str,
    output: &'a str,
}

enum ParseOutcome {
    Malformed,
    UnsupportedRegion,
}

impl<'a> ZeroRegionCertificate<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, ParseOutcome> {
        let text = std::str::from_utf8(bytes).map_err(|_| ParseOutcome::Malformed)?;
        if !text.ends_with('\n') || text.ends_with("\n\n") || text.contains('\r') {
            return Err(ParseOutcome::Malformed);
        }
        let lines = text.lines().collect::<Vec<_>>();
        if lines.first() != Some(&"evp1") {
            return Err(ParseOutcome::Malformed);
        }
        if lines.len() > 4 && lines.get(3).is_some_and(|line| line.starts_with("region ")) {
            // Region records are syntactically recognized but are not accepted
            // until the independent eligibility/reassembly implementation lands.
            return Err(ParseOutcome::UnsupportedRegion);
        }
        if lines.len() != 4 {
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
        let output = lines[3]
            .strip_prefix("output ")
            .ok_or(ParseOutcome::Malformed)?;
        if !is_digest(input) || !is_digest(output) {
            return Err(ParseOutcome::Malformed);
        }
        Ok(Self { input, output })
    }
}

fn is_digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
