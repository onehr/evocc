use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn interpreter_results(source: &str) -> (u128, u128) {
    let arena = Arena::default();
    let file = FileId::new(45024);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}: {source}", parser.diagnostic());
    let main = unit
        .iter()
        .find_map(|declaration| match declaration {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let chir =
        interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
            .unwrap()
            .return_value
            .unwrap()
            .raw_bits;
    let mut evoir = 0;
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            optimize,
        )
        .unwrap();
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap();
        let args = main
            .params
            .iter()
            .map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0))
            .collect::<Vec<_>>();
        evoir = evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .unwrap()
        .return_values[0]
            .raw_bits;
        assert_eq!(u128::from(evoir), chir, "optimized={optimize}: {source}");
    }
    (chir, evoir.into())
}

#[test]
fn guarded_constant_queries_use_dominating_range_facts() {
    for source in [
        "int check(int n){if(n>0){int x=n;return __builtin_constant_p((long long)x>=0)&&((long long)x>=0);}return 1;}
         int main(void){return check(4)!=1||check(-1)!=1;}",
        "int check(int n){while(n>0){int x=n;if(!(__builtin_constant_p((long long)x>=0)&&((long long)x>=0)))return 1;n-=3;}return 0;}
         int main(void){return check(8);}",
        "int check(int n){if(n<0)return __builtin_constant_p((long)n<0);else return __builtin_constant_p((long)n>=0);}
         int main(void){return check(-4)!=1||check(4)!=1;}",
        "int check(int n){while(n>0){int x=n;do{if(!(__builtin_constant_p((long long)x>=0)&&((long long)x>=0)))return 1;}while(0);n-=3;}return 0;}
         int main(void){return check(8);}",
    ] {
        assert_eq!(interpreter_results(source), (0, 0), "{source}");
    }
}

#[test]
fn guarded_query_facts_do_not_survive_mutation_or_bypass() {
    for source in [
        "int check(int n,int value){if(n>0){n=value;return __builtin_constant_p(n>=0);}return 0;}
         int main(void){return check(4,-2);}",
        "int check(volatile int n){if(n>0)return __builtin_constant_p(n>=0);return 0;}
         int main(void){return check(4);}",
        "void change(int *p){*p=-1;}int check(int n){if(n>0){change(&n);return __builtin_constant_p(n>=0);}return 0;}
         int main(void){return check(4);}",
        "int check(int n,int jump){if(jump)goto inside;if(n>0){inside:return __builtin_constant_p(n>=0);}return 0;}
         int main(void){return check(-4,1);}",
        "int check(int n){if(n>0){int x=n;(void)x;}return __builtin_constant_p(n>=0);}
         int main(void){return check(4);}",
        "int check(int n){while(n>0){n=-1;}return __builtin_constant_p(n>=0);}
         int main(void){return check(4);}",
        "int check(int n,int value){if(n>0){do{n=value;}while(0);return __builtin_constant_p(n>=0);}return 0;}
         int main(void){return check(4,-1);}",
    ] {
        assert_eq!(interpreter_results(source), (0, 0), "{source}");
    }
}

#[test]
fn kernel_build_assertions_leave_no_unreachable_calls() {
    let source = include_str!("../../../tests/fixtures/kernel-build-assert-folds.c");
    assert_eq!(interpreter_results(source), (0, 0));
    let arena = Arena::default();
    let file = FileId::new(45025);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    for optimize in [false, true] {
        let module = evocc_lower::lower_unit_with_optimization(
            &unit,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
            optimize,
        )
        .unwrap();
        assert!(module
            .functions
            .iter()
            .any(|function| function.name == "check_sender"));
        assert!(module.functions.iter().flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts).any(|i|
                matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "check_sender")),
            "noinline call was replaced instead of propagating its parameter");
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|block| &block.insts) {
                assert!(
                    !matches!(instruction, evocc_evoir::Inst::Call { callee, .. }
                        if callee.starts_with("invalid_")),
                    "unreachable build assertion retained in {}: {instruction:?}",
                    function.name,
                );
            }
        }
    }
}

#[test]
fn build_assertion_facts_do_not_invent_values_or_erase_effects() {
    for source in [
        "struct R{unsigned value;void *ptr;int tail[];};
         int main(void){struct R r={.value=~0U};r.value=0;return r.value>=16;}",
        "struct R{unsigned value;void *ptr;int tail[];};
         void change(struct R *r){r->value=0;}
         int main(void){struct R r={.value=~0U};change(&r);return r.value>=16;}",
        "struct R{unsigned value;void *ptr;int tail[];};
         int main(void){volatile struct R r={.value=~0U};r.value=0;return r.value>=16;}",
        "int n;int main(void){int x;if(!((++n),&x))return 2;return n!=1;}",
        "static inline int check(int *p){return !p;}int main(void){return check(0)!=1;}",
    ] {
        assert_eq!(interpreter_results(source), (0, 0), "{source}");
    }
    // ELF weak references may resolve to null. Object addresses are not
    // invented integer values either, even when their truth is known.
    for source in [
        "extern int object __attribute__((weak));void selected(void);
         int f(void){if(!&object)selected();return 0;}",
        "extern void weak_fn(void) __attribute__((weak));void selected(void);
         int f(void){if(weak_fn==0)selected();return 0;}",
        "int object;void selected(void);
         int f(void){if((unsigned long)&object==1)selected();return 0;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45026);
        let mut lexer = Lexer::new(file, source);
        let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        for optimize in [false, true] {
            let module = evocc_lower::lower_unit_with_optimization(
                &unit,
                &TargetDataLayout::aarch64_unknown_linux_gnu(),
                optimize,
            )
            .unwrap();
            assert!(module.functions.iter().flat_map(|f| &f.blocks)
                .flat_map(|b| &b.insts).any(|i|
                    matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "selected")),
                "unknown address test was folded: {source}");
        }
    }
}

#[test]
fn function_argument_facts_require_all_callers_and_stable_parameters() {
    for source in [
        "static void a(void){}static void b(void){}
         static __attribute__((noinline)) int test(void(*fn)(void)){return fn==a;}
         int main(void){return test(a)!=1 || test(b)!=0;}",
        "static void a(void){}static void b(void){}
         static __attribute__((noinline)) int test(void(*fn)(void)){fn=b;return fn==a;}
         int main(void){return test(a);}",
        "static void a(void){}static void b(void){}
         static __attribute__((noinline)) int test(void(*fn)(void)){return fn==a;}
         int (*escaped)(void(*)(void))=test;
         int main(void){return test(a)!=1 || escaped(b)!=0;}",
        "static void a(void){}static void b(void){}
         int test(void(*fn)(void)){return fn==a;}
         int main(void){return test(a)!=1 || test(b)!=0;}",
        "static void a(void){}static void b(void){}int n;
         void(*get(void))(void){++n;return n==1?a:b;}
         static __attribute__((noinline)) int test(void(*fn)(void)){return fn==a;}
         int main(void){int first=test(get());int second=test(get());return first!=1 || second!=0 || n!=2;}",
    ] {
        assert_eq!(interpreter_results(source), (0, 0), "{source}");
    }
}

#[test]
fn expectation_evaluation_is_fixed_before_optimization() {
    for (source, expected) in [
        ("volatile long input=3; long main(void){int z=7; long v=__builtin_expect(input,z++); return v*100+z;}", 308),
        ("long main(void){int z=7; long v=__builtin_expect(1,z++); return v*100+z;}", 107),
        ("long main(void){const long k=1;int z=7;long v=__builtin_expect(k,z++);return v*100+z;}", 108),
        ("volatile long input=3;long main(void){int z=7;long v=__builtin_expect(__builtin_expect(input,0),z++);return v*100+z;}", 308),
        ("long main(void){int z=7;long v=__builtin_expect(__builtin_expect(1,z++),z++);return v*100+z;}", 107),
        ("static __attribute__((always_inline)) long hint(long k,int *p){return __builtin_expect(k,(*p)++);} long main(void){int z=7;long v=hint(1,&z);return v*100+z;}", 108),
        ("long main(void){int z=7;unsigned long n=sizeof(__builtin_expect(z++,z++));if(0)__builtin_expect(z++,z++);return n*100+z;}", 807),
        ("int trace;long first(void){trace=trace*10+1;return 3;}long second(void){trace=trace*10+2;return 0;}long main(void){long v=__builtin_expect(first(),second());return v*100+trace;}", 312),
        ("long main(void){int z=0;long sum=0;for(int i=0;i<3;i++)sum+=__builtin_expect(i,z++);return sum*100+z;}", 303),
        ("long main(void){int z=7;long v=__builtin_expect(z++,z++);return v*100+z;}", 709),
        ("long main(void){int z=7;long v=__builtin_expect(z++,z>7?z++:z--);return v*100+z;}", 709),
        ("long f(int k){return __builtin_expect(k,({if(k)return 9;0;}));}long main(void){return f(1);}", 9),
        ("long f(int k){return __builtin_expect(({if(k)return 9;0;}),({return 8;0;}));}long main(void){return f(1);}", 9),
    ] {
        assert_eq!(interpreter_results(source), (expected, expected), "{source}");
    }
}

#[test]
fn calls_invalidate_global_assignment_facts_not_local_object_values() {
    for source in [
        include_str!("../../../tests/fixtures/addressed-local-before-loop.c"),
        "int x; void write(void){x=5;} int main(void){x=0;write();return x!=5;}",
        "int x; void write(void){x=5;} int main(void){void (*f)(void)=write;x=0;f();return x!=5;}",
        "int x; void write(void){x=5;} int main(void){int x;x=7;write();return x!=7;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45023);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let main = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
                .unwrap()
                .return_value
                .unwrap()
                .raw_bits,
            0,
            "C-HIR: {source}"
        );
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let main = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = main
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        assert_eq!(
            evocc_evoir::interpret_module(
                &module,
                "main",
                &args,
                evocc_evoir::InterpreterLimits::new(10000)
            )
            .unwrap()
            .return_values[0]
                .raw_bits,
            0,
            "EvoIR: {source}"
        );
    }
}

#[test]
fn full_memory_barrier_is_ordered_assembly_not_an_external_function() {
    let source = include_str!("../../../tests/fixtures/full-memory-barrier.c");
    let arena = Arena::default();
    let file = FileId::new(45021);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let assemblies = module
        .functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.insts)
        .filter_map(|i| match i {
            evocc_evoir::Inst::Asm(a) => Some(a),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(assemblies.len(), 1, "the typeof operand is unevaluated");
    let assembly = assemblies[0];
    assert_eq!(assembly.template, "dmb ish");
    assert!(assembly.volatile);
    assert_eq!(assembly.clobbers, ["memory"]);
    assert!(assembly.inputs.is_empty() && assembly.outputs.is_empty());
    assert!(!module
        .declares
        .iter()
        .any(|f| f.name == "__sync_synchronize"));
    let text = evocc_evoir::render_module(&module);
    evocc_evoir::parse_section9_module(text.as_bytes()).unwrap();
    let main = unit
        .iter()
        .find_map(|d| match d {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
            .unwrap_err()
            .code(),
        "E3021"
    );
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let args = main
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    assert_eq!(
        evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000)
        )
        .unwrap_err()
        .code(),
        "E3021"
    );
}

#[test]
fn full_memory_barrier_rejects_arguments_and_is_not_an_integer_constant() {
    for (source, code) in [
        ("void f(void){__sync_synchronize(1);}", "E2909"),
        (
            "_Static_assert((__sync_synchronize(), 1), \"not an ICE\");",
            "E2912",
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45022);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "accepted {source}");
        assert_eq!(parser.diagnostic().unwrap().code, code, "{source}");
    }
}

#[test]
fn literal_string_comparison_does_not_bind_a_same_named_local_function() {
    let source = include_str!("../../../tests/fixtures/literal-string-comparison.c");
    for builtins in [true, false] {
        let arena = Arena::default();
        let file = FileId::new(45019);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        parser.set_standard_builtins(builtins);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let main = unit
            .iter()
            .find_map(|decl| match decl {
                CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                _ => None,
            })
            .unwrap();
        let chir =
            interp::interpret_scalar_function(&unit, main, interp::InterpreterLimits::new(10000))
                .unwrap();
        assert_eq!(chir.return_value.unwrap().raw_bits, 0);
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        assert!(!evocc_evoir::render_module(&module).contains("call @strcmp"));
        let f = module.functions.iter().find(|f| f.name == "main").unwrap();
        let args = f
            .params
            .iter()
            .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
            .collect::<Vec<_>>();
        let ir = evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000),
        )
        .unwrap();
        assert_eq!(ir.return_values[0].raw_bits, 0);
    }
}

#[test]
fn nonliteral_builtin_comparison_with_local_binding_is_refused() {
    let source = "static int strcmp(const char *a,const char *b){return -123;} int f(const char *a,const char *b){return __builtin_strcmp(a,b);}";
    let arena = Arena::default();
    let file = FileId::new(45020);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error(), "conflicting local binding was accepted");
    assert_eq!(parser.diagnostic().unwrap().code, "E2946");
}

#[test]
fn prefetch_keeps_address_evaluation_without_an_external_call() {
    let source = "int n; int *address(void){n++;return &n;} int main(void){__builtin_prefetch(address()); __builtin_prefetch(address(),0); __builtin_prefetch(address(),1,3); return n-3;}";
    let arena = Arena::default();
    let file = FileId::new(45012);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let text = evocc_evoir::render_module(&module);
    assert!(!text.contains("call @__builtin_prefetch"), "{text}");
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let args = main
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    assert_eq!(
        evocc_evoir::interpret_module(
            &module,
            "main",
            &args,
            evocc_evoir::InterpreterLimits::new(10000)
        )
        .unwrap()
        .return_values[0]
            .raw_bits,
        0
    );
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000))
            .unwrap()
            .return_value
            .unwrap()
            .raw_bits,
        0
    );
}

#[test]
fn dynamic_stack_allocation_never_becomes_an_external_call() {
    for (source, builtin) in [
        (
            "void *f(unsigned long n){return __builtin_alloca(n);}",
            true,
        ),
        ("void f(void){__builtin_alloca(16);}", true),
        (
            "static void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
            false,
        ),
    ] {
        let arena = Arena::default();
        let file = FileId::new(45013);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .expect("dynamic storage lowers without a library fallback");
        let instructions = module
            .functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.insts);
        assert_eq!(
            instructions
                .clone()
                .any(|i| matches!(i, evocc_evoir::Inst::DynamicAlloca { .. })),
            builtin
        );
        assert_eq!(
            instructions
                .clone()
                .any(|i| matches!(i, evocc_evoir::Inst::Call { callee, .. } if callee == "alloca")),
            !builtin
        );
    }
}

#[test]
fn ordinary_allocation_calls_follow_the_builtin_option_and_prototype() {
    for (prefix, legacy, builtins) in [
        ("", true, true),
        ("extern void *alloca(unsigned long);", false, true),
        ("extern void *alloca(unsigned long);", false, false),
    ] {
        let source = format!("{prefix} int main(void) {{ unsigned long n=19; unsigned char *p=alloca(n); p[0]=17; p[18]=29; return p[0]+p[18]-46; }}");
        let arena = Arena::default();
        let file = FileId::new(45014);
        let mut lexer = Lexer::new(file, &source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, &source);
        parser.set_legacy_declarations(legacy);
        parser.set_standard_builtins(builtins);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module =
            evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
        let text = evocc_evoir::render_module(&module);
        assert_eq!(text.contains("alloca.dynamic"), builtins, "{text}");
        assert_eq!(text.contains("call @alloca"), !builtins, "{text}");
        if builtins {
            let main = unit
                .iter()
                .find_map(|d| match d {
                    CHirDecl::Function(f) if f.name.spelling == "main" => Some(f.symbol),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                interp::interpret_scalar_function(
                    &unit,
                    main,
                    interp::InterpreterLimits::new(10000)
                )
                .unwrap()
                .return_value
                .unwrap()
                .raw_bits,
                0
            );
            let function = module.functions.iter().find(|f| f.name == "main").unwrap();
            let arguments = function
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
    }
}

#[test]
fn conflicting_ordinary_allocation_bindings_are_explicitly_refused() {
    for source in [
        "void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long) asm(\"replacement\"); void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long); void *f(void){return alloca(16);} void *alloca(unsigned long n){return 0;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45015);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        let _unit = parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "conflicting builtin binding was accepted: {source}");
        assert_eq!(parser.diagnostic().unwrap().code, "E2944");
    }
}

#[test]
fn disabled_allocation_recognition_preserves_written_functions() {
    for source in [
        "void *alloca(unsigned long n){return 0;} void *f(void){return alloca(16);}",
        "extern void *alloca(unsigned long) asm(\"replacement\"); void *f(void){return alloca(16);}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45016);
        let mut lexer = Lexer::new(file, source);
        let tokens = std::iter::from_fn(|| lexer.next()).collect();
        let mut parser = Parser::new(tokens, file, source);
        parser.set_standard_builtins(false);
        let unit = parser.parse_translation_unit_in(&arena);
        assert!(!parser.had_error(), "{:?}", parser.diagnostic());
        let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
            .unwrap();
        let text = evocc_evoir::render_module(&module);
        assert!(!text.contains("alloca.dynamic"), "{text}");
        assert!(text.contains("call @alloca") || text.contains("call @replacement"), "{text}");
    }
}

#[test]
fn explicit_allocation_is_not_disabled_by_the_ordinary_name_option() {
    let source = "void *f(unsigned long n){return __builtin_alloca(n);}";
    let arena = Arena::default();
    let file = FileId::new(45017);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    parser.set_standard_builtins(false);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    assert!(evocc_evoir::render_module(&module).contains("alloca.dynamic"));
}

#[test]
fn ordinary_allocation_needs_a_declaration_outside_legacy_mode() {
    let source = "void *f(unsigned long n){return alloca(n);}";
    let arena = Arena::default();
    let file = FileId::new(45018);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let _unit = parser.parse_translation_unit_in(&arena);
    assert!(parser.had_error());
    assert_eq!(parser.diagnostic().unwrap().code, "E2001");
}
