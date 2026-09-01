//! Front-end-resolved attribute queries, variable arguments, and empty records.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_121);
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

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    parse(source, arena).unwrap_or_else(|diagnostic| {
        panic!(
            "fixture must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    })
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

#[test]
fn attribute_queries_are_boolean_constants_from_written_member_attributes() {
    let source = concat!(
        "struct sb { char s_id[32]; ",
        "char s_sysfs_name[32] __attribute__((nonstring)); char *p; }; ",
        "int main(void) { struct sb v; struct sb *q = &v; int side = 0; ",
        "_Static_assert(__builtin_has_attribute(v.s_id, nonstring) == 0, \"plain\"); ",
        "_Static_assert(__builtin_has_attribute(v.s_sysfs_name, nonstring) == 1, \"member\"); ",
        "_Static_assert(__builtin_has_attribute(v.p, nonstring) == 0, \"pointer\"); ",
        "_Static_assert(__builtin_has_attribute(q->s_sysfs_name, nonstring) == 1, \"arrow\"); ",
        "_Static_assert(__builtin_has_attribute(char[4], nonstring) == 0, \"type\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_has_attribute(v.s_id, nonstring)), _Bool), \"bool\"); ",
        "_Static_assert(sizeof(__builtin_has_attribute(v.s_id, nonstring)) == 1, \"size\"); ",
        "int answer = __builtin_has_attribute((side++, v.s_id), nonstring); ",
        "return side + answer * 0; }\n",
    );
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let outcome = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(200_000),
    )
    .expect("the resolved query is executable C-HIR");
    assert_eq!(
        outcome
            .return_value
            .expect("main returns a scalar")
            .mathematical_value(unit.types())
            .expect("return value is integral"),
        0,
        "the query operand must not be evaluated"
    );
}

#[test]
fn attribute_queries_use_the_written_entity_declaration() {
    let source = concat!(
        "char tagged[4] __attribute__((nonstring)); char plain[4]; ",
        "_Static_assert(__builtin_has_attribute(tagged, nonstring) == 1, \"tagged\"); ",
        "_Static_assert(__builtin_has_attribute(plain, nonstring) == 0, \"plain\"); ",
        "_Static_assert(__builtin_has_attribute(char[4], nonstring) == 0, \"type\");\n",
    );
    let arena = Arena::default();
    parse_valid(source, &arena);
}

#[test]
fn variable_argument_builtins_are_retained_with_void_type() {
    let source = concat!(
        "typedef __builtin_va_list va_list; ",
        "int vprint(const char *fmt, va_list ap); ",
        "int ok(const char *fmt, ...) { va_list args, copied; ",
        "__builtin_va_start(args, fmt); __builtin_va_copy(copied, args); ",
        "__builtin_va_end(copied); __builtin_va_end(args); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(__builtin_va_end(args)), void), \"void\"); ",
        "return vprint(fmt, args); }\n",
    );
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let retained = format!("{unit:#?}");
    for operation in ["Start", "Copy", "End"] {
        assert!(
            retained.contains(operation),
            "C-HIR must retain varargs operation {operation}: {retained}"
        );
    }
}

#[test]
fn va_start_requires_a_variadic_function_and_the_last_named_parameter() {
    for (source, expected_span) in [
        (
            "int bad(int first, int last, ...) { __builtin_va_list ap; __builtin_va_start(ap, first); return 0; }\n",
            "first",
        ),
        (
            "int bad(int only) { __builtin_va_list ap; __builtin_va_start(ap, only); return 0; }\n",
            "only",
        ),
    ] {
        let arena = Arena::default();
        let diagnostic = parse(source, &arena).expect_err("va_start context must be rejected");
        assert_eq!(diagnostic.code, "E2926");
        assert_eq!(diagnostic.message, "va-start-outside-variadic-function");
        assert_eq!(
            &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            expected_span
        );
    }
}

#[test]
fn every_variable_argument_object_operand_must_be_builtin_va_list() {
    for source in [
        "int bad(int last, ...) { int ap; __builtin_va_start(ap, last); return 0; }\n",
        "int bad(void) { int ap; __builtin_va_end(ap); return 0; }\n",
        "int bad(void) { __builtin_va_list good; int bad; __builtin_va_copy(good, bad); return 0; }\n",
    ] {
        let arena = Arena::default();
        let diagnostic = parse(source, &arena).expect_err("non-va-list operand must be rejected");
        assert_eq!(diagnostic.code, "E2913", "source: {source}");
        assert_eq!(diagnostic.message, "builtin-operand-not-convertible");
    }
}

#[test]
fn va_arg_remains_outside_the_front_end_profile() {
    let source = "int bad(void) { __builtin_va_list ap; return __builtin_va_arg(ap, int); }\n";
    let arena = Arena::default();
    let diagnostic = parse(source, &arena).expect_err("va_arg is deliberately not admitted");
    assert_eq!(diagnostic.code, "E2001");
    assert_eq!(
        diagnostic.message,
        "undeclared-identifier: __builtin_va_arg"
    );
}

#[test]
fn records_with_only_static_assertions_have_zero_size_and_unit_alignment() {
    let source = concat!(
        "_Static_assert(sizeof(struct { _Static_assert(1, \"\"); }) == 0, \"struct size\"); ",
        "_Static_assert(__alignof__(struct { _Static_assert(1, \"\"); }) == 1, \"struct align\"); ",
        "_Static_assert(sizeof(union { _Static_assert(1, \"\"); }) == 0, \"union size\"); ",
        "_Static_assert(__alignof__(union { _Static_assert(1, \"\"); }) == 1, \"union align\");\n",
    );
    let arena = Arena::default();
    parse_valid(source, &arena);
}
