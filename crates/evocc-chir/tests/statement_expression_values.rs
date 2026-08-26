use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_114);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected {source:?}: {:?}",
        parser.diagnostic()
    );
    unit
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function {name}"))
}

fn evaluate(source: &str) -> i128 {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let outcome = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(100_000),
    )
    .unwrap_or_else(|error| panic!("interpreter rejected {source:?}: {error:?}"));
    outcome
        .return_value
        .expect("main returns a scalar")
        .mathematical_value(unit.types())
        .expect("integer result")
}

#[test]
fn final_expression_and_nested_values_are_observable() {
    assert_eq!(
        evaluate("int main(void) { return ({ int t = 1; t + 1; }); }\n"),
        2
    );
    assert_eq!(
        evaluate("int main(void) { return ({ ({ 1; }) + ({ 2; }); }); }\n"),
        3
    );
}

#[test]
fn break_leaves_the_statement_expression_and_its_enclosing_loop() {
    assert_eq!(
        evaluate(
            "int main(void) { int sum = 0; for (int i = 0; i < 3; ++i) {\n\
               sum += ({ if (i == 1) break; 10; });\n\
             } return sum; }\n"
        ),
        10
    );
}

#[test]
fn reentering_a_block_scope_literal_allocates_a_fresh_object() {
    assert_eq!(
        evaluate(
            "int main(void) { int total = 0; for (int i = 0; i < 3; ++i) {\n\
               int *p = &(int){ 0 }; if (*p != 0) return 99; *p = 7; total += *p;\n\
             } return total; }\n"
        ),
        21
    );
}

#[test]
fn const_file_scope_literal_rejects_a_store_with_e3014() {
    let source = "const int *p = &(const int){ 1 }; int main(void) { *(int *)p = 2; return 0; }\n";
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let error = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(100_000),
    )
    .expect_err("store through a const file-scope compound literal must fail");
    assert_eq!(error.code(), "E3014");
    assert_eq!(error.message(), "write-to-read-only-object");
}
