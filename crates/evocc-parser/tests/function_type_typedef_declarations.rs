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

fn assert_function_declarations(source: &str, names: &[&str], parameter_count: usize) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|diagnostic| {
        panic!(
            "function declaration list must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    });
    for name in names {
        let function = unit
            .iter()
            .find_map(|declaration| match declaration {
                CHirDecl::Function(function) if function.name.spelling == *name => Some(function),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name} must be declared as a function"));
        assert!(function.body.is_none());
        assert!(matches!(
            &unit.types().get(function.ty).unwrap().kind,
            TypeKind::Function {
                parameters: FunctionParameters::Prototype { types, variadic: false },
                ..
            } if types.len() == parameter_count
        ));
        assert!(!unit.iter().any(|declaration| {
            matches!(declaration, CHirDecl::Object(object) if object.name.spelling == *name)
        }));
    }
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
fn function_type_typedef_declares_every_function_in_a_list() {
    let source = concat!(
        "typedef int fn(int); ",
        "fn a, b; ",
        "extern fn c, d; ",
        "fn *pa, *pb;\n",
    );
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|diagnostic| {
        panic!(
            "function-type declaration lists must parse, got {}: {}",
            diagnostic.code, diagnostic.message
        )
    });
    for name in ["a", "b", "c", "d"] {
        assert!(unit.iter().any(|declaration| {
            matches!(declaration, CHirDecl::Function(function) if function.name.spelling == name && function.body.is_none())
        }));
    }
    for name in ["pa", "pb"] {
        assert!(unit.iter().any(|declaration| {
            matches!(declaration, CHirDecl::Object(object) if object.name.spelling == name)
        }));
    }
}

#[test]
fn written_function_declarators_may_share_a_declaration() {
    assert_function_declarations("int f(void), g(void);\n", &["f", "g"], 0);
}

#[test]
fn a_function_declarator_list_cannot_be_a_definition() {
    let arena = Arena::default();
    assert!(
        parse("int f(void), g(void) { return 0; }\n", &arena).is_err(),
        "a function definition cannot contain a declarator list"
    );
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

#[test]
fn typeof_function_designator_declares_the_same_function_type() {
    let arena = Arena::default();
    let source = "int exported(int value); extern typeof(exported) exported;\n";
    let unit = parse(source, &arena).unwrap_or_else(|diagnostic| {
        panic!(
            "typeof(function) declaration must parse, got {}: {}",
            diagnostic.code, diagnostic.message
        )
    });
    let declarations = unit
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "exported" => Some(function),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(declarations.len(), 2);
    assert!(declarations.iter().all(|function| function.body.is_none()));
    assert!(declarations.iter().all(|function| {
        matches!(
            &unit.types().get(function.ty).unwrap().kind,
            TypeKind::Function {
                parameters: FunctionParameters::Prototype { types, variadic: false },
                ..
            } if types.len() == 1
        )
    }));
}

#[test]
fn typeof_dereferenced_function_pointer_declares_a_function() {
    let source = concat!(
        "int target(int value); ",
        "int (*fp)(int) = target; ",
        "extern typeof(*fp) inherited; ",
        "typeof(*fp) *copy = target;\n",
    );
    assert_function_declarations(source, &["inherited"], 1);
}

#[test]
fn typeof_dereferenced_function_pointer_covers_the_static_call_shape() {
    let source = concat!(
        "struct perf_guest_info_callbacks { unsigned int (*state)(void); }; ",
        "extern struct perf_guest_info_callbacks *perf_guest_cbs; ",
        "extern typeof(*perf_guest_cbs->state) __SCT____perf_guest_state;\n",
    );
    assert_function_declarations(source, &["__SCT____perf_guest_state"], 0);
}

#[test]
fn typeof_function_type_cannot_define_a_function_body() {
    let arena = Arena::default();
    let diagnostic = parse(
        "int declared(int value); typeof(declared) definition { return 0; }\n",
        &arena,
    )
    .expect_err("an inherited function type may not define a body");
    assert_eq!(diagnostic.code, "E1001");
    assert_eq!(diagnostic.message, "expected-declaration");
}
