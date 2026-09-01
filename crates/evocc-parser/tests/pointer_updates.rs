use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_201),
        "pointer-updates.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
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

fn run(source: &str) -> Result<i128, String> {
    let arena = Arena::default();
    let unit = parse(source, &arena)?;
    let outcome =
        interpret_scalar_function(&unit, entry(&unit), InterpreterLimits::new(STEP_BUDGET))
            .map_err(|error| format!("{}: {}", error.code(), error.message()))?;
    outcome
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .map_err(|error| error.to_string())
}

#[test]
fn void_pointer_arithmetic_uses_one_byte_steps_in_every_form() {
    let cases = [
        (
            "compound-add",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a; p += 3; return *(char *)p; }",
            40,
        ),
        (
            "compound-subtract",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a + 4; p -= 2; return *(char *)p; }",
            30,
        ),
        (
            "post-increment",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a; void *old = p++; return *(char *)old * 10 + *(char *)p; }",
            120,
        ),
        (
            "post-decrement",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a + 2; void *old = p--; return *(char *)old * 10 + *(char *)p; }",
            320,
        ),
        (
            "pre-increment",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a; void *now = ++p; return *(char *)now; }",
            20,
        ),
        (
            "pre-decrement",
            "int main(void) { char a[8] = {10,20,30,40,50,60,70,80}; void *p = a + 2; void *now = --p; return *(char *)now; }",
            20,
        ),
        (
            "pointer-difference",
            "int main(void) { char a[8]; void *first = a; void *last = a + 5; return last - first; }",
            5,
        ),
    ];

    let observed = cases
        .into_iter()
        .map(|(name, source, expected)| (name, run(source), expected))
        .collect::<Vec<_>>();
    assert!(
        observed
            .iter()
            .all(|(_, actual, expected)| actual.as_ref() == Ok(expected)),
        "every void-pointer form must use byte scale: {observed:#?}",
    );
}

#[test]
fn direct_pointer_updates_preserve_prefix_postfix_and_scaled_offset_semantics() {
    let source = concat!(
        "int main(void) { ",
        "int a[6] = {10, 20, 30, 40, 50, 60}; int *p = a; ",
        "int first = *p++; int second = *++p; int third = *p--; int fourth = *--p; ",
        "p += 3; int fifth = *p; p -= 2; int sixth = *p; ",
        "return first + second + third + fourth + fifth + sixth; }",
    );
    assert_eq!(run(source), Ok(140));
}

#[test]
fn pointer_members_support_the_same_update_forms_without_double_evaluation() {
    let source = concat!(
        "struct Cursor { int *p; }; ",
        "int main(void) { int a[4] = {10, 20, 30, 40}; struct Cursor cursor; ",
        "cursor.p = a; int first = *cursor.p++; cursor.p += 2; int second = *cursor.p; ",
        "--cursor.p; int third = *cursor.p; return first + second + third; }",
    );
    assert_eq!(run(source), Ok(80));
}

#[test]
fn non_object_or_non_integer_pointer_updates_remain_typed_rejections() {
    for source in [
        "int main(void) { int a[2]; int *p = a; p += 1.5; return 0; }",
        "int f(void) { return 0; } int main(void) { int (*p)(void) = f; ++p; return 0; }",
    ] {
        let arena = Arena::default();
        let error = parse(source, &arena).expect_err("invalid pointer update must be rejected");
        assert_eq!(error, "E2102: invalid-update-operands");
    }
}
