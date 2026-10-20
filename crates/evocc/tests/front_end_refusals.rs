//! Source constraints must fail at their source boundary, not as invalid IR.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(source: &str, target: &str, mode: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args([
            "--target",
            target,
            mode,
            "--error-format=short",
            "-w",
            "/dev/stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let _ = child.stdin.take().unwrap().write_all(source.as_bytes());
    child.wait_with_output().unwrap()
}

fn rejects(source: &str, code: &str, at: &str) {
    let offset = source.rfind(at).expect("diagnostic anchor in source");
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = prefix.rsplit('\n').next().unwrap().len() + 1;
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        for mode in ["--check", "--emit=evoir"] {
            let output = run(source, target, mode);
            let message = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{source}\n{message}");
            assert!(
                message.starts_with(&format!("/dev/stdin:{line}:{column}: error[{code}]:")),
                "{target} {mode}: {source}\n{message}"
            );
            assert!(!message.contains("E3101"), "{message}");
            assert!(output.stdout.is_empty(), "a refusal emits no program");
        }
    }
}

fn executes(source: &str) {
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        for mode in ["--interpret-c-hir", "--interpret-evoir"] {
            let output = run(source, target, mode);
            assert!(output.status.success(), "{source}\n{output:?}");
            assert_eq!(output.stdout, b"0\n", "{target} {mode}: {source}");
        }
    }
}

#[test]
fn overflow_builtins_require_a_writable_integer_pointee() {
    for operation in ["add", "sub", "mul"] {
        for declaration in ["const int r=0;", "typedef const int ci; ci r=0;"] {
            rejects(
                &format!(
                    "int main(void){{{declaration}return __builtin_{operation}_overflow(1,2,&r);}}"
                ),
                "E2913",
                "&r",
            );
        }
    }
    executes(
        "int main(void){int r=0;int *const p=&r;return __builtin_add_overflow(1,2,p)||(r!=3);}",
    );
}

#[test]
fn supported_builtin_queries_report_the_implemented_operations() {
    executes("_Static_assert(__has_builtin(__builtin_memset),\"memset\");_Static_assert(__has_builtin(__builtin_clz),\"clz\");_Static_assert(__has_builtin(__builtin_trap),\"trap\");_Static_assert(__has_builtin(__builtin_offsetof),\"offsetof\");int main(void){return 0;}");
}

#[test]
fn library_symbol_collisions_have_a_source_diagnostic() {
    for source in [
        "void f(char *p){__builtin_memset(p,0,1);}static int memset=3;int main(void){char p=1;f(&p);return p+(memset!=3);}",
        "static int memset=3;void f(char *p){__builtin_memset(p,0,1);}int main(void){char p=1;f(&p);return p+(memset!=3);}",
    ] {
        rejects(source, "E2946", "memset=3");
    }
    executes("int main(void){static int memset=3;char p=1;__builtin_memset(&p,0,1);return p+(memset!=3);}");
    executes("static int memset=3;int main(void){return memset!=3;}");
}

#[test]
fn ambiguous_members_name_the_member_instead_of_an_internal_projection() {
    for access in ["v.x", "p->x"] {
        rejects(
            &format!("struct S{{struct {{int x;}};struct {{int x;}};}};int main(void){{struct S v={{0}};struct S *p=&v;return {access};}}"),
            "E2906", "x;",
        );
    }
    executes("struct S{struct {int x;};};int main(void){struct S v={3};struct S *p=&v;return v.x+p->x-6;}");
}

#[test]
fn jumps_into_statement_expressions_use_the_dedicated_code() {
    rejects(
        "int main(void){goto inside;({inside:1;});return 0;}",
        "E2918",
        "goto inside",
    );
    rejects(
        "int main(void){goto *&&inside;({inside:1;});return 0;}",
        "E2918",
        "goto *",
    );
    executes("int main(void){({goto outside;1;});outside:return 0;}");
}

#[test]
fn packing_inside_a_record_is_not_an_ordinary_syntax_error() {
    rejects(
        "struct S{int x;\n#pragma pack(1)\nint y;};int main(void){return 0;}",
        "E0107",
        "#pragma",
    );
    executes("#pragma pack(push,1)\nstruct S{char c;int x;};\n#pragma pack(pop)\nint main(void){return sizeof(struct S)!=5;}");
}

#[test]
fn incompatible_record_casts_are_refused_before_lowering() {
    rejects("struct A{int x;};struct B{int x;};int main(void){struct A a={1};struct B b=(struct B)a;return b.x;}", "E2953", "(struct B)a");
    executes(
        "struct A{int x;};int main(void){struct A a={3};struct A b=(struct A)a;return b.x-3;}",
    );
}

#[test]
fn invalid_bound_types_take_precedence_over_storage_restrictions() {
    for source in [
        "int a[1.5];int main(void){return 0;}",
        "int main(void){int a[1.5];return 0;}",
        "int main(void){static int a[1.5];return 0;}",
        "struct S{int a[1.5];};int main(void){return 0;}",
        "void f(int a[1.5]);int main(void){return 0;}",
    ] {
        rejects(source, "E2923", "1.5");
    }
    rejects("int n;int a[n];int main(void){return 0;}", "E2203", "[n]");
    executes("int a[(int)1.5];int main(void){int n=2;int b[n];return sizeof(a)!=4||sizeof(b)!=8;}");
}
