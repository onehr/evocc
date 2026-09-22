//! `typeof` (specification 10.1.10): a type resolved by the front end.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit, ParameterSyntax, WrittenDeclaratorLayer};
use evocc_common::{Arena, FileId, SourceFile};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{ArrayBound, IntRank, TargetDataLayout, TypeKind};

fn parse<'a>(source: &str, arena: &'a Arena) -> Result<CHirUnit<'a>, String> {
    let file = SourceFile::new(
        FileId::new(9_301),
        "gnu-typeof.c".to_owned(),
        source.to_owned(),
    );
    let mut lexer = Lexer::new(file.id, &file.contents);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        if token.kind == TokenKind::Error {
            return Err("lex: rejected input".to_owned());
        }
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file.id, &file.contents);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic() {
        Some(diagnostic) => Err(format!("{}: {}", diagnostic.code, diagnostic.message)),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!("must parse, got {error}\n  source: {source}");
    }
}

fn assert_parameter_ranks_without_panic(source: &str, name: &str, expected: &[IntRank]) {
    let arena = Arena::default();
    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(source, &arena)));
    let parsed = parsed.unwrap_or_else(|_| {
        panic!("valid nested typeof parameter declaration must not panic\n  source: {source}")
    });
    let unit = parsed.unwrap_or_else(|error| {
        panic!(
            "valid nested typeof parameter declaration must parse, got {error}\n  source: {source}"
        )
    });
    let function = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{name} must be retained as a function declaration"));
    let ParameterSyntax::Prototype {
        parameters,
        variadic: false,
    } = &function.parameters
    else {
        panic!("{name} must retain a non-variadic prototype")
    };
    assert_eq!(parameters.len(), expected.len(), "source: {source}");
    for (parameter, expected_rank) in parameters.iter().zip(expected) {
        let canonical = unit
            .types()
            .get(parameter.ty)
            .expect("parameter types are interned");
        assert_eq!(
            canonical.kind,
            TypeKind::Int(*expected_rank),
            "wrong parameter type in {name}: {source}"
        );
    }
}

fn rejects(source: &str, code: &str) {
    let arena = Arena::default();
    match parse(source, &arena) {
        Ok(_) => panic!("must reject with {code}, but parsed\n  source: {source}"),
        Err(error) => assert!(
            error.starts_with(&format!("{code}:")),
            "expected {code}, got {error}\n  source: {source}"
        ),
    }
}

fn layout_of(source: &str, name: &str) -> (u64, u32) {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("must parse, got {error}"));
    let symbol = unit
        .symbols()
        .resolve(unit.symbols().file_scope(), name)
        .expect("symbol table is live")
        .unwrap_or_else(|| panic!("{name} is not declared"));
    let ty = unit.symbols().symbol(symbol).expect("declared").ty();
    let layout = TargetDataLayout::aarch64_unknown_linux_gnu()
        .layout_of(unit.types(), ty)
        .unwrap_or_else(|error| panic!("layout of {name}: {error:?}"));
    (layout.size_bytes, layout.align_bytes)
}

fn run_main(source: &str) -> i128 {
    let arena = Arena::default();
    let unit = parse(source, &arena).unwrap_or_else(|error| panic!("must parse, got {error}"));
    let main = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function)
                if function.body.is_some() && function.name.spelling == "main" =>
            {
                Some(function.symbol)
            }
            _ => None,
        })
        .expect("fixture defines main");
    interpret_scalar_function(&unit, main, InterpreterLimits::new(200_000))
        .expect("fixture executes")
        .return_value
        .expect("main returns a value")
        .mathematical_value(unit.types())
        .expect("main returns an integer")
}

#[test]
fn typeof_of_a_type_name_is_that_type() {
    assert_eq!(layout_of("typeof(int) x;\n", "x"), (4, 4));
    assert_eq!(layout_of("__typeof__(long) x;\n", "x"), (8, 8));
    assert_eq!(layout_of("typeof(char *) x;\n", "x"), (8, 8));
}

#[test]
fn single_trailing_underscore_typeof_has_the_measured_value_semantics() {
    assert_eq!(
        run_main("int main(void) { int x = 41; __typeof(x) y = x + 1; return y; }\n"),
        42,
    );
}

#[test]
fn statement_expression_typeof_scope_discards_every_declaration_kind() {
    accepts(concat!(
        "void f(void) { ",
        "typeof(({ int local = 1; struct hidden { int x; }; ",
        "extern void linked(void); local; })) value; ",
        "int local = 2; struct hidden { long y; }; extern void linked(void); ",
        "(void)value; (void)local; }\n",
    ));
}

#[test]
fn compatible_linkage_redeclarations_are_accepted_and_conflicts_are_exact_e2002() {
    for source in [
        "void f(void) { extern void c(void); extern void c(void); }\n",
        "void f(void) { extern void c(void); { extern void c(void); } }\n",
        "void f(void) { ({ extern void c(void); 1; }); ({ extern void c(void); 1; }); }\n",
        "void f(void) { do { extern void c(void); } while (0); do { extern void c(void); } while (0); }\n",
        "void f(void) { extern void c(void) __attribute__((unused)); extern void c(void); }\n",
        "void f(void) { extern void c(void); extern void c(void) __attribute__((unused)); }\n",
    ] {
        accepts(source);
    }

    let arena = Arena::default();
    let error = parse("extern int g(int); extern long g(int);\n", &arena)
        .expect_err("incompatible redeclaration must be rejected");
    assert_eq!(error, "E2002: incompatible-redeclaration: g");
}

#[test]
fn typeof_of_an_expression_is_its_type_before_lvalue_conversion() {
    assert_eq!(layout_of("int a[3]; typeof(a) b;\n", "b"), (12, 4));
    assert_eq!(
        layout_of("const int c = 1; typeof(c) d = 2;\n", "d"),
        (4, 4)
    );
    rejects(
        "const int c = 1; typeof(c) d = 2; void f(void) { d = 3; }\n",
        "E2101",
    );
}

#[test]
fn typeof_operand_is_not_evaluated_but_its_written_form_is_retained() {
    let arena = Arena::default();
    let unit = parse("int i; typeof(i++) j;\n", &arena).expect("typeof must parse");
    let retained = format!("{unit:#?}");
    assert!(retained.contains("Typeof"), "{retained}");
    assert!(retained.contains("Expression"), "{retained}");
    assert!(!retained.contains("Update"), "{retained}");
}

#[test]
fn typeof_nests_and_composes_with_declarators() {
    assert_eq!(layout_of("typeof(typeof(int)) x;\n", "x"), (4, 4));
    assert_eq!(layout_of("typeof(int) *p;\n", "p"), (8, 8));
    assert_eq!(layout_of("typeof(int) a[2];\n", "a"), (8, 4));
    accepts("void f(typeof(int) x, typeof(x) y);\n");
    accepts("struct s { typeof(int) m; };\n");
    accepts("int g(void) { return sizeof(typeof(int)); }\n");
    accepts("int h(void) { return (typeof(int))1; }\n");
}

#[test]
fn nested_typeof_in_the_second_parameter_is_typed_without_panicking() {
    assert_parameter_ranks_without_panic(
        "long second(int a, typeof(__builtin_types_compatible_p(typeof(0L), long)) len);\n",
        "second",
        &[IntRank::Int, IntRank::Int],
    );
}

#[test]
fn nested_typeof_in_the_third_nonfinal_parameter_is_typed_without_panicking() {
    assert_parameter_ranks_without_panic(
        "long third(int a, long b, typeof(__builtin_types_compatible_p(typeof(0L), long)) len, unsigned int tail);\n",
        "third",
        &[IntRank::Int, IntRank::Long, IntRank::Int, IntRank::UInt],
    );
}

#[test]
fn nested_typeof_in_the_last_parameter_is_typed_without_panicking() {
    assert_parameter_ranks_without_panic(
        "long last(int a, long b, unsigned int c, typeof(__builtin_types_compatible_p(typeof(0L), long)) len);\n",
        "last",
        &[IntRank::Int, IntRank::Long, IntRank::UInt, IntRank::Int],
    );
}

#[test]
fn three_nested_typeof_levels_are_typed_without_panicking() {
    assert_parameter_ranks_without_panic(
        "long deep(int a, typeof(__builtin_types_compatible_p(typeof(__builtin_types_compatible_p(typeof(0L), long)), _Bool)) len);\n",
        "deep",
        &[IntRank::Int, IntRank::Int],
    );
}

#[test]
fn nested_typeof_may_resolve_an_earlier_parameter_without_panicking() {
    assert_parameter_ranks_without_panic(
        "long h(unsigned long start, __typeof(__builtin_choose_expr(__builtin_types_compatible_p(typeof(start), long), 1L, 1UL)) len);\n",
        "h",
        &[IntRank::ULong, IntRank::ULong],
    );
}

#[test]
fn parameter_bounds_resolve_the_prior_parameter_type() {
    for (source, expected) in [
        ("void f(long n, int data[sizeof(n)]);", 8),
        ("void f(char n, int data[sizeof(n)]);", 1),
        ("void f(long n, int data[sizeof(n++)]);", 8),
        ("void f(long n, int data[__alignof__(n)]);", 8),
        ("void f(int n[3], int data[sizeof(n)]);", 8),
        (
            "void f(int n __attribute__((unused)), int data[__builtin_has_attribute(n, unused)]);",
            1,
        ),
    ] {
        let arena = Arena::default();
        let unit = parse(source, &arena).unwrap_or_else(|error| panic!("{source}: {error}"));
        let function = unit
            .iter()
            .find_map(|declaration| match declaration {
                CHirDecl::Function(function) if function.name.spelling == "f" => Some(function),
                _ => None,
            })
            .unwrap();
        let parameter = &function.parameters.parameters()[1];
        assert!(matches!(
            unit.types().get(parameter.ty).unwrap().kind,
            TypeKind::Pointer(_)
        ));
        let [WrittenDeclaratorLayer::Array { bound, .. }] = parameter.written.layers.as_slice()
        else {
            panic!("array parameter must preserve its written bound");
        };
        assert_eq!(*bound, ArrayBound::Fixed(expected), "{source}");
    }
}

#[test]
fn parameter_bounds_shadow_outer_names_without_leaking() {
    for outer in ["enum { n = 2 };", "typedef char n;", "char n;"] {
        let source = format!("{outer} void f(long n, int data[sizeof(n)]);");
        let arena = Arena::default();
        let unit = parse(&source, &arena).unwrap_or_else(|error| panic!("{source}: {error}"));
        let function = unit
            .iter()
            .find_map(|declaration| match declaration {
                CHirDecl::Function(function) if function.name.spelling == "f" => Some(function),
                _ => None,
            })
            .unwrap();
        let [WrittenDeclaratorLayer::Array { bound, .. }] = function.parameters.parameters()[1]
            .written
            .layers
            .as_slice()
        else {
            panic!("array parameter must preserve its written bound");
        };
        assert_eq!(*bound, ArrayBound::Fixed(8), "{source}");
    }
    accepts("typedef char n; void f(long n, int data[sizeof(n)]); n object;");
    rejects(
        "void f(long n, int data[sizeof(n)]); int object = n;",
        "E2001",
    );
    rejects("void f(int n, int n[sizeof(n)]);", "E2002");
    rejects("void f(register int n, int data[sizeof(&n)]);", "E2928");
    rejects("void f(const int n, int data[sizeof(n++)]);", "E2101");
}

#[test]
fn parameter_bounds_adjust_after_name_resolution() {
    for source in [
        "void f(int n, int data[n]);",
        "void f(int n, int data[n]) {}",
        "void f(long n, void (*callback)(int data[n]));",
    ] {
        accepts(source);
    }
}

#[test]
fn typeof_captures_a_variable_type_name_but_refuses_conflicting_pointer_evaluation() {
    accepts("void f(int n) { typeof(int[n]) a; }\n");
    rejects(
        "void f(int n) { int a[n]; int calls=0; typeof((calls++, &a)) p=&a; }\n",
        "E2904",
    );
}

#[test]
fn the_kernel_read_once_line_parses() {
    accepts(concat!(
        "unsigned long f(unsigned long *addr) {\n",
        " return (*(const volatile typeof( _Generic((*(unsigned long *)addr), char: (char)0, ",
        "unsigned char: (unsigned char)0, signed char: (signed char)0, unsigned short: (unsigned short)0, ",
        "signed short: (signed short)0, unsigned int: (unsigned int)0, signed int: (signed int)0, ",
        "unsigned long: (unsigned long)0, signed long: (signed long)0, unsigned long long: (unsigned long long)0, ",
        "signed long long: (signed long long)0, default: (*(unsigned long *)addr))) *)&(*(unsigned long *)addr));\n",
        "}\n"
    ));
}
