//! Label addresses lower to code pointers; indirect jumps remain fail-closed.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::{ParseDiagnostic, Parser};
use evocc_types::TargetDataLayout;

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_305);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn parse_valid<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    parse(source, arena).unwrap_or_else(|diagnostic| {
        panic!(
            "fixture must parse, got {}: {}\nsource: {source}",
            diagnostic.code, diagnostic.message
        )
    })
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

fn assert_execution_and_lowering_refuse(source: &str) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let interpreter = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(100_000),
    )
    .expect_err("C-HIR execution must fail closed");
    assert_eq!(interpreter.code(), "E3029");

    let lowering = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("computed goto must not reach EvoIR");
    assert_eq!(lowering.code(), "E3029");
    assert_eq!(lowering.to_string(), "not-lowered: computed-goto");
}

fn check_label_address(source: &str, expected: Result<u64, &str>, forms: &[&str]) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("label addresses have an EvoIR representation");
    let text = evocc_evoir::render_module(&module);
    for form in forms {
        assert!(text.contains(form), "missing {form}: {text}");
    }
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments = main
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let result = evocc_evoir::interpret_module(
        &module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(100_000),
    );
    match expected {
        Ok(value) => assert_eq!(result.unwrap().return_values[0].raw_bits, value),
        Err(code) => assert_eq!(result.unwrap_err().code(), code),
    }
}

#[test]
fn label_addresses_have_stable_distinct_code_identity() {
    check_label_address(
        "int main(void){ void *p=&&first; int n=0; if(n)goto last; first: return p==&&first && p!=&&last; last: return 0; }",
        Ok(1),
        &["block_addr"],
    );
    check_label_address(
        "int main(void){ if(0){done:return 0;} return &&done != 0; }",
        Ok(1),
        &["block_addr"],
    );
}

#[test]
fn computed_goto_is_preserved_but_execution_and_lowering_fail_closed() {
    assert_execution_and_lowering_refuse(concat!(
        "int main(void) { void *target = &&done; goto *target; ",
        "done: return 0; }\n",
    ));
}

#[test]
fn addressed_expression_labels_keep_their_conditional_entry_edges() {
    for body in [
        "if(0) note(({__label__ here; here: (unsigned long)&&here;}));",
        "if(0) {unsigned long p=({__label__ here; here: (unsigned long)&&here;}); note(p);}",
        "if(1) {} else note(({__label__ here; here: (unsigned long)&&here;}));",
    ] {
        let source = format!(
            "int called; void note(unsigned long p){{called++;}} int main(void){{{body} return called;}}"
        );
        check_label_address(&source, Ok(0), &["block_addr", "cond_br"]);
    }
}

#[test]
fn an_addressed_switch_label_keeps_its_terminator_edge_path() {
    check_label_address(
        "int main(void){void *p=&&keep;switch(4){case 3:keep:return p==0;case 4:return 0;}return 1;}",
        Ok(0),
        &["block_addr", "cond_br"],
    );
}

#[test]
fn addressed_labels_in_expression_arms_keep_their_entry_edges() {
    for expression in [
        "0 && ({keep: ++n;})",
        "1 || ({keep: ++n;})",
        "1 ? 7 : ({keep: ++n;})",
        "0 ? ({keep: ++n;}) : 7",
        "7 ?: ({keep: ++n;})",
    ] {
        check_label_address(
            &format!("int main(void){{void *p=&&keep;int n=0;(void)({expression});return n!=0 || p==0;}}"),
            Ok(0),
            &["block_addr", "cond_br"],
        );
    }
}

#[test]
fn an_address_only_dead_label_keeps_the_reachability_boundary() {
    // Section 3.7 admits the address, but 2.5 still forbids a validated
    // block without a terminator-edge path from entry. Do not weaken SSA.
    let arena = Arena::default();
    let unit = parse_valid(
        "unsigned long f(void){return (unsigned long)&&after; after:return 0;}",
        &arena,
    );
    let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap_err();
    assert_eq!(error.code(), "E3101");
    assert!(error.to_string().contains("unreachable from entry"));
}

#[test]
fn cohort_label_address_form_lowers_to_an_integer_address() {
    check_label_address("int main(void){ unsigned long a=({__label__ __here; __here: (unsigned long)&&__here;}); unsigned long b=({__label__ __here; __here: (unsigned long)&&__here;}); return a!=0 && a!=b; }", Ok(1), &["block_addr", "ptr.to_int"]);
}

#[test]
fn static_label_addresses_relocate_to_the_owning_function_block() {
    check_label_address("int f(void){static void *targets[2]={[0 ... 1]=&&done}; done:return targets[0]==&&done && targets[1]==&&done;} int main(void){static void *p=&&done; done:return f() && p==&&done;}", Ok(1), &["relocs=[", "@f^b", "@main^b"]);
}

#[test]
fn code_addresses_do_not_acquire_object_provenance() {
    check_label_address(
        "int main(void){here:return *(int *)&&here;}",
        Err("E3013"),
        &["block_addr", "load"],
    );
}

#[test]
fn static_label_address_initializer_reaches_the_typed_execution_boundary() {
    assert_execution_and_lowering_refuse(concat!(
        "int main(void) { static void *targets[2] = { [0 ... 1] = &&done }; ",
        "goto *targets[0]; done: return 0; }\n",
    ));
}

#[test]
fn undeclared_ordinary_goto_keeps_the_typed_diagnostic() {
    let arena = Arena::default();
    let diagnostic = parse("int main(void) { goto nowhere; }\n", &arena)
        .expect_err("an undeclared label must be rejected");
    assert_eq!(diagnostic.code, "E2920");
}
