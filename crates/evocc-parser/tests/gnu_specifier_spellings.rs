use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_204);
    let mut lexer = Lexer::new(file, source);
    let mut input = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        input.push(token);
    }
    let mut preprocessor = Preprocessor::new();
    let tokens = preprocessor
        .process(TokenStream::new(input), source)
        .into_vec();
    assert!(
        preprocessor.errors().is_empty(),
        "fixture must preprocess: {:?}",
        preprocessor.errors()
    );
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!(
            "GNU spelling must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

#[test]
fn pragma_operator_is_valid_at_file_scope_and_after_macro_expansion() {
    accepts(concat!(
        "#define PUSH() _Pragma(\"GCC diagnostic push\")\n",
        "_Pragma(\"GCC diagnostic ignored \\\"-Wmissing-prototypes\\\"\")\n",
        "PUSH()\n",
        "int object;\n",
    ));
}

#[test]
fn inline_and_restrict_aliases_are_keywords() {
    accepts(concat!(
        "static __inline int first(int *__restrict input) { return *input; } ",
        "static __inline__ int second(int *__restrict__ input) { return *input; }\n",
    ));
}

#[test]
fn attributes_may_separate_declaration_specifiers() {
    accepts(concat!(
        "static const __attribute__((unused)) struct record { int value; } objects[] = { { 1 } }; ",
        "unsigned __attribute__((unused)) long counter; ",
        "static __attribute__((unused)) int flag;\n",
    ));
}

#[test]
fn noreturn_function_specifiers_preserve_their_written_form() {
    let arena = Arena::default();
    let unit = parse(
        "_Noreturn void stop(void); void _Noreturn again(void);",
        &arena,
    )
    .expect("C11 function specifier must parse");
    for declaration in unit.iter() {
        let evocc_chir::CHirDecl::Function(function) = declaration else {
            panic!("function declaration");
        };
        assert!(function
            .attributes
            .iter()
            .any(|a| a.name == "noreturn" && a.written == "_Noreturn"));
    }
    accepts("_Noreturn void stop(void){for(;;){}} int main(void){return 0;}");
    accepts("void f(void){_Noreturn void stop(void);}");
}

#[test]
fn noreturn_does_not_apply_to_objects_or_typedefs() {
    for source in ["_Noreturn int object;", "typedef _Noreturn void fn(void);"] {
        let arena = Arena::default();
        assert_eq!(parse(source, &arena).unwrap_err().code, "E2934", "{source}");
    }
}
