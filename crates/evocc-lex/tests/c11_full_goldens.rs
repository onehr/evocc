use std::collections::HashSet;

use evocc_common::FileId;
use evocc_lex::{Lexer, TokenKind};

const GOLDENS: &str = include_str!("../../../tests/contracts/lexer-c11-goldens.tsv");
const EXPECTED_CASES: usize = 141;

#[derive(Debug)]
struct ExpectedToken {
    kind: String,
    spelling: Vec<u8>,
    byte_start: u32,
    byte_end: u32,
    line: u32,
    column: u32,
}

#[derive(Debug)]
struct Case {
    id: String,
    input: Vec<u8>,
    tokens: Vec<ExpectedToken>,
    diagnostic: String,
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0, "odd hex length: {value}");
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
        .collect()
}

fn parse_case(line: &str) -> Case {
    let fields: Vec<&str> = line.split('\t').collect();
    assert_eq!(fields.len(), 4, "invalid golden row: {line:?}");
    let tokens = fields[2]
        .split(';')
        .map(|encoded| {
            let parts: Vec<&str> = encoded.split('|').collect();
            assert_eq!(parts.len(), 6, "invalid token row: {encoded:?}");
            ExpectedToken {
                kind: parts[0].to_owned(),
                spelling: decode_hex(parts[1]),
                byte_start: parts[2].parse().unwrap(),
                byte_end: parts[3].parse().unwrap(),
                line: parts[4].parse().unwrap(),
                column: parts[5].parse().unwrap(),
            }
        })
        .collect();
    Case {
        id: fields[0].to_owned(),
        input: decode_hex(fields[1]),
        tokens,
        diagnostic: fields[3].to_owned(),
    }
}

fn cases() -> Vec<Case> {
    let mut lines = GOLDENS.lines();
    assert_eq!(
        lines.next(),
        Some("case_id\tinput_hex\texpected_tokens\texpected_diagnostic")
    );
    lines.map(parse_case).collect()
}

#[test]
fn golden_denominator_is_exact_and_inputs_are_distinct() {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASES);
    assert!(cases.len() >= 100);
    let ids: HashSet<&str> = cases.iter().map(|case| case.id.as_str()).collect();
    assert_eq!(ids.len(), EXPECTED_CASES, "duplicate case ID");
    let inputs: HashSet<&[u8]> = cases.iter().map(|case| case.input.as_slice()).collect();
    assert_eq!(inputs.len(), EXPECTED_CASES, "duplicate input bytes");
}

#[test]
fn every_c11_golden_matches_kind_spelling_span_diagnostic_and_eof() {
    for case in cases() {
        let source = String::from_utf8(case.input.clone())
            .unwrap_or_else(|_| panic!("{} belongs in the raw-byte driver matrix", case.id));
        let mut lexer = Lexer::new(FileId::new(7), &source);
        let mut actual = Vec::new();
        while let Some(token) = lexer.next() {
            actual.push(token);
        }
        assert_eq!(
            actual.len(),
            case.tokens.len(),
            "{} token count: actual={actual:?}",
            case.id
        );
        for (index, (observed, expected)) in actual.iter().zip(&case.tokens).enumerate() {
            assert_eq!(
                format!("{:?}", observed.kind),
                expected.kind,
                "{} token {index} kind",
                case.id
            );
            let spelling = source
                .as_bytes()
                .get(observed.span.byte_start as usize..observed.span.byte_end as usize)
                .unwrap_or(&[]);
            assert_eq!(
                spelling, expected.spelling,
                "{} token {index} spelling",
                case.id
            );
            assert_eq!(
                (
                    observed.span.byte_start,
                    observed.span.byte_end,
                    observed.span.line_start,
                    observed.span.column_start,
                ),
                (
                    expected.byte_start,
                    expected.byte_end,
                    expected.line,
                    expected.column,
                ),
                "{} token {index} span",
                case.id
            );
        }
        assert_eq!(
            actual
                .iter()
                .filter(|token| token.kind == TokenKind::Eof)
                .count(),
            1,
            "{} must emit exactly one EOF",
            case.id
        );
        assert_eq!(actual.last().map(|token| token.kind), Some(TokenKind::Eof));
        assert!(lexer.next().is_none(), "{} revived after EOF", case.id);
        assert!(
            lexer.next().is_none(),
            "{} revived after second EOF probe",
            case.id
        );
        assert!(
            lexer.next().is_none(),
            "{} revived after third EOF probe",
            case.id
        );
        let diagnostics = actual
            .iter()
            .filter(|token| token.kind == TokenKind::Error)
            .map(|token| {
                format!(
                    "E0001@{}:{}",
                    token.span.line_start, token.span.column_start
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        let diagnostics = if diagnostics.is_empty() {
            "-".to_owned()
        } else {
            diagnostics
        };
        assert_eq!(diagnostics, case.diagnostic, "{} diagnostic", case.id);
    }
}
