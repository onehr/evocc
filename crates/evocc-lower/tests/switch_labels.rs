use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn assert_execution(source: &str) -> evocc_evoir::Module {
    assert_execution_with_optimization(source, false)
}

fn assert_execution_with_optimization(source: &str, optimize: bool) -> evocc_evoir::Module {
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
    let module = evocc_lower::lower_unit_with_optimization(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        optimize,
    )
    .unwrap();
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
fn inline_returns_do_not_collide_with_source_end_labels() {
    for optimize in [false, true] {
        assert_execution_with_optimization(
            include_str!("../../../tests/fixtures/inline-source-end-label.c"),
            optimize,
        );
    }
}

#[test]
fn default_entry_retains_its_environment_regardless_of_source_order() {
    for body in [
        "default: value += *ptr; break; case 1: return 1; case 2: return 2;",
        "case 1: return 1; default: value += *ptr; break; case 2: return 2;",
        "case 1: return 1; case 2: return 2; default: value += *ptr; break;",
        "default: value += *ptr; case 1: value += 3; break; case 2: return 2;",
    ] {
        let expected = if body.contains("value += 3") { 15 } else { 12 };
        let source = format!(
            "int pick(int op,int value,int *ptr){{switch(op){{{body}}}return value;}}
             int main(void){{int data=7;return pick(3,5,&data)!={expected} || pick(2,5,&data)!=2;}}"
        );
        for optimize in [false, true] {
            assert_execution_with_optimization(&source, optimize);
        }
    }
    assert_execution(include_str!(
        "../../../tests/fixtures/switch-default-pointer-environment.c"
    ));
}

#[test]
fn expression_gotos_keep_otherwise_unselected_label_destinations() {
    assert_execution(include_str!(
        "../../../tests/fixtures/expression-goto-targets.c"
    ));
}

#[test]
fn constant_inline_configuration_calls_remove_unreachable_symbol_uses() {
    for body in [
        "if (!enabled()) return 0; return absent;",
        "if (enabled()) return absent; return 0;",
        "while (enabled()) return absent; return 0;",
    ] {
        let source = format!(
            "extern int absent; static inline _Bool enabled(void) {{ return 0; }}
             int main(void) {{ {body} }}"
        );
        let module = assert_execution(&source);
        assert!(
            !module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
                .any(|inst| matches!(inst, evocc_evoir::Inst::GlobalAddr { symbol, .. } if symbol == "absent")),
            "unreachable external object still has an emitted reference: {body}"
        );
    }
    // Calls with effects are not constant just because their return is zero.
    assert_execution(
        "int n; static inline int enabled(void){ ++n; return 0; }
        int main(void){ if(enabled()) return 1; return n != 1; }",
    );
    // Unused arguments still evaluate, even when the callee returns a literal.
    assert_execution(
        "int n; static inline int enabled(int ignored){return 0;}
        int main(void){ if(enabled(++n)) return 1; return n != 1; }",
    );
    assert_execution(
        "static inline _Bool enabled(void){return 256;}
        int main(void){return enabled() != 1;}",
    );
    assert_execution(
        "static inline unsigned char enabled(void){return 256;}
        int main(void){return enabled() != 0;}",
    );
    assert_execution(
        "static inline int enabled(void){return 0;}
        int main(void){return __builtin_constant_p(enabled());}",
    );
}

#[test]
fn constant_configuration_results_retain_calls_and_argument_evaluation() {
    for (definition, guard) in [
        (
            "static inline _Bool enabled(int n){return 0 && n;}",
            "enabled(++n) && n > 0",
        ),
        (
            "static inline _Bool enabled(int n){return 256;}",
            "!enabled(++n)",
        ),
        (
            "static inline unsigned char enabled(int n){return 256;}",
            "enabled(++n)",
        ),
    ] {
        let guard = guard.replace("++n", "tick()");
        let source = format!(
            "extern int absent(int); int counter; int tick(void){{return ++counter;}} {definition}
             int main(void){{int n=0;if({guard})return absent(n);return counter!=1;}}"
        );
        let module = assert_execution_with_optimization(&source, true);
        let calls: Vec<_> = module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts)
            .filter_map(|inst| match inst {
                evocc_evoir::Inst::Call { callee, .. } => Some(callee.as_str()),
                _ => None,
            })
            .collect();
        assert!(
            calls.contains(&"tick"),
            "inlining must keep the argument's observable call"
        );
        assert!(
            !calls.contains(&"absent"),
            "unreachable external call remains: {source}"
        );
    }
    for (definition, expected_counter) in [
        ("int enabled(int n){return 0;}", 0),
        ("static inline int enabled(int n){return n-1;}", 0),
        (
            "static inline int enabled(int n){++counter;return n-counter;}",
            1,
        ),
    ] {
        let source = format!(
            "extern int absent(int); int counter; {definition}
             int check(int start){{int n=start;if(enabled(++n))return absent(n);
             return n!=1 || counter!={expected_counter};}} int main(void){{return check(0);}}"
        );
        let module = assert_execution(&source);
        assert!(module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
            .any(|inst| matches!(inst, evocc_evoir::Inst::Call {callee, ..} if callee == "absent")),
            "an unproved or interposable return was treated as constant: {source}");
    }
    assert_execution(
        "static inline int enabled(int n){return 0;}
         int main(void){int n=1;return __builtin_constant_p(enabled(n));}",
    );
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
fn masked_configuration_edges_do_not_keep_undefined_calls() {
    for condition in [
        "(n & 0U) != 0",
        "(0U & n) != 0",
        "(n * 0U) != 0",
        "(n | ~0U) == 0",
        "(n ^ n) != 0",
        "(n - n) != 0",
    ] {
        let source = format!(
            "extern int absent(void); int check(unsigned n){{if({condition})return absent();return 0;}}
             int main(void){{return check(7);}}"
        );
        let module = assert_execution_with_optimization(&source, true);
        assert!(
            !module
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "absent")),
            "a proved configuration mask retained its unselected call: {condition}"
        );
    }
    let module = assert_execution_with_optimization(
        "extern int absent(void); unsigned field=0xff00U;
         static __attribute__((always_inline)) inline unsigned mask(unsigned field){return field / (field & -field);}
         int main(void){if(7U & ~mask(0xff00U))return absent();return 0;}",
        true,
    );
    assert!(!module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "absent")));
}

#[test]
fn guarded_private_state_keeps_the_impossible_case_unlinked() {
    let module = assert_execution_with_optimization(
        include_str!("../../../tests/fixtures/guarded-private-state.c"),
        true,
    );
    assert!(!module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(
            |i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_state")
        ));
}

#[test]
fn private_state_guards_survive_nested_loop_exits() {
    for body in [
        "while(k){if(k==1)break;--k;}",
        "do{--k;break;}while(k);",
        "while(k){--k;if(k)continue;}",
    ] {
        let source = format!(
            "extern void impossible_state(void);struct S{{int state;}};
             int run(int n,int k){{struct S s={{n}};if(s.state){{{body}if(!s.state)impossible_state();}}return 0;}}
             int main(void){{return run(1,2);}}"
        );
        let module = assert_execution_with_optimization(&source, true);
        assert!(!module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
            .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_state")),
            "unreachable loop-exit scaffolding obscures the dominating guard: {body}");
    }
}

#[test]
fn private_state_guards_are_invalidated_by_writes_escapes_and_volatile_reads() {
    for (qualifier, action, expected) in [
        ("", "s.state=0;", 1),
        ("", "int *p=&s.state; *p=0;", 1),
        ("", "reset(&s);", 1),
        ("", "int *p=n?&s.state:&s.other; *p=0;", 1),
        ("volatile", "", 0),
    ] {
        let source = format!(
            "struct S{{int state;int other;}};int hits;
             void reset(struct S *p){{p->state=0;}}
             void possible(void){{++hits;}}
             int run(int n){{{qualifier} struct S s={{n,7}};if(s.state){{{action}if(!s.state)possible();}}return hits;}}
             int main(void){{return run(1)!={expected};}}"
        );
        let module = assert_execution_with_optimization(&source, true);
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "possible")),
            "a write or volatile reread was incorrectly assumed to preserve the guard: {source}"
        );
    }
    for body in [
        "struct S a={n,7},b={0,9};if(a.state){if(!b.state)possible();}",
        "int a[2]={n,7};if(a[0]){a[i]=0;if(!a[0])possible();}",
        "struct S s={n,7};if(s.state){while(i--){s.state=0;}if(!s.state)possible();}",
    ] {
        let source = format!(
            "struct S{{int state;int other;}};int hits;void possible(void){{++hits;}}
             int run(int n,int i){{{body}return hits;}}int main(void){{return run(1,0)!={};}}",
            i32::from(!body.contains("while")),
        );
        let module = assert_execution_with_optimization(&source, true);
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "possible")),
            "a different field or a possibly modified array was incorrectly equated: {source}"
        );
    }
}

#[test]
fn private_constant_stores_survive_calls_without_exposing_the_object() {
    let source =
        "extern void impossible_private(void); int effects; void called(void){++effects;}\n\
        int main(void){int a[2];a[0]=0;called();if(a[0])impossible_private();\n\
        a[1]=0;called();if(a[1])impossible_private();return effects!=2;}";
    let module = assert_execution_with_optimization(source, true);
    assert!(!module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(
            |i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_private")
        ));
    for (qualifier, action, expected) in [
        ("", "a[0]=1;", 1),
        ("", "change(a);", 1),
        ("volatile", "called();", 0),
        ("", "a[index]=1;", 1),
    ] {
        let source = format!("int effects,hits;void called(void){{++effects;}}void change(int*p){{*p=1;}}void possible(void){{++hits;}}int f(int index){{{qualifier} int a[2];a[0]=0;{action}if(a[0])possible();return hits;}}int main(void){{return f(0)!={expected};}}");
        let module = assert_execution_with_optimization(&source, true);
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "possible")),
            "{source}"
        );
    }
}

#[test]
fn mask_facts_keep_operand_effects_and_poison_consumption() {
    assert_execution_with_optimization(
        "int n; int tick(void){return ++n;} int main(void){if(tick() & 0)return 1;return n!=1;}",
        true,
    );
    for operand in ["n", "(1 / n)"] {
        let source = format!("int main(void){{int n; if(({operand}) & 0)return 1;return 0;}}");
        let arena = Arena::default();
        let file = FileId::new(45031);
        let mut lexer = Lexer::new(file, &source);
        let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error());
        let module = evocc_lower::lower_unit_with_optimization(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            true,
        )
        .unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args: Vec<_> = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect();
        let error = evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .unwrap_err();
        assert_eq!(error.code(), "E3001", "{source}: {error}");
    }
}

#[test]
fn shift_identities_prune_only_the_impossible_edges() {
    let module = assert_execution_with_optimization(
        include_str!("../../../tests/fixtures/shift-identity-edges.c"),
        true,
    );
    assert!(!module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(
            |i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_shift")
        ));
    for (expression, expected) in [
        ("0 << n", "E3001"),
        ("0 << 32", "E3005"),
        ("-1 >> -1", "E3005"),
        ("n >> 0", "E3001"),
        ("(n<n)&&(n>=n)", "E3001"),
        ("n<=0||(unsigned)(n-1)<4294967295U", "E3001"),
    ] {
        let source = format!("int main(void){{int n;return {expression};}}");
        let arena = Arena::default();
        let file = FileId::new(45032);
        let mut lexer = Lexer::new(file, &source);
        let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, &source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error());
        let module = evocc_lower::lower_unit_with_optimization(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            true,
        )
        .unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        let error = evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .unwrap_err();
        assert_eq!(error.code(), expected, "{source}: {error}");
    }
}

#[test]
fn comparison_partitions_keep_the_original_evaluation_contract() {
    let module = assert_execution_with_optimization(
        include_str!("../../../tests/fixtures/comparison-partitions.c"),
        true,
    );
    assert!(!module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
        .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_comparison")));
    for source in [
        "int hits;void possible(void){++hits;}int f(int x,int y){if((x<y)&&((x=3)>y))possible();return hits;}int main(void){return f(1,2)!=1;}",
        "int hits;void possible(void){++hits;}int f(int x,int y){if((x<y)&&((unsigned)x<(unsigned)y))possible();return hits;}int main(void){return f(1,2)!=1;}",
        "int hits;void possible(void){++hits;}int f(volatile int x,int y){if((x==y)&&(x!=y))possible();return hits;}int main(void){return f(1,2);}",
    ] {
        let module = assert_execution_with_optimization(source, true);
        assert!(module.functions.iter().flat_map(|f| &f.blocks).flat_map(|b| &b.insts)
            .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "possible")), "{source}");
    }
}

#[test]
fn short_circuit_ranges_use_the_guarded_source_width() {
    let module = assert_execution_with_optimization(
        include_str!("../../../tests/fixtures/guarded-integer-range.c"),
        true,
    );
    assert!(!module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .any(
            |i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "impossible_range")
        ));
    for expression in [
        "x<=0 || (unsigned)(x-1)<7U",
        "x<=0 || (unsigned)(x-1)<(unsigned)(x-2)",
        "x<=0 || (unsigned char)(x-1)==0",
        "x<=0 || (unsigned)((x=0)-1)<4294967295U",
    ] {
        let source = format!("int hits;void possible(void){{++hits;}}int f(int x){{if(!({expression}))possible();return hits;}}int main(void){{f(0);return 0;}}");
        let module = assert_execution_with_optimization(&source, true);
        assert!(
            module
                .functions
                .iter()
                .flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts)
                .any(|i| matches!(i, evocc_evoir::Inst::Call {callee, ..} if callee == "possible")),
            "{source}"
        );
    }
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
