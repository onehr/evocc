//! GNU range initializers share a value; they do not repeat its effects.

// Original c-testsuite tests/single-exec/00216.c and its expected output.
// Keep the source bytes: this is an upstream replay, not a reduced program.
const CTEST_INITIALIZERS: &str = r###"typedef unsigned char u8;
typedef struct {} empty_s;
struct contains_empty {
    u8 a;
    empty_s empty;
    u8 b;
};
struct contains_empty ce = { { (1) }, (empty_s){}, 022, };
/* The following decl of 'q' would demonstrate the TCC bug in init_putv when
   handling copying compound literals.  (Compound literals
   aren't acceptable constant initializers in isoc99, but
   we accept them like gcc, except for this case)
//char *q = (char *){ "trara" }; */
struct SS {u8 a[3], b; };
struct SS sinit16[] = { { 1 }, 2 };
struct S
{
  u8 a,b;
  u8 c[2];
};

struct T
{
  u8 s[16];
  u8 a;
};

struct U
{
  u8 a;
  struct S s;
  u8 b;
  struct T t;
};

struct V
{
  struct S s;
  struct T t;
  u8 a;
};

struct W
{
  struct V t;
  struct S s[];
};

struct S gs = ((struct S){1, 2, 3, 4});
struct S gs2 = {1, 2, {3, 4}};
struct T gt = {"hello", 42};
struct U gu = {3, 5,6,7,8, 4, "huhu", 43};
struct U gu2 = {3, {5,6,7,8}, 4, {"huhu", 43}};
/* Optional braces around scalar initializers.  Accepted, but with
   a warning.  */
struct U gu3 = { {3}, {5,6,7,8,}, 4, {"huhu", 43}};
/* Many superfluous braces and leaving out one initializer for U.s.c[1] */
struct U gu4 = { 3, {5,6,7,},  5, { "bla", {44}} };
/* Superfluous braces and useless parens around values */
struct S gs3 = { (1), {(2)}, {(((3))), {4}}};
/* Superfluous braces, and leaving out braces for V.t, plus cast */
struct V gv = {{{3},4,{5,6}}, "haha", (u8)45, 46};
/* Compound literal */
struct V gv2 = {(struct S){7,8,{9,10}}, {"hihi", 47}, 48};
/* Parens around compound literal */
struct V gv3 = {((struct S){7,8,{9,10}}), {"hoho", 49}, 50};
/* Initialization of a flex array member (warns in GCC) */
struct W gw = {{1,2,3,4}, {1,2,3,4,5}};

union UU {
    u8 a;
    u8 b;
};
struct SU {
    union UU u;
    u8 c;
};
struct SU gsu = {5,6};

/* Unnamed struct/union members aren't ISO C, but it's a widely accepted
   extension.  See below for further extensions to that under -fms-extension.*/
union UV {
    struct {u8 a,b;};
    struct S s;
};
union UV guv = {{6,5}};
union UV guv2 = {{.b = 7, .a = 8}};
union UV guv3 = {.b = 8, .a = 7};

/* Under -fms-extensions also the following is valid:
union UV2 {
    struct Anon {u8 a,b;};    // unnamed member, but tagged struct, ...
    struct S s;
};
struct Anon gan = { 10, 11 }; // ... which makes it available here.
union UV2 guv4 = {{4,3}};     // and the other inits from above as well
*/

struct in6_addr {
    union {
	u8 u6_addr8[16];
	unsigned short u6_addr16[8];
    } u;
};
struct flowi6 {
    struct in6_addr saddr, daddr;
};
struct pkthdr {
    struct in6_addr daddr, saddr;
};
struct pkthdr phdr = { { { 6,5,4,3 } }, { { 9,8,7,6 } } };

struct Wrap {
    void *func;
};
int global;
void inc_global (void)
{
  global++;
}

struct Wrap global_wrap[] = {
    ((struct Wrap) {inc_global}),
    inc_global,
};

#include <stdio.h>
void print_ (const char *name, const u8 *p, long size)
{
  printf ("%s:", name);
  while (size--) {
      printf (" %x", *p++);
  }
  printf ("\n");
}
#define print(x) print_(#x, (u8*)&x, sizeof (x))
#if 1
void foo (struct W *w, struct pkthdr *phdr_)
{
  struct S ls = {1, 2, 3, 4};
  struct S ls2 = {1, 2, {3, 4}};
  struct T lt = {"hello", 42};
  struct U lu = {3, 5,6,7,8, 4, "huhu", 43};
  struct U lu1 = {3, ls, 4, {"huhu", 43}};
  struct U lu2 = {3, (ls), 4, {"huhu", 43}};
  const struct S *pls = &ls;
  struct S ls21 = *pls;
  struct U lu22 = {3, *pls, 4, {"huhu", 43}};
  /* Incomplete bracing.  */
  struct U lu21 = {3, ls, 4, "huhu", 43};
  /* Optional braces around scalar initializers.  Accepted, but with
     a warning.  */
  struct U lu3 = { 3, {5,6,7,8,}, 4, {"huhu", 43}};
  /* Many superfluous braces and leaving out one initializer for U.s.c[1] */
  struct U lu4 = { 3, {5,6,7,},  5, { "bla", 44} };
  /* Superfluous braces and useless parens around values */
  struct S ls3 = { (1), (2), {(((3))), 4}};
  /* Superfluous braces, and leaving out braces for V.t, plus cast */
  struct V lv = {{3,4,{5,6}}, "haha", (u8)45, 46};
  /* Compound literal */
  struct V lv2 = {(struct S)w->t.s, {"hihi", 47}, 48};
  /* Parens around compound literal */
  struct V lv3 = {((struct S){7,8,{9,10}}), ((const struct W *)w)->t.t, 50};
  const struct pkthdr *phdr = phdr_;
  struct flowi6 flow = { .daddr = phdr->daddr, .saddr = phdr->saddr };
  int elt = 0x42;
  /* Range init, overlapping */
  struct T lt2 = { { [1 ... 5] = 9, [6 ... 10] = elt, [4 ... 7] = elt+1 }, 1 };
  print(ls);
  print(ls2);
  print(lt);
  print(lu);
  print(lu1);
  print(lu2);
  print(ls21);
  print(lu21);
  print(lu22);
  print(lu3);
  print(lu4);
  print(ls3);
  print(lv);
  print(lv2);
  print(lv3);
  print(lt2);
  print(flow);
}
#endif

void test_compound_with_relocs (void)
{
  struct Wrap local_wrap[] = {
      ((struct Wrap) {inc_global}),
      inc_global,
  };
  void (*p)(void);
  p = global_wrap[0].func; p();
  p = global_wrap[1].func; p();
  p = local_wrap[0].func; p();
  p = local_wrap[1].func; p();
}

void sys_ni(void) { printf("ni\n"); }
void sys_one(void) { printf("one\n"); }
void sys_two(void) { printf("two\n"); }
void sys_three(void) { printf("three\n"); }
typedef void (*fptr)(void);
const fptr table[3] = {
    [0 ... 2] = &sys_ni,
    [0] = sys_one,
    [1] = sys_two,
    [2] = sys_three,
};

void test_multi_relocs(void)
{
  int i;
  for (i = 0; i < sizeof(table)/sizeof(table[0]); i++)
    table[i]();
}

/* Following is from GCC gcc.c-torture/execute/20050613-1.c.  */

struct SEA { int i; int j; int k; int l; };
struct SEB { struct SEA a; int r[1]; };
struct SEC { struct SEA a; int r[0]; };
struct SED { struct SEA a; int r[]; };

static void
test_correct_filling (struct SEA *x)
{
  static int i;
  if (x->i != 0 || x->j != 5 || x->k != 0 || x->l != 0)
    printf("sea_fill%d: wrong\n", i);
  else
    printf("sea_fill%d: okay\n", i);
  i++;
}

int
test_zero_init (void)
{
  /* The peculiarity here is that only a.j is initialized.  That
     means that all other members must be zero initialized.  TCC
     once didn't do that for sub-level designators.  */
  struct SEB b = { .a.j = 5 };
  struct SEC c = { .a.j = 5 };
  struct SED d = { .a.j = 5 };
  test_correct_filling (&b.a);
  test_correct_filling (&c.a);
  test_correct_filling (&d.a);
  return 0;
}

int main()
{
  print(ce);
  print(gs);
  print(gs2);
  print(gt);
  print(gu);
  print(gu2);
  print(gu3);
  print(gu4);
  print(gs3);
  print(gv);
  print(gv2);
  print(gv3);
  print(sinit16);
  print(gw);
  print(gsu);
  print(guv);
  print(guv.b);
  print(guv2);
  print(guv3);
  print(phdr);
  foo(&gw, &phdr);
  //printf("q: %s\n", q);
  test_compound_with_relocs();
  test_multi_relocs();
  test_zero_init();
  return 0;
}
"###;
const CTEST_EXPECTED: &str = r###"ce: 1 12
gs: 1 2 3 4
gs2: 1 2 3 4
gt: 68 65 6c 6c 6f 0 0 0 0 0 0 0 0 0 0 0 2a
gu: 3 5 6 7 8 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
gu2: 3 5 6 7 8 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
gu3: 3 5 6 7 8 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
gu4: 3 5 6 7 0 5 62 6c 61 0 0 0 0 0 0 0 0 0 0 0 0 0 2c
gs3: 1 2 3 4
gv: 3 4 5 6 68 61 68 61 0 0 0 0 0 0 0 0 0 0 0 0 2d 2e
gv2: 7 8 9 a 68 69 68 69 0 0 0 0 0 0 0 0 0 0 0 0 2f 30
gv3: 7 8 9 a 68 6f 68 6f 0 0 0 0 0 0 0 0 0 0 0 0 31 32
sinit16: 1 0 0 0 2 0 0 0
gw: 1 2 3 4 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0
gsu: 5 6
guv: 6 5 0 0
guv.b: 5
guv2: 8 7 0 0
guv3: 7 8 0 0
phdr: 6 5 4 3 0 0 0 0 0 0 0 0 0 0 0 0 9 8 7 6 0 0 0 0 0 0 0 0 0 0 0 0
ls: 1 2 3 4
ls2: 1 2 3 4
lt: 68 65 6c 6c 6f 0 0 0 0 0 0 0 0 0 0 0 2a
lu: 3 5 6 7 8 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
lu1: 3 1 2 3 4 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
lu2: 3 1 2 3 4 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
ls21: 1 2 3 4
lu21: 3 1 2 3 4 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
lu22: 3 1 2 3 4 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
lu3: 3 5 6 7 8 4 68 75 68 75 0 0 0 0 0 0 0 0 0 0 0 0 2b
lu4: 3 5 6 7 0 5 62 6c 61 0 0 0 0 0 0 0 0 0 0 0 0 0 2c
ls3: 1 2 3 4
lv: 3 4 5 6 68 61 68 61 0 0 0 0 0 0 0 0 0 0 0 0 2d 2e
lv2: 1 2 3 4 68 69 68 69 0 0 0 0 0 0 0 0 0 0 0 0 2f 30
lv3: 7 8 9 a 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 32
lt2: 0 9 9 9 43 43 43 43 42 42 42 0 0 0 0 0 1
flow: 9 8 7 6 0 0 0 0 0 0 0 0 0 0 0 0 6 5 4 3 0 0 0 0 0 0 0 0 0 0 0 0
one
two
three
sea_fill0: okay
sea_fill1: okay
sea_fill2: okay
"###;

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn original_ctest_initializers_keep_their_output() {
    let work = Work::new();
    let source = work.source(CTEST_INITIALIZERS);
    let assembly = successful(compile(&source, LINUX, &["--emit=asm"]));
    std::fs::write(work.0.join("subject.s"), assembly).unwrap();
    let output = Command::new("container")
        .args(["run", "--rm", "--platform", "linux/arm64", "--network", "none", "-v"])
        .arg(format!("{}:/work", work.0.display()))
        .args(["evo-nexlab:sealed", "sh", "-c",
            "cd /work && gcc-14 -std=gnu17 -w -O2 input.c -o reference && gcc-14 subject.s -o subject && ./reference > reference.out && ./subject > subject.out && cmp reference.out subject.out && cat subject.out"])
        .output().unwrap();
    assert_eq!(successful(output), CTEST_EXPECTED);
}

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";
static NEXT: AtomicU32 = AtomicU32::new(0);

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-range-values-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn source(&self, text: &str) -> PathBuf {
        let path = self.0.join("input.c");
        std::fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn compile(path: &Path, target: &str, flags: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(LINUX).display()
        ))
        .args(["-w", "--error-format=short"])
        .args(flags)
        .arg(path)
        .output()
        .unwrap()
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

const PREFIX: &str = "int counter, trace;\n\
    int next(int id) { ++counter; trace = trace * 10 + id; return counter * 10 + id; }\n\
    struct P { int x, y; };\n\
    struct P pair(int id) { int v = next(id); struct P p = {v, v + 1}; return p; }\n";

// Expected values and ordering come from the GCC 14 native measurements.
const CASES: &[(&str, &str, &str)] = &[
    ("one", "int a[4] = {[0 ... 3] = next(1)};", "counter == 1 && trace == 1 && a[0] == 11 && a[1] == 11 && a[2] == 11 && a[3] == 11"),
    ("overlap", "int a[6] = {[0 ... 3] = next(1), [2 ... 5] = next(2)};", "counter == 2 && trace == 12 && a[0] == 11 && a[1] == 11 && a[2] == 22 && a[3] == 22 && a[4] == 22 && a[5] == 22"),
    ("overridden", "int a[4] = {[0 ... 3] = next(1), [0 ... 3] = next(2)};", "counter == 1 && trace == 2 && a[0] == 12 && a[1] == 12 && a[2] == 12 && a[3] == 12"),
    ("member", "struct T { int a[6]; int b; }; struct T t = {{[0 ... 3] = next(1), [2 ... 5] = next(2)}, 7};", "counter == 2 && trace == 12 && t.a[0] == 11 && t.a[1] == 11 && t.a[2] == 22 && t.a[3] == 22 && t.a[4] == 22 && t.a[5] == 22 && t.b == 7"),
    ("record", "struct P a[3] = {[0 ... 2] = pair(1)};", "counter == 1 && trace == 1 && a[0].x == 11 && a[0].y == 12 && a[1].x == 11 && a[1].y == 12 && a[2].x == 11 && a[2].y == 12"),
    ("compound", "int *a = (int[4]){[0 ... 3] = next(1)};", "counter == 1 && trace == 1 && a[0] == 11 && a[1] == 11 && a[2] == 11 && a[3] == 11"),
    ("nested-member", "struct P a[3] = {[1 ... 2].x = next(1)};", "counter == 1 && trace == 1 && a[0].x == 0 && a[0].y == 0 && a[1].x == 11 && a[1].y == 0 && a[2].x == 11 && a[2].y == 0"),
    ("record-list", "struct P a[3] = {[0 ... 2] = {next(1), next(2)}};", "counter == 2 && trace == 12 && a[0].x == 11 && a[0].y == 22 && a[1].x == 11 && a[1].y == 22 && a[2].x == 11 && a[2].y == 22"),
    ("destination-order", "int a[4] = {[2 ... 3] = next(1), [0 ... 1] = next(2)};", "counter == 2 && trace == 21 && a[0] == 12 && a[1] == 12 && a[2] == 21 && a[3] == 21"),
    ("record-overridden-member", "struct P a[2] = {[0 ... 1] = pair(1), [0].x = next(2)};", "counter == 2 && trace == 21 && a[0].x == 12 && a[0].y == 0 && a[1].x == 21 && a[1].y == 22"),
    ("changed-source", "struct P g = {3,4}; struct P a[2] = {[0 ... 1] = g, [0].x = (g.y = 9)};", "counter == 0 && trace == 0 && a[0].x == 9 && a[0].y == 0 && a[1].x == 3 && a[1].y == 9"),
    ("snapshot", "struct P g = {3,4}; struct P a[4] = {[0 ... 3] = g, [2].x = (g.y = 9)};", "a[0].x == 3 && a[0].y == 4 && a[1].x == 3 && a[1].y == 4 && a[2].x == 9 && a[2].y == 0 && a[3].x == 3 && a[3].y == 4"),
    ("branch", "int a[4] = {[0 ... 1] = next(1), [2 ... 3] = next(2) > 0 ? next(3) : next(4)};", "counter == 3 && trace == 123 && a[0] == 11 && a[1] == 11 && a[2] == 33 && a[3] == 33"),
    ("pointer", "int x; int *a[3] = {[0 ... 2] = (next(1), &x)};", "counter == 1 && trace == 1 && a[0] == &x && a[1] == &x && a[2] == &x"),
    ("narrow", "unsigned char a[4] = {[0 ... 3] = next(1) + 256};", "counter == 1 && trace == 1 && a[0] == 11 && a[1] == 11 && a[2] == 11 && a[3] == 11"),
    ("floating", "double a[4] = {[0 ... 3] = next(1)*0.5};", "counter == 1 && trace == 1 && a[0] == 5.5 && a[1] == 5.5 && a[2] == 5.5 && a[3] == 5.5"),
    ("fresh-execution", "int sum = 0; for (int k = 0; k < 3; ++k) { int a[2] = {[0 ... 1] = next(1)}; sum += a[0] + a[1]; }", "counter == 3 && trace == 111 && sum == 126"),
    ("macro-values", "\n#define PAIR next(1), next(2)\nstruct P a[2] = {[0 ... 1] = {PAIR}};", "counter == 2 && trace == 12 && a[0].x == 11 && a[0].y == 22 && a[1].x == 11 && a[1].y == 22"),
];

fn program(body: &str, condition: &str) -> String {
    format!("{PREFIX}int main(void) {{ {body} return !({condition}); }}\n")
}

#[test]
fn linux_range_values_execute_once_in_destination_order() {
    let work = Work::new();
    for (name, body, condition) in CASES {
        let source = work.source(&program(body, condition));
        for mode in ["--interpret-c-hir", "--interpret-evoir", "--check"] {
            let output = compile(&source, LINUX, &[mode]);
            assert!(
                output.status.success(),
                "{name} {mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let text = String::from_utf8(output.stdout).unwrap();
            assert!(
                if mode == "--check" {
                    text.contains("main returned 0")
                } else {
                    text.trim() == "0"
                },
                "{name} {mode}: {text}"
            );
        }
        successful(compile(&source, LINUX, &["--emit=asm"]));
    }
}

#[test]
fn darwin_names_the_nonconstant_value_not_the_index() {
    let work = Work::new();
    for (name, body, condition) in CASES {
        let source = work.source(&program(body, condition));
        let output = compile(&source, DARWIN, &["--emit=asm"]);
        assert_eq!(output.status.code(), Some(1), "{name}");
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostic.contains("E2969")
                && diagnostic.contains("range-designator-value-not-constant"),
            "{name}: {diagnostic}"
        );
        assert!(output.stdout.is_empty(), "{name}");
    }
}

#[test]
fn constant_values_and_nonconstant_index_rules_do_not_change() {
    let work = Work::new();
    for target in [LINUX, DARWIN] {
        let source =
            work.source("int main(void) { int a[4] = {[0 ... 3] = 7}; return a[3] != 7; }\n");
        for mode in [
            "--emit=asm",
            "--interpret-c-hir",
            "--interpret-evoir",
            "--check",
        ] {
            let output = successful(compile(&source, target, &[mode]));
            if mode.starts_with("--interpret-") {
                assert_eq!(output.trim(), "0");
            } else if mode == "--check" {
                assert!(output.contains("main returned 0"));
            } else {
                let positional =
                    work.source("int main(void) { int a[4] = {7, 7, 7, 7}; return a[3] != 7; }\n");
                assert_eq!(output, successful(compile(&positional, target, &[mode])));
                work.source("int main(void) { int a[4] = {[0 ... 3] = 7}; return a[3] != 7; }\n");
            }
        }
        let source =
            work.source("int main(void) { int i = 1; int a[4] = {[i] = 7}; return a[0]; }\n");
        let output = compile(&source, target, &["--emit=asm"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("E2917"));
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn linux_keeps_the_reference_array_value_refusal() {
    let work = Work::new();
    let source = work.source(&program(
        "int a[2][2] = {[0 ... 1] = {[0 ... 1] = next(1)}};",
        "a[1][1] == 11",
    ));
    let output = compile(&source, LINUX, &["--emit=asm"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("range-designator-array-value-not-constant"));
    assert!(output.stdout.is_empty());
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn linux_native_range_values_match_gcc14() {
    let work = Work::new();
    for (name, body, condition) in CASES {
        let source = work.source(&format!("extern int puts(const char *);\n{PREFIX}int main(void) {{ {body} if (!({condition})) return 1; puts(\"ok\"); return 0; }}\n"));
        let assembly = successful(compile(&source, LINUX, &["--emit=asm"]));
        std::fs::write(work.0.join("subject.s"), assembly).unwrap();
        let output = Command::new("container").args(["run", "--rm", "--platform", "linux/arm64", "--network", "none", "-v"]).arg(format!("{}:/work", work.0.display())).args(["evo-nexlab:sealed", "sh", "-c", "cd /work && gcc-14 -std=gnu17 -O2 input.c -o reference && gcc-14 subject.s -o subject && ./reference > reference.out && ./subject > subject.out && cmp reference.out subject.out && cat subject.out"]).output().unwrap();
        assert_eq!(successful(output), "ok\n", "{name}");
    }
}
