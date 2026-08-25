use evocc_chir::AllocationSiteManifest;
use evocc_common::{Arena, FileId};
use evocc_evoir::{render_module, validate_module, Module};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn lower(source: &str, ordinal: u32) -> Module {
    let arena = Arena::default();
    let file = FileId::new(9_400 + ordinal);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(
            token.kind,
            TokenKind::Error,
            "lexer rejected case {ordinal}"
        );
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(
        !parser.had_error(),
        "parser rejected case {ordinal}: {:?}",
        parser.diagnostic()
    );
    AllocationSiteManifest::derive(&unit)
        .unwrap_or_else(|error| panic!("allocation manifest case {ordinal}: {error:?}"));
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("lowering case {ordinal}: {error}"));
    validate_module(&module).unwrap_or_else(|error| panic!("validation case {ordinal}: {error}"));
    module
}

#[test]
fn front_end_resolved_forms_leave_byte_identical_evoir() {
    let pairs = [
        (
            "int f(int i) { typeof(i) j = i + 1; return j; }\n",
            "int f(int i) { int j = i + 1; return j; }\n",
        ),
        (
            "int f(int i) { return _Generic(i, int: i * 2, default: 0); }\n",
            "int f(int i) { return i * 2; }\n",
        ),
        (
            "long f(void) { long x = 7; long *p = &x; return *(const volatile typeof(_Generic(*p, long: (long)0, default: *p)) *)p; }\n",
            "long f(void) { long x = 7; long *p = &x; return *(const volatile long *)p; }\n",
        ),
    ];

    for (ordinal, (written, spelled_out)) in pairs.into_iter().enumerate() {
        let written = render_module(&lower(written, ordinal as u32));
        let spelled_out = render_module(&lower(spelled_out, ordinal as u32));
        assert_eq!(
            written, spelled_out,
            "front-end-only form leaked into EvoIR pair {ordinal}"
        );
    }
}
