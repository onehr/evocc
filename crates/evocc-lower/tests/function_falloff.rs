use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn check(source: &str, expected: Result<u128, &str>, chir: bool) {
    let arena = Arena::default();
    let file = FileId::new(45011);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let actual = if chir {
        let symbol = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000))
            .map(|out| out.return_value.unwrap().raw_bits)
            .map_err(|err| err.code().to_owned())
    } else {
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        let text = evocc_evoir::render_module(&module);
        assert!(
            !text.contains("trap.scalar_function_fell_off_end"),
            "{text}"
        );
        assert_eq!(
            evocc_evoir::render_module(
                &evocc_evoir::parse_section9_module(text.as_bytes()).unwrap()
            ),
            text
        );
        evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .map(|out| u128::from(out.return_values[0].raw_bits))
        .map_err(|err| err.code().to_owned())
    };
    assert_eq!(actual, expected.map_err(str::to_owned), "{source}");
}

fn discarded(chir: bool) {
    for source in [
        "int f(void){} int main(void){f(); return 0;}",
        "int f(void){} int g(void){return f();} int main(void){g(); return 0;}",
        "struct s {int x;}; struct s f(void){} int main(void){f(); return 0;}",
        "int count; int f(void){count++;} int main(void){(void)f(); f(); return count-2;}",
        "int *f(void){} int main(void){f(); return 0;}",
        "__int128 f(void){} int main(void){f(); return 0;}",
        "int f(int n){if(n)return 7;} int main(void){f(0); return f(1)-7;}",
        "int main(void){}",
    ] {
        check(source, Ok(0), chir);
    }
}

fn consumed(chir: bool) {
    for source in [
        "int f(void){} int main(void){return f()+1;}",
        "int f(void){} int main(void){if(f())return 1; return 0;}",
        "int f(void){} int main(void){return f();}",
    ] {
        check(source, Err("E3001"), chir);
    }
}

#[test]
fn chir_discards_a_missing_result_without_discarding_effects() {
    discarded(true);
}

#[test]
fn evoir_discards_a_missing_result_without_discarding_effects() {
    discarded(false);
}

#[test]
fn chir_reports_poison_when_the_missing_result_is_used() {
    consumed(true);
}

#[test]
fn evoir_reports_poison_when_the_missing_result_is_used() {
    consumed(false);
}
