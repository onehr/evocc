//! Regression contract for the first full-configuration lowering frontier.

use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits as ChirLimits};
use evocc_chir::{AllocationSiteManifest, CHirDecl, SymbolId};
use evocc_common::{Arena, FileId};
use evocc_evoir::{
    interpret_module, render_module, InterpreterLimits as EvoIrLimits, RuntimeValue,
};
use evocc_lex::{Lexer, TokenKind};
use evocc_lower::lower_unit;
use evocc_parser::Parser;
use evocc_types::{EvoIRType, TargetDataLayout};

fn parse<'arena>(arena: &'arena Arena, source: &str, ordinal: u32) -> evocc_chir::CHirUnit<'arena> {
    let file = FileId::new(10_200 + ordinal);
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
    let unit = parser.parse_translation_unit_in(arena);
    assert!(
        !parser.had_error(),
        "parser rejected case {ordinal}: {:?}",
        parser.diagnostic()
    );
    unit
}

#[test]
fn address_of_forward_declared_object_survives_record_completion() {
    for (ordinal, kind, member) in [
        (100, "struct", "int member;"),
        (101, "union", "int member; long other;"),
        (102, "struct", "const int member;"),
    ] {
        let source = format!(
            "{kind} item; extern {kind} item object; \
             {kind} item *address(void) {{ return &object; }} \
             {kind} item {{ {member} }};"
        );
        let arena = Arena::default();
        let unit = parse(&arena, &source, ordinal);
        AllocationSiteManifest::derive(&unit)
            .expect("completing a tag preserves an earlier address");
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("address of an external object lowers after tag completion");
        assert!(render_module(&module).contains("global_addr @object"));
    }
}

#[test]
fn global_register_declaration_has_no_allocation_site_or_global() {
    let source = "register unsigned long sp_var asm(\"sp\");\n";
    let arena = Arena::default();
    let unit = parse(&arena, source, 0);
    let object = match unit.declarations() {
        [CHirDecl::Object(object)] => object,
        declarations => panic!("unexpected declarations: {}", declarations.len()),
    };
    let manifest = AllocationSiteManifest::derive(&unit)
        .unwrap_or_else(|error| panic!("global register manifest: {error:?}"));
    assert!(
        manifest.object_site(object.symbol).is_none(),
        "a global register variable must not acquire object provenance"
    );
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("declaration-only lowering: {} {error}", error.code()));
    assert!(module.globals.is_empty());
    assert!(module.functions.is_empty());
}

#[test]
fn global_register_reads_and_writes_lower_to_ordered_assembly() {
    let cases = [
        (
            "register unsigned long sp_var asm(\"sp\"); unsigned long read_sp(void) { return sp_var; }\n",
            1,
        ),
        (
            "register unsigned long sp_var asm(\"sp\"); void write_sp(unsigned long value) { sp_var = value; }\n",
            2,
        ),
    ];
    for (source, ordinal) in cases {
        let arena = Arena::default();
        let unit = parse(&arena, source, ordinal);
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("machine-register use lowers through the assembly family");
        assert!(module.globals.is_empty());
        let text = render_module(&module);
        assert!(
            text.contains(if ordinal == 1 {
                "mov %0, sp"
            } else {
                "mov sp, %0"
            }),
            "{text}"
        );
        assert!(text.contains("volatile"), "{text}");
        assert_eq!(
            evocc_evoir::render_module(
                &evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()
            ),
            text
        );
        let function = &module.functions[0];
        let args = function
            .params
            .iter()
            .map(|p| RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        assert_eq!(
            interpret_module(&module, &function.name, &args, EvoIrLimits::new(1000))
                .unwrap_err()
                .code(),
            "E3021"
        );
    }
}

#[test]
fn global_register_execution_is_an_assembly_boundary_only_when_reached() {
    for (source, expected) in [
        (
            "register unsigned long p asm(\"sp\"); int main(void){return p!=0;}",
            Some("E3021"),
        ),
        (
            "register unsigned long p asm(\"sp\"); int main(void){p=7; return 0;}",
            Some("E3021"),
        ),
        (
            "register unsigned long p asm(\"sp\"); int main(void){if(0)return p; return 7;}",
            None,
        ),
    ] {
        let arena = Arena::default();
        let unit = parse(&arena, source, 12);
        let symbol = unit
            .iter()
            .find_map(|d| match d {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        let result = interpret_scalar_function(&unit, symbol, ChirLimits::new(1000));
        if let Some(code) = expected {
            assert_eq!(result.unwrap_err().code(), code);
        } else {
            assert_eq!(result.unwrap().return_value.unwrap().raw_bits, 7);
        }
    }
}

#[test]
fn register_updates_and_switch_selection_do_not_duplicate_reads() {
    for (body, expected) in [
        (
            "p += 7; return p++;",
            vec!["mov %0, sp", "mov sp, %0", "mov %0, sp", "mov sp, %0"],
        ),
        (
            "switch(p){case 1: return 2; case 3: return 4; default: return 0;}",
            vec!["mov %0, sp"],
        ),
    ] {
        let source =
            format!("register unsigned long p asm(\"sp\"); unsigned long f(void){{{body}}}");
        let arena = Arena::default();
        let unit = parse(&arena, &source, 15);
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let templates = module.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.insts)
            .filter_map(|inst| match inst {
                evocc_evoir::Inst::Asm(assembly) => Some(assembly.template.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(templates, expected);
    }
}

#[test]
fn register_addresses_remain_a_front_end_error() {
    let source = concat!(
        "register unsigned long sp_var asm(\"sp\"); ",
        "unsigned long *address(void) { return &sp_var; }\n",
    );
    let arena = Arena::default();
    let file = FileId::new(10_203);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect::<Vec<_>>();
    assert!(tokens.iter().all(|token| token.kind != TokenKind::Error));
    let mut parser = Parser::new(tokens, file, source);
    let _ = parser.parse_translation_unit_in(&arena);
    let diagnostic = parser
        .diagnostic()
        .expect("taking a register address must be refused");
    assert_eq!(diagnostic.code, "E2928");
    assert_eq!(diagnostic.message, "address-of-register-object");
    let address = source.find('&').expect("fixture has address operator");
    assert_eq!(diagnostic.span.byte_start, address as u32);
}

#[test]
fn block_scope_register_binding_remains_an_automatic_value() {
    let source = concat!(
        "unsigned long local(unsigned long value) { ",
        "register unsigned long bound asm(\"x9\") = value; ",
        "bound += 1; return bound; }\n",
    );
    let arena = Arena::default();
    let unit = parse(&arena, source, 4);
    AllocationSiteManifest::derive(&unit)
        .unwrap_or_else(|error| panic!("local register manifest: {error:?}"));
    lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("local register lowering: {} {error}", error.code()));
}

#[test]
fn completed_array_redeclaration_uses_its_composite_identity() {
    let source = "extern int values[]; extern int values[4];\n";
    let arena = Arena::default();
    let unit = parse(&arena, source, 5);
    let manifest = AllocationSiteManifest::derive(&unit)
        .unwrap_or_else(|error| panic!("completed-array manifest: {error:?}"));
    assert!(manifest.sites().is_empty());
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("completed-array lowering: {} {error}", error.code()));
    assert!(module.globals.is_empty());
    assert!(module.functions.is_empty());
}

fn function_symbol(unit: &evocc_chir::CHirUnit<'_>, name: &str) -> SymbolId {
    unit.iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == name => Some(function.symbol),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing function {name}"))
}

fn entry_arguments(module: &evocc_evoir::Module) -> Vec<RuntimeValue> {
    module.functions[0]
        .params
        .iter()
        .map(|parameter| match &parameter.ty {
            EvoIRType::Mem(_) | EvoIRType::World => RuntimeValue::new(parameter.ty.clone(), 0),
            other => panic!("unexpected ordinary entry parameter {other:?}"),
        })
        .collect()
}

#[test]
fn pointer_integer_conversions_lower_to_explicit_forms() {
    let cases = [
        (
            "unsigned long bits(int *pointer) { return (unsigned long)pointer; }\n",
            "ptr.to_int",
            6,
        ),
        (
            "int *pointer(unsigned long bits) { return (int *)bits; }\n",
            "ptr.from_int.ptr<generic>",
            7,
        ),
    ];
    for (source, conversion, ordinal) in cases {
        let arena = Arena::default();
        let unit = parse(&arena, source, ordinal);
        let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        assert!(evocc_evoir::render_module(&module).contains(conversion));
    }

    let source = "int *null_pointer(void) { return (int *)0; }\n";
    let arena = Arena::default();
    let unit = parse(&arena, source, 8);
    lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap_or_else(|error| {
        panic!(
            "null pointer constant was rejected: {} {error}",
            error.code()
        )
    });
}

#[test]
fn pointer_poison_crosses_cfg_edges_until_definite_assignment() {
    let source = concat!(
        "int *select_pointer(int *a, int c) { int *t; ",
        "if (c) t = a; else t = 0; return t; }\n",
    );
    let arena = Arena::default();
    let unit = parse(&arena, source, 9);
    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("pointer poison did not lower: {} {error}", error.code()));
    let text = render_module(&module);
    assert!(
        text.contains("uninitialized.ptr<generic>"),
        "the missing incoming value must be explicit pointer poison:\n{text}"
    );
}

#[test]
fn dereferencing_pointer_poison_is_e3013_in_both_interpreters() {
    let source = "int main(void) { int *pointer; return *pointer; }\n";
    let arena = Arena::default();
    let unit = parse(&arena, source, 10);
    let chir_error = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        ChirLimits::new(100_000),
    )
    .expect_err("C-HIR dereferenced an absent-provenance pointer");
    assert_eq!(
        chir_error.code(),
        "E3013",
        "wrong C-HIR error: {chir_error}"
    );
    assert_eq!(chir_error.message(), "invalid-pointer-representation");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("pointer poison lowering failed: {} {error}", error.code()));
    let evoir_error = interpret_module(
        &module,
        "main",
        &entry_arguments(&module),
        EvoIrLimits::new(100_000),
    )
    .expect_err("EvoIR dereferenced an absent-provenance pointer");
    assert_eq!(
        evoir_error.code(),
        "E3013",
        "wrong EvoIR error: {evoir_error}"
    );
    assert_eq!(evoir_error.message(), "invalid-pointer-representation");
}

#[test]
fn calling_function_pointer_poison_is_e3013_in_both_interpreters() {
    let source = "int main(void) { int (*callee)(void); return callee(); }\n";
    let arena = Arena::default();
    let unit = parse(&arena, source, 11);
    let chir_error = interpret_scalar_function(
        &unit,
        function_symbol(&unit, "main"),
        ChirLimits::new(100_000),
    )
    .expect_err("C-HIR called an absent-provenance function pointer");
    assert_eq!(
        chir_error.code(),
        "E3013",
        "wrong C-HIR error: {chir_error}"
    );
    assert_eq!(chir_error.message(), "invalid-pointer-representation");

    let module = lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("function pointer poison failed: {} {error}", error.code()));
    let text = render_module(&module);
    assert!(
        text.contains("uninitialized.fnptr<"),
        "missing function pointer poison:\n{text}"
    );
    let evoir_error = interpret_module(
        &module,
        "main",
        &entry_arguments(&module),
        EvoIrLimits::new(100_000),
    )
    .expect_err("EvoIR called an absent-provenance function pointer");
    assert_eq!(
        evoir_error.code(),
        "E3013",
        "wrong EvoIR error: {evoir_error}"
    );
    assert_eq!(evoir_error.message(), "invalid-pointer-representation");
}
