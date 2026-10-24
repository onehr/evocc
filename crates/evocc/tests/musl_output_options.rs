//! The ten output and assembler options used by the EvoCC-run musl commands.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const LINUX: &str = "aarch64-unknown-linux-gnu";
const SOURCE: &str = "extern int ext;\nint data = 7;\nint f(int x) { return x + data + ext; }\n";
static NEXT: AtomicU64 = AtomicU64::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn work(label: &str) -> PathBuf {
    let path = root().join("target/frontend/tmp").join(format!(
        "musl-output-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
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
fn cc(arguments: &[&str], source: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_evocc"));
    command.args([
        "--target=aarch64-unknown-linux-gnu",
        "--sysroot",
        root().join("sysroot").join(LINUX).to_str().unwrap(),
    ]);
    command.args(arguments).arg(source).output().unwrap()
}
fn sections(path: &Path) -> Vec<String> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"\x7fELF");
    let u16_at = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
    let u32_at = |offset: usize| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize
    };
    let u64_at = |offset: usize| {
        let mut value = [0; 8];
        value.copy_from_slice(&bytes[offset..offset + 8]);
        u64::from_le_bytes(value) as usize
    };
    let offset = u64_at(0x28);
    let size = u16_at(0x3a);
    let count = u16_at(0x3c);
    let strings = u16_at(0x3e);
    let string_offset = u64_at(offset + strings * size + 0x18);
    let mut names = Vec::new();
    for index in 0..count {
        let header = offset + index * size;
        let name = u32_at(header);
        let start = string_offset + name;
        let end = bytes[start..].iter().position(|b| *b == 0).unwrap() + start;
        names.push(String::from_utf8_lossy(&bytes[start..end]).into_owned());
    }
    names
}
fn object(options: &[&str], directory: &Path, name: &str) -> PathBuf {
    let source = directory.join("source.c");
    if !source.exists() {
        std::fs::write(&source, SOURCE).unwrap();
    }
    let output = directory.join(name);
    let result = cc(
        &[options, &["-O2", "-c", "-o", output.to_str().unwrap()]].concat(),
        &source,
    );
    assert!(result.status.success(), "{options:?}\n{}", text(&result));
    output
}
fn assembly(options: &[&str], directory: &Path, name: &str) -> String {
    let source = directory.join("source.c");
    if !source.exists() {
        std::fs::write(&source, SOURCE).unwrap();
    }
    let output = directory.join(name);
    let result = cc(
        &[options, &["-O2", "-S", "-o", output.to_str().unwrap()]].concat(),
        &source,
    );
    assert!(result.status.success(), "{options:?}\n{}", text(&result));
    std::fs::read_to_string(output).unwrap()
}

#[test]
fn output_actions_and_pipe_write_the_requested_files() {
    let directory = work("outputs");
    let source = directory.join("source.c");
    std::fs::write(&source, SOURCE).unwrap();
    let object = directory.join("probe.o");
    let assembly_path = directory.join("probe.s");
    let preprocessed = directory.join("probe.i");
    for arguments in [
        vec!["-O2", "-c", "-o", object.to_str().unwrap()],
        vec!["-O2", "-S", "-o", assembly_path.to_str().unwrap()],
        vec!["-O2", "-E", "-o", preprocessed.to_str().unwrap()],
    ] {
        let result = cc(&arguments, &source);
        assert!(result.status.success(), "{}", text(&result));
    }
    assert_eq!(&std::fs::read(&object).unwrap()[..4], b"\x7fELF");
    assert!(std::fs::read_to_string(&assembly_path)
        .unwrap()
        .contains(".text"));
    assert!(std::fs::read_to_string(&preprocessed)
        .unwrap()
        .contains("extern int ext"));
    let piped = assembly(&["-pipe"], &directory, "pipe.s");
    let plain = assembly(&[], &directory, "plain.s");
    assert_eq!(piped, plain);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn object_sections_pin_the_assembler_options() {
    let directory = work("sections");
    let base = object(&[], &directory, "base.o");
    let noexec = object(&["-Wa,--noexecstack"], &directory, "noexec.o");
    let sections_object = object(
        &["-ffunction-sections", "-fdata-sections"],
        &directory,
        "sections.o",
    );
    let unwind = object(
        &["-fno-asynchronous-unwind-tables", "-fno-unwind-tables"],
        &directory,
        "unwind.o",
    );
    let no_stack = object(&["-fno-stack-protector"], &directory, "nostack.o");
    assert!(sections(&base).iter().all(|name| name != ".note.GNU-stack"));
    assert!(sections(&noexec)
        .iter()
        .any(|name| name == ".note.GNU-stack"));
    assert!(sections(&sections_object)
        .iter()
        .any(|name| name == ".text.f"));
    assert!(sections(&sections_object)
        .iter()
        .any(|name| name == ".data.data"));
    assert!(sections(&unwind).iter().all(|name| name != ".eh_frame"));
    assert!(sections(&no_stack).iter().all(|name| name != ".eh_frame"));
    let pic = assembly(&["-fPIC"], &directory, "pic.s");
    assert!(pic.contains(":got:ext"), "{pic}");
    let no_pic = assembly(&[], &directory, "nopic.s");
    assert!(no_pic.contains(":got:ext"), "{no_pic}");
    assert!(!assembly(&["-fno-stack-protector"], &directory, "nostack.s").contains("__stack_chk"));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
#[ignore = "sealed Linux lane; needs evo-nexlab:sealed; run explicitly and serially"]
fn gcc_14_has_the_same_properties() {
    let directory = work("gcc");
    let source = directory.join("source.c");
    std::fs::write(&source, SOURCE).unwrap();
    let script = "\
set -eu; cd /work
gcc-14 -O2 -c source.c -o base.o
gcc-14 -O2 -Wa,--noexecstack -c source.c -o noexec.o
gcc-14 -O2 -ffunction-sections -fdata-sections -c source.c -o sections.o
gcc-14 -O2 -fno-asynchronous-unwind-tables -fno-unwind-tables -c source.c -o unwind.o
gcc-14 -O2 -fPIC -c source.c -o pic.o
gcc-14 -O2 -fno-stack-protector -c source.c -o nostack.o
gcc-14 -O2 -S source.c -o base.s
gcc-14 -O2 -pipe -S source.c -o pipe.s
cmp base.s pipe.s
for f in base noexec sections unwind pic nostack; do echo ===$f; readelf -SW $f.o | grep -E 'text[.]f|data[.]data|note.GNU-stack|eh_frame' || true; readelf -rW $f.o | grep GOT || true; done";
    let output = Command::new("container")
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work", directory.display()))
        .args(["evo-nexlab:sealed", "sh", "-c", script])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", text(&output));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("===base"));
    assert!(stdout.contains("text.f"));
    assert!(stdout.contains("data.data"));
    assert!(stdout.contains("note.GNU-stack"));
    assert!(stdout.contains("===unwind"));
    assert!(stdout.contains("GOT"));
    assert!(stdout.contains("===nostack"));
    assert!(!stdout.contains("__stack_chk"));
    let _ = std::fs::remove_dir_all(directory);
}
