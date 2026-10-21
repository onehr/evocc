//! A store to a bit-field writes only its own bits (EvoIR 3.8 and 10.1.6,
//! D23 and D25). It is never a read of the other members of its storage
//! unit, and the other bits keep their initializedness, for a volatile
//! bit-field too. A later read of a member that was never stored is still
//! `E3001`; a read of a stored member is not. Each case runs on both
//! targets, in the C-HIR and EvoIR interpreters, through `--check`, and in
//! the EvoMIR interpreter on the lowered EvoIR that the driver emits.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use evocc_mir::{MirLimits, MirValue, Target};

const TARGETS: [&str; 2] = ["aarch64-unknown-linux-gnu", "aarch64-apple-darwin"];
const MODES: [&str; 3] = ["--interpret-c-hir", "--interpret-evoir", "--check"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

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

/// The EvoIR that the driver emits for `path` at `level`.
fn emitted(path: &Path, target: &str, level: &str) -> evocc_evoir::Module {
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .args(["-w", level, "--emit=evoir"])
        .arg(path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    evocc_evoir::parse_target_module(&output.stdout, target).unwrap()
}

fn machine_target(target: &str) -> Target {
    if target.contains("linux") {
        Target::Linux
    } else {
        Target::Darwin
    }
}

/// The EvoMIR interpreter on the lowered module: `main`'s value, or the code
/// of the diagnostic that stopped it, with the volatile trace.
fn machine(module: &evocc_evoir::Module, target: &str) -> (Result<i64, String>, Vec<String>) {
    let lowered = evocc_mir::lower_module_for(machine_target(target), module, &|_| false)
        .unwrap_or_else(|refusals| {
            panic!(
                "lowering refused: {:?}",
                refusals
                    .iter()
                    .map(|refusal| refusal.code)
                    .collect::<Vec<_>>()
            )
        });
    evocc_mir::validate_module(&lowered).unwrap();
    match evocc_mir::interpret_module(&lowered, "main", &[], MirLimits::new(1_000_000)) {
        Ok(outcome) => {
            let trace = outcome
                .world
                .volatile_trace()
                .iter()
                .map(|event| format!("{event:?}"))
                .collect();
            match outcome.results.first() {
                Some(MirValue::Int(raw)) => (Ok(i64::from(*raw as u32 as i32)), trace),
                other => (Err(format!("{other:?}")), trace),
            }
        }
        Err(error) => (Err(error.code()), Vec::new()),
    }
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
        for level in LEVELS {
            let (value, _) = machine(&emitted(&path, target, level), target);
            assert_eq!(value, Ok(expected), "{name} {target} {level} EvoMIR");
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
        // EvoMIR has no source positions; it stops with the same code.
        for level in LEVELS {
            let (value, _) = machine(&emitted(&path, target, level), target);
            assert_eq!(
                value,
                Err("E3001".to_owned()),
                "{name} {target} {level} EvoMIR"
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
/// EvoMIR passes a record argument in general registers with transport
/// accesses, which move the same state (D29), so every case runs in the
/// EvoMIR lane too.
#[test]
fn record_copies_keep_the_initializedness_of_each_bit() {
    let declaration = "struct S { unsigned a : 3; unsigned b : 5; };\n\
                       static unsigned first(struct S s) { return s.a; }\n\
                       static unsigned second(struct S s) { return s.b; }\n";
    completes(
        "local-copy",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) { struct S s; s.a = 6; struct S t = s; t.b = 1; struct S u = t; return u.a * 10 + u.b; }\n",
        61,
    );
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

/// D25: a store to a volatile bit-field reads its container in one volatile
/// access, but that read is not a C read of the other members. A read of a
/// volatile member that was never stored is still `E3001`.
#[test]
fn a_volatile_bit_field_store_is_not_a_read_of_its_neighbour() {
    for (name, declaration, object) in [
        (
            "volatile-members",
            "struct S { volatile unsigned a : 3; volatile unsigned b : 5; };\n",
            "struct S s;",
        ),
        (
            "volatile-object",
            "struct S { unsigned a : 3; unsigned b : 5; };\n",
            "volatile struct S s;",
        ),
    ] {
        completes(
            name,
            &format!("{declaration}int main(void) {{ {object} s.a = 1; return 0; }}\n"),
            0,
        );
        completes(
            &format!("{name}-stored"),
            &format!(
                "{declaration}int main(void) {{ {object} s.b = 17; s.a = 5; return s.a * 100 + s.b; }}\n"
            ),
            517,
        );
        uninitialized_read_at(
            &format!("{name}-never-stored"),
            &format!(
                "{declaration}int main(void) {{\n\
                 \x20   {object}\n\
                 \x20   s.a = 1;\n\
                 \x20   return s.b;\n\
                 }}\n"
            ),
            5,
            12,
        );
    }
    // Through a pointer to a volatile record, as device code writes it.
    completes(
        "volatile-pointer",
        "struct S { unsigned a : 3; unsigned b : 5; };\n\
         int main(void) { struct S s; volatile struct S *p = &s; p->a = 6; return p->a; }\n",
        6,
    );
}

/// A compound update of a volatile bit-field reads its own field, then
/// stores it.
#[test]
fn a_volatile_compound_update_reads_only_its_own_field() {
    let declaration = "struct S { volatile unsigned a : 3; volatile unsigned b : 5; };\n";
    completes(
        "volatile-compound",
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
        "volatile-compound-unstored",
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

/// Wide and packed volatile containers: the unit read of a store demands no
/// bit, and each unit read of a load demands only the field's bits there.
#[test]
fn volatile_fields_in_wide_and_packed_containers_initialize_only_their_bits() {
    let wide = "struct U { volatile unsigned p : 4; volatile unsigned q : 20; volatile unsigned r : 8; };\n";
    completes(
        "volatile-wide",
        &format!("{wide}int main(void) {{ struct U u; u.q = 0xabcde; return u.q == 0xabcde; }}\n"),
        1,
    );
    uninitialized_read_at(
        "volatile-wide-unstored",
        &format!(
            "{wide}int main(void) {{\n\
             \x20   struct U u;\n\
             \x20   u.q = 0xabcde;\n\
             \x20   return u.r;\n\
             }}\n"
        ),
        5,
        12,
    );
    let packed = "struct __attribute__((packed)) P {\n\
                  \x20   char c;\n\
                  \x20   volatile unsigned long long x : 41;\n\
                  \x20   volatile unsigned long long y : 15;\n\
                  };\n";
    completes(
        "volatile-packed",
        &format!(
            "{packed}int main(void) {{\n\
             \x20   struct P v;\n\
             \x20   v.x = 0x1abcdef0123ULL;\n\
             \x20   v.y = 0x5a5a;\n\
             \x20   return (v.x == 0x1abcdef0123ULL) + 2 * (v.y == 0x5a5a);\n\
             }}\n"
        ),
        3,
    );
    uninitialized_read_at(
        "volatile-packed-unstored",
        &format!(
            "{packed}int main(void) {{\n\
             \x20   struct P v;\n\
             \x20   v.x = 1;\n\
             \x20   return v.y;\n\
             }}\n"
        ),
        9,
        12,
    );
}

/// The interpreters see the same volatile accesses: each unit access of a
/// volatile bit-field is one event, and the EvoIR and EvoMIR traces of the
/// emitted module are equal at both levels.
#[test]
fn volatile_bit_field_traces_agree_in_evoir_and_evomir() {
    let work = Work::new("trace");
    let path = work.source(
        "trace.c",
        "struct S { volatile unsigned a : 3; volatile unsigned b : 5; volatile unsigned short c : 9; };\n\
         int main(void) { struct S s; s.a = 5; s.c = 300; s.a += 1; return s.a + s.c; }\n",
    );
    for target in TARGETS {
        let layout = if target.contains("linux") {
            evocc_types::TargetDataLayout::aarch64_unknown_linux_gnu()
        } else {
            evocc_types::TargetDataLayout::aarch64_apple_darwin()
        };
        for level in LEVELS {
            let module = emitted(&path, target, level);
            let main = module
                .functions
                .iter()
                .find(|function| function.name == "main")
                .unwrap();
            let arguments = main
                .params
                .iter()
                .map(|parameter| evocc_evoir::RuntimeValue::new(parameter.ty.clone(), 0))
                .collect::<Vec<_>>();
            let outcome = evocc_evoir::interpret_module_with_target(
                &module,
                "main",
                &arguments,
                &layout,
                evocc_evoir::InterpreterLimits::new(1_000_000),
            )
            .unwrap();
            let evoir = outcome
                .world
                .volatile_trace()
                .iter()
                .map(|event| format!("{event:?}"))
                .collect::<Vec<_>>();
            // a = 5: a container read and a store; c = 300: the same; a += 1:
            // a read of a, a container read and a store; the return reads a
            // and c.
            assert_eq!(evoir.len(), 9, "{target} {level}: {evoir:#?}");
            let (value, machine_trace) = machine(&module, target);
            assert_eq!(value, Ok(306), "{target} {level}");
            assert_eq!(machine_trace, evoir, "{target} {level}");
        }
    }
}

/// Native code keeps one access of each unit: the masks of the volatile
/// accesses change no instruction.
#[test]
fn volatile_bit_field_accesses_carry_masks_that_change_no_instruction() {
    let work = Work::new("masks");
    let path = work.source(
        "masks.c",
        "struct S { volatile unsigned a : 3; volatile unsigned b : 5; };\n\
         unsigned f(struct S *s) { s->a = 1; return s->b; }\n",
    );
    for target in TARGETS {
        let ir = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg(format!("--target={target}"))
            .args(["-O2", "--emit=evoir"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(ir.status.success(), "{}", text(&ir));
        let ir = String::from_utf8_lossy(&ir.stdout);
        // The store's container read demands no bit, the store initializes
        // bits 0 to 2, and the read of b demands bits 3 to 7.
        for form in ["[mask=0]", "[mask=7]", "[mask=248]"] {
            assert!(ir.contains(form), "{target} missing {form}: {ir}");
        }
        assert_eq!(ir.matches("load volatile").count(), 2, "{ir}");
        assert_eq!(ir.matches("store volatile").count(), 1, "{ir}");
        let asm = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .arg(format!("--target={target}"))
            .args(["-O2", "--emit=asm"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(asm.status.success(), "{}", text(&asm));
        let asm = String::from_utf8_lossy(&asm.stdout);
        // One load and one store of the 4-byte unit for the store of a, and
        // one load of the unit for the read of b.
        let count = |mnemonic: &str| {
            asm.lines()
                .filter(|line| line.trim_start().starts_with(mnemonic))
                .count()
        };
        assert_eq!(count("ldr w"), 2, "{asm}");
        assert_eq!(count("str w"), 1, "{asm}");
    }
}
