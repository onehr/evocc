//! C11 typedef names may denote function types in later declarations.

use evocc_chir::{CHirDecl, CHirUnit, ParameterSyntax};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::{ArrayBound, FunctionParameters, TypeKind};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_307);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn assert_function_declaration(source: &str, name: &str) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|diagnostic| {
        panic!(
            "fixture must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    });
    let function = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function),
            _ => None,
        })
        .expect("function-type typedef use must declare a function");
    assert!(function.body.is_none());
    assert!(function.written.layers.is_empty());
    assert!(matches!(
        &function.parameters,
        ParameterSyntax::Prototype {
            parameters,
            variadic: false,
        } if parameters.is_empty()
    ));
    assert!(matches!(
        &unit.types().get(function.ty).unwrap().kind,
        TypeKind::Function {
            parameters: FunctionParameters::Prototype { types, variadic: false },
            ..
        } if types.is_empty()
    ));
    assert!(!unit.iter().any(|declaration| {
        matches!(declaration, CHirDecl::Object(object) if object.name.spelling == name)
    }));
}

#[test]
fn function_type_typedef_declares_an_extern_function() {
    assert_function_declaration("typedef int fn(void); extern fn f;\n", "f");
}

#[test]
fn parenthesized_function_type_typedef_declares_an_extern_function() {
    assert_function_declaration("typedef int (fn)(void); extern fn f;\n", "f");
}

#[test]
fn function_type_typedef_can_form_the_cohort_pointer_table() {
    let arena = Arena::default();
    let source = concat!(
        "typedef int bool; typedef bool (pstate_check_t)(unsigned long); ",
        "extern pstate_check_t * const aarch32_opcode_cond_checks[16];\n",
    );
    let unit = parse(source, &arena).unwrap_or_else(|diagnostic| {
        panic!(
            "fixture must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    });
    let object = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == "aarch32_opcode_cond_checks" => {
                Some(object)
            }
            _ => None,
        })
        .expect("cohort declaration must remain an object");
    let TypeKind::Array {
        element,
        bound: ArrayBound::Fixed(16),
    } = unit.types().get(object.ty).unwrap().kind
    else {
        panic!("cohort object must be a 16-element array");
    };
    let TypeKind::Pointer(function) = unit.types().get(element).unwrap().kind else {
        panic!("cohort array element must be a function pointer");
    };
    assert!(unit.types().get(element).unwrap().qualifiers.is_const);
    assert!(matches!(
        &unit.types().get(function).unwrap().kind,
        TypeKind::Function {
            parameters: FunctionParameters::Prototype { types, variadic: false },
            ..
        } if types.len() == 1
    ));
}

#[test]
fn function_type_typedef_cannot_define_a_function_body() {
    let arena = Arena::default();
    assert!(
        parse("typedef int fn(void); fn f { return 0; }\n", &arena).is_err(),
        "a function-type typedef may declare but not define a function"
    );
}
