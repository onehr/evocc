//! Consumer-side integrity checks for front-end-resolved builtin records.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, FrontEndBuiltinRecord, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn function_symbol(unit: &evocc_chir::CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

#[test]
fn consumers_reject_a_forged_attribute_query_result() {
    let source = concat!(
        "struct s { char text[4] __attribute__((nonstring)); }; ",
        "int main(void) { struct s value; return ",
        "__builtin_has_attribute(value.text, nonstring); }\n",
    );
    let arena = Arena::default();
    let file = FileId::new(10_122);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let mut unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "fixture must parse");

    let mut records = unit.front_end_builtins().to_vec();
    let forged = records
        .iter_mut()
        .find_map(|record| match record {
            FrontEndBuiltinRecord::HasAttribute { result, .. } => Some(result),
            _ => None,
        })
        .expect("fixture retains an attribute query");
    *forged = !*forged;
    unit.set_front_end_builtins(records);

    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert!(
        evocc_chir::validate_front_end_builtin_records(&unit, &target).is_err(),
        "the consumer must recompute the query instead of trusting its retained bit: {:#?}",
        unit.front_end_builtins(),
    );

    let interpreter = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(10_000),
    )
    .expect_err("the interpreter must reject the forged producer claim");
    assert_eq!(interpreter.code(), "E3007");
    assert_eq!(
        interpreter.message(),
        "unsupported-interpreter-expression: inconsistent front-end builtin record"
    );

    let lowering = lower_unit(&unit, &target)
        .expect_err("lowering must reject the forged producer claim")
        .to_string();
    assert!(
        lowering.contains("inconsistent front-end builtin record"),
        "unexpected lowering diagnostic: {lowering}"
    );
}

#[test]
fn consumers_recompute_declared_object_bounds() {
    let source = "int main(void){char object[16];return __builtin_object_size(object,1);}";
    let arena = Arena::default();
    let file = FileId::new(10_123);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let mut unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let mut records = unit.front_end_builtins().to_vec();
    let FrontEndBuiltinRecord::ObjectSize { result, .. } = &mut records[0] else {
        panic!("fixture retains the object-size query");
    };
    assert_eq!(*result, 16);
    *result = 99;
    unit.set_front_end_builtins(records);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    assert!(evocc_chir::validate_front_end_builtin_records(&unit, &target).is_err());
    assert!(interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(10_000),
    )
    .is_err());
    assert!(lower_unit(&unit, &target).is_err());
}
