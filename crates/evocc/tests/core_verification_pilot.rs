//! Executable population freeze, not a formal compilation proof.
use evocc_chir::{core_profile::inspect_core, CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_cpp::{Preprocessor, TokenStream};
use evocc_lex::{Lexer, TokenKind};
use evocc_mir::{MInst, Target};
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(10_601);
    let mut lexer = Lexer::new(file, source);
    let mut tokens = Vec::new();
    while let Some(token) = lexer.next() {
        assert_ne!(token.kind, TokenKind::Error);
        tokens.push(token);
    }
    let mut cpp = Preprocessor::new();
    let tokens = cpp.process(TokenStream::new(tokens), source).into_vec();
    assert!(cpp.errors().is_empty(), "{:?}", cpp.errors());
    let mut parser = Parser::new(tokens, file, source);
    // Exercise the original abs definition, not ordinary-name builtin rewriting.
    parser.set_standard_builtins(false);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(parser.diagnostic().is_none(), "{:?}", parser.diagnostic());
    unit
}

fn run_population(source: &str, diagnostic: Option<&str>) {
    let arena = Arena::default();
    let unit = parse(source, &arena);
    let target = TargetDataLayout::aarch64_unknown_linux_gnu();
    let inventory = inspect_core(&unit, &target, &["main"]).expect("frozen source profile");
    let main = unit
        .declarations()
        .iter()
        .find_map(|d| match d {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    let source_result = evocc_chir::interp::interpret_scalar_function(
        &unit,
        main,
        evocc_chir::interp::InterpreterLimits::new(100_000),
    );
    let ir = evocc_lower::lower_unit(&unit, &target).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let text = evocc_evoir::render_module(&ir);
    let reconstructed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&reconstructed), text);
    let entry = ir.functions.iter().find(|f| f.name == "main").unwrap();
    let args = entry
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let ir_result = evocc_evoir::interpret_module(
        &ir,
        "main",
        &args,
        evocc_evoir::InterpreterLimits::new(100_000),
    );
    let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| true).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    let mir_result =
        evocc_mir::interpret_module(&mir, "main", &[], evocc_mir::MirLimits::new(100_000));
    match diagnostic {
        Some(code) => {
            assert_eq!(source_result.unwrap_err().code(), code);
            assert_eq!(ir_result.unwrap_err().code(), code);
            assert_eq!(mir_result.unwrap_err().code(), code);
        }
        None => {
            assert_eq!(source_result.unwrap().return_value.unwrap().raw_bits, 0);
            assert_eq!(ir_result.unwrap().return_values[0].raw_bits, 0);
            assert_eq!(mir_result.unwrap().results, [evocc_mir::MirValue::Int(0)]);
        }
    }
    assert_eq!(inventory.functions.len(), mir.functions.len());
    let mut spills = 0;
    let mut stores = 0;
    let mut reloads = 0;
    let mut incoming_stack = 0;
    let mut blocks = 0;
    for function in &mir.functions {
        let lowered = evocc_mir::out_of_ssa(function);
        let allocated = evocc_mir::allocate_for(Target::Linux, &lowered).unwrap();
        evocc_mir::verify_allocation_for(Target::Linux, &allocated).unwrap();
        blocks += function.blocks.len();
        spills += allocated.spilled;
        for instruction in allocated.blocks.iter().flat_map(|b| &b.insts) {
            stores += usize::from(matches!(instruction, MInst::SpillStore { .. }));
            reloads += usize::from(matches!(instruction, MInst::SpillReload { .. }));
            incoming_stack += usize::from(matches!(instruction, MInst::IncomingArg { .. }));
        }
    }
    if source.contains("unsigned long pressure(") {
        assert!(
            spills > 0 && stores > 0 && reloads > 0,
            "pilot must exercise actual spills, not just many source locals"
        );
        assert!(
            incoming_stack >= 24,
            "32-argument call must exercise stack argument entry"
        );
    }
    let asm = evocc_mir::compile_module_for(Target::Linux, &ir, &|_| true).unwrap();
    let repeated_ir = evocc_lower::lower_unit(&unit, &target).unwrap();
    let repeated_asm =
        evocc_mir::compile_module_for(Target::Linux, &repeated_ir, &|_| true).unwrap();
    assert_eq!(
        asm, repeated_asm,
        "same-process pipeline determinism, not an object reproducibility claim"
    );
    println!("core-v1: functions={} blocks={blocks} spills={spills} stores={stores} reloads={reloads} stack_args={incoming_stack} asm_bytes={}", inventory.functions.len(), asm.len());
}

#[test]
fn original_functions_traverse_the_existing_pipeline() {
    run_population(&format!("{}\nint main(void) {{ return FixedMul(98304, 131072) != 196608 || FixedMul(-98304, 131072) != -196608; }}",
        include_str!("../../../tests/fixtures/verification/doom-fixedmul.c")), None);
    run_population(
        &format!(
            "{}\nint main(void) {{ return abs(-9) != 9 || abs(0) != 0 || abs(7) != 7; }}",
            include_str!("../../../tests/fixtures/verification/musl-abs.c")
        ),
        None,
    );
}

#[test]
fn joins_backedges_and_short_circuit_side_effects() {
    let mut total = 0_u32;
    let mut x = 1_u32;
    for i in 0..17_u32 {
        if i % 4 == 0 {
            continue;
        }
        total = total.wrapping_add(if x & 1 != 0 { i } else { i ^ x });
        x = x.rotate_right(1);
    }
    let mut n = 64_u32;
    while n > 20 {
        n -= 7;
    }
    while n != 0 {
        total = total.wrapping_add(n);
        n >>= 1;
    }
    run_population(&format!("{}\nint main(void) {{ return loop_join(0, 1) != 1U || loop_join(1, 4) != 0xc000000dU || loop_join(1, 64) != {total}U || short_circuit(0) != 2 || short_circuit(1) != 11; }}",
        include_str!("../../../tests/fixtures/verification/control.c")), None);
}

#[test]
fn real_spills_and_direct_call_stack_arguments() {
    run_population(
        &format!(
            "{}\nint main(void) {{ return pressure(17UL) != 1040UL; }}",
            include_str!("../../../tests/fixtures/verification/pressure.c")
        ),
        None,
    );
}

#[test]
fn checked_source_diagnostic_survives_every_existing_interpreter() {
    run_population(
        &format!(
            "{}\nint main(void) {{ return abs(-2147483647 - 1); }}",
            include_str!("../../../tests/fixtures/verification/musl-abs.c")
        ),
        Some("E3004"),
    );
}
