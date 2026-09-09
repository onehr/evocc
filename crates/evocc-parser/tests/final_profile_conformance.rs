use evocc_chir::{CHirDecl, CHirUnit, InitAction, ObjectDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::{ParseDiagnostic, Parser};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    parse_with_layout(
        source,
        arena,
        evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu(),
    )
}

fn parse_with_layout<'arena>(
    source: &str,
    arena: &'arena Arena,
    layout: evocc_types::TargetDataLayout,
) -> Result<CHirUnit<'arena>, ParseDiagnostic> {
    let file = FileId::new(10_153);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex: {source}");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    parser.set_target_layout(layout);
    let unit = parser.parse_translation_unit_in(arena);
    match parser.diagnostic().cloned() {
        Some(diagnostic) => Err(diagnostic),
        None => Ok(unit),
    }
}

fn accepts(source: &str) {
    let arena = Arena::default();
    if let Err(error) = parse(source, &arena) {
        panic!(
            "must parse, got {}: {}\nsource: {source}",
            error.code, error.message
        );
    }
}

fn object<'arena>(unit: &CHirUnit<'arena>, name: &str) -> &'arena ObjectDecl<'arena> {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Object(object) if object.name.spelling == name => Some(object),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture declares {name}"))
}

#[test]
fn short_wchar_controls_wide_literal_type_and_concatenation() {
    let arena = Arena::default();
    parse_with_layout(concat!(
        "typedef unsigned short efi_char16_t; ",
        "static const efi_char16_t wide[] = L\"Mok\"; ",
        "static const efi_char16_t wide2[] = L\"\" \"abc\"; ",
        "_Static_assert(sizeof wide == 8, \"wide\"); ",
        "_Static_assert(sizeof wide2 == 8, \"concat\"); ",
        "_Static_assert(__builtin_types_compatible_p(typeof(L\"x\"), unsigned short[2]), \"element\");\n",
    ), &arena, evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu().with_short_wchar(true))
    .expect("the short target preserves the measured 16-bit literal layout");
}

#[test]
fn register_is_accepted_for_parameters_and_block_objects() {
    accepts("unsigned long f(register unsigned long value) { register int one = 1; return value + one; }\n");
}

#[test]
fn taking_the_address_of_a_register_object_is_e2928_at_the_operator() {
    let cases = [
        "void f(void) { register int value = 0; int *pointer = &value; }\n",
        "void f(register int value) { int *pointer = &value; }\n",
        concat!(
            "struct pair { int first; int second; }; ",
            "void f(void) { register struct pair value; int *pointer = &value.second; }\n",
        ),
        "void f(void) { register int values[2]; int *pointer = &values[1]; }\n",
    ];
    let failures = cases
        .into_iter()
        .filter_map(|source| {
            let arena = Arena::default();
            let expected_start =
                source.find('&').expect("fixture contains address operator") as u32;
            match parse(source, &arena) {
                Err(diagnostic)
                    if diagnostic.code == "E2928"
                        && diagnostic.message == "address-of-register-object"
                        && diagnostic.span.byte_start == expected_start
                        && diagnostic.span.byte_end == expected_start + 1 =>
                {
                    None
                }
                Err(diagnostic) => Some(format!(
                    "expected E2928 at &: got {} {} at {}..{} for {source:?}",
                    diagnostic.code,
                    diagnostic.message,
                    diagnostic.span.byte_start,
                    diagnostic.span.byte_end,
                )),
                Ok(_) => Some(format!("accepted address of register object: {source:?}")),
            }
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "register address failures: {failures:#?}"
    );
}

#[test]
fn va_list_is_an_object_type_with_target_layout() {
    accepts(concat!(
        "typedef __builtin_va_list saved_args; ",
        "struct warn_args { const char *fmt; saved_args args; }; ",
        "saved_args table[2]; ",
        "_Static_assert(sizeof(saved_args) == 32, \"va size\"); ",
        "_Static_assert(__alignof__(saved_args) == 8, \"va align\"); ",
        "_Static_assert(sizeof(struct warn_args) == 40, \"record size\"); ",
        "_Static_assert(__alignof__(struct warn_args) == 8, \"record align\");\n",
    ));
}

#[test]
fn wide_array_elements_and_va_arg_dereferences_are_modifiable() {
    let cases = [
        concat!(
            "struct fp { __uint128_t vregs[4]; }; ",
            "void put(struct fp *state, int i, __uint128_t value) { state->vregs[i] = value; }\n",
        ),
        concat!(
            "int take(int count, ...) { __builtin_va_list args; ",
            "__builtin_va_start(args, count); *__builtin_va_arg(args, int *) = 3; ",
            "__builtin_va_end(args); return 0; }\n",
        ),
    ];
    let failures = cases
        .into_iter()
        .filter_map(|source| {
            let arena = Arena::default();
            parse(source, &arena)
                .err()
                .map(|error| format!("{}: {}", error.code, error.message))
        })
        .collect::<Vec<_>>();
    assert!(
        failures.is_empty(),
        "modifiable-place failures: {failures:?}"
    );
}

#[test]
fn function_parameter_compatibility_ignores_top_level_qualifiers() {
    accepts(concat!(
        "typedef int const_fn(const int); typedef int plain_fn(int); ",
        "int implementation(int value) { return value; } ",
        "void bind(const_fn *source) { plain_fn *first = source; const_fn *second = implementation; first = second; }\n",
    ));
}

#[test]
fn function_pointer_arithmetic_uses_byte_scale_in_every_front_end_form() {
    accepts(concat!(
        "typedef unsigned int (*bpf_fn)(const void *, const void *); ",
        "void move(bpf_fn pointer, long offset) { ",
        "bpf_fn a = pointer + offset; bpf_fn b = offset + pointer; ",
        "pointer -= offset; pointer += offset; ++pointer; pointer--; ",
        "long distance = a - b; (void)distance; }\n",
    ));
}

#[test]
fn escape_e_is_the_ascii_escape_character() {
    accepts("_Static_assert('\\e' == 27, \"escape\"); int escape_value(void) { return '\\e'; }\n");
}

#[test]
fn the_address_of_a_string_literal_is_a_static_address_constant() {
    accepts(concat!(
        "typedef unsigned long kernel_ulong_t; struct id { kernel_ulong_t driver_data; }; ",
        "static const struct id ids[] = { ",
        "{ (0 ? 0 : (kernel_ulong_t)&\"PARPORT_SERIAL\") }, { 0 } };\n",
    ));
}

#[test]
fn address_and_null_comparisons_drive_conditional_address_constants() {
    accepts(concat!(
        "typedef int (*of_init_fn_1)(int); int fixed_init(int x) { return x; } ",
        "struct dev_id { const void *data; }; ",
        "static const struct dev_id table = { ",
        ".data = (fixed_init == (of_init_fn_1)((void *)0)) ? fixed_init : fixed_init };\n",
    ));
}

#[test]
fn initializer_plan_size_depends_on_written_entries_not_object_extent() {
    let source = "static int values[100000] = { [0] = 1, [99999] = 2 };\n";
    let arena = Arena::default();
    let unit = parse(source, &arena)
        .unwrap_or_else(|error| panic!("must parse, got {}: {}", error.code, error.message));
    let plan = object(&unit, "values")
        .initializer
        .as_ref()
        .expect("fixture has initializer")
        .plan();
    let explicit_writes = plan
        .actions()
        .iter()
        .filter(|action| !matches!(action, InitAction::ZeroScalar { .. }))
        .count();
    assert_eq!(explicit_writes, 2);
    assert!(
        plan.actions().len() <= 3,
        "the sparse plan expanded to {} actions",
        plan.actions().len()
    );
}

#[test]
fn string_literals_follow_character_pointer_conversions_at_every_site() {
    accepts(concat!(
        "struct q { const unsigned char *name; unsigned char *mode; }; ",
        "void set(struct q *value) { value->mode = \"unsupported\"; } ",
        "const struct q empty = { .name = \"\" }; ",
        "unsigned char *return_string(void) { return \"ok\"; } ",
        "void consume(unsigned char *); void pass(void) { consume(\"ok\"); }\n",
    ));
}
