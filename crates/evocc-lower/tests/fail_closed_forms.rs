//! Fail-closed lowering for front-end-complete, IR-deferred forms.

use evocc_chir::AllocationSiteManifest;
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

#[test]
fn asm_scalar_result_bindings_do_not_require_addressable_objects() {
    for source in [
        "int f(int x){int y; asm(\"mov %w0,%w1\" : \"=r\"(y) : \"r\"(x)); return y;}",
        "int f(int x){asm(\"add %w0,%w0,#1\" : \"+r\"(x)); return x;}",
        "double f(double x){asm(\"fneg %d0,%d0\" : \"+w\"(x)); return x;}",
        "int f(int x){int y; asm goto(\"mov %w0,%w1; cbnz %w1,%l[L]\" : \"=r\"(y) : \"r\"(x) : : L); return y+1; L:return y;}",
        "int *f(int *x){int *y; asm(\"mov %0,%1\" : \"=r\"(y) : \"r\"(x)); return y;}",
        "unsigned char f(unsigned char x){asm(\"add %w0,%w0,#1\" : \"+r\"(x)); return x;}",
        "int f(int x){register int y asm(\"x19\")=x; asm(\"add %w0,%w0,#1\" : \"+r\"(y)); return y;}",
        "int f(int x){while(x){asm(\"sub %w0,%w0,#1\" : \"+r\"(x));} return x;}",
    ] {
        for optimize in [false, true] {
            let text = asm_binding_ir(source, optimize);
            assert!(!text.contains(" = alloca "), "{text}");
            assert!(!text.contains(" = load "), "{text}");
            assert!(!text.contains(" = store "), "{text}");
        }
    }
}

#[test]
fn asm_result_bindings_preserve_required_object_storage() {
    for source in [
        "int f(int x){volatile int y; asm(\"mov %w0,%w1\" : \"=r\"(y) : \"r\"(x)); return y;}",
        "extern void escape(int *); int f(int x){int y; escape(&y); asm(\"mov %w0,%w1\" : \"=r\"(y) : \"r\"(x)); return y;}",
        "int f(int x){int y; asm(\"str %w1,%0\" : \"=Q\"(y) : \"r\"(x)); return y;}",
        "int f(int x){int y; asm(\"mov %w0,%w1\" : \"=r\"(y) : \"r\"(x)); void *p=&&L; goto *p; L:return y;}",
    ] {
        for optimize in [false, true] {
            let text = asm_binding_ir(source, optimize);
            assert!(text.contains(" = alloca "), "{text}");
        }
    }
}

fn asm_binding_ir(source: &str, optimize: bool) -> String {
    let arena = Arena::default();
    let file = FileId::new(9_999);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module = evocc_lower::lower_unit_with_optimization(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        optimize,
    )
    .unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    evocc_evoir::render_module(&module)
}

#[test]
fn measured_refusal_classes_do_not_hide_behind_the_lowering_wrapper() {
    assert_lowering_error(
        "int f() { return 1; } int main(void) { return f(1); }",
        306,
        "E3051",
        "malformed function f: call arity disagrees with the canonical prototype",
    );
}

/// L04: a `float`, `double` or `long double` complex value passes as an
/// HFA of its two parts, so these interfaces lower; the measured refusals
/// 307 to 309 no longer apply.
#[test]
fn floating_complex_call_interfaces_lower() {
    for source in [
        "_Complex float f(_Complex float x) { return x; }",
        "_Complex double f(_Complex double x) { return x; }",
        "_Complex long double f(_Complex long double x) { return x; }",
    ] {
        for optimize in [false, true] {
            asm_binding_ir(source, optimize);
        }
    }
}

#[test]
fn target_floating_call_interfaces_use_the_named_refusal() {
    let interfaces = |spelling: &str| {
        [
            format!("{spelling} identity({spelling} value) {{ return value; }}"),
            format!("int ignore({spelling} value) {{ return 0; }}"),
            format!("extern {spelling} external(void); int main(void) {{ external(); return 0; }}"),
            format!("int callback({spelling} (*function)({spelling})) {{ return 0; }}"),
            format!("typedef {spelling} signature({spelling}); signature *address(void) {{ return 0; }}"),
        ]
    };
    for spelling in [
        "_Float16",
        "_Float32",
        "_Float64",
        "_Float128",
        "_Float32x",
        "_Float64x",
        "__fp16",
        "__bf16",
    ] {
        // L03: the half formats cross a call in a vector register (EvoIR
        // 3.13.6). The other target floating types keep the refusal.
        let half = matches!(spelling, "_Float16" | "__fp16" | "__bf16");
        for source in interfaces(spelling) {
            if half {
                for optimize in [false, true] {
                    asm_binding_ir(&source, optimize);
                }
            } else {
                assert_lowering_error(
                    &source,
                    310,
                    "E3049",
                    "not-lowered: floating-call-interface",
                );
            }
        }
        // Equal-sized nominal types are not scalar aliases; pointer and layout
        // use does not justify refusing the containing ordinary function.
        asm_binding_ir(
            &format!("unsigned size(const {spelling} *p) {{ return sizeof(*p); }}"),
            false,
        );
    }
    // AAPCS64 makes a record of `__bf16` members an HFA, and GCC 14 passes it
    // in general registers, so the interface keeps the named refusal.
    let interfaces = |record: &str| {
        [
            format!("{record} struct B identity(struct B value) {{ return value; }}"),
            format!("{record} int ignore(struct B value) {{ return 0; }}"),
            format!("{record} extern struct B external(void); int main(void) {{ external(); return 0; }}"),
        ]
    };
    for record in ["struct B { __bf16 a, b; };", "struct B { __bf16 a[3]; };"] {
        for source in interfaces(record) {
            assert_lowering_error(
                &source,
                311,
                "E3049",
                "not-lowered: floating-call-interface",
            );
        }
    }
    // A record of both half formats is no HFA, because AAPCS64 gives them two
    // fundamental types: on Linux it is an ordinary composite, as in GCC 14.
    for source in interfaces("struct B { __bf16 a; _Float16 b; };") {
        for optimize in [false, true] {
            asm_binding_ir(&source, optimize);
        }
    }
    for optimize in [false, true] {
        asm_binding_ir(
            "struct H { _Float16 a, b, c; }; struct H identity(struct H value) { return value; }",
            optimize,
        );
    }
}

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
fn ancestor_addresses_lower_to_null_and_not_to_machine_frame_reads() {
    let ir = asm_binding_ir("void *f(void) { return __builtin_frame_address(1); } void *g(void) { return __builtin_return_address(65535); }", false);
    assert!(
        !ir.contains("frame_address") && !ir.contains("return_address"),
        "{ir}"
    );
    assert_eq!(ir.matches("ptr.null").count(), 2, "{ir}");
}

#[test]
fn function_pointer_arithmetic_uses_exposed_address_instructions() {
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
                Err(error) => Some(format!(
                    "case {index}: expected address arithmetic, got {} {error}",
                    error.code()
                )),
                Ok(module) => {
                    let text = evocc_evoir::render_module(&module);
                    (!text.contains("ptr.to_int") || !text.contains("ptr.from_int.fnptr"))
                        .then(|| format!("case {index}: missing exposed-address conversion"))
                }
            }
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "function-pointer lowering failures: {failures:#?}"
    );
}

/// L04: a record of floating complex members is an HFA or a general
/// composite, also as an anonymous argument.
#[test]
fn floating_complex_record_variable_arguments_lower() {
    for source in [
        "struct S { _Complex long double x; }; int f(int n, ...); int pass(struct S s) { return f(1, s); }\n",
        "struct S { _Complex long double x; }; int f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); struct S s = __builtin_va_arg(ap, struct S); return 0; }\n",
        "struct S { char tag; _Complex double z; }; int f(int n, ...); int pass(struct S s) { return f(1, s); }\n",
    ] {
        for optimize in [false, true] {
            asm_binding_ir(source, optimize);
        }
    }
}

/// L03: a record with a half member is an ordinary variadic record, and a
/// binary16 HFA is an HFA. A record that AAPCS64 makes an HFA because of a
/// `__bf16` member keeps the named refusal, and a member type without
/// admitted values keeps the variadic refusal.
#[test]
fn half_precision_record_variable_arguments_follow_the_record_abi() {
    for member in [
        "__fp16 x; int tag;",
        "_Float16 x; int tag;",
        "__bf16 x; int tag;",
        "_Float16 x, y;",
    ] {
        for optimize in [false, true] {
            asm_binding_ir(
                &format!("struct S {{ {member} }}; int f(int n, ...); int pass(struct S s) {{ return f(1, s); }}"),
                optimize,
            );
            asm_binding_ir(
                &format!("struct S {{ {member} }}; int f(int n, ...) {{ __builtin_va_list ap; __builtin_va_start(ap, n); struct S s = __builtin_va_arg(ap, struct S); return 0; }}"),
                optimize,
            );
        }
    }
    for (ordinal, source, code, message) in [
        (
            5,
            "struct S { _Float32 x; int tag; }; int f(int n, ...); int pass(struct S s) { return f(1, s); }\n",
            "E3030",
            "not-lowered: varargs",
        ),
        (
            6,
            "struct S { _Float32 x; int tag; }; int f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); struct S s = __builtin_va_arg(ap, struct S); return 0; }\n",
            "E3030",
            "not-lowered: varargs",
        ),
        (
            7,
            "struct B { __bf16 x, y; }; int f(int n, ...); int pass(struct B s) { return f(1, s); }\n",
            "E3049",
            "not-lowered: floating-call-interface",
        ),
        (
            8,
            "struct B { __bf16 x, y; }; int f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap, n); struct B s = __builtin_va_arg(ap, struct B); return 0; }\n",
            "E3049",
            "not-lowered: floating-call-interface",
        ),
    ] {
        assert_lowering_error(source, ordinal, code, message);
    }
}
