//! Private storage may become SSA values, but its checks remain executable.
use evocc_common::{Arena, FileId};
use evocc_evoir::{Inst, Module, RuntimeValue};
use evocc_lex::Lexer;
use evocc_parser::Parser;
use evocc_types::TargetDataLayout;

fn lower(source: &str, optimize: bool) -> Module {
    let arena = Arena::default();
    let file = FileId::new(46_004);
    let mut lexer = Lexer::new(file, source);
    let tokens = std::iter::from_fn(|| lexer.next()).collect();
    let mut parser = Parser::new(tokens, file, source);
    let unit = parser.parse_translation_unit_in(&arena);
    assert!(!parser.had_error(), "{:?}", parser.diagnostic());
    let module = evocc_lower::lower_unit_with_optimization(
        &unit,
        &TargetDataLayout::aarch64_unknown_linux_gnu(),
        optimize,
    )
    .unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    module
}

fn outcome(module: &Module) -> Result<u64, String> {
    let main = module.functions.iter().find(|f| f.name == "main").unwrap();
    let arguments: Vec<_> = main
        .params
        .iter()
        .map(|p| RuntimeValue::new(p.ty.clone(), 0))
        .collect();
    evocc_evoir::interpret_module(
        module,
        "main",
        &arguments,
        evocc_evoir::InterpreterLimits::new(500_000),
    )
    .map(|result| result.return_values[0].raw_bits)
    .map_err(|error| error.code().to_owned())
}

fn memory_ops(module: &Module, name: &str) -> usize {
    module
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap()
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter(|inst| matches!(inst, Inst::Load { .. } | Inst::Store { .. }))
        .count()
}

#[test]
fn private_cells_follow_branch_and_loop_values() {
    for source in [
        "int main(void){int a[2]; a[0]=3; a[1]=5; return a[0]+a[1];}",
        "int f(int n){int a[2]; a[0]=1; a[1]=2; if(n) a[0]=7; else a[1]=9; return a[0]*10+a[1];} int main(void){return f(0)+f(1);}",
        "int f(int n){int a[2]; a[0]=1; a[1]=2; while(n-- > 0){int t=a[0]; a[0]=a[1]; a[1]=t+a[1];} return a[0]+a[1];} int main(void){return f(7);}",
        "int main(void){unsigned char a[2]; a[0]=255; a[1]=3; a[0]+=a[1]; return a[0]+a[1];}",
        "int f(int n){int a[1]; if(n) a[0]=7; else a[0]=9; return a[0];} int main(void){return f(0)+f(1);}",
        "int f(int n){int a[1]; a[0]=8; while(n--){if(n==2) continue; a[0]+=n; if(n==1) break;} return a[0];} int main(void){return f(0)+f(5);}",
        "int f(int n){int a[1]; a[0]=8; do {a[0]+=n;} while(n-- > 0); return a[0];} int main(void){return f(0)+f(5);}",
    ] {
        let before = lower(source, false);
        let after = lower(source, true);
        assert_eq!(outcome(&after), outcome(&before), "{source}");
        let name = if source.starts_with("int f(") {
            "f"
        } else {
            "main"
        };
        assert!(memory_ops(&before, name) > 0);
        assert_eq!(
            memory_ops(&after, name),
            0,
            "{}",
            evocc_evoir::render_module(&after)
        );
        // Allocation identity, lifetime and bounds remain IR obligations.
        let text = evocc_evoir::render_module(&after);
        assert!(
            text.contains(" = alloca ") && text.contains("access_in_bounds"),
            "{text}"
        );
        assert_eq!(
            text.matches("lifetime.end").count(),
            evocc_evoir::render_module(&before)
                .matches("lifetime.end")
                .count(),
            "{text}"
        );
    }
}

#[test]
fn private_cell_promotion_preserves_uninitialized_and_invalid_accesses() {
    for source in [
        "int main(void){int a[2]; a[1]=2; return a[0];}",
        "int main(void){int x; int a[1]; a[0]=x; return 0;}",
        "int main(void){int x; int a[1]; a[0]=x; a[0]=7; return a[0];}",
        "int f(int n){int a[1]; if(n) a[0]=1; return a[0];} int main(void){return f(0);}",
        "int f(int n){int a[1]; while(n-- > 0) a[0]=1; return a[0];} int main(void){return f(0);}",
        "int f(int n){int a[1]; while(n-- > 0){int v=a[0]; a[0]=7; return v;} return 0;} int main(void){return f(1);}",
        "int main(void){int a[1]; a[0]=7; return a[1];}",
        "int main(void){int a[1]; a[0]=7; return *(int*)((char*)a+1);}",
        "struct S{unsigned long x;}; int main(void){struct S a={7}; struct S *p=0; *p=a; return 0;}",
    ] {
        let before = lower(source, false);
        let after = lower(source, true);
        assert!(outcome(&before).is_err(), "{source}");
        assert_eq!(outcome(&after), outcome(&before), "{source}");
        assert!(
            after
                .functions
                .iter()
                .any(|f| memory_ops(&after, &f.name) > 0)
        );
    }
}

#[test]
fn private_cell_promotion_declines_escape_volatile_and_overlapping_views() {
    for source in [
        "int main(void){volatile int a[1]; a[0]=7; return a[0];}",
        "int main(void){int a[1]; a[0]=7; asm volatile(\"\" : : \"r\"(a) : \"memory\"); return a[0];}",
        "extern void use(int*); int f(void){int a[1]; a[0]=7; use(a); return a[0];}",
        "int f(int n){int a[2]; a[0]=7; a[1]=9; return a[n];}",
        "int f(int n){int a[1],b[1]; a[0]=7; b[0]=9; int *p=n?a:b; return *p;}",
        "int f(int n){int a[2]; a[0]=7; a[1]=9; int *p=a; while(n-- > 0) p++; return *p;}",
        "int main(void){unsigned a[1]; a[0]=7; ((unsigned char*)a)[1]=1; return a[0];}",
    ] {
        let after = lower(source, true);
        assert!(
            after
                .functions
                .iter()
                .any(|f| memory_ops(&after, &f.name) > 0),
            "{source}"
        );
    }
}

#[test]
fn independent_private_objects_do_not_hide_external_effects() {
    let source = "int g; void update(void){g=19;} int main(void){int a[1],b[1]; a[0]=3; b[0]=7; update(); g+=a[0]; return g+b[0];}";
    let before = lower(source, false);
    let after = lower(source, true);
    assert_eq!(outcome(&before), Ok(29));
    assert_eq!(outcome(&after), Ok(29));
    assert!(memory_ops(&after, "main") < memory_ops(&before, "main"));
    // The public write/read is not private storage and remains observable.
    assert!(memory_ops(&after, "main") > 0);
}

#[test]
fn whole_integer_records_keep_copy_results_without_private_memory() {
    for source in [
        "struct S{unsigned long x;}; int main(void){struct S a={7},b; b=a; return b.x;}",
        "struct S{unsigned long x;}; int main(void){struct S a={7},b; b=a; a=b; a=a; return a.x;}",
        "struct S{unsigned long x;}; int f(int n){struct S a={7}; if(n){struct S b=a; b.x+=n; a=b;} return a.x;} int main(void){return f(0)+f(2);}",
        "struct S{unsigned long x;}; struct S g; int main(void){struct S a={7}; g=a; return g.x;}",
    ] {
        let before = lower(source, false);
        let after = lower(source, true);
        assert_eq!(outcome(&after), outcome(&before), "{source}");
        let text = evocc_evoir::render_module(&after);
        assert!(!text.contains(" = copy "), "{text}");
        let name = if source.contains("int f(") {
            "f"
        } else {
            "main"
        };
        if !source.contains("struct S g;") {
            assert_eq!(memory_ops(&after, name), 0, "{text}");
        }
    }
}

#[test]
fn representation_copies_do_not_turn_unknown_bytes_into_scalar_reads() {
    for source in [
        "struct S{unsigned long x;}; int main(void){struct S a,b; b=a; return 0;}",
        "struct S{unsigned long x;}; int main(void){struct S a,b; b=a; return b.x;}",
        "struct S{unsigned char a; unsigned b;}; int main(void){struct S a={1,2},b; b=a; return b.b;}",
        "struct S{int *x;}; int g=9; int main(void){struct S a={&g},b; b=a; return *b.x;}",
        "union S{unsigned long x;}; volatile union S g; int main(void){union S a; a=g; return a.x;}",
        "struct S{unsigned long x;}; int main(void){struct S a={7}; for(int i=0;i<4;i++){struct S b=a; b.x+=i; a=b;} return a.x;}",
    ] {
        let before = lower(source, false);
        let after = lower(source, true);
        assert_eq!(outcome(&after), outcome(&before), "{source}");
        assert!(
            evocc_evoir::render_module(&after).contains("copy"),
            "{source}"
        );
    }
}

#[test]
fn private_storage_matches_the_scalar_oracle() {
    let source = include_str!("../../../tests/fixtures/private-storage-promotion.c");
    for optimize in [false, true] {
        assert_eq!(outcome(&lower(source, optimize)), Ok(0));
    }
}
