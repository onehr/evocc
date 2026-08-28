//! Generic tests-only target-layout observation probe for the Day-5 corpus.
//!
//! Input is one tab-separated record per line:
//!
//! `case-id<TAB>hex(source)<TAB>hex(typedef)<TAB>hex(path),...<TAB>hex(assignment),...`
//!
//! A dash represents an empty path or assignment list. Output uses the frozen
//! Day-5 observation schema:
//!
//! `case-id<TAB>sizeof<TAB>alignof<TAB>offset,...<TAB>storage-hex`
//!
//! This executable never receives oracle observations. Every result comes
//! from parsing the supplied C source and querying `TargetDataLayout` over the
//! canonical types produced by `Parser`.

use std::io::{self, BufRead, Write};

use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{
    ArrayBound, BitFieldLayout, RecordField, TagDefinition, TagState, TargetDataLayout, TypeId,
    TypeInterner, TypeKind,
};

const FILE_ID: u32 = 75;

#[derive(Debug, Clone, PartialEq, Eq)]
enum PathStep {
    Field(String),
    Index(u64),
}

#[derive(Debug, Clone, Copy)]
struct ResolvedPath {
    ty: TypeId,
    byte_offset: u64,
    bit_field: Option<ResolvedBitField>,
}

#[derive(Debug, Clone, Copy)]
struct ResolvedBitField {
    storage_byte_offset: u64,
    layout: BitFieldLayout,
}

fn hex_digit(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(format!("invalid hex digit {byte:?}")),
    }
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>, String> {
    if !encoded.len().is_multiple_of(2) {
        return Err("odd-length hex input".to_owned());
    }
    encoded
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| Ok((hex_digit(pair[0])? << 4) | hex_digit(pair[1])?))
        .collect()
}

fn decode_string(encoded: &str, label: &str) -> Result<String, String> {
    String::from_utf8(hex_decode(encoded)?).map_err(|_| format!("{label} is not valid UTF-8"))
}

fn decode_list(encoded: &str, label: &str) -> Result<Vec<String>, String> {
    if encoded == "-" {
        return Ok(Vec::new());
    }
    if encoded.is_empty() {
        return Err(format!("empty {label} list must use '-'"));
    }
    encoded
        .split(',')
        .map(|item| decode_string(item, label))
        .collect()
}

fn hex_encode(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn parse_identifier(input: &str, cursor: &mut usize) -> Result<String, String> {
    let bytes = input.as_bytes();
    let start = *cursor;
    let first = bytes
        .get(*cursor)
        .copied()
        .ok_or_else(|| "expected identifier".to_owned())?;
    if !(first == b'_' || first.is_ascii_alphabetic()) {
        return Err(format!("expected identifier at byte {start}"));
    }
    *cursor += 1;
    while bytes
        .get(*cursor)
        .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
    {
        *cursor += 1;
    }
    Ok(input[start..*cursor].to_owned())
}

fn parse_path(input: &str) -> Result<Vec<PathStep>, String> {
    let mut cursor = 0;
    let mut steps = vec![PathStep::Field(parse_identifier(input, &mut cursor)?)];
    let bytes = input.as_bytes();
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'.' => {
                cursor += 1;
                steps.push(PathStep::Field(parse_identifier(input, &mut cursor)?));
            }
            b'[' => {
                cursor += 1;
                let start = cursor;
                while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
                    cursor += 1;
                }
                if start == cursor || bytes.get(cursor) != Some(&b']') {
                    return Err(format!("malformed array index at byte {start}"));
                }
                let index = input[start..cursor]
                    .parse::<u64>()
                    .map_err(|_| "array index exceeds u64".to_owned())?;
                cursor += 1;
                steps.push(PathStep::Index(index));
            }
            _ => return Err(format!("unexpected path byte at {cursor}")),
        }
    }
    Ok(steps)
}

fn record_fields(
    types: &TypeInterner,
    ty: TypeId,
) -> Result<(evocc_types::TagId, &[RecordField]), String> {
    let canonical = types
        .get(ty)
        .ok_or_else(|| format!("unknown type ID {}", ty.raw()))?;
    let tag_id = match canonical.kind {
        TypeKind::Struct(tag) | TypeKind::Union(tag) => tag,
        _ => return Err("member selection requires a struct or union".to_owned()),
    };
    let tag = types
        .tag(tag_id)
        .ok_or_else(|| format!("unknown tag ID {}", tag_id.raw()))?;
    let fields = match &tag.state {
        TagState::Complete(TagDefinition::Struct(fields) | TagDefinition::Union(fields)) => fields,
        TagState::Complete(TagDefinition::Enum(_)) => {
            return Err("member selection reached an enum".to_owned());
        }
        TagState::Incomplete => return Err("member selection reached an incomplete tag".to_owned()),
    };
    Ok((tag_id, fields))
}

fn resolve_path(
    target: &TargetDataLayout,
    types: &TypeInterner,
    root: TypeId,
    path: &str,
) -> Result<ResolvedPath, String> {
    let steps = parse_path(path)?;
    let mut current = root;
    let mut byte_offset = 0_u64;
    let mut bit_field = None;

    for (step_index, step) in steps.iter().enumerate() {
        if bit_field.is_some() {
            return Err(format!("bit-field is not final in path {path:?}"));
        }
        match step {
            PathStep::Field(name) => {
                let record_base = byte_offset;
                let (tag, fields) = record_fields(types, current)?;
                let field_index = fields
                    .iter()
                    .position(|field| field.name.as_deref() == Some(name.as_str()))
                    .ok_or_else(|| format!("record has no named member {name:?}"))?;
                let field = &fields[field_index];
                let record_layout = target
                    .record_layout(types, tag)
                    .map_err(|error| format!("layout member {name:?}: {error}"))?;
                let field_layout = record_layout
                    .fields
                    .get(field_index)
                    .ok_or_else(|| "record layout omitted a canonical field".to_owned())?;
                if field_layout.field_index != field_index {
                    return Err(
                        "record layout field order is not canonical source order".to_owned()
                    );
                }
                byte_offset = record_base
                    .checked_add(field_layout.byte_offset)
                    .ok_or_else(|| "member byte offset overflow".to_owned())?;
                if let Some(layout) = field_layout.bit_field {
                    bit_field = Some(ResolvedBitField {
                        storage_byte_offset: record_base
                            .checked_add(layout.storage_byte_offset)
                            .ok_or_else(|| "bit-field storage offset overflow".to_owned())?,
                        layout,
                    });
                    if step_index + 1 != steps.len() {
                        return Err(format!("bit-field is not final in path {path:?}"));
                    }
                }
                current = field.ty;
            }
            PathStep::Index(index) => {
                let canonical = types
                    .get(current)
                    .ok_or_else(|| format!("unknown type ID {}", current.raw()))?;
                let TypeKind::Array { element, bound } = &canonical.kind else {
                    return Err(format!("index in path {path:?} does not select an array"));
                };
                let ArrayBound::Fixed(count) = *bound else {
                    return Err(format!("index in path {path:?} selects an unbounded array"));
                };
                if *index >= count {
                    return Err(format!("array index {index} is outside bound {count}"));
                }
                let element_layout = target
                    .layout_of(types, *element)
                    .map_err(|error| format!("layout array element: {error}"))?;
                let delta = element_layout
                    .size_bytes
                    .checked_mul(*index)
                    .ok_or_else(|| "array byte offset overflow".to_owned())?;
                byte_offset = byte_offset
                    .checked_add(delta)
                    .ok_or_else(|| "array byte offset overflow".to_owned())?;
                current = *element;
            }
        }
    }

    Ok(ResolvedPath {
        ty: current,
        byte_offset,
        bit_field,
    })
}

fn parse_integer(input: &str) -> Result<u64, String> {
    let trimmed = input.trim();
    let lowercase = trimmed.to_ascii_lowercase();
    let suffix_len = ["ull", "llu", "ul", "lu", "ll", "u", "l"]
        .into_iter()
        .find_map(|suffix| lowercase.ends_with(suffix).then_some(suffix.len()))
        .unwrap_or(0);
    let digits = &trimmed[..trimmed.len() - suffix_len];
    let (radix, digits) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        (16, hex)
    } else if let Some(binary) = digits
        .strip_prefix("0b")
        .or_else(|| digits.strip_prefix("0B"))
    {
        (2, binary)
    } else if digits.starts_with('0') && digits.len() > 1 {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    if digits.is_empty() {
        return if radix == 8 {
            Ok(0)
        } else {
            Err("integer has no digits".to_owned())
        };
    }
    u64::from_str_radix(digits, radix).map_err(|_| format!("invalid integer literal {trimmed:?}"))
}

fn parse_assignment(input: &str) -> Result<(String, u64), String> {
    let statement = input
        .trim()
        .strip_suffix(';')
        .ok_or_else(|| "assignment must end with ';'".to_owned())?;
    let (left, right) = statement
        .split_once('=')
        .ok_or_else(|| "assignment lacks '='".to_owned())?;
    if right.contains('=') {
        return Err("assignment contains more than one '='".to_owned());
    }
    let path = left
        .trim()
        .strip_prefix("value.")
        .ok_or_else(|| "assignment LHS must begin with 'value.'".to_owned())?;
    parse_path(path)?;
    Ok((path.to_owned(), parse_integer(right)?))
}

fn byte_range(
    offset: u64,
    size: u64,
    storage_len: usize,
) -> Result<std::ops::Range<usize>, String> {
    let start = usize::try_from(offset).map_err(|_| "byte offset exceeds usize".to_owned())?;
    let size = usize::try_from(size).map_err(|_| "object size exceeds usize".to_owned())?;
    let end = start
        .checked_add(size)
        .ok_or_else(|| "byte range overflow".to_owned())?;
    if end > storage_len {
        return Err(format!(
            "byte range {start}..{end} exceeds object size {storage_len}"
        ));
    }
    Ok(start..end)
}

fn write_assignment(
    target: &TargetDataLayout,
    types: &TypeInterner,
    root: TypeId,
    storage: &mut [u8],
    assignment: &str,
) -> Result<(), String> {
    let (path, value) = parse_assignment(assignment)?;
    let resolved = resolve_path(target, types, root, &path)?;
    if let Some(bit_field) = resolved.bit_field {
        let width = u32::from(bit_field.layout.width_bits);
        let bit_offset = u32::from(bit_field.layout.bit_offset);
        let unit_bytes = u64::from(bit_field.layout.storage_size_bytes);
        if unit_bytes > 8 {
            return Err("bit-field allocation unit exceeds u64".to_owned());
        }
        let range = byte_range(bit_field.storage_byte_offset, unit_bytes, storage.len())?;
        let mut unit = 0_u64;
        for (index, byte) in storage[range.clone()].iter().copied().enumerate() {
            unit |= u64::from(byte) << (index * 8);
        }
        let value_mask = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        if value & !value_mask != 0 {
            return Err(format!(
                "value {value} does not fit {width}-bit field {path:?}"
            ));
        }
        let shifted_mask = value_mask
            .checked_shl(bit_offset)
            .ok_or_else(|| "bit-field mask shift overflow".to_owned())?;
        let shifted_value = value
            .checked_shl(bit_offset)
            .ok_or_else(|| "bit-field value shift overflow".to_owned())?;
        unit = (unit & !shifted_mask) | shifted_value;
        for (index, byte) in storage[range].iter_mut().enumerate() {
            *byte = ((unit >> (index * 8)) & 0xff) as u8;
        }
        return Ok(());
    }

    let canonical = types
        .get(resolved.ty)
        .ok_or_else(|| format!("unknown type ID {}", resolved.ty.raw()))?;
    if !matches!(canonical.kind, TypeKind::Int(_) | TypeKind::Enum(_)) {
        return Err(format!("assignment path {path:?} is not an integer scalar"));
    }
    let layout = target
        .layout_of(types, resolved.ty)
        .map_err(|error| format!("layout assignment target {path:?}: {error}"))?;
    if layout.size_bytes > 8 {
        return Err("integer assignment target exceeds u64".to_owned());
    }
    if layout.size_bytes < 8 && value >= (1_u64 << (layout.size_bytes * 8)) {
        return Err(format!(
            "value {value} does not fit {}-byte target {path:?}",
            layout.size_bytes
        ));
    }
    let range = byte_range(resolved.byte_offset, layout.size_bytes, storage.len())?;
    for (index, byte) in storage[range].iter_mut().enumerate() {
        *byte = ((value >> (index * 8)) & 0xff) as u8;
    }
    Ok(())
}

fn project(
    case_id: &str,
    source: &str,
    type_name: &str,
    paths: &[String],
    assignments: &[String],
) -> Result<String, String> {
    let file = FileId::new(FILE_ID);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!(
                "lexical error at bytes {}..{}",
                token.span.byte_start, token.span.byte_end
            ));
        }
        tokens.push(token);
    }

    let mut preprocessor = Preprocessor::new();
    let tokens = preprocessor
        .process(TokenStream::new(tokens), source)
        .into_vec();
    if let Some(diagnostic) = preprocessor.errors().first() {
        return Err(format!(
            "{}: {} at bytes {}..{}",
            diagnostic.code,
            diagnostic.message,
            diagnostic.span.byte_start,
            diagnostic.span.byte_end
        ));
    }

    let arena = Arena::default();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    if parser.had_error() {
        let diagnostic = parser
            .diagnostic()
            .ok_or_else(|| "parser reported an error without a diagnostic".to_owned())?;
        return Err(format!(
            "{}: {} at bytes {}..{}",
            diagnostic.code,
            diagnostic.message,
            diagnostic.span.byte_start,
            diagnostic.span.byte_end
        ));
    }

    let matches = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Typedef(declaration) if declaration.name.spelling == type_name => {
                Some(declaration.ty)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    let [root] = matches.as_slice() else {
        return Err(format!(
            "expected exactly one typedef named {type_name:?}, found {}",
            matches.len()
        ));
    };

    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let layout = target
        .layout_of(unit.types(), *root)
        .map_err(|error| format!("layout typedef {type_name:?}: {error}"))?;
    let offsets = paths
        .iter()
        .map(|path| resolve_path(&target, unit.types(), *root, path).map(|value| value.byte_offset))
        .collect::<Result<Vec<_>, _>>()?;

    let storage_hex = if assignments.is_empty() {
        "-".to_owned()
    } else {
        let storage_len = usize::try_from(layout.size_bytes)
            .map_err(|_| "object size exceeds usize".to_owned())?;
        let mut storage = vec![0_u8; storage_len];
        for assignment in assignments {
            write_assignment(&target, unit.types(), *root, &mut storage, assignment)?;
        }
        hex_encode(&storage)
    };
    let offsets = if offsets.is_empty() {
        "-".to_owned()
    } else {
        offsets
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    Ok(format!(
        "{case_id}\t{}\t{}\t{offsets}\t{storage_hex}",
        layout.size_bytes, layout.align_bytes
    ))
}

fn run() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| format!("read protocol: {error}"))?;
        let fields = line.split('\t').collect::<Vec<_>>();
        let [case_id, source_hex, type_hex, paths_hex, assignments_hex] = fields.as_slice() else {
            return Err("input record must have exactly five TSV fields".to_owned());
        };
        if case_id.is_empty() || case_id.contains(['\t', '\n', '\r']) {
            return Err("invalid case ID".to_owned());
        }
        let source = decode_string(source_hex, "source")?;
        let type_name = decode_string(type_hex, "typedef name")?;
        let paths = decode_list(paths_hex, "path")?;
        let assignments = decode_list(assignments_hex, "assignment")?;
        match project(case_id, &source, &type_name, &paths, &assignments) {
            Ok(observation) => writeln!(stdout, "{observation}"),
            Err(error) => writeln!(stdout, "ERR\t{case_id}\t{}", hex_encode(error.as_bytes())),
        }
        .map_err(|error| format!("write protocol: {error}"))?;
    }
    stdout
        .flush()
        .map_err(|error| format!("flush protocol: {error}"))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("day5 layout probe failed: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_assignment, parse_integer, parse_path, project, PathStep};

    #[test]
    fn parses_nested_member_and_array_path_without_case_knowledge() {
        assert_eq!(
            parse_path("pair[1].member").unwrap(),
            vec![
                PathStep::Field("pair".to_owned()),
                PathStep::Index(1),
                PathStep::Field("member".to_owned()),
            ]
        );
    }

    #[test]
    fn parses_manifest_assignment_literals_and_suffixes() {
        assert_eq!(
            parse_assignment("value.inner.bits = 0xabcde12345ul;").unwrap(),
            ("inner.bits".to_owned(), 0xabcde12345)
        );
        assert_eq!(
            parse_integer("0x5555555555555555ul").unwrap(),
            0x5555555555555555
        );
    }

    #[test]
    fn rejects_non_manifest_lvalue_roots() {
        assert!(parse_assignment("oracle.value = 1;").is_err());
    }

    #[test]
    fn observes_an_arbitrarily_named_real_typedef_through_production_layout() {
        let source =
            "typedef struct { char head; unsigned int a : 3; unsigned int b : 5; char tail; } arbitrary_t;";
        let paths = ["head".to_owned(), "tail".to_owned()];
        let assignments = [
            "value.head = 0x12;".to_owned(),
            "value.a = 5u;".to_owned(),
            "value.b = 17u;".to_owned(),
            "value.tail = 0x34;".to_owned(),
        ];
        assert_eq!(
            project("arbitrary", source, "arbitrary_t", &paths, &assignments).unwrap(),
            "arbitrary\t4\t4\t0,2\t128d3400"
        );
    }
}
