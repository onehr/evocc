//! Parser unit tests.

use super::*;
use evocc_lex::Lexer;

fn parse_unit<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let mut lexer = Lexer::new(FileId::new(99), source);
    let mut tokens = Vec::new();
    while let Some(tok) = lexer.next() {
        tokens.push(tok);
    }
    let mut parser = Parser::new(tokens, FileId::new(99), source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    unit
}

fn parse_expr<'arena>(source: &str, arena: &'arena Arena) -> &'arena CHirExpr<'arena> {
    let wrapped = format!("int sink = {source}");
    let unit = parse_unit(&wrapped, arena);
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("expected initialized object")
    };
    object
        .initializer
        .as_ref()
        .expect("expected initializer")
        .written()
        .expression()
        .expect("expression initializer")
}

fn parse_rejected<'arena>(
    source: &str,
    arena: &'arena Arena,
) -> (CHirUnit<'arena>, ParseDiagnostic) {
    let mut lexer = Lexer::new(FileId::new(99), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, FileId::new(99), source);
    let unit = parser.parse_translation_unit_in(arena);
    let diagnostic = parser
        .diagnostic()
        .cloned()
        .expect("malformed input must produce one diagnostic");
    (unit, diagnostic)
}

fn diagnostic_text<'src>(source: &'src str, diagnostic: &ParseDiagnostic) -> &'src str {
    &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize]
}

#[test]
fn parser_recognizes_declarators() {
    let arena = Arena::default();
    let unit = parse_unit("int x;", &arena);
    assert!(!unit.is_empty());
    match unit.declarations()[0] {
        CHirDecl::Object(object) => {
            assert_eq!(object.name.spelling, "x");
            assert!(matches!(
                unit.types().get(object.ty).map(|ty| &ty.kind),
                Some(TypeKind::Int(IntRank::Int))
            ));
            assert!(object.initializer.is_none());
        }
        _ => panic!("expected VarDecl, got {:?}", unit[0]),
    }
}

#[test]
fn parser_recognizes_declaration_with_init() {
    let arena = Arena::default();
    let unit = parse_unit("int y = 42;", &arena);
    assert!(!unit.is_empty());
    match unit.declarations()[0] {
        CHirDecl::Object(object) => {
            assert_eq!(object.name.spelling, "y");
            assert!(matches!(
                unit.types().get(object.ty).map(|ty| &ty.kind),
                Some(TypeKind::Int(IntRank::Int))
            ));
            assert!(object.initializer.is_some());
        }
        _ => panic!("expected VarDecl"),
    }
}

#[test]
fn gnu_empty_initializer_completes_an_unbounded_array() {
    let arena = Arena::default();
    let unit = parse_unit("int inferred[] = {};", &arena);
    for declaration in unit.declarations() {
        let CHirDecl::Object(object) = declaration else {
            panic!("expected object declaration")
        };
        assert!(matches!(
            unit.types().get(object.ty).map(|ty| &ty.kind),
            Some(TypeKind::Array {
                bound: ArrayBound::Fixed(0),
                ..
            })
        ));
        assert_eq!(
            unit.symbols().symbol(object.symbol).unwrap().ty(),
            object.ty
        );
        let initializer = object
            .initializer
            .as_ref()
            .expect("written empty initializer");
        assert!(initializer
            .written()
            .elements()
            .is_some_and(|elements| elements.is_empty()));
        assert!(initializer.plan().is_empty());
        initializer.plan().validate(unit.types()).unwrap();
    }
}

#[test]
fn a_written_zero_bound_remains_fixed_with_an_empty_initializer() {
    let arena = Arena::default();
    let unit = parse_unit("int fixed[0] = {};", &arena);
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("expected zero-length object declaration")
    };
    assert!(matches!(
        unit.types().get(object.ty).map(|ty| &ty.kind),
        Some(TypeKind::Array {
            bound: ArrayBound::Fixed(0),
            ..
        })
    ));
    assert!(object
        .initializer
        .as_ref()
        .expect("written empty initializer")
        .plan()
        .is_empty());
}

#[test]
fn initialized_integer_constant_array_bounds_remain_fixed() {
    let arena = Arena::default();
    let unit = parse_unit(
        "enum { N = 2 }; int sized[sizeof(int)] = { 1 }; \
             int named[N] = { 1 }; int folded[1 + 1] = { 1 };",
        &arena,
    );
    let bounds = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) => match &unit.types().get(object.ty)?.kind {
                TypeKind::Array { bound, .. } => Some(bound.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        bounds,
        [
            ArrayBound::Fixed(4),
            ArrayBound::Fixed(2),
            ArrayBound::Fixed(2),
        ]
    );

    let arena = Arena::default();
    let source = "int f(int n) { int values[2][n] = { { 1 } }; return 0; }";
    let (_, diagnostic) = parse_rejected(source, &arena);
    assert_eq!(diagnostic.code, "E2203");
    assert_eq!(diagnostic.message, "variable-length-object");
    assert_eq!(
        &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
        "{ { 1 } }"
    );
}

#[test]
fn adjacent_string_fragments_and_ucns_infer_execution_unit_bounds() {
    let arena = Arena::default();
    let source = r#"char split[] = "\x1" "2"; char ucn[] = "\u00e9";"#;
    let unit = parse_unit(source, &arena);
    for (declaration, expected_value, expected_fragments) in [
        (unit.declarations()[0], "\\x12", vec!["\\x1", "2"]),
        (unit.declarations()[1], "\\u00e9", vec!["\\u00e9"]),
    ] {
        let CHirDecl::Object(object) = declaration else {
            panic!("expected character array")
        };
        assert!(matches!(
            unit.types().get(object.ty).map(|ty| &ty.kind),
            Some(TypeKind::Array {
                bound: ArrayBound::Fixed(3),
                ..
            })
        ));
        let expression = object
            .initializer
            .as_ref()
            .expect("string initializer")
            .written()
            .expression()
            .expect("string expression");
        let CHirExpr::StringLit(literal, _) = expression else {
            panic!("expected string literal")
        };
        assert_eq!(literal.value, expected_value);
        assert_eq!(
            literal.fragments,
            expected_fragments
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
        assert_eq!(literal.explicit_unit_count(), Some(2));
    }
}

#[test]
fn malformed_string_units_fail_with_a_literal_diagnostic() {
    let arena = Arena::default();
    for source in [
        r#"char value[] = "\x";"#,
        r#"char value[] = "\u0061";"#,
        r#"char value[] = "\uD800";"#,
    ] {
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert!(unit.is_empty());
        assert_eq!(diagnostic.code, "E1006");
        assert_eq!(diagnostic.message, "invalid-string-literal");
        assert_eq!(
            &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            source
                .split('=')
                .nth(1)
                .unwrap()
                .trim_end_matches(';')
                .trim(),
        );
    }
}

#[test]
fn forbidden_typedef_and_block_extern_initializers_rollback_bindings() {
    for (source, rejected) in [
        ("int keep; typedef int bad = 1; long after;", "bad"),
        (
            "int keep; int bad(void) { extern int local = 1; return local; } long after;",
            "bad",
        ),
    ] {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2202");
        assert_eq!(diagnostic.message, "invalid-initializer-type");
        assert_eq!(
            &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
            "1"
        );
        assert_eq!(
            unit.symbols()
                .symbols_in_source_order()
                .iter()
                .map(|symbol| symbol.spelling())
                .collect::<Vec<_>>(),
            ["keep", "after"]
        );
        assert_eq!(
            unit.symbols()
                .resolve(unit.symbols().file_scope(), rejected),
            Ok(None)
        );
    }

    let arena = Arena::default();
    let unit = parse_unit("extern int definition = 1;", &arena);
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("expected file-scope extern definition")
    };
    assert_eq!(object.storage, StorageClass::Extern);
    assert!(object.initializer.is_some());
}

#[test]
fn static_storage_initializers_validate_the_source_constant_expression() {
    let arena = Arena::default();
    let unit = parse_unit(
        "int anchor; int scalar = 1.5; enum E { A }; enum E e = 1.5; \
             _Bool truth = &anchor; int *address = (int *)1; \
             double maybe(void); double dead = 1 ? 2.0 : maybe(); \
             int ordinary; double shorted = 0 && ordinary; int folded = 1 + 2; \
             int local(int n) { static int stable = 1 + 2; int automatic = n; return automatic; }",
        &arena,
    );
    assert!(unit.declarations().len() >= 10);

    for (source, offending) in [
        ("int keep; int value; int bad = value; long after;", "value"),
        (
            "int keep; int call(void); int bad = call(); long after;",
            "call()",
        ),
        (
            "int keep; int bad(int n) { static int local = n; return local; } long after;",
            "n",
        ),
        (
            "int keep; _Thread_local int tls; int *bad = &tls; long after;",
            "&tls",
        ),
    ] {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2208");
        assert_eq!(diagnostic.message, "non-constant-static-initializer");
        assert_eq!(diagnostic_text(source, &diagnostic), offending);
        assert_eq!(
            unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
            Ok(None)
        );
        assert!(unit
            .symbols()
            .resolve(unit.symbols().file_scope(), "after")
            .unwrap()
            .is_some());
    }
}

#[test]
fn failed_declaration_cannot_leak_a_const_initializer_into_reused_symbols() {
    let arena = Arena::default();
    let (unit, _) = parse_rejected(
        "const int leaked = 7, broken = ; extern const int replacement; \
             int bad = replacement; int after;",
        &arena,
    );
    let scope = unit.symbols().file_scope();
    for name in ["leaked", "broken", "bad"] {
        assert_eq!(unit.symbols().resolve(scope, name), Ok(None), "{name}");
    }
    assert!(unit
        .symbols()
        .resolve(scope, "replacement")
        .unwrap()
        .is_some());
    assert!(unit.symbols().resolve(scope, "after").unwrap().is_some());
}

#[test]
fn storage_context_restrictions_are_transactional() {
    for (source, offending) in [
        ("int keep; auto int bad = 1; long after;", "bad"),
        (
            "int keep; int bad(void) { _Thread_local int local = 1; return local; } long after;",
            "local",
        ),
    ] {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E2002");
        assert_eq!(diagnostic_text(source, &diagnostic), offending);
        assert_eq!(
            unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
            Ok(None)
        );
        assert!(unit
            .symbols()
            .resolve(unit.symbols().file_scope(), "after")
            .unwrap()
            .is_some());
    }

    let arena = Arena::default();
    let unit = parse_unit(
        "_Thread_local int tls = 1; extern int definition = 1;",
        &arena,
    );
    assert_eq!(unit.declarations().len(), 2);
}

#[test]
fn initializer_uses_the_effective_composite_redeclaration_type() {
    let arena = Arena::default();
    let unit = parse_unit(
        "extern int values[3]; int values[] = { 1, 2 }; \
             extern int other[3]; extern int other[];",
        &arena,
    );
    let values = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == "values" => Some(object),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].symbol, values[1].symbol);
    assert_eq!(values[0].ty, values[1].ty);
    assert!(matches!(
        unit.types().get(values[1].ty).map(|ty| &ty.kind),
        Some(TypeKind::Array {
            bound: ArrayBound::Fixed(3),
            ..
        })
    ));
    let plan = values[1].initializer.as_ref().unwrap().plan();
    assert_eq!(plan.actions().len(), 2);
    assert!(plan.zero_fills_unwritten());

    let other = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == "other" => Some(object),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(other.len(), 2);
    assert_eq!(other[0].symbol, other[1].symbol);
    assert_eq!(other[0].ty, other[1].ty);

    for (source, code, offending) in [
        (
            "extern int values[2]; int values[] = { 1, 2, 3 }; long after;",
            "E2201",
            "3",
        ),
        (
            "extern char text[2]; char text[] = \"abc\"; long after;",
            "E2204",
            "\"abc\"",
        ),
    ] {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, code);
        assert_eq!(diagnostic_text(source, &diagnostic), offending);
        let symbol = unit
            .symbols()
            .resolve(
                unit.symbols().file_scope(),
                if code == "E2201" { "values" } else { "text" },
            )
            .unwrap()
            .expect("the prior complete declaration remains");
        assert!(matches!(
            unit.types()
                .get(unit.symbols().symbol(symbol).unwrap().ty())
                .map(|ty| &ty.kind),
            Some(TypeKind::Array {
                bound: ArrayBound::Fixed(2),
                ..
            })
        ));
        assert!(unit
            .symbols()
            .resolve(unit.symbols().file_scope(), "after")
            .unwrap()
            .is_some());
    }
}

#[test]
fn initializer_plan_size_tracks_written_entries_and_keeps_string_stores_compact() {
    let arena = Arena::default();
    let unit = parse_unit(
        "char direct[1000000] = \"x\"; \
             struct S { char text[1000000]; int tail; }; \
             struct S nested = { \"x\", 1 }; \
             char rows[][1000000] = { \"x\" };",
        &arena,
    );
    let action_counts = unit
        .declarations()
        .iter()
        .filter_map(|declaration| match declaration {
            CHirDecl::Object(object) => object
                .initializer
                .as_ref()
                .map(|initializer| initializer.plan().actions().len()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(action_counts, [1, 2, 1]);

    let arena = Arena::default();
    let unit = parse_unit("int huge[65537] = { 1 }; long after;", &arena);
    let CHirDecl::Object(huge) = unit.declarations()[0] else {
        panic!("expected the large array declaration")
    };
    let plan = huge.initializer.as_ref().unwrap().plan();
    assert_eq!(plan.actions().len(), 1);
    assert!(plan.zero_fills_unwritten());
}

#[test]
fn record_completion_rejects_nonfinal_flexible_and_recursive_members() {
    for (source, offending) in [
        (
            "struct Bad { int values[]; int tail; }; long after;",
            "values[]",
        ),
        (
            "union Bad { int value; union Bad nested; }; long after;",
            "nested",
        ),
    ] {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_rejected(source, &arena);
        assert_eq!(diagnostic.code, "E1005");
        assert_eq!(diagnostic.message, "invalid-declarator");
        assert_eq!(diagnostic_text(source, &diagnostic), offending);
        assert!(unit
            .symbols()
            .resolve(unit.symbols().file_scope(), "after")
            .unwrap()
            .is_some());
    }

    let arena = Arena::default();
    let unit = parse_unit(
        "struct Flexible { int values[]; }; \
             union GNUFlexible { int values[]; int scalar; }; \
             struct Clean { int; int value; }; struct Clean clean = { 7 };",
        &arena,
    );
    let clean = unit
        .declarations()
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == "clean" => Some(object),
            _ => None,
        })
        .unwrap();
    let TypeKind::Struct(tag) = unit.types().get(clean.ty).unwrap().kind else {
        panic!("expected struct object")
    };
    let TagState::Complete(TagDefinition::Struct(fields)) = &unit.types().tag(tag).unwrap().state
    else {
        panic!("expected complete struct")
    };
    assert_eq!(
        fields.len(),
        1,
        "empty builtin member declaration leaked a field"
    );
    assert_eq!(
        clean.initializer.as_ref().unwrap().plan().actions()[0]
            .path()
            .steps(),
        [SubobjectStep::Field { field_index: 0 }]
    );

    let arena = Arena::default();
    let source = "union Flexible { int values[]; int scalar; }; union Flexible bad = { 1 };";
    let (_, diagnostic) = parse_rejected(source, &arena);
    assert_eq!(diagnostic.code, "E2706");
    assert_eq!(diagnostic_text(source, &diagnostic), "1");
}

#[test]
fn atomic_character_arrays_do_not_take_the_string_special_case() {
    let arena = Arena::default();
    let source = "int keep; _Atomic char bad[2] = \"x\"; long after;";
    let (unit, diagnostic) = parse_rejected(source, &arena);
    assert_eq!(diagnostic.code, "E2202");
    assert_eq!(diagnostic.message, "invalid-initializer-type");
    assert_eq!(diagnostic_text(source, &diagnostic), "\"x\"");
    assert_eq!(
        unit.symbols().resolve(unit.symbols().file_scope(), "bad"),
        Ok(None)
    );

    let arena = Arena::default();
    let unit = parse_unit("_Atomic char values[2] = { 'x' };", &arena);
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("expected atomic character array")
    };
    let plan = object.initializer.as_ref().unwrap().plan();
    assert_eq!(plan.actions().len(), 1);
    assert!(plan.zero_fills_unwritten());
}

#[test]
fn parser_builds_expression_tree() {
    let arena = Arena::default();
    let unit = parse_unit("int r = 1 + 2 * 3;", &arena);
    assert!(!unit.is_empty());
    match unit.declarations()[0] {
        CHirDecl::Object(ObjectDecl {
            initializer: Some(init),
            ..
        }) => match init.written().expression().expect("expression initializer") {
            CHirExpr::Binary(BinaryOp::Add, lhs, rhs, _) => {
                assert!(matches!(lhs, CHirExpr::Lit(_, _)));
                assert!(matches!(rhs, CHirExpr::Binary(BinaryOp::Mul, _, _, _)));
            }
            _ => panic!("expected Add at root with Mul as right child"),
        },
        _ => panic!("expected declaration with initializer"),
    }
}

#[test]
fn parser_builds_function_definition() {
    let arena = Arena::default();
    let unit = parse_unit("int f(int x) { return x + 1; }", &arena);
    assert_eq!(unit.len(), 1);
    match unit.declarations()[0] {
        CHirDecl::Function(function) => {
            assert_eq!(function.name.spelling, "f");
            let ParameterSyntax::Prototype { parameters, .. } = &function.parameters else {
                panic!("expected function prototype parameters")
            };
            assert_eq!(parameters.len(), 1);
            assert_eq!(parameters[0].name.as_ref().unwrap().spelling, "x");
            assert!(matches!(
                unit.types()
                    .get(function.return_specifier.type_id())
                    .map(|ty| &ty.kind),
                Some(TypeKind::Int(IntRank::Int))
            ));
            match function.body.expect("expected function body") {
                CHirStmt::Compound(stmts, _, _) => {
                    assert!(!stmts.is_empty());
                    assert!(matches!(
                        stmts[0],
                        CHirBlockItem::Statement(CHirStmt::Return(Some(_), _))
                    ));
                }
                _ => panic!("expected compound statement body"),
            }
        }
        _ => panic!("expected function declaration"),
    }
}

#[test]
fn parser_handles_parenthesized_expressions() {
    let arena = Arena::default();
    let expr = parse_expr("(1 + 2) * 3;", &arena);
    match expr {
        CHirExpr::Binary(BinaryOp::Mul, lhs, rhs, _) => {
            assert!(matches!(*lhs, CHirExpr::Binary(BinaryOp::Add, _, _, _)));
            assert!(matches!(*rhs, CHirExpr::Lit(_, _)));
        }
        _ => panic!("expected Mul of (Add) * Lit"),
    }
}

#[test]
fn parser_handles_comparison_operators() {
    let arena = Arena::default();
    let expr = parse_expr("1 < 2;", &arena);
    assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Lt, _, _, _)));
}

#[test]
fn parser_handles_equality_operators() {
    let arena = Arena::default();
    let expr = parse_expr("1 == 2;", &arena);
    assert!(matches!(expr, CHirExpr::Binary(BinaryOp::Eq, _, _, _)));
}

#[test]
fn parser_handles_unary_negation() {
    let arena = Arena::default();
    let expr = parse_expr("-42;", &arena);
    match expr {
        CHirExpr::Unary(UnaryOp::Neg, inner, _) => {
            assert!(matches!(*inner, CHirExpr::Lit(_, _)));
        }
        _ => panic!("expected unary negation"),
    }
}

#[test]
fn parser_handles_assignment() {
    let arena = Arena::default();
    let unit = parse_unit("int sink; void f(void) { sink = 5; }", &arena);
    let CHirDecl::Function(function) = unit.declarations()[1] else {
        panic!("expected function")
    };
    let CHirStmt::Compound(items, _, _) = function.body.expect("expected function body") else {
        panic!("expected compound body")
    };
    let CHirBlockItem::Statement(CHirStmt::Expr(expr, _)) = items[0] else {
        panic!("expected assignment expression statement")
    };
    match expr {
        CHirExpr::Assign {
            op,
            target,
            value,
            facts,
            ..
        } => {
            assert_eq!(*op, AssignmentOp::Simple);
            assert!(matches!(*target, CHirExpr::Ident { .. }));
            assert!(matches!(*value, CHirExpr::Lit(_, _)));
            assert_eq!(facts.category(), evocc_chir::ExprCategory::Value);
        }
        _ => panic!("expected assignment"),
    }
}

#[test]
fn parser_empty_statement() {
    let arena = Arena::default();
    let unit = parse_unit("void f(void) { ; ; }", &arena);
    assert_eq!(unit.len(), 1);
    match unit.declarations()[0] {
        CHirDecl::Function(function) => match function.body.expect("expected body") {
            CHirStmt::Compound(stmts, _, _) => {
                assert_eq!(stmts.len(), 2);
                assert!(matches!(
                    stmts[0],
                    CHirBlockItem::Statement(CHirStmt::Null(_))
                ));
            }
            _ => panic!("expected compound"),
        },
        _ => panic!("expected function"),
    }
}

#[test]
fn misplaced_statement_keywords_have_context_diagnostics() {
    let cases = [
        ("break;", "break", "E1201", "break-outside-loop-or-switch"),
        ("continue;", "continue", "E1202", "continue-outside-loop"),
        ("case 1: ;", "case", "E1203", "case-outside-switch"),
        ("default: ;", "default", "E1204", "default-outside-switch"),
        (
            "int f(void) { break; }",
            "break",
            "E1201",
            "break-outside-loop-or-switch",
        ),
        (
            "int f(void) { continue; }",
            "continue",
            "E1202",
            "continue-outside-loop",
        ),
        (
            "int f(void) { case 1: ; }",
            "case",
            "E1203",
            "case-outside-switch",
        ),
        (
            "int f(void) { default: ; }",
            "default",
            "E1204",
            "default-outside-switch",
        ),
    ];

    for (source, keyword, code, message) in cases {
        let arena = Arena::default();
        let (_, diagnostic) = parse_rejected(source, &arena);
        let start = source.find(keyword).expect("keyword in source") as u32;
        assert_eq!(diagnostic.code, code, "source {source:?}");
        assert_eq!(diagnostic.message, message, "source {source:?}");
        assert_eq!(diagnostic.span.file, FileId::new(99));
        assert_eq!(diagnostic.span.byte_start, start, "source {source:?}");
        assert_eq!(
            diagnostic.span.byte_end,
            start + keyword.len() as u32,
            "source {source:?}"
        );
    }
}

#[test]
fn malformed_expression_does_not_fabricate_a_chir_declaration() {
    let arena = Arena::default();
    let (unit, diagnostic) = parse_rejected("int broken = !;", &arena);
    assert_eq!(diagnostic.code, "E1003");
    assert_eq!(diagnostic.message, "expected-expression");
    assert!(
        unit.is_empty(),
        "a rejected expression must not survive in reachable C-HIR: {unit:?}"
    );
}

#[test]
fn failed_declaration_rolls_back_enumerator_bindings() {
    let source = "enum { MUST_NOT_LEAK = 7 } broken trailing;";
    let mut lexer = Lexer::new(FileId::new(99), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let arena = Arena::default();
    let mut parser = Parser::new(tokens, FileId::new(99), source);
    let unit = parser.parse_translation_unit_in(&arena);

    assert!(parser.had_error());
    assert!(unit.is_empty());
    assert!(
        unit.symbols()
            .resolve(unit.symbols().file_scope(), "MUST_NOT_LEAK")
            .unwrap()
            .is_none(),
        "a rejected declaration must roll back ordinary-name bindings"
    );
}

#[test]
fn span_composition_never_mixes_file_coordinate_systems() {
    let parser = Parser::new(Vec::new(), FileId::new(99), "");
    let header_start = SourceSpan::new(FileId::new(701), 4, 9, 2, 3);
    let header_end = SourceSpan::new(FileId::new(701), 20, 24, 3, 7);
    assert_eq!(
        parser.merge_span(header_start, header_end),
        SourceSpan::new(FileId::new(701), 4, 24, 2, 3)
    );

    let main_end = SourceSpan::new(FileId::new(702), 40, 44, 5, 2);
    assert_eq!(
        parser.merge_span(header_start, main_end),
        header_start,
        "cross-file endpoints must select the start token as primary origin"
    );
}
