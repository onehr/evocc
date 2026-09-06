//! Record values cross calls through deterministic caller-owned storage.

use evocc_chir::CHirUnit;
use evocc_common::{Arena, FileId};
use evocc_evoir::{render_module, validate_module, Inst, Module};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{AddressSpace, EvoIRType, ParamMark, RecordShape, Region, TargetDataLayout};

fn parse<'arena>(source: &str, arena: &'arena Arena) -> CHirUnit<'arena> {
    let file = FileId::new(10_306);
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
        .expect("record-call fixture lowers");
    validate_module(&module).expect("record-call module validates");
    module
}

fn record_program() -> &'static str {
    concat!(
        "struct S { int first; long second; }; ",
        "struct S identity(struct S value) { return value; } ",
        "int main(void) { struct S input = { 4, 9 }; ",
        "struct S output = identity(input); return (int)output.second; }\n",
    )
}

#[test]
fn record_parameter_and_return_destination_have_the_frozen_signature() {
    let module = lower(record_program());
    let identity = module
        .functions
        .iter()
        .find(|function| function.name == "identity")
        .expect("lowering emits identity");
    let parameter_types = identity
        .params
        .iter()
        .map(|parameter| parameter.ty.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        parameter_types,
        vec![
            EvoIRType::Ptr(AddressSpace::Generic),
            EvoIRType::Ptr(AddressSpace::Generic),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ]
    );
    assert_eq!(
        identity.ret_tys,
        vec![EvoIRType::Mem(Region::All), EvoIRType::World]
    );
    assert_eq!(identity.param_marks, vec![
        ParamMark::ByVal(RecordShape { size: 16, align: 8 }),
        ParamMark::Sret(RecordShape { size: 16, align: 8 }),
        ParamMark::None,
        ParamMark::None,
    ]);
}

#[test]
fn record_marks_agree_across_declarations_definitions_and_indirect_calls() {
    let module = lower("struct S { int a; long b; }; extern struct S external(struct S); struct S local(struct S x){return x;} struct S invoke(struct S (*fp)(struct S),struct S x){return fp(x);} struct S caller(struct S x){return invoke(local,external(x));}");
    let local = module.functions.iter().find(|f| f.name == "local").unwrap();
    let external = module.declares.iter().find(|f| f.name == "external").unwrap();
    assert!(!local.param_marks.is_empty());
    assert_eq!(local.param_marks, external.param_marks);
    let invoke = module.functions.iter().find(|f| f.name == "invoke").unwrap();
    let EvoIRType::FnPtr { marks, .. } = &invoke.params[0].ty else {panic!("expected function pointer")};
    assert_eq!(marks, &local.param_marks);
    let text = render_module(&module);
    assert!(text.contains("byval(16, 8)"), "{text}");
    assert!(text.contains("sret(16, 8)"), "{text}");
    assert_eq!(render_module(&evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()),text);
}

#[test]
fn caller_temporaries_use_a_reserved_deterministic_origin_range() {
    let first = lower(record_program());
    let second = lower(record_program());
    assert_eq!(render_module(&first), render_module(&second));

    let main = first
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("lowering emits main");
    let origins = main
        .blocks
        .iter()
        .flat_map(|block| block.insts.iter())
        .filter_map(|instruction| match instruction {
            Inst::Alloca { origin, .. } => Some(*origin),
            _ => None,
        })
        .collect::<Vec<_>>();
    let synthetic = origins
        .iter()
        .copied()
        .filter(|origin| origin & (1_u64 << 63) != 0)
        .collect::<Vec<_>>();
    let source_derived = origins
        .iter()
        .copied()
        .filter(|origin| origin & (1_u64 << 63) == 0)
        .collect::<Vec<_>>();
    assert_eq!(synthetic.len(), 2, "origins={origins:?}");
    assert!(!source_derived.is_empty(), "origins={origins:?}");
    assert!(
        synthetic
            .iter()
            .all(|origin| !source_derived.contains(origin)),
        "origins={origins:?}"
    );
}

#[test]
fn volatile_record_copy_remains_memberwise() {
    let module = lower(concat!(
        "struct S { int first; long second; }; ",
        "void copy(volatile struct S *to, volatile struct S *from) { *to = *from; }\n",
    ));
    let text = render_module(&module);
    assert_eq!(text.matches("load volatile").count(), 2, "{text}");
    assert_eq!(text.matches("store volatile").count(), 2, "{text}");
    assert!(!text.contains(" = copy "), "{text}");
}
