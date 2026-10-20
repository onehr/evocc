//! `__has_include_next` through the driver (EvoIR 10.4, D18). Clang's
//! resource `<limits.h>` and `<stdint.h>` include the SDK header of the same
//! name only when `__has_include_next` finds it. Before D18 the query was
//! undefined, so on Darwin `PATH_MAX` was undeclared after `<limits.h>` and
//! the APR headers stopped at their `#error` for `APR_PATH_MAX`.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const DARWIN: &str = "aarch64-apple-darwin";

struct Work(PathBuf);
impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-has-include-next-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained failed probe: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn evocc(target: &str, args: &[&str], source: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_evocc"))
        .arg(format!("--target={target}"))
        .args(["--error-format=short"])
        .args(args)
        .arg(source)
        .output()
        .unwrap()
}

/// The query and the directive continue after the directory that found the
/// current header, and the main file searches from the first directory.
#[test]
fn has_include_next_continues_the_command_line_search_on_both_targets() {
    let work = Work::new("chain");
    work.source(
        "first/chain.h",
        concat!(
            "#define FIRST 3\n",
            "#if __has_include_next(<chain.h>)\n",
            "#include_next <chain.h>\n",
            "#endif\n",
            "#if __has_include_next(<only_first.h>)\n",
            "#error the first directory is searched again\n",
            "#endif\n",
        ),
    );
    work.source("first/only_first.h", "");
    work.source(
        "second/chain.h",
        concat!(
            "#define SECOND 4\n",
            "#if __has_include_next(<chain.h>)\n",
            "#error the last directory has a successor\n",
            "#endif\n",
        ),
    );
    let main = work.source(
        "main.c",
        concat!(
            "#if __has_include_next(<only_first.h>)\n",
            "MAIN_SEARCHES_FROM_THE_FIRST_DIRECTORY\n",
            "#endif\n",
            "#include <chain.h>\n",
            "FIRST SECOND\n",
        ),
    );
    let first = format!("-I{}", work.0.join("first").display());
    let second = format!("-I{}", work.0.join("second").display());
    for target in [LINUX, DARWIN] {
        let output = evocc(target, &["-E", "-P", "-nostdinc", &first, &second], &main);
        assert!(output.status.success(), "{target}\n{}", text(&output));
        assert_eq!(
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["MAIN_SEARCHES_FROM_THE_FIRST_DIRECTORY", "3", "4"],
            "{target}\n{}",
            text(&output)
        );
    }
}

/// The limits and the integer types after Clang's resource headers, as an
/// EvoCC program and an Apple clang program print them.
#[cfg(target_os = "macos")]
const LIMITS_PROGRAM: &str = r#"#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#define NAME(x) _Generic((x)0, signed char: "signed char", short: "short", \
    unsigned short: "unsigned short", int: "int", unsigned: "unsigned", \
    long: "long", unsigned long: "unsigned long", long long: "long long", \
    unsigned long long: "unsigned long long", default: "other")
char path[PATH_MAX];
int main(void) {
    printf("%d %d %d %d %d %d %d\n", PATH_MAX, NAME_MAX, ARG_MAX, CHILD_MAX,
           LINE_MAX, IOV_MAX, PIPE_BUF);
    printf("%d %d %d %ld %d %d\n", MB_LEN_MAX, LONG_BIT, WORD_BIT,
           (long)SSIZE_MAX, CHAR_MIN, CHAR_MAX);
    printf("%lld %llu %ld %u\n", LLONG_MAX, ULLONG_MAX, LONG_MIN, UINT_MAX);
    printf("%s %s %s %s %s %s %s %s\n", NAME(int64_t), NAME(uint64_t),
           NAME(intptr_t), NAME(intmax_t), NAME(int8_t), NAME(int_least16_t),
           NAME(int_fast16_t), NAME(uint_fast32_t));
    printf("%lld %ld %zu %ld %d\n", (long long)INT64_MAX, (long)INTPTR_MIN,
           (size_t)SIZE_MAX, (long)PTRDIFF_MAX, (int)SIG_ATOMIC_MIN);
    return sizeof path == PATH_MAX ? 0 : 1;
}
"#;

#[cfg(target_os = "macos")]
fn run_program(program: &Path) -> String {
    let output = Command::new(program).output().unwrap();
    assert!(output.status.success(), "{}", text(&output));
    String::from_utf8(output.stdout).unwrap()
}

/// Darwin: `<limits.h>` and `<stdint.h>` reach the SDK headers, so the
/// program prints what the Apple clang program prints, at -O0 and -O2.
#[cfg(target_os = "macos")]
#[test]
fn darwin_limits_and_stdint_reach_the_sdk_headers_as_with_apple_clang() {
    let work = Work::new("darwin-limits");
    let source = work.source("limits.c", LIMITS_PROGRAM);
    let reference = work.0.join("limits-clang");
    let output = Command::new("clang")
        .args(["-std=gnu11", "-w"])
        .arg(&source)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let expected = run_program(&reference);
    assert!(expected.starts_with("1024 255 "), "{expected}");
    for level in ["-O0", "-O2"] {
        let object = work.0.join(format!("limits{level}.o"));
        let output = evocc(
            DARWIN,
            &[level, "-c", "-o", object.to_str().unwrap()],
            &source,
        );
        assert!(output.status.success(), "{level}\n{}", text(&output));
        let program = work.0.join(format!("limits{level}"));
        let output = Command::new("clang")
            .arg(&object)
            .arg("-o")
            .arg(&program)
            .output()
            .unwrap();
        assert!(output.status.success(), "{level}\n{}", text(&output));
        assert_eq!(run_program(&program), expected, "{level}");
    }
}

/// Darwin: APR headers decide `APR_PATH_MAX` from `PATH_MAX`. The sweep's
/// E0104 group of 64 headers stopped at `apr.h:567`; three of them here.
#[cfg(target_os = "macos")]
#[test]
fn darwin_apr_headers_find_path_max() {
    let work = Work::new("darwin-apr");
    for header in [
        "apr-1/apr.h",
        "apr-1/apr_allocator.h",
        "apr-1/apr_file_info.h",
    ] {
        let source = work.source(
            &format!("{}.c", header.replace(['/', '.'], "_")),
            &format!("#include <{header}>\n_Static_assert(APR_PATH_MAX == 1024, \"PATH_MAX\");\n"),
        );
        for level in ["-O0", "-O2"] {
            let assembly = work.0.join("unit.s");
            let output = evocc(
                DARWIN,
                &[level, "-S", "-o", assembly.to_str().unwrap()],
                &source,
            );
            assert!(
                output.status.success(),
                "{header} {level}\n{}",
                text(&output)
            );
        }
    }
}
