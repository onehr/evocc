//! Compiler-provided 128-bit integer spellings fixed by specification 10.1.19.

use evocc_chir::CHirDecl;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{GnuInt128Signedness, TargetDataLayout, TypeKind, TypeLayout};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let file = FileId::new(10_121);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        parser.diagnostic().is_none(),
        "source: {source}; diagnostic: {:?}",
        parser.diagnostic(),
    );
    unit
}

#[test]
fn compiler_spellings_are_the_underlying_128_bit_types() {
    let arena = Arena::default();
    let unit = parse(
        concat!(
            "typedef __int128_t signed_spelling;\n",
            "typedef __int128 signed_underlying;\n",
            "typedef __uint128_t unsigned_spelling;\n",
            "typedef unsigned __int128 unsigned_underlying;\n",
            "_Static_assert(sizeof(__int128_t) == 16, \"signed size\");\n",
            "_Static_assert(sizeof(__uint128_t) == 16, \"unsigned size\");\n",
            "_Static_assert(__builtin_types_compatible_p(__int128_t, __int128), \"signed identity\");\n",
            "_Static_assert(__builtin_types_compatible_p(__uint128_t, unsigned __int128), \"unsigned identity\");\n",
        ),
        &arena,
    );
    let aliases = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Typedef(alias) => Some(alias),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 4);
    assert_eq!(aliases[0].ty, aliases[1].ty);
    assert_eq!(aliases[2].ty, aliases[3].ty);
    assert_eq!(
        unit.types().get(aliases[0].ty).expect("signed type").kind,
        TypeKind::GnuInt128(GnuInt128Signedness::Signed),
    );
    assert_eq!(
        unit.types().get(aliases[2].ty).expect("unsigned type").kind,
        TypeKind::GnuInt128(GnuInt128Signedness::Unsigned),
    );
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert_eq!(
        layout.layout_of(unit.types(), aliases[0].ty),
        Ok(TypeLayout::new(16, 16)),
    );
    assert_eq!(
        layout.layout_of(unit.types(), aliases[2].ty),
        Ok(TypeLayout::new(16, 16)),
    );
}

#[test]
fn compiler_spellings_are_contextual_after_preprocessing() {
    for spelling in ["__int128_t", "__uint128_t"] {
        let mut lexer = Lexer::new(FileId::new(10_122), spelling);
        assert_eq!(
            lexer.next().expect("compiler spelling token").kind,
            TokenKind::Identifier,
        );
    }

    let arena = Arena::default();
    parse(
        "extern __int128_t consume(__uint128_t); __uint128_t *pointer;\n",
        &arena,
    );
}

#[test]
fn signed_and_unsigned_128_bit_objects_are_modifiable_assignment_targets() {
    let arena = Arena::default();
    parse(
        concat!(
            "__uint128_t copy_unsigned(__uint128_t *p) { ",
            "__uint128_t tmp; tmp = *p; *p = tmp; return tmp; }\n",
            "__int128_t copy_signed(__int128_t *p) { ",
            "__int128_t tmp; tmp = *p; *p = tmp; return tmp; }\n",
        ),
        &arena,
    );
}

#[test]
fn wide_integer_initializers_accept_convertible_integer_values() {
    let arena = Arena::default();
    parse(
        concat!(
            "__int128_t signed_value = -1;\n",
            "__uint128_t unsigned_value = 7;\n",
            "void initialize(void) { __int128 local = 3; unsigned __int128 other = 9; }\n",
        ),
        &arena,
    );
}

#[test]
fn wide_integer_compound_assignments_share_the_integer_assignment_path() {
    let arena = Arena::default();
    parse(
        concat!(
            "typedef unsigned int u32;\n",
            "unsigned long checksum(const unsigned char *iph, int ihl) {\n",
            "  __uint128_t tmp = *(const __uint128_t *)iph;\n",
            "  unsigned long sum; int n = ihl; iph += 16; n -= 4;\n",
            "  tmp += ((tmp >> 64) | (tmp << 64));\n",
            "  sum = tmp >> 64;\n",
            "  do { sum += *(const u32 *)iph; iph += 4; } while (--n > 0);\n",
            "  return sum;\n",
            "}\n",
            "__int128_t all_compound_forms(__int128_t x, __int128_t y) {\n",
            "  x += y; x -= y; x *= y; x /= y; x %= y;\n",
            "  x <<= 1; x >>= 1; x &= y; x |= y; x ^= y;\n",
            "  return x;\n",
            "}\n",
        ),
        &arena,
    );
}
