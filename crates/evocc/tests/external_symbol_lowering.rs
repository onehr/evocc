use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-external-{}-{}.c",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["--target=aarch64-unknown-linux-gnu", mode])
        .arg(&path)
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

#[test]
fn external_object_addresses_are_position_independent() {
    let source = include_str!("../../../tests/fixtures/external-object-pie.c");
    let output = run(source, "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let assembly = String::from_utf8(output.stdout).unwrap();
    assert!(assembly.contains(":got:environ"));
    assert!(assembly.contains("#:got_lo12:environ"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "external-object-address");
}

#[test]
fn unnamed_definition_parameters_preserve_call_positions_and_argument_effects() {
    let source = include_str!("../../../tests/fixtures/unnamed-definition-parameters.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir", "--emit=asm"] {
        let output = run(source, mode);
        if !output.status.success() {
            failures.push(format!(
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        } else if mode != "--emit=asm" && output.stdout != b"0\n" {
            failures.push(format!("{mode}: unexpected result {:?}", output.stdout));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn jumps_preserve_automatic_objects_and_skip_initializer_effects() {
    let source = include_str!("../../../tests/fixtures/goto-object-storage.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "goto-object-storage");
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn unnamed_definition_parameters_execute_with_the_positional_abi() {
    assert_native_execution(
        include_str!("../../../tests/fixtures/unnamed-definition-parameters.c"),
        "unnamed-parameters",
    );
}

#[test]
fn function_and_void_pointer_equality_preserves_addresses_and_effects() {
    let source = include_str!("../../../tests/fixtures/function-void-pointer-equality.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "mixed-pointer-equality");
}

#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn assert_native_execution(source: &str, case: &str) {
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let assembly =
        evocc_mir::compile_module_for(evocc_mir::Target::Darwin, &module, &|_| true).unwrap();
    let directory = std::env::temp_dir().join(format!("evocc-{case}-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let asm = directory.join("probe.s");
    let binary = directory.join("probe");
    fs::write(&asm, assembly).unwrap();
    let linked = Command::new("clang")
        .arg(&asm)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let executed = Command::new(&binary).output().unwrap();
    assert_eq!(
        executed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    fs::remove_file(binary).unwrap();
    fs::remove_file(asm).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn unprototyped_function_pointers_recover_the_promoted_call_interface() {
    let source = include_str!("../../../tests/fixtures/unprototyped-function-pointers.c");
    let mut failures = Vec::new();
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        if !output.status.success() || output.stdout != b"0\n" {
            failures.push(format!(
                "{mode}: {:?} {}",
                output.stdout,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "unprototyped-function-pointers");
}

#[test]
fn unprototyped_calls_still_reject_a_mismatched_dynamic_interface() {
    for arguments in ["7L", ""] {
        let source = format!(
            "int f(int x){{return x;}} int main(void){{int (*p)()=f; return p({arguments});}}"
        );
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(&source, mode);
            assert!(!output.status.success(), "{mode} accepted {source}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3019"),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn nested_switch_entry_executes_without_bypassed_conditions() {
    let source = include_str!("../../../tests/fixtures/nested-switch-labels.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "nested-switch-entry");
}

#[test]
fn discarded_expression_exits_execute_without_invented_fallthrough() {
    let source = include_str!("../../../tests/fixtures/discarded-expression-exits.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "discarded-expression-exits");
}

#[test]
fn constant_case_folding_does_not_waive_runtime_shift_checks() {
    let source = "int main(void) { int x = -2147483647-1; switch (x) {
        case 1 << 31: return 0; default: return 1; } }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
        let runtime = source.replace("case 1 << 31: return 0", "case 1 << 31: return 1 << 31");
        let output = run(&runtime, mode);
        assert!(!output.status.success(), "{mode} waived the executed shift");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3006"));
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "case-constant-folding");
}

#[test]
fn substituted_member_macro_keeps_its_single_object_projection() {
    let source = "struct S { int slot; };\n\
        #define slot state->slot\n\
        #define SET(s, value) ((s) = (value))\n\
        int main(void) { struct S object = {0}; struct S *state = &object;
          SET(slot, 7); return slot != 7; }";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "substituted-member-macro");
}

#[test]
fn enum_pointer_composites_preserve_the_signed_integer_representation() {
    let source = include_str!("../../../tests/fixtures/enum-compatible-pointers.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    #[cfg(all(target_arch = "aarch64", target_os = "macos"))]
    assert_native_execution(source, "enum-compatible-pointers");
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn undefined_weak_object_and_function_addresses_are_zero_natively() {
    let source = "extern int value __attribute__((weak)); extern int optional(void) __attribute__((weak)); int main(void){return optional != 0 || &value != 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let assembly =
        evocc_mir::compile_module_for(evocc_mir::Target::Darwin, &module, &|_| true).unwrap();
    let directory =
        std::env::temp_dir().join(format!("evocc-weak-addresses-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let asm = directory.join("probe.s");
    let binary = directory.join("probe");
    let reference_source = directory.join("reference.c");
    let reference = directory.join("reference");
    fs::write(&reference_source, source).unwrap();
    // Darwin's linker requires an explicit list of unresolved symbols even
    // for Clang's weak references. Exercise identical policy for both objects;
    // this does not turn a strong reference into a weak one.
    let unresolved = ["-Wl,-U,_optional,-U,_value"];
    let reference_link = Command::new("clang")
        .arg(&reference_source)
        .args(unresolved)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        reference_link.status.success(),
        "{}",
        String::from_utf8_lossy(&reference_link.stderr)
    );
    assert_eq!(Command::new(&reference).status().unwrap().code(), Some(0));
    fs::write(&asm, assembly).unwrap();
    let linked = Command::new("clang")
        .arg(&asm)
        .args(unresolved)
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let executed = Command::new(&binary).output().unwrap();
    assert_eq!(
        executed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&executed.stderr)
    );
    fs::remove_file(binary).unwrap();
    fs::remove_file(asm).unwrap();
    fs::remove_file(reference).unwrap();
    fs::remove_file(reference_source).unwrap();
    fs::remove_dir(directory).unwrap();
}

#[test]
fn referenced_external_symbols_lower_without_invented_bodies() {
    for (source, expected) in [
        (
            "extern int g(); int f(void){g((char)1);return g(2);}",
            "declare @g(bits<32>,",
        ),
        (
            "extern int g(int); int f(int x){return g(x);}",
            "declare @g(",
        ),
        (
            "extern int x; int f(void){return x;}",
            "extern @x size=4 align=4",
        ),
        (
            "void f(void*p){__builtin_memset(p,0,1);}",
            "declare @memset(",
        ),
        (
            "extern int g(int); int (*f(void))(int){return g;}",
            "declare @g(",
        ),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(expected), "{text}");
        assert!(!text.contains("func @g("));
        evocc_evoir::parse_section9_module(text.as_bytes()).expect("valid external references");
    }
}

#[test]
fn unresolved_runtime_symbols_fail_only_when_reached() {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        for (source, code) in [
            ("extern int g(void); int main(void){return g();}", "E3039"),
            ("extern int x; int main(void){return x;}", "E3040"),
            (
                "extern int isdigit(int); int main(void){return isdigit(48);}",
                "E3039",
            ),
            ("int main(void){return __builtin_isdigit(48);}", "E3039"),
            (
                "extern int g(void); int main(void){int (*p)(void)=g;return p();}",
                "E3039",
            ),
        ] {
            let output = run(source, mode);
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = run(
            "extern int g(void); extern int x; int main(void){if(0)return g()+x;return 0;}",
            mode,
        );
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn external_memory_library_executes_real_bytes() {
    let source = "int main(void){char a[8]; char b[8]; __builtin_memset(a,0,8); __builtin_memcpy(b,a,8); __builtin_memmove(b+1,b,4); return __builtin_memcmp(a,b,8) + __builtin_strcmp(a,b) + __builtin_strncmp(a,b,8) + __builtin_strlen(a);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert_eq!(
            output.stdout,
            b"0\n",
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (source, expected) in [
            ("int main(void){char a[8]=\"abc\"; __builtin_memmove(a+1,a,4); return __builtin_strlen(a) + 10*__builtin_strcmp(a,\"aabc\");}", "4\n"),
            ("int main(void){char a[4],b[4];a[0]=7;__builtin_memcpy(b,a,4);return b[0];}", "7\n"),
            ("int main(void){char a[4];return (__builtin_memset(a,42,4)==a)+a[3];}", "43\n"),
        ] {
            let output = run(source, mode);
            assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, expected.as_bytes());
        }
        let output = run(
            "int main(void){char a[4],b[4];a[0]=7;__builtin_memcpy(b,a,4);return b[1];}",
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn unprototyped_external_calls_require_one_promoted_signature() {
    let output = run(
        "extern int g(); int f(void){g(1);return g(1L);}",
        "--emit=evoir",
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("E2931"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn external_variadic_calls_promote_anonymous_arguments() {
    let source =
        "extern int log_value(int, ...); int main(void){return log_value(1, (char)2, (short)3);}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(
        text.contains("declare @log_value(bits<32>, ..., mem<all>, world)"),
        "{text}"
    );
    evocc_evoir::parse_section9_module(text.as_bytes()).expect("promoted variadic call");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3039"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn external_provenance_has_the_same_volatile_observation_in_both_interpreters() {
    use evocc_chir::CHirDecl;
    use evocc_common::{Arena, FileId};
    use evocc_lex::Lexer;
    use evocc_parser::Parser;
    use evocc_types::TargetDataLayout;

    for source in [
        "extern int unused; extern int z; extern int x; int *volatile p; int main(void){p=&z;p=&x;return p==&x;}",
        "extern int unused(void); extern int z(void); extern int x(void); int (*volatile p)(void); int main(void){p=z;p=x;return p==x;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45_001);
        let mut lexer = Lexer::new(file, source);
        let mut tokens = Vec::new();
        while let Some(token) = lexer.next() {
            assert_ne!(token.kind, evocc_lex::TokenKind::Error);
            tokens.push(token);
        }
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let symbol = unit.iter().find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "main" => Some(function.symbol),
            _ => None,
        }).unwrap();
        let chir = evocc_chir::interp::interpret_scalar_function(&unit, symbol, evocc_chir::interp::InterpreterLimits::new(100_000)).unwrap();
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let function = module.functions.iter().find(|function| function.name == "main").unwrap();
        let arguments = function.params.iter().map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0)).collect::<Vec<_>>();
        let evoir = evocc_evoir::interpret_module(&module, "main", &arguments, evocc_evoir::InterpreterLimits::new(100_000)).unwrap();
        assert_eq!(chir.world.volatile_trace(), evoir.world.volatile_trace(), "{source}");
    }
}

#[test]
fn library_arguments_are_converted_to_the_declared_parameter_type() {
    let source = "int main(void){char a[300];__builtin_memset(a,0,300);return __builtin_memcmp(a,a,(signed char)-1);}";
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            !output.status.success(),
            "{mode}: a negative signed count must convert to size_t, not 255"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3011"),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
