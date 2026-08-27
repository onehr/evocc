use std::collections::HashSet;

use evocc_chir::{
    BinaryOp, CHirBlockItem, CHirDecl, CHirExpr, CHirForInit, CHirStmt, InitAction, TagMembers,
    TypeSpecifier,
};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::TagState;

struct ShapeCase {
    id: &'static str,
    source: &'static str,
    expected: &'static str,
    expected_spans: &'static [(&'static str, u32, u32)],
}

const FILE_RAW: u32 = 43;

const SHAPE_CASES: &[ShapeCase] = &[
    ShapeCase {
        id: "compound.block_item_order",
        source: "int mixed(int input) { int local = input; local = local + 1; ; return local; }",
        expected: "fn:mixed=block[decl:object(local=id(input)),stmt:expr(assign(id(local),binary(add,id(local),int(1)))),stmt:null,stmt:return(id(local))]",
        expected_spans: &[
            ("compound", 21, 78),
            ("expression", 42, 60),
            ("null", 61, 62),
            ("return", 63, 76),
        ],
    },
    ShapeCase {
        id: "selection.dangling_else_binds_innermost",
        source: "int choose(int a, int b) { if (a) if (b) return 1; else return 2; return 0; }",
        expected: "fn:choose=block[stmt:if(id(a),if(id(b),return(int(1)),return(int(2))),none),stmt:return(int(0))]",
        expected_spans: &[
            ("compound", 25, 77),
            ("if", 27, 65),
            ("if", 34, 65),
            ("return", 41, 50),
            ("return", 56, 65),
            ("return", 66, 75),
        ],
    },
    ShapeCase {
        id: "iteration.while_do_break_continue",
        source: "int loops(int n) { while (n) { if (n) continue; break; } do n = n - 1; while (n); return n; }",
        expected: "fn:loops=block[stmt:while(id(n),block[stmt:if(id(n),continue,none),stmt:break]),stmt:do(expr(assign(id(n),binary(sub,id(n),int(1)))),id(n)),stmt:return(id(n))]",
        expected_spans: &[
            ("compound", 17, 93),
            ("while", 19, 56),
            ("compound", 29, 56),
            ("if", 31, 47),
            ("continue", 38, 47),
            ("break", 48, 54),
            ("do-while", 57, 81),
            ("expression", 60, 70),
            ("return", 82, 91),
        ],
    },
    ShapeCase {
        id: "iteration.all_for_initializers",
        source: "int fors(int n) { int i; for (;;) break; for (i = 0; i < n; i = i + 1) ; for (int j = 0; j < n; j = j + 1) continue; return i; }",
        expected: "fn:fors=block[decl:object(i=-),stmt:for(empty,-,-,break),stmt:for(expr:assign(id(i),int(0)),binary(lt,id(i),id(n)),assign(id(i),binary(add,id(i),int(1))),null),stmt:for(decl:object(j=int(0)),binary(lt,id(j),id(n)),assign(id(j),binary(add,id(j),int(1))),continue),stmt:return(id(i))]",
        expected_spans: &[
            ("compound", 16, 128),
            ("for", 25, 40),
            ("break", 34, 40),
            ("for", 41, 72),
            ("null", 71, 72),
            ("for", 73, 116),
            ("continue", 107, 116),
            ("return", 117, 126),
        ],
    },
    ShapeCase {
        id: "selection.switch_stacked_labels",
        source: "int classify(int n) { switch (n) { case 0: case 1: return 10; case 2: break; default: return 20; } return 0; }",
        expected: "fn:classify=block[stmt:switch(id(n),block[stmt:case(int(0),case(int(1),return(int(10)))),stmt:case(int(2),break),stmt:default(return(int(20)))]),stmt:return(int(0))]",
        expected_spans: &[
            ("compound", 20, 110),
            ("switch", 22, 98),
            ("compound", 33, 98),
            ("case", 35, 61),
            ("case", 43, 61),
            ("return", 51, 61),
            ("case", 62, 76),
            ("break", 70, 76),
            ("default", 77, 96),
            ("return", 86, 96),
            ("return", 99, 108),
        ],
    },
    ShapeCase {
        id: "jump.goto_and_labels",
        source: "int jump(int x) { goto done; skipped: ; done: return x; }",
        expected: "fn:jump=block[stmt:goto(done),stmt:label(skipped,null),stmt:label(done,return(id(x)))]",
        expected_spans: &[
            ("compound", 16, 57),
            ("goto", 18, 28),
            ("label", 29, 39),
            ("null", 38, 39),
            ("label", 40, 55),
            ("return", 46, 55),
        ],
    },
];

fn file() -> FileId {
    FileId::new(FILE_RAW)
}

fn parse<'arena>(source: &str, arena: &'arena Arena) -> evocc_chir::CHirUnit<'arena> {
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }

    let mut parser = Parser::new(tokens, file(), source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected {source:?}: {:?}",
        parser.diagnostic()
    );
    assert_unit_owned(arena, &unit);
    unit
}

fn parse_with_diagnostic<'arena>(
    source: &str,
    arena: &'arena Arena,
) -> (evocc_chir::CHirUnit<'arena>, evocc_parser::ParseDiagnostic) {
    let mut lexer = Lexer::new(file(), source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "lexer rejected {source:?}");
        tokens.push(token);
    }

    let mut parser = Parser::new(tokens, file(), source);
    let unit = parser.parse_translation_unit_in(arena);
    let diagnostic = parser
        .diagnostic()
        .cloned()
        .expect("malformed translation unit must be rejected");
    (unit, diagnostic)
}

fn assert_expression_owned(arena: &Arena, expression: &CHirExpr<'_>) {
    assert!(
        arena.contains(expression),
        "expression escaped caller Arena"
    );
    match expression {
        CHirExpr::Lit(..)
        | CHirExpr::FloatLit(..)
        | CHirExpr::CharLit(..)
        | CHirExpr::StringLit(..)
        | CHirExpr::Ident { .. }
        | CHirExpr::SizeOf(..)
        | CHirExpr::AlignOf(..)
        | CHirExpr::BuiltinUnreachable { .. }
        | CHirExpr::BuiltinReturnAddress { .. } => {}
        CHirExpr::Unary(_, child, _)
        | CHirExpr::Cast(_, child, _)
        | CHirExpr::SizeOfExpr(child, _)
        | CHirExpr::BitBuiltin { operand: child, .. }
        | CHirExpr::Member(child, _, _)
        | CHirExpr::PtrMember(child, _, _) => assert_expression_owned(arena, child),
        CHirExpr::Update { target, .. } => {
            assert_expression_owned(arena, target);
        }
        CHirExpr::MemoryUpdate { place, .. } | CHirExpr::MemoryRead { place, .. } => {
            for source in place.source_expressions() {
                assert_expression_owned(arena, source);
            }
        }
        CHirExpr::Binary(_, left, right, _)
        | CHirExpr::Index(left, right, _)
        | CHirExpr::Comma(left, right, _) => {
            assert_expression_owned(arena, left);
            assert_expression_owned(arena, right);
        }
        CHirExpr::Assign { target, value, .. } => {
            assert_expression_owned(arena, target);
            assert_expression_owned(arena, value);
        }
        CHirExpr::MemoryAssign { place, value, .. } => {
            for source in place.source_expressions() {
                assert_expression_owned(arena, source);
            }
            assert_expression_owned(arena, value);
        }
        CHirExpr::Ternary(condition, when_true, when_false, _) => {
            assert_expression_owned(arena, condition);
            assert_expression_owned(arena, when_true);
            assert_expression_owned(arena, when_false);
        }
        CHirExpr::Call(callee, arguments, _) => {
            assert_expression_owned(arena, callee);
            for argument in arguments {
                assert_expression_owned(arena, argument);
            }
        }
        CHirExpr::CheckedArithmetic {
            lhs, rhs, target, ..
        } => {
            assert_expression_owned(arena, lhs);
            assert_expression_owned(arena, rhs);
            assert_expression_owned(arena, target);
        }
        CHirExpr::CompoundLit(_, initializer, _) => {
            for action in initializer.plan().actions() {
                if let InitAction::StoreScalar { value, .. } = action {
                    assert_expression_owned(arena, value);
                }
            }
        }
        CHirExpr::StatementExpr { body, .. } => assert_statement_owned(arena, body),
    }
}

fn assert_declaration_owned(arena: &Arena, declaration: &CHirDecl<'_>) {
    assert!(
        arena.contains(declaration),
        "declaration escaped caller Arena"
    );
    match declaration {
        CHirDecl::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                assert_expression_owned(arena, operand.expression);
            }
        }
        CHirDecl::Object(object) => {
            if let Some(initializer) = object.initializer.as_ref() {
                assert_expression_owned(
                    arena,
                    initializer
                        .written()
                        .expression()
                        .expect("Day 4 uses expression initializers"),
                );
            }
        }
        CHirDecl::Function(function) => {
            if let Some(body) = function.body {
                assert_statement_owned(arena, body);
            }
        }
        CHirDecl::Tag(tag) => match &tag.members {
            TagMembers::Incomplete => {}
            TagMembers::Struct(members) | TagMembers::Union(members) => {
                for member in members {
                    if let Some(width) = member.bit_width {
                        assert_expression_owned(arena, width);
                    }
                }
            }
            TagMembers::Enum(enumerators) => {
                for enumerator in enumerators {
                    if let Some(value) = enumerator.written_value {
                        assert_expression_owned(arena, value);
                    }
                }
            }
        },
        CHirDecl::Typedef(_) => {}
    }
}

fn assert_statement_owned(arena: &Arena, statement: &CHirStmt<'_>) {
    assert!(arena.contains(statement), "statement escaped caller Arena");
    match statement {
        CHirStmt::Asm(assembly) => {
            for operand in assembly.outputs.iter().chain(&assembly.inputs) {
                assert_expression_owned(arena, operand.expression);
            }
        }
        CHirStmt::Expr(expression, _) => assert_expression_owned(arena, expression),
        CHirStmt::If(condition, then_branch, else_branch, _) => {
            assert_expression_owned(arena, condition);
            assert_statement_owned(arena, then_branch);
            if let Some(else_branch) = else_branch {
                assert_statement_owned(arena, else_branch);
            }
        }
        CHirStmt::Switch(expression, body, _) | CHirStmt::While(expression, body, _) => {
            assert_expression_owned(arena, expression);
            assert_statement_owned(arena, body);
        }
        CHirStmt::DoWhile(body, expression, _) => {
            assert_statement_owned(arena, body);
            assert_expression_owned(arena, expression);
        }
        CHirStmt::For(initializer, condition, step, body, _) => {
            match initializer {
                CHirForInit::Empty => {}
                CHirForInit::Expression(expression) => assert_expression_owned(arena, expression),
                CHirForInit::Declaration(declarations) => {
                    for declaration in declarations {
                        assert_declaration_owned(arena, declaration);
                    }
                }
            }
            if let Some(condition) = condition {
                assert_expression_owned(arena, condition);
            }
            if let Some(step) = step {
                assert_expression_owned(arena, step);
            }
            assert_statement_owned(arena, body);
        }
        CHirStmt::Return(value, _) => {
            if let Some(value) = value {
                assert_expression_owned(arena, value);
            }
        }
        CHirStmt::Compound(items, _) => {
            for item in items {
                match item {
                    CHirBlockItem::Declaration(declaration) => {
                        assert_declaration_owned(arena, declaration)
                    }
                    CHirBlockItem::Statement(statement) => assert_statement_owned(arena, statement),
                }
            }
        }
        CHirStmt::Label(_, nested, _) | CHirStmt::Default(nested, _) => {
            assert_statement_owned(arena, nested)
        }
        CHirStmt::Case(value, nested, _) => {
            assert_expression_owned(arena, value);
            assert_statement_owned(arena, nested);
        }
        CHirStmt::Break(_) | CHirStmt::Continue(_) | CHirStmt::Goto(_, _) | CHirStmt::Null(_) => {}
    }
}

fn assert_unit_owned(arena: &Arena, unit: &evocc_chir::CHirUnit<'_>) {
    for declaration in unit.declarations() {
        assert_declaration_owned(arena, declaration);
    }
}

fn binary_name(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "add",
        BinaryOp::Sub => "sub",
        BinaryOp::Lt => "lt",
        _ => panic!("operator outside statement shape contract: {operator:?}"),
    }
}

fn expression_fingerprint(expression: &CHirExpr<'_>) -> String {
    match expression {
        CHirExpr::Lit(literal, _) => format!("int({})", literal.value),
        CHirExpr::Ident { spelling, .. } => format!("id({spelling})"),
        CHirExpr::Binary(operator, left, right, _) => format!(
            "binary({},{},{})",
            binary_name(*operator),
            expression_fingerprint(left),
            expression_fingerprint(right)
        ),
        CHirExpr::Assign {
            target: left,
            value: right,
            ..
        } => format!(
            "assign({},{})",
            expression_fingerprint(left),
            expression_fingerprint(right)
        ),
        _ => panic!("expression outside statement shape contract: {expression:?}"),
    }
}

fn declaration_fingerprint(declaration: &CHirDecl<'_>) -> String {
    match declaration {
        CHirDecl::Object(object) => format!(
            "object({}={})",
            object.name.spelling,
            object.initializer.as_ref().map_or_else(
                || "-".into(),
                |initializer| {
                    expression_fingerprint(
                        initializer
                            .written()
                            .expression()
                            .expect("Day 4 uses expression initializers"),
                    )
                }
            )
        ),
        _ => panic!("declaration outside statement shape contract: {declaration:?}"),
    }
}

fn optional_expression(expression: Option<&CHirExpr<'_>>) -> String {
    expression.map_or_else(|| "-".into(), expression_fingerprint)
}

fn statement_kind(statement: &CHirStmt<'_>) -> &'static str {
    match statement {
        CHirStmt::Asm(..) => "asm",
        CHirStmt::Expr(..) => "expression",
        CHirStmt::If(..) => "if",
        CHirStmt::Switch(..) => "switch",
        CHirStmt::While(..) => "while",
        CHirStmt::DoWhile(..) => "do-while",
        CHirStmt::For(..) => "for",
        CHirStmt::Return(..) => "return",
        CHirStmt::Compound(..) => "compound",
        CHirStmt::Break(..) => "break",
        CHirStmt::Continue(..) => "continue",
        CHirStmt::Goto(..) => "goto",
        CHirStmt::Label(..) => "label",
        CHirStmt::Case(..) => "case",
        CHirStmt::Default(..) => "default",
        CHirStmt::Null(..) => "null",
    }
}

fn collect_statement_spans(statement: &CHirStmt<'_>, facts: &mut Vec<(&'static str, u32, u32)>) {
    let span = statement.span();
    assert_eq!(span.file, file());
    assert_eq!(span.line_start, 1);
    assert_eq!(span.column_start, span.byte_start + 1);
    facts.push((statement_kind(statement), span.byte_start, span.byte_end));

    match statement {
        CHirStmt::If(_, then_branch, else_branch, _) => {
            collect_statement_spans(then_branch, facts);
            if let Some(else_branch) = else_branch {
                collect_statement_spans(else_branch, facts);
            }
        }
        CHirStmt::Switch(_, body, _)
        | CHirStmt::While(_, body, _)
        | CHirStmt::For(_, _, _, body, _) => collect_statement_spans(body, facts),
        CHirStmt::DoWhile(body, _, _)
        | CHirStmt::Label(_, body, _)
        | CHirStmt::Case(_, body, _)
        | CHirStmt::Default(body, _) => collect_statement_spans(body, facts),
        CHirStmt::Compound(items, _) => {
            for item in items {
                if let CHirBlockItem::Statement(statement) = item {
                    collect_statement_spans(statement, facts);
                }
            }
        }
        CHirStmt::Expr(..)
        | CHirStmt::Asm(..)
        | CHirStmt::Return(..)
        | CHirStmt::Break(..)
        | CHirStmt::Continue(..)
        | CHirStmt::Goto(..)
        | CHirStmt::Null(..) => {}
    }
}

fn statement_fingerprint(statement: &CHirStmt<'_>) -> String {
    match statement {
        CHirStmt::Asm(assembly) => format!("asm({})", assembly.template),
        CHirStmt::Expr(expression, _) => {
            format!("expr({})", expression_fingerprint(expression))
        }
        CHirStmt::If(condition, then_branch, else_branch, _) => format!(
            "if({},{},{})",
            expression_fingerprint(condition),
            statement_fingerprint(then_branch),
            else_branch.map_or_else(|| "none".into(), |branch| statement_fingerprint(branch))
        ),
        CHirStmt::Switch(expression, body, _) => format!(
            "switch({},{})",
            expression_fingerprint(expression),
            statement_fingerprint(body)
        ),
        CHirStmt::While(expression, body, _) => format!(
            "while({},{})",
            expression_fingerprint(expression),
            statement_fingerprint(body)
        ),
        CHirStmt::DoWhile(body, expression, _) => format!(
            "do({},{})",
            statement_fingerprint(body),
            expression_fingerprint(expression)
        ),
        CHirStmt::For(initializer, condition, step, body, _) => {
            let initializer = match initializer {
                CHirForInit::Empty => "empty".into(),
                CHirForInit::Expression(expression) => {
                    format!("expr:{}", expression_fingerprint(expression))
                }
                CHirForInit::Declaration(declarations) => format!(
                    "decl:{}",
                    declarations
                        .iter()
                        .map(|declaration| declaration_fingerprint(declaration))
                        .collect::<Vec<_>>()
                        .join("|")
                ),
            };
            format!(
                "for({},{},{},{})",
                initializer,
                optional_expression(*condition),
                optional_expression(*step),
                statement_fingerprint(body)
            )
        }
        CHirStmt::Return(value, _) => {
            format!("return({})", optional_expression(*value))
        }
        CHirStmt::Compound(items, _) => format!(
            "block[{}]",
            items
                .iter()
                .map(|item| match item {
                    CHirBlockItem::Declaration(declaration) => {
                        format!("decl:{}", declaration_fingerprint(declaration))
                    }
                    CHirBlockItem::Statement(statement) => {
                        format!("stmt:{}", statement_fingerprint(statement))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
        CHirStmt::Break(_) => "break".into(),
        CHirStmt::Continue(_) => "continue".into(),
        CHirStmt::Goto(label, _) => format!("goto({})", label.spelling),
        CHirStmt::Label(label, nested, _) => format!(
            "label({},{})",
            label.spelling,
            statement_fingerprint(nested)
        ),
        CHirStmt::Case(value, nested, _) => format!(
            "case({},{})",
            expression_fingerprint(value),
            statement_fingerprint(nested)
        ),
        CHirStmt::Default(nested, _) => {
            format!("default({})", statement_fingerprint(nested))
        }
        CHirStmt::Null(_) => "null".into(),
    }
}

fn unit_fingerprint(unit: &evocc_chir::CHirUnit<'_>) -> String {
    unit.declarations()
        .iter()
        .map(|declaration| match declaration {
            CHirDecl::Function(function) => format!(
                "fn:{}={}",
                function.name.spelling,
                function
                    .body
                    .map_or_else(|| "prototype".into(), statement_fingerprint)
            ),
            declaration => declaration_fingerprint(declaration),
        })
        .collect::<Vec<_>>()
        .join(";")
}

#[test]
fn statement_trees_have_independent_canonical_fingerprints() {
    assert_eq!(SHAPE_CASES.len(), 6, "shape denominator changed");
    let mut ids = HashSet::new();
    let mut sources = HashSet::new();

    for case in SHAPE_CASES {
        assert!(ids.insert(case.id), "duplicate case id {}", case.id);
        assert!(
            sources.insert(case.source.as_bytes()),
            "duplicate source bytes for {}",
            case.id
        );
        let arena = Arena::default();
        let unit = parse(case.source, &arena);
        assert_eq!(unit_fingerprint(&unit), case.expected, "case {}", case.id);

        assert_eq!(unit.len(), 1, "case {} top-level denominator", case.id);
        let CHirDecl::Function(function) = unit.declarations()[0] else {
            panic!("case {} did not produce one function", case.id);
        };
        let mut actual_spans = Vec::new();
        collect_statement_spans(
            function.body.expect("function definition"),
            &mut actual_spans,
        );
        assert_eq!(
            actual_spans.as_slice(),
            case.expected_spans,
            "case {} exact statement spans",
            case.id
        );
    }
}

#[test]
fn for_declaration_initializer_preserves_every_declaration_event() {
    let source = "int outer(void) { for (struct S { int x; } s;;) break; return 0; }";
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let CHirDecl::Function(function) = unit.declarations()[0] else {
        panic!("expected outer function")
    };
    let CHirStmt::Compound(items, _) = function.body.expect("outer body") else {
        panic!("expected compound body")
    };
    let CHirBlockItem::Statement(CHirStmt::For(initializer, _, _, _, _)) = items[0] else {
        panic!("expected leading for statement")
    };
    let CHirForInit::Declaration(declarations) = initializer else {
        panic!("expected declaration initializer")
    };
    let declaration_start = source.find("struct S").expect("initializer start") as u32;
    let declaration_end = source.find(";;").expect("initializer terminator") as u32 + 1;
    for declaration in declarations {
        assert_eq!(declaration.span().file, file());
        assert_eq!(declaration.span().byte_start, declaration_start);
        assert_eq!(declaration.span().byte_end, declaration_end);
    }
    let fingerprint = declarations
        .iter()
        .map(|declaration| match declaration {
            CHirDecl::Tag(tag) => format!(
                "tag({})",
                tag.name
                    .as_ref()
                    .map_or("anonymous", |name| name.spelling.as_str())
            ),
            CHirDecl::Object(object) => format!("object({})", object.name.spelling),
            other => panic!("unexpected for-init declaration: {other:?}"),
        })
        .collect::<Vec<_>>()
        .join(";");
    assert_eq!(
        fingerprint, "tag(S);object(s)",
        "the tag declaration must not disappear behind the object declaration"
    );
}

#[test]
fn nested_function_definitions_are_rejected_at_a_fresh_statement_context() {
    let cases = [
        (
            "int outer(int x) { while (x) { int inner(void) { break; } } return x; }",
            "E1201",
            "break-outside-loop-or-switch",
            "break",
            0,
            5,
        ),
        (
            "int outer(int x) { switch (x) { int inner(void) { case 1: return 1; } } return x; }",
            "E1203",
            "case-outside-switch",
            "case",
            0,
            4,
        ),
        (
            "int outer(void) { int inner(void) { return 1; } return 0; }",
            "E1205",
            "nested-function-definition",
            "inner(void) {",
            "inner(void) ".len(),
            1,
        ),
    ];

    for (source, code, message, anchor, offset, width) in cases {
        let arena = Arena::default();
        let (unit, diagnostic) = parse_with_diagnostic(source, &arena);
        let start = source.find(anchor).expect("diagnostic anchor") as u32 + offset as u32;
        assert_eq!(diagnostic.code, code, "source {source:?}");
        assert_eq!(diagnostic.message, message, "source {source:?}");
        assert_eq!(diagnostic.span.file, file());
        assert_eq!(diagnostic.span.byte_start, start, "source {source:?}");
        assert_eq!(diagnostic.span.byte_end, start + width, "source {source:?}");
        assert!(
            unit.is_empty(),
            "a declaration containing a rejected function body must roll back: {unit:?}"
        );
    }
}

#[test]
fn failed_declaration_cannot_leak_side_declarations_into_the_next_one() {
    let source = "int broken = sizeof(struct S { int x; }) + ; int ok;";
    let arena = Arena::default();
    let (unit, diagnostic) = parse_with_diagnostic(source, &arena);
    let expected_start = source.find("; int ok").expect("failing semicolon") as u32;
    assert_eq!(diagnostic.code, "E1003");
    assert_eq!(diagnostic.message, "expected-expression");
    assert_eq!(diagnostic.span.file, file());
    assert_eq!(diagnostic.span.byte_start, expected_start);
    assert_eq!(diagnostic.span.byte_end, expected_start + 1);

    assert_eq!(
        unit.len(),
        1,
        "failed declaration events must not attach to the next declaration: {unit:?}"
    );
    let CHirDecl::Object(object) = unit.declarations()[0] else {
        panic!("only the later valid object may remain reachable: {unit:?}")
    };
    assert_eq!(object.name.spelling, "ok");
}

#[test]
fn failed_declaration_rolls_back_type_and_tag_state() {
    let source = "int broken = sizeof(struct S { int x; }) + ; struct S ok;";
    let arena = Arena::default();
    let (unit, _) = parse_with_diagnostic(source, &arena);
    assert_eq!(
        unit.len(),
        2,
        "later declaration must recreate its tag event"
    );

    let CHirDecl::Tag(tag) = unit.declarations()[0] else {
        panic!("later declaration must expose a fresh tag event: {unit:?}")
    };
    assert!(
        matches!(tag.members, TagMembers::Incomplete),
        "the failed complete definition must have been rolled back"
    );
    let later_start = source.rfind("struct S ok;").expect("later declaration") as u32;
    assert_eq!(tag.span.byte_start, later_start);
    assert_eq!(tag.span.byte_end, source.len() as u32);
    assert!(matches!(
        unit.types().tag(tag.tag).map(|record| &record.state),
        Some(TagState::Incomplete)
    ));

    let CHirDecl::Object(object) = unit.declarations()[1] else {
        panic!("later object declaration missing: {unit:?}")
    };
    assert_eq!(object.name.spelling, "ok");
    let TypeSpecifier::Tag {
        tag: object_tag, ..
    } = object.specifier
    else {
        panic!("later object must retain its struct specifier")
    };
    assert_eq!(object_tag, tag.tag);
}
