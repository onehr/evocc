use evocc_chir::interp::{interpret_scalar_function, InterpreterLimits};
use evocc_chir::{CHirDecl, CHirUnit};
use evocc_common::{Arena, FileId};
use evocc_evoir::{interpret_module, RuntimeValue};
use evocc_lex::{Lexer, TokenKind};
use evocc_parser::Parser;
use evocc_types::{EvoIRType, Region, TargetDataLayout};

fn parse<'a>(source: &str, arena: &'a Arena) -> CHirUnit<'a> {
    let file = FileId::new(45_004);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect::<Vec<_>>();
    assert!(tokens.iter().all(|token| token.kind != TokenKind::Error));
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(arena);
    assert!(!parser.had_error(), "{:?}: {source}", parser.diagnostic());
    unit
}

const COPIED: &str = "int forwarded(__builtin_va_list ap){return __builtin_va_arg(ap,int);} int take(int n,...){__builtin_va_list ap,bp; __builtin_va_start(ap,n); int a=__builtin_va_arg(ap,int); __builtin_va_copy(bp,ap); int b=forwarded(bp); int c=__builtin_va_arg(ap,int); __builtin_va_end(bp); __builtin_va_end(ap); return a+10*b+100*c;} int main(void){signed char a=7; short b=9; return take(2,a,b);}";

#[test]
fn projected_lists_preserve_value_copies_and_once_only_address_evaluation() {
    let arena = Arena::default();
    let unit = parse(
        include_str!("../../../tests/fixtures/projected-variadic-lists.c"),
        &arena,
    );
    let main = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let chir = interpret_scalar_function(&unit, main, InterpreterLimits::new(100_000));
    assert_eq!(chir.unwrap().return_value.unwrap().raw_bits, 0);
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("list subobjects use the same value-copy path as named lists");
    let text = evocc_evoir::render_module(&module);
    assert!(text.contains("copy "), "{text}");
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
    let result = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        evocc_evoir::InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(result.return_values[0].raw_bits, 0);
}

#[test]
fn integer_aggregate_arguments_follow_the_variadic_sequence() {
    let arena = Arena::default();
    let unit = parse(
        include_str!("../../../tests/fixtures/aggregate-variadic-values.c"),
        &arena,
    );
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    let result = interpret_scalar_function(&unit, symbol, InterpreterLimits::new(100_000));
    assert_eq!(result.unwrap().return_value.unwrap().raw_bits, 0);
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let text = evocc_evoir::render_module(&module);
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
    let optimized = evocc_eqopt::optimize_scalar_module_with_evp1(
        &parsed,
        evocc_eqopt::ScalarEqBudget::default(),
    );
    evocc_evoir::validate_module(&optimized.module).unwrap();
    let result = interpret_module(
        &optimized.module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        evocc_evoir::InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(result.return_values[0].raw_bits, 0);
}

#[test]
fn aggregate_lists_copy_values_and_reject_a_different_shape() {
    for (source, expected) in [
        ("struct S{int x;}; struct S g={3}; int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); g.x=10; struct S s=__builtin_va_arg(ap,struct S); return s.x;} int main(void){return f(0,g);}", Ok(3)),
        ("struct S{int *p;}; int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); struct S s=__builtin_va_arg(ap,struct S); return *s.p;} int main(void){int x=17; struct S s={&x};return f(0,s);}", Ok(17)),
        ("struct S{int x;}; struct T{long x,y;}; int f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);struct T t=__builtin_va_arg(ap,struct T);return t.x;} int main(void){struct S s={3};return f(0,s);}", Err("E3038")),
        ("struct S{int x;}; int f(int n,...){__builtin_va_list ap;__builtin_va_start(ap,n);struct S s=__builtin_va_arg(ap,struct S);return s.x;} int main(void){return f(0,3);}", Err("E3038")),
    ] {
        let arena = Arena::default();
        let unit = parse(source, &arena);
        let symbol = unit.iter().find_map(|decl| match decl { CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol), _ => None }).unwrap();
        let chir = interpret_scalar_function(&unit, symbol, InterpreterLimits::new(100_000));
        match expected {
            Ok(value) => assert_eq!(chir.unwrap().return_value.unwrap().raw_bits, value),
            Err(code) => assert_eq!(chir.unwrap_err().code(), code),
        }
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let result = interpret_module(&module, "main", &[
            RuntimeValue::new(EvoIRType::Mem(Region::All),0), RuntimeValue::new(EvoIRType::World,0),
        ], evocc_evoir::InterpreterLimits::new(100_000));
        match expected {
            Ok(value) => assert_eq!(u128::from(result.unwrap().return_values[0].raw_bits), value),
            Err(code) => assert_eq!(result.unwrap_err().code(), code),
        }
    }
}

#[test]
fn anonymous_record_marks_and_destinations_are_validated() {
    use evocc_evoir::{Inst, ParamMark, VariadicOp};
    let arena = Arena::default();
    let unit = parse(
        include_str!("../../../tests/fixtures/aggregate-variadic-values.c"),
        &arena,
    );
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    for mutation in 0..4 {
        let mut broken = module.clone();
        if mutation < 3 {
            let marks = broken
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.insts)
                .find_map(|inst| match inst {
                    Inst::Call { arg_marks, .. } if !arg_marks.is_empty() => Some(arg_marks),
                    _ => None,
                })
                .unwrap();
            let index = marks
                .iter()
                .position(|mark| matches!(mark, ParamMark::ByVal(_)))
                .unwrap();
            match mutation {
                0 => {
                    marks.pop();
                }
                1 => {
                    marks[index] = ParamMark::Sret(marks[index].shape().unwrap());
                }
                2 => {
                    marks.swap(0, index);
                }
                _ => unreachable!(),
            }
        } else {
            let destination = broken
                .functions
                .iter_mut()
                .flat_map(|f| &mut f.blocks)
                .flat_map(|b| &mut b.insts)
                .find_map(|inst| match inst {
                    Inst::VaAccess(access) => match &mut access.op {
                        VariadicOp::ArgRecord { destination, .. } => Some(destination),
                        _ => None,
                    },
                    _ => None,
                })
                .unwrap();
            destination.ty = EvoIRType::Bits(64);
        }
        assert!(
            evocc_evoir::validate_module(&broken).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn variadic_lowering_promotes_arguments_and_copies_list_state() {
    let arena = Arena::default();
    let unit = parse(COPIED, &arena);
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .expect("scalar variadic operations lower");
    let text = evocc_evoir::render_module(&module);
    for form in ["...", "va_start ", "va_arg.bits<32> ", "va_end ", "copy "] {
        assert!(text.contains(form), "missing {form}: {text}");
    }
    let parsed = evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    assert_eq!(evocc_evoir::render_module(&parsed), text);
    let result = interpret_module(
        &module,
        "main",
        &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ],
        evocc_evoir::InterpreterLimits::new(100_000),
    )
    .unwrap();
    assert_eq!(result.return_values[0].raw_bits, 997);
}

#[test]
fn chir_executes_variadic_lists_and_reports_mismatched_reads() {
    for (source, expected) in [
        (COPIED.to_owned(), Ok(997)),
        ("int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); return __builtin_va_arg(ap,int);} int main(void){return f(0);}".to_owned(), Err("E3038")),
        ("int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); return __builtin_va_arg(ap,int);} int main(void){return f(1,7L);}".to_owned(), Err("E3038")),
        ("int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); __builtin_va_end(ap); return __builtin_va_arg(ap,int);} int main(void){return f(1,7);}".to_owned(), Err("E3038")),
        ("int main(void){__builtin_va_list ap; return __builtin_va_arg(ap,int);}".to_owned(), Err("E3038")),
        ("int f(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); return *__builtin_va_arg(ap,int*);} int main(void){int x=19; int (*p)(int,...)=f; return p(1,&x);}".to_owned(), Ok(19)),
    ] {
        let arena = Arena::default();
        let unit = parse(&source, &arena);
        let symbol = unit.iter().find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        }).unwrap();
        let result = interpret_scalar_function(&unit, symbol, InterpreterLimits::new(100_000));
        match expected {
            Ok(value) => assert_eq!(result.unwrap().return_value.unwrap().raw_bits, value),
            Err(code) => assert_eq!(result.unwrap_err().code(), code),
        }
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let result = interpret_module(&module, "main", &[
            RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
            RuntimeValue::new(EvoIRType::World, 0),
        ], evocc_evoir::InterpreterLimits::new(100_000));
        match expected {
            Ok(value) => assert_eq!(u128::from(result.unwrap().return_values[0].raw_bits), value),
            Err(code) => assert_eq!(result.unwrap_err().code(), code),
        }
    }
}

#[test]
fn unpromoted_va_arg_types_are_front_end_errors() {
    for ty in ["char", "short", "_Bool", "float"] {
        let source = format!("int f(int n,...){{__builtin_va_list ap; __builtin_va_start(ap,n); return __builtin_va_arg(ap,{ty});}}");
        let arena = Arena::default();
        let file = FileId::new(45_004);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect::<Vec<_>>();
        assert!(tokens.iter().all(|token| token.kind != TokenKind::Error));
        let mut parser = Parser::new(tokens, file, &source);
        parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "unpromoted {ty} was accepted");
        assert_eq!(parser.diagnostic().unwrap().code, "E2929");
    }
}
