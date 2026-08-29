//! GNU unqualified typeof semantics fixed by specification 10.1.10.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{IntRank, Qualifiers, TypeId, TypeKind};

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_123);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn type_of(unit: &CHirUnit<'_>, name: &str) -> TypeId {
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is declared"));
    unit.symbols().symbol(symbol).expect("declared symbol").ty()
}

fn diagnostic(source: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    parse(source, &arena).expect_err("fixture must be rejected")
}

fn entry(unit: &CHirUnit<'_>) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.body.is_some() && function.name.spelling == "main" =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main")
}

#[test]
fn scalar_top_level_qualifiers_are_removed_for_both_operand_forms() {
    let arena = Arena::default();
    let unit = parse(
        concat!(
            "const int ci = 0; volatile int vi = 0; const volatile int cvi = 0; ",
            "_Atomic int ai = 0; ",
            "__typeof_unqual__(ci) from_const = 5; ",
            "__typeof_unqual__(vi) from_volatile = 5; ",
            "__typeof_unqual__(cvi) from_both = 5; ",
            "__typeof_unqual__(ai) from_atomic = 5; ",
            "__typeof_unqual__(const int) from_type_name = 5; ",
            "__typeof__(ci) plain_typeof = 5; ",
            "void write_all(void) { from_const = 6; from_volatile = 6; ",
            "from_both = 6; from_atomic = 6; from_type_name = 6; }\n",
        ),
        &arena,
    )
    .expect("unqualified typeof scalars must parse");

    for name in [
        "from_const",
        "from_volatile",
        "from_both",
        "from_atomic",
        "from_type_name",
    ] {
        let canonical = unit.types().get(type_of(&unit, name)).expect("object type");
        assert!(matches!(canonical.kind, TypeKind::Int(IntRank::Int)));
        assert_eq!(canonical.qualifiers, Qualifiers::default(), "{name}");
    }
    assert!(
        unit.types()
            .get(type_of(&unit, "plain_typeof"))
            .expect("plain typeof type")
            .qualifiers
            .is_const,
        "plain typeof must retain the top-level qualifier",
    );
}

#[test]
fn pointer_top_level_qualifiers_are_removed_but_pointee_qualifiers_remain() {
    let arena = Arena::default();
    let unit = parse(
        concat!(
            "int *const pc = 0; const int *cp = 0; const char *const ccp = 0; ",
            "__typeof_unqual__(pc) pc_copy; ",
            "__typeof_unqual__(cp) cp_copy; ",
            "__typeof_unqual__(ccp) ccp_copy; ",
            "__typeof_unqual__(int *restrict) restrict_copy; ",
            "void reset(void) { pc_copy = 0; cp_copy = 0; ccp_copy = 0; restrict_copy = 0; }\n",
        ),
        &arena,
    )
    .expect("unqualified typeof pointers must parse");

    let pointer = |name| {
        let canonical = unit
            .types()
            .get(type_of(&unit, name))
            .expect("pointer type");
        assert_eq!(canonical.qualifiers, Qualifiers::default(), "{name}");
        let TypeKind::Pointer(pointee) = canonical.kind else {
            panic!("{name} is a pointer");
        };
        pointee
    };
    assert!(
        !unit
            .types()
            .get(pointer("pc_copy"))
            .unwrap()
            .qualifiers
            .is_const
    );
    assert!(
        unit.types()
            .get(pointer("cp_copy"))
            .unwrap()
            .qualifiers
            .is_const
    );
    assert!(
        unit.types()
            .get(pointer("ccp_copy"))
            .unwrap()
            .qualifiers
            .is_const
    );
    assert!(
        !unit
            .types()
            .get(pointer("restrict_copy"))
            .unwrap()
            .qualifiers
            .is_restrict
    );
}

#[test]
fn a_const_record_member_remains_const_after_outer_qualification_is_removed() {
    let error = diagnostic(concat!(
        "struct s { const int m; }; const struct s sv = { 1 }; ",
        "__typeof_unqual__(sv) w = { 2 }; void mutate(void) { w.m = 3; }\n",
    ));
    assert_eq!(error.code, "E2101");
}

#[test]
fn array_element_qualification_is_kept_in_the_refusal_direction() {
    let error = diagnostic(concat!(
        "const int arr[3] = { 1, 2, 3 }; ",
        "__typeof_unqual__(arr) w = { 0 }; void mutate(void) { w[0] = 9; }\n",
    ));
    assert_eq!(error.code, "E2101");
}

#[test]
fn expression_operand_is_unevaluated() {
    let arena = Arena::default();
    let unit = parse(
        "int main(void) { int side = 0; (void)sizeof(__typeof_unqual__(side++)); return side; }\n",
        &arena,
    )
    .expect("unqualified typeof expression must parse");
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .expect("fixture executes");
    assert_eq!(
        outcome
            .return_value
            .expect("main returns a value")
            .mathematical_value(unit.types())
            .expect("integer result"),
        0,
    );
}

#[test]
fn only_the_double_underscore_spelling_is_an_operator() {
    let mut exact = Lexer::new(FileId::new(10_124), "__typeof_unqual__");
    assert_ne!(
        exact.next().expect("exact spelling token").kind,
        TokenKind::Identifier,
        "the admitted spelling must be recognized as an operator",
    );

    let mut c23 = Lexer::new(FileId::new(10_125), "typeof_unqual");
    assert_eq!(
        c23.next().expect("C23 spelling token").kind,
        TokenKind::Identifier,
    );
    let arena = Arena::default();
    assert!(
        parse("int x; typeof_unqual(x) w;\n", &arena).is_err(),
        "the C23 keyword is not admitted under this GNU C11 profile",
    );
}
