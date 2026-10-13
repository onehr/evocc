//! Object-size checked library builtins, the GNU argument pack and the
//! diagnostic for an unknown builtin, at the front-end boundary.

use evocc_chir::{CHirBlockItem, CHirDecl, CHirExpr, CHirStmt, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

const SIZE_T: &str = "typedef unsigned long size_t; ";

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_207);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    parser.enable_lints();
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => {
            // Lint 2 is the implicit function declaration.
            assert!(
                parser
                    .lint_findings()
                    .iter()
                    .all(|finding| finding.lint != 2),
                "implicit declaration: {source}"
            );
            Ok(unit)
        }
    }
}

fn rejects(source: &str, code: &str) -> ParseDiagnostic {
    let arena = Arena::default();
    let error = parse(source, &arena)
        .err()
        .unwrap_or_else(|| panic!("expected {code}: {source}"));
    assert_eq!(error.code, code, "{}: {source}", error.message);
    error
}

/// The callee spelling and argument count of the call that function `f`
/// returns.
fn returned_call(source: &str) -> (String, usize) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| {
        panic!("{} {}: {source}", error.code, error.message);
    });
    for declaration in unit.iter() {
        let CHirDecl::Function(function) = declaration else {
            continue;
        };
        let (Some(CHirStmt::Compound(items, _, _)), "f") =
            (function.body, function.name.spelling.as_str())
        else {
            continue;
        };
        for item in items {
            let CHirBlockItem::Statement(CHirStmt::Return(Some(mut value), _)) = item else {
                continue;
            };
            while let CHirExpr::Cast(_, operand, _) = value {
                value = operand;
            }
            if let CHirExpr::Call(CHirExpr::Ident { spelling, .. }, arguments, _) = value {
                return (spelling.clone(), arguments.len());
            }
        }
    }
    panic!("f returns no direct call: {source}");
}

#[test]
fn an_unknown_size_calls_the_plain_function() {
    for (call, plain, arguments) in [
        ("__builtin___memcpy_chk(d, s, n, (size_t)-1)", "memcpy", 3),
        ("__builtin___memmove_chk(d, s, n, -1)", "memmove", 3),
        ("__builtin___mempcpy_chk(d, s, n, ~0ul)", "mempcpy", 3),
        ("__builtin___memset_chk(d, 0, n, -1)", "memset", 3),
        ("__builtin___strcpy_chk(c, t, -1)", "strcpy", 2),
        ("__builtin___stpcpy_chk(c, t, -1)", "stpcpy", 2),
        ("__builtin___strcat_chk(c, t, -1)", "strcat", 2),
        ("__builtin___strncpy_chk(c, t, n, -1)", "strncpy", 3),
        ("__builtin___stpncpy_chk(c, t, n, -1)", "stpncpy", 3),
        ("__builtin___strncat_chk(c, t, n, -1)", "strncat", 3),
        (
            "__builtin___sprintf_chk(c, 0, -1, t, 1, 2.0f)",
            "sprintf",
            4,
        ),
        (
            "__builtin___snprintf_chk(c, n, 0, -1, t, 'x')",
            "snprintf",
            4,
        ),
        ("__builtin___vsprintf_chk(c, 0, -1, t, ap)", "vsprintf", 3),
        (
            "__builtin___vsnprintf_chk(c, n, 0, -1, t, ap)",
            "vsnprintf",
            4,
        ),
    ] {
        let source = format!(
            "{SIZE_T}long f(void *d, const void *s, char *c, const char *t, size_t n, __builtin_va_list ap) {{ return (long)({call}); }}"
        );
        assert_eq!(
            returned_call(&source),
            (plain.to_owned(), arguments),
            "{call}"
        );
    }
}

#[test]
fn a_known_size_keeps_the_check_unless_a_constant_length_fits() {
    let declarations = format!("{SIZE_T}char buffer[8]; ");
    for (call, callee, arguments) in [
        ("__builtin___memcpy_chk(buffer, s, n, 8)", "__memcpy_chk", 4),
        ("__builtin___memcpy_chk(buffer, s, 8, 8)", "memcpy", 3),
        ("__builtin___memcpy_chk(buffer, s, 9, 8)", "__memcpy_chk", 4),
        (
            "__builtin___strncat_chk(buffer, s, 1, 8)",
            "__strncat_chk",
            4,
        ),
        ("__builtin___strcpy_chk(buffer, s, 8)", "__strcpy_chk", 3),
        (
            "__builtin___snprintf_chk(buffer, 8, 0, 8, s)",
            "snprintf",
            3,
        ),
        (
            "__builtin___snprintf_chk(buffer, 9, 0, 8, s)",
            "__snprintf_chk",
            5,
        ),
        // A nonzero flag asks the C library for format checks.
        (
            "__builtin___snprintf_chk(buffer, 8, 1, 8, s)",
            "__snprintf_chk",
            5,
        ),
        (
            "__builtin___sprintf_chk(buffer, 1, -1, s)",
            "__sprintf_chk",
            4,
        ),
        (
            "__builtin___sprintf_chk(buffer, flag, -1, s)",
            "__sprintf_chk",
            4,
        ),
        (
            "__builtin___memcpy_chk(buffer, s, n, __builtin_object_size(buffer, 0))",
            "__memcpy_chk",
            4,
        ),
        (
            "__builtin___memcpy_chk(buffer, s, 8, __builtin_object_size(buffer, 0))",
            "memcpy",
            3,
        ),
        (
            "__builtin___memcpy_chk(buffer, s, sizeof buffer, __builtin_object_size(buffer, 0))",
            "memcpy",
            3,
        ),
    ] {
        let source = format!(
            "{declarations}long f(const char *s, size_t n, int flag) {{ return (long)({call}); }}"
        );
        assert_eq!(
            returned_call(&source),
            (callee.to_owned(), arguments),
            "{call}"
        );
    }
}

#[test]
fn a_deferred_object_size_query_is_not_a_parse_time_constant() {
    // Substitution can still find the extent behind a parameter, so the
    // parser keeps the checked entry point for it.
    let source = format!(
        "{SIZE_T}static inline __attribute__((always_inline)) char *f(char *d, const char *s) {{ return __builtin___strcpy_chk(d, s, __builtin_object_size(d, 1)); }}"
    );
    assert_eq!(returned_call(&source), ("__strcpy_chk".to_owned(), 3));
}

#[test]
fn checked_builtins_have_their_library_prototypes() {
    rejects(
        &format!("{SIZE_T}void *f(void *d) {{ return __builtin___memcpy_chk(d, d, 1); }}"),
        "E2909",
    );
    rejects(
        &format!("{SIZE_T}int f(char *d) {{ return __builtin___sprintf_chk(d, 0, 1); }}"),
        "E2909",
    );
    rejects(
        &format!("{SIZE_T}struct s {{ int x; }}; void *f(void *d, struct s v) {{ return __builtin___memcpy_chk(d, d, v, 1); }}"),
        "E2913",
    );
    rejects(
        &format!("{SIZE_T}int memcpy(void); void *f(void *d) {{ return __builtin___memcpy_chk(d, d, 1, 1); }}"),
        "E2927",
    );
    rejects(
        &format!("{SIZE_T}void *__memcpy_chk(void *, const void *, size_t); void *f(void *d) {{ return __builtin___memcpy_chk(d, d, 1, 2); }}"),
        "E2927",
    );
}

#[test]
fn an_unknown_builtin_call_names_the_builtin_where_it_is_spelled() {
    let source = "int f(void) { return __builtin_not_provided(1, 2); }";
    let error = rejects(source, "E2949");
    assert_eq!(error.message, "unknown-builtin: __builtin_not_provided");
    let start = source.find("__builtin_not_provided").unwrap();
    assert_eq!(error.span.byte_start as usize, start);
    assert_eq!(
        error.span.byte_end as usize,
        start + "__builtin_not_provided".len()
    );
    // The diagnostic does not depend on the lint level or language mode.
    let arena = Arena::default();
    let file = FileId::new(10_208);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    parser.set_legacy_declarations(true);
    parser.parse_translation_unit_in(&arena);
    assert_eq!(parser.diagnostic().map(|error| error.code), Some("E2949"));
    // An implicit `int` result never reaches a pointer return: the call is
    // refused at the builtin, not as an invalid initializer.
    rejects(
        "void *f(void *d) { return __builtin___unknown_chk(d, d, 1, 2); }",
        "E2949",
    );
    // Other uses remain undeclared identifiers.
    rejects(
        "int f(void) { return __builtin_not_provided != 0; }",
        "E2001",
    );
    // The atomic and legacy synchronization namespaces are reserved too.
    for (call, name) in [
        (
            "(int)__atomic_fetch_add(&value, 1, 5)",
            "__atomic_fetch_add",
        ),
        ("__sync_fetch_and_frob(&value, 1)", "__sync_fetch_and_frob"),
    ] {
        let error = rejects(
            &format!("unsigned value; int f(void) {{ return {call}; }}"),
            "E2949",
        );
        assert_eq!(error.message, format!("unknown-builtin: {name}"));
    }
    // A declared function in a reserved namespace is an ordinary call.
    let arena = Arena::default();
    assert!(parse(
        "unsigned long __atomic_load_16(const volatile void *, int); unsigned long f(void *p) { return __atomic_load_16(p, 5); }",
        &arena,
    )
    .is_ok());
}

#[test]
fn the_argument_pack_forwards_only_as_the_last_anonymous_argument() {
    let target = "int g(int, ...); int h(int, int); ";
    let accepted = [
        "int f(int n, ...) { return g(n, __builtin_va_arg_pack()); }",
        "int f(int n, ...) { return g(n, 1, __builtin_va_arg_pack()) + __builtin_va_arg_pack_len(); }",
        "int f(char *d, ...) { return __builtin___sprintf_chk(d, 0, -1, \"%d\", __builtin_va_arg_pack()); }",
        "int f(const char *s, ...) { return __builtin_printf(s, __builtin_va_arg_pack()); }",
    ];
    for source in accepted {
        let arena = Arena::default();
        let source = format!("{SIZE_T}{target}{source}");
        if let Err(error) = parse(&source, &arena) {
            panic!("{} {}: {source}", error.code, error.message);
        }
    }
    for source in [
        "int f(int n) { return g(n, __builtin_va_arg_pack()); }",
        "int f(int n, ...) { return g(__builtin_va_arg_pack(), n); }",
        "int f(int n, ...) { return h(n, __builtin_va_arg_pack()); }",
        "int f(int n, ...) { int x = __builtin_va_arg_pack(); return x; }",
        "int f(int n) { return __builtin_va_arg_pack_len(); }",
        "int f(char *d, ...) { return (int)(long)__builtin___memcpy_chk(d, d, 1, 1, __builtin_va_arg_pack()); }",
        "int f(const char *s, ...) { return (int)__builtin_strlen(s, __builtin_va_arg_pack()); }",
    ] {
        rejects(&format!("{SIZE_T}{target}{source}"), "E2950");
    }
}
