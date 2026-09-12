//! Aggregate C forms lower through the single EvoIR object-copy primitive.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as CHirLimits};
use evocc_chir::{CHirDecl, CHirUnit, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, validate_module, InterpreterLimits as EvoIrLimits, Module,
    RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_203);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error, "fixture must lex");
        tokens.push(token);
    }
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "fixture must parse: {:?}\nsource: {source}",
        parser.diagnostic()
    );
    unit
}

fn lower(source: &str) -> Module {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("aggregate-copy fixture lowers");
    validate_module(&module).expect("aggregate-copy module validates");
    module
}

fn copy_count(module: &Module) -> usize {
    render_module(module)
        .lines()
        .filter(|line| line.contains(" = copy "))
        .count()
}

#[test]
fn volatile_union_snapshots_preserve_representation_and_only_requested_events() {
    use evocc_memory::VolatileDirection::{Load, Store};
    for (source_qualifier, destination_qualifier, directions) in [
        ("const volatile", "", vec![Load]),
        ("", "volatile", vec![Store]),
        ("const volatile", "volatile", vec![Load, Store]),
    ] {
        let source = format!("union U{{struct{{unsigned char a,b,c,d;}}bytes;unsigned word;}};union U source={{.word=0x44332211}};{destination_qualifier} union U destination;int main(void){{destination=*({source_qualifier} union U *)&source;return ((union U *)&destination)->word!=0x44332211;}}");
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let text = render_module(&module);
        assert_eq!(
            render_module(&evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()),
            text
        );
        let chir = interpret_scalar_function(
            &unit,
            function_symbol(&unit, "main"),
            CHirLimits::new(200_000),
        )
        .unwrap();
        let ir = interpret_module(
            &module,
            "main",
            &[
                RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            EvoIrLimits::new(200_000),
        )
        .unwrap();
        assert_eq!(chir.return_value.unwrap().raw_bits, 0);
        assert_eq!(ir.return_values[0].raw_bits, 0);
        for world in [&chir.world, &ir.world] {
            assert_eq!(
                world
                    .volatile_trace()
                    .iter()
                    .map(|event| event.direction)
                    .collect::<Vec<_>>(),
                directions,
                "{text}"
            );
            assert!(world
                .volatile_trace()
                .iter()
                .all(|event| event.access.size_bytes == 4));
        }
    }
}

#[test]
fn frozen_read_once_union_copy_runs_once_and_preserves_pointer_members() {
    use evocc_memory::VolatileDirection::{Load, Store};
    let source = include_str!("../../../tests/fixtures/volatile-union.c");
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let chir = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        CHirLimits::new(200_000),
    )
    .unwrap();
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let ir = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        EvoIrLimits::new(200_000),
    )
    .unwrap();
    assert_eq!(chir.return_value.unwrap().raw_bits, 0);
    assert_eq!(ir.return_values[0].raw_bits, 0);
    for world in [&chir.world, &ir.world] {
        assert_eq!(
            world
                .volatile_trace()
                .iter()
                .map(|event| (event.direction, event.access.size_bytes))
                .collect::<Vec<_>>(),
            [(Load, 4), (Load, 4), (Store, 4), (Load, 8)]
        );
    }
}

#[test]
fn record_and_union_assignments_each_emit_one_opaque_copy() {
    for source in [
        "struct S { int x; long y; }; void f(struct S *p, struct S *q) { *p = *q; }\n",
        "union U { long wide; int words[2]; }; void f(union U *p, union U *q) { *p = *q; }\n",
    ] {
        let module = lower(source);
        let text = render_module(&module);
        assert_eq!(copy_count(&module), 1, "{text}");
        assert!(
            !text.lines().any(|line| line.contains(" = load ")),
            "{text}"
        );
        assert!(
            !text.lines().any(|line| line.contains(" = store ")),
            "{text}"
        );
    }
}

#[test]
fn copy_initializers_emit_one_copy_instead_of_member_expansion() {
    for source in [
        "struct S { int x; long y; }; int f(struct S input) { struct S value = input; return value.x; }\n",
        "struct S { int x; long y; }; int f(struct S *input) { struct S value = *input; return value.x; }\n",
        "struct S { int x; long y; }; int f(void) { struct S value = (struct S){ 3, 4 }; return value.x; }\n",
    ] {
        let module = lower(source);
        assert_eq!(copy_count(&module), 1, "{}", render_module(&module));
    }
}

#[test]
fn volatile_aggregate_copy_is_memberwise_and_never_uses_copy() {
    let module = lower(concat!(
        "struct S { int x; long y; }; ",
        "void f(volatile struct S *destination, volatile struct S *source) { ",
        "*destination = *source; }\n",
    ));
    let text = render_module(&module);
    assert_eq!(copy_count(&module), 0, "{text}");
    assert_eq!(text.matches("load volatile").count(), 2, "{text}");
    assert_eq!(text.matches("store volatile").count(), 2, "{text}");
}

fn function_symbol(unit: &CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("fixture defines {name}"))
}

fn signed(value: &RuntimeValue) -> i128 {
    let EvoIRType::Bits(width @ 1..=64) = value.ty else {
        panic!("scalar result has unexpected type: {:?}", value.ty);
    };
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    };
    let raw = value.raw_bits & mask;
    let sign = 1_u64 << (width - 1);
    if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    }
}

#[test]
fn chir_and_evoir_agree_after_copying_a_three_member_record() {
    let source = concat!(
        "struct S { int first; int selected; long last; }; int main(void) { ",
        "struct S source = { 2, 7, 11 }; struct S destination = source; ",
        "return destination.selected; }\n",
    );
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let chir = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        CHirLimits::new(200_000),
    )
    .expect("C-HIR copy executes")
    .return_value
    .expect("main returns a value")
    .mathematical_value(unit.types())
    .expect("main returns an integer");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("copy program lowers");
    validate_module(&module).expect("copy program validates");
    assert_eq!(copy_count(&module), 1, "{}", render_module(&module));
    let main = module
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("lowering emits main");
    let arguments = match main.params.as_slice() {
        [] => Vec::new(),
        [memory, world]
            if memory.ty == EvoIRType::Mem(Region::All) && world.ty == EvoIRType::World =>
        {
            vec![
                RuntimeValue::new(memory.ty.clone(), 0),
                RuntimeValue::new(world.ty.clone(), 0),
            ]
        }
        parameters => panic!("unexpected main parameters: {parameters:?}"),
    };
    let evoir = interpret_module(&module, "main", &arguments, EvoIrLimits::new(200_000))
        .expect("EvoIR copy executes");
    assert_eq!(signed(&evoir.return_values[0]), chir);
}

#[test]
fn by_value_argument_return_and_result_initialization_use_one_mechanism() {
    let module = lower(concat!(
        "struct S { int x; int y; }; struct S identity(struct S value) { return value; } ",
        "int main(void) { struct S input = { 4, 9 }; struct S output = identity(input); ",
        "return output.y; }\n",
    ));
    assert_eq!(copy_count(&module), 3, "{}", render_module(&module));
}

#[test]
fn file_scope_flexible_array_initializer_extends_only_the_object() {
    let module = lower(
        "struct T { int n; int d[]; }; struct T object = { 1, { 2, 3 } }; int main(void) { return object.d[1]; }\n",
    );
    let object = module
        .globals
        .iter()
        .find(|global| global.symbol == "object")
        .expect("flexible object is materialized");
    assert_eq!(object.layout.size_bytes, 12);
    assert_eq!(object.layout.align_bytes, 4);
    assert_eq!(
        u32::from_le_bytes(object.bytes[8..12].try_into().unwrap()),
        3
    );
}

#[test]
fn flexible_string_initializers_preserve_object_extent_and_terminators() {
    let module = lower(include_str!(
        "../../../tests/fixtures/flexible-string-initializers.c"
    ));
    let expected: [(&str, &[u8]); 6] = [
        ("text", b"owx\0"),
        ("listed", b"lwx\0"),
        ("empty", b"e\0"),
        (
            "wide",
            &[7, 0, 0, 0, b'w', 0, 0, 0, b'x', 0, 0, 0, 0, 0, 0, 0],
        ),
        ("terminated", b"wx\0\0\0"),
        ("exact", b"wx"),
    ];
    for (name, bytes) in expected {
        let object = module
            .globals
            .iter()
            .find(|global| global.symbol == name)
            .unwrap();
        assert_eq!(object.layout.size_bytes, bytes.len() as u64, "{name}");
        assert_eq!(object.bytes, bytes, "{name}");
    }
}
