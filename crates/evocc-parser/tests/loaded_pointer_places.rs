use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

const STEP_BUDGET: u64 = 200_000;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_202),
        "loaded-pointer-places.c".to_owned(),
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
fn arrow_chains_rooted_at_loaded_pointer_fields_are_readable_and_writable() {
    let source = concat!(
        "struct Node { int value; struct Node *next; }; ",
        "int main(void) { struct Node a; struct Node b; struct Node c; ",
        "a.next = &b; b.next = &c; b.value = 1; c.value = 7; struct Node *p = &a; ",
        "p->next->value = 5; p->next->next->value += 2; ",
        "return p->next->value * 10 + p->next->next->value; }",
    );
    assert_eq!(run(source), Ok(59));
}

#[test]
fn pointer_fields_and_loaded_dereferences_can_become_new_place_roots() {
    let source = concat!(
        "struct Node { int value; }; struct Holder { struct Node *next; }; ",
        "int main(void) { struct Node node; struct Holder holder; holder.next = &node; ",
        "holder.next->value = 8; struct Node *p = &node; struct Node **pp = &p; ",
        "(*pp)->value += 1; int scalar = 0; int *ip = &scalar; int **ipp = &ip; ",
        "**ipp = 9; return holder.next->value * 10 + **ipp; }",
    );
    assert_eq!(run(source), Ok(99));
}

#[test]
fn a_loaded_pointer_chain_is_traversable_in_a_loop() {
    let source = concat!(
        "struct Node { int value; struct Node *next; }; ",
        "int main(void) { struct Node a; struct Node b; struct Node c; ",
        "a.value = 2; b.value = 3; c.value = 4; a.next = &b; b.next = &c; c.next = 0; ",
        "struct Node *p = &a; int sum = 0; while (p) { sum += p->value; p = p->next; } ",
        "return sum; }",
    );
    assert_eq!(run(source), Ok(9));
}
