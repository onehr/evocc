//! Under `-mstrict-align`, every access that EvoCC emits is aligned to its
//! width (EvoMIR 8.9 and 9.5, D31). A record of at most sixteen bytes moves
//! through general registers in pieces no wider than the alignment of their
//! addresses, and a slot that the lowering gives such a record is aligned to
//! eight bytes. Each record fixture compiles on both targets at -O0 and -O2
//! under the strict-alignment check (`E4022`), and a scan of the emitted
//! assembly finds no frame access below its width. The EvoMIR interpreter runs
//! the strict lowering, and on Darwin the native programs return what Apple
//! clang's programs return. An ignored test runs the Linux programs in QEMU
//! with alignment checking on.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use evocc_mir::{MirLimits, MirValue, Target};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const TARGETS: [&str; 2] = [LINUX, "aarch64-apple-darwin"];
const LEVELS: [&str; 2] = ["-O0", "-O2"];

/// The fixtures that move records in general registers, with the targets that
/// compile them. Before D31 the check stopped all but two of them, on every
/// target and level that compiles them or at -O2.
#[cfg(target_os = "macos")]
const RECORD_FIXTURES: [(&str, &[&str]); 11] = [
    ("record-registers/strict_align.c", &TARGETS),
    ("record-registers/records.c", &TARGETS),
    ("record-registers/unstored.c", &TARGETS),
    ("record-registers/unstored_result.c", &TARGETS),
    ("aggregate-parameter-edges.c", &TARGETS),
    ("floating-half-abi-driver.c", &TARGETS),
    ("floating-half-abi-interpreted.c", &TARGETS),
    ("floating-half-abi-worker.c", &TARGETS),
    ("integer-complex-abi-worker.c", &TARGETS),
    // Darwin refuses the half-precision calls of these two with E3049.
    ("rules-20261013/l03-half-abi-driver.c", &[LINUX]),
    ("rules-20261013/l03-half-abi-worker.c", &[LINUX]),
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    root().join("tests/fixtures").join(name)
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn evocc(target: &str, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.arg(format!("--target={target}"));
    if target.contains("linux") {
        command.arg(format!(
            "--sysroot={}",
            root().join("sysroot").join(target).display()
        ));
    }
    command.args(args).output().unwrap()
}

/// The assembly of `path`, or the diagnostics that stopped it.
fn assembly(path: &Path, target: &str, flags: &[&str]) -> Result<String, String> {
    let mut args = vec!["-w"];
    args.extend_from_slice(flags);
    args.extend(["-S", "-o", "-", path.to_str().unwrap()]);
    let output = evocc(target, &args);
    if output.status.success() {
        Ok(String::from_utf8(output.stdout).unwrap())
    } else {
        Err(text(&output))
    }
}

/// What the scan knows of a register: its value is `residue` modulo
/// `modulus`, a power of two.
#[derive(Clone, Copy)]
struct Known {
    modulus: u64,
    residue: u64,
}

impl Known {
    const STACK: Self = Self {
        modulus: 16,
        residue: 0,
    };

    fn plus(self, delta: i64) -> Self {
        Self {
            modulus: self.modulus,
            residue: self.residue.wrapping_add(delta as u64) & (self.modulus - 1),
        }
    }

    fn alignment(self) -> u64 {
        if self.residue == 0 {
            self.modulus
        } else {
            self.modulus.min(1 << self.residue.trailing_zeros())
        }
    }
}

/// The result of scanning one assembly text.
#[derive(Debug, Default)]
struct Scan {
    /// Accesses whose address the scan knows.
    checked: usize,
    /// Accesses through a register that the scan does not follow.
    unknown: usize,
    /// Each access below its width, with its line.
    unaligned: Vec<String>,
}

/// The general register a spelled operand names: `Some(31)` for `sp`.
fn general(operand: &str) -> Option<u8> {
    match operand {
        "sp" | "wsp" => Some(31),
        "fp" => Some(29),
        "lr" => Some(30),
        _ => {
            let digits = operand.strip_prefix('x').or(operand.strip_prefix('w'))?;
            digits.parse().ok().filter(|number| *number < 31)
        }
    }
}

/// The bytes that one data register of an access moves.
fn register_bytes(operand: &str) -> Option<u64> {
    let bytes = match operand.chars().next()? {
        'b' => 1,
        'h' => 2,
        'w' | 's' => 4,
        'x' | 'd' => 8,
        'q' => 16,
        _ => return None,
    };
    operand[1..]
        .chars()
        .all(|ch| ch.is_ascii_digit())
        .then_some(bytes)
        .or_else(|| (operand == "xzr" || operand == "wzr").then_some(bytes))
}

/// An immediate operand `#N`, decimal or hexadecimal, signed.
fn immediate(operand: &str) -> Option<i64> {
    let value = operand.strip_prefix('#')?;
    let (negative, digits) = match value.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, value),
    };
    let magnitude = match digits.strip_prefix("0x") {
        Some(hex) => i64::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    Some(if negative { -magnitude } else { magnitude })
}

/// Whether `mnemonic` is a store-exclusive, whose first operand is a status.
fn exclusive_store(mnemonic: &str) -> bool {
    mnemonic.starts_with("st") && (mnemonic.contains("xr") || mnemonic.contains("xp"))
}

/// The width of each element that an access mnemonic moves, from the
/// mnemonic or its data register; `None` for an instruction that is no load
/// or store.
fn access_width(mnemonic: &str, operands: &[&str]) -> Option<u64> {
    let memory = mnemonic.starts_with("ld") || mnemonic.starts_with("st");
    if !memory || mnemonic.starts_with("ld1") || mnemonic.starts_with("st1") {
        return None;
    }
    if mnemonic.ends_with('b') {
        return Some(1);
    }
    if mnemonic.ends_with('h') {
        return Some(2);
    }
    if mnemonic.ends_with("sw") {
        return Some(4);
    }
    let data = usize::from(exclusive_store(mnemonic));
    register_bytes(operands.get(data)?)
}

/// Scan an assembly text. `sp` and `x29` are 16-byte aligned at every
/// instruction; another register is known only after an `add`, `sub`, `mov`
/// or `and` of a known register and an immediate in the same block. A label
/// or a call forgets what it clobbers, and any other write forgets its
/// destination.
fn scan(assembly: &str) -> Scan {
    let mut result = Scan::default();
    let mut known: HashMap<u8, Known> = HashMap::new();
    let fact = |known: &HashMap<u8, Known>, register: u8| match register {
        29 | 31 => Some(Known::STACK),
        _ => known.get(&register).copied(),
    };
    for line in assembly.lines() {
        let line = line.trim();
        if line.ends_with(':') {
            known.clear();
            continue;
        }
        if line.is_empty() || line.starts_with('.') || line.starts_with("//") {
            continue;
        }
        let (mnemonic, rest) = line.split_once(' ').unwrap_or((line, ""));
        let (plain, memory) = match rest.split_once('[') {
            Some((before, after)) => (before, Some(after)),
            None => (rest, None),
        };
        let operands: Vec<&str> = plain
            .split(',')
            .map(str::trim)
            .filter(|operand| !operand.is_empty())
            .collect();
        let first = operands.first().copied().unwrap_or("");
        if let (Some(width), Some(memory)) = (access_width(mnemonic, &operands), memory) {
            let (inside, after) = memory.split_once(']').unwrap_or((memory, ""));
            let mut parts = inside.split(',').map(str::trim);
            let base = parts.next().and_then(general);
            let offset = match parts.next() {
                None => Some(0),
                Some(operand) => immediate(operand),
            };
            // A post-indexed access uses the base before its writeback.
            let post = after.trim_start().starts_with(',');
            let offset = if post { Some(0) } else { offset };
            match (base.and_then(|base| fact(&known, base)), offset) {
                (Some(base), Some(offset)) => {
                    result.checked += 1;
                    let alignment = base.plus(offset).alignment();
                    if alignment < width {
                        result
                            .unaligned
                            .push(format!("{line}: {width} bytes at alignment {alignment}"));
                    }
                }
                _ => result.unknown += 1,
            }
            // A load writes its data registers, a store-exclusive its status,
            // and a writeback its base.
            if mnemonic.starts_with("ld") || exclusive_store(mnemonic) {
                for operand in &operands {
                    if let Some(register) = general(operand) {
                        known.remove(&register);
                    }
                }
            }
            if post || after.trim_start().starts_with('!') {
                if let Some(base) = base {
                    known.remove(&base);
                }
            }
            continue;
        }
        let destination = general(first);
        match mnemonic {
            "add" | "sub" if operands.len() >= 3 => {
                let source = general(operands[1]).and_then(|source| fact(&known, source));
                let delta = immediate(operands[2]).map(|value| {
                    let shift = operands
                        .get(3)
                        .and_then(|shift| shift.strip_prefix("lsl #"))
                        .and_then(|shift| shift.parse::<u32>().ok())
                        .unwrap_or(0);
                    let value = value << shift;
                    if mnemonic == "sub" {
                        -value
                    } else {
                        value
                    }
                });
                if let Some(destination) = destination.filter(|register| *register != 31) {
                    match source.zip(delta) {
                        Some((source, delta)) => {
                            known.insert(destination, source.plus(delta));
                        }
                        None => {
                            known.remove(&destination);
                        }
                    }
                }
            }
            "mov" if operands.len() == 2 => {
                if let Some(destination) = destination.filter(|register| *register != 31) {
                    match general(operands[1]).and_then(|source| fact(&known, source)) {
                        Some(source) if first.starts_with('x') => {
                            known.insert(destination, source);
                        }
                        _ => {
                            known.remove(&destination);
                        }
                    }
                }
            }
            "and" if operands.len() == 3 => {
                let source = general(operands[1]).and_then(|source| fact(&known, source));
                let mask = immediate(operands[2]);
                if let Some(destination) = destination.filter(|register| *register != 31) {
                    match source.zip(mask) {
                        // Clearing the low bits aligns the value to them.
                        Some((source, mask))
                            if mask < 0 && mask.unsigned_abs().is_power_of_two() =>
                        {
                            let modulus = source.modulus.max(mask.unsigned_abs());
                            known.insert(
                                destination,
                                Known {
                                    modulus,
                                    residue: source.residue & (mask as u64) & (modulus - 1),
                                },
                            );
                        }
                        _ => {
                            known.remove(&destination);
                        }
                    }
                }
            }
            "bl" | "blr" => {
                known.retain(|register, _| (19..=28).contains(register));
            }
            "cmp" | "cmn" | "tst" | "b" | "br" | "ret" | "cbz" | "cbnz" | "tbz" | "tbnz"
            | "brk" | "nop" | "msr" | "isb" | "dmb" | "dsb" | "paciasp" | "autiasp" | "hlt"
            | "fcmp" | "fcmpe" | "ccmp" | "ccmn" | "clrex" => {}
            _ if mnemonic.starts_with("b.") => {}
            _ => {
                if let Some(destination) = destination {
                    known.remove(&destination);
                }
                if mnemonic == "xpaclri" {
                    known.remove(&30);
                }
            }
        }
    }
    result
}

#[cfg(target_os = "macos")]
#[test]
fn record_fixtures_compile_under_the_check_with_aligned_frame_accesses() {
    for (name, targets) in RECORD_FIXTURES {
        let path = fixture(name);
        for target in targets {
            for level in LEVELS {
                let label = format!("{name} {target} {level}");
                let assembly = assembly(&path, target, &[level, "-mstrict-align"])
                    .unwrap_or_else(|error| panic!("{label}\n{error}"));
                let scan = scan(&assembly);
                assert!(scan.checked > 0, "{label}: {scan:?}");
                assert!(scan.unaligned.is_empty(), "{label}: {scan:?}");
            }
        }
    }
}

/// The scan finds the accesses that D31 removed: without the option, a
/// four-aligned record's chunk still moves as one doubleword from an address
/// that only four divides, which ordinary AArch64 memory admits.
#[test]
fn without_the_option_a_chunk_of_a_four_aligned_record_is_one_doubleword() {
    let path = fixture("record-registers/strict_align.c");
    for target in TARGETS {
        for level in LEVELS {
            let label = format!("{target} {level}");
            let plain = assembly(&path, target, &[level]).unwrap();
            let unaligned = scan(&plain).unaligned;
            assert!(
                unaligned
                    .iter()
                    .any(|line| line.ends_with("8 bytes at alignment 4")),
                "{label}: {unaligned:?}"
            );
            let strict = assembly(&path, target, &[level, "-mstrict-align"]).unwrap();
            assert_ne!(plain, strict, "{label}");
            // The last option wins.
            let restored = assembly(
                &path,
                target,
                &[level, "-mstrict-align", "-mno-strict-align"],
            )
            .unwrap();
            assert_eq!(plain, restored, "{label}");
        }
    }
}

fn machine_target(target: &str) -> Target {
    if target.contains("linux") {
        Target::Linux
    } else {
        Target::Darwin
    }
}

/// The EvoIR that the driver emits for `path` at `level`.
fn emitted(path: &Path, target: &str, level: &str) -> evocc_evoir::Module {
    let output = evocc(
        target,
        &["-w", level, "--emit=evoir", path.to_str().unwrap()],
    );
    assert!(output.status.success(), "{}", text(&output));
    evocc_evoir::parse_target_module(&output.stdout, target).unwrap()
}

/// The strict lowering moves the same bytes: the EvoMIR interpreter runs it
/// with the transport rule (EvoMIR 4.1), and `main` returns 0.
#[test]
fn the_strict_lowering_runs_in_the_evomir_interpreter() {
    for name in [
        "record-registers/strict_align.c",
        "record-registers/records.c",
    ] {
        let path = fixture(name);
        for target in TARGETS {
            for level in LEVELS {
                let label = format!("{name} {target} {level}");
                let module = emitted(&path, target, level);
                let options = |strict_alignment| evocc_mir::emit::CodegenOptions {
                    strict_alignment,
                    anonymous_on_stack: target.contains("darwin"),
                    ..Default::default()
                };
                let lower = |strict| {
                    evocc_mir::lower_module_for_options(
                        machine_target(target),
                        &module,
                        &|_| false,
                        &options(strict),
                    )
                    .unwrap_or_else(|refusals| panic!("{label}: {refusals:?}"))
                };
                let strict = lower(true);
                assert_ne!(strict, lower(false), "{label}");
                evocc_mir::validate_module(&strict).unwrap();
                let outcome =
                    evocc_mir::interpret_module(&strict, "main", &[], MirLimits::new(1_000_000))
                        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
                assert_eq!(outcome.results.first(), Some(&MirValue::Int(0)), "{label}");
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_strict_programs_return_what_apple_clang_programs_return() {
    let directory =
        std::env::temp_dir().join(format!("evocc-strict-alignment-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    for name in [
        "record-registers/strict_align.c",
        "record-registers/records.c",
    ] {
        let source = fixture(name);
        let stem = source.file_stem().unwrap().to_str().unwrap().to_owned();
        for level in LEVELS {
            let mine = directory.join(format!("{stem}-evocc{level}"));
            let theirs = directory.join(format!("{stem}-clang{level}"));
            let built = evocc(
                "aarch64-apple-darwin",
                &[
                    level,
                    "-mstrict-align",
                    source.to_str().unwrap(),
                    "-o",
                    mine.to_str().unwrap(),
                ],
            );
            assert!(built.status.success(), "{}", text(&built));
            let built = Command::new("clang")
                .args(["-w", level, "-mstrict-align"])
                .arg(&source)
                .arg("-o")
                .arg(&theirs)
                .output()
                .unwrap();
            assert!(built.status.success(), "{}", text(&built));
            for program in [&mine, &theirs] {
                let run = Command::new(program).output().unwrap();
                assert_eq!(run.status.code(), Some(0), "{name} {level}\n{}", text(&run));
            }
        }
    }
    let _ = std::fs::remove_dir_all(&directory);
}

/// Run `objects` in QEMU with the harness of `tests/fixtures/strict-align-qemu`:
/// the exit status and what the harness printed.
fn run_in_qemu(directory: &Path, label: &str, objects: &[PathBuf]) -> (Option<i32>, String) {
    let harness = fixture("strict-align-qemu");
    let start = directory.join("start.o");
    if !start.exists() {
        let built = Command::new("clang")
            .args(["--target=aarch64-unknown-linux-gnu", "-c"])
            .arg(harness.join("start.s"))
            .arg("-o")
            .arg(&start)
            .output()
            .unwrap();
        assert!(built.status.success(), "{}", text(&built));
    }
    let image = directory.join(format!("{label}.elf"));
    let linked = Command::new("ld.lld")
        .arg("-T")
        .arg(harness.join("link.ld"))
        .arg(&start)
        .args(objects)
        .arg("-o")
        .arg(&image)
        .output()
        .unwrap();
    assert!(linked.status.success(), "{label}\n{}", text(&linked));
    let run = Command::new("qemu-system-aarch64")
        .args([
            "-M",
            "virt",
            "-cpu",
            "cortex-a72",
            "-nographic",
            "-semihosting",
        ])
        .arg("-kernel")
        .arg(&image)
        .output()
        .unwrap();
    // Semihosting writes to the standard error of QEMU.
    (
        run.status.code(),
        String::from_utf8_lossy(&run.stderr).into_owned(),
    )
}

/// The object of the Linux assembly of `source`.
fn linux_object(directory: &Path, label: &str, source: &Path, flags: &[&str]) -> PathBuf {
    let target = "aarch64-unknown-linux-gnu";
    let assembly = assembly(source, target, flags).unwrap_or_else(|error| panic!("{error}"));
    let text_path = directory.join(format!("{label}.s"));
    std::fs::write(&text_path, assembly).unwrap();
    let object = directory.join(format!("{label}.o"));
    let built = Command::new("clang")
        .arg(format!("--target={target}"))
        .arg("-c")
        .arg(&text_path)
        .arg("-o")
        .arg(&object)
        .output()
        .unwrap();
    assert!(built.status.success(), "{label}\n{}", text(&built));
    object
}

/// The Linux programs run with alignment checking on: at EL1 with the MMU off
/// and `SCTLR_EL1.A` set, in qemu-system-aarch64. Each returns 0. The build
/// without the option faults with an alignment fault, which shows that the
/// harness traps such an access.
#[test]
#[ignore = "needs clang, ld.lld and qemu-system-aarch64; run explicitly"]
fn linux_strict_programs_run_with_alignment_checking() {
    let directory = std::env::temp_dir().join(format!(
        "evocc-strict-alignment-qemu-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    let programs: [(&str, &[&str]); 4] = [
        ("strict_align", &["record-registers/strict_align.c"]),
        ("records", &["record-registers/records.c"]),
        ("aggregate", &["aggregate-parameter-edges.c"]),
        (
            "complex",
            &[
                "integer-complex-abi-driver.c",
                "integer-complex-abi-worker.c",
            ],
        ),
    ];
    for (name, sources) in programs {
        for level in LEVELS {
            let label = format!("{name}{level}");
            let objects: Vec<PathBuf> = sources
                .iter()
                .enumerate()
                .map(|(index, source)| {
                    linux_object(
                        &directory,
                        &format!("{label}-{index}"),
                        &fixture(source),
                        &[level, "-mstrict-align"],
                    )
                })
                .collect();
            let (status, printed) = run_in_qemu(&directory, &label, &objects);
            assert_eq!(status, Some(0), "{label}\n{printed}");
        }
    }
    for level in LEVELS {
        let label = format!("plain{level}");
        let object = linux_object(
            &directory,
            &label,
            &fixture("record-registers/strict_align.c"),
            &[level],
        );
        let (status, printed) = run_in_qemu(&directory, &label, &[object]);
        assert_eq!(status, Some(200), "{label}\n{printed}");
        // ESR_EL1: a data abort at the same level (EC 0x25) with an
        // alignment fault (DFSC 0x21).
        let esr = printed
            .lines()
            .nth(1)
            .and_then(|line| u64::from_str_radix(line.trim(), 16).ok())
            .unwrap_or_else(|| panic!("{label}\n{printed}"));
        assert_eq!((esr >> 26, esr & 0x3f), (0x25, 0x21), "{label}\n{printed}");
    }
    let _ = std::fs::remove_dir_all(&directory);
}
