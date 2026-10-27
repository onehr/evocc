//! Framework headers on Darwin (EvoIR 10.5, D19). `-F` and `-iframework`
//! name framework directories, the SDK's `System/Library/Frameworks` and
//! `System/Library/SubFrameworks` follow the system headers, and `-framework`
//! links a framework. The crossed tests run Apple clang on the same trees and
//! programs: the search order, `#import`, the SDK's CoreFoundation and a
//! framework built for the test. Linux keeps no framework directories.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const LINUX: &str = "aarch64-unknown-linux-gnu";

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
            "evocc-darwin-frameworks-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn evocc(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn clang(&self, args: &[&str]) -> Output {
        Command::new("clang")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap()
    }

    #[cfg(target_os = "macos")]
    fn run(&self, name: &str) -> Output {
        Command::new(self.0.join(name)).output().unwrap()
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
fn succeeded(output: &Output, label: &str) {
    assert!(output.status.success(), "{label}\n{}", text(output));
}

/// The words of preprocessed output, without line structure.
#[cfg(target_os = "macos")]
fn words(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// The tree of `crates/evocc-cpp/tests/framework_headers.rs`, with one more
/// framework for the system framework option.
#[cfg(target_os = "macos")]
const FRAMEWORK_TREE: [(&str, &str); 15] = [
    (
        "F1/Alpha.framework/Headers/Alpha.h",
        "alpha_main\n#include <Alpha/Detail.h>\n#include \"Local.h\"\n#include <Alpha/Secret.h>\n",
    ),
    ("F1/Alpha.framework/Headers/Detail.h", "alpha_detail\n"),
    ("F1/Alpha.framework/Headers/Local.h", "alpha_local\n"),
    (
        "F1/Alpha.framework/PrivateHeaders/Secret.h",
        "alpha_secret\n",
    ),
    ("F1/Beta.framework/Headers/OnlyOne.h", "beta_one\n"),
    ("F2/Beta.framework/Headers/OnlyTwo.h", "beta_two\n"),
    ("F2/Gamma.framework/Headers/Gamma.h", "gamma_two\n"),
    ("I1/Alpha/Alpha.h", "i1_alpha\n"),
    (
        "F1/Umbrella.framework/Headers/Umbrella.h",
        "umbrella_main\n#include <Sub/Sub.h>\n#include <Sub/Next.h>\n",
    ),
    (
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Sub.h",
        "sub_main\n#include <Sub/Other.h>\n#include <Sub/Hidden.h>\n#include <Alpha/Detail.h>\n",
    ),
    (
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Other.h",
        "sub_other\n",
    ),
    (
        "F1/Umbrella.framework/Frameworks/Sub.framework/PrivateHeaders/Hidden.h",
        "sub_hidden\n",
    ),
    (
        "F1/Umbrella.framework/Frameworks/Sub.framework/Headers/Next.h",
        "sub_next\n#include_next <Next.h>\n",
    ),
    ("I2/Next.h", "i2_next\n"),
    ("S1/Gamma/Gamma.h", "s1_gamma\n"),
];

#[cfg(target_os = "macos")]
const FRAMEWORK_MAIN: &str = r#"#include <Alpha/Alpha.h>
#include <Gamma/Gamma.h>
#include <Umbrella/Umbrella.h>
#if __has_include(<Beta/OnlyOne.h>)
has_beta_one
#endif
#if __has_include(<Beta/OnlyTwo.h>)
has_beta_two
#else
no_beta_two
#endif
#if __has_include(<Sub/Sub.h>)
main_sees_sub
#else
main_no_sub
#endif
#if __has_include(<Alpha/Missing.h>)
wrong
#endif
#include "Alpha/Detail.h"
"#;

/// The option orders of the crossed search test.
#[cfg(target_os = "macos")]
const ORDERS: [&[&str]; 7] = [
    &["-F", "F1", "-F", "F2", "-I", "I2"],
    &["-I", "I1", "-F", "F1", "-F", "F2", "-I", "I2"],
    &["-F", "F1", "-I", "I1", "-F", "F2", "-I", "I2"],
    &["-F", "F2", "-F", "F1", "-I", "I2"],
    &["-iframework", "F1", "-F", "F2", "-I", "I2"],
    &["-FF1", "-isystem", "S1", "-iframework", "F2", "-I", "I2"],
    &["-isystem", "S1", "-iframeworkF2", "-FF1", "-II2"],
];

#[cfg(target_os = "macos")]
#[test]
fn framework_search_order_matches_apple_clang() {
    let work = Work::new("order");
    for (name, contents) in FRAMEWORK_TREE {
        work.write(name, contents);
    }
    work.write("main.c", FRAMEWORK_MAIN);
    for order in ORDERS {
        let mut args = vec!["-E", "-P", "main.c"];
        args.extend_from_slice(order);
        let theirs = work.clang(&args);
        succeeded(&theirs, "clang -E");
        let mut args = vec!["-E", "main.c"];
        args.extend_from_slice(order);
        let mine = work.evocc(&args);
        succeeded(&mine, "evocc -E");
        assert_eq!(words(&mine), words(&theirs), "{order:?}");
        assert!(words(&mine).len() >= 13, "{order:?}\n{}", text(&mine));
    }
}

#[cfg(target_os = "macos")]
const IMPORT_MAIN: &str = r#"#import "once.h"
#import "once.h"
#include "once.h"
#include "twice.h"
#import "twice.h"
#include "twice.h"
#include "plain.h"
#include "nested.h"
#include "plain.h"
#import <guarded.h>
end_main
"#;

#[cfg(target_os = "macos")]
#[test]
fn import_matches_apple_clang() {
    let work = Work::new("import");
    work.write("once.h", "once_body\n");
    work.write("twice.h", "twice_body\n");
    work.write("plain.h", "plain_body\n");
    work.write("nested.h", "nested_body\n#import \"plain.h\"\n");
    work.write(
        "guarded.h",
        "#ifndef GUARDED\n#define GUARDED\nguarded_body\n#endif\n",
    );
    work.write("main.c", IMPORT_MAIN);
    let theirs = work.clang(&["-E", "-P", "-I.", "main.c"]);
    succeeded(&theirs, "clang -E");
    let mine = work.evocc(&["-E", "-I.", "main.c"]);
    succeeded(&mine, "evocc -E");
    assert_eq!(words(&mine), words(&theirs));
    assert_eq!(
        words(&mine),
        [
            "once_body",
            "twice_body",
            "plain_body",
            "nested_body",
            "guarded_body",
            "end_main"
        ]
    );
}

/// GCC 14 gives `#import` the same meaning on Linux.
#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn import_matches_gcc_14() {
    let work = Work::new("import-gcc");
    work.write("once.h", "once_body\n");
    work.write("plain.h", "plain_body\n");
    work.write("nested.h", "nested_body\n#import \"plain.h\"\n");
    work.write(
        "main.c",
        "#import \"once.h\"\n#include \"once.h\"\n#include \"plain.h\"\n#include \"nested.h\"\n#include \"plain.h\"\nint end_main;\n",
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../sysroot")
        .join(LINUX);
    let mine = work.evocc(&[
        &format!("--target={LINUX}"),
        &format!("--sysroot={}", root.display()),
        "-E",
        "-I.",
        "main.c",
    ]);
    assert!(mine.status.success(), "{}", text(&mine));
    let theirs = Command::new("container")
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
            "-w",
            "/work",
            "evo-nexlab:sealed",
            "gcc-14",
            "-std=gnu11",
            "-w",
            "-E",
            "-P",
            "-I.",
            "main.c",
        ])
        .output()
        .unwrap();
    assert!(theirs.status.success(), "{}", text(&theirs));
    let split = |output: &Output| {
        String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(split(&mine), split(&theirs));
    assert_eq!(
        split(&mine),
        ["once_body", "plain_body", "nested_body", "int", "end_main;"]
    );
}

#[test]
fn framework_options_are_refused_on_linux() {
    let work = Work::new("linux");
    work.write("main.c", "int x;\n");
    for args in [
        &["-F", "F1"][..],
        &["-FF1"][..],
        &["-iframework", "F1"][..],
        &["-iframeworkF1"][..],
    ] {
        let mut argv = vec!["--target=aarch64-unknown-linux-gnu", "--emit=asm", "main.c"];
        argv.extend_from_slice(args);
        let output = work.evocc(&argv);
        assert_eq!(output.status.code(), Some(1), "{args:?}\n{}", text(&output));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unsupported option"),
            "{args:?}\n{}",
            text(&output)
        );
    }
}

/// A program that creates, prints and releases a `CFString`.
#[cfg(target_os = "macos")]
const CORE_FOUNDATION: &str = r#"
#include <CoreFoundation/CoreFoundation.h>
#include <stdio.h>
int main(void)
{
    CFStringRef s = CFStringCreateWithCString(kCFAllocatorDefault, "hello, CF",
                                              kCFStringEncodingUTF8);
    if (s == NULL)
        return 1;
    char buffer[64];
    if (!CFStringGetCString(s, buffer, sizeof buffer, kCFStringEncodingUTF8))
        return 2;
    printf("%s %ld\n", buffer, (long)CFStringGetLength(s));
    fflush(stdout);
    CFShow(s);
    CFStringRef copy = CFStringCreateCopy(NULL, s);
    printf("%d\n", CFStringCompare(s, copy, 0) == kCFCompareEqualTo);
    CFRelease(copy);
    CFRelease(s);
    return 0;
}
"#;

#[cfg(target_os = "macos")]
#[test]
fn a_core_foundation_program_agrees_with_apple_clang() {
    let work = Work::new("core-foundation");
    work.write("cf.c", CORE_FOUNDATION);
    for level in ["-O0", "-O2"] {
        let mine = format!("cf-evocc{level}");
        let theirs = format!("cf-clang{level}");
        succeeded(
            &work.evocc(&[level, "cf.c", "-framework", "CoreFoundation", "-o", &mine]),
            "evocc",
        );
        succeeded(
            &work.clang(&[level, "cf.c", "-framework", "CoreFoundation", "-o", &theirs]),
            "clang",
        );
        let mine = work.run(&mine);
        let theirs = work.run(&theirs);
        succeeded(&mine, "EvoCC program");
        assert_eq!(mine.status.code(), theirs.status.code(), "{level}");
        assert_eq!(mine.stdout, theirs.stdout, "{level}");
        assert_eq!(mine.stderr, theirs.stderr, "{level}");
        assert_eq!(
            String::from_utf8_lossy(&mine.stdout),
            "hello, CF 9\n1\n",
            "{level}"
        );
    }
}

/// A framework of the test: a header and a library in `lib/Probe.framework`.
#[cfg(target_os = "macos")]
fn probe_framework(work: &Work) {
    work.write(
        "lib/Probe.framework/Headers/Probe.h",
        "int probe_value(int);\n#define PROBE_BIAS 3\n",
    );
    let source = work.write("probe.c", "int probe_value(int x) { return x * 7; }\n");
    succeeded(
        &work.clang(&[
            "-dynamiclib",
            "-install_name",
            "@rpath/Probe.framework/Probe",
            source.to_str().unwrap(),
            "-o",
            "lib/Probe.framework/Probe",
        ]),
        "clang -dynamiclib",
    );
    work.write(
        "main.c",
        "#include <Probe/Probe.h>\n#include <stdio.h>\nint main(void) { printf(\"%d\\n\", probe_value(6) + PROBE_BIAS); return 0; }\n",
    );
}

#[cfg(target_os = "macos")]
#[test]
fn a_framework_directory_reaches_the_headers_and_the_linker() {
    let work = Work::new("link");
    probe_framework(&work);
    let rpath = format!("-Wl,-rpath,{}", work.0.join("lib").display());
    for option in [
        &["-F", "lib"][..],
        &["-Flib"][..],
        &["-iframework", "lib"][..],
    ] {
        for level in ["-O0", "-O2"] {
            let mut args = vec![level, "main.c"];
            args.extend_from_slice(option);
            args.extend(["-framework", "Probe", &rpath, "-o", "main-evocc"]);
            succeeded(&work.evocc(&args), "evocc");
            let output = work.run("main-evocc");
            succeeded(&output, "EvoCC program");
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "45\n",
                "{option:?}"
            );
        }
    }
    // Without the framework directory, neither compiler finds the header.
    let theirs = work.clang(&["-fsyntax-only", "main.c"]);
    let mine = work.evocc(&["-fsyntax-only", "main.c"]);
    assert!(
        !theirs.status.success() && !mine.status.success(),
        "{}",
        text(&mine)
    );
    assert!(
        String::from_utf8_lossy(&mine.stderr).contains("E0105"),
        "{}",
        text(&mine)
    );
}

/// The SDK framework directories come after the system headers, so an
/// `-isystem` directory hides a framework header of the same name, as in
/// Apple clang.
#[cfg(target_os = "macos")]
#[test]
fn the_sdk_framework_directories_follow_the_system_headers() {
    let work = Work::new("sdk");
    work.write("S1/CoreFoundation/CFBase.h", "s1_cfbase\n");
    work.write(
        "main.c",
        "#include <CoreFoundation/CFBase.h>\n#if __has_include(<IOKit/IOKitLib.h>)\nsdk_iokit\n#endif\n",
    );
    let theirs = work.clang(&["-E", "-P", "main.c", "-isystem", "S1"]);
    succeeded(&theirs, "clang -E");
    let mine = work.evocc(&["-E", "main.c", "-isystem", "S1"]);
    succeeded(&mine, "evocc -E");
    assert_eq!(words(&mine), ["s1_cfbase", "sdk_iokit"]);
    assert_eq!(words(&mine), words(&theirs));
}
