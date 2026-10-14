//! EvoMIR Section 6.7: an `asm` clobber of a callee-saved general register
//! is saved in the prologue and restored in every epilogue, so a caller that
//! another compiler built keeps the value it holds in that register (T03g).
//!
//! The host lane links an EvoCC callee with an Apple clang caller. The sealed
//! Linux lane links the same callee, compiled for Linux, with a GCC-14 caller
//! in the container; it is ignored by default and runs explicitly.

use std::path::PathBuf;
use std::process::{Command, Output};

/// The callees. Each one returns normally after its `asm` writes the named
/// registers.
const CALLEE: &str = r#"
void clobber_x19(void) { __asm__ volatile("mov x19, #5" ::: "x19"); }
void clobber_callee_saved(void)
{
    __asm__ volatile("mov x19, #1\n\tmov x20, #1\n\tmov x21, #1\n\tmov x22, #1\n\t"
                     "mov x23, #1\n\tmov x24, #1\n\tmov x25, #1\n\tmov x26, #1\n\t"
                     "mov x27, #1\n\tmov x28, #1"
                     ::: "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26",
                         "x27", "x28");
}
void clobber_w20(int n)
{
    if (n)
        __asm__ volatile("mov w20, #9" ::: "w20");
}
void clobber_link(void) { __asm__ volatile("mov x30, xzr" ::: "x30"); }
"#;

/// The caller keeps values in `x19` to `x28` across each call. It returns 42
/// when every value survives.
const CALLER: &str = r#"
void clobber_x19(void);
void clobber_callee_saved(void);
void clobber_w20(int);
void clobber_link(void);
static int one(void)
{
    register long v __asm__("x19") = 42;
    __asm__ volatile("" : "+r"(v));
    clobber_x19();
    __asm__ volatile("" : "+r"(v));
    return v == 42;
}
static int all(void)
{
    register long a __asm__("x19") = 19;
    register long b __asm__("x20") = 20;
    register long c __asm__("x21") = 21;
    register long d __asm__("x22") = 22;
    register long e __asm__("x23") = 23;
    register long f __asm__("x24") = 24;
    register long g __asm__("x25") = 25;
    register long h __asm__("x26") = 26;
    register long i __asm__("x27") = 27;
    register long j __asm__("x28") = 28;
    __asm__ volatile("" : "+r"(a), "+r"(b), "+r"(c), "+r"(d), "+r"(e),
                          "+r"(f), "+r"(g), "+r"(h), "+r"(i), "+r"(j));
    clobber_callee_saved();
    clobber_w20(1);
    clobber_link();
    __asm__ volatile("" : "+r"(a), "+r"(b), "+r"(c), "+r"(d), "+r"(e),
                          "+r"(f), "+r"(g), "+r"(h), "+r"(i), "+r"(j));
    return a + b + c + d + e + f + g + h + i + j == 235;
}
int main(void) { return one() && all() ? 42 : 1; }
"#;

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A fresh directory with the two sources, removed on success.
struct Work(PathBuf);

impl Work {
    fn new(lane: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-asm-callee-saved-{lane}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("callee.c"), CALLEE).unwrap();
        std::fs::write(path.join("caller.c"), CALLER).unwrap();
        Self(path)
    }

    fn evocc(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
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

#[cfg(target_os = "macos")]
#[test]
fn callee_saved_assembly_clobbers_survive_a_clang_caller() {
    let work = Work::new("darwin");
    succeeded(
        &Command::new("clang")
            .current_dir(&work.0)
            .args(["-O1", "-c", "caller.c", "-o", "caller.o"])
            .output()
            .unwrap(),
    );
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[level, "-c", "callee.c", "-o", "callee.o"]));
        succeeded(
            &Command::new("clang")
                .current_dir(&work.0)
                .args(["caller.o", "callee.o", "-o", "program"])
                .output()
                .unwrap(),
        );
        let run = Command::new(work.0.join("program")).output().unwrap();
        assert_eq!(run.status.code(), Some(42), "{level}: {run:?}");
    }
}

/// Both ABI profiles save the clobbered registers in the prologue and restore
/// them before the return.
#[test]
fn both_profiles_save_and_restore_the_clobbered_registers() {
    let work = Work::new("text");
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let output = work.evocc(&[
            &format!("--target={target}"),
            "-O2",
            "-S",
            "callee.c",
            "-o",
            "-",
        ]);
        succeeded(&output);
        let text = String::from_utf8(output.stdout).unwrap();
        let symbol = if target.contains("darwin") {
            "_clobber_x19:"
        } else {
            "clobber_x19:"
        };
        let body = &text[text.find(symbol).expect("clobber_x19 is emitted")..];
        let body = &body[..body.find("ret").expect("a return")];
        let save = body.find("str x19,").expect("x19 is saved");
        let clobber = body.find("mov x19, #5").expect("the template");
        let restore = body.find("ldr x19,").expect("x19 is restored");
        assert!(save < clobber && clobber < restore, "{target}:\n{body}");
    }
}

/// The frame record and every frame address use `x29`, so an `asm` that
/// clobbers it is refused (EvoMIR Section 10, `E4009`), as GCC refuses it.
#[test]
fn an_assembly_clobber_of_the_frame_register_is_refused() {
    let work = Work::new("frame");
    std::fs::write(
        work.0.join("frame.c"),
        "void f(void) { __asm__ volatile(\"mov x29, sp\" ::: \"x29\"); }\n",
    )
    .unwrap();
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let output = work.evocc(&[
            &format!("--target={target}"),
            "-S",
            "frame.c",
            "-o",
            "frame.s",
        ]);
        assert_eq!(output.status.code(), Some(1), "{target}: {output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("E4009"),
            "{target}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "sealed Linux lane; run explicitly and serially"]
fn callee_saved_assembly_clobbers_survive_a_gcc_caller_in_the_sealed_container() {
    let work = Work::new("linux");
    for level in ["-O0", "-O2"] {
        succeeded(&work.evocc(&[
            "--target=aarch64-unknown-linux-gnu",
            level,
            "-S",
            "callee.c",
            "-o",
            "callee.s",
        ]));
        let output = Command::new("/usr/local/bin/container")
            .args([
                "run",
                "--rm",
                "--platform",
                "linux/arm64",
                "--network",
                "none",
                "-v",
            ])
            .arg(format!("{}:/work", work.0.display()))
            .args([
                "evo-nexlab:sealed",
                "bash",
                "-c",
                "set -eu; cd /work; gcc-14 -O1 caller.c callee.s -o program; \
                 set +e; ./program; echo \"exit=$?\"",
            ])
            .output()
            .unwrap();
        succeeded(&output);
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("exit=42"),
            "{level}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
