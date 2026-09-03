//! Section 8: emission for Darwin arm64.
//!
//! "The assembly text is the platform assembler's syntax. Functions are
//! emitted in module order, each as `.globl`/`.p2align 2`/label/body; globals
//! as data ... with their EvoIR initial bytes; symbols get the platform prefix
//! (`_` on Darwin)."
//!
//! The instrument is the host assembler: a text it rejects is not the
//! platform's syntax, whatever it looks like. The tests that need it are
//! skipped, with the reason printed, where it is absent.

use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{
    allocate, emit_module, frame_of, lower_module, out_of_ssa, verify_allocation, Allocated,
};

/// The host assembler, and its version, or the reason there is none.
fn assembler() -> Result<String, String> {
    let output = Command::new("clang")
        .arg("--version")
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !output.status.success() {
        return Err("clang did not report a version".to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if !text.contains("arm64") && !text.contains("aarch64") {
        return Err(format!(
            "the host toolchain does not target arm64: {}",
            text.lines().nth(1).unwrap_or_default()
        ));
    }
    Ok(text.lines().next().unwrap_or_default().to_owned())
}

fn build(source: &str) -> Vec<Allocated> {
    let module = parse_section9_module(source.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    lowered
        .functions
        .iter()
        .map(|function| {
            let allocated = allocate(&out_of_ssa(function)).expect("allocation");
            verify_allocation(&allocated).expect("the allocation verifies");
            allocated
        })
        .collect()
}

/// Assemble a text, returning the assembler's complaint when it has one.
fn assemble(text: &str) -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!("evocc-emit-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join("unit.s");
    let object = directory.join("unit.o");
    std::fs::write(&source, text).map_err(|error| error.to_string())?;
    let output = Command::new("clang")
        .args(["-c", "-o"])
        .arg(&object)
        .arg(&source)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&object);
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

const ARITHMETIC: &str = "\
func @main(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = add.bits<32> %v0, %v1
    %v5 = mul.bits<32> %v4, %v0
    %v6 = const.bits<32> 305419896
    %v7 = xor.bits<32> %v5, %v6
    ret %v7, %v2, %v3
}
";

const EVERY_SHAPE: &str = "\
global @counter origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111

func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<32> %v0, %v0
    ret %v3, %v1, %v2
}

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=1, size=16, align=8]
    (%v6, %v7, %v8) = alloca %v4, %v5 [origin=2, size=16, align=8]
    %v9 = access_in_bounds %v7, %v3 [size=4, align=4]
    (%v10, %v11) = store %v9, %v8, %v3, %v0
    (%v12, %v13) = copy %v10, %v11, %v6, %v3 [size=16, align=8]
    %v14 = global_addr @counter : ptr<generic>
    %v15 = access_in_bounds %v12, %v14 [size=4, align=4]
    (%v16, %v17, %v18) = load %v15, %v13, %v14 : bits<32>
    (%v19, %v20, %v21) = call @helper(%v16, %v17, %v18)
    %v22 = divisor_nonzero %v19
    ub.assume.division_by_zero %v22
    %v23 = udiv.bits<32> %v0, %v19
    %v24 = icmp.ult %v23, %v19
    cond_br %v24, ^b1(%v23, %v20, %v21), ^b2(%v19, %v20, %v21)

^b1(%v25: bits<32>, %v26: mem<all>, %v27: world):
    ret %v25, %v26, %v27

^b2(%v28: bits<32>, %v29: mem<all>, %v30: world):
    %v31 = sub.bits<32> %v28, %v28
    ret %v31, %v29, %v30
}
";

#[test]
fn a_function_is_emitted_with_the_platform_prefix_and_alignment() {
    let text = emit_module(&build(ARITHMETIC), &[]);
    assert!(text.contains("\n.globl _main\n"), "{text}");
    assert!(text.contains(".p2align 2\n_main:\n"), "{text}");
    assert!(
        text.contains("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n"),
        "the Section 6 prologue: {text}"
    );
    assert!(
        text.contains("    ldp x29, x30, [sp], #16\n    ret\n"),
        "the epilogue reverses it: {text}"
    );
}

#[test]
fn a_global_is_emitted_as_data_with_its_initial_bytes() {
    let module = parse_section9_module(EVERY_SHAPE.as_bytes()).expect("canonical EvoIR");
    let text = emit_module(&build(EVERY_SHAPE), &module.globals);
    assert!(text.contains(".section __DATA,__data\n"), "{text}");
    assert!(text.contains(".globl _counter\n_counter:\n"), "{text}");
    assert!(text.contains("    .byte 7\n"), "{text}");
}

#[test]
fn an_address_uses_the_page_relative_pair() {
    let text = emit_module(&build(EVERY_SHAPE), &[]);
    assert!(text.contains("adrp x"), "{text}");
    assert!(text.contains("_counter@PAGE"), "{text}");
    assert!(text.contains("_counter@PAGEOFF"), "{text}");
}

#[test]
fn the_frame_is_a_multiple_of_sixteen_and_orders_its_areas() {
    for allocated in build(EVERY_SHAPE) {
        let frame = frame_of(&allocated);
        assert_eq!(frame.size % 16, 0, "@{}", allocated.name);
        // Section 6: the callee-saved area is highest, the spill area lowest.
        let lowest_saved = frame.saved.values().min().copied();
        let highest_spill = frame.spills.values().max().copied();
        if let (Some(saved), Some(spill)) = (lowest_saved, highest_spill) {
            assert!(saved > spill, "@{}", allocated.name);
        }
        for slot in frame.slots.values() {
            assert!(*slot < frame.size, "@{}", allocated.name);
        }
    }
}

#[test]
fn the_emitted_text_is_deterministic() {
    let one = emit_module(&build(EVERY_SHAPE), &[]);
    let two = emit_module(&build(EVERY_SHAPE), &[]);
    assert_eq!(one, two);
}

#[test]
fn the_host_assembler_accepts_the_emitted_text() {
    let version = match assembler() {
        Ok(version) => version,
        Err(reason) => {
            println!("skipped: {reason}");
            return;
        }
    };
    println!("assembler={version}");
    for (name, source) in [("arithmetic", ARITHMETIC), ("everyShape", EVERY_SHAPE)] {
        let module = parse_section9_module(source.as_bytes()).expect("canonical EvoIR");
        let text = emit_module(&build(source), &module.globals);
        if let Err(complaint) = assemble(&text) {
            panic!("the assembler refused {name}:\n{complaint}\n{text}");
        }
    }
}
