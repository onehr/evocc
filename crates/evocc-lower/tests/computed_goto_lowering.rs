//! Label addresses and indirect jumps preserve local objects and edge bindings.

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

fn check_computed_goto(source: &str, expected: u64) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let interpreter = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        InterpreterLimits::new(100_000),
    )
    .expect("C-HIR executes the selected label in the current activation");
    assert_eq!(
        interpreter.return_value.unwrap().raw_bits,
        u128::from(expected)
    );
    check_label_address(source, Ok(expected), &["br.indirect"]);
}

fn check_label_address(source: &str, expected: Result<u64, &str>, forms: &[&str]) {
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("label addresses have an EvoIR representation");
    for function in &module.functions {
        for block in &function.blocks {
            for instruction in block.insts.iter().chain([&block.terminator]) {
                if let evocc_evoir::Inst::IndirectBr { edges, .. } = instruction {
                    for edge in edges {
                        assert!(
                            edge.arguments.iter().all(|argument| matches!(
                                argument.ty,
                                evocc_evoir::EvoIRType::Ptr(_)
                                    | evocc_evoir::EvoIRType::Mem(_)
                                    | evocc_evoir::EvoIRType::World
                            )),
                            "C indirect entries transport storage addresses and effects"
                        );
                    }
                }
            }
        }
    }
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
        Ok(value) => assert_eq!(
            result
                .unwrap_or_else(|error| panic!("{error:?}\nsource: {source}\n{text}"))
                .return_values[0]
                .raw_bits,
            value
        ),
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
fn computed_goto_executes_a_local_target() {
    check_computed_goto(
        concat!(
            "int main(void) { void *target = &&done; goto *target; ",
            "done: return 0; }\n",
        ),
        0,
    );
}

#[test]
fn computed_edges_match_the_native_fixture_in_both_interpreters() {
    check_computed_goto(include_str!("../../../tests/fixtures/computed-edges.c"), 0);
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
fn static_label_address_initializer_dispatches_to_the_selected_label() {
    check_computed_goto(
        concat!(
            "int main(void) { static void *targets[2] = { [0 ... 1] = &&done }; ",
            "goto *targets[0]; done: return 0; }\n",
        ),
        0,
    );
}

#[test]
fn computed_goto_preserves_distinct_storage_values_and_recursive_activations() {
    for (source, expected) in [
        ("int main(void){int x=0;void *p=&&again;goto *p;again:if(++x<3)goto *p;return x;}", 3),
        ("int main(void){int x=5,y=9;void *p=&&left;goto *p;left:y+=x;p=&&right;goto *p;right:x+=y;return x*100+y;}", 1914),
        ("int f(int n){int x=n;void *p=&&done;if(n)x+=f(n-1);goto *p;done:return x;}int main(void){return f(4);}", 10),
        ("int main(void){int x;void *p=&&set;goto *p;set:x=7;p=&&done;goto *p;done:return x;}", 7),
        ("int main(void){int x=0;void *p=&&done;{int y=5;x=y;goto *p;}done:return x;}", 5),
        ("int main(void){int x=7;unsigned long p=(unsigned long)&&done;goto *(void *)p;done:return x;}", 7),
        ("int main(void){int n=3;int *p=__builtin_alloca(n*sizeof(int));p[0]=9;goto *&&done;done:return p[0];}", 9),
        ("int main(void){int n=3;typedef int row[n++];goto *&&done;done:return sizeof(row)+n;}", 16),
        ("int main(void){int x=0;void *p=&&again;again:x++;if(x<3)goto *p;return x;}", 3),
        ("int main(void){int x=7;goto *&&entry;exit:return x;entry:goto exit;}", 7),
        ("int main(void){int x=7;static void *p=&&entry;goto dispatch;exit:return x;entry:goto exit;dispatch:goto *p;}", 7),
        ("int main(void){int n=0;return 3+({here:if(++n<3)goto *&&here;5;});}", 8),
        ("int main(void){int n=0;return 3+({here:if(++n<3)goto *&&here;4+({5;});});}", 12),
        ("int main(void){int n=0;int x=3+({here:if(++n<3)goto *&&here;goto done;5;});done:return n;}", 3),
        ("int main(void){int n=0;void *p=&&here;return 3+({here:if(++n<3)goto *p;5;});}", 8),
        ("int f(int n){int a[6];int (*p)[n]=(void *)a;goto *&&done;done:return (char *)(p+1)-(char *)p;}int main(void){return f(3);}", 12),
    ] {
        check_computed_goto(source, expected);
    }
}

#[test]
fn indirect_transport_does_not_read_uninitialized_contents() {
    let source = "int main(void){int x;goto *&&done;done:return x;}";
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    assert_eq!(
        interpret_scalar_function(
            &unit,
            function_symbol(&unit, "main"),
            InterpreterLimits::new(1000)
        )
        .unwrap_err()
        .code(),
        "E3001"
    );
    check_label_address(source, Err("E3001"), &["br.indirect"]);
}

#[test]
fn a_foreign_function_label_is_not_a_local_dispatch_target() {
    let source = "void *foreign(void){here:return &&here;}int main(void){void *own=&&done;void *p=foreign();if(!own)return 1;goto *p;done:return 0;}";
    let arena = Arena::default();
    let unit = parse_valid(source, &arena);
    assert_eq!(
        interpret_scalar_function(
            &unit,
            function_symbol(&unit, "main"),
            InterpreterLimits::new(1000)
        )
        .unwrap_err()
        .code(),
        "E3048"
    );
    check_label_address(source, Err("E3048"), &["br.indirect"]);
}

#[test]
fn undeclared_ordinary_goto_keeps_the_typed_diagnostic() {
    let arena = Arena::default();
    let diagnostic = parse("int main(void) { goto nowhere; }\n", &arena)
        .expect_err("an undeclared label must be rejected");
    assert_eq!(diagnostic.code, "E2920");
}
