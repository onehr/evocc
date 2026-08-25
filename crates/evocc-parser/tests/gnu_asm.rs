//! GNU asm front-end contract (specification 10.1.16).

use evocc_chir::{render_unit_summary, CHirUnit};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, String> {
    let file = SourceFile::new(
        FileId::new(9_602),
        "gnu-asm.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
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

#[test]
fn basic_asm_parses_at_file_and_block_scope() {
    accepts("asm(\".pushsection .init\");\n");
    accepts("__asm__(\".popsection\");\n");
    accepts("void f(void) { asm(\"nop\"); }\n");
    accepts("void f(void) { asm(\"a\" \"b\" \"c\"); }\n");
}

#[test]
fn extended_asm_forms_parse() {
    accepts("void f(int x) { asm volatile(\"nop\" : \"+r\"(x) : : \"memory\"); }\n");
    accepts("void f(int x) { __asm__ __volatile__(\"nop\" : \"=r\"(x) : \"0\"(x)); }\n");
    accepts("void f(int x, int y) { asm(\"add %0, %1, %2\" : \"=r\"(x) : \"r\"(x), \"r\"(y)); }\n");
    accepts("void f(int x) { asm(\"mov %[out], %[in]\" : [out] \"=r\"(x) : [in] \"r\"(x)); }\n");
    accepts("void f(int x) { asm inline volatile(\"nop\" : : \"r\"(x)); }\n");
}

#[test]
fn asm_goto_forms_parse_and_forward_labels_resolve() {
    accepts("int h(void) { asm goto(\"b %l[out]\" : : : : out); return 0; out: return 1; }\n");
    accepts(
        "int h(int x) { asm volatile goto(\"cbz %0, %l[z]\" : : \"r\"(x) : \"cc\" : z); return 0; z: return 1; }\n",
    );
    accepts(
        "int f(void) { do { asm goto(\"1: nop\\n .pushsection __bug_table\\n .popsection\" : : \"i\"(0) : : l_yes); } while (0); return 0; l_yes: return 1; }\n",
    );
}

#[test]
fn asm_labels_on_declarators_are_accepted_and_inert() {
    accepts("int x asm(\"real_x\");\n");
    accepts("extern int y __asm__(\"real_y\");\n");
    accepts("int f(void) asm(\"real_f\");\n");
    accepts("int a asm(\"ra\"), b asm(\"rb\");\n");
    accepts("void f(void) { static int z asm(\"real_z\"); }\n");
}

#[test]
fn extended_asm_is_checked_at_the_front_end() {
    rejects("void f(int x) { asm(\"nop\" : \"=r\"(x + 1)); }\n", "E2914");
    rejects(
        "void f(int x) { asm(\"mov %[nope], %0\" : \"=r\"(x)); }\n",
        "E2914",
    );
    rejects(
        "void f(void) { asm volatile volatile(\"nop\"); }\n",
        "E2914",
    );
}

#[test]
fn asm_goto_labels_are_checked_at_function_end() {
    rejects(
        "int h(void) { asm goto(\"b %l[out]\" : : : : nowhere); return 0; }\n",
        "E2914",
    );
}

#[test]
fn asm_reaches_chir_losslessly() {
    let arena = Arena::default();
    let unit = parse(
        "void f(int x) { asm volatile(\"mov %[o], %[i]\" : [o] \"=r\"(x) : [i] \"r\"(x) : \"memory\"); }\n",
        &arena,
    )
    .expect("extended asm must parse");
    let summary = render_unit_summary(&unit);
    for needle in [
        "asm volatile",
        "mov %[o], %[i]",
        "[o] \"=r\"",
        "[i] \"r\"",
        "clobber \"memory\"",
    ] {
        assert!(summary.contains(needle), "{needle} missing in:\n{summary}");
    }
}

#[test]
fn the_kernel_extended_asm_line_parses() {
    accepts(
        "unsigned long f(unsigned long dividend) { asm(\"\" : \"+rm\"(dividend)); return dividend; }\n",
    );
}
