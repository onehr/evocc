use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn assert_execution(source: &str) {
    let arena = Arena::default();
    let file = FileId::new(45030);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let main = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let expected =
        interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
            .unwrap();
    assert_eq!(expected.return_value.unwrap().raw_bits, 0);
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments = main
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let result = evocc_evoir::interpret_module(
        &module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(10000),
    )
    .unwrap();
    assert_eq!(result.return_values[0].raw_bits, 0);
}

#[test]
fn goto_can_reach_a_label_after_the_segment_loses_fallthrough() {
    assert_execution(
        "int f(int x) { int n = 0; switch (x) {
          case 0: goto again;
          case 1: n = 2; goto done;
          again: n++; if (n < 3) goto again;
          done: return n;
          default: return 7;
        } return 9; }
        int main(void) { return f(0) != 3 || f(1) != 2 || f(2) != 7; }",
    );
}

#[test]
fn goto_reaches_a_nested_label_without_evaluating_its_guard() {
    assert_execution(
        "int f(int x) { int n = 0; switch (x) {
          case 0: goto target;
          case 1: goto target;
          if (++n == 99) { target: n += 4; }
          n++; break;
          default: n = 8;
        } return n; }
        int main(void) { return f(0) != 5 || f(1) != 5 || f(2) != 8; }",
    );
}

#[test]
fn shared_label_enters_a_nested_switch_with_effect_tokens() {
    assert_execution(
        "int total;
        void pack(int type, unsigned char *p) {
          int width;
          switch (type) {
            case 0: width = 1; goto unpack;
            case 1: width = 2; goto unpack;
            unpack: switch (width) {
              case 1: total += p[0]; break;
              case 2: total += p[0] + p[1]; break;
            }
            break;
          }
        }
        int main(void) { unsigned char p[2] = {3, 5};
          pack(0, p); pack(1, p); return total != 11; }",
    );
}
