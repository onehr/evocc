//! A store to a bit-field writes only its own bits (EvoIR 3.8 and 10.1.6,
//! D23). It is never a read of the other members of its storage unit, and the
//! other bits keep their initializedness. A later read of a member that was
//! never stored is still `E3001`; a read of a stored member is not. Each
//! case runs on both targets, in both interpreters and through `--check`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"];
const MODES: [&str; 3] = ["--interpret-c-hir", "--interpret-evoir", "--check"];

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A fresh directory, removed on success.
struct Work(PathBuf);

impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-bit-field-initialization-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained: {}", self.0.display());
            return;
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn evocc(path: &Path, target: &str, mode: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .args(["-w", "--error-format=short", mode])
        .arg(path)
        .output()
        .unwrap()
}

/// `main` of `source` completes in every mode and returns `expected`.
fn completes(name: &str, source: &str, expected: i64) {
    let work = Work::new(name);
    let path = work.source(&format!("{name}.c"), source);
    for target in TARGETS {
        for mode in MODES {
            let output = evocc(&path, target, mode);
            let label = format!("{name} {target} {mode}");
            assert!(output.status.success(), "{label}\n{}", text(&output));
            assert!(output.stderr.is_empty(), "{label}\n{}", text(&output));
            if mode != "--check" {
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    expected.to_string(),
                    "{label}\n{}",
                    text(&output)
                );
            }
        }
    }
}

/// Every mode stops with `E3001` at `line:column` of `source`, and at no
/// earlier position.
fn uninitialized_read_at(name: &str, source: &str, line: u32, column: u32) {
    let work = Work::new(name);
    let path = work.source(&format!("{name}.c"), source);
    let expected = format!(
        "{}:{line}:{column}: error[E3001]: read-of-uninitialized-object",
        path.display()
    );
    for target in TARGETS {
        for mode in MODES {
            let output = evocc(&path, target, mode);
            let label = format!("{name} {target} {mode}");
            assert_eq!(output.status.code(), Some(1), "{label}\n{}", text(&output));
            assert_eq!(
                String::from_utf8_lossy(&output.stderr).lines().next(),
                Some(expected.as_str()),
                "{label}\n{}",
                text(&output)
            );
        }
    }
}

/// The lead's reproducer: two fields share one byte, and only one is stored.
#[test]
fn a_bit_field_store_is_not_a_read_of_its_neighbour() {
    completes(
        "store",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) { struct S s; s.a = 1; return 0; }\n",
        0,
    );
}

#[test]
fn a_read_of_the_stored_member_has_its_value() {
    completes(
        "stored",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) { struct S s; s.a = 5; return s.a; }\n",
        5,
    );
}

#[test]
fn a_read_of_a_never_stored_member_is_an_uninitialized_read() {
    uninitialized_read_at(
        "never-stored",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) {\n\
         \x20   struct S s;\n\
         \x20   s.a = 1;\n\
         \x20   return s.b;\n\
         }\n",
        5,
        12,
    );
}

/// Adjacent fields of one byte, each stored and read, in both orders.
#[test]
fn adjacent_fields_in_one_byte_keep_their_own_values() {
    completes(
        "one-byte",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) {\n\
         \x20   struct S s, t;\n\
         \x20   s.a = 5; s.b = 17;\n\
         \x20   t.b = 30; t.a = 2;\n\
         \x20   s.a = 6;\n\
         \x20   return s.a + 8 * s.b + 1000 * t.a + 10000 * t.b;\n\
         }\n",
        6 + 8 * 17 + 1000 * 2 + 10000 * 30,
    );
}

/// A field that crosses bytes, with whole middle bytes, leaves the bits of
/// its neighbours uninitialized in both partial bytes.
#[test]
fn fields_across_bytes_initialize_only_their_own_bits() {
    let declaration = "struct U { unsigned p : 4; unsigned q : 20; unsigned r : 8; };\n";
    completes(
        "across",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct U u;\n\
             \x20   u.q = 0xabcde;\n\
             \x20   return u.q == 0xabcde;\n\
             }}\n"
        ),
        1,
    );
    uninitialized_read_at(
        "across-low",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct U u;\n\
             \x20   u.q = 0xabcde;\n\
             \x20   return u.p;\n\
             }}\n"
        ),
        5,
        12,
    );
    uninitialized_read_at(
        "across-high",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct U u;\n\
             \x20   u.q = 0xabcde;\n\
             \x20   return u.r;\n\
             }}\n"
        ),
        5,
        12,
    );
    let crossing = "struct T { unsigned x : 6; unsigned y : 7; unsigned z : 3; };\n";
    completes(
        "crossing",
        &format!(
            "{crossing}int main(void) {{\n\
             \x20   struct T t;\n\
             \x20   t.y = 100;\n\
             \x20   t.z = 5;\n\
             \x20   t.x = 33;\n\
             \x20   return t.y + 1000 * t.z + 10000 * t.x;\n\
             }}\n"
        ),
        100 + 1000 * 5 + 10000 * 33,
    );
    uninitialized_read_at(
        "crossing-low",
        &format!(
            "{crossing}int main(void) {{\n\
             \x20   struct T t;\n\
             \x20   t.y = 100;\n\
             \x20   return t.x;\n\
             }}\n"
        ),
        5,
        12,
    );
    uninitialized_read_at(
        "crossing-high",
        &format!(
            "{crossing}int main(void) {{\n\
             \x20   struct T t;\n\
             \x20   t.y = 100;\n\
             \x20   return t.z;\n\
             }}\n"
        ),
        5,
        12,
    );
}

/// A signed field keeps its sign, and its unstored neighbour stays
/// uninitialized.
#[test]
fn signed_fields_keep_their_sign_and_their_neighbours_state() {
    let declaration = "struct V { int s : 3; int t : 5; signed char u : 4; };\n";
    completes(
        "signed",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct V v;\n\
             \x20   v.s = -2;\n\
             \x20   v.u = -8;\n\
             \x20   return v.s * 100 + v.u;\n\
             }}\n"
        ),
        -208,
    );
    uninitialized_read_at(
        "signed-neighbour",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct V v;\n\
             \x20   v.s = -2;\n\
             \x20   return v.t;\n\
             }}\n"
        ),
        5,
        12,
    );
}

/// A bit-field next to ordinary members: the field's store initializes none
/// of them, and their stores initialize none of the field's bits.
#[test]
fn a_bit_field_next_to_ordinary_members() {
    let declaration = "struct W { char c; unsigned f : 4; unsigned g : 4; int i; };\n";
    completes(
        "ordinary",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct W w;\n\
             \x20   w.f = 9;\n\
             \x20   w.c = 7;\n\
             \x20   w.i = -3;\n\
             \x20   return w.f * 100 + w.c * 10 + w.i;\n\
             }}\n"
        ),
        967,
    );
    for (member, column) in [("c", 12), ("g", 12), ("i", 12)] {
        uninitialized_read_at(
            &format!("ordinary-{member}"),
            &format!(
                "{declaration}int main(void) {{\n\
                 \x20   struct W w;\n\
                 \x20   w.f = 9;\n\
                 \x20   return w.{member};\n\
                 }}\n"
            ),
            5,
            column,
        );
    }
    uninitialized_read_at(
        "ordinary-field",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct W w;\n\
             \x20   w.c = 1;\n\
             \x20   w.i = 2;\n\
             \x20   return w.g;\n\
             }}\n"
        ),
        6,
        12,
    );
}

/// A compound update reads its own field: after a store it has a value,
/// and without one it is an uninitialized read.
#[test]
fn a_compound_update_reads_only_its_own_field() {
    let declaration = "struct S { unsigned a : 3; unsigned b : 5; };\n";
    completes(
        "compound",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct S s;\n\
             \x20   s.a = 1;\n\
             \x20   s.a += 2;\n\
             \x20   s.a++;\n\
             \x20   return s.a;\n\
             }}\n"
        ),
        4,
    );
    uninitialized_read_at(
        "compound-unstored",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct S s;\n\
             \x20   s.a = 1;\n\
             \x20   s.b += 2;\n\
             \x20   return 0;\n\
             }}\n"
        ),
        5,
        5,
    );
}

/// A copy of the record moves each bit's initializedness: the copy of a
/// stored member has its value, and the copy of an unstored one does not.
#[test]
fn record_copies_keep_the_initializedness_of_each_bit() {
    let declaration = "struct S { unsigned a : 3; unsigned b : 5; };\n\
                       static unsigned first(struct S s) { return s.a; }\n\
                       static unsigned second(struct S s) { return s.b; }\n";
    completes(
        "copy",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct S s;\n\
             \x20   s.a = 6;\n\
             \x20   struct S t = s;\n\
             \x20   return t.a * 10 + first(s);\n\
             }}\n"
        ),
        66,
    );
    uninitialized_read_at(
        "copy-unstored",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct S s;\n\
             \x20   s.a = 6;\n\
             \x20   struct S t = s;\n\
             \x20   return t.b;\n\
             }}\n"
        ),
        8,
        12,
    );
    uninitialized_read_at(
        "argument-unstored",
        &format!(
            "{declaration}int main(void) {{\n\
             \x20   struct S s;\n\
             \x20   s.a = 6;\n\
             \x20   return second(s);\n\
             }}\n"
        ),
        3,
        45,
    );
}
