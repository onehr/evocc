//! EvoMIR Section 8: emission owns instruction selection, and a branch it
//! selects has to reach.
//!
//! Section 8 makes emission responsible for "the details of instruction
//! selection". `tbz`/`tbnz` encode a fourteen-bit word displacement, which is
//! thirty-two kibibytes either way, and `b.cond` a nineteen-bit one. A large
//! function reaches past both, and the assembler answers `Error: conditional
//! branch out of range`. The c-testsuite's twenty-four-thousand-line function
//! is the measurement that opened this.
//!
//! Emission therefore emits the short form only where it provably reaches: a
//! function whose body is at or above the relaxation threshold spells every
//! conditional terminator as the inverted short branch over an unconditional
//! `b`, which reaches one hundred twenty-eight mebibytes.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// The runtime and image the build-environment contract seals.
const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-nexlab:sealed";

/// Filler instructions between the branch and its target.
///
/// `tbz` reaches 8,192 instructions either way; twelve thousand is past it
/// with room the threshold's own margin does not have to explain.
const FILLER: usize = 12_000;

/// A function whose `cond_br` on a `bits<1>` parameter lowers to `tbnz` and
/// whose taken block sits `FILLER` instructions further on.
///
/// The condition is a parameter rather than an `icmp`, because Section 5 fuses
/// a comparison consumed only by the branch into `b.cond`; a bare `bits<1>` is
/// the `tbnz` row of the table.
fn far_branch(filler: usize) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "func @far(%arg0: bits<1>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default\n\
         \x20   effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}\n\
         {{\n\
         ^entry(%v0: bits<1>, %v1: bits<32>, %v2: mem<all>, %v3: world):\n\
         \x20   cond_br %v0, ^b2(%v1, %v2, %v3), ^b1(%v1, %v2, %v3)\n"
    );
    let _ = writeln!(text, "^b1(%v4: bits<32>, %v5: mem<all>, %v6: world):");
    let mut last = 4_u32;
    for next in (100_u32..).take(filler) {
        let _ = writeln!(text, "    %v{next} = add.bits<32> %v{last}, %v{last}");
        last = next;
    }
    let _ = writeln!(text, "    br ^b2(%v{last}, %v5, %v6)\n");
    let _ = writeln!(
        text,
        "^b2(%v7: bits<32>, %v8: mem<all>, %v9: world):\n    ret %v7, %v8, %v9\n}}"
    );
    text
}

fn emitted(source: &str, target: Target) -> String {
    let module = parse_section9_module(source.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

/// Assemble one emitted text for the host and report the assembler's answer.
fn assemble_on_darwin(name: &str, text: &str) -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!("evocc-relax-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{name}.s"));
    let object = directory.join(format!("{name}.o"));
    std::fs::write(&source, text).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .arg("-c")
        .arg("-o")
        .arg(&object)
        .arg(&source)
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&object);
    if built.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&built.stderr).trim().to_owned())
    }
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories below the workspace root")
        .to_path_buf()
}

/// Whether the sealed image is present and its runtime answers.
fn container() -> Result<(), String> {
    if !Path::new(CONTAINER_BIN).exists() {
        return Err(format!("{CONTAINER_BIN} is absent"));
    }
    let listed = Command::new(CONTAINER_BIN)
        .args(["image", "list"])
        .output()
        .map_err(|error| format!("{CONTAINER_BIN}: {error}"))?;
    if !listed.status.success() {
        return Err("the container runtime is not started".to_owned());
    }
    if !String::from_utf8_lossy(&listed.stdout).contains("evo-nexlab") {
        return Err(format!("{IMAGE} is not present"));
    }
    Ok(())
}

/// Assemble one emitted text with the sealed container's GCC-14.
fn assemble_on_linux(name: &str, text: &str) -> Result<(), String> {
    let directory = workspace()
        .join("target")
        .join(format!("evocc-relax-linux-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    std::fs::write(directory.join(format!("{name}.s")), text).map_err(|error| error.to_string())?;
    let run = Command::new(CONTAINER_BIN)
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/arm64",
            "--network",
            "none",
            "-v",
        ])
        .arg(format!("{}:/work/probes", directory.display()))
        .args([
            IMAGE,
            "bash",
            "-c",
            &format!("cd /work/probes && gcc -c -o {name}.o {name}.s"),
        ])
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(&directory);
    if run.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&run.stderr).trim().to_owned())
    }
}

#[test]
fn a_branch_past_the_short_range_still_assembles_on_darwin() {
    let text = emitted(&far_branch(FILLER), Target::Darwin);
    // A clang on another host cannot assemble Darwin text.
    if !cfg!(target_os = "macos") {
        eprintln!("the Darwin lane is skipped: the host is not macOS");
        return;
    }
    if let Err(message) = assemble_on_darwin("far_darwin", &text) {
        panic!("the host assembler refused the emitted text:\n{message}");
    }
}

#[test]
fn a_branch_past_the_short_range_still_assembles_on_linux() {
    if let Err(reason) = container() {
        eprintln!("the Linux lane is skipped: {reason}");
        return;
    }
    let text = emitted(&far_branch(FILLER), Target::Linux);
    if let Err(message) = assemble_on_linux("far_linux", &text) {
        panic!("the container's GCC-14 refused the emitted text:\n{message}");
    }
}

/// The three lines the entry block's terminator emits, whatever precedes it.
fn entry_terminator(text: &str, take: usize) -> Vec<String> {
    text.lines()
        .skip_while(|line| line.trim() != "Lfar_block0:")
        .skip(1)
        .take(take)
        .map(|line| line.trim().to_owned())
        .collect()
}

/// The block the out-of-SSA form gives the taken edge, read from the small
/// function's own short branch, so the test does not fix a block numbering
/// that belongs to the allocator.
fn taken_label(text: &str) -> String {
    entry_terminator(text, 1)[0]
        .rsplit(' ')
        .next()
        .expect("the branch names its target")
        .to_owned()
}

#[test]
fn the_relaxed_form_is_the_inverted_branch_over_an_unconditional_one() {
    let short = emitted(&far_branch(4), Target::Darwin);
    let taken = taken_label(&short);
    let text = emitted(&far_branch(FILLER), Target::Darwin);
    let entry = entry_terminator(&text, 3);
    assert_eq!(
        entry[0], "tbz w0, #0, Lfar_relax0",
        "the short branch is inverted and jumps over the long one: {entry:?}"
    );
    assert_eq!(
        entry[1],
        format!("b {taken}"),
        "the long form reaches the target: {entry:?}"
    );
    assert_eq!(
        entry[2], "Lfar_relax0:",
        "the fresh label follows it: {entry:?}"
    );
}

#[test]
fn a_function_below_the_threshold_keeps_the_short_form() {
    let text = emitted(&far_branch(4), Target::Darwin);
    let entry = entry_terminator(&text, 1);
    assert!(
        entry[0].starts_with("tbnz w0, #0, Lfar_block"),
        "a small function is untouched: {entry:?}"
    );
    assert!(
        !text.contains("Lfar_relax"),
        "and no relaxation label is emitted:\n{text}"
    );
}
