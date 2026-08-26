use evocc_chir::{CHirDecl, CHirUnit, InitAction, ObjectDecl};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(10_112),
        "designated-initializers.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err(format!("lexer rejected input at {:?}", token.span));
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

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\n  source: {source}");
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n  source: {source}"
        ),
    }
}

fn object<'arena>(unit: &CHirUnit<'arena>, name: &str) -> &'arena ObjectDecl<'arena> {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == name => Some(object),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing object {name}"))
}

#[test]
fn field_index_chain_and_range_designators_parse() {
    accepts(
        "struct inner { int lane[2]; };\n\
         struct outer { int first; struct inner nested; int last; };\n\
         struct outer a = { .last = 9 };\n\
         int b[4] = { [2] = 7 };\n\
         struct outer c = { .nested.lane[1] = 5 };\n\
         int d[5] = { [0 ... 3] = 4 };\n",
    );
}

#[test]
fn positional_cursor_continues_after_a_designator() {
    let arena = Arena::default();
    let unit = parse(
        "struct row { int a; int b; int c; }; struct row value = { .b = 1, 2 };\n",
        &arena,
    )
    .expect("designated initializer must parse");
    let plan = object(&unit, "value").initializer.as_ref().unwrap().plan();
    let stores = plan
        .actions()
        .iter()
        .filter(|action| matches!(action, InitAction::StoreScalar { .. }))
        .count();
    assert_eq!(stores, 2, "both b and the following member c are written");
}

#[test]
fn later_overlapping_action_wins_while_written_form_retains_both() {
    let arena = Arena::default();
    let unit = parse(
        "struct row { int a; int b; }; struct row value = { .a = 1, .a = 2 };\n",
        &arena,
    )
    .expect("overlapping designated initializer must parse");
    let initializer = object(&unit, "value").initializer.as_ref().unwrap();
    assert_eq!(
        initializer.written().elements().unwrap().len(),
        2,
        "lossless written form retains both actions"
    );
    let writes_to_a = initializer
        .plan()
        .actions()
        .iter()
        .filter(|action| {
            matches!(action, InitAction::StoreScalar { path, .. } if path.steps().len() == 1)
        })
        .count();
    assert_eq!(
        writes_to_a, 1,
        "normalized plan retains only the later write"
    );
}

#[test]
fn nested_array_record_and_string_designators_parse() {
    accepts(
        "struct leaf { int x; char name[4]; };\n\
         struct tree { struct leaf leaf[2]; int tail; };\n\
         struct tree a = { .leaf = { [1] = { .x = 7, .name = \"ok\" } }, .tail = 3 };\n\
         struct leaf b[2] = { [1].name = \"hi\", [0].x = 4 };\n",
    );
}

#[test]
fn invalid_designators_report_e2906_at_the_front_end() {
    rejects(
        "struct row { int a; }; struct row value = { .missing = 1 };\n",
        "E2906",
    );
    rejects("int values[2] = { [2] = 1 };\n", "E2906");
    rejects("int scalar = { .member = 1 };\n", "E2906");
}

#[test]
fn measured_nested_kernel_style_initializers_parse_verbatim() {
    accepts(
        "struct list_head { struct list_head *next; struct list_head *prev; };\n\
         struct task { int state; struct list_head tasks; };\n\
         struct task init_task = {\n\
           .state = 0,\n\
           .tasks = { .next = &init_task.tasks, .prev = &init_task.tasks },\n\
         };\n\
         struct descriptor { unsigned long base; struct { unsigned short limit; unsigned short flags; } gate; };\n\
         struct descriptor desc = { .base = 0, .gate = { .limit = 7, .flags = 3 } };\n",
    );
}
