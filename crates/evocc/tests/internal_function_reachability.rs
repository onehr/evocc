use std::fs;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

fn run(source: &str, mode: &str) -> Output {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "evocc-internal-{}-{}.c",
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
fn unused_internal_bodies_do_not_require_execution_support() {
    let mut failures = Vec::new();
    for declaration in [
        "static int hidden(int n){asm(\"\"::\"i\"(n));return n;}",
        "static int hidden(void *p){goto *p;}",
        "static long hidden(void){struct S{unsigned long x:40;}s={3};return s.x<<1;}",
        "static int hidden(void){struct __attribute__((scalar_storage_order(\"big-endian\"))) S{int x;}s={1};return s.x;}",
        "static void hidden(void){__builtin_exit(0);}",
        "static int hidden(void){return __builtin_puts(\"unused\");}",
        "static int hidden(int);static int other(int n){return hidden(n);}static int hidden(int n){asm(\"\"::\"i\"(n));return other(n);}",
        "static inline __attribute__((always_inline)) int hidden(int n){return n?hidden(n-1):0;}",
        "static int hidden(void){static double object;return sizeof object;}",
    ] {
        let source = format!("{declaration} int main(void){{return 0;}}");
        let output = run(&source, "--emit=evoir");
        if !output.status.success() {
            failures.push(format!("{declaration}: {}", String::from_utf8_lossy(&output.stderr)));
            continue;
        }
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        let names: Vec<_> = module.functions.iter().map(|function| function.name.as_str()).collect();
        assert_eq!(names, ["main"], "{declaration}");
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn live_references_retain_the_body_and_its_refusals() {
    let declaration = "static int hidden(int n){asm(\"\"::\"i\"(n));return n;}";
    for reference in [
        "int entry(int n){return hidden(n);}",
        "int entry(void){if(0)return hidden(1);return 0;}",
        "int (*pointer)(int)=hidden;",
        "extern int exposed(int) __attribute__((alias(\"hidden\")));",
        "asm(\".quad hidden\");",
        "int entry(void){asm(\"bl hidden\");return 0;}",
        "static int middle(int n){return hidden(n);}int entry(int n){return middle(n);}",
    ] {
        let output = run(&format!("{declaration}{reference}"), "--emit=evoir");
        assert!(!output.status.success(), "lost live body: {reference}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E3021"),
            "{reference}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    for reference in [
        "unsigned long entry(void){return hidden();}",
        "unsigned long (*pointer)(void)=hidden;",
        "asm(\".quad hidden\");",
    ] {
        let source = format!("static unsigned long hidden(void){{struct S{{unsigned long x:40;}}s={{3}};return s.x<<1;}}{reference}");
        let output = run(&source, "--emit=evoir");
        assert!(
            !output.status.success(),
            "lost profile refusal: {reference}"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2939"));
    }
}

#[test]
fn internal_bodies_still_receive_ordinary_front_end_checks() {
    for (source, code) in [
        (
            "static int hidden(void){return unknown;}int main(void){return 0;}",
            "E2001",
        ),
        (
            "static int hidden(void){int x;int x;return 0;}int main(void){return 0;}",
            "E2002",
        ),
    ] {
        let output = run(source, "--emit=evoir");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(code));
    }
}

#[test]
fn function_linkage_is_inherited_from_its_first_declaration() {
    let mut failures = Vec::new();
    for prefix in ["", "extern "] {
        let source = format!(
            "static int hidden(int); {prefix}int hidden(int n){{asm(\"\"::\"i\"(n));return n;}}int main(void){{return 0;}}"
        );
        let output = run(&source, "--emit=evoir");
        if !output.status.success() {
            failures.push(String::from_utf8_lossy(&output.stderr).into_owned());
        }
        let source = format!(
            "static int hidden(int); {prefix}int hidden(int n){{return n;}}int main(void){{return hidden(0);}}"
        );
        let output = run(&source, "--emit=evoir");
        assert!(output.status.success(), "{:?}", output.stderr);
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        if module
            .functions
            .iter()
            .find(|f| f.name == "hidden")
            .unwrap()
            .visibility
            != evocc_evoir::GlobalVisibility::Internal
        {
            failures.push(format!("{prefix:?} definition lost its internal linkage"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn deferred_profile_diagnostics_keep_their_source_order() {
    let body = "long selected(void){struct S{unsigned long x:40;}s={3};return s.x<<1;}";
    let tail = "int broken(void){return missing;}";
    let output = run(&format!("{body}{tail}"), "--parse-only");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(text.lines().next().unwrap().contains("E2939"), "{text}");
    let output = run(&format!("static {body}{tail}"), "--parse-only");
    let text = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(text.lines().next().unwrap().contains("E2001"), "{text}");
    assert!(!text.contains("E2939"), "{text}");
}

#[test]
fn directly_interpreting_an_unused_body_preserves_its_profile_refusal() {
    use evocc_chir::{interp, CHirDecl};
    use evocc_common::{Arena, FileId};
    use evocc_lex::Lexer;
    use evocc_parser::Parser;

    let source = "static long hidden(void){struct S{unsigned long x:40;}s={3};return s.x<<1;}int main(void){return 0;}";
    let arena = Arena::default();
    let file = FileId::new(1);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let function = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "hidden" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("parsing retains the source body");
    let error =
        interp::interpret_scalar_function(&unit, function, interp::InterpreterLimits::default())
            .unwrap_err();
    assert_eq!(error.code(), "E2939");
}

#[test]
fn removing_a_body_preserves_retained_allocation_origins() {
    use evocc_chir::{AllocationSiteManifest, CHirDecl};
    use evocc_common::{Arena, FileId};
    use evocc_lex::Lexer;
    use evocc_parser::Parser;

    let source = "static int hidden(void){static int dead;return dead;}volatile int live;int main(void){live=7;return live;}";
    let arena = Arena::default();
    let file = FileId::new(1);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let symbol = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == "live" => Some(object.symbol),
            _ => None,
        })
        .unwrap();
    let manifest = AllocationSiteManifest::derive(&unit).unwrap();
    let expected = manifest.object_site(symbol).unwrap().ordinal().raw();
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
    let actual = module
        .globals
        .iter()
        .find(|global| global.symbol == "live")
        .unwrap()
        .origin;
    assert_eq!(actual, expected);
}
