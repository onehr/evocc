//! Clang's availability attribute on Darwin (EvoIR 10.1.9, D19). The SDK
//! marks declarations with it through `__API_AVAILABLE` and related macros.
//! A use of a declaration that is unavailable for the deployment target is
//! `E2962` where Apple clang refuses it, and a declaration introduced after
//! the deployment target is a weak import where Apple clang makes it one.
//! EvoCC assumes macOS 11.0 unless `-mmacosx-version-min`,
//! `-mmacos-version-min` or `MACOSX_DEPLOYMENT_TARGET` names another target.
//! The expected positions and symbols are the ones of Apple clang 21 with
//! `-mmacosx-version-min=11.0`; the crossed tests run it again. Linux keeps
//! the attributes without effect.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A fresh directory for one compilation of a test.
fn scratch(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "evocc-darwin-availability-{label}-{}-{}",
        std::process::id(),
        NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

const DARWIN: &str = "aarch64-apple-darwin";
const LINUX: &str = "aarch64-unknown-linux-gnu";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/darwin-availability")
        .join(name)
}

fn text(output: &Output) -> String {
    format!(
        "{:?}\n{}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn evocc(args: &[&str], environment: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command
        .args(args)
        .env_remove("MACOSX_DEPLOYMENT_TARGET")
        .env_remove("SDKROOT");
    for (name, value) in environment {
        command.env(name, value);
    }
    command.output().unwrap()
}

/// The `LINE:COLUMN` of each `code` diagnostic of a survey.
fn positions(output: &Output, code: &str) -> Vec<String> {
    let marker = format!(": error[{code}]");
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.contains(&marker))
        .map(|line| {
            let location = line.split(&marker).next().unwrap();
            let mut parts = location.rsplitn(3, ':');
            let column = parts.next().unwrap();
            let line = parts.next().unwrap();
            format!("{line}:{column}")
        })
        .collect()
}

fn survey(name: &str, target: &str) -> Output {
    let path = fixture(name);
    evocc(
        &[
            &format!("--target={target}"),
            "--survey",
            "--error-format=short",
            "--parse-only",
            path.to_str().unwrap(),
        ],
        &[],
    )
}

/// The uses that Apple clang 21 refuses in each fixture.
const REFUSED: [(&str, &[&str]); 6] = [
    ("versions.c", &["10:23"]),
    (
        "uses.c",
        &[
            "13:23", "14:23", "16:23", "19:23", "20:23", "25:18", "26:31",
        ],
    ),
    (
        "entities.c",
        &["7:1", "8:8", "9:23", "10:38", "14:30", "15:8"],
    ),
    (
        "contexts.c",
        &["8:12", "9:41", "13:16", "14:19", "15:1", "16:15"],
    ),
    ("declarators.c", &["4:1", "9:17", "10:18"]),
    ("records.c", &["7:60", "9:12"]),
];

#[test]
fn uses_of_unavailable_declarations_are_refused_where_apple_clang_refuses_them() {
    for (name, expected) in REFUSED {
        let output = survey(name, DARWIN);
        assert_eq!(output.status.code(), Some(1), "{name}\n{}", text(&output));
        assert_eq!(
            positions(&output, "E2962"),
            expected,
            "{name}\n{}",
            text(&output)
        );
        let other = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.contains(": error[") && !line.contains("E2962"))
            .count();
        assert_eq!(other, 0, "{name}\n{}", text(&output));
    }
    let output = survey("uses.c", DARWIN);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for reason in [
        "unavailable-declaration: gone: not available on macOS",
        "unavailable-declaration: obs: obsoleted in macOS 10.15",
        "unavailable-declaration: strict_later: introduced in macOS 13.0",
        "unavailable-declaration: plain_unavail\n",
        "unavailable-declaration: msg_unavail: no way",
    ] {
        assert!(stderr.contains(reason), "{reason}\n{stderr}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_refused_uses_are_the_ones_of_apple_clang() {
    for (name, _) in REFUSED {
        let output = Command::new("clang")
            .args(["-mmacosx-version-min=11.0", "-fsyntax-only"])
            .arg(fixture(name))
            .output()
            .unwrap();
        let theirs: Vec<String> = String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| line.contains(": error: ") && line.contains("is unavailable"))
            .map(|line| {
                let mut parts = line.split(':');
                parts.next();
                format!("{}:{}", parts.next().unwrap(), parts.next().unwrap())
            })
            .collect();
        assert_eq!(
            positions(&survey(name, DARWIN), "E2962"),
            theirs,
            "{name}\n{}",
            text(&output)
        );
    }
}

/// The weak references of a compilation, sorted.
fn weak_references(output: &Output) -> Vec<String> {
    let mut symbols: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().strip_prefix(".weak_reference "))
        .map(str::to_owned)
        .collect();
    symbols.sort();
    symbols
}

fn darwin_assembly(name: &str, deployment: Option<&str>) -> Output {
    let path = fixture(name);
    let option = deployment.map(|version| format!("-mmacosx-version-min={version}"));
    let mut args = vec![format!("--target={DARWIN}"), "-O2".to_owned()];
    args.extend(option);
    args.extend(["--emit=asm".to_owned(), path.to_str().unwrap().to_owned()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = evocc(&args, &[]);
    assert!(output.status.success(), "{name}\n{}", text(&output));
    output
}

/// The weak references that Apple clang 21 emits for each fixture and
/// deployment target.
const WEAK: [(&str, &str, &[&str]); 6] = [
    (
        "weak-imports.c",
        "11.0",
        &["_later", "_later_object", "_redeclared", "_wi"],
    ),
    ("weak-imports.c", "13.0", &["_redeclared", "_wi"]),
    ("weak-imports.c", "15.0", &["_wi"]),
    (
        "merges.c",
        "11.0",
        &[
            "_mixed", "_same", "_t1", "_t2", "_t3", "_t5", "_t6", "_vers2", "_vers3",
        ],
    ),
    ("merges.c", "13.0", &["_t2", "_t3"]),
    ("merges.c", "15.0", &[]),
];

#[test]
fn declarations_introduced_after_the_deployment_target_are_weak_imports() {
    for (name, deployment, expected) in WEAK {
        let output = darwin_assembly(name, Some(deployment));
        assert_eq!(weak_references(&output), expected, "{name} {deployment}");
    }
    // Without an option or MACOSX_DEPLOYMENT_TARGET, EvoCC assumes 11.0.
    assert_eq!(
        weak_references(&darwin_assembly("weak-imports.c", None)),
        WEAK[0].2
    );
    // The address of a weak import is compared at run time.
    let output = darwin_assembly("weak-imports.c", Some("11.0"));
    let assembly = String::from_utf8_lossy(&output.stdout);
    let body = assembly.split("_h:").nth(1).unwrap();
    assert!(
        body.contains("_later@GOTPAGE") && body.contains("cmp"),
        "{assembly}"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn weak_imports_are_the_ones_of_apple_clang() {
    for (name, deployment, _) in WEAK {
        let output = Command::new("clang")
            .args([
                &format!("-mmacosx-version-min={deployment}"),
                "-O2",
                "-S",
                "-o",
                "-",
            ])
            .arg(fixture(name))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
        assert_eq!(
            weak_references(&darwin_assembly(name, Some(deployment))),
            weak_references(&output),
            "{name} {deployment}"
        );
    }
}

/// The deployment macro of a Darwin compilation.
fn deployment_macro(options: &[&str], environment: &[(&str, &str)]) -> Result<String, String> {
    let directory = scratch("macro");
    let source = directory.join("macro.c");
    std::fs::write(
        &source,
        "VERSION __ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__\n",
    )
    .unwrap();
    let mut args = vec!["--target=aarch64-apple-darwin", "-E"];
    args.extend_from_slice(options);
    args.push(source.to_str().unwrap());
    let output = evocc(&args, environment);
    let _ = std::fs::remove_dir_all(&directory);
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(stdout
        .split("VERSION")
        .nth(1)
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_owned())
}

/// Options, environment and the expected deployment macro.
type DeploymentCase = (
    &'static [&'static str],
    &'static [(&'static str, &'static str)],
    &'static str,
);

#[test]
fn the_deployment_target_is_selected_as_apple_clang_selects_it() {
    let cases: [DeploymentCase; 8] = [
        (&[], &[], "110000"),
        (&["-mmacosx-version-min=12.3.1"], &[], "120301"),
        (
            &["-mmacos-version-min=14.2", "-mmacosx-version-min=12.1"],
            &[],
            "120100",
        ),
        (&[], &[("MACOSX_DEPLOYMENT_TARGET", "13.1")], "130100"),
        (
            &["-mmacos-version-min=14.2"],
            &[("MACOSX_DEPLOYMENT_TARGET", "13.1")],
            "140200",
        ),
        (&["-mmacosx-version-min=10.15"], &[], "101500"),
        (&["-mmacosx-version-min=10.9.5"], &[], "1095"),
        (&["-mmacosx-version-min=99.0"], &[], "990000"),
    ];
    for (options, environment, expected) in cases {
        assert_eq!(
            deployment_macro(options, environment).as_deref(),
            Ok(expected),
            "{options:?} {environment:?}"
        );
    }
    for (options, environment) in [
        (&["-mmacosx-version-min=13.4.5.6"][..], &[][..]),
        (&["-mmacosx-version-min=abc"][..], &[][..]),
        (&["-mmacosx-version-min=10_14"][..], &[][..]),
        (&["-mmacosx-version-min=9.0"][..], &[][..]),
        (&["-mmacosx-version-min=12.100"][..], &[][..]),
        (&["-mmacosx-version-min=100.0"][..], &[][..]),
        (&[][..], &[("MACOSX_DEPLOYMENT_TARGET", "bad")][..]),
    ] {
        let error = deployment_macro(options, environment).unwrap_err();
        assert!(
            error.contains("invalid version number"),
            "{options:?} {error}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn the_deployment_macro_is_the_one_of_apple_clang() {
    for version in ["11.0", "12.3.1", "13", "10.15", "10.9.5", "99.0"] {
        let output = Command::new("clang")
            .args([
                &format!("-mmacosx-version-min={version}"),
                "-dM",
                "-E",
                "-x",
                "c",
            ])
            .arg("/dev/null")
            .output()
            .unwrap();
        let theirs = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| {
                line.strip_prefix("#define __ENVIRONMENT_MAC_OS_X_VERSION_MIN_REQUIRED__ ")
            })
            .unwrap()
            .to_owned();
        let option = format!("-mmacosx-version-min={version}");
        assert_eq!(deployment_macro(&[&option], &[]), Ok(theirs), "{version}");
    }
}

const QUERIES: [&str; 14] = [
    "__has_attribute(availability)",
    "__has_attribute(__availability__)",
    "__has_attribute(unavailable)",
    "__has_attribute(weak_import)",
    "__has_feature(attribute_availability)",
    "__has_feature(attribute_availability_with_message)",
    "__has_feature(attribute_availability_app_extension)",
    "__has_feature(attribute_availability_with_version_underscores)",
    "__has_feature(attribute_availability_tvos)",
    "__has_feature(attribute_availability_watchos)",
    "__has_feature(attribute_availability_driverkit)",
    "__has_feature(attribute_availability_with_strict)",
    "__has_feature(attribute_availability_with_replacement)",
    "__has_feature(attribute_availability_swift)",
];

fn query_source() -> String {
    QUERIES
        .iter()
        .enumerate()
        .map(|(index, query)| format!("#if {query}\nyes{index}\n#else\nno{index}\n#endif\n"))
        .collect()
}

fn answers(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

#[test]
fn darwin_answers_the_availability_feature_queries_and_linux_does_not() {
    let directory = scratch("queries");
    let source = directory.join("queries.c");
    std::fs::write(&source, query_source()).unwrap();
    for (target, word) in [(DARWIN, "yes"), (LINUX, "no")] {
        let output = evocc(
            &[
                &format!("--target={target}"),
                "-E",
                source.to_str().unwrap(),
            ],
            &[],
        );
        assert!(output.status.success(), "{}", text(&output));
        let expected: Vec<String> = (0..QUERIES.len()).map(|i| format!("{word}{i}")).collect();
        assert_eq!(answers(&output), expected, "{target}");
    }
    #[cfg(target_os = "macos")]
    {
        let theirs = Command::new("clang")
            .args(["-E", "-P"])
            .arg(&source)
            .output()
            .unwrap();
        let mine = evocc(
            &[
                &format!("--target={DARWIN}"),
                "-E",
                source.to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(answers(&mine), answers(&theirs));
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn linux_keeps_the_attributes_without_effect() {
    let output = survey("uses.c", LINUX);
    assert!(output.status.success(), "{}", text(&output));
    let path = fixture("weak-imports.c");
    let output = evocc(
        &[
            &format!("--target={LINUX}"),
            "-O2",
            "--emit=asm",
            path.to_str().unwrap(),
        ],
        &[],
    );
    assert!(output.status.success(), "{}", text(&output));
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains(".weak"),
        "{}",
        text(&output)
    );
    let output = evocc(
        &[
            &format!("--target={LINUX}"),
            "-mmacosx-version-min=11.0",
            "--parse-only",
            path.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(output.status.code(), Some(1), "{}", text(&output));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported option"));
}

fn compile_source(target: &str, source: &str, extra: &[&str]) -> Output {
    let directory = scratch("source");
    let path = directory.join("source.c");
    std::fs::write(&path, source).unwrap();
    let mut args = vec![format!("--target={target}")];
    args.extend(extra.iter().map(|arg| (*arg).to_owned()));
    args.extend(["--parse-only".to_owned(), path.to_str().unwrap().to_owned()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = evocc(&args, &[]);
    let _ = std::fs::remove_dir_all(&directory);
    output
}

fn first_code(output: &Output) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let start = stderr.find("error[")? + "error[".len();
    Some(stderr[start..start + 5].to_owned())
}

#[test]
fn availability_versions_are_read_on_darwin() {
    let source = "int f(void) __attribute__((availability(macos,introduced=10.14.4)));\n\
                  int g(void) __attribute__((availability(macos,introduced=10_14_4,deprecated=12.0)));\n\
                  int h(void) { return f() + g(); }\n";
    assert!(compile_source(DARWIN, source, &[]).status.success());
    // GCC 14 refuses the preprocessing number, and Linux keeps the refusal.
    assert_eq!(
        first_code(&compile_source(LINUX, source, &[])).as_deref(),
        Some("E0112")
    );
    for malformed in [
        "int f(void) __attribute__((availability(macos,introduced=1.2.3.4)));\n",
        "int f(void) __attribute__((availability(macos,since=13.0)));\n",
        "int f(void) __attribute__((availability(macos,message=13)));\n",
        "int f(void) __attribute__((availability(13,introduced=13.0)));\n",
    ] {
        assert_eq!(
            first_code(&compile_source(DARWIN, malformed, &[])).as_deref(),
            Some("E2903"),
            "{malformed}"
        );
    }
}

#[test]
fn an_attribute_pragma_with_an_effect_is_refused_on_darwin() {
    let refused = [
        "#pragma clang attribute push (__attribute__((availability(macos,unavailable))), apply_to = function)\nint f(void);\n#pragma clang attribute pop\n",
        "_Pragma(\"clang attribute push (__attribute__((availability(macos,introduced=13.0))), apply_to = any(function, variable))\")\nint f(void);\n_Pragma(\"clang attribute pop\")\n",
        "#pragma clang attribute push (__attribute__((weak_import)), apply_to = function)\nint f(void);\n#pragma clang attribute pop\n",
    ];
    for source in refused {
        assert_eq!(
            first_code(&compile_source(DARWIN, source, &[])).as_deref(),
            Some("E0114"),
            "{source}"
        );
        assert!(
            compile_source(LINUX, source, &[]).status.success(),
            "{source}"
        );
    }
    let accepted = [
        "#pragma clang attribute push (__attribute__((availability(macos,introduced=10.0,deprecated=13.0,message=\"old\"))), apply_to = function)\nint f(void);\n#pragma clang attribute pop\n",
        "#pragma clang attribute push (__attribute__((availability(ios,unavailable))), apply_to = function)\nint f(void);\n#pragma clang attribute pop\n",
        "#pragma clang attribute push (__attribute__((availability(macos,introduced=13.0))), apply_to = function)\nint f(void);\n#pragma clang attribute pop\n",
    ];
    for (index, source) in accepted.iter().enumerate() {
        // The last push takes effect only before macOS 13.
        let options: &[&str] = if index == 2 {
            &["-mmacosx-version-min=13.0"]
        } else {
            &[]
        };
        let output = compile_source(DARWIN, source, options);
        assert!(output.status.success(), "{source}\n{}", text(&output));
    }
}

#[cfg(target_os = "macos")]
mod sdk {
    use super::*;

    fn work(label: &str) -> PathBuf {
        scratch(label)
    }

    #[test]
    fn an_sdk_declaration_unavailable_on_macos_is_refused_as_apple_clang_refuses_it() {
        let directory = work("sdk-unavailable");
        let source = directory.join("zone.c");
        std::fs::write(
            &source,
            "#include <CoreFoundation/CoreFoundation.h>\nint main(void) {\n    return CFAllocatorCreateWithZone(NULL, NULL) != NULL;\n}\n",
        )
        .unwrap();
        let mine = evocc(&["-fsyntax-only", source.to_str().unwrap()], &[]);
        assert_eq!(
            first_code(&mine).as_deref(),
            Some("E2962"),
            "{}",
            text(&mine)
        );
        assert!(
            String::from_utf8_lossy(&mine.stderr).contains("zone.c:3:12"),
            "{}",
            text(&mine)
        );
        let theirs = Command::new("clang")
            .args(["-fsyntax-only"])
            .arg(&source)
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&theirs.stderr)
                .contains("zone.c:3:12: error: 'CFAllocatorCreateWithZone' is unavailable"),
            "{}",
            text(&theirs)
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// `strchrnul` is introduced in macOS 15.4 and `freadlink` in 13.0.
    const LATER: &str = r#"
#include <stdio.h>
#include <string.h>
#include <unistd.h>
int main(void)
{
    printf("%d %d\n", strchrnul != NULL, freadlink != NULL);
    printf("%s\n", strchrnul("hello", 'l'));
    return 0;
}
"#;

    #[test]
    fn sdk_functions_introduced_later_are_weak_imports_as_in_apple_clang() {
        let directory = work("sdk-weak");
        let source = directory.join("later.c");
        std::fs::write(&source, LATER).unwrap();
        for (deployment, expected) in [
            ("11.0", &["_freadlink", "_strchrnul"][..]),
            ("13.0", &["_strchrnul"][..]),
            ("15.4", &[][..]),
        ] {
            let option = format!("-mmacosx-version-min={deployment}");
            let mine = evocc(
                &[&option, "-O2", "-S", "-o", "-", source.to_str().unwrap()],
                &[],
            );
            assert!(mine.status.success(), "{}", text(&mine));
            let theirs = Command::new("clang")
                .args([&option, "-O2", "-S", "-o", "-"])
                .arg(&source)
                .output()
                .unwrap();
            assert_eq!(weak_references(&mine), expected, "{deployment}");
            assert_eq!(
                weak_references(&mine),
                weak_references(&theirs),
                "{deployment}"
            );
            for level in ["-O0", "-O2"] {
                let binary = directory.join(format!("later-{deployment}{level}"));
                let built = evocc(
                    &[
                        &option,
                        level,
                        source.to_str().unwrap(),
                        "-o",
                        binary.to_str().unwrap(),
                    ],
                    &[],
                );
                assert!(built.status.success(), "{}", text(&built));
                let run = Command::new(&binary).output().unwrap();
                assert_eq!(String::from_utf8_lossy(&run.stdout), "1 1\nllo\n");
            }
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn sdk_headers_with_availability_ranges_compile() {
        let directory = work("sdk-ranges");
        let source = directory.join("ranges.c");
        std::fs::write(
            &source,
            "#include <dns_util.h>\n#include <xar/xar.h>\n#include <term.h>\n",
        )
        .unwrap();
        let output = evocc(&["-fsyntax-only", source.to_str().unwrap()], &[]);
        assert!(output.status.success(), "{}", text(&output));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
