//! Execute the N03f interactions that text reconstruction cannot establish:
//! unwind through aligned/dynamic frames, all return paths and canary failure.
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

const SUBJECT: &str = r#"
extern long work(long);
extern void observe(void *, unsigned long);
extern void alter_guard(int);
long paired(long n) {
    long a=work(n), b=work(a), c=work(b), d=work(c);
    return a+b+c+d;
}
long aligned(long n) {
    _Alignas(64) unsigned char p[80];
    long a=work(n), b=work(a);
    observe(p, 64);
    if(n == 1) return a+b;
    return b+p[0];
}
long dynamic(long n) {
    unsigned char p[n]; unsigned char *q=__builtin_alloca(n+16);
    long a=work(n), b=work(a);
    observe(p, 1); observe(q, 16);
    if(n == 32) return a+b+p[0]+q[0];
    return a+p[0];
}
long variadic(int n, ...) {
    __builtin_va_list ap; __builtin_va_start(ap,n);
    long s=work(n);
    for(int i=0;i<n;i++) s+=__builtin_va_arg(ap,long);
    __builtin_va_end(ap); return s;
}
unsigned long leaf(unsigned long n) { return n+7; }
int guarded(int n) {
    unsigned char p[32]; observe(p, 1); alter_guard(n);
    if(n) return 1;
    return p[0]-9;
}
"#;

const HARNESS: &str = r#"
#include <stdint.h>
#include <stdlib.h>
#include <stdio.h>
#include <unwind.h>
uintptr_t __stack_chk_guard = 0x1234abcddcba4321UL;
void __stack_chk_fail(void) { _Exit(77); }
static _Unwind_Reason_Code visit(struct _Unwind_Context *ctx, void *arg) {
    if (_Unwind_GetIP(ctx)) ++*(int *)arg;
    return _URC_NO_REASON;
}
static void unwind_check(void) {
    static unsigned calls;
    int n=0; int status=_Unwind_Backtrace(visit,&n); ++calls;
    if(n<3) { fprintf(stderr,"unwind call=%u frames=%d status=%d\n",calls,n,status); _Exit(90); }
}
long work(long n) { unwind_check(); return n+1; }
void observe(void *p, unsigned long a) {
    if((uintptr_t)p%a) _Exit(91);
    unwind_check(); *(unsigned char *)p=9;
}
void alter_guard(int n) { if(n) __stack_chk_guard ^= 0x55; }
extern long paired(long), aligned(long), dynamic(long), variadic(int,...);
extern unsigned long leaf(unsigned long);
extern int guarded(int);
int main(int argc, char **argv) {
    (void)argv;
    if(paired(1)!=14 || aligned(1)!=5 || aligned(2)!=13) return 92;
    if(dynamic(32)!=85 || dynamic(17)!=27 || leaf(9)!=16) return 93;
#ifndef DARWIN
    if(variadic(3,5L,7L,11L)!=27) return 94;
#endif
    /* Keep main in the unwind chain even when the reference inlines helpers. */
    volatile int result=guarded(argc>1);
    return result;
}
"#;

fn success(output: Output) -> Vec<u8> {
    assert!(
        output.status.success(),
        "status={} stdout={} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

struct Scratch(PathBuf);
impl Scratch {
    fn new(lane: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let path = root.join(format!(
            ".evidence/codegen/n03f/native-{lane}-{}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "retained minimal failed frame interaction: {}",
                self.0.display()
            );
            return;
        }
        // This invocation exclusively created this exact directory.
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn assembly(linux: bool, variant: Option<&str>, signed: bool) -> Vec<u8> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.args(["--emit=evoir", "-O2"]);
    if linux {
        command.arg("-fstack-protector-strong");
    }
    let mut child = command
        .arg("/dev/stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(SUBJECT.as_bytes())
        .unwrap();
    let ir = success(child.wait_with_output().unwrap());
    let module = evocc_evoir::parse_section9_module(&ir).unwrap();
    evocc_mir::compile_module_with_options(
        if linux {
            evocc_mir::Target::Linux
        } else {
            evocc_mir::Target::Darwin
        },
        &module,
        &|_| true,
        evocc_mir::RelocationModel::PositionIndependent,
        &evocc_mir::emit::CodegenOptions {
            frame: variant
                .map(|v| evocc_mir::emit::FrameVariants::parse(v).unwrap())
                .unwrap_or_default(),
            unwind_tables: true,
            sign_returns: signed,
            ..Default::default()
        },
    )
    .unwrap()
    .into_bytes()
}

fn run(linux: bool) {
    let scratch = Scratch::new(if linux { "linux" } else { "darwin" });
    std::fs::write(scratch.0.join("harness.c"), HARNESS).unwrap();
    std::fs::write(scratch.0.join("reference.c"), SUBJECT).unwrap();
    if !linux {
        success(
            Command::new("clang")
                .current_dir(&scratch.0)
                .args([
                    "-O2",
                    "-fno-stack-protector",
                    "-funwind-tables",
                    "-DDARWIN",
                    "harness.c",
                    "reference.c",
                    "-o",
                    "reference",
                ])
                .output()
                .unwrap(),
        );
        success(Command::new(scratch.0.join("reference")).output().unwrap());
    }
    // The frozen plan executes signing/unwinding interactions under GCC-14.
    // This host's unsigned harness cannot unwind even a Clang-built PAC-ret
    // subject (exit 90, first callback sees only two frames). Do not turn that
    // failed reference control into a PASS. Darwin's additional execution
    // coverage here is unsigned; the signed text matrix remains independent.
    let signing_modes: &[bool] = if linux { &[false, true] } else { &[false] };
    for &signed in signing_modes {
        for variant in [
            None,
            Some("paired-saves"),
            Some("paired-saves,shared-epilogue"),
            Some("paired-saves,shared-epilogue,leaf-frames"),
        ] {
            eprintln!(
                "frame-interaction starting linux={linux} signed={signed} variant={variant:?}"
            );
            std::fs::write(
                scratch.0.join("subject.s"),
                assembly(linux, variant, signed),
            )
            .unwrap();
            if linux {
                let output=Command::new("/usr/local/bin/container")
                    .args(["run","--rm","--platform","linux/arm64","--network","none","-v"])
                    .arg(format!("{}:/work",scratch.0.display()))
                    .args(["evo-flight-lab:sealed","bash","-c",
                        "set -eu; cd /work; gcc-14 -O2 -fno-stack-protector -funwind-tables harness.c subject.s -o probe; ./probe; set +e; ./probe corrupt; code=$?; test \"$code\" -eq 77"])
                    .output().unwrap();
                success(output);
            } else {
                success(
                    Command::new("clang")
                        .current_dir(&scratch.0)
                        .args([
                            "-O2",
                            "-fno-stack-protector",
                            "-funwind-tables",
                            "-DDARWIN",
                            "harness.c",
                            "subject.s",
                            "-o",
                            "probe",
                        ])
                        .output()
                        .unwrap(),
                );
                success(Command::new(scratch.0.join("probe")).output().unwrap());
            }
            eprintln!(
                "frame-interaction lane={} signed={signed} variants={} PASS",
                if linux { "linux" } else { "darwin" },
                variant.unwrap_or("off")
            );
        }
    }
}

#[test]
fn darwin_unwind_and_frame_interactions_execute() {
    run(false);
}

#[test]
#[ignore = "N03f sealed Linux interaction lane; run explicitly and serially"]
fn linux_unwind_and_guard_interactions_execute() {
    run(true);
}
