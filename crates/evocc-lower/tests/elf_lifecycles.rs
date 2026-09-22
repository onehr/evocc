use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn load_instance_metadata_survives_text_and_model_changes_are_semantic() {
    let source = "static __thread int n=7; static void init(void) __attribute__((constructor(100+101))); static void init(void){n=9;} static void fini(void) __attribute__((destructor(202))); static void fini(void){n=0;} int main(void){return n;}";
    let arena = Arena::default();
    let file = FileId::new(9020);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let text = evocc_evoir::render_module(&module);
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
    assert_eq!(parsed.lifecycle.len(), 2);
    assert_eq!(parsed.lifecycle[0].priority, 201);
    assert_eq!(parsed.tls.len(), 1);
    for change in [
        text.replace("priority=201", "priority=203"),
        text.replace("model=global_dynamic", "model=initial_exec"),
    ] {
        assert_ne!(
            evocc_evoir::parse_section9_module(change.as_bytes()).unwrap(),
            parsed
        );
    }
    for change in [
        text.replace("priority=201", "priority=100"),
        text.replace("init @init", "init @missing"),
        text.replace("tls @n model=global_dynamic\n", ""),
        text.replace("model=global_dynamic", "model=unknown"),
    ] {
        assert!(
            evocc_evoir::parse_section9_module(change.as_bytes()).is_err(),
            "{change}"
        );
    }
    let main = unit
        .iter()
        .find_map(|d| match d {
            evocc_chir::CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    let result = evocc_chir::interp::interpret_program(
        &unit,
        main,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        Default::default(),
    )
    .unwrap();
    assert!(result.memory.objects().any(|o| o.origin.thread.is_some()));
}
