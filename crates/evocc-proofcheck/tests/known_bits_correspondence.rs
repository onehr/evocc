//! The producer's known-bits lattice and the checker's answer the same way.
//!
//! Section 11.6 puts the analysis on the "not shared" side of the boundary, so
//! `crates/evocc-eqopt/src/known_bits.rs` and `crates/evocc-proofcheck/src/facts.rs`
//! are two implementations of the Section 8.10 table with two different
//! representations. That is the point, and it is also the risk: they may drift,
//! and a premise the producer records and the checker cannot confirm is a
//! refused certificate rather than a caught defect.
//!
//! `tools/lean/vectors.py` generates the full correspondence set and
//! `tools/lean/diff.py` compares two answered files; those run by hand and reach
//! widths this test does not. This is the part that has to stay green: every
//! normalized fact of `bits<3>`, every transfer of the table over every pair of
//! them, and the three lattice operations. The producer's answer is computed
//! here; the checker's comes from running its own vector binary, so the checker
//! is measured through the same file interface the larger runs use.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use evocc_eqopt::known_bits::{Comparison, KnownBits};

/// Small enough to enumerate exhaustively, wide enough that a carry, a sign bit
/// and an out-of-range shift amount all occur.
const WIDTH: u16 = 3;
/// The widths the cast rows are measured across.
const NARROW: u16 = 2;
const WIDE: u16 = 5;

const BINARY: [&str; 6] = ["and", "or", "xor", "add", "sub", "mul"];
const COMPARISONS: [&str; 6] = [
    "icmp.eq", "icmp.ne", "icmp.slt", "icmp.sle", "icmp.ult", "icmp.ule",
];
const SHIFTS: [&str; 3] = ["shl", "lshr", "ashr"];

/// Every normalized fact of `bits<width>`: each bit unknown, known zero or
/// known one, so `3^width` of them.
fn facts(width: u16) -> Vec<KnownBits> {
    let mut all = Vec::new();
    for code in 0..3_u32.pow(u32::from(width)) {
        let mut mask = 0_u64;
        let mut value = 0_u64;
        let mut remaining = code;
        for bit in 0..width {
            let digit = remaining % 3;
            remaining /= 3;
            if digit >= 1 {
                mask |= 1 << bit;
            }
            if digit == 2 {
                value |= 1 << bit;
            }
        }
        all.push(KnownBits::new(width, mask, value));
    }
    all
}

fn fact_literal(fact: KnownBits) -> String {
    format!(
        "{{\"mask\":\"0x{:x}\",\"value\":\"0x{:x}\"}}",
        fact.mask(),
        fact.value()
    )
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

/// One vector line and the answer the producer's implementation gives it.
fn vectors() -> Vec<(String, String)> {
    let mut cases = Vec::new();
    let same = facts(WIDTH);
    for op in [
        "divisor_nonzero",
        "signed_div_rem_no_overflow",
        "signed_add_defined",
        "signed_sub_defined",
        "signed_mul_defined",
        "shift_count_defined.unsigned",
        "shift_count_defined.signed",
    ] {
        for &a in &same {
            let seconds = if op == "divisor_nonzero" {
                vec![None]
            } else {
                same.iter().map(|&b| Some(b)).collect()
            };
            for b in seconds {
                let pair = b.map_or_else(
                    || fact_literal(a),
                    |b| format!("{},{}", fact_literal(a), fact_literal(b)),
                );
                cases.push((format!("{{\"kind\":\"transfer\",\"op\":\"{op}\",\"n\":{WIDTH},\"facts\":[{pair}]}}"),
                    fact_literal(a.transfer_guard(op,b).unwrap())));
            }
        }
    }
    for op in BINARY.iter().chain(&COMPARISONS) {
        for &left in &same {
            for &right in &same {
                let answer = match comparison(op) {
                    Some(comparison) => left.transfer_icmp(right, comparison),
                    None => match *op {
                        "and" => left.transfer_and(right),
                        "or" => left.transfer_or(right),
                        "xor" => left.transfer_xor(right),
                        "add" => left.transfer_add(right),
                        "sub" => left.transfer_sub(right),
                        _ => left.transfer_mul(right),
                    },
                };
                cases.push((
                    format!(
                        "{{\"kind\":\"transfer\",\"op\":\"{op}\",\"n\":{WIDTH},\"facts\":[{},{}]}}",
                        fact_literal(left),
                        fact_literal(right)
                    ),
                    fact_literal(answer),
                ));
            }
        }
    }
    for op in &SHIFTS {
        for &left in &same {
            for &amount in &same {
                let answer = match *op {
                    "shl" => left.transfer_shl(amount),
                    "lshr" => left.transfer_lshr(amount),
                    _ => left.transfer_ashr(amount),
                };
                cases.push((
                    format!(
                        "{{\"kind\":\"transfer\",\"op\":\"{op}\",\"n\":{WIDTH},\"m\":{WIDTH},\"facts\":[{},{}]}}",
                        fact_literal(left),
                        fact_literal(amount)
                    ),
                    fact_literal(answer),
                ));
            }
        }
    }
    for source in facts(NARROW) {
        for (op, answer) in [
            ("zext", source.transfer_zext(WIDE)),
            ("sext", source.transfer_sext(WIDE)),
        ] {
            cases.push((
                format!(
                    "{{\"kind\":\"transfer\",\"op\":\"{op}\",\"w\":{NARROW},\"n\":{WIDE},\"facts\":[{}]}}",
                    fact_literal(source)
                ),
                fact_literal(answer),
            ));
        }
    }
    for source in facts(WIDE) {
        cases.push((
            format!(
                "{{\"kind\":\"transfer\",\"op\":\"trunc\",\"w\":{WIDE},\"n\":{NARROW},\"facts\":[{}]}}",
                fact_literal(source)
            ),
            fact_literal(source.transfer_trunc(NARROW)),
        ));
    }
    for &left in &same {
        for &right in &same {
            let pair = format!("{},{}", fact_literal(left), fact_literal(right));
            cases.push((
                format!("{{\"kind\":\"meet\",\"n\":{WIDTH},\"facts\":[{pair}]}}"),
                left.meet(right)
                    .map_or_else(|| "\"conflict\"".to_owned(), fact_literal),
            ));
            cases.push((
                format!("{{\"kind\":\"join\",\"n\":{WIDTH},\"facts\":[{pair}]}}"),
                left.join(right)
                    .map_or_else(|| "\"unsupported\"".to_owned(), fact_literal),
            ));
            cases.push((
                format!("{{\"kind\":\"covers\",\"n\":{WIDTH},\"facts\":[{pair}]}}"),
                format!("{}", left.covers(right)),
            ));
        }
    }
    cases
}

fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "evocc-known-bits-correspondence-{}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create the correspondence scratch directory");
    root
}

#[test]
fn the_producer_and_the_checker_answer_every_small_vector_the_same_way() {
    let cases = vectors();
    let root = scratch();
    let input = root.join("small.jsonl");
    let output = root.join("small.proofcheck.rust.jsonl");
    let mut text = String::new();
    for (line, _) in &cases {
        text.push_str(line);
        text.push('\n');
    }
    fs::write(&input, &text).expect("write the correspondence vectors");

    let run = Command::new(env!("CARGO_BIN_EXE_evo-facts-vectors"))
        .args([&input, &output])
        .output()
        .expect("run the checker's vector binary");
    assert_eq!(
        run.status.code(),
        Some(0),
        "the checker's vector binary refused the input: {}",
        String::from_utf8_lossy(&run.stderr)
    );

    let answered = fs::read_to_string(&output).expect("read the checker's answers");
    let answers = answered.lines().collect::<Vec<_>>();
    assert_eq!(answers.len(), cases.len(), "one answer per vector");

    let mut agreements = 0;
    let mut differences = Vec::new();
    for ((line, expected), answer) in cases.iter().zip(&answers) {
        let body = line.strip_suffix('}').expect("a vector is a JSON object");
        let together = format!("{body},\"result\":{expected}}}");
        if &together == answer {
            agreements += 1;
        } else if differences.len() < 10 {
            differences.push(format!("producer: {together}\nchecker:  {answer}"));
        }
    }
    assert!(
        differences.is_empty(),
        "the two implementations of Section 8.10 disagree on {} of {} vectors:\n{}",
        cases.len() - agreements,
        cases.len(),
        differences.join("\n")
    );
    println!(
        "known-bits correspondence: vectors={}, agreements={agreements}, differences=0",
        cases.len()
    );
    let _ = fs::remove_dir_all(&root);
}
