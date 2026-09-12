//! Fail-closed lowering for front-end-complete, IR-deferred forms.

use evocc_chir::AllocationSiteManifest;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn assert_lowering_error(source: &str, ordinal: u32, code: &str, message: &str) {
    let arena = Arena::default();
    let file = FileId::new(9_610 + ordinal);
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
    if matches!(code, "E3021" | "E3031") {
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("the operation has an IR representation");
        let function = &module.functions[0];
        let arguments = function
            .params
            .iter()
            .map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0))
            .collect::<Vec<_>>();
        let error = evocc_evoir::interpret_module(
            &module,
            &function.name,
            &arguments,
            evocc_evoir::InterpreterLimits::new(100),
        )
        .expect_err("native-only execution remains fail closed");
        assert_eq!(error.code(), code);
        assert!(error.to_string().contains(message));
        return;
    }
    let error = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect_err("lowering must fail closed");
    assert_eq!(error.code(), code, "wrong lowering diagnostic: {error}");
    assert_eq!(error.to_string(), message, "wrong lowering message");
}

#[test]
fn extended_asm_lowers_and_execution_fails_closed() {
    assert_lowering_error(
        "void f(void) { asm volatile(\"nop\" ::: \"memory\"); }\n",
        1,
        "E3021",
        "not-lowered: asm",
    );
}

#[test]
fn asm_goto_lowers_and_execution_fails_closed() {
    assert_lowering_error(
        "int h(void) { asm goto(\"b %l[o]\" : : : : o); return 0; o: return 1; }\n",
        2,
        "E3021",
        "not-lowered: asm",
    );
}

#[test]
fn return_address_lowers_to_an_activation_address() {
    let source = "void *f(void) { return __builtin_return_address(0); }\n";
    let arena = Arena::default();
    let file = FileId::new(9614);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    assert!(evocc_evoir::render_module(&module).contains(" = return_address : ptr<generic>"));
}

#[test]
fn ancestor_frame_address_lowers_and_execution_fails_closed_with_e3031() {
    assert_lowering_error(
        "void *f(void) { return __builtin_frame_address(1); }\n",
        8,
        "E3031",
        "not-executed: frame-address",
    );
}

#[test]
fn function_pointer_arithmetic_fails_closed_with_only_e3034() {
    let cases = [
        "typedef int (*fn)(void); fn f(fn pointer, long offset) { return pointer + offset; }\n",
        "typedef int (*fn)(void); fn f(fn pointer, long offset) { return offset + pointer; }\n",
        "typedef int (*fn)(void); fn f(fn pointer, long offset) { return pointer - offset; }\n",
        "typedef int (*fn)(void); fn f(fn pointer, long offset) { pointer += offset; return pointer; }\n",
        "typedef int (*fn)(void); fn f(fn pointer, long offset) { pointer -= offset; return pointer; }\n",
        "typedef int (*fn)(void); fn f(fn pointer) { return ++pointer; }\n",
        "typedef int (*fn)(void); fn f(fn pointer) { return pointer--; }\n",
        concat!(
            "typedef int (*fn)(void); struct holder { fn pointer; }; ",
            "fn f(struct holder *holder, long offset) { holder->pointer += offset; return holder->pointer; }\n",
        ),
    ];
    let failures = cases
        .into_iter()
        .enumerate()
        .filter_map(|(index, source)| {
            let arena = Arena::default();
            let file = FileId::new(9_700 + index as u32);
            let mut lexer = Lexer::new(file, source);
            let mut tokens = Vec::new();
            while let Some(token) = lexer.next() {
                assert_ne!(token.kind, TokenKind::Error, "lexer rejected case {index}");
                tokens.push(token);
            }
            let mut parser = Parser::new(tokens, file, source);
            let unit = parser.parse_translation_unit_in(&arena);
            if let Some(diagnostic) = parser.diagnostic() {
                return Some(format!(
                    "parser rejected case {index}: {} {}",
                    diagnostic.code, diagnostic.message
                ));
            }
            match lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()) {
                Err(error)
                    if error.code() == "E3034"
                        && error.to_string() == "not-lowered: function-pointer-arithmetic" =>
                {
                    None
                }
                Err(error) => Some(format!(
                    "case {index}: expected E3034, got {} {error}",
                    error.code()
                )),
                Ok(_) => Some(format!("case {index}: lowering unexpectedly succeeded")),
            }
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "function-pointer lowering failures: {failures:#?}"
    );
}

#[test]
fn floating_record_variable_arguments_remain_outside_the_integer_abi() {
    for (ordinal, source) in [
        (
            5,
            "struct S { double x; }; int f(int n, ...); int pass(struct S s) { return f(1, s); }\n",
        ),
        (
            6,
            "struct S { double x; }; int f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); struct S s = __builtin_va_arg(ap, struct S); return 0; }\n",
        ),
    ] {
        assert_lowering_error(
            source,
            ordinal,
            "E3030",
            "not-lowered: varargs",
        );
    }
}
