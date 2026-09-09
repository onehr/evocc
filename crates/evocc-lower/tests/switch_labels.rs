use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn assert_execution(source: &str) -> evocc_evoir::Module {
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
    module
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
fn signed_enum_and_integer_pointer_arms_form_a_signed_place() {
    assert_execution(
        "enum e { a = (-2147483647 - 1) }; int *p; enum e *q;
         int main(void) { enum e x = a; q = &x;
           return *(1 ? q : p) > 0; }",
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

#[test]
fn switch_accepts_a_single_labeled_statement_without_braces() {
    assert_execution(
        "int f(int x) { switch (x) case 3: return 7; return 9; }
         int main(void) { return f(3) != 7 || f(4) != 9; }",
    );
}

#[test]
fn dispatch_enters_nested_cases_without_running_bypassed_guards() {
    assert_execution(
        "int f(int x) { int n = 0; switch (x) {
           case 0: if (++n == 2) {
             case 1: n += 3;
           }
           n += 5; break;
           { case 2 ... 3: n = 11; break; }
           default: n = 17;
         } return n; }
         int main(void) { return f(0) != 6 || f(1) != 8 ||
           f(2) != 11 || f(3) != 11 || f(4) != 17; }",
    );
}

#[test]
fn dispatch_into_a_loop_preserves_backedges_and_break_targets() {
    assert_execution(
        "void transfer(int *out, int *in, int count) {
           int n = (count + 3) / 4;
           switch (count % 4) {
             case 0: do { *out++ = *in++;
             case 3:      *out++ = *in++;
             case 2:      *out++ = *in++;
             case 1:      *out++ = *in++;
                     } while (--n > 0);
           }
         }
         int main(void) { int in[9] = {1,2,3,4,5,6,7,8,9}; int out[9];
           for (int n = 1; n <= 9; n++) {
             for (int i = 0; i < 9; i++) out[i] = 0;
             transfer(out, in, n);
             for (int i = 0; i < 9; i++)
               if (out[i] != (i < n ? in[i] : 0)) return 1;
           } return 0; }",
    );
}

#[test]
fn narrow_switch_selectors_keep_their_integer_promotion() {
    assert_execution(
        "int f(unsigned char x) { switch (x) {
           case 256: return 1;
           case 0: return 7;
           default: return 9;
         } }
         int main(void) { return f(0) != 7 || f(1) != 9; }",
    );
}

#[test]
fn nested_switches_own_their_cases_and_continue_targets() {
    assert_execution(
        "int main(void) { int total = 0, guards = 0;
           for (int i = 0; i < 3; i++) {
             switch (i) {
               if (++guards) {
                 case 0: total += 1; continue;
                 case 1: switch (i + 1) { case 2: total += 3; break; }
                         break;
                 default: total += 5;
               }
             }
             total += 10;
           }
           return total != 29 || guards != 0;
         }",
    );
}

#[test]
fn dispatch_preserves_the_switch_block_compound_literal_storage() {
    assert_execution(
        "int f(int x) { int value = 0; switch (x) {
           if (0) case 1: value = ((int[]){7})[0];
         } return value; }
         int main(void) { return f(1) != 7 || f(0) != 0; }",
    );
}

#[test]
fn case_constants_are_folded_without_runtime_arithmetic_obligations() {
    assert_execution(
        "int main(void) { int x = -2147483647 - 1; switch (x) {
           case 1 << 31: return 0;
           default: return 1;
         } }",
    );
}

#[test]
fn constant_condition_removes_only_the_ordinary_entry_not_targeted_labels() {
    let source = "extern void link_error(void); int ok;
        void f(int x) { switch (x) { case 0: if (0) {
          link_error(); case 1: ok = 1;
        } } }
        int main(void) { f(1); return ok != 1; }";
    assert_execution(source);
    let arena = Arena::default();
    let file = FileId::new(45031);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .any(|i| matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "link_error")),
        "unreachable call prevents the native program from linking"
    );
}

#[test]
fn constant_switch_dispatch_does_not_emit_unselected_external_calls() {
    for source in [
        include_str!("../../../tests/fixtures/constant-switch-linkage.c"),
        "extern void __bad_size_call_parameter(void); int main(void){int n=0; switch(3){case 1 ... 3:n=7; case 4:n+=2;break;default:__bad_size_call_parameter();}return n!=9;}",
        "extern void __bad_size_call_parameter(void); int main(void){int n=0; switch(sizeof(int)){if(0){case 4:n=7;break;}default:__bad_size_call_parameter();}return n!=7;}",
        "extern void __bad_size_call_parameter(void); int main(void){switch((unsigned char)255){case -1:__bad_size_call_parameter();break;case 255:return 0;default:return 1;}return 2;}",
        "extern void __bad_size_call_parameter(void); int main(void){switch(9){case 1:__bad_size_call_parameter();break;default:return 0;}return 2;}",
    ] {
        let module = assert_execution(source);
        assert!(
            !module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "__bad_size_call_parameter")),
            "constant dispatch leaves an unresolved external call: {source}"
        );
    }
}

#[test]
fn constant_switch_preserves_label_entry_and_selector_effects() {
    assert_execution("int main(void){int n=0;goto enter;switch(4){case 3:enter:n+=7;break;case 4:n=2;break;}return n!=7;}");
    assert_execution(
        "int main(void){int n=0;switch((n++,4)){case 4:n+=7;break;default:n=99;}return n!=8;}",
    );
    assert_execution("int main(void){int n=0;switch(4){case 4:switch(7){case 7:n=5;break;default:n=99;}n+=2;break;default:n=88;}return n!=7;}");
}

#[test]
fn constant_expression_edges_remove_only_unselected_external_calls() {
    for expression in [
        "0 && link_error()",
        "(0 && n) && link_error()",
        "(1 || n) || link_error()",
        "1 ? 7 : link_error()",
        "0 ? link_error() : 7",
        "7 ?: link_error()",
        "sizeof(n) == 4 ? 7 : link_error()",
    ] {
        let source = format!("extern int link_error(void); int main(void){{int n=3; (void)({expression}); return 0;}}");
        let module = assert_execution(&source);
        assert!(!module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
            .any(|i| matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "link_error")),
            "unselected expression still links an external call: {expression}");
    }
}

#[test]
fn constant_expression_edges_keep_selected_values_and_effects() {
    for expression in [
        "1 && ++n",
        "0 || ++n",
        "1 ? ++n : 9",
        "0 ? 9 : ++n",
        "0 ?: ++n",
    ] {
        assert_execution(&format!(
            "int main(void){{int n=0;int value=({expression});return value!=1 || n!=1;}}"
        ));
    }
    assert_execution("int main(void){int n=0;int v=(n++ ? 9 : ++n);return v!=2 || n!=2;}");
    assert_execution("int main(void){int n=0;int v=(++n ?: ++n);return v!=1 || n!=1;}");
}

#[test]
fn constant_branch_selection_uses_the_conditional_common_type() {
    assert_execution(include_str!(
        "../../../tests/fixtures/conditional-common-type.c"
    ));
    for condition in [
        "(1 ? -1 : 0U) < 0",
        "(0 ? 0U : -1) < 0",
        "(1 ? -1L : 0UL) < 0",
        "(0 ? 0ULL : -1LL) < 0",
        "(1 ? -1 : (0 ? 1 : 0U)) < 0",
        "(1 ? -1 : (unsigned __int128)0) < 0",
        "(-1 ?: 0U) < 0",
    ] {
        for statement in [
            format!("if ({condition}) return 1;"),
            format!("switch ({condition}) {{case 0:break;default:return 1;}}"),
            format!("int n=({condition}) ? 1 : 0; if(n) return 1;"),
            format!("int n=({condition}) && 1; if(n) return 1;"),
            format!("int n=({condition}) || 0; if(n) return 1;"),
            format!("int n=({condition}) ?: 0; if(n) return 1;"),
        ] {
            assert_execution(&format!("int main(void){{ {statement} return 0;}}"));
        }
    }
    assert_execution(
        "extern unsigned int absent(void);int main(void){if((1?-1:absent())<0)return 1;return 0;}",
    );
}

#[test]
fn static_address_selection_uses_the_conditional_common_type() {
    assert_execution("int a=3,b=7;static int *p=((1?-1:0U)<0)?&a:&b;int main(void){return *p!=7;}");
}
