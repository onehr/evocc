use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(["-O2", mode, "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

#[test]
fn dynamic_object_queries_capture_allocation_sizes_without_repeating_effects() {
    let source = include_str!("../../../tests/fixtures/dynamic-object-size.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__builtin_dynamic_object_size"));
}

#[test]
fn allocation_extents_survive_evaluated_statement_expression_results() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/allocation-statement-result.c"
    ));
    let declarations = "typedef unsigned long size_t;char storage[128];__attribute__((noinline,alloc_size(1))) void*allocate(size_t n){return storage;}";
    for body in [
        "int main(void){void*p=({void*r=allocate(25);r;});return __builtin_dynamic_object_size(p,1)!=25;}",
        "int main(void){void*p=({void*r;r=allocate(25);r;});return __builtin_object_size(p,1)!=25;}",
        "int main(void){size_t n=25;void*p=({void*r;if(0)r=allocate(n++);else r=allocate(n++);r;});return __builtin_dynamic_object_size(p,1)!=25||n!=26;}",
    ] {
        assert_constant_program(&format!("{declarations}{body}"));
    }
    for body in [
        "int main(void){volatile int c=0;void*p=({void*r=allocate(25);if(c)r=allocate(9);r;});return __builtin_dynamic_object_size(p,1)!=~0ul;}",
        "int main(void){void*p=({void*r=allocate(25);goto done;r=allocate(9);done:r;});return __builtin_dynamic_object_size(p,1)!=~0ul;}",
        "int main(void){int calls=0;unsigned long n=__builtin_dynamic_object_size(({++calls;allocate(25);}),1);return n!=~0ul||calls;}",
        "int main(void){void*p=({void*volatile r=allocate(25);r;});return __builtin_dynamic_object_size(p,1)!=~0ul;}",
    ] {
        assert_constant_program(&format!("{declarations}{body}"));
    }
}

#[test]
fn allocation_attributes_before_function_names_keep_their_parameter_indices() {
    assert_constant_program(
        "typedef unsigned long size_t;char storage[128];int calls;\
         void * __attribute__((alloc_size(2))) allocate(void*device,size_t n);\
         __attribute__((noinline)) void*allocate(void*device,size_t n){++calls;return storage;}\
         int main(void){volatile size_t input=25;size_t n=input;void*p=allocate(0,n++);\
         return __builtin_dynamic_object_size(p,1)!=25||n!=26||calls!=1;}",
    );
}

#[test]
fn direct_inline_returns_preserve_allocation_extents_without_replaying_calls() {
    let declarations = "typedef unsigned long size_t;char storage[128];int calls;\
        __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){++calls;return storage;}\
        static inline void*wrapper(size_t n){return allocate(n);}\
        static inline void*nested(size_t n){return wrapper(n);}";
    for body in [
        "int main(void){void*p=wrapper(11);return calls!=1||__builtin_object_size(p,1)!=11||__builtin_dynamic_object_size(p,1)!=11;}",
        "int main(void){volatile size_t input=11;size_t n=input;void*p=nested(n++);return calls!=1||n!=12||__builtin_dynamic_object_size(p,1)!=11;}",
    ] {
        assert_constant_program(&format!("{declarations}{body}"));
    }
    for (wrapper, extent) in [
        (
            "__attribute__((noinline)) void*indirect(size_t n){return allocate(n);}",
            "~0ul",
        ),
        (
            "static inline void*indirect(size_t n){if(n)return allocate(n);return allocate(7);}",
            "11",
        ),
    ] {
        // An unexpanded call stays opaque. Expanded multiple returns now
        // transport the executed arm's extent, not a body-wide size promise.
        assert_constant_program(&format!(
            "{declarations}{wrapper}int main(void){{volatile size_t n=11;void*p=indirect(n);\
             return calls!=1||__builtin_dynamic_object_size(p,1)!={extent};}}"
        ));
    }
}

#[test]
fn inline_local_results_preserve_captured_extents_and_effects() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/inline-allocation-result.c"
    ));
    let declarations = "typedef unsigned long size_t;char storage[128];int calls;\
        __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){++calls;return storage;}";
    assert_constant_program(&format!(
        "{declarations}static inline void*wrapper(size_t n){{void*p=allocate(n++);n=7;return p;}}\
         int main(void){{volatile size_t input=13;void*p=wrapper(input);\
         return __builtin_dynamic_object_size(p,1)!=13||calls!=1;}}"
    ));
    for wrapper in [
        "static inline void*wrapper(size_t n){void*p=allocate(n);goto done;done:return p;}",
        "static inline void*wrapper(size_t n){void*volatile p=allocate(n);return p;}",
        "static inline void*wrapper(size_t n){char*p=allocate(n);p++;return p;}",
        "static inline void*wrapper(size_t n){void*p=allocate(n);if(n)p=storage;return p;}",
        "__attribute__((noinline)) void redirect(void**p){*p=storage;}static inline void*wrapper(size_t n){void*p=allocate(n);redirect(&p);return p;}",
    ] {
        assert_constant_program(&format!(
            "{declarations}{wrapper}int main(void){{volatile size_t n=11;void*p=wrapper(n);\
             return calls!=1||__builtin_dynamic_object_size(p,1)!=~0ul;}}"
        ));
    }
}

#[test]
fn inline_return_paths_transport_only_the_selected_allocation_extent() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/inline-allocation-return-paths.c"
    ));
    let declarations =
        "typedef unsigned long size_t;char storage[128];char*volatile backing=storage;int calls;\
        __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){++calls;return backing;}";
    for (wrapper, known, calls) in [
        ("static inline void*f(int c){if(c)return allocate(11);return backing;}", 11, 1),
        ("static inline void*f(int c){if(c)return 0;return allocate(7);}", u64::MAX, 1),
        ("static inline void*f(int c){if(c){void*p=allocate(11);return p;}return backing;}", 11, 1),
        ("static inline void*f(int c){if(c){void*volatile p=allocate(11);return p;}return backing;}", u64::MAX, 1),
    ] {
        // Each call needs its own capture; the unknown arm must overwrite a
        // previous known result. Query operands themselves never execute.
        let unknown = if wrapper.contains("return allocate(7)") { "7" } else { "~0ul" };
        assert_constant_program(&format!(
            "{declarations}{wrapper}int main(void){{volatile int c=1;void*a=f(c);\
             c=0;void*b=f(c);if(__builtin_dynamic_object_size(a,1)!={known}ul||\
             __builtin_dynamic_object_size(b,1)!={unknown}||calls!={calls})return 1;\
             if(__builtin_dynamic_object_size(f(c++),1)!=~0ul||c)return 2;\
             return __builtin_object_size(a,1)!=~0ul;}}"
        ));
    }
    assert_constant_program(&format!(
        "{declarations}struct S{{char a[4];char b[8];}};static struct S object;\
         static inline char*f(int c){{if(c)return object.b+2;return allocate(7);}}\
         int main(void){{volatile int c=1;char*p=f(c);c=0;char*q=f(c);\
         return __builtin_dynamic_object_size(p,0)!=6||__builtin_dynamic_object_size(p,1)!=6||\
         __builtin_dynamic_object_size(q,2)!=7||__builtin_dynamic_object_size(q,3)!=7||calls!=1;}}"
    ));
}

#[test]
fn checked_constant_stores_feed_inline_allocation_queries_without_losing_the_store() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/checked-allocation-constant-flow.c"
    ));
    let declarations =
        "typedef unsigned long size_t;char storage[128];char*volatile backing=storage;int calls;\
        __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){++calls;return backing;}\
        static inline void*array(size_t count,size_t size){size_t bytes;\
        if(__builtin_mul_overflow(count,size,&bytes))return 0;return allocate(bytes);}";
    for n in [0, 1, 11, 128] {
        assert_constant_program(&format!(
            "{declarations}int main(void){{void*p=array(1,{n});\
             return __builtin_object_size(p,1)!={n}||__builtin_dynamic_object_size(p,1)!={n}||calls!=1;}}"
        ));
    }
    let wrapped = declarations.replace(
        "static inline void*array",
        "static inline _Bool checked(_Bool overflow){return __builtin_expect(overflow,0);}static inline void*array",
    ).replace("if(__builtin_mul_overflow(count,size,&bytes))", "if(checked(__builtin_mul_overflow(count,size,&bytes)))");
    assert_constant_program(&format!(
        "{wrapped}int main(void){{void*p=array(1,11);return __builtin_object_size(p,1)!=11||calls!=1;}}"
    ));
    for source in [
        "int calls;static inline int f(void){++calls;return 0;}int main(void){if(f())return 2;return calls!=1;}",
        "static int*out;void change(void){*out=7;}int main(void){int n;out=&n;if(__builtin_mul_overflow(3,4,&n))return 1;change();return n!=7;}",
        "int main(void){int n;if(__builtin_add_overflow(1,2,&n))return 1;n=7;return n!=7;}",
        "int main(void){int n=8;volatile int c=0;if(c&&__builtin_add_overflow(1,2,&n))return 1;return n!=8;}",
        "int main(void){int n=8;if(__builtin_constant_p(__builtin_add_overflow(1,2,&n)))return 1;return n!=8;}",
        "int main(void){int n=8;goto chosen;if(__builtin_mul_overflow(3,4,&n)){chosen:return n!=8;}return 2;}",
    ] { assert_constant_program(source); }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(
            "int main(void){if(__builtin_mul_overflow(2,3,(int*)0))return 1;return 0;}",
            mode,
        );
        assert!(
            !output.status.success(),
            "{mode} removed a required invalid store"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3009"));
    }
}

#[test]
fn constant_byte_queries_use_initialized_objects_without_broadening_source_ice() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/constant-query-object-bytes.c"
    ));
}

#[test]
fn checked_allocation_facts_cross_nested_boolean_expectations() {
    assert_constant_program(
        "typedef unsigned long size_t;char storage[256];int calls;\
         __attribute__((noinline,alloc_size(2))) void*allocate(void*d,size_t n,unsigned f){++calls;return storage;}\
         static inline _Bool checked(_Bool overflow){return __builtin_expect(!!overflow,0);}\
         static inline void*array(void*d,size_t n,size_t s,unsigned f){size_t bytes;\
           if(__builtin_expect(!!checked(__builtin_mul_overflow(n,s,&bytes)),0))return (void*)0;\
           return allocate(d,bytes,f);}\
         static inline void*zero(void*d,size_t n,size_t s,unsigned f){return array(d,n,s,f|256);}\
         int main(void){void*a=array(0,1,11,3);void*b=zero(0,128,1,3);void*c=array(0,1,0,3);\
           return __builtin_object_size(a,1)!=11||__builtin_object_size(b,1)!=128||\
             __builtin_object_size(c,1)!=0||calls!=3;}",
    );
}

#[test]
fn initialized_byte_queries_survive_unrelated_volatile_location_stores() {
    assert_constant_program(
        "struct location{const char*file;int line;};struct test{struct location last;};\
         int f(struct test*t){char a[80]=\"Test!\";\
           *(const char*volatile*)&t->last.file=\"here\";*(volatile int*)&t->last.line=7;\
           return !__builtin_constant_p(a[0])||!__builtin_constant_p(a[79]);}\
         int main(void){struct test t={0};return f(&t)||t.last.line!=7;}",
    );
    assert_constant_program(include_str!(
        "../../../tests/fixtures/fortify-query-context.c"
    ));
    for source in [
        "int f(char*p){char a[]=\"abc\";p=a;volatile char z='z';*p=z;return __builtin_constant_p(a[0])||a[0]!='z';}int main(void){return f(0);}",
        "int f(char*p){char a[]=\"abc\";char**q=&p;*q=a;volatile char z='z';*p=z;return __builtin_constant_p(a[0])||a[0]!='z';}int main(void){return f(0);}",
        "struct box{char*p;};int f(struct box*b){char a[]=\"abc\";b->p=a;volatile char z='z';*b->p=z;return __builtin_constant_p(a[0])||a[0]!='z';}int main(void){struct box b={0};return f(&b);}",
        "typedef unsigned long size_t;char storage[20];__attribute__((noinline,alloc_size(1)))void*allocate(size_t n){return storage;}static inline void*array(void){size_t n;if(__builtin_expect(!!__builtin_mul_overflow(1,11,&n),(n=7)))return 0;return allocate(n);}int main(void){void*p=array();return __builtin_dynamic_object_size(p,1)!=7;}",
    ] { assert_constant_program(source); }
}

#[test]
fn static_extent_uses_the_proved_size_at_the_allocation_not_a_later_assignment() {
    assert_constant_program(
        "typedef unsigned long size_t;char storage[128];\
         __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){return storage;}\
         int main(void){size_t n;n=11;void*p=allocate(n);n=7;\
         return __builtin_object_size(p,1)!=11||__builtin_dynamic_object_size(p,1)!=11||n!=7;}",
    );
    assert_constant_program(
        "typedef unsigned long size_t;char storage[128];volatile size_t zero;\
         __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){return storage;}\
         int main(void){size_t n;n=11;if(!__builtin_constant_p(1))n+=zero;\
         void*p=allocate(n);return __builtin_object_size(p,1)!=11;}",
    );
    for source in [
        "int main(void){int n=11;goto selected;if(0){selected:n=7;}return n!=7;}",
        "int main(void){int n=11;switch(1){if(0){case 1:n=7;}}return n!=7;}",
        "int main(void){int n=11;volatile int yes=1;if(yes)n=7;return n!=7;}",
        "int main(void){int n=0;if(n++)return 9;return n!=1;}",
    ] {
        assert_constant_program(source);
    }
}

#[test]
fn unrelated_volatile_stores_preserve_unescaped_allocation_size_facts() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/allocation-static-size-flow.c"
    ));
    let declarations = "typedef unsigned long size_t;char storage[128];\
        char*volatile backing=storage;volatile size_t zero;\
        __attribute__((noinline,alloc_size(1))) void*allocate(size_t n){return backing;}";
    for body in [
        // An escaped scalar may be modified through the unrelated-looking
        // address; neither its subsequent use nor a query can keep old facts.
        "int main(void){size_t n;n=11;size_t*p=&n;*(volatile size_t*)p=7;\
         void*q=allocate(n);return n!=7||__builtin_dynamic_object_size(q,1)!=7;}",
        // Volatile sizes remain runtime values even when initialized by a
        // literal. An independent volatile store must still execute.
        "int main(void){volatile size_t n=11;size_t marker=0;\
         void*p=allocate(n);*(volatile size_t*)&marker=9;\
         return marker!=9||__builtin_object_size(p,1)!=~0ul||\
         __builtin_dynamic_object_size(p,1)!=11;}",
        // Real source writes retain their one-time effects and invalidate the
        // old scalar fact while leaving an earlier allocation capture intact.
        "int main(void){size_t n;n=11;if(!__builtin_constant_p(1))n+=zero;\
         void*p=allocate(n);size_t*q=&n;*(volatile size_t*)q=7;\
         return n!=7||__builtin_dynamic_object_size(p,1)!=11;}",
    ] {
        assert_constant_program(&format!("{declarations}{body}"));
    }
}

#[test]
fn assembly_memory_outputs_expose_objects_to_later_indirect_writes() {
    for (constraint, expected) in [
        ("=m", 0),
        ("+m", 0),
        ("=Q", 0),
        ("+Q", 0),
        ("=r", 1),
        ("+r", 1),
        ("=&r", 1),
        ("+&r", 1),
    ] {
        let source = format!(
            "int f(volatile int*p){{int n=0;asm volatile(\"\":\"{constraint}\"(n));\
             n=11;*p=7;return __builtin_constant_p(n);}}"
        );
        let output = run(&source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{constraint}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Reached assembly is not interpreter-executable. Inspect the actual
        // returned SSA constant after independently parsing/validating the IR,
        // rather than counting an unrelated constant elsewhere in the text.
        let module = evocc_evoir::parse_section9_module(&output.stdout).unwrap();
        let [function] = module.functions.as_slice() else {
            panic!("one source function expected");
        };
        let [block] = function.blocks.as_slice() else {
            panic!("straight-line function expected");
        };
        let evocc_evoir::Inst::Ret(values) = &block.terminator else {
            panic!("return expected");
        };
        let value = values.first().expect("integer return value");
        assert!(
            block.insts.iter().any(|inst| matches!(inst,
                evocc_evoir::Inst::Const(result, bits) if result == value && *bits == expected)),
            "{constraint}: returned query must be {expected}"
        );
    }
    // The exposure fix must not silently admit new early-clobber memory
    // combinations; those constraints remain outside the current profile.
    for constraint in ["=&m", "+&m", "=&Q", "+&Q"] {
        let source = format!("void f(void){{int n=0;asm volatile(\"\":\"{constraint}\"(n));}}");
        let output = run(&source, "--emit=evoir");
        assert!(!output.status.success(), "{constraint}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3021"));
    }
}

#[test]
fn extent_queries_do_not_read_unknown_values_or_escape_source_control_flow() {
    for source in [
        "int main(void){char a[16];int i;return __builtin_dynamic_object_size(a+i,0)!=~0ul;}",
        "int main(void){char *p;return __builtin_dynamic_object_size(p,1)!=~0ul;}",
        "int main(void){char a[16];int i=0;unsigned long n=__builtin_dynamic_object_size(a+i++,1);return n!=~0ul||i;}",
        "int main(void){char a[16];char*p;goto after; p=a; after:return __builtin_dynamic_object_size(p,1)!=~0ul;}",
        "int calls;char *allocate(void){++calls;return 0;}int main(void){return __builtin_dynamic_object_size(allocate(),1)!=~0ul||calls;}",
        "int main(void){char a[16];char*p=a;unsigned long n=__builtin_dynamic_object_size(p++,3);return n||p!=a;}",
    ] { assert_constant_program(source); }
}

#[test]
fn object_extents_follow_substituted_parameters_and_captured_array_bounds() {
    for source in [
        "static __attribute__((always_inline)) inline unsigned long extent(char*p){return __builtin_dynamic_object_size(p,1);}int main(void){char a[17];return extent(a)!=17;}",
        "__attribute__((noinline)) int f(unsigned long n){char a[13];typedef char V[n];V*p=(V*)&a;n=1;return __builtin_dynamic_object_size(*p,1)!=13;}int main(void){return f(13);}",
        "int main(void){char a[16];char*p=a+4;char*q=p-2;return __builtin_dynamic_object_size(q,0)!=14;}",
        "int main(void){char a[16];return __builtin_dynamic_object_size(a+16,0)||__builtin_dynamic_object_size(a+17,1)||__builtin_dynamic_object_size(a-1,2);}",
    ] { assert_constant_program(source); }
}

#[test]
fn dynamic_object_queries_reject_invalid_modes() {
    for mode in ["4", "-1", "mode"] {
        let source = format!(
            "unsigned long f(char *p,int mode){{return __builtin_dynamic_object_size(p,{mode});}}"
        );
        let output = run(&source, "--emit=evoir");
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2925"));
    }
}

#[test]
fn extent_capture_accepts_pointer_conditions_and_loaded_integer_sizes() {
    for source in [
        include_str!("../../../tests/fixtures/extent-capture-conditions.c"),
        "int main(void){char a[4]={1},b[9]={2};void*p=a;char*q=p?a:b;return q[0]!=1||__builtin_dynamic_object_size(q,0)!=4;}",
        "int calls;int*choose(void){++calls;return &calls;}int main(void){char a[4],b[9];char*q=choose()?a:b;return calls!=1||__builtin_dynamic_object_size(q,0)!=4;}",
        "int main(void){char a[4],b[9];void*p=a;char*q=(p&&p)?a:b;return __builtin_dynamic_object_size(q,0)!=4;}",
        "char storage[64];char*volatile base=storage;__attribute__((noinline,alloc_size(1))) void*allocate(unsigned long n){return base;}int main(void){unsigned long n=13;char*p=allocate(*&n);n=1;return __builtin_dynamic_object_size(p,0)!=13;}",
        "char storage[64];char*volatile base=storage;__attribute__((noinline,alloc_size(1))) void*allocate(unsigned char n){return base;}int main(void){unsigned long n=257;char*p=allocate(n);return __builtin_dynamic_object_size(p,0)!=1;}",
    ] { assert_constant_program(source); }
}

#[test]
fn private_values_before_escape_discharge_build_assertions() {
    let source = include_str!("../../../tests/fixtures/private-constant-before-escape.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn immutable_global_loads_discharge_conditions_after_inline_pointer_selection() {
    for source in [
        include_str!("../../../tests/fixtures/immutable-table-conditions.c"),
        "void invalid(void); int main(void){if(\"message\\n\"[sizeof(\"message\\n\")-2]!='\\n')invalid();return 0;}",
        "struct S{int head;unsigned short value;}; static const struct S table[]={{1,262},{2,312}}; static __attribute__((always_inline)) inline const struct S *pick(unsigned char n){if(n<2)return &table[n];return 0;} void invalid(void); int main(void){if(pick(0)->value!=262||pick(1)->value!=312)invalid();return 0;}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"), "immutable load was not resolved: {source}");
        assert!(String::from_utf8_lossy(&output.stdout).contains(" = load "));
        assert_constant_program(source);
    }
}

#[test]
fn immutable_global_facts_do_not_cover_writes_volatile_reads_or_pointer_relocations() {
    for source in [
        "static int table[]={1,2};int main(void){table[0]=7;return table[0]!=7;}",
        "static int table[]={1,2};void change(int*p){*p=7;}int main(void){change(table);return table[0]!=7;}",
        "static int n; static int * const ptr=&n;int main(void){*ptr=7;return *ptr!=7;}",
    ] { assert_constant_program(source); }
    let output = run("static const volatile int table[]={1,2};void invalid(void);void f(void){if(table[0]!=1)invalid();}", "--emit=evoir");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    let output = run("static int n;static int * const p=&n;void invalid(void);void f(void){if(*(unsigned long*)&p==0)invalid();}", "--emit=evoir");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    assert_constant_program("static const int a[]={1,2};int f(int n){const int*p=n?a:a+1;return *p;}int main(void){return f(0)!=2||f(1)!=1;}");
    let output = run(
        "static const int a[]={1,2};int main(void){return a[2];}",
        "--interpret-evoir",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("E3011"));
}

#[test]
fn constant_queries_follow_sequential_writes_and_finite_initializer_values() {
    let source = include_str!("../../../tests/fixtures/sequential-constant-queries.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    assert_constant_program(source);
}

#[test]
fn finite_query_facts_preserve_unknown_values_mutations_and_evaluation_counts() {
    for source in [
        "__attribute__((noinline)) int f(int c){int n=c?-1:255;return __builtin_constant_p(n>=0);}int main(void){return f(0)||f(1);}",
        "int main(void){volatile int n=16;return __builtin_constant_p(n>=0);}",
        "int main(void){goto next;int n=16;next:return __builtin_constant_p(n>=0);}",
        "int calls;__attribute__((noinline)) int f(void){++calls;return calls;}int main(void){int n=f()?16:255;int p=__builtin_constant_p((long long)n>=0);return !p||calls!=1||n!=16;}",
        "int main(void){int calls=0;int n=16;int p=__builtin_constant_p((calls++,n>=0));return p||calls;}",
        "__attribute__((noinline)) int f(int c){int n;n=4;if(c)n=7;return n;}int main(void){return f(0)!=4||f(1)!=7;}",
        "int *saved;void escape(int*p){saved=p;}void change(void){*saved=7;}int main(void){int n;n=4;escape(&n);n=6;change();return n!=7;}",
        "int main(void){int n;n=4;for(int i=0;i<3;i++)n+=i;return n!=7;}",
        "__attribute__((noinline)) int f(int c){unsigned char n=c?256:257;return __builtin_constant_p(n>=0);}int main(void){return !f(0)||!f(1);}",
        "__attribute__((noinline)) int f(int c){unsigned char n=c?256:257;return __builtin_constant_p(n>=1);}int main(void){return f(0)||f(1);}",
        "__attribute__((noinline)) int f(int c){signed char n=c?127:128;return __builtin_constant_p(n>=0);}int main(void){return f(0)||f(1);}",
    ] { assert_constant_program(source); }
}

#[test]
fn normalized_initializers_share_nested_declaration_identities_with_the_written_form() {
    for source in [
        include_str!("../../../tests/fixtures/nested-initializer-identity.c"),
        "int main(void){int n=1;unsigned long a[]={({int x=n++;x;}),({int y=n++;y;})};return a[0]!=1||a[1]!=2||n!=3;}",
        "int main(void){int n=1;struct S{int x;}a[]={ (struct S){({int x=n++;x;})},(struct S){({int y=n++;y;})} };return a[0].x!=1||a[1].x!=2||n!=3;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{source}: {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n", "{mode}");
        }
    }
}

#[test]
fn sequential_constants_stop_at_mutation_escape_and_external_entry() {
    for source in [
        "__attribute__((noinline))void replace(int*p){*p=7;}int main(void){int x=32;if(x!=32)return 1;replace(&x);return x!=7;}",
        "int main(void){int x=32;int*p=&x;*p=7;return x!=7;}",
        "int main(void){int x=32;for(int i=0;i<2;i++)x++;return x!=34;}",
        "int f(int c){int x=32;if(c)x=7;return x;}int main(void){return f(0)!=32||f(1)!=7;}",
        "int main(void){int x=32;goto after;again:x=7;after:if(x==32)goto again;return x!=7;}",
        "int main(void){int x=32;while(x<34){int y=x;if(y!=x)return 1;x++;}return x!=34;}",
        "int main(void){int x=32;unsigned long size=__builtin_object_size(&x,0);int*p=&x;*p=7;return size!=4||x!=7;}",
        "int main(void){int x=32;int constant=__builtin_constant_p(&x);int*p=&x;*p=7;return constant||x!=7;}",
    ] { assert_constant_program(source); }
}

#[test]
fn numeric_pointer_bits_fold_in_execution_but_not_as_source_ice() {
    let source = "static __attribute__((always_inline))inline _Bool bad(const void*p){return __builtin_expect(!!((unsigned long)(void*)p>=(unsigned long)-4095),0);}void invalid(void);int main(void){if(bad((void*)0xffff0123456789abUL))invalid();return !bad((void*)-1L)||bad((void*)0);}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    let output = run(
        "_Static_assert((unsigned long)(void*)1 == 1,\"not an ICE\");",
        "--parse-only",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("E2912"));
}

#[test]
fn expectation_specialization_preserves_second_operand_failures() {
    for (source, expected_error) in [
        (
            "int main(void){return __builtin_expect(1,1/0);}",
            None,
        ),
        (
            "int main(void){const long value=1;return __builtin_expect(value,1/0);}",
            Some("E3002"),
        ),
        (
            "int main(void){const long value=1;int uninitialized;return __builtin_expect(value,uninitialized);}",
            Some("E3001"),
        ),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            if let Some(code) = expected_error {
                assert!(!output.status.success(), "{mode}: {source}");
                assert!(
                    String::from_utf8_lossy(&output.stderr).contains(code),
                    "{mode}: {source}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            } else {
                assert!(output.status.success(), "{mode}: {source}");
                assert_eq!(output.stdout, b"1\n");
            }
        }
    }
}

#[test]
fn static_numeric_pointer_comparisons_use_target_address_bits() {
    let source = include_str!("../../../tests/fixtures/static-numeric-pointer-comparisons.c");
    assert_constant_program(source);
    let output = run(source, "--emit=asm");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn static_numeric_pointer_folding_does_not_read_objects_or_admit_source_ices() {
    for (source, code) in [
        ("void *p; static int x = p < (void *)1;", "E2208"),
        ("void *f(void); static int x = f() < (void *)1;", "E2208"),
        (
            "_Static_assert((void *)1 < (void *)2, \"not an ICE\");",
            "E2912",
        ),
    ] {
        let output = run(source, "--parse-only");
        assert!(!output.status.success(), "{source}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(code),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn inline_constant_queries_are_recomputed_after_parameter_substitution() {
    let source = include_str!("../../../tests/fixtures/inline-constant-query.c");
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_required"));
}

#[test]
fn return_address_extraction_is_an_evaluated_identity_not_an_external_call() {
    let source = include_str!("../../../tests/fixtures/extract-return-address.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("__builtin_extract_return_addr"));
}

#[test]
fn constant_inline_returns_do_not_copy_unselected_local_declarations() {
    for expression in [
        "0 && ({ int local = 7; local; })",
        "1 || ({ int local = 7; local; })",
    ] {
        let expected = u8::from(expression.starts_with('1'));
        let source = format!("static inline int config(void){{return {expression};}} int main(void){{return config() != {expected};}}");
        let output = run(&source, "--interpret-evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n");
    }
}

#[test]
fn immutable_static_values_prune_false_error_calls_without_becoming_ices() {
    let source = include_str!("../../../tests/fixtures/static-constant-branch.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    let source = "int main(void){static const int value=7; return !__builtin_constant_p(value);}";
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn static_value_propagation_preserves_escapes_volatile_reads_and_updates() {
    for source in [
        "int main(void){static const int value=7;const int *p=&value;return *p!=7;}",
        "int main(void){static const volatile int value=7;return value!=7;}",
        "static int f(void){static int value=5; ++value;return value;}int main(void){static const int expected=7;f();return f()!=expected;}",
        "int main(void){static const int value=7; int a=0; if(a) return 2; static const int later=8; return value+later!=15;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{source}: {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n", "{source}: {mode}");
        }
    }
    let output = run(
        "int main(void){static const volatile int value=7;return value;}",
        "--emit=evoir",
    );
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("load volatile"));
}

#[test]
fn inline_constant_pointer_results_prune_unreachable_external_calls() {
    let source = r#"
        struct area;
        extern unsigned long absent_size(const struct area *);
        static inline struct area *get_area(void *dev) { return (struct area *)0; }
        int available(void) {
            struct area *p = get_area((void *)0);
            if (!p) return 0;
            return absent_size(p) != 0;
        }
        int main(void) { return available(); }
    "#;
    let assigned = source.replace("struct area *p =", "struct area *p; p =");
    for source in [source, assigned.as_str()] {
        let output = run(source, "--emit=asm");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("absent_size"));
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(
                output.status.success(),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, b"0\n");
        }
    }
    // The literal-call shortcut must not drop argument evaluation, volatile
    // parameter initialization, or a different result on another input.
    for source in [
        "static inline void*f(int x){return (void*)0;}int main(void){int n=0;void*p=f(++n);return p!=0||n!=1;}",
        "static inline void*f(int x){return (void*)4096;}int main(void){return f(0)!=(void*)4096;}",
        "static int object;static inline int*f(int *p){return p;}int main(void){return f(&object)!=&object||f(0)!=0;}",
        "static inline void*f(volatile int x){return (void*)0;}int main(void){return f(0)!=0;}",
        "int main(void){int x;int*p;p=0;if(p)return 1;p=&x;return p!=&x;}",
        "__attribute__((noinline)) void change(int**p,int*q){*p=q;}int main(void){int x;int*p;p=0;change(&p,&x);return p!=&x;}",
        "int main(void){volatile int c=1;int x;int*p;p=0;if(c)p=&x;return p!=&x;}",
        "int main(void){int x;int*p=&x;goto done;p=0;done:return p!=&x;}",
        "int main(void){int*volatile p;p=0;return p!=0;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(output.status.success(), "{source}: {mode}: {}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n", "{source}: {mode}");
        }
    }
}

#[test]
fn constant_inline_leaf_results_remove_false_error_calls() {
    let source = include_str!("../../../tests/fixtures/constant-call-results.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn common_private_parameter_values_preserve_calls_and_effects() {
    let source = include_str!("../../../tests/fixtures/private-constant-parameter.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_failed"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @map"));
    let output = run(source, "--interpret-evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"0\n");
}

#[test]
fn parameter_facts_do_not_escape_their_proven_callers_or_replace_effects() {
    for source in [
        "static int f(int x){return x;}int main(void){return f(1)+f(2)!=3;}",
        "static int f(int x){return x;}int main(void){int (*p)(int)=f;return f(1)+p(2)!=3;}",
        "static int f(int x){int *p=&x;*p=4;return x;}int main(void){return f(1)!=4;}",
        "static int f(int x){++x;return x;}int main(void){return f(1)!=2;}",
        "static int f(int x){return __builtin_constant_p(x);}int main(void){return !f(1);}",
        "static __attribute__((always_inline)) inline int f(signed char x){return x;}int main(void){return f(255)!=-1;}",
        "static __attribute__((always_inline)) inline int f(int x){return x+x;}int main(void){int x=1;int y=f(x++);return x!=2||y!=2;}",
    ] {
        let output = run(source, "--interpret-evoir");
        assert!(output.status.success(), "{source}: {}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(output.stdout, b"0\n", "{source}");
    }
    for source in [
        "static __attribute__((always_inline)) inline int f(volatile int x){return x;}int main(void){return f(7);}",
        "static int f(volatile int x){return x;}int main(void){return f(7);}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("load volatile"));
    }
}

#[test]
fn constant_queries_are_ices_in_static_initializers() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/constant-query-initializers.c"
    ));
}

#[test]
fn constant_queries_are_ices_in_case_labels() {
    assert_constant_program(
        "int fallback(int x); int main(void){switch(32){case __builtin_constant_p(2)?2<<4:fallback(2):return 0;default:return 1;}}",
    );
}

#[test]
fn constant_queries_select_static_relocations_without_evaluating_the_other_arm() {
    assert_constant_program(
        "int fallback(int x); static int object=7;static int *p=(__builtin_constant_p(2)?2:fallback(2))?&object:0;int main(void){return *p!=7;}",
    );
}

fn assert_constant_program(source: &str) {
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"0\n", "{mode}");
    }
}

#[test]
fn small_inline_helpers_specialize_distinct_constant_arguments() {
    let source = "extern void invalid_size(void) __attribute__((error(\"size\")));static inline void check(unsigned long n){if(n>64)invalid_size();}int main(void){check(16);check(48);return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_size"));
    assert_constant_program(source);
}

#[test]
fn numeric_pointer_conditions_fold_with_unsigned_pointer_width() {
    let source = "extern void invalid_sign(void) __attribute__((error(\"sign\")));int main(void){if((unsigned char *)-1 < (unsigned char *)1)invalid_sign();if(!((void *)0<(void *)1))invalid_sign();return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_sign"));
}

#[test]
fn optional_inlining_preserves_recursive_and_noinline_calls() {
    for source in [
        "static inline int f(int n){return n?f(n-1)+1:0;}int main(void){return f(3)!=3;}",
        "static __attribute__((noinline)) inline int f(int n){return n+1;}int main(void){return f(3)!=4;}",
        "static inline int f(void){static int n;return ++n;}int main(void){f();return f()!=2;}",
        "static inline int f(int n){return n+n;}int main(void){int n=2;int r=f(n++);return r!=4||n!=3;}",
    ] {
        assert_constant_program(source);
    }
    let output=run("static __attribute__((noinline)) inline int f(int n){return n+1;}int main(void){return f(3);}","--emit=evoir");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("call @f"));
}

#[test]
fn nested_numeric_pointer_conditions_keep_their_arithmetic_result() {
    let source = "extern void invalid(void) __attribute__((error(\"sign\")));int main(void){if(!((((char *)-1<(char *)1)?2:1)&(((int *)-1<(int *)1)?2:1)))invalid();return 0;}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn immutable_automatic_constants_resolve_optimization_time_queries() {
    let source = "extern void invalid(void) __attribute__((error(\"value\")));static int f(void){const unsigned long last=41;if(last+1!=42)invalid();return __builtin_constant_p(last);}int main(void){return !f();}";
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    assert_constant_program(source);
    for source in [
        "static int f(void){int x=1;++x;return x!=2;}int main(void){return f();}",
        "static int f(void){int x=1;int *p=&x;*p=3;return x!=3;}int main(void){return f();}",
        "static int f(int c){const int a=({if(c)goto end; 1;});int b=2;end:b=4;return b!=4;}int main(void){return f(1);}",
    ] { assert_constant_program(source); }
}

#[test]
fn optional_inline_hints_preserve_address_taken_label_identity() {
    let source = "static inline int f(int n){if(n)return 0;here:return (unsigned long)&&here!=0;}int main(void){return f(1);}";
    assert_constant_program(source);
}

#[test]
fn optimization_constant_queries_resolve_local_values_in_both_interpreters() {
    let source = include_str!("../../../tests/fixtures/optimization-constant-query.c");
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(source, mode);
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"1\n", "{mode}");
    }
}

#[test]
fn optimization_queries_follow_proved_compound_assignments() {
    let source = include_str!("../../../tests/fixtures/bitmap-constant-mutation.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_assertion_failed"));
}

#[test]
fn optimization_queries_follow_private_memory_through_inline_helpers() {
    let source = include_str!("../../../tests/fixtures/bitmap-constant-memory.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @bitmap_constant_required"));
}

#[test]
fn optimization_queries_select_constant_memory_read_paths() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/bitmap-constant-selection.c"
    ));
}

#[test]
fn private_memory_facts_do_not_cross_unknown_writes_or_conditional_execution() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/bitmap-memory-boundaries.c"
    ));
    for source in [
        "static inline __attribute__((always_inline)) void set(unsigned long*p){*p=7;} int main(void){unsigned long a[2]; a[0]=1;a[1]=2;set(a+1);int r=a[0]+a[1];return !__builtin_constant_p(r)||r!=8;}",
        "__attribute__((noinline)) void change(unsigned long*p){*p=9;} int main(void){unsigned long a[1];a[0]=3;change(a);return a[0]!=9;}",
        "int f(int x){unsigned long a[1];a[0]=3;if(x)a[0]=8;return a[0];} int main(void){return f(0)!=3||f(1)!=8;}",
        "int main(void){unsigned long a[1];a[0]=3;volatile int c=0;c?({a[0]=8;0;}):({if(a[0]!=3)return 1;0;});return a[0]!=3;}",
        "int main(void){unsigned long a[1];a[0]=3;volatile int c=0;c&&({a[0]=8;0;});return a[0]!=3;}",
        "int main(void){unsigned long a[1];a[0]=3;unsigned char*p=(unsigned char*)a;*p=9;return a[0]!=9;}",
        "int main(void){unsigned long a[1];a[0]=3;volatile unsigned long*p=a;*p=9;return a[0]!=9;}",
        "int main(void){unsigned long a[1];a[0]=3;int k=__builtin_constant_p((a[0]=9));return k||a[0]!=3;}",
        "int main(void){unsigned long a[1];a[0]=3;int k=__builtin_constant_p(&a[0]);return k||a[0]!=3;}",
        "static inline __attribute__((always_inline)) void f(unsigned long*p,int c){if(c)return;*p=8;} int main(void){unsigned long a[1];a[0]=3;volatile int c=1;f(a,c);return a[0]!=3;}",
        "int main(void){unsigned long a[1];a[0]=3;for(int i=0;i<4;i++)a[0]+=i;return a[0]!=9;}",
        "int main(void){unsigned long a[1];a[0]=3;goto end;a[0]=9;end:return a[0]!=3;}",
        "int main(void){unsigned long a[1];a[0]=3;do{a[0]|=8;}while(0);int x=a[0];return !__builtin_constant_p(x)||x!=11;}",
        "int main(void){unsigned long a[1];a[0]=3;return __builtin_constant_p((unsigned int)a!=0);}",
        "void cleanup(unsigned long **p){**p=9;} int main(void){unsigned long a[1];{unsigned long *p __attribute__((cleanup(cleanup)))=a;a[0]=3;}return a[0]!=9;}",
        "static inline __attribute__((always_inline)) int f(unsigned long*p,int c){if(c)return *p;*p=8;return 9;} int main(void){unsigned long a[1];a[0]=3;int x=f(a,1);return !__builtin_constant_p(x)||x!=3||a[0]!=3;}",
    ] { assert_constant_program(source); }
}

#[test]
fn private_memory_proofs_keep_uninitialized_and_bounds_checks() {
    for (source, code) in [
        (
            "int main(void){unsigned long a[1];a[0]&=0;return a[0];}",
            "E3001",
        ),
        (
            "int main(void){int a[1];a[0]=2147483647;a[0]+=1;return a[0];}",
            "E3004",
        ),
        (
            "int main(void){unsigned long a[2];a[0]=3;return a[1];}",
            "E3001",
        ),
        (
            "int main(void){unsigned long a[1];a[0]=3;return a[1];}",
            "E3011",
        ),
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(!output.status.success(), "{source} {mode}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(code),
                "{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn compound_query_facts_preserve_conversion_effects_and_unknown_paths() {
    assert_constant_program(
        r#"
        int main(void) {
            unsigned char x=250;
            x+=10; if (!__builtin_constant_p(x) || x!=4) return 1;
            x*=3; x-=2; x/=2; x%=4;
            x<<=3; x>>=1; x|=2; x^=1; x&=6;
            if (!__builtin_constant_p(x) || x!=6) return 2;
            volatile int flag=1;
            if(flag) x+=3;
            if(x!=9) return 3;
            unsigned y=1; unsigned *p=&y;
            y|=4; *p=19;
            if(y!=19) return 4;
            unsigned z=1;
            if((++z, 0)) z=90;
            if(z!=2) return 5;
            volatile unsigned v=1; v|=4;
            if(v!=5 || __builtin_constant_p(v)) return 6;
            return 0;
        }
    "#,
    );
    assert_constant_program(
        r#"
        int main(void) {
            unsigned n=1;
            goto enter;
            n=5;
        enter:
            n|=2;
            return n!=3;
        }
    "#,
    );
}

#[test]
fn optimization_queries_preserve_effects_and_resolve_kernel_request_values() {
    let source = include_str!("../../../tests/fixtures/optimization-query-effects.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @constant_required"));
}

#[test]
fn query_facts_do_not_initialize_objects_across_incoming_jumps() {
    let source = "int main(void){goto next;{int x=7;next:return __builtin_constant_p(x);}}";
    assert_constant_program(source);
}

#[test]
fn exhaustive_masked_switches_do_not_reference_an_impossible_default() {
    let source = "extern void impossible(void);int f(unsigned x){switch(x&1){case 0:return 7;case 1:return 9;default:impossible();return 3;}}int main(void){return f(0)!=7||f(1)!=9||f(2)!=7||f(~0u)!=9;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @impossible"));
}

#[test]
fn masked_switch_folding_preserves_effects_fallthrough_and_uncovered_defaults() {
    for source in [
        "int f(unsigned x){switch(x&3){case 0:return 7;case 1:return 9;default:return 11;}}int main(void){return f(2)!=11||f(3)!=11;}",
        "int main(void){int x=2;int y=0;switch(x++&1){case 0:y=1;default:y+=2;break;case 1:y=7;}return x!=3||y!=3;}",
        "int f(unsigned x){switch(x&3){case 0 ... 2:return 7;case 3:return 9;default:return 4;}}int main(void){return f(2)!=7||f(3)!=9;}",
        "int main(void){int y=0;goto in;switch(y&1){case 0:return 3;case 1:return 4;default:in:return 0;}}",
    ] { assert_constant_program(source); }
}

#[test]
fn fresh_compound_literal_representation_reads_expose_layout_checks() {
    let source = include_str!("../../../tests/fixtures/constant-representation-reads.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid_layout"));
}

#[test]
fn representation_facts_do_not_invent_padding_or_ignore_volatile_and_runtime_values() {
    for source in [
        "void observe(void);struct p{char a;int b;};int f(void){if(((unsigned char*)&(struct p){.a=3})[1]==0)observe();return 0;}",
        "void observe(void);struct p{char a;};int f(void){if(((volatile unsigned char*)&(struct p){.a=3})[0]==3)observe();return 0;}",
        "void observe(void);struct p{char a;};int f(int x){if(((unsigned char*)&(struct p){.a=x})[0]==3)observe();return 0;}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("call @observe"));
    }
    assert_constant_program("struct p{char a;};int main(void){int n=0;int a=((unsigned char*)&(struct p){.a=++n})[0];return n!=1||a!=1;}");
}

#[test]
fn constant_facts_cross_internal_inline_exits_but_not_source_jumps() {
    let source = "void invalid(void);struct s{unsigned n;};static inline struct s*get(struct s*p){return p;}unsigned f(struct s*p){return({__auto_type a=get(p)->n;__auto_type b=2;if(!__builtin_constant_p(b))invalid();a<b?a:b;});}int main(void){struct s s={5};return f(&s)!=2;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn constant_scalar_literals_and_same_object_address_relations_prune_branches() {
    let source = "void invalid(void);struct list{void*next;void*prev;};static struct list node;int main(void){if((int){0}!=0||(int){1}!=1)invalid();if((struct list*)&node.prev<=&node||(struct list*)&node.prev>=&node+1)invalid();return 0;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn finite_conditional_results_prove_bounds_without_skipping_calls() {
    let source = "void invalid(void);static int calls;int choose(int n){++calls;return n;}int f(int n){if(58<((choose(n)?52:48)-12))invalid();return 0;}int main(void){f(0);f(1);return calls!=2;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn finite_facts_preserve_mixed_outcomes_conversions_and_runtime_errors() {
    for source in [
        "int f(int c){return (c?52:48)>50;}int main(void){return f(0)!=0||f(1)!=1;}",
        "int f(int c){return (unsigned char)(c?256:257)>0;}int main(void){return f(0)!=1||f(1)!=0;}",
        "int f(int c){return (c?-1:1)<0;}int main(void){return f(0)!=0||f(1)!=1;}",
    ] {
        assert_constant_program(source);
    }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(
            "int f(int c){if(1/(c?1:0)>2)return 1;return 0;}int main(void){return f(0);}",
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3002"));
    }
    for source in [
        "void observe(void);static int a,b;void f(void){if(&a<&b)observe();}",
        "void observe(void);static int a;void f(void){if((unsigned long)&a+~0ul<(unsigned long)&a)observe();}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("call @observe"));
    }
}

#[test]
fn assigned_constants_and_finite_flags_survive_ssa_edges() {
    for source in [
        "void invalid(void);int main(void){unsigned long size;size=sizeof(long);if(size>4096)invalid();return size!=8;}",
        "void invalid(void);int main(void){const int size=({int n=-22;if(sizeof(long)==1)n=16;else if(sizeof(long)==2)n=8;else if(sizeof(long)==4)n=0;else if(sizeof(long)==8)n=24;n;});if(size<0)invalid();return size!=24;}",
        "void invalid(void);int f(int c,int n){int flags=c?1:0;if(n>1)flags|=2;if(flags>3)invalid();return flags;}int main(void){return f(0,1)!=0||f(1,1)!=1||f(0,2)!=2||f(1,2)!=3;}",
        "void invalid(void);static __attribute__((always_inline)) inline int order(unsigned long n){if(__builtin_constant_p(n)){if(!n)return 52;if(n<4096)return 0;return 64-__builtin_clzl(n-1)-12;}n--;n>>=12;return n?64-__builtin_clzl(n):0;}int main(void){if(order(32768)>7)invalid();return order(32768)!=3;}",
        "void invalid(void);int main(void){if(65531<({unsigned x=4000000,y=65000;x<y?x:y;}))invalid();return 0;}",
    ] {
        assert_constant_program(source);
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"), "{source}");
    }
}

#[test]
fn infinite_control_flow_retains_effect_tokens_after_dead_address_pruning() {
    for body in [
        "stop(); while (1);",
        "stop(); for (;;);",
        "stop(); again: goto again;",
        "asm volatile(\"\" ::: \"memory\"); while (1);",
    ] {
        let source = format!("void stop(void); void halt(void){{{body}}}");
        let output = run(&source, "--emit=evoir");
        assert!(
            output.status.success(),
            "{source}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("mem<all>"), "{text}");
        assert!(text.contains("world"), "{text}");
        if body.starts_with("stop()") {
            assert!(text.contains("call @stop"), "{text}");
        }
    }
}

#[test]
fn unused_private_callback_tables_do_not_keep_unreachable_calls_linked() {
    let source = include_str!("../../../tests/fixtures/private-callback-reachability.c");
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(!text.contains("call @missing"), "{text}");
    assert!(!text.contains("global @operations"), "{text}");
    assert_constant_program(source);
}

#[test]
fn branch_pruning_releases_only_unreferenced_internal_definitions() {
    let source="void invalid(void);static int impossible(void){invalid();return 1;}static int enabled(int n){return 0;}int f(void){return enabled(4)?impossible():0;}int main(void){return f();}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("func @impossible"));
}

#[test]
fn integer_flow_widens_at_loops_and_keeps_poison_and_indirect_entries() {
    for source in [
        "int f(int n){int x=0,seen=0;while(x<n){++x;if(x==9)++seen;}return x+seen;}int main(void){return f(0)!=0||f(7)!=7||f(20)!=21;}",
        "int f(int c){int x=2;if(c)x=7;else x=9;return x==7;}int main(void){return f(0)!=0||f(1)!=1;}",
        "int f(int c){int x=0;goto * (c?&&a:&&b);a:x=3;goto out;b:x=7;out:return x;}int main(void){return f(0)!=7||f(1)!=3;}",
    ] { assert_constant_program(source); }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run("int main(void){int x;if(x==0)return 1;return 0;}", mode);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn internal_roots_include_relocations_aliases_assembly_and_used_definitions() {
    for source in [
        "static int keep(void){return 1;}int(*p)(void)=keep;int main(void){return p()!=1;}",
        "static int keep(void){return 1;}int exported(void)__attribute__((alias(\"keep\")));int main(void){return exported()!=1;}",
        "static __attribute__((used)) int keep(void){return 1;}int main(void){return 0;}",
        "static int keep(void){return 1;}asm(\".quad keep\");int main(void){return 0;}",
        "static int keep(void){return 1;}static int(*p)(void)__attribute__((used))=keep;int main(void){return 0;}",
        "static int keep(void){return 1;}static int(*p)(void)__attribute__((section(\".callbacks\")))=keep;int main(void){return 0;}",
        "static int keep(void){return 1;}static int(*p)(void)=keep;asm(\".quad p\");int main(void){return 0;}",
        "static int keep(void){return 1;}static int(*p)(void)=keep;extern int(*alias)(void)__attribute__((alias(\"p\")));int main(void){return alias()!=1;}",
        "static int keep(void){return 1;}int f(void){static int(*p)(void)__attribute__((used))=keep;return 0;}int main(void){return f();}",
    ] {
        let output=run(source,"--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("func @keep"));
    }
}

#[test]
fn private_initialized_union_representation_proves_layout_assertions() {
    let source="void invalid(void);union lock{unsigned value;struct{unsigned char a,b,c,d;}bytes;};int main(void){union lock u={.bytes={0,0,0,0}};if(u.value!=0)invalid();return 0;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    for source in [
        "union u{int v;char a[4];};int main(void){union u x={0};x.v=3;return x.v!=3;}",
        "union u{int v;char a[4];};int main(void){union u x={0};int*p=&x.v;*p=3;return x.v!=3;}",
        "union u{int v;char a[4];};int main(void){union u x={0};char*p=x.a;p[0]=3;return x.v!=3;}",
        "union u{int v;char a[4];};int main(void){goto later;{union u x={0};later:x.v=3;return x.v!=3;}}",
    ] {assert_constant_program(source);}
}

#[test]
fn representation_facts_compose_with_assigned_values_and_short_circuit_edges() {
    let source = include_str!("../../../tests/fixtures/constant-control-flow.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn inline_local_representation_is_known_only_after_its_initialization() {
    let source = "void invalid(void);typedef struct{int counter;}atomic_t;typedef struct{union{atomic_t val;struct{unsigned char locked,pending;};struct{unsigned short locked_pending,tail;};};}spin;static inline void check(spin*p){spin*l=(void*)p;union{unsigned val;spin lock;}u={.lock={{.val={(0)}}}};do{if(!(u.val==0))invalid();}while(0);(void)l;}void f(spin*p){check(p);}int main(void){spin s={0};f(&s);return 0;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));

    for source in [
        "int main(void){goto later;{union{int v;}u={0};later:return u.v;}}",
        "int main(void){switch(1){union{int v;}u={0};case 1:return u.v;}return 0;}",
    ] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, mode);
            assert!(!output.status.success(), "{source}: {mode}");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains("E3001"),
                "{source}: {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn constant_queries_use_proven_values_after_a_defining_assignment() {
    for source in [
        "void invalid(void);int main(void){unsigned long meta;meta=255;if(!__builtin_constant_p(meta))invalid();return meta!=255;}",
        "void invalid(void);int main(void){int n;int before=__builtin_constant_p(n);n=7;if(!__builtin_constant_p(n))invalid();return before||n!=7;}",
        "void invalid(void);int main(void){int calls=0;unsigned char n;n=(++calls,257);if(!__builtin_constant_p(n))invalid();return calls!=1||n!=1;}",
    ] {
        assert_constant_program(source);
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    }
}

#[test]
fn assignment_query_facts_do_not_escape_branches_or_aliases() {
    for source in [
        "int f(int c){int x;if(c){x=7;}return __builtin_constant_p(x);}int main(void){return f(0)||f(1);}",
        "int f(int n){int x=1;int*p=&x;x=7;*p=n;return __builtin_constant_p(x);}int main(void){return f(3);}",
        "int main(void){goto out;{int x;x=7;out:return __builtin_constant_p(x);}}",
        "int f(int n){int x=1;while(n--){x=7;}return __builtin_constant_p(x);}int main(void){return f(0)||f(1);}",
    ] { assert_constant_program(source); }
}

#[test]
fn bounded_private_table_reductions_fold_without_exporting_runtime_state() {
    let source = "void invalid(void);static const unsigned char sizes[]={3,5,7};static __attribute__((always_inline)) inline unsigned sum(void){unsigned n=2;int i;for(i=0;i<sizeof(sizes);i++)n+=sizes[i];return n;}int main(void){if(sum()>255)invalid();return sum()!=17;}";
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
    for source in [
        "static unsigned char values[]={1};static inline int sum(void){int n=0;for(int i=0;i<1;i++)n+=values[i];return n;}int main(void){values[0]=7;return sum()!=7;}",
        "static int count;static inline int sum(void){for(int i=0;i<3;i++)++count;return count;}int main(void){sum();return sum()!=6;}",
        "static const unsigned char values[]={1};static inline int sum(void){int n=0;for(int i=0;i<2;i++)n+=values[i];return n;}int main(void){return sum();}",
    ] {
        let output = run(source, "--interpret-evoir");
        if source.contains("i<2") {
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("E3011"));
        } else {
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
            assert_eq!(output.stdout, b"0\n");
        }
    }
}

#[test]
fn private_loop_folding_does_not_export_interpreter_addresses() {
    for source in [
        "static inline unsigned long address(void){int x;unsigned long a=0;for(int i=0;i<1;i++)a=(unsigned long)&x;return a;}unsigned long f(void){return address();}",
        "static const int table[]={1};static inline unsigned long address(void){unsigned long a=0;for(int i=0;i<1;i++)a=(unsigned long)table;return a;}unsigned long f(void){return address();}",
    ] {
        let output = run(source, "--emit=evoir");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("ptr.to_int"), "{source}");
    }
}

#[test]
fn object_size_proves_declared_bounds_without_reading_the_object() {
    let source = include_str!("../../../tests/fixtures/object-size-bounds.c");
    assert_constant_program(source);
    let output = run(source, "--emit=evoir");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("call @invalid"));
}

#[test]
fn object_size_declines_unproved_or_effectful_operands() {
    for source in [
        "int main(void){char a[16];int effects=0;unsigned long n=__builtin_object_size((effects++,a),1);return effects||n!=~0ul;}",
        "unsigned long f(char*p,int i){return __builtin_object_size(p+i,1);}int main(void){return f((char*)37,2)!=~0ul;}",
        "int main(void){char a[16];char*p=a;return __builtin_object_size(p++,3)!=0||p!=a;}",
        "struct s{int head;char flexible[];};int f(struct s*p){return __builtin_object_size(p->flexible,1)!=~0ul;}int main(void){return f((struct s*)0);}",
        "struct s{char a[16];};int main(void){struct s *volatile p=(void*)0;return __builtin_object_size(p->a,1)!=~0ul;}",
        "extern int a[];unsigned long n=__builtin_object_size(a,0);int a[4];int main(void){return n!=~0ul;}",
        "struct s;extern struct s x;unsigned long n=__builtin_object_size(&x,0);struct s{int a;};struct s x;int main(void){return n!=~0ul;}",
        "struct s{char head[7],name[16],tail[5];};int main(void){char a[8];return __builtin_object_size(((struct s*)a)->name,1)!=1;}",
        "int main(void){char rows[3][4];return __builtin_object_size(rows[1],0)!=8||__builtin_object_size(rows[1],1)!=~0ul;}",
    ] { assert_constant_program(source); }
}

#[test]
fn constant_queries_prove_literal_bytes_without_broadening_source_ice() {
    assert_constant_program(include_str!(
        "../../../tests/fixtures/constant-query-literal-bytes.c"
    ));
    for source in [
        "int main(void){const char*p=\"a\\0b\";return !__builtin_constant_p(*(p+2))||!__builtin_constant_p((p+3)[0])||!__builtin_constant_p(1[p]);}",
        "int main(void){const char*p=\"abc\";return __builtin_constant_p(p[-1])||__builtin_constant_p(p[4]);}",
        "int main(void){const char*p=\"abc\";return __builtin_constant_p((p+5)[-3]);}",
        "int main(void){const volatile char*p=\"abc\";return __builtin_constant_p(*p)||__builtin_constant_p(p[1]);}",
        "int main(void){const char*volatile p=\"abc\";return __builtin_constant_p(*p);}",
        "int calls;const char*text(void){++calls;return \"abc\";}int main(void){return __builtin_constant_p(*text())||calls;}",
        "int main(void){const char*p=\"abc\";int i=0;return __builtin_constant_p(p[i++])||i;}",
        "int main(void){const char*p=\"abc\";int i=0;return __builtin_constant_p((i++,p)[0])||i;}",
        "int main(void){const char*p=\"abc\";volatile int i=0;return __builtin_constant_p(p[i]);}",
        "int main(void){char bytes[]=\"abc\";volatile char c='z';bytes[0]=c;return __builtin_constant_p(bytes[0]);}",
        "int calls;unsigned long strlen(const char*p){++calls;return 37;}int main(void){const char*p=\"abc\";return __builtin_strlen(p)!=37||calls!=1;}",
    ] {
        assert_constant_program(source);
    }
}

#[test]
fn initialized_byte_queries_drop_facts_after_possible_writes() {
    for source in [
        "int main(void){char a[4]=\"abc\";volatile char v='z';a[0]=v;return __builtin_constant_p(a[0])||a[0]!='z';}",
        "int main(void){char a[4]=\"abc\";char*p=a;volatile char v='z';*p=v;return __builtin_constant_p(a[0])||a[0]!='z';}",
        "__attribute__((noinline)) void change(char*p){*p='z';}int main(void){char a[4]=\"abc\";change(a);return __builtin_constant_p(a[0])||a[0]!='z';}",
        "int main(void){char a[4]=\"abc\";volatile int v='z';__builtin_add_overflow(v,0,&a[0]);return __builtin_constant_p(a[0])||a[0]!='z';}",
        "int main(void){char a[4]=\"abc\";volatile char v='z';__atomic_store_n(a,v,0);return __builtin_constant_p(a[0])||a[0]!='z';}",
        "int main(void){char a[4]=\"abc\";volatile int n=1;while(n--){a[0]='z';}return __builtin_constant_p(a[0])||a[0]!='z';}",
        "int main(void){goto later;{char a[4]=\"abc\";later:return __builtin_constant_p(a[0]);}}",
        "int main(void){char a[4];return __builtin_constant_p(a[0]);}",
        "static const char*p=\"abc\";int main(void){p=\"z\";return p[0]!='z';}",
        "static const char*p=\"abc\";__attribute__((noinline)) void change(const char**q){*q=\"z\";}int main(void){change(&p);return p[0]!='z';}",
        "static const char*p=\"abc\";extern const char*q __attribute__((alias(\"p\")));int main(void){q=\"z\";return p[0]!='z';}",
    ] { assert_constant_program(source); }
    for mode in ["--interpret-c-hir", "--interpret-evoir"] {
        let output = run(
            "int main(void){char a[4];int known=__builtin_constant_p(a[0]);return known+a[0];}",
            mode,
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("E3001"));
    }
}

#[test]
fn initialized_byte_queries_respect_array_limits_qualifiers_and_unevaluated_inputs() {
    for source in [
        "int main(void){char a[80]=\"abc\";return !__builtin_constant_p(a[79])||a[79]!=0||__builtin_constant_p(a[80])||__builtin_constant_p(a[-1]);}",
        "int main(void){char a[3]=\"abc\";return !__builtin_constant_p(a[2])||__builtin_constant_p(a[3])||a[2]!='c';}",
        "int main(void){unsigned char a[]=\"a\\0b\\377\";return !__builtin_constant_p(a[3])||a[1]||a[3]!=255;}",
        "int main(void){volatile char a[]=\"abc\";return __builtin_constant_p(a[0]);}",
        "int main(void){char a[]=\"abc\";int i=0;return __builtin_constant_p(a[i++])||i||!__builtin_constant_p(a[0]);}",
        "int main(void){char a[]=\"abc\";int q=__builtin_constant_p(a[0]='z');return q||a[0]!='a'||!__builtin_constant_p(a[0]);}",
    ] { assert_constant_program(source); }
}

#[test]
fn private_pointer_query_facts_exclude_linker_visible_storage() {
    for source in [
        "static const char*p __attribute__((used))=\"abc\";const char*get(void){return p;}",
        "static const char*p __attribute__((section(\".callbacks\")))=\"abc\";const char*get(void){return p;}",
        "static const char*p=\"abc\";asm(\".quad p\");const char*get(void){return p;}",
        "static const char*p=\"abc\";extern const char*q __attribute__((alias(\"p\")));const char*get(void){return p;}",
    ] {
        let output=run(source,"--emit=evoir");
        assert!(output.status.success(),"{}",String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8_lossy(&output.stdout).contains("global_addr @p"),"{source}");
    }
}
