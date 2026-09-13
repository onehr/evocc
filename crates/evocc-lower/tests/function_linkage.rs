use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn adjacent_attribute_strings_name_the_same_linker_entities() {
    for (source, section, hidden, alias) in [
        (
            "int target __attribute__((section(\".data..percpu\" \"\")));",
            Some(".data..percpu"),
            false,
            false,
        ),
        (
            "int target __attribute__((section(\"\\x2e\" \"data..percpu\")));",
            Some(".data..percpu"),
            false,
            false,
        ),
        (
            "extern int target __attribute__((section(\".data.\" \".percpu\"))); int target __attribute__((section(\".data..percpu\")));",
            Some(".data..percpu"),
            false,
            false,
        ),
        (
            "int target __attribute__((visibility(\"hid\" \"den\")));",
            None,
            true,
            false,
        ),
        (
            "int target; extern int other __attribute__((alias(\"tar\" \"get\")));",
            None,
            false,
            true,
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45012);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
        let linkage = unit.symbol_linkage().expect(source);
        let module = evocc_lower::lower_unit(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .expect(source);
        let object = module.globals.iter().find(|g| g.symbol == "target").unwrap();
        assert_eq!(object.section.as_deref(), section, "{source}");
        assert_eq!(linkage.hidden.values().any(|&v| v), hidden, "{source}");
        assert_eq!(!linkage.aliases.is_empty(), alias, "{source}");
        evocc_evoir::validate_module(&module).unwrap();
    }
}

#[test]
fn composite_function_declarations_keep_the_completed_interface() {
    let source = include_str!("../../../tests/fixtures/composite-function-declarations.c");
    let arena = Arena::default();
    let file = FileId::new(45011);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    assert!(module.declares.is_empty());
    assert_eq!(module.functions.len(), 5);
    for name in ["complete", "first", "typed"] {
        let function = module.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(function.params[0].ty, evocc_types::EvoIRType::Bits(32));
    }
}

#[test]
fn static_definitions_keep_internal_linkage_in_the_ir_header() {
    let source = "static int private(void){return 3;} static inline int helper(void){return private();} int (*keep_bodies[])(void)={private,helper}; int public(void){return helper();}";
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
