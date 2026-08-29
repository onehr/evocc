//! Anonymous record members participate in every designator path.

use evocc_chir::{CHirDecl, CHirExpr, CHirUnit, InitAction, ObjectDecl, SubobjectStep};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

const OUTER: &str = concat!(
    "struct outer { int head; union { struct { long regs[2]; long sp; }; ",
    "long raw[3]; }; int tail; };\n",
);

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = SourceFile::new(
        FileId::new(10_120),
        "anonymous-member-designators.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(
            token.kind,
            TokenKind::Error,
            "lexer rejected anonymous-member fixture at {:?}",
            token.span
        );
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    parse(source, arena).unwrap_or_else(|diagnostic| {
        panic!(
            "valid anonymous-member source rejected with {} {} at {}..{}",
            diagnostic.code,
            diagnostic.message,
            diagnostic.span.byte_start,
            diagnostic.span.byte_end
        )
    })
}

fn object<'arena>(unit: &CHirUnit<'arena>, name: &str) -> &'arena ObjectDecl<'arena> {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == name => Some(object),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing object {name}"))
}

fn scalar_stores(unit: &CHirUnit<'_>, name: &str) -> Vec<(Vec<usize>, u64)> {
    object(unit, name)
        .initializer
        .as_ref()
        .expect("object initializer")
        .plan()
        .actions()
        .iter()
        .filter_map(|action| {
            let InitAction::StoreScalar { path, value, .. } = action else {
                return None;
            };
            let fields = path
                .steps()
                .iter()
                .map(|step| match step {
                    SubobjectStep::Field { field_index } => *field_index,
                    SubobjectStep::Index { index } => {
                        panic!("unexpected index {index} in record member path")
                    }
                })
                .collect::<Vec<_>>();
            let CHirExpr::Lit(literal, _) = value else {
                panic!("expected literal initializer, got {value:?}")
            };
            Some((fields, literal.value))
        })
        .collect()
}

fn assert_e2906_at_designator(source: &str) {
    let arena = Arena::default();
    let diagnostic = parse(source, &arena).expect_err("ambiguous member must be rejected");
    assert_eq!(diagnostic.code, "E2906");
    assert_eq!(diagnostic.message, "designator-outside-object");
    assert_eq!(
        &source[diagnostic.span.byte_start as usize..diagnostic.span.byte_end as usize],
        "x"
    );
}

#[test]
fn anonymous_members_have_the_sealed_layout_and_offsets() {
    let source = format!(
        "{OUTER}\
         _Static_assert(sizeof(struct outer) == 40, \"size\");\
         _Static_assert(__builtin_offsetof(struct outer, head) == 0, \"head\");\
         _Static_assert(__builtin_offsetof(struct outer, sp) == 24, \"sp\");\
         _Static_assert(__builtin_offsetof(struct outer, raw) == 8, \"raw\");\
         _Static_assert(__builtin_offsetof(struct outer, tail) == 32, \"tail\");\n"
    );
    let arena = Arena::default();
    parse_valid(&source, &arena);
}

#[test]
fn designated_initializer_records_every_anonymous_member_step() {
    let source = format!("{OUTER}struct outer value = {{ .head = 1, .sp = 7 }};\n");
    let arena = Arena::default();
    let unit = parse_valid(&source, &arena);
    let stores = scalar_stores(&unit, "value");
    assert!(
        stores.contains(&(vec![0], 1)),
        "head store missing: {stores:?}"
    );
    assert!(
        stores.contains(&(vec![1, 0, 1], 7)),
        "anonymous union/struct path missing: {stores:?}"
    );
}

#[test]
fn anonymous_member_resolution_recurses_through_deep_nesting() {
    let source = concat!(
        "struct deep { int h; union { struct { union { struct { long q; }; }; }; }; };\n",
        "_Static_assert(__builtin_offsetof(struct deep, q) == 8, \"q\");\n",
        "struct deep value = { .q = 3 };\n",
    );
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let stores = scalar_stores(&unit, "value");
    assert!(
        stores.contains(&(vec![1, 0, 0, 0, 0], 3)),
        "deep anonymous path missing: {stores:?}"
    );
}

#[test]
fn kernel_pt_regs_sp_shape_uses_the_promoted_member() {
    let source = concat!(
        "typedef unsigned long u64; typedef signed int s32; typedef unsigned int u32;\n",
        "struct user_pt_regs { u64 regs[31]; u64 sp; u64 pc; u64 pstate; };\n",
        "struct frame_record_meta { u64 fp; u64 lr; };\n",
        "struct pt_regs {\n",
        " union { struct user_pt_regs user_regs; struct { u64 regs[31]; u64 sp; u64 pc; u64 pstate; }; };\n",
        " u64 orig_x0; s32 syscallno; u32 pmr; u64 sdei_ttbr1; struct frame_record_meta stackframe;\n",
        "};\n",
        "unsigned long pt_regs_sp = __builtin_offsetof(struct pt_regs, sp);\n",
    );
    let arena = Arena::default();
    parse_valid(source, &arena);
}

#[test]
fn ambiguous_anonymous_members_report_e2906_at_the_name() {
    assert_e2906_at_designator(concat!(
        "struct amb { union { struct { int x; }; struct { int x; }; }; };\n",
        "unsigned long offset = __builtin_offsetof(struct amb, x);\n",
    ));
    assert_e2906_at_designator(concat!(
        "struct amb { union { struct { int x; }; struct { int x; }; }; };\n",
        "struct amb value = { .x = 1 };\n",
    ));
}
