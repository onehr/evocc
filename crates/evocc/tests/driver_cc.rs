//! The cc command line: actions, outputs, several inputs, dependency rules,
//! ignored options, the version line and input errors.
//!
//! The tests that assemble use the platform tools of the host target. The
//! tests that link and run a program run on a macOS host only.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A fresh directory with copies of the driver fixtures, removed on drop.
struct Work(PathBuf);

impl Work {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "evocc-driver-cc-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/driver");
        for name in ["main.c", "helper.c", "helper.h", "variadic.c"] {
            std::fs::copy(fixtures.join(name), path.join(name)).unwrap();
        }
        Self(path)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evocc"))
            .args((!args.is_empty()).then_some("--error-format=short"))
            .current_dir(&self.0)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .unwrap()
    }

    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.file(name)).unwrap()
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn nonempty(path: &Path) {
    let size = std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("{} is absent: {error}", path.display()))
        .len();
    assert!(size > 0, "{} is empty", path.display());
}

#[test]
fn assembly_and_object_outputs_exist_with_default_and_explicit_names() {
    let work = Work::new("outputs");
    succeeded(&work.run(&["-S", "helper.c"]));
    nonempty(&work.file("helper.s"));
    assert!(work.read("helper.s").contains("helper_twice"));

    succeeded(&work.run(&["-S", "main.c", "-o", "renamed.s"]));
    nonempty(&work.file("renamed.s"));
    assert!(!work.file("main.s").exists());

    succeeded(&work.run(&["-c", "helper.c"]));
    nonempty(&work.file("helper.o"));

    succeeded(&work.run(&["-c", "main.c", "-o", "objects-main.o"]));
    nonempty(&work.file("objects-main.o"));

    // Several inputs with -c write one object each, beside the command.
    succeeded(&work.run(&["-c", "-O2", "main.c", "helper.c", "-DUNUSED=1"]));
    nonempty(&work.file("main.o"));
}

#[test]
fn preprocessing_writes_standard_output_or_the_named_file() {
    let work = Work::new("preprocess");
    let output = work.run(&["-E", "main.c"]);
    succeeded(&output);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("helper_twice(20)"), "{text}");
    succeeded(&work.run(&["-E", "main.c", "-o", "main.i"]));
    assert_eq!(work.read("main.i"), text);
}

#[test]
fn several_outputs_cannot_share_one_output_name() {
    let work = Work::new("several");
    let output = work.run(&["-c", "main.c", "helper.c", "-o", "both.o"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("cannot specify -o with -c, -S or -E with multiple files"));
}

#[test]
fn dependency_rules_name_the_user_headers() {
    let work = Work::new("deps");
    succeeded(&work.run(&["-c", "main.c", "-MMD", "-MF", "main.dep", "-o", "main.o"]));
    assert_eq!(work.read("main.dep"), "main.o: main.c helper.h\n");
    nonempty(&work.file("main.o"));

    // -MD without -MF writes NAME.d beside the output, and -MP adds a phony
    // rule for each header.
    succeeded(&work.run(&["-c", "main.c", "-MD", "-MP", "-MT", "target.o"]));
    assert_eq!(
        work.read("main.d"),
        "target.o: main.c helper.h\n\nhelper.h:\n"
    );

    // -MM is the output and nothing is compiled.
    let output = work.run(&["-MM", "helper.c", "-MQ", "out dir/$x.o"]);
    succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "out\\ dir/$$x.o: helper.c helper.h\n"
    );
    assert!(!work.file("helper.o").exists());
}

#[test]
fn an_unknown_f_option_warns_once_and_the_build_continues() {
    let work = Work::new("ignored");
    let output = work.run(&[
        "-c",
        "-fno-such-option",
        "-Wall",
        "-pedantic",
        "-pipe",
        "helper.c",
    ]);
    succeeded(&output);
    nonempty(&work.file("helper.o"));
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stderr,
        "evocc: warning: option `-fno-such-option` is not supported and has no effect\n"
    );
    // -w silences the warning.
    let quiet = work.run(&["-c", "-w", "-mno-such-option", "helper.c"]);
    succeeded(&quiet);
    assert!(quiet.stderr.is_empty());
}

#[test]
fn unsupported_options_obey_warning_promotion_in_capability_probes() {
    let work = Work::new("capability-options");
    for option in [
        "-fthis-option-does-not-exist",
        "-mthis-option-does-not-exist",
        "--coverage",
    ] {
        let output = work.run(&["-Werror", option, "-fsyntax-only", "helper.c"]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "{option}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("has no effect"));
        for override_flag in ["-Wno-error", "-w", "--cap-lints=warn", "--cap-lints=allow"] {
            succeeded(&work.run(&[
                "-Werror",
                override_flag,
                option,
                "-fsyntax-only",
                "helper.c",
            ]));
        }
    }
    succeeded(&work.run(&[
        "-Werror",
        "-fno-omit-frame-pointer",
        "-fsyntax-only",
        "helper.c",
    ]));
}

#[test]
fn the_basic_stack_protector_uses_the_existing_strong_policy() {
    let work = Work::new("stack-protector");
    std::fs::write(
        work.file("guarded.c"),
        "extern void use(char *); void guarded(void) { char data[16]; use(data); }\n",
    )
    .unwrap();
    for option in ["-fstack-protector", "-fstack-protector-strong"] {
        succeeded(&work.run(&["-Werror", option, "-S", "guarded.c"]));
        let assembly = work.read("guarded.s");
        assert!(assembly.contains("__stack_chk_fail"), "{assembly}");
        succeeded(&work.run(&["-Werror", option, "-fno-stack-protector", "-S", "guarded.c"]));
        assert!(!work.read("guarded.s").contains("__stack_chk_fail"));
    }
}

#[test]
fn assembler_standard_input_works_for_kbuild_instruction_probes() {
    use std::io::Write;
    let work = Work::new("assembler-stdin");
    for language in ["assembler", "assembler-with-cpp"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_evocc"))
            .current_dir(&work.0)
            .args([
                "-Werror",
                "-Wa,--fatal-warnings",
                "-c",
                "-x",
                language,
                "-o",
                "probe.o",
                "-",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let _ = child.stdin.take().unwrap().write_all(b".text\nnop\n");
        succeeded(&child.wait_with_output().unwrap());
        nonempty(&work.file("probe.o"));
    }
}

#[test]
fn assembly_preprocessing_keeps_lines_and_assembly_token_pastes() {
    let work = Work::new("assembly-preprocessor");
    std::fs::write(work.file("layout.S"), "#ifndef __ASSEMBLER__\n#error assembly language was lost\n#endif\n#define SECTION(name) .hyp##name\n/* retained comment */\nSECTION(.text) : { *(.text) }\nnext_line\n").unwrap();
    let output = work.run(&["-E", "-P", "-C", "layout.S", "-o", "layout.lds"]);
    succeeded(&output);
    let text = work.read("layout.lds");
    assert!(text.contains(".hyp.text"), "{text}");
    assert!(text.contains("/* retained comment */"), "{text}");
    assert!(
        text.lines().any(|line| line.trim() == "next_line"),
        "{text}"
    );
    std::fs::write(work.file("second.S"), "nop\n").unwrap();
    let output = work.run(&["-E", "layout.S", "second.S", "-o", "both.s"]);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn assembly_dependency_files_cover_includes_and_keep_the_requested_target() {
    let work = Work::new("assembly-dependencies");
    std::fs::write(work.file("asm.h"), "#define INSTRUCTION nop\n").unwrap();
    std::fs::write(
        work.file("probe.S"),
        "#include \"asm.h\"\n.text\nINSTRUCTION\n",
    )
    .unwrap();
    succeeded(&work.run(&[
        "-c",
        "probe.S",
        "-Wp,-MMD,.probe.o.d",
        "-MP",
        "-o",
        "probe.o",
    ]));
    let deps = work.read(".probe.o.d");
    assert!(deps.starts_with("probe.o:"), "{deps}");
    assert!(deps.contains("asm.h"), "{deps}");
    succeeded(&work.run(&[
        "-E",
        "-P",
        "probe.S",
        "-MMD",
        "-MF",
        "layout.d",
        "-MQ",
        "out dir/$x.lds",
        "-o",
        "layout.lds",
    ]));
    let deps = work.read("layout.d");
    assert!(deps.starts_with("out\\ dir/$$x.lds:"), "{deps}");
    assert!(deps.contains("asm.h"), "{deps}");
    assert!(work.read("layout.lds").contains("nop"));
}

#[test]
#[cfg(target_os = "macos")]
fn sealed_preprocessing_mounts_absolute_outputs_and_preserves_standard_input() {
    use std::os::unix::fs::PermissionsExt;
    let work = Work::new("sealed-preprocess");
    let outputs = Work::new("sealed-preprocess-outputs");
    let dependencies = Work::new("sealed-preprocess-dependencies");
    std::fs::create_dir_all(work.file("sysroot/usr/include")).unwrap();
    std::fs::write(work.file("sysroot/usr/include/stdio.h"), "").unwrap();
    std::fs::create_dir(work.file("bin")).unwrap();
    // This checks command transport only. Real assembly and preprocessing are
    // tested above with the host tools and in the retained Linux probe.
    let tool = work.file("bin/container");
    std::fs::write(
        &tool,
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$EVOCC_TOOL_ARGS\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evocc"))
        .current_dir(&work.0)
        .env("PATH", work.file("bin"))
        .env_remove("EVOCC_AS")
        .env("EVOCC_TOOL_ARGS", work.file("arguments"))
        .args([
            "--target=aarch64-unknown-linux-gnu",
            "--sysroot=sysroot",
            "-E",
            "-P",
            "-MMD",
            "-x",
            "assembler-with-cpp",
            "-",
            "-o",
        ])
        .arg(outputs.file("probe.s"))
        .arg("-MF")
        .arg(dependencies.file("probe.d"))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    succeeded(&output);
    let args = work.read("arguments");
    assert!(args.lines().any(|arg| arg == "--interactive"), "{args}");
    for directory in [&outputs.0, &dependencies.0] {
        let host = directory.canonicalize().unwrap();
        let guest = std::path::absolute(directory).unwrap();
        assert!(
            args.contains(&format!("-v\n{}:{}\n", host.display(), guest.display())),
            "{args}"
        );
    }
}

#[test]
fn compiler_resource_queries_do_not_require_an_input_file() {
    let work = Work::new("resource-query");
    let plugin = work.run(&["-print-file-name=plugin"]);
    succeeded(&plugin);
    assert_eq!(plugin.stdout, b"plugin\n");
    assert!(plugin.stderr.is_empty());
    let output = work.run(&["-print-file-name=include"]);
    succeeded(&output);
    let include = String::from_utf8(output.stdout).unwrap();
    assert!(
        Path::new(include.trim()).join("stddef.h").is_file(),
        "{include}"
    );
    let output = work.run(&["-print-libgcc-file-name"]);
    succeeded(&output);
    assert!(!output.stdout.is_empty());
}

#[test]
fn coverage_options_warn_once_without_instrumentation() {
    let work = Work::new("coverage-options");
    for option in [
        "-coverage",
        "--coverage",
        "-fprofile-arcs",
        "-ftest-coverage",
    ] {
        let output = work.run(&["-c", option, "helper.c"]);
        succeeded(&output);
        nonempty(&work.file("helper.o"));
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            format!("evocc: warning: option `{option}` is not supported and has no effect\n")
        );
        let quiet = work.run(&["-c", "-w", option, "helper.c"]);
        succeeded(&quiet);
        assert!(quiet.stderr.is_empty());
    }
    for entry in std::fs::read_dir(&work.0).unwrap() {
        let path = entry.unwrap().path();
        assert!(!matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("gcno" | "gcda")
        ));
    }
}

#[test]
fn the_version_line_names_the_release_the_commit_and_the_target() {
    let work = Work::new("version");
    let output = work.run(&["--version"]);
    succeeded(&output);
    let text = String::from_utf8(output.stdout).unwrap();
    let mut lines = text.lines();
    let first = lines.next().unwrap();
    let inside = first
        .strip_prefix("evocc 0.1.0 (")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("unexpected version line `{first}`"));
    let fields: Vec<&str> = inside.split(' ').collect();
    assert_eq!(fields.len(), 2, "{first}");
    assert!(!fields[0].is_empty() && !fields[1].is_empty(), "{first}");
    let target = lines.next().unwrap();
    assert!(
        target == "Target: aarch64-apple-darwin" || target == "Target: aarch64-unknown-linux-gnu",
        "{target}"
    );
    let linux = work.run(&["--version", "--target=aarch64-unknown-linux-gnu"]);
    assert!(String::from_utf8(linux.stdout)
        .unwrap()
        .ends_with("Target: aarch64-unknown-linux-gnu\n"));

    let verbose = work.run(&["-v"]);
    succeeded(&verbose);
    assert!(String::from_utf8(verbose.stderr)
        .unwrap()
        .starts_with(first));
    let machine = work.run(&["-dumpmachine", "--target=aarch64-apple-darwin"]);
    assert_eq!(machine.stdout, b"aarch64-apple-darwin\n");
    let version = work.run(&["-dumpversion"]);
    assert_eq!(version.stdout, b"0.1.0\n");
}

#[test]
fn assembly_producer_version_matches_the_driver_release() {
    let work = Work::new("producer-version");
    let version = work.run(&["--version"]);
    succeeded(&version);
    let version = String::from_utf8(version.stdout).unwrap();
    let release = version.split_whitespace().nth(1).unwrap();
    assert_eq!(release, env!("CARGO_PKG_VERSION"));

    succeeded(&work.run(&[
        "-S",
        "--target=aarch64-unknown-linux-gnu",
        "--ub-checks=checked",
        "-nostdinc",
        "helper.c",
        "-o",
        "linux.s",
    ]));
    let expected = format!(".ident \"EvoCC {release} aarch64-unknown-linux-gnu\"");
    let assembly = work.read("linux.s");
    let producers: Vec<_> = assembly
        .lines()
        .filter(|line| line.starts_with(".ident"))
        .collect();
    assert_eq!(producers, [expected.as_str()]);

    succeeded(&work.run(&[
        "-S",
        "--target=aarch64-apple-darwin",
        "helper.c",
        "-o",
        "darwin.s",
    ]));
    assert!(!work.read("darwin.s").contains(".ident"));
}

#[test]
fn checked_assembly_ast_producer_matches_the_driver_release() {
    let module = evocc_evoir::parse_section9_module(include_bytes!(
        "../../../tests/fixtures/verification/machine-private.evo"
    ))
    .unwrap();
    let (ast, assembly) = evocc_mir::asm_ast::export_core_asm(&module).unwrap();
    let producer = format!(
        "EvoCC {} aarch64-unknown-linux-gnu",
        env!("CARGO_PKG_VERSION")
    );
    assert!(
        ast.starts_with(&format!(
            "[\"core-asm-v1\",\"aarch64-unknown-linux-gnu\",\"checked\",\"{producer}\","
        )),
        "{ast}"
    );
    assert!(assembly
        .lines()
        .any(|line| line == format!(".ident \"{producer}\"")));
}

#[test]
fn unknown_inputs_and_unnamed_standard_input_are_errors() {
    let work = Work::new("inputs");
    std::fs::write(work.file("notes.txt"), "int x;\n").unwrap();
    let output = work.run(&["-c", "notes.txt"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "evocc: unknown file type of input `notes.txt`; name a .c, .s, .S, .o or .a file, or select the language with -x\n"
    );

    let output = work.run(&["-c", "-"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("-E or -x is required when the input is standard input"));

    let output = work.run(&["-c", "-x", "c++", "main.c"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported language `c++`"));

    // With -x c the same file is C.
    succeeded(&work.run(&["-S", "-x", "c", "notes.txt", "-o", "notes.s"]));
    nonempty(&work.file("notes.s"));

    let output = work.run(&[]);
    assert_eq!(
        output.status.code(),
        Some(69),
        "the bare command keeps its usage text"
    );
    let output = work.run(&["-c"]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stderr, b"evocc: no input files\n");
}

#[test]
fn a_compile_error_is_exit_status_one_with_the_diagnostic() {
    let work = Work::new("error");
    std::fs::write(work.file("broken.c"), "int main(void) { return 0 }\n").unwrap();
    let output = work.run(&["-c", "broken.c"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("broken.c:1:"));
    assert!(!work.file("broken.o").exists());
}

#[test]
fn the_development_actions_keep_the_linux_default_and_the_cc_actions_follow_the_target() {
    let work = Work::new("targets");
    let development = work.run(&["--emit=asm", "main.c"]);
    succeeded(&development);
    let development = String::from_utf8(development.stdout).unwrap();
    assert!(development.contains("bl helper_twice"), "{development}");

    succeeded(&work.run(&[
        "-S",
        "--target=aarch64-unknown-linux-gnu",
        // This target-format control has no system headers. Header discovery
        // has separate tests and must not depend on an installed Linux SDK.
        "-nostdinc",
        "main.c",
        "-o",
        "linux.s",
    ]));
    assert_eq!(work.read("linux.s"), development);

    succeeded(&work.run(&[
        "-S",
        "--target=aarch64-apple-darwin",
        "main.c",
        "-o",
        "darwin.s",
    ]));
    assert!(work.read("darwin.s").contains("bl _helper_twice"));

    let output = work.run(&["-S", "--target=arm64-apple-darwin", "main.c"]);
    assert_eq!(output.status.code(), Some(1));
}

#[cfg(target_os = "macos")]
#[test]
fn a_two_file_darwin_program_links_and_runs() {
    let work = Work::new("link");
    succeeded(&work.run(&["main.c", "helper.c", "-o", "program"]));
    let status = Command::new(work.file("program")).status().unwrap();
    assert_eq!(status.code(), Some(41));

    // Objects and libraries go to the linker in command-line order.
    succeeded(&work.run(&["-c", "helper.c"]));
    succeeded(&work.run(&["main.c", "helper.o", "-lm", "-o", "mixed"]));
    let status = Command::new(work.file("mixed")).status().unwrap();
    assert_eq!(status.code(), Some(41));

    // The linker's own message and exit status 1 report a missing symbol.
    let output = work.run(&["main.c", "-o", "unresolved"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("helper_twice"));
}

#[cfg(target_os = "macos")]
#[test]
fn darwin_variadic_calls_agree_with_the_c_library() {
    let work = Work::new("variadic");
    succeeded(&work.run(&["variadic.c", "-o", "variadic"]));
    let output = Command::new(work.file("variadic")).output().unwrap();
    succeeded(&output);
    assert_eq!(output.stdout, b"7-seven-0.25 45 1234567890123\n");
}
