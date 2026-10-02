//! N03f: opt-in, emission-only frame variants.
//!
//! Every variant defaults off. Each test lowers C once through the unchanged
//! frontend and compiles the same EvoIR with and without a variant, so the
//! difference is the variant and nothing else. Paired saves are checked by
//! reconstruction: splitting every emitted pair back into its two single
//! transfers must give exactly the default text, which proves that no offset,
//! order, CFI value or other instruction moved.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use evocc_mir::emit::{CodegenOptions, FrameVariants, StackGuard};
use evocc_mir::{compile_module_with_options, RelocationModel, Target};

fn run(arguments: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(arguments)
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
        .write_all(source.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn successful(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn evoir(flags: &[&str], source: &str) -> String {
    let mut arguments = vec!["--emit=evoir"];
    arguments.extend(flags);
    successful(run(&arguments, source))
}

fn outcome(target: Target, ir: &str, options: &CodegenOptions) -> Result<String, String> {
    let module = evocc_evoir::parse_section9_module(ir.as_bytes()).expect("canonical EvoIR");
    compile_module_with_options(
        target,
        &module,
        &|_| true,
        RelocationModel::PositionIndependent,
        options,
    )
    .map_err(|stop| format!("{} {}", stop.component(), stop.detail()))
}

fn compile(target: Target, ir: &str, options: &CodegenOptions) -> String {
    outcome(target, ir, options).unwrap_or_else(|stop| panic!("{stop}"))
}

fn with(frame: FrameVariants, base: &CodegenOptions) -> CodegenOptions {
    CodegenOptions {
        frame,
        ..base.clone()
    }
}

const PAIRED: FrameVariants = FrameVariants {
    paired_saves: true,
    ..FrameVariants::OFF
};

/// The header line naming the effective variants is identity, not code.
fn without_header(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("// evocc-native-frame="))
        .map(|line| format!("{line}\n"))
        .collect()
}

/// Undo pairing: `stp a, b, [sp, #o]` followed by its CFI lines becomes the
/// two single stores, each followed by its own CFI line, exactly as the
/// default emission writes them; `ldp` likewise with `.cfi_restore`.
fn split_pairs(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        let pair = line
            .strip_prefix("    stp ")
            .map(|rest| ("str", rest))
            .or_else(|| line.strip_prefix("    ldp ").map(|rest| ("ldr", rest)));
        let parsed = pair.and_then(|(single, rest)| {
            let (registers, address) = rest.split_once(", [sp, #")?;
            let (first, second) = registers.split_once(", ")?;
            let offset: u64 = address.strip_suffix(']')?.parse().ok()?;
            Some((single, first, second, offset))
        });
        let Some((single, first, second, offset)) = parsed else {
            out.push_str(line);
            out.push('\n');
            index += 1;
            continue;
        };
        if first == "x29" || first.starts_with('q') {
            // The frame record and the variadic vector save area are not
            // callee-saved pairs.
            out.push_str(line);
            out.push('\n');
            index += 1;
            continue;
        }
        index += 1;
        let mut cfi = Vec::new();
        while index < lines.len() && lines[index].starts_with(".cfi_") && cfi.len() < 2 {
            cfi.push(lines[index]);
            index += 1;
        }
        out.push_str(&format!("    {single} {first}, [sp, #{offset}]\n"));
        if let Some(line) = cfi.first() {
            out.push_str(line);
            out.push('\n');
        }
        out.push_str(&format!("    {single} {second}, [sp, #{}]\n", offset + 8));
        if let Some(line) = cfi.get(1) {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn callee_pairs(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            (line.starts_with("    stp ") || line.starts_with("    ldp "))
                && !line.contains("x29, x30")
                && !line.contains(" q")
        })
        .count()
}

const FRAME_SPILLS: &str = "extern void use(void *, long);
extern long work(long);
long frame(long n)
{
    char buf[40];
    long arr[7];
    long a = work(n), b = work(a), c = work(b), d = work(c), e = work(d), f = work(e);
    long g = work(f), h = work(g), i = work(h), j = work(i), k = work(j), l = work(k);
    use(buf, a);
    use(arr, b);
    return a + b + c + d + e + f + g + h + i + j + k + l;
}
long odd(long n) { long a = work(n), b = work(a), c = work(b), d = work(c); return a + b + c + d; }
long many(long n, int c)
{
    long a = work(n), b = work(a), d = work(b);
    if (c == 1) return a;
    if (c == 2) return a + b;
    if (c == 3) { use(0, d); return d; }
    return a + b + d + work(d);
}";

const FAR_SAVES: &str = "extern void use(void *, long);
extern long work(long);
long far(long n) { char buf[4000]; long a = work(n), b = work(a), c = work(b), d = work(c); use(buf, a); return a + b + c + d; }
long edge(long n) { char buf[440]; long a = work(n), b = work(a), c = work(b), d = work(c), e = work(d), f = work(e); use(buf, a); return a + b + c + d + e + f; }";

const DYNAMIC: &str = "extern void use(void *, long);
extern long work(long);
long dyn(long n) { char vla[n]; char *p = __builtin_alloca(n * 2); long a = work(n), b = work(a); use(vla, a); use(p, b); return a + b; }";

const ALIGNED: &str = "extern void use(void *, long);
extern long work(long);
long aligned(long n) { _Alignas(64) char buf[64]; long a = work(n), b = work(a), c = work(b); use(buf, a); return a + b + c; }";

const GUARDED: &str = "extern void use(void *, long);
extern long work(long);
int prot(int c) { char a[16]; long x = work(c), y = work(x); use(a, x); if (c) return 1 + y; return 2 + x; }";

const VECTORS: &str = "extern void use(void *, long);
long vclob(long n) { __asm__ volatile(\"\" ::: \"v8\", \"v9\", \"v15\", \"x19\", \"x20\", \"x21\"); use(0, n); return n; }
long vpair(long n) { __asm__ volatile(\"\" ::: \"v10\", \"v11\", \"v12\", \"v13\"); use(0, n); return n; }";

const VARIADIC: &str = "extern long work(long);
long vsum(int n, ...)
{
    __builtin_va_list ap;
    __builtin_va_start(ap, n);
    long s = 0, t = work(n), u = work(t);
    for (int i = 0; i < n; i++) s += __builtin_va_arg(ap, long);
    __builtin_va_end(ap);
    return s + t + u;
}";

fn option_sets() -> Vec<(&'static str, CodegenOptions)> {
    vec![
        ("plain", CodegenOptions::default()),
        (
            "unwind",
            CodegenOptions {
                unwind_tables: true,
                ..CodegenOptions::default()
            },
        ),
        (
            "unwind-signed",
            CodegenOptions {
                unwind_tables: true,
                sign_returns: true,
                ..CodegenOptions::default()
            },
        ),
        (
            "general-only",
            CodegenOptions {
                general_registers_only: true,
                ..CodegenOptions::default()
            },
        ),
        (
            "sysreg-guard",
            CodegenOptions {
                stack_guard: StackGuard::SystemRegister {
                    register: "sp_el0".into(),
                    offset: 64,
                },
                ..CodegenOptions::default()
            },
        ),
    ]
}

#[test]
fn frame_variants_are_explicit_driver_options_defaulting_off() {
    let source = "extern long work(long); long f(long n) { long a = work(n), b = work(a); return a + b + work(b); }";
    let default = successful(run(&["--emit=asm"], source));
    assert!(!default.contains("evocc-native-frame"), "{default}");
    let paired = successful(run(&["--emit=asm", "--frame-opt=paired-saves"], source));
    assert!(
        paired.contains("// evocc-native-frame=paired-saves\n"),
        "{paired}"
    );
    assert_eq!(split_pairs(&without_header(&paired)), default);
    for refused in [
        vec!["--emit=asm", "--frame-opt="],
        vec!["--emit=asm", "--frame-opt=unknown"],
        vec!["--emit=asm", "--frame-opt=paired-saves,paired-saves"],
        vec![
            "--emit=asm",
            "--frame-opt=paired-saves",
            "--frame-opt=paired-saves",
        ],
        vec!["--check", "--frame-opt=paired-saves"],
    ] {
        assert!(!run(&refused, source).status.success(), "{refused:?}");
    }
    assert_eq!(
        FrameVariants::parse("paired-saves").unwrap(),
        PAIRED,
        "the library parser is the driver's"
    );
    assert_eq!(CodegenOptions::default().frame, FrameVariants::OFF);
}

#[test]
fn paired_saves_only_merge_adjacent_slots_at_unchanged_offsets() {
    let mut pairs = 0;
    for (name, source, flags) in [
        ("frame_spills", FRAME_SPILLS, &[][..]),
        ("far_saves", FAR_SAVES, &[][..]),
        ("dynamic", DYNAMIC, &[][..]),
        ("aligned", ALIGNED, &[][..]),
        ("guarded", GUARDED, &["-fstack-protector-strong"][..]),
        ("vectors", VECTORS, &[][..]),
        ("variadic", VARIADIC, &[][..]),
    ] {
        let ir = evoir(flags, source);
        for (options_name, options) in option_sets() {
            for target in [Target::Darwin, Target::Linux] {
                // A refusal is the same refusal with the variant on.
                let default = match outcome(target, &ir, &options) {
                    Ok(text) => text,
                    Err(stop) => {
                        assert_eq!(
                            outcome(target, &ir, &with(PAIRED, &options)),
                            Err(stop),
                            "{name}/{options_name}/{target:?}"
                        );
                        continue;
                    }
                };
                let paired = compile(target, &ir, &with(PAIRED, &options));
                assert!(
                    paired.contains("// evocc-native-frame=paired-saves\n"),
                    "{name}/{options_name}"
                );
                assert_eq!(
                    split_pairs(&without_header(&paired)),
                    default,
                    "{name}/{options_name}/{target:?}:\n{paired}"
                );
                pairs += callee_pairs(&paired);
            }
        }
    }
    assert!(pairs > 0, "no shape exercised a pair");
}

#[test]
fn pairs_follow_the_encodable_adjacent_slots() {
    let ir = evoir(&[], FRAME_SPILLS);
    let paired = compile(
        Target::Linux,
        &ir,
        &with(PAIRED, &CodegenOptions::default()),
    );
    let body = |name: &str| -> String {
        paired
            .split(&format!("\n{name}:\n"))
            .nth(1)
            .unwrap()
            .split(&format!(".size {name},"))
            .next()
            .unwrap()
            .to_owned()
    };
    // Ten callee-saved registers: five pairs saved and five restored.
    let frame = body("frame");
    assert_eq!(
        frame
            .lines()
            .filter(|l| l.starts_with("    stp x") && !l.contains("x29"))
            .count(),
        5,
        "{frame}"
    );
    assert!(
        !frame.contains("    str x19") && !frame.contains("    ldr x19"),
        "{frame}"
    );
    // An odd count keeps one single transfer at the highest slot.
    let odd = body("odd");
    let singles = odd
        .lines()
        .filter(|l| {
            (l.starts_with("    str x") || l.starts_with("    ldr x")) && l.contains("[sp, #")
        })
        .count();
    assert_eq!(singles, 2, "{odd}");
    // Slots beyond the signed scaled seven-bit offset stay single transfers.
    let far_ir = evoir(&[], FAR_SAVES);
    let far = compile(
        Target::Linux,
        &far_ir,
        &with(PAIRED, &CodegenOptions::default()),
    );
    let far_body = far
        .split("\nfar:\n")
        .nth(1)
        .unwrap()
        .split(".size far,")
        .next()
        .unwrap();
    assert_eq!(callee_pairs(far_body), 0, "{far_body}");
    // Dynamic frames address saves from x29 and keep single transfers.
    let dynamic = compile(
        Target::Linux,
        &evoir(&[], DYNAMIC),
        &with(PAIRED, &CodegenOptions::default()),
    );
    assert_eq!(callee_pairs(&dynamic), 0, "{dynamic}");
    // Partially preserved vector halves pair as D registers.
    let vectors = compile(
        Target::Linux,
        &evoir(&[], VECTORS),
        &with(PAIRED, &CodegenOptions::default()),
    );
    assert!(vectors.contains("    stp d8, d9, [sp, #"), "{vectors}");
    assert!(vectors.contains("    ldp d10, d11, [sp, #"), "{vectors}");
}

fn linux_body<'a>(text: &'a str, name: &str) -> &'a str {
    text.split(&format!("\n{name}:\n"))
        .nth(1)
        .unwrap()
        .split(&format!(".size {name},"))
        .next()
        .unwrap()
}

#[test]
fn shared_epilogues_keep_one_complete_exit_and_leave_single_returns_unchanged() {
    let shared = FrameVariants::parse("shared-epilogue").unwrap();
    for (source, flags, name) in [
        (FRAME_SPILLS, &[][..], "many"),
        (GUARDED, &["-fstack-protector-strong"][..], "prot"),
    ] {
        let ir = evoir(flags, source);
        let options = CodegenOptions {
            unwind_tables: true,
            sign_returns: true,
            ..CodegenOptions::default()
        };
        let before = compile(Target::Linux, &ir, &options);
        let after = compile(Target::Linux, &ir, &with(shared, &options));
        let old_body = linux_body(&before, name);
        let body = linux_body(&after, name);
        let returns = old_body.matches("    ret\n").count();
        assert!(
            returns >= 2,
            "fixture must retain multiple returns: {old_body}"
        );
        let label = format!(".L{name}_frame_exit");
        assert_eq!(body.matches("    ret\n").count(), 1, "{body}");
        assert_eq!(body.matches(&format!("{label}:\n")).count(), 1);
        assert_eq!(
            body.matches(&format!("    b {label}\n")).count(),
            returns - 1
        );
        assert_eq!(body.matches(".cfi_remember_state\n").count(), 1);
        assert_eq!(body.matches(".cfi_restore_state\n").count(), 1);
        assert_eq!(body.matches("    autiasp\n").count(), 1);
        let exit = body.split(&format!("{label}:\n")).nth(1).unwrap();
        assert!(exit.find("ldp x29, x30").unwrap() < exit.find("autiasp").unwrap());
        if name == "prot" {
            assert_eq!(body.matches("bl __stack_chk_fail").count(), 1);
            assert!(exit.find("bl __stack_chk_fail").unwrap() < exit.find("ldp x29, x30").unwrap());
        } else {
            assert_eq!(linux_body(&before, "odd"), linux_body(&after, "odd"));
            assert_eq!(linux_body(&before, "frame"), linux_body(&after, "frame"));
        }
    }
}

#[test]
fn protected_profiles_decline_all_requested_frame_variants() {
    for flags in [
        vec!["-fno-omit-frame-pointer"],
        vec!["-mno-omit-leaf-frame-pointer"],
        vec!["-mgeneral-regs-only"],
        vec![
            "-mstack-protector-guard=sysreg",
            "-mstack-protector-guard-reg=sp_el0",
            "-mstack-protector-guard-offset=64",
        ],
        vec!["-fpatchable-function-entry=2"],
    ] {
        let mut base = vec!["--emit=asm"];
        base.extend(flags.iter().copied());
        let before = successful(run(&base, FRAME_SPILLS));
        base.push("--frame-opt=paired-saves");
        let after = successful(run(&base, FRAME_SPILLS));
        assert_eq!(without_header(&after), before, "{flags:?}");
    }
}

#[test]
fn shared_epilogue_label_survives_conditional_branch_relaxation() {
    // Enough dependent, live instructions to force the emitter's second pass.
    let mut ir = String::from("func @far_exit(%arg0: bits<1>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: bits<1>, %v1: bits<32>, %v2: mem<all>, %v3: world):\n    cond_br %v0, ^b2(%v1, %v2, %v3), ^b1(%v1, %v2, %v3)\n^b1(%v4: bits<32>, %v5: mem<all>, %v6: world):\n");
    let mut last = 4;
    for next in 100..5100 {
        ir.push_str(&format!("    %v{next} = add.bits<32> %v{last}, %v{last}\n"));
        last = next;
    }
    ir.push_str(&format!("    ret %v{last}, %v5, %v6\n^b2(%v7: bits<32>, %v8: mem<all>, %v9: world):\n    ret %v7, %v8, %v9\n}}\n"));
    let options = with(
        FrameVariants::parse("shared-epilogue").unwrap(),
        &CodegenOptions::default(),
    );
    for target in [Target::Linux, Target::Darwin] {
        let after = compile(target, &ir, &options);
        assert!(after.contains("_relax0:"), "conditional was not relaxed");
        assert_eq!(after.matches("_frame_exit:\n").count(), 1);
        assert_eq!(after.matches("    ret\n").count(), 1);
    }
}

#[test]
fn leaf_frames_remove_only_the_unneeded_record_and_preserve_authentication() {
    let ir = evoir(
        &["-O2"],
        "unsigned long leaf(unsigned long x) { return x + 7; }",
    );
    let leaf = FrameVariants::parse("leaf-frames").unwrap();
    for target in [Target::Linux, Target::Darwin] {
        for signed in [false, true] {
            let options = CodegenOptions {
                unwind_tables: true,
                sign_returns: signed,
                ..CodegenOptions::default()
            };
            let before = compile(target, &ir, &options);
            assert!(before.contains("stp x29, x30"));
            let after = compile(target, &ir, &with(leaf, &options));
            assert!(!after.contains("stp x29, x30"), "{after}");
            assert!(!after.contains("ldp x29, x30"), "{after}");
            assert!(!after.contains("mov x29, sp"), "{after}");
            assert!(!after.contains(".cfi_def_cfa 29"), "{after}");
            assert!(after.contains(".cfi_startproc"));
            assert!(after.contains(".cfi_endproc"));
            assert_eq!(after.matches("paciasp").count(), usize::from(signed));
            assert_eq!(after.matches("autiasp").count(), usize::from(signed));
            if signed {
                assert!(after.find("paciasp").unwrap() < after.find("autiasp").unwrap());
                assert_eq!(after.matches(".cfi_negate_ra_state").count(), 2);
            }
        }
    }
}

#[test]
fn leaf_frames_keep_activation_stack_calls_and_opaque_assembly() {
    let leaf = FrameVariants::parse("leaf-frames").unwrap();
    for source in [
        "extern long work(long); long f(long n) { return work(n); }",
        "void *f(void) { return __builtin_frame_address(0); }",
        "void *f(void) { return __builtin_return_address(0); }",
        "void *f(void) { return __builtin_frame_address(1); }",
        "long f(long n) { __asm__ volatile(\"nop\"); return n; }",
        "long f(long n) { volatile long a[2]; a[1] = n; return a[1]; }",
        "long f(long n) { volatile char a[n]; a[0] = 1; return a[0]; }",
        "long f(long a,long b,long c,long d,long e,long f,long g,long h,long i) { return i; }",
        "long f(int n, ...) { __builtin_va_list ap; __builtin_va_start(ap,n); long x=__builtin_va_arg(ap,long); __builtin_va_end(ap); return x; }",
    ] {
        let ir = evoir(&["-O2"], source);
        let before = compile(Target::Linux, &ir, &CodegenOptions::default());
        let after = compile(Target::Linux, &ir, &with(leaf, &CodegenOptions::default()));
        assert_eq!(without_header(&after), before, "{source}");
        assert!(after.contains("stp x29, x30"), "{source}");
    }
}

#[test]
fn frame_pointer_requests_have_ordered_and_independent_meanings() {
    let source = "unsigned long f(unsigned long n) { return n + 7; }";
    for (flags, retained) in [
        (vec!["-fno-omit-frame-pointer"], true),
        (
            vec!["-fno-omit-frame-pointer", "-fomit-frame-pointer"],
            false,
        ),
        (
            vec!["-fomit-frame-pointer", "-fno-omit-frame-pointer"],
            true,
        ),
        (
            vec!["-mno-omit-leaf-frame-pointer", "-fomit-frame-pointer"],
            true,
        ),
    ] {
        let mut args = vec!["--emit=asm", "-O2", "--frame-opt=leaf-frames"];
        args.extend(flags);
        let assembly = successful(run(&args, source));
        assert_eq!(
            assembly.contains("stp x29, x30"),
            retained,
            "{args:?}\n{assembly}"
        );
    }
}
