//! Parser lexical values.

use super::*;

pub(super) fn is_builtin_type_word(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Void
            | TokenKind::BuiltinVaList
            | TokenKind::TargetBuiltinType
            | TokenKind::GnuInt128
            | TokenKind::Bool
            | TokenKind::Char
            | TokenKind::Short
            | TokenKind::Int
            | TokenKind::Long
            | TokenKind::Signed
            | TokenKind::Unsigned
            | TokenKind::Float
            | TokenKind::Double
            | TokenKind::Complex
    )
}

/// Resolve the order-independent builtin declaration-specifier multiset.
///
/// C permits combinations such as `unsigned const long int`; consuming the
/// words one at a time as complete types loses that grammar. This resolver
/// accepts exactly the standard integer, real, and complex families and
/// rejects contradictory or duplicate words.
pub(super) fn resolve_builtin_type(words: &[BuiltinTypeWord]) -> Option<TypeKind> {
    if let Some(target) = words.iter().find_map(|word| word.target_builtin) {
        // GCC 14 admits `_Complex` with exactly the six interchange types on
        // aarch64; `__fp16 _Complex` and `__bf16 _Complex` are errors there.
        let complex = words
            .iter()
            .filter(|word| word.kind == TokenKind::Complex)
            .count();
        return match (words.len(), complex) {
            (1, 0) => Some(TypeKind::TargetBuiltin(target)),
            (2, 1) => ComplexBase::interchange(target).map(TypeKind::Complex),
            _ => None,
        };
    }
    let count = |needle| words.iter().filter(|word| word.kind == needle).count();
    let void = count(TokenKind::Void);
    let builtin_va_list = count(TokenKind::BuiltinVaList);
    let gnu_int128 = count(TokenKind::GnuInt128);
    let boolean = count(TokenKind::Bool);
    let character = count(TokenKind::Char);
    let short = count(TokenKind::Short);
    let integer = count(TokenKind::Int);
    let long = count(TokenKind::Long);
    let signed = count(TokenKind::Signed);
    let unsigned = count(TokenKind::Unsigned);
    let float = count(TokenKind::Float);
    let double = count(TokenKind::Double);
    let complex = count(TokenKind::Complex);

    if void > 1
        || builtin_va_list > 1
        || gnu_int128 > 1
        || boolean > 1
        || character > 1
        || short > 1
        || integer > 1
        || long > 2
        || signed > 1
        || unsigned > 1
        || float > 1
        || double > 1
        || complex > 1
        || (signed != 0 && unsigned != 0)
    {
        return None;
    }

    let total = words.len();
    if builtin_va_list == 1 {
        return (total == 1).then_some(TypeKind::BuiltinVaList);
    }
    if gnu_int128 == 1 {
        let forced_signedness = words.iter().find_map(|word| word.forced_int128_signedness);
        if forced_signedness.is_some() && total != 1 {
            return None;
        }
        if forced_signedness.is_none() && total != 1 + signed + unsigned {
            return None;
        }
        return Some(TypeKind::GnuInt128(forced_signedness.unwrap_or({
            if unsigned == 1 {
                GnuInt128Signedness::Unsigned
            } else {
                GnuInt128Signedness::Signed
            }
        })));
    }
    if void == 1 {
        return (total == 1).then_some(TypeKind::Void);
    }
    if boolean == 1 {
        return (total == 1).then_some(TypeKind::Int(IntRank::Bool));
    }

    if complex == 1 {
        if float == 0 && double == 0 {
            let component = words
                .iter()
                .filter(|word| word.kind != TokenKind::Complex)
                .copied()
                .collect::<Vec<_>>();
            if component.is_empty() {
                // GCC and Clang read plain `_Complex` as `double _Complex`.
                return Some(TypeKind::Complex(ComplexBase::CF64));
            }
            return match resolve_builtin_type(&component)? {
                TypeKind::Int(rank) if rank != IntRank::Bool => {
                    Some(TypeKind::Complex(ComplexBase::Integer(rank)))
                }
                _ => None,
            };
        }
        if signed != 0
            || unsigned != 0
            || short != 0
            || integer != 0
            || character != 0
            || boolean != 0
            || void != 0
        {
            return None;
        }
        return match (float, double, long) {
            (1, 0, 0) => Some(TypeKind::Complex(ComplexBase::CF32)),
            (0, 1, 0) => Some(TypeKind::Complex(ComplexBase::CF64)),
            (0, 1, 1) => Some(TypeKind::Complex(ComplexBase::CF128)),
            _ => None,
        };
    }

    if float == 1 {
        return (total == 1).then_some(TypeKind::Float(FloatKind::F32));
    }
    if double == 1 {
        if signed != 0 || unsigned != 0 || short != 0 || integer != 0 || character != 0 {
            return None;
        }
        return match long {
            0 => Some(TypeKind::Float(FloatKind::F64)),
            1 => Some(TypeKind::Float(FloatKind::F128)),
            _ => None,
        };
    }
    if character == 1 {
        if short != 0 || long != 0 || integer != 0 {
            return None;
        }
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UChar
        } else if signed == 1 {
            IntRank::SChar
        } else {
            IntRank::Char
        }));
    }
    if short == 1 {
        if long != 0 || character != 0 {
            return None;
        }
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UShort
        } else {
            IntRank::Short
        }));
    }
    if long != 0 {
        if short != 0 || character != 0 {
            return None;
        }
        return Some(TypeKind::Int(match (unsigned, long) {
            (1, 1) => IntRank::ULong,
            (1, 2) => IntRank::ULongLong,
            (0, 1) => IntRank::Long,
            (0, 2) => IntRank::LongLong,
            _ => return None,
        }));
    }
    if integer <= 1 && signed <= 1 && unsigned <= 1 && total != 0 {
        return Some(TypeKind::Int(if unsigned == 1 {
            IntRank::UInt
        } else {
            IntRank::Int
        }));
    }
    None
}

// ── integer literal parsing ────────────────────────────────────────────────

pub(super) fn parse_int_literal(text: &str) -> Option<IntLit> {
    let (base, radix, prefix_len) = if text.starts_with("0x") || text.starts_with("0X") {
        (IntBase::Hex, 16, 2)
    } else if text.starts_with("0b") || text.starts_with("0B") {
        (IntBase::Bin, 2, 2)
    } else if text.starts_with('0') && text.len() > 1 {
        (IntBase::Oct, 8, 1)
    } else {
        (IntBase::Dec, 10, 0)
    };

    let digit_count = text[prefix_len..]
        .bytes()
        .take_while(|byte| match radix {
            2 => matches!(byte, b'0' | b'1'),
            8 => matches!(byte, b'0'..=b'7'),
            10 => byte.is_ascii_digit(),
            16 => byte.is_ascii_hexdigit(),
            _ => unreachable!("known integer literal radix"),
        })
        .count();
    if digit_count == 0 && prefix_len != 1 {
        return None;
    }
    let digit_end = prefix_len + digit_count;
    let digits = if prefix_len == 1 && digit_count == 0 {
        "0"
    } else {
        &text[prefix_len..digit_end]
    };
    let suffix = match text[digit_end..].to_ascii_lowercase().as_str() {
        "" => IntSuffix::None,
        "u" => IntSuffix::U,
        "l" => IntSuffix::L,
        "ul" | "lu" => IntSuffix::UL,
        "ll" => IntSuffix::LL,
        "ull" | "llu" => IntSuffix::ULL,
        _ => return None,
    };
    Some(IntLit {
        value: u64::from_str_radix(digits, radix).ok()?,
        suffix,
        base,
    })
}

/// The first token without a C token form after preprocessing, at the exact
/// preserved token: `E0002` for an encoding error, else `E0112`.
pub(super) fn lexical_diagnostic(tokens: &[Token]) -> Option<ParseDiagnostic> {
    lexical_diagnostic_except(tokens, &HashSet::new())
}

/// The first token without a C token form, skipping the tokens at `exempt`.
pub(super) fn lexical_diagnostic_except(
    tokens: &[Token],
    exempt: &HashSet<usize>,
) -> Option<ParseDiagnostic> {
    tokens
        .iter()
        .enumerate()
        .filter(|(index, _)| !exempt.contains(index))
        .map(|(_, token)| token)
        .find(|token| {
            matches!(
                token.kind,
                TokenKind::PpNumber | TokenKind::PpOther | TokenKind::EncodingError
            )
        })
        .map(|token| ParseDiagnostic {
            span: token.span,
            previous_declaration: None,
            code: if token.kind == TokenKind::EncodingError {
                "E0002"
            } else {
                "E0112"
            },
            message: "stray-token".to_owned(),
        })
}

/// The indexes of the preprocessing numbers that are versions of an
/// `availability` attribute: a number after `introduced =`, `deprecated =`
/// or `obsoleted =` inside `availability ( ... )`. Clang reads `10.14.4` and
/// `10_14_4` there, which have no C token form.
pub(super) fn availability_version_tokens(tokens: &[Token]) -> HashSet<usize> {
    fn spelling(token: &Token) -> &str {
        token.text.spelling().unwrap_or("")
    }
    let mut versions = HashSet::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        let name = spelling(&tokens[index]);
        if !matches!(name, "availability" | "__availability__")
            || tokens[index + 1].kind != TokenKind::LParen
        {
            index += 1;
            continue;
        }
        let mut depth = 0_usize;
        index += 1;
        while index < tokens.len() {
            match tokens[index].kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                TokenKind::PpNumber
                    if index >= 2
                        && tokens[index - 1].kind == TokenKind::Assign
                        && matches!(
                            spelling(&tokens[index - 2]),
                            "introduced" | "deprecated" | "obsoleted"
                        ) =>
                {
                    versions.insert(index);
                }
                _ => {}
            }
            index += 1;
        }
    }
    versions
}

/// A literal rounds once, directly into the format of its type. An `L`
/// literal therefore has the target's `long double` format.
/// A half constant (`f16`, `bf16`) rounds once to binary32, its value
/// format (C23 H.3); its type is still the half type.
pub(super) fn parse_float_literal(text: &str, target: TargetDataLayout) -> Option<FloatLit> {
    let (number, suffix) = if let Some((half, bfloat)) = evocc_lex::half_floating_suffix(text) {
        (
            &text[..text.len() - half.len()],
            if bfloat {
                FloatSuffix::BF16
            } else {
                FloatSuffix::F16
            },
        )
    } else {
        match text.as_bytes().last().copied() {
            Some(b'f' | b'F') => (&text[..text.len() - 1], FloatSuffix::F),
            Some(b'l' | b'L') => (&text[..text.len() - 1], FloatSuffix::L),
            _ => (text, FloatSuffix::None),
        }
    };
    Some(FloatLit {
        value: evocc_chir::FloatConstant::Real(evocc_types::floating::FloatBits::parse(
            target.float_format(suffix.value_kind()),
            number,
        )?),
        suffix,
    })
}

pub(super) fn nan_builtin_suffix(name: &str) -> Option<FloatSuffix> {
    match name {
        "__builtin_nan" => Some(FloatSuffix::None),
        "__builtin_nanf" => Some(FloatSuffix::F),
        "__builtin_nanl" => Some(FloatSuffix::L),
        _ => None,
    }
}

/// The feature requests used by Apple's NEON headers need no per-function
/// code generation. Other feature and architecture contracts stay refused.
pub(super) fn positive_target_attribute(attribute: &Attribute) -> bool {
    if attribute.semantic_name() != Some("target") || attribute.arguments.len() != 1 {
        return false;
    }
    let Some(units) = attribute.arguments[0]
        .string_literal()
        .and_then(|s| s.decoded_units())
    else {
        return false;
    };
    let Some(text) = units
        .into_iter()
        .map(char::from_u32)
        .collect::<Option<String>>()
    else {
        return false;
    };
    text.split(',').all(|feature| {
        matches!(
            feature.trim_ascii(),
            "aes"
                | "bf16"
                | "dotprod"
                | "faminmax"
                | "fp16fml"
                | "fp8"
                | "fp8dot2"
                | "fp8dot4"
                | "fp8fma"
                | "fullfp16"
                | "i8mm"
                | "neon"
                | "sha2"
                | "sha3"
                | "sm4"
                | "v8.1a"
                | "v8.3a"
                | "v8.5a"
        )
    })
}

/// EvoIR 10.1.9: the `optimize` options that EvoCC already satisfies. An
/// argument is an integer constant expression from 0 to 3, or a string whose
/// comma-separated options are each an optimization level, with or without
/// `-`, or a listed disabling option, with or without `-f`. The strict driver
/// accepts each listed option with no change of code. A level promises no
/// transform, and EvoCC makes none of the transforms that the options forbid.
pub(super) fn optimize_options_are_satisfied(arguments: &[AttributeArgument]) -> bool {
    const LEVELS: [&str; 7] = ["O", "O0", "O1", "O2", "O3", "Os", "Og"];
    const OPTIONS: [&str; 12] = [
        "no-strict-aliasing",
        "no-optimize-sibling-calls",
        "no-align-functions",
        "no-align-jumps",
        "no-align-labels",
        "no-align-loops",
        "no-prefetch-loop-arrays",
        "no-tree-ch",
        "no-tree-loop-distribute-patterns",
        "no-tree-vectorize",
        "no-tree-loop-vectorize",
        "no-tree-slp-vectorize",
    ];
    let names = |option: &[u32], list: &[&str]| {
        list.iter()
            .any(|name| option.iter().copied().eq(name.bytes().map(u32::from)))
    };
    !arguments.is_empty()
        && arguments.iter().all(|argument| {
            if let Some(level) = argument.integer {
                return level <= 3;
            }
            let Some(units) = argument
                .string_literal()
                .and_then(|literal| literal.decoded_units())
            else {
                return false;
            };
            units.split(|unit| *unit == u32::from(b',')).all(|option| {
                let level = option.strip_prefix(&[u32::from(b'-')]).unwrap_or(option);
                let flag = option
                    .strip_prefix(&[u32::from(b'-'), u32::from(b'f')])
                    .unwrap_or(option);
                names(level, &LEVELS) || names(flag, &OPTIONS)
            })
        })
}

pub(super) fn nan_literal_payload(literal: &StringLit) -> Option<u128> {
    if !matches!(literal.prefix, StringPrefix::None | StringPrefix::U8) {
        return None;
    }
    let bytes = literal
        .decoded_units()?
        .into_iter()
        .map(u8::try_from)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    let text = std::str::from_utf8(&bytes).ok()?;
    if text.is_empty() {
        return Some(0);
    }
    let (digits, radix) =
        if let Some(digits) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            (digits, 16)
        } else if text.starts_with('0') {
            (text, 8)
        } else {
            (text, 10)
        };
    if digits.is_empty() {
        return None;
    }
    // Only the low significand bits survive. Wrapping here supports arbitrary
    // payload lengths without overflow or a host floating conversion.
    digits.bytes().try_fold(0u128, |value, digit| {
        let digit = (digit as char).to_digit(radix)?;
        Some(
            value
                .wrapping_mul(u128::from(radix))
                .wrapping_add(u128::from(digit)),
        )
    })
}

pub(super) fn parse_string_literal(text: &str) -> Option<StringLit> {
    let (quoted, prefix) = if let Some(quoted) = text.strip_prefix("u8") {
        (quoted, StringPrefix::U8)
    } else if let Some(quoted) = text.strip_prefix('u') {
        (quoted, StringPrefix::U16)
    } else if let Some(quoted) = text.strip_prefix('U') {
        (quoted, StringPrefix::U)
    } else if let Some(quoted) = text.strip_prefix('L') {
        (quoted, StringPrefix::L)
    } else {
        (text, StringPrefix::None)
    };
    let contents = quoted.strip_prefix('"')?.strip_suffix('"')?;
    Some(StringLit::single(contents, prefix))
}

pub(super) fn concatenate_string_prefix(
    left: StringPrefix,
    right: StringPrefix,
) -> Option<StringPrefix> {
    match (left, right) {
        (left, right) if left == right => Some(left),
        (StringPrefix::None, right) => Some(right),
        (left, StringPrefix::None) => Some(left),
        _ => None,
    }
}

pub(super) fn parse_char_literal(text: &str, target: TargetDataLayout) -> Option<CharLit> {
    // A plain constant whose source bytes are non-ASCII UTF-8 is a
    // multi-character constant for GCC 14. Apple clang refuses it.
    if let Some(value) = evocc_lex::decode_utf8_plain_character(text) {
        if target.names_clang_extensions() {
            return None;
        }
        return Some(CharLit {
            value,
            prefix: CharPrefix::None,
        });
    }
    // A multi-character constant has type `int` and the value that GCC 14
    // and Clang give it; plain char signedness does not apply (EvoIR 10.5).
    if let Some(value) = evocc_lex::decode_multicharacter_literal(text) {
        return Some(CharLit {
            value,
            prefix: CharPrefix::None,
        });
    }
    let (value, prefix) = evocc_lex::decode_character_literal(text)?;
    let hexadecimal_escape = text
        .split_once('\'')
        .and_then(|(_, contents)| contents.strip_suffix('\''))
        .is_some_and(|contents| contents.starts_with("\\x"));
    let prefix = match prefix {
        "u8" => CharPrefix::U8,
        "u" => CharPrefix::U16,
        "U" => CharPrefix::U,
        "L" => match target.wide_character_rank() {
            IntRank::UShort => CharPrefix::L16,
            IntRank::Int => CharPrefix::LInt,
            _ => CharPrefix::L,
        },
        "" => CharPrefix::None,
        _ => return None,
    };
    let value = if prefix == CharPrefix::L16 {
        if value > u32::from(u16::MAX) {
            // Apple clang refuses every out-of-range value. Linux keeps the
            // reviewed refusal for a universal character name, but follows
            // GCC 14 for a hexadecimal escape and truncates its low bits.
            if target.names_clang_extensions() || !hexadecimal_escape {
                return None;
            }
            value & u32::from(u16::MAX)
        } else {
            value
        }
    } else if prefix == CharPrefix::U16 && value > u32::from(u16::MAX) {
        // GCC 14 keeps the low UTF-16 surrogate in a char16_t; Apple clang
        // refuses the constant.
        if target.names_clang_extensions() {
            return None;
        }
        0xDC00 | (value & 0x3FF)
    } else if prefix == CharPrefix::None && target.plain_char_is_signed() && value <= 255 {
        (value as u8 as i8 as i32) as u32
    } else {
        value
    };
    let literal = CharLit { value, prefix };
    (literal.integer_value().raw_bits() == u128::from(value)).then_some(literal)
}
