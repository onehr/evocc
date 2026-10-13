//! EvoMIR Section 5, revision 0.15: the label address row.
//!
//! "`block_addr ^b` | `adr rd, <label of block b>` (revision 0.15); a static
//! relocation against a block uses a local symbol `.L<function>.<block>` the
//! emitter places at the block's first instruction."
//!
//! EvoIR 1.9.68 gives the form its meaning: "`block_addr` yields the address of
//! the first instruction of block `^label` of the current function as a
//! `ptr<generic>` with *code* provenance: two `block_addr` of the same block
//! are equal, of different blocks unequal". It exists because the kernel writes
//! `({ __label__ __here; __here: (unsigned long)&&__here; })` to record an
//! instruction address; the value is only ever compared, stored and printed.
//!
//! The two facts the program below establishes are exactly those two: the
//! address is not null, and two readings of the same label agree. Neither the
//! test nor the program it compares against ever reports the address itself,
//! which is a link-time and load-time quantity no two builds have to share.

use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{
    compile_module_for, interpret_module as interpret_machine_module, lower_module_for, MirLimits,
    MirValue, Target,
};
use evocc_types::{EvoIRType, Region};

/// The runtime and image the build-environment contract seals.
const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-nexlab:sealed";

/// The kernel's idiom, written in C for the reference compilers.
///
/// `a != 0` is the low bit of the answer doubled, `a != b` the low bit: the
/// expected status is 2, which says the address is not null and the two
/// readings agree. The address itself is never printed or returned.
const PROGRAM: &str = r#"int main(void)
{
    __label__ here;
    unsigned long a;
    unsigned long b;
here:
    a = (unsigned long)&&here;
    b = (unsigned long)&&here;
    return (a != 0) * 2 + (a != b);
}
"#;

/// The same program in EvoIR, written by hand because the front end has not
/// yet given `&&label` its `block_addr` lowering.
const UNIT: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    br ^b1(%v0, %v1)

^b1(%v2: mem<all>, %v3: world):
    %v4 = block_addr ^b1 : ptr<generic>
    %v5 = block_addr ^b1 : ptr<generic>
    %v6 = ptr.to_int %v4
    %v7 = ptr.to_int %v5
    %v8 = const.bits<64> 0
    %v9 = icmp.ne %v6, %v8
    %v10 = icmp.ne %v6, %v7
    %v11 = zext.bits<32> %v9
    %v12 = zext.bits<32> %v10
    %v13 = add.bits<32> %v11, %v11
    %v14 = add.bits<32> %v13, %v12
    ret %v14, %v2, %v3
}
";

fn emitted(target: Target) -> String {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

/// The lines of one emitted block, up to the next label.
fn block_body(text: &str, label: &str) -> Vec<String> {
    text.lines()
        .skip_while(|line| line.trim() != label)
        .skip(1)
        .take_while(|line| line.starts_with(' '))
        .map(|line| line.trim().to_owned())
        .collect()
}

#[test]
fn a_block_address_lowers_to_one_adr() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let lowered = lower_module_for(Target::Darwin, &module, &|_| false)
        .expect("every form is in the Section 5 table");
    let function = &lowered.functions[0];
    let block = function
        .blocks
        .iter()
        .find(|block| block.id == 1)
        .expect("the addressed block");
    let adrs = block
        .insts
        .iter()
        .filter(|inst| matches!(inst, evocc_mir::MInst::Adr { block: 1, .. }))
        .count();
    assert_eq!(
        adrs, 2,
        "each block_addr is one adr and nothing else: {:?}",
        block.insts
    );
}

#[test]
fn the_emitter_places_the_block_symbol_at_the_first_instruction_on_darwin() {
    let text = emitted(Target::Darwin);
    let mut lines = text
        .lines()
        .skip_while(|line| line.trim() != "Lmain_block1:");
    lines.next();
    assert_eq!(
        lines.next().map(str::trim),
        Some("Lmain.1:"),
        "the local symbol follows the block's own label:\n{text}"
    );
    let body = block_body(&text, "Lmain.1:");
    assert_eq!(
        body.iter()
            .filter(|line| line.starts_with("adr ") && line.ends_with(", Lmain.1"))
            .count(),
        2,
        "and both adr name it, each into its own register:\n{text}"
    );
}

#[test]
fn the_emitter_places_the_block_symbol_at_the_first_instruction_on_linux() {
    let text = emitted(Target::Linux);
    let mut lines = text
        .lines()
        .skip_while(|line| line.trim() != ".Lmain_block1:");
    lines.next();
    assert_eq!(
        lines.next().map(str::trim),
        Some(".Lmain.1:"),
        "the local symbol is the platform's own, `.L` on Linux:\n{text}"
    );
    assert!(
        text.contains(", .Lmain.1\n"),
        "and the adr names it:\n{text}"
    );
}

#[test]
fn a_block_that_is_never_addressed_keeps_only_its_branch_label() {
    let text = emitted(Target::Darwin);
    assert!(
        !text.contains("Lmain.0:"),
        "the entry block's address is never taken, so it gets no symbol:\n{text}"
    );
}

#[test]
fn both_interpreters_answer_the_two_facts() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let arguments = vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let outcome = interpret_module(&module, "main", &arguments, InterpreterLimits::new(100_000))
        .expect("the execution is defined");
    let evoir = outcome
        .return_values
        .iter()
        .find(|value| matches!(value.ty, EvoIRType::Bits(_)))
        .expect("one ordinary result")
        .raw_bits;
    assert_eq!(
        evoir, 2,
        "the address is not null and two readings of one label agree"
    );

    let lowered = lower_module_for(Target::Darwin, &module, &|_| false).expect("lowering");
    let machine = interpret_machine_module(&lowered, "main", &[], MirLimits::new(100_000))
        .expect("the machine agrees");
    assert_eq!(
        machine.results.first(),
        Some(&MirValue::Int(2)),
        "and the machine interpreter says the same"
    );
}

fn build_and_run_on_darwin(name: &str, text: &str) -> Result<i32, String> {
    let directory = std::env::temp_dir().join(format!("evocc-blockaddr-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{name}.s"));
    let program = directory.join(name);
    std::fs::write(&source, text).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "the toolchain refused {name}:\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    let run = Command::new(&program)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&program);
    run.status
        .code()
        .ok_or_else(|| format!("{name} did not exit normally"))
}

/// The host compiler's own build of the C program, for the same two facts.
fn reference_on_darwin() -> Result<i32, String> {
    // A clang on another host cannot link Darwin text.
    if !cfg!(target_os = "macos") {
        return Err("the host is not macOS".to_owned());
    }
    let directory = std::env::temp_dir().join(format!("evocc-blockaddr-c-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join("here.c");
    let program = directory.join("here");
    std::fs::write(&source, PROGRAM).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .args(["-O0", "-o"])
        .arg(&program)
        .arg(&source)
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "the reference compiler refused the program:\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    let run = Command::new(&program)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&program);
    run.status
        .code()
        .ok_or_else(|| "the reference did not exit normally".to_owned())
}

#[test]
fn the_native_run_agrees_with_the_host_compiler_on_darwin() {
    let reference = match reference_on_darwin() {
        Ok(status) => status,
        Err(reason) => {
            eprintln!("the Darwin lane is skipped: {reason}");
            return;
        }
    };
    let ours = build_and_run_on_darwin("block_addr", &emitted(Target::Darwin))
        .expect("the emitted text assembles, links and runs");
    assert_eq!(
        ours, reference,
        "the two builds report the same two facts about the label address"
    );
    assert_eq!(reference, 2, "which are: not null, and the readings agree");
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories below the workspace root")
        .to_path_buf()
}

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

#[test]
fn the_native_run_agrees_with_gcc_in_the_container_on_linux() {
    if let Err(reason) = container() {
        eprintln!("the Linux lane is skipped: {reason}");
        return;
    }
    let directory = workspace()
        .join("target")
        .join(format!("evocc-blockaddr-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    std::fs::write(directory.join("ours.s"), emitted(Target::Linux)).expect("the emitted text");
    std::fs::write(directory.join("here.c"), PROGRAM).expect("the C program");
    let script = "cd /work/probes && gcc -o ours ours.s && gcc -O0 -o reference here.c && \
                  ./ours; ours=$?; ./reference; echo \"$ours $?\"";
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
        .args([IMAGE, "bash", "-c", script])
        .output()
        .expect("the container runs");
    let _ = std::fs::remove_dir_all(&directory);
    let stdout = String::from_utf8_lossy(&run.stdout);
    let answer = stdout
        .lines()
        .last()
        .expect("the script prints the two statuses")
        .trim()
        .to_owned();
    assert_eq!(
        answer,
        "2 2",
        "the pipeline's build and GCC-14's build report the same two facts:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&run.stderr)
    );
}

#[test]
fn the_two_textual_forms_round_trip() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let rendered = evocc_evoir::render_module(&module);
    assert!(
        rendered.contains("= block_addr ^b1 : ptr<generic>"),
        "the EvoIR renderer spells the form of Section 9.3.1:\n{rendered}"
    );
    let again = parse_section9_module(rendered.as_bytes()).expect("and parses its own output");
    assert_eq!(again, module, "the EvoIR round trip is the identity");

    let lowered = lower_module_for(Target::Darwin, &module, &|_| false).expect("lowering");
    let text = evocc_mir::render_module(&lowered);
    assert!(
        text.contains(" = adr ^b1"),
        "the EvoMIR renderer spells the block, not a label:\n{text}"
    );
    let reparsed = evocc_mir::parse_module(&text).expect("and parses its own output");
    assert_eq!(reparsed, lowered, "the EvoMIR round trip is the identity");
}
