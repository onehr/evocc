use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn static_definitions_keep_internal_linkage_in_the_ir_header() {
    let source = "static int private(void){return 3;} static inline int helper(void){return private();} int public(void){return helper();}";
    let arena = Arena::default();
    let file = FileId::new(45010);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let text = evocc_evoir::render_module(&module);
    for name in ["private", "helper", "public"] {
        let header = text
            .lines()
            .find(|line| line.starts_with(&format!("func @{name}(")))
            .unwrap();
        assert_eq!(
            header.contains(" visibility=internal"),
            name != "public",
            "{header}"
        );
    }
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
}
