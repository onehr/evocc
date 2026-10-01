//! Answer the correspondence vectors with the producer's known-bits lattice.
//!
//! `tools/lean/vectors.py` writes one call per line; this copies every line and
//! appends the answer `crates/evocc-eqopt/src/known_bits.rs` gives. The checker
//! has its own runner over its own implementation
//! (`crates/evocc-proofcheck/src/bin/evo-facts-vectors.rs`), and
//! `tools/lean/diff.py` compares the two files. Section 11.6 keeps the analysis
//! on the "not shared" side of the boundary precisely so that the two can
//! disagree; nothing here may call across to the other side.
//!
//! ```text
//! cargo run -p evocc-eqopt --example known_bits_vectors -- <input.jsonl> [<output.jsonl>]
//! ```
//!
//! With no output path the answers go to `<input stem>.eqopt.rust.jsonl` beside
//! the input.
//!
//! Two answers are not results. `skipped` marks a kind this runner does not
//! implement -- the value vectors belong to the interpreter -- and `unsupported`
//! marks a vector this implementation cannot represent: `KnownBits` carries a
//! mask and a value as `u64`, so a fact wider than sixty-four bits has no
//! faithful form here and inventing one would report agreement where there is
//! only truncation.

use std::io::{BufRead, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use evocc_eqopt::known_bits::{Comparison, KnownBits};

/// The widest fact `KnownBits` represents.
const REPRESENTABLE: u16 = 64;

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    let (input, output) = match arguments.as_slice() {
        [input] => {
            let input = PathBuf::from(input);
            let output = default_output(&input);
            (input, output)
        }
        [input, output] => (PathBuf::from(input), PathBuf::from(output)),
        _ => {
            eprintln!("usage: known_bits_vectors <input.jsonl> [<output.jsonl>]");
            return ExitCode::from(2);
        }
    };
    match run(&input, &output) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("known_bits_vectors: {message}");
            ExitCode::from(1)
        }
    }
}

fn default_output(input: &Path) -> PathBuf {
    let stem = input.file_stem().map_or_else(
        || "vectors".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    input.with_file_name(format!("{stem}.eqopt.rust.jsonl"))
}

fn run(input: &Path, output: &Path) -> Result<String, String> {
    let source =
        std::fs::File::open(input).map_err(|error| format!("open {}: {error}", input.display()))?;
    let sink = std::fs::File::create(output)
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    let mut writer = BufWriter::new(sink);

    let mut answered = 0_u64;
    let mut skipped = 0_u64;
    let mut unsupported = 0_u64;
    for (number, line) in std::io::BufReader::new(source).lines().enumerate() {
        let line = line.map_err(|error| format!("read {}: {error}", input.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let result = answer(&line).ok_or_else(|| format!("line {}: not a vector", number + 1))?;
        match result.as_str() {
            "\"skipped\"" => skipped += 1,
            "\"unsupported\"" => unsupported += 1,
            _ => answered += 1,
        }
        let body = line
            .trim_end()
            .strip_suffix('}')
            .ok_or_else(|| format!("line {}: not a JSON object", number + 1))?;
        writeln!(writer, "{body},\"result\":{result}}}")
            .map_err(|error| format!("write {}: {error}", output.display()))?;
    }
    writer
        .flush()
        .map_err(|error| format!("write {}: {error}", output.display()))?;
    Ok(format!(
        "{}: answered={answered}, skipped={skipped}, unsupported={unsupported}",
        output.display()
    ))
}

/// The answer for one vector, already rendered as the JSON it will carry.
fn answer(line: &str) -> Option<String> {
    let kind = text_field(line, "kind")?;
    match kind {
        "transfer" => Some(transfer(line)),
        "meet" | "join" | "covers" => Some(lattice(line, kind)),
        "value" => Some("\"skipped\"".to_owned()),
        _ => None,
    }
}

fn transfer(line: &str) -> String {
    let Some(n) = number_field(line, "n") else {
        return unsupported();
    };
    let Some(op) = text_field(line, "op") else {
        return unsupported();
    };
    let facts = fact_list(field(line, "facts").unwrap_or("[]"));
    let source_width = number_field(line, "w").unwrap_or(n);
    let amount_width = number_field(line, "m").unwrap_or(n);

    if let Some(comparison) = comparison(op) {
        let (Some(left), Some(right)) = (fact(n, facts.first()), fact(n, facts.get(1))) else {
            return unsupported();
        };
        return rendered(left.transfer_icmp(right, comparison));
    }
    match op {
        "divisor_nonzero"
        | "signed_div_rem_no_overflow"
        | "signed_add_defined"
        | "signed_sub_defined"
        | "signed_mul_defined"
        | "shift_count_defined.unsigned"
        | "shift_count_defined.signed" => {
            let Some(left) = fact(n, facts.first()) else {
                return unsupported();
            };
            let width = if op.starts_with("shift_count_defined.") {
                amount_width
            } else {
                n
            };
            left.transfer_guard(op, fact(width, facts.get(1)))
                .map_or_else(unsupported, rendered)
        }
        "and" | "or" | "xor" | "add" | "sub" | "mul" => {
            let (Some(left), Some(right)) = (fact(n, facts.first()), fact(n, facts.get(1))) else {
                return unsupported();
            };
            rendered(match op {
                "and" => left.transfer_and(right),
                "or" => left.transfer_or(right),
                "xor" => left.transfer_xor(right),
                "add" => left.transfer_add(right),
                "sub" => left.transfer_sub(right),
                _ => left.transfer_mul(right),
            })
        }
        "shl" | "lshr" | "ashr" => {
            let (Some(left), Some(amount)) =
                (fact(n, facts.first()), fact(amount_width, facts.get(1)))
            else {
                return unsupported();
            };
            rendered(match op {
                "shl" => left.transfer_shl(amount),
                "lshr" => left.transfer_lshr(amount),
                _ => left.transfer_ashr(amount),
            })
        }
        "zext" | "sext" | "trunc" => {
            let Some(source) = fact(source_width, facts.first()) else {
                return unsupported();
            };
            if n > REPRESENTABLE {
                return unsupported();
            }
            rendered(match op {
                "zext" => source.transfer_zext(n),
                "sext" => source.transfer_sext(n),
                _ => source.transfer_trunc(n),
            })
        }
        _ => unsupported(),
    }
}

fn lattice(line: &str, kind: &str) -> String {
    let Some(n) = number_field(line, "n") else {
        return unsupported();
    };
    let facts = fact_list(field(line, "facts").unwrap_or("[]"));
    let (Some(left), Some(right)) = (fact(n, facts.first()), fact(n, facts.get(1))) else {
        return unsupported();
    };
    match kind {
        "meet" => left
            .meet(right)
            .map_or_else(|| "\"conflict\"".to_owned(), rendered),
        // `join` answers `None` only for a width mismatch, which is a malformed
        // pairing rather than the disagreement `meet` reports as a conflict.
        "join" => left.join(right).map_or_else(unsupported, rendered),
        _ => format!("{}", left.covers(right)),
    }
}

fn comparison(op: &str) -> Option<Comparison> {
    Some(match op {
        "icmp.eq" => Comparison::Eq,
        "icmp.ne" => Comparison::Ne,
        "icmp.slt" => Comparison::SLt,
        "icmp.sle" => Comparison::SLe,
        "icmp.ult" => Comparison::ULt,
        "icmp.ule" => Comparison::ULe,
        _ => return None,
    })
}

/// One fact of the vector, or `None` when this implementation cannot carry it.
fn fact(width: u16, pair: Option<&(u128, u128)>) -> Option<KnownBits> {
    let &(mask, value) = pair?;
    if width == 0 || width > REPRESENTABLE {
        return None;
    }
    Some(KnownBits::new(
        width,
        u64::try_from(mask).ok()?,
        u64::try_from(value).ok()?,
    ))
}

fn rendered(fact: KnownBits) -> String {
    format!(
        "{{\"mask\":\"0x{:x}\",\"value\":\"0x{:x}\"}}",
        fact.mask(),
        fact.value()
    )
}

fn unsupported() -> String {
    "\"unsupported\"".to_owned()
}

// -- reading one generated line ---------------------------------------------
//
// The vectors are this repository's own canonical rendering rather than
// arbitrary JSON: one flat object per line, no whitespace, and the only nested
// keys are `mask` and `value`. A scanner for that shape reads them without the
// workspace taking on a JSON parser it has no other use for.

/// The raw text of one top-level field, brackets and quotes included.
fn field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\":");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let mut depth = 0_usize;
    let mut quoted = false;
    for (index, character) in rest.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '[' | '{' if !quoted => depth += 1,
            ']' | '}' if !quoted => {
                if depth == 0 {
                    return Some(&rest[..index]);
                }
                depth -= 1;
            }
            ',' if !quoted && depth == 0 => return Some(&rest[..index]),
            _ => {}
        }
    }
    Some(rest)
}

fn text_field<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    field(line, key)?.trim_matches('"').into()
}

fn number_field(line: &str, key: &str) -> Option<u16> {
    field(line, key)?.parse().ok()
}

/// Every quoted hexadecimal literal in `raw`, in order.
fn hex_list(raw: &str) -> Vec<u128> {
    raw.split('"')
        .filter_map(|piece| piece.strip_prefix("0x"))
        .filter_map(|digits| u128::from_str_radix(digits, 16).ok())
        .collect()
}

/// The facts of a `facts` array: each object spells its mask then its value.
fn fact_list(raw: &str) -> Vec<(u128, u128)> {
    hex_list(raw)
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (pair[0], pair[1]))
        .collect()
}
