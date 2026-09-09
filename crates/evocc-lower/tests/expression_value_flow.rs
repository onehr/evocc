use evocc_chir::{interp, CHirDecl};
use evocc_common::{Arena, FileId};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn agrees(source: &str) {
    evaluates(source, None);
}

fn evaluates(source: &str, diagnostic: Option<&str>) {
    let arena = Arena::default();
    let file = FileId::new(45023);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{source}: {:?}", parser.diagnostic());
    let symbol = unit
        .iter()
        .find_map(|decl| match decl {
            CHirDecl::Function(function) if function.name.spelling == "main" => {
                Some(function.symbol)
            }
            _ => None,
        })
        .unwrap();
    let module = evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu())
        .unwrap_or_else(|error| panic!("{source}: {error:?}"));
    let function = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments = function
        .params
        .iter()
        .map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0))
        .collect::<Vec<_>>();
    let execution = evocc_evoir::interpret_module(
        &module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(10000),
    );
    let chir =
        interp::interpret_scalar_function(&unit, symbol, interp::InterpreterLimits::new(10000));
    if let Some(code) = diagnostic {
        assert_eq!(execution.unwrap_err().code(), code, "EvoIR: {source}");
        assert_eq!(chir.unwrap_err().code(), code, "C-HIR: {source}");
        return;
    }
    let execution = execution.unwrap();
    assert_eq!(execution.return_values[0].raw_bits, 0, "EvoIR: {source}");
    let execution = chir.unwrap();
    assert_eq!(
        execution.return_value.unwrap().raw_bits,
        0,
        "C-HIR: {source}"
    );
    if evocc_chir::inline::has_inline_definitions(&unit) {
        let rewritten = evocc_chir::inline::substitute(
            &unit,
            &arena,
            &TargetDataLayout::aarch64_unknown_linux_gnu(),
        )
        .unwrap();
        let execution = interp::interpret_scalar_function(
            &rewritten,
            symbol,
            interp::InterpreterLimits::new(10000),
        )
        .unwrap();
        assert_eq!(
            execution.return_value.unwrap().raw_bits,
            0,
            "substituted C-HIR: {source}"
        );
    }
}

#[test]
fn declaration_specifier_order_preserves_storage_and_types() {
    agrees(include_str!(
        "../../../tests/fixtures/declaration-specifier-order.c"
    ));
}

#[test]
fn qualifier_spellings_preserve_the_qualified_type() {
    agrees(include_str!(
        "../../../tests/fixtures/qualifier-spellings.c"
    ));
    for source in [
        "int main(void){__const int x=1;x=2;return x;}",
        "int main(void){int x=1;int * __const__ p=&x;p=&x;return 0;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45026);
        let mut lexer = Lexer::new(file, source);
        let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
        let _ = parser.parse_translation_unit_in(&arena);
        assert_eq!(
            parser.diagnostic().map(|d| d.code),
            Some("E2101"),
            "{source}"
        );
    }
}

#[test]
fn extension_prefix_on_record_members_preserves_layout_and_values() {
    agrees(include_str!(
        "../../../tests/fixtures/record-extension-prefix.c"
    ));
}

#[test]
fn identifier_list_parameters_keep_declared_objects_and_promoted_interface() {
    agrees(include_str!(
        "../../../tests/fixtures/identifier-list-functions.c"
    ));
}

#[test]
fn identifier_list_declaration_order_does_not_change_argument_order() {
    agrees("int f(a,b,c) long c; int a,b; {return a!=1||b!=2||c!=3;} int main(void){return f(1,2,3L);}");
    agrees("int f(a) const int a; {return a!=7;} int main(void){return f(7);}");
    agrees("int f();int main(void){return f(255)!=-1;}int f(c) signed char c;{return c;}");
    agrees(
        "int f(c) signed char c;{return c;} extern typeof(f) f;int main(void){return f(255)!=-1;}",
    );
    agrees("static inline int f(c) signed char c;{return c;}int main(void){return f(255)!=-1;}");
    agrees("int f(char);int f(a) char a;{return a;}int main(void){return f(255)!=255;}");
}

#[test]
fn invalid_identifier_lists_remain_diagnosed() {
    for source in [
        "int f(a) int b; { return 0; }",
        "int f(a,a) int a; { return a; }",
        "int f(a) int a; int a; { return a; }",
        "int f(a) int a=1; { return a; }",
        "int f(a) static int a; { return a; }",
        "int f(a) { return a; }",
        "int f(a);",
        "int (*p)(a);",
        "int f(int (*p)(a));",
        "int f(long); int f(a) char a; {return a;}",
        "int f(signed char); int f(a) int a; {return a;}",
    ] {
        let arena = Arena::default();
        let file = FileId::new(45025);
        let mut lexer = Lexer::new(file, source);
        let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
        let _ = parser.parse_translation_unit_in(&arena);
        assert!(parser.had_error(), "accepted {source}");
        assert!(
            parser.diagnostic().is_some(),
            "missing diagnostic: {source}"
        );
    }
}

#[test]
fn compound_literal_identity_follows_its_block_activation() {
    agrees(include_str!(
        "../../../tests/fixtures/compound-literal-identity.c"
    ));
    agrees("int main(void){int n=0;int v=({int *p=0,*q=0;again:q=p;p=&(int){++n};if(n<2)goto again; p==q && *q==2;});return !v;}");
    agrees("int main(void){int n=0,sum=0;for(int *p=&(int){3};n<3;++n)sum+=*p;switch(n){case 3:{int *p=&(int){7};sum+=*p;break;}default:return 2;}return sum!=16;}");
}

#[test]
fn compound_cleanup_order_uses_ir_identity_not_host_ast_addresses() {
    let source = "struct S{int *p;};int main(void){int n=0;again:if(n)return 0;{struct S *p=&(struct S){&(int){7}};n=*p->p;goto again;}}";
    let arena = Arena::default();
    let file = FileId::new(45024);
    let mut lexer = Lexer::new(file, source);
    let mut parser = Parser::new(std::iter::from_fn(|| lexer.next()).collect(), file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module =
        evocc_lower::lower_unit(&unit, &TargetDataLayout::aarch64_unknown_linux_gnu()).unwrap();
    let mut checked = false;
    for block in &module.functions[0].blocks {
        let mut allocated = block
            .insts
            .iter()
            .filter_map(|inst| match inst {
                evocc_evoir::Inst::Alloca { result, .. } => Some(result.id),
                _ => None,
            })
            .collect::<Vec<_>>();
        if allocated.len() != 2 {
            continue;
        }
        let ended = block
            .insts
            .iter()
            .filter_map(|inst| match inst {
                evocc_evoir::Inst::LifetimeEnd { pointer, .. }
                    if allocated.contains(&pointer.id) =>
                {
                    Some(pointer.id)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        allocated.reverse();
        assert_eq!(ended, allocated);
        checked = true;
    }
    assert!(checked, "missing nested compound storage block");
}

#[test]
fn empty_union_initialization_does_not_define_inactive_tail_bytes() {
    evaluates(
        include_str!("../../../tests/fixtures/empty-union-initialization.c"),
        Some("E3001"),
    );
}

#[test]
fn entering_a_compound_literal_scope_by_goto_creates_its_storage() {
    agrees("int main(void){goto inside;{inside:return (int){7}!=7;}}");
    agrees("int main(void){int *p=0,n=0;goto inside;{inside:p=&(int){++n};if(n<2)goto inside;return *p!=2;}}");
}

#[test]
fn compound_literal_lifetime_ends_on_each_scope_exit() {
    for source in [
        "int main(void){int *p;{p=&(int){7};}return *p;}",
        "int main(void){int *p;for(int i=0;i<2;++i){if(i)return *p;p=&(int){7};}return 0;}",
        "int main(void){int *p;while(1){p=&(int){7};break;}return *p;}",
        "int main(void){int *p;for(int i=0;i<1;++i){p=&(int){7};continue;}return *p;}",
        "int main(void){int *p=({&(int){7};});return *p;}",
        "int main(void){int *p;switch(1){case 1:p=&(int){7};break;}return *p;}",
    ] {
        evaluates(source, Some("E3010"));
    }
}

#[test]
fn jumps_into_blocks_keep_live_values_and_skip_initializers() {
    agrees(include_str!("../../../tests/fixtures/goto-into-block.c"));
}

#[test]
fn jumps_preserve_object_storage_and_only_execute_reached_initializers() {
    agrees(include_str!(
        "../../../tests/fixtures/goto-object-storage.c"
    ));
}

#[test]
fn a_backward_jump_across_a_declaration_keeps_the_same_object() {
    agrees("int main(void){int n=0,*p=0;again:;int a[2]={5,6};if(n++==0){p=a;goto again;}return p!=a||*p!=5;}");
}

#[test]
fn a_jump_into_a_block_allocates_an_address_taken_scalar_without_initializing_it() {
    agrees("int calls;int main(void){goto target;{int x=++calls;target:;int *p=&x;*p=9;return x!=9||calls!=0;}}");
}

#[test]
fn jumping_over_an_initializer_leaves_the_object_uninitialized() {
    evaluates(
        "int main(void){goto target;{int a[2]={7,8};target:return a[0];}}",
        Some("E3001"),
    );
}

#[test]
fn automatic_objects_end_on_break_continue_and_outgoing_goto() {
    for source in [
        "int main(void){int *p;while(1){int a[1]={7};p=a;break;}return *p;}",
        "int main(void){int *p;for(int i=0;i<1;i++){int a[1]={7};p=a;continue;}return *p;}",
        "int main(void){int *p,n=0;for(int a[1]={7};n<1;n++){p=a;}return *p;}",
        "int main(void){int *p;{int a[1]={7};p=a;goto out;}out:return *p;}",
    ] {
        evaluates(source, Some("E3010"));
    }
}

#[test]
fn reentering_a_block_does_not_revive_an_escaped_pointer() {
    evaluates(
        "int main(void){int *p=0,n=0;again:{int a[1]={7};if(n++)return *p;p=a;}goto again;}",
        Some("E3010"),
    );
}

#[test]
fn switch_dispatch_creates_skipped_object_storage() {
    agrees("int f(int x){switch(x){int a[2]={7,8};case 1:a[0]=9;return a[0]!=9;case 2:a[1]=5;return a[1]!=5;}return 0;}int main(void){return f(1)||f(2);}");
}

#[test]
fn enum_bitfields_preserve_the_enumerator_range() {
    agrees(include_str!(
        "../../../tests/fixtures/enum-bitfield-signedness.c"
    ));
}

#[test]
fn compound_assignment_observes_rhs_call_effects() {
    agrees(include_str!(
        "../../../tests/fixtures/compound-assignment-call-effects.c"
    ));
}

#[test]
fn narrow_bitfields_promote_by_range_not_declared_rank() {
    agrees(include_str!(
        "../../../tests/fixtures/bitfield-integer-promotions.c"
    ));
}

#[test]
fn bitfield_promotion_is_visible_in_unevaluated_arithmetic() {
    agrees("struct S { unsigned long x:3; signed long y:32; unsigned long z:32; }; struct S s; _Static_assert(sizeof(+s.x)==4, \"narrow unsigned\"); _Static_assert(sizeof(+s.y)==4, \"signed int range\"); _Static_assert(sizeof(+s.z)==4, \"unsigned int range\"); int main(void){return 0;}");
}

#[test]
fn bitfield_promotions_cover_pointer_anonymous_and_variadic_uses() {
    agrees("struct S { struct { unsigned long x:3; }; }; struct S s={.x=7}; int take(int n,...){__builtin_va_list ap; __builtin_va_start(ap,n); int v=__builtin_va_arg(ap,int); __builtin_va_end(ap); return v;} int main(void){struct S *p=&s; _Static_assert(sizeof(+p->x)==4, \"pointer field\"); _Static_assert(sizeof(1 ? s.x : -1)==4, \"conditional\"); _Static_assert(sizeof(s.x ?: -1)==4, \"elvis\"); return take(0,s.x)!=7 || p->x-8!=-1;}");
}

#[test]
fn a_pointer_return_value_can_be_stored_through_another_pointer() {
    agrees("int n=7; int *get(void){return &n;} int main(void){int *p=0;int **q=&p;*q=get();return *p!=7;}");
}

#[test]
fn aggregate_array_elements_can_be_passed_and_copied() {
    agrees("struct S{int n;};struct S a[2]={{3},{7}};int f(struct S s){return s.n;}int main(void){struct S b=a[1];return f(a[0])!=3 || b.n!=7;}");
}

#[test]
fn pointer_equality_accepts_integer_signedness_counterparts() {
    agrees("int main(void){short x=0;short *p=&x;unsigned short *q=(unsigned short *)&x;int r=p!=q;return r || !(p==q);}");
}

#[test]
fn assignment_values_are_operands_with_their_effects_preserved() {
    agrees("int main(void){int a=0;int b=0;int r=(a=3)+(b=4);return r!=7 || a!=3 || b!=4;}");
    agrees("int a;int main(void){int b=2;int r=(a=3)*(b+=4);return r!=18 || a!=3 || b!=6;}");
}

#[test]
fn a_selected_short_circuit_operand_defines_a_previously_uninitialized_local() {
    agrees("int f(int y){int x;if(y||(x=5))return x;return 0;}int main(void){return f(0)!=5;}");
    agrees("int f(int y){int x=9;if(y||(x=5))return x;return 0;}int main(void){return f(1)!=9 || f(0)!=5;}");
}

#[test]
fn inline_calls_preserve_parameters_returns_labels_and_static_identity() {
    agrees("static __attribute__((always_inline)) int f(int x){static int n=3; ++n; if(x) return n+x; return n;} int main(void){return f(2)!=6 || f(0)!=5;}");
    agrees("static __attribute__((always_inline)) int f(int x){if(x)goto done;x=7;done:return x;} int main(void){int x=0;int a=f(x++);int b=f(x++);return a!=7 || b!=1 || x!=2;}");
    agrees("static __attribute__((always_inline)) int g(int x){return x+1;} static __attribute__((always_inline)) int f(int x){return g(x)*2;} int main(void){return f(3)!=8;}");
    agrees("static __attribute__((always_inline)) void f(int *p){*p+=3;return;} int main(void){int x=2;f(&x);return x!=5;}");
    agrees("struct S{int n;};static __attribute__((always_inline)) struct S f(struct S s){s.n++;return s;}int main(void){struct S s={3};struct S b=f(s);return b.n!=4 || s.n!=3;}");
    agrees("static __attribute__((always_inline)) int f(void){return __func__[0];}int main(void){return f()!='f';}");
    agrees("static __attribute__((always_inline)) int f(void){static int n=2;return ++n;}int (*p)(void)=f;int main(void){return f()!=3 || p()!=4;}");
    agrees("static __attribute__((always_inline)) int f(int n,...){return n;}int main(void){return f(3,4)!=3;}");
    agrees("int n=3;int g(void){return n;}static __attribute__((always_inline)) int f(void){extern int n;extern int g(void);return n+g();}int main(void){int n=9;return f()!=6 || n!=9;}");
}

#[test]
fn dereferenced_function_pointers_remain_function_designators() {
    agrees("int f(int n){return n+1;} int main(void){int (*p)(int)=f;return (*p)(6)!=7;}");
}

#[test]
fn empty_parameter_lists_in_definitions_have_no_parameters() {
    agrees("int f(){return 7;} int main(){return f()!=7;}");
    agrees("int count;void f(){++count;}int main(){f();return count!=1;}");
}

#[test]
fn inline_rewriting_preserves_existing_statement_expression_values() {
    agrees("static __attribute__((always_inline)) int *get(int *p){return p;} int main(void){int x=7;return *({get(&x);})!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){typedef int T;struct S{T x;};struct S s={n};return s.x;} int main(void){return f(3)+f(4)!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){return n;} int main(void){int x=({int n=7;f(n);});return x!=7;}");
}

#[test]
fn inline_calls_in_unevaluated_static_syntax_are_not_executed() {
    agrees("static __attribute__((always_inline)) int f(int n){return n;} int a[2]={[1 ? 1 : f(0)]=7};int main(void){return a[1]!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){return n;} static int x=1 ? 7 : f(0);int main(void){return x!=7;}");
    agrees("static __attribute__((always_inline)) int f(int n){static int x=1 ? 7 : f(0);return x+n;} int main(void){return f(1)!=8;}");
}

#[test]
fn members_of_returned_records_keep_their_value() {
    agrees("struct S{int n;};static __attribute__((always_inline)) struct S f(int n){struct S s={n};return s;}int main(void){return f(7).n!=7;}");
    agrees("struct S{int n;};struct S f(int n){struct S s={n};return s;}int main(void){return f(7).n!=7;}");
}

#[test]
fn inline_returns_from_nested_object_scopes_join_without_dead_addresses() {
    agrees("static __attribute__((always_inline)) int f(int n){if(n){int a[2]={3,7};return a[1];}return 5;}int main(void){return f(1)!=7 || f(0)!=5;}");
    agrees("static __attribute__((always_inline)) int f(int n){if(n){int a[2]={3,7};if(n>1)return a[0];return a[1];}return 5;}int main(void){return 2+f(2)!=5 || 2+f(1)!=9 || 2+f(0)!=7;}");
}

#[test]
fn completed_array_declarations_keep_existing_uses_valid() {
    agrees("extern int a[];int f(void){return a[0];} int a[4]={7};int main(void){return f()!=7;}");
    agrees("typedef int A[];static const A a={7};int main(void){return a[0]!=7;}");
    agrees("struct S{int n;};typedef struct S A[];static const A a={{7}};int main(void){return a[0].n!=7;}");
    agrees("int a[4] __attribute__((aligned(64)))={7};int f(int *p){return *p;} int main(void){return f(a)!=7;}");
}

#[test]
fn array_decay_before_later_completion_keeps_its_element_type() {
    agrees("static const int a[];int f(const int *p){return *p;}int g(void){return f(a);}static const int a[]={7};int main(void){return g()!=7;}");
}

#[test]
fn function_uses_before_compatible_parameter_redeclarations_remain_valid() {
    agrees("int f(const int n);int g(void){return f(7);}int f(int n){return n;}int main(void){return g()!=7;}");
    agrees(
        "int f(int n);int (*p)(int)=f;int f(const int n){return n;}int main(void){return p(7)!=7;}",
    );
}

#[test]
fn completion_does_not_hide_different_bounds_elements_or_qualifiers() {
    let source = "extern int a[]; int b[4]; int c[5]; const int d[4]; long e[4];";
    let arena = Arena::default();
    let file = FileId::new(45024);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error());
    let ty = |name| {
        unit.iter()
            .find_map(|d| match d {
                CHirDecl::Object(o) if o.name.spelling == name => Some(o.ty),
                _ => None,
            })
            .unwrap()
    };
    let matches = |a, b| evocc_chir::type_at_use_matches_binding(unit.types(), ty(a), ty(b));
    assert!(matches("a", "b"));
    assert!(!matches("b", "a"));
    assert!(!matches("b", "c"));
    assert!(!matches("a", "d"));
    assert!(!matches("a", "e"));
}

#[test]
fn a_conditional_array_index_is_evaluated_once() {
    agrees("int main(void){int a[2]={3,7};int n=0;return a[n++ ? 0 : 1]!=7 || n!=1;}");
}

#[test]
fn a_local_bit_field_record_is_initialized_and_updated() {
    agrees("struct S{unsigned a:3;unsigned b:5;};int main(void){struct S s={2,7};s.a+=1;return s.a!=3 || s.b!=7;}");
}

#[test]
fn bit_fields_preserve_neighbors_sign_and_cross_byte_values() {
    for source in [
        "struct S{signed a:4;unsigned b:12;};int main(void){struct S s={-3,1000};int old=s.a++;return old!=-3 || s.a!=-2 || s.b!=1000;}",
        "struct __attribute__((packed)) S{unsigned a:9;unsigned b:7;};int main(void){struct S s={511,73};s.a=0;return s.a!=0 || s.b!=73 || sizeof(s)!=2;}",
        "struct S{char head;unsigned b:3;};int main(void){struct S s={42,5};s.b++;return s.head!=42 || s.b!=6;}",
        "struct S{unsigned a:3;unsigned b:5;};struct S s={2,7};int main(void){return ++s.a!=3 || s.b!=7;}",
        "struct S{unsigned long a:60;unsigned long b:4;};int main(void){struct S s={0xfffffffffffffffUL,9};s.a=3;unsigned long value=s.a;return value!=3 || s.b!=9;}",
        "struct S{unsigned a:3;};int main(void){struct S s={1};return !((s.a-2)<0);}",
    ] { agrees(source); }
}
