//! Small build-system contracts, independent of application replays.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn compiler(args: &[&str], source: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let written = child.stdin.take().unwrap().write_all(source.as_bytes());
    let output = child.wait_with_output().unwrap();
    if output.status.success() {
        written.unwrap();
    } else if let Err(error) = written {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    output
}

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn both_targets_predefine_the_actual_evocc_identity_and_version() {
    for target in ["aarch64-apple-darwin", "aarch64-unknown-linux-gnu"] {
        let target = format!("--target={target}");
        let output = success(compiler(
            &["-E", "-nostdinc", &target, "-x", "c", "-"],
            "#if __GNUC__ <= 0\n#error GNU compatibility macro must remain available\n#endif\nEVOCC_ID __EVOCC__\nEVOCC_RELEASE __EVOCC_VERSION__\n",
        ));
        let tokens = String::from_utf8(output.stdout)
            .unwrap()
            .split_whitespace()
            .collect::<String>();
        assert_eq!(
            tokens,
            format!("EVOCC_ID1EVOCC_RELEASE\"{}\"", env!("CARGO_PKG_VERSION"))
        );
    }
}

#[test]
fn no_error_obeys_order_without_overriding_specific_warning_controls() {
    let cases: &[(&[&str], i32, Option<&str>)] = &[
        (&["-Werror", "-Wno-error"], 0, Some("warning")),
        (&["-Wno-error", "-Werror"], 1, Some("error")),
        (
            &["-Werror=unused-parameter", "-Wno-error"],
            1,
            Some("error"),
        ),
        (&["-Werror", "-Wno-error", "-Wno-unused-parameter"], 0, None),
        (
            &["-Werror=unused-parameter", "-Wno-unused-parameter"],
            0,
            None,
        ),
        (
            &["-Wno-unused-parameter", "-Werror=unused-parameter"],
            1,
            Some("error"),
        ),
    ];
    for &(flags, status, level) in cases {
        let mut args = vec![
            "-fsyntax-only",
            "-nostdinc",
            "--error-format=json",
            "--color=never",
            "-Wall",
        ];
        args.extend_from_slice(flags);
        args.extend_from_slice(&["-x", "c", "-"]);
        let output = compiler(&args, "int f(int unused) { return 0; }\n");
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert_eq!(output.status.code(), Some(status), "{flags:?}: {stderr}");
        match level {
            Some(level) => {
                assert!(stderr.contains("\"code\":\"W0002\""), "{stderr}");
                assert!(stderr.contains(&format!("\"level\":\"{level}\"")));
            }
            None => assert!(stderr.is_empty(), "{flags:?}: {stderr}"),
        }
    }
}

struct Work(PathBuf);
impl Work {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "evocc-build-systems-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }

    fn command(&self, program: &str, linux: bool) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.0)
            .env_remove("EVOCC_AS")
            .env_remove("EVOCC_CC")
            .env_remove("EVOCC_SYSROOT")
            .env_remove("EVOCC_BENCH_SYSROOT");
        if linux && cfg!(target_os = "macos") {
            command.env("EVOCC_AS", "clang --target=aarch64-linux-gnu");
        }
        command
    }

    fn build(&self, linux: bool) -> PathBuf {
        let toolchain = root().join("cmake/evocc.cmake");
        assert!(toolchain.is_file(), "missing {}", toolchain.display());
        std::fs::write(
            self.0.join("CMakeLists.txt"),
            r#"cmake_minimum_required(VERSION 3.16)
project(EvoCCBuildContract LANGUAGES C)
if(EVOCC_TEST_OBJECT_ONLY)
  add_library(probe OBJECT probe.c)
  file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/artifact"
       CONTENT "$<TARGET_OBJECTS:probe>")
else()
  add_executable(probe probe.c)
  file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/artifact"
       CONTENT "$<TARGET_FILE:probe>")
endif()
"#,
        )
        .unwrap();
        std::fs::write(
            self.0.join("probe.c"),
            "#include <stdint.h>\n#ifndef __EVOCC__\n#error expected EvoCC\n#endif\nint main(void) { return sizeof(uintptr_t) == 8 ? 0 : 1; }\n",
        )
        .unwrap();
        let mut configure = self.command("cmake", linux);
        configure
            .args(["-S", ".", "-B", "build", "-G", "Unix Makefiles"])
            .arg(format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()))
            .arg(format!(
                "-DCMAKE_C_COMPILER={}",
                env!("CARGO_BIN_EXE_evocc")
            ));
        if linux {
            let sysroot = self.0.join("linux headers");
            std::os::unix::fs::symlink(root().join("sysroot/aarch64-unknown-linux-gnu"), &sysroot)
                .unwrap();
            configure
                .args([
                    "-DEVOCC_TARGET=aarch64-unknown-linux-gnu",
                    "-DEVOCC_TEST_OBJECT_ONLY=ON",
                ])
                .arg(format!("-DCMAKE_SYSROOT={}", sysroot.display()));
        }
        success(configure.output().unwrap());
        success(
            self.command("cmake", linux)
                .args(["--build", "build", "--parallel", "2"])
                .output()
                .unwrap(),
        );
        let artifact = std::fs::read_to_string(self.0.join("build/artifact")).unwrap();
        assert!(!artifact.contains(';'), "expected exactly one artifact");
        PathBuf::from(artifact.trim())
    }
}
impl Drop for Work {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn stock_cmake_builds_and_runs_a_native_executable() {
    let work = Work::new();
    let executable = work.build(false);
    success(Command::new(executable).output().unwrap());
}

#[test]
fn stock_cmake_cross_builds_an_aarch64_linux_object() {
    let work = Work::new();
    let object = std::fs::read(work.build(true)).unwrap();
    assert!(object.len() >= 20);
    assert_eq!(&object[..4], b"\x7fELF");
    assert_eq!(object[4], 2, "ELF64");
    assert_eq!(object[5], 1, "little endian");
    assert_eq!(u16::from_le_bytes([object[16], object[17]]), 1, "ET_REL");
    assert_eq!(u16::from_le_bytes([object[18], object[19]]), 183, "AArch64");
}

#[test]
fn make_rebuilds_after_a_header_changes_using_the_emitted_depfile() {
    let work = Work::new();
    let source = "#include \"answer.h\"\nint main(void) { return ANSWER - 42; }\n";
    std::fs::write(work.0.join("main.c"), source).unwrap();
    std::fs::write(work.0.join("answer.h"), "#define ANSWER 42\n").unwrap();
    std::fs::write(
        work.0.join("Makefile"),
        "all: main\nmain: main.o\n\t$(CC) main.o -o $@\nmain.o: main.c\n\t$(CC) -MMD -MF main.d -c main.c -o $@\n-include main.d\n",
    )
    .unwrap();
    let make = || {
        let mut command = work.command("make", false);
        command.arg(format!("CC={}", env!("CARGO_BIN_EXE_evocc")));
        command
    };
    success(make().output().unwrap());
    success(Command::new(work.0.join("main")).output().unwrap());
    assert_eq!(
        make().args(["-q", "main"]).output().unwrap().status.code(),
        Some(0)
    );
    let depfile = std::fs::read_to_string(work.0.join("main.d")).unwrap();
    assert!(depfile.contains("main.c") && depfile.contains("answer.h"));
    // Cross the timestamp resolution of make and coarse filesystems.
    std::thread::sleep(Duration::from_millis(1100));
    std::fs::write(work.0.join("answer.h"), "#define ANSWER 43\n").unwrap();
    assert_eq!(
        make().args(["-q", "main"]).output().unwrap().status.code(),
        Some(1)
    );
    success(make().output().unwrap());
    let run = Command::new(work.0.join("main")).output().unwrap();
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(
        std::fs::read_to_string(work.0.join("main.c")).unwrap(),
        source
    );
}

#[test]
fn pkg_config_zlib_flags_build_a_native_round_trip() {
    let work = Work::new();
    std::fs::write(
        work.0.join("roundtrip.c"),
        r#"#include <zlib.h>
#include <string.h>
int main(void) {
    const unsigned char input[] = "a build system passes library flags unchanged";
    unsigned char compressed[256], restored[sizeof input];
    uLongf size = sizeof compressed, restored_size = sizeof restored;
    if (compress(compressed, &size, input, sizeof input) != Z_OK) return 1;
    if (uncompress(restored, &restored_size, compressed, size) != Z_OK) return 2;
    return restored_size != sizeof input || memcmp(input, restored, sizeof input);
}
"#,
    )
    .unwrap();
    // The checked interpreter can refuse external library calls. Keep its result
    // separate from the native ABI check; a refusal is not a source UB finding.
    let _check = work
        .command(env!("CARGO_BIN_EXE_evo"), false)
        .args(["check", "roundtrip.c", "--json"])
        .output()
        .unwrap();
    success(
        work.command("python3", false)
            .args([
                "-c",
                "import shlex, subprocess, sys; flags = shlex.split(subprocess.check_output(['pkg-config', '--cflags', '--libs', 'zlib'], text=True)); subprocess.run([sys.argv[1], 'roundtrip.c', '-o', 'roundtrip', *flags], check=True)",
                env!("CARGO_BIN_EXE_evocc"),
            ])
            .output()
            .unwrap(),
    );
    success(Command::new(work.0.join("roundtrip")).output().unwrap());
}

#[test]
fn native_stack_protector_uses_the_host_object_format() {
    let work = Work::new();
    std::fs::write(
        work.0.join("protected.c"),
        "int main(void) { volatile char bytes[32]; bytes[0] = 7; bytes[31] = 9; return bytes[0] + bytes[31] - 16; }\n",
    )
    .unwrap();
    success(
        work.command(env!("CARGO_BIN_EXE_evo"), false)
            .args(["check", "protected.c", "--json"])
            .output()
            .unwrap(),
    );
    success(
        work.command(env!("CARGO_BIN_EXE_evocc"), false)
            .args(["-fstack-protector-strong", "protected.c", "-o", "protected"])
            .output()
            .unwrap(),
    );
    success(Command::new(work.0.join("protected")).output().unwrap());
}
