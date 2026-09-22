use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn function_sections_follow_the_complete_declaration_chain() {
    for source in [
        include_str!("../../../tests/fixtures/kernel-function-section.c"),
        "int boot_function(void) __attribute__((section(\".init.text\"))); int boot_function(void) { return 42; }\nint boot_function(void) __attribute__((section(\".init.text\")));",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45015);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let text = evocc_evoir::render_module(&module);
        let header = text.lines().find(|line| line.starts_with("func @boot_function(")).unwrap();
        assert!(header.ends_with(" section=\".init.text\""), "{header}");
    }
}

#[test]
fn function_section_merging_keeps_symbol_identity_and_rejects_conflicts() {
    let accepted = [
        ("int f(void) __attribute__((section(\".init.\" \"text\"))); int f(void) __attribute__((section(\".init.text\"))); int f(void) { return 1; }", "f"),
        ("int a(void) asm(\"f\") __attribute__((section(\".init.text\"))); int f(void) { return 1; } int call(void) { return a(); }", "f"),
        ("int *__attribute__((section(\".init.text\"))) f(void) { return 0; }", "f"),
        ("int f(void) __attribute__((section(\".init.text\"))); int f(void) { return 1; } int other(void) __attribute__((alias(\"f\"), section(\".init.text\")));", "f"),
    ];
    let rejected = [
        "int f(void) { return 1; } int f(void) __attribute__((section(\".init.text\")));",
        "int f(void) { return 1; } int a(void) asm(\"f\") __attribute__((section(\".init.text\")));",
        "int f(void) __attribute__((section(\".init.text\"))); int f(void) __attribute__((section(\".exit.text\"))); int f(void) { return 1; }",
        "int *__attribute__((section(\".init.text\"))) f(void); int *f(void) __attribute__((section(\".exit.text\"))); int *f(void) { return 0; }",
        "int f(void) __attribute__((section(\"\"))); int f(void) { return 1; }",
        "int f(void) __attribute__((section(\"bad\\nname\"))); int f(void) { return 1; }",
        "int f(void) __attribute__((section(42))); int f(void) { return 1; }",
        "int f(void) __attribute__((section(\".init.text\", \"extra\"))); int f(void) { return 1; }",
        "int a(void) asm(\"f\") __attribute__((section(\".init.text\"))); int f(void) __attribute__((section(\".exit.text\"))); int f(void) { return 1; }",
        "int f(void) __attribute__((section(\".init.text\"))); int f(void) { return 1; } int other(void) __attribute__((alias(\"f\"), section(\".exit.text\")));",
    ];
    for (source, name) in accepted
        .into_iter()
        .chain(rejected.into_iter().map(|s| (s, "")))
    {
        let arena = Arena::default();
        let file = FileId::new(45016);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
        let result = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu());
        if name.is_empty() {
            assert_eq!(result.unwrap_err().code(), "E2943", "{source}");
        } else {
            let module = result.unwrap_or_else(|e| panic!("{source}: {e}"));
            assert_eq!(
                module
                    .functions
                    .iter()
                    .find(|f| f.name == name)
                    .unwrap()
                    .section
                    .as_deref(),
                Some(".init.text"),
                "{source}"
            );
        }
    }
}

#[test]
fn function_section_names_preserve_utf8_execution_bytes() {
    for (spelling, expected) in [
        (r#"".text.é""#, Some(".text.é")),
        (r#"".text." "é""#, Some(".text.é")),
        (r#"".text.\\x""#, Some(".text.\\x")),
        (r#"".text.\xc3" "\xa9""#, None),
        (r#"".text.\303\251""#, None),
        (r#"".text.\xff""#, None),
    ] {
        let source = format!(
            "int f(void) __attribute__((section({spelling}))); int f(void) {{ return 1; }}"
        );
        let arena = Arena::default();
        let file = FileId::new(45017);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let result = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu());
        if let Some(expected) = expected {
            assert_eq!(
                result.unwrap().functions[0].section.as_deref(),
                Some(expected)
            );
        } else {
            assert_eq!(result.unwrap_err().code(), "E2943");
        }
    }
}

#[test]
fn pointer_declarators_preserve_symbol_linkage() {
    let source = include_str!("../../../tests/fixtures/pointer-symbol-linkage.c");
    let arena = Arena::default();
    let file = FileId::new(45013);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    for name in ["choose", "choose_pair", "choose_qualified"] {
        let function = module.functions.iter().find(|f| f.name == name).unwrap();
        assert_eq!(function.linkage, evocc_evoir::Linkage::Weak, "{name}");
    }
    for name in ["pointer_object", "callback"] {
        let object = module.globals.iter().find(|g| g.symbol == name).unwrap();
        assert_eq!(object.linkage, evocc_evoir::Linkage::Weak, "{name}");
    }
    assert!(module
        .aliases
        .iter()
        .any(|alias| alias.name == "alternative" && alias.target == "implementation"));
    let inventory = unit.attribute_inventory();
    assert_eq!(inventory.preserved_not_applied.get("weak"), Some(&5));
    assert_eq!(inventory.preserved_not_applied.get("alias"), Some(&1));
    evocc_evoir::validate_module(&module).unwrap();
}

#[test]
fn pointer_symbol_attributes_keep_linkage_rejections() {
    for (source, code) in [
        (
            "static int * __attribute__((weak)) hidden(void) { return 0; }",
            "E3060",
        ),
        (
            "extern int * __attribute__((alias(\"missing\"))) other(void);",
            "E3061",
        ),
        (
            "int *__attribute__((section(\".data.first\"))) object; extern int *object __attribute__((section(\".data.second\")));",
            "E2943",
        ),
        (
            "void (*__attribute__((section(\"\"))) callback)(void);",
            "E2943",
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45014);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
        let error = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .unwrap_err();
        assert_eq!(error.code(), code, "{source}");
    }
}

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
            "int *__attribute__((section(\".data..percpu\"))) target;",
            Some(".data..percpu"),
            false,
            false,
        ),
        (
            "void (*__attribute__((section(\".data..percpu\"))) target)(void);",
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
        assert_eq!(
            linkage.visibility.values().any(|&v| v == evocc_chir::linkage::SymbolVisibility::Hidden),
            hidden,
            "{source}"
        );
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
