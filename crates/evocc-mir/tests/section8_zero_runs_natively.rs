//! Section 9's third instrument over the run encoding, on Darwin.
//!
//! "The assembled program is run on the host and its output compared with the
//! reference compiler's build of the same C source ... the host toolchain for
//! Darwin, recorded by version", and its programs "are compared by exit status
//! modulo 256, the eight bits a process status carries".
//!
//! The Linux half of this measurement is taken in the sealed container against
//! GCC-14.2 and clang-19, where the program's whole standard output is
//! compared; this is the same objects on the other target, through the path a
//! user takes: the driver lowers the C source to EvoIR, this crate emits
//! Darwin assembly, and the host assembles, links and runs it. The program
//! folds a digest of every static object, taken before it writes to any of
//! them and again after, into the eight bits it exits with, so a byte the run
//! encoding got wrong changes the status.
//!
//! Standard output is deliberately not the instrument here. A variadic call
//! lowered from this front end passes its arguments the way Linux AArch64
//! does, and Darwin passes them on the stack, so `printf` on this target
//! prints the wrong values for reasons that have nothing to do with static
//! data: `printf("%d %d %d\n", 11, 22, 33)` already does. The exit status
//! travels in `w0` and is unaffected.
//!
//! The test is skipped, with the reason printed, where the driver or the host
//! toolchain is absent.

use std::path::Path;
use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module, Driver, Toolchain};

/// A megabyte of zeros, sixty-four kibibytes that are zero then a few values
/// then zero again, a sparse array, a string with interior nul bytes and a
/// table of relocated pointers into the rest.
const SOURCE: &str = r#"
static unsigned char big[1048576];
static int partial[16384] = {[4000] = 7, [4001] = 8, [4002] = 9, [4003] = -1};
static const char message[] = "b11 zero runs, and a tail of \0\0\0 nul bytes";
static int scattered[64] = {[0] = 1, [17] = -1, [40] = 0x7fffffff, [63] = 2};

static int *const table[4] = {
    &partial[0],
    &partial[1000],
    &scattered[17],
    (int *)0,
};

static unsigned long digest(const unsigned char *bytes, unsigned long count) {
    unsigned long hash = 1469598103934665603UL;
    for (unsigned long index = 0; index < count; index++) {
        hash ^= bytes[index];
        hash *= 1099511628211UL;
    }
    return hash;
}

int main(void) {
    unsigned long hash = digest(big, sizeof big);
    hash = hash * 31UL + digest((const unsigned char *)partial, sizeof partial);
    hash = hash * 31UL + digest((const unsigned char *)scattered, sizeof scattered);
    hash = hash * 31UL + digest((const unsigned char *)message, sizeof message);

    /* The relocated table has to name the objects it was written from. */
    if (table[0] != &partial[0]) return 201;
    if (table[1] != &partial[1000]) return 202;
    if (table[2] != &scattered[17]) return 203;
    if (table[3] != 0) return 204;

    partial[0] = 11;
    partial[1000] = 22;
    partial[16383] = 33;
    big[0] = 1;
    big[1048575] = 2;
    hash = hash * 31UL + digest(big, sizeof big);
    hash = hash * 31UL + digest((const unsigned char *)partial, sizeof partial);

    long sum = 0;
    for (unsigned long index = 0; index < sizeof big; index++) sum += big[index];
    for (unsigned long index = 0; index < 16384; index++) sum += partial[index];
    for (unsigned long index = 0; index < 64; index++) sum += scattered[index];
    hash = hash * 31UL + (unsigned long)sum;

    return (int)(hash % 199UL);
}
"#;

/// Run one program and report the eight bits its exit status carries.
fn status_of(program: &Path) -> i32 {
    let outcome = Command::new(program)
        .output()
        .unwrap_or_else(|error| panic!("{}: {error}", program.display()));
    outcome
        .status
        .code()
        .unwrap_or_else(|| panic!("{} did not exit normally", program.display()))
        & 0xff
}

#[test]
fn the_run_encoding_agrees_with_the_host_compiler_on_darwin() {
    let driver = match Driver::locate() {
        Ok(driver) => driver,
        Err(reason) => {
            eprintln!("skipped: {reason}");
            return;
        }
    };
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            eprintln!("skipped: {reason}");
            return;
        }
    };
    eprintln!(
        "driver {}, {}",
        driver.binary().display(),
        toolchain.version()
    );

    let directory = std::env::temp_dir().join(format!("evocc-b11-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a working directory");
    let source = directory.join("b11-zero-runs.c");
    std::fs::write(&source, SOURCE).expect("the source is written");

    let text = driver
        .emit_evoir(&source, false)
        .unwrap_or_else(|complaint| panic!("the driver refused the source: {complaint}"));
    let module = parse_section9_module(text.as_bytes()).expect("the driver's EvoIR parses");
    let assembly = match compile_module(&module) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    };

    // The megabyte of zeros is reserved rather than stored, and the whole unit
    // is a page of text rather than a megabyte of it.
    assert!(
        assembly.contains(".section __DATA,__bss,zerofill\n.p2align 0\n_big:\n    .zero 1048576\n"),
        "the megabyte is four lines in the zero-filled section:\n{}",
        tail(&assembly)
    );
    assert!(
        assembly.contains("    .byte 7, 0, 0, 0, 8, 0, 0, 0, 9, 0, 0, 0, 255, 255, 255, 255\n"),
        "the values inside the partial object are one `.byte` line:\n{}",
        tail(&assembly)
    );
    assert!(
        assembly.lines().count() < 1000,
        "the whole unit is {} lines",
        assembly.lines().count()
    );

    let object = toolchain
        .assemble("b11-zero-runs", &assembly)
        .unwrap_or_else(|stop| panic!("the host assembler refused: {}", stop.detail()));
    let program = toolchain
        .link("b11-zero-runs", &object)
        .unwrap_or_else(|stop| panic!("the host linker refused: {}", stop.detail()));
    let ours = status_of(&program);
    let _ = std::fs::remove_file(&program);

    let mut statuses = Vec::new();
    for level in ["-O0", "-O2"] {
        let reference = directory.join(format!("b11-reference{level}"));
        let built = Command::new("clang")
            .arg(level)
            .arg("-o")
            .arg(&reference)
            .arg(&source)
            .output()
            .expect("clang runs");
        assert!(
            built.status.success(),
            "the host compiler built the source at {level}: {}",
            String::from_utf8_lossy(&built.stderr)
        );
        statuses.push((level, status_of(&reference)));
    }

    assert!(
        ours < 199,
        "a status of 201 to 204 is the program's own report that the relocated \
         table does not name its objects: {ours}"
    );
    for (level, theirs) in statuses {
        assert_eq!(
            ours, theirs,
            "the emitted program and clang {level} digest the same static bytes"
        );
    }
}

/// The last two kibibytes of an emitted unit, which is where its globals are.
fn tail(assembly: &str) -> &str {
    let start = assembly.len().saturating_sub(2048);
    &assembly[assembly
        .char_indices()
        .map(|(index, _)| index)
        .find(|index| *index >= start)
        .unwrap_or(0)..]
}
