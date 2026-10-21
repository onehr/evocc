//! Clang extensions that Apple's SDK and Clang's resource headers use and
//! EvoCC does not implement (EvoIR 10.1.9 and 10.1.14, D19). On Darwin each
//! one is a named refusal at its first use: Blocks are `E2963`, `overloadable`
//! is `E2964`, the os_log builtins are `E2965`, and the ACLE builtins of
//! `<arm_acle.h>` are `E2966`. The headers that declare them compile when the
//! unit does not use them. Linux keeps the diagnostics of GCC 14's dialect.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const DARWIN: &str = "aarch64-apple-darwin";
const LINUX: &str = "aarch64-unknown-linux-gnu";

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

fn scratch() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "evocc-clang-extensions-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Compile `source` with `--parse-only` and report every diagnostic.
fn survey(target: &str, source: &str) -> Output {
    let directory = scratch();
    let path = directory.join("source.c");
    std::fs::write(&path, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.args([
        format!("--target={target}"),
        "--survey".to_owned(),
        "--error-format=short".to_owned(),
        "--parse-only".to_owned(),
        path.to_str().unwrap().to_owned(),
    ]);
    if target == LINUX {
        command.arg(format!(
            "--sysroot={}",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../sysroot")
                .join(LINUX)
                .display()
        ));
    }
    let output = command.output().unwrap();
    let _ = std::fs::remove_dir_all(&directory);
    output
}

/// The `CODE LINE:COLUMN` of each diagnostic in `source.c`.
fn diagnostics(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter_map(|line| {
            let (location, rest) = line.split_once(": error[")?;
            let code = &rest[..5];
            let mut parts = location.rsplitn(3, ':');
            let column = parts.next()?;
            let line = parts.next()?;
            let file = parts.next()?;
            Some(if file.ends_with("source.c") {
                format!("{code} {line}:{column}")
            } else {
                format!(
                    "{code} {}:{line}:{column}",
                    file.rsplit('/').next().unwrap()
                )
            })
        })
        .collect()
}

fn first(output: &Output) -> Option<String> {
    diagnostics(output).into_iter().next()
}

const BLOCKS: &str = "typedef void (^handler_t)(int);
int run(int (^callback)(void));
int main(void)
{
    void (^done)(void) = ^{ };
    return (int)sizeof(void (^)(void));
}
";

#[test]
fn blocks_are_refused_on_darwin_where_apple_clang_without_blocks_refuses_them() {
    let output = survey(DARWIN, BLOCKS);
    let refused: Vec<String> = diagnostics(&output)
        .into_iter()
        .filter(|diagnostic| diagnostic.starts_with("E2963"))
        .collect();
    assert_eq!(
        refused,
        [
            "E2963 1:15",
            "E2963 2:14",
            "E2963 5:11",
            "E2963 5:26",
            "E2963 6:30"
        ],
        "{}",
        text(&output)
    );
    assert_eq!(first(&survey(LINUX, BLOCKS)).as_deref(), Some("E1001 1:15"));
}

#[cfg(target_os = "macos")]
#[test]
fn the_blocks_positions_are_the_ones_of_apple_clang_without_blocks() {
    let directory = scratch();
    let path = directory.join("source.c");
    std::fs::write(&path, BLOCKS).unwrap();
    let output = Command::new("clang")
        .args(["-fno-blocks", "-fsyntax-only"])
        .arg(&path)
        .output()
        .unwrap();
    let theirs: Vec<String> = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.contains("blocks support disabled"))
        .map(|line| {
            let mut parts = line.split(':');
            parts.next();
            format!("E2963 {}:{}", parts.next().unwrap(), parts.next().unwrap())
        })
        .collect();
    let mine: Vec<String> = diagnostics(&survey(DARWIN, BLOCKS))
        .into_iter()
        .filter(|diagnostic| diagnostic.starts_with("E2963"))
        .collect();
    assert_eq!(mine, theirs);
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn overloadable_is_refused_on_darwin_and_preserved_on_linux() {
    let source = "int twice(int x) __attribute__((overloadable));
int halve(int x) [[clang::overloadable]];
__attribute__((__overloadable__)) int third(int x);
int main(void) { return 0; }
";
    let output = survey(DARWIN, source);
    assert_eq!(
        diagnostics(&output),
        ["E2964 1:33", "E2964 2:20", "E2964 3:16"],
        "{}",
        text(&output)
    );
    let output = survey(LINUX, source);
    assert!(output.status.success(), "{}", text(&output));
}

#[test]
fn the_os_log_builtins_are_refused_at_their_call_on_darwin() {
    let query = "#if __has_builtin(__builtin_os_log_format) && __has_builtin(__builtin_os_log_format_buffer_size)
int both;
#else
#error missing
#endif
int main(void) { return 0; }
";
    assert!(survey(DARWIN, query).status.success());
    assert_eq!(first(&survey(LINUX, query)).as_deref(), Some("E0104 4:1"));
    let call = "int main(void) { return (int)__builtin_os_log_format_buffer_size(\"%d\", 1); }\n";
    assert_eq!(first(&survey(DARWIN, call)).as_deref(), Some("E2965 1:30"));
    assert_eq!(first(&survey(LINUX, call)).as_deref(), Some("E2949 1:30"));
}

#[cfg(target_os = "macos")]
#[test]
fn os_log_compiles_and_a_signpost_is_refused_at_its_use() {
    let header = "#include <os/log.h>\n#include <os/signpost.h>\nint main(void) { return 0; }\n";
    let output = survey(DARWIN, header);
    assert!(output.status.success(), "{}", text(&output));
    let signpost = "#include <os/signpost.h>
int main(void)
{
    os_log_t log = os_log_create(\"com.example\", \"test\");
    os_signpost_event_emit(log, OS_SIGNPOST_ID_EXCLUSIVE, \"event\");
    return 0;
}
";
    let output = survey(DARWIN, signpost);
    assert_eq!(
        first(&output).as_deref(),
        Some("E2965 5:5"),
        "{}",
        text(&output)
    );
}

#[test]
fn acle_builtins_are_refused_at_their_use() {
    let source = "void __dmb(unsigned int);
static unsigned int unused(unsigned int x) { return __builtin_arm_rbit(x); }
int main(void)
{
    __dmb(15);
    return (int)__builtin_arm_clz(1u);
}
";
    let output = survey(DARWIN, source);
    assert_eq!(
        diagnostics(&output),
        ["E2966 5:5", "E2966 6:17"],
        "{}",
        text(&output)
    );
    // GCC 14 has no `__builtin_arm_` builtins and no `__dmb` builtin.
    let output = survey(LINUX, source);
    assert_eq!(
        first(&output).as_deref(),
        Some("E2949 2:53"),
        "{}",
        text(&output)
    );
    let ordinary = "void __dmb(unsigned int);\nint main(void) { __dmb(15); return 0; }\n";
    assert!(survey(LINUX, ordinary).status.success());
}

#[test]
fn darwin_defines_the_acle_macro_of_apple_clang() {
    let source = "#if defined(__ARM_ACLE) && __ARM_ACLE == 202420 && __ARM_ACLE_VERSION(2024, 2, 0) == 202420
int acle;
#else
#error no ACLE
#endif
#ifdef __ARM_FEATURE_CRC32
#error no optional instruction is claimed
#endif
";
    assert!(survey(DARWIN, source).status.success());
    assert_eq!(first(&survey(LINUX, source)).as_deref(), Some("E0104 4:1"));
}

#[cfg(target_os = "macos")]
#[test]
fn the_acle_headers_compile_and_an_intrinsic_is_refused_at_its_use() {
    let headers = "#include <arm_acle.h>\n#include <arm64/mte.h>\nint main(void) { return 0; }\n";
    let output = survey(DARWIN, headers);
    assert!(output.status.success(), "{}", text(&output));
    let rbit = "#include <arm_acle.h>\nint main(void) { return __rbit(1u) != 0x80000000u; }\n";
    let output = survey(DARWIN, rbit);
    assert_eq!(
        first(&output).as_deref(),
        Some("E2966 arm_acle.h:213:10"),
        "{}",
        text(&output)
    );
    let crc = "#include <arm_acle.h>\nint main(void) { return (int)__crc32b(0, 1); }\n";
    let output = survey(DARWIN, crc);
    assert!(!output.status.success(), "{}", text(&output));
    let output = Command::new("clang")
        .args(["-dM", "-E", "-x", "c", "/dev/null"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("#define __ARM_ACLE 202420\n"));
}

#[test]
fn an_internal_definition_with_a_list_attribute_is_refused_only_when_kept_on_darwin() {
    let unused = "static int checksum(void) __attribute__((target(\"crc\")));
static int checksum(void) { return 1; }
static __inline__ unsigned int __attribute__((target(\"crc\"))) step(unsigned int x) { return x; }
static void *__attribute__((target(\"gcs\"))) stack(void *x) { return x; }
int main(void) { return 0; }
";
    let output = survey(DARWIN, unused);
    assert!(output.status.success(), "{}", text(&output));
    // Linux keeps the refusal of every definition (D17).
    assert_eq!(first(&survey(LINUX, unused)).as_deref(), Some("E2952 2:12"));
    let used = "static __inline__ unsigned int __attribute__((target(\"crc\"))) step(unsigned int x) { return x; }
int main(void) { return (int)step(0); }
";
    let output = survey(DARWIN, used);
    assert_eq!(
        first(&output).as_deref(),
        Some("E2952 1:63"),
        "{}",
        text(&output)
    );
    // An object keeps the refusal at its declaration.
    let object = "int *__attribute__((target(\"gcs\"))) pointer;\nint main(void) { return 0; }\n";
    assert_eq!(
        first(&survey(DARWIN, object)).as_deref(),
        Some("E2952 1:21")
    );
}
