//! Section 9, instrument three: the first native differential.
//!
//! "The assembled program is run on the host and its output compared with the
//! reference compiler's build of the same C source." The corpus of the
//! roadmap's B4 is not here yet; what is here is the first half of the
//! instrument, which compares a native run with the EvoIR interpreter that
//! already agrees with the machine interpreter.
//!
//! **The exit-status rule.** A process exit status carries eight bits, so a
//! `bits<32>` result is compared modulo 256: the program returns its result in
//! `x0`, the C runtime passes `x0` to `exit`, and the host reports its low
//! eight bits. Both sides are reduced the same way before they are compared.

use std::process::Command;

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{allocate, emit_module, lower_module, out_of_ssa, verify_allocation, Allocated};
use evocc_types::EvoIRType;

fn toolchain() -> Result<String, String> {
    let output = Command::new("clang")
        .arg("--version")
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !output.status.success() {
        return Err("clang did not report a version".to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    if !text.contains("arm64") && !text.contains("aarch64") {
        return Err("the host toolchain does not target arm64".to_owned());
    }
    Ok(text.lines().next().unwrap_or_default().to_owned())
}

fn build(source: &str) -> (Vec<Allocated>, evocc_evoir::Module) {
    let module = parse_section9_module(source.as_bytes()).expect("canonical EvoIR");
    let lowered = lower_module(&module).expect("every form is in the Section 5 table");
    let functions = lowered
        .functions
        .iter()
        .map(|function| {
            let allocated = allocate(&out_of_ssa(function)).expect("allocation");
            verify_allocation(&allocated).expect("the allocation verifies");
            allocated
        })
        .collect();
    (functions, module)
}

/// Assemble, link and run one emitted text, returning the process exit status.
fn run_native(name: &str, text: &str) -> Result<i32, String> {
    let directory = std::env::temp_dir().join(format!("evocc-native-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{name}.s"));
    let program = directory.join(name);
    std::fs::write(&source, text).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .output()
        .map_err(|error| error.to_string())?;
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

/// The interpreter's answer for a `main` that takes nothing and returns one
/// `bits<32>`, reduced to the eight bits an exit status carries.
fn interpreted(module: &evocc_evoir::Module) -> u8 {
    let arguments = vec![
        RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let outcome = interpret_module(
        module,
        "main",
        &arguments,
        InterpreterLimits::new(1_000_000),
    )
    .expect("the execution is defined");
    let value = outcome
        .return_values
        .iter()
        .find(|value| matches!(value.ty, EvoIRType::Bits(_)))
        .expect("one ordinary result");
    u8::try_from(value.raw_bits & 0xff).expect("eight bits")
}

const CONSTANT: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 42
    ret %v2, %v0, %v1
}
";

const ARITHMETIC: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 1000003
    %v3 = const.bits<32> 7
    %v4 = mul.bits<32> %v2, %v3
    %v5 = const.bits<32> 11
    %v6 = udiv.bits<32> %v4, %v5
    %v7 = urem.bits<32> %v4, %v5
    %v8 = add.bits<32> %v6, %v7
    %v9 = xor.bits<32> %v8, %v3
    ret %v9, %v0, %v1
}
";

const NARROW: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<13> 8191
    %v3 = const.bits<13> 3
    %v4 = ashr.bits<13> %v2, %v3
    %v5 = lshr.bits<13> %v2, %v3
    %v6 = add.bits<13> %v4, %v5
    %v7 = zext.bits<32> %v6
    %v8 = signed_add_defined %v4, %v5
    %v9 = zext.bits<32> %v8
    %v10 = add.bits<32> %v7, %v9
    ret %v10, %v0, %v1
}
";

const BRANCHING: &str = "\
func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<64> 10
    %v3 = const.bits<64> 0
    br ^b1(%v2, %v3, %v0, %v1)

^b1(%v4: bits<64>, %v5: bits<64>, %v6: mem<all>, %v7: world):
    %v8 = const.bits<64> 0
    %v9 = icmp.ult %v8, %v4
    cond_br %v9, ^b2(%v4, %v5, %v6, %v7), ^b3(%v5, %v6, %v7)

^b2(%v10: bits<64>, %v11: bits<64>, %v12: mem<all>, %v13: world):
    %v14 = const.bits<64> 1
    %v15 = sub.bits<64> %v10, %v14
    %v16 = add.bits<64> %v11, %v10
    br ^b1(%v15, %v16, %v12, %v13)

^b3(%v17: bits<64>, %v18: mem<all>, %v19: world):
    %v20 = trunc.bits<32> %v17
    ret %v20, %v18, %v19
}
";

const MEMORY: &str = "\
global @counter origin=0 kind=file visibility=external size=4 align=4 bytes=11000000 initialized=1111

func @helper(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<32> %v0, %v0
    ret %v3, %v1, %v2
}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size=8, align=4]
    %v5 = const.bits<32> 5
    %v6 = access_in_bounds %v3, %v2 [size=4, align=4]
    (%v7, %v8) = store %v6, %v4, %v2, %v5
    %v9 = access_in_bounds %v7, %v2 [size=4, align=4]
    (%v10, %v11, %v12) = load %v9, %v8, %v2 : bits<32>
    %v13 = global_addr @counter : ptr<generic>
    %v14 = access_in_bounds %v11, %v13 [size=4, align=4]
    (%v15, %v16, %v17) = load %v14, %v12, %v13 : bits<32>
    %v18 = add.bits<32> %v10, %v15
    (%v19, %v20, %v21) = call @helper(%v18, %v16, %v17)
    ret %v19, %v20, %v21
}
";

/// The whole corpus, each of which takes no arguments and returns a `bits<32>`.
const PROGRAMS: [(&str, &str); 5] = [
    ("constant", CONSTANT),
    ("arithmetic", ARITHMETIC),
    ("narrow", NARROW),
    ("branching", BRANCHING),
    ("memory", MEMORY),
];

/// A program whose only interesting instruction is a `copy` of `size` bytes
/// from a static object into an automatic one, folding the first, middle and
/// last word of the destination into the exit status so that a copy that moved
/// part of the range answers differently from one that moved all of it.
fn copy_program(size: usize) -> String {
    assert!(size.is_multiple_of(8) && size >= 32);
    let bytes: String = (0..size)
        .map(|index| format!("{:02x}", source_byte(index)))
        .collect();
    let initialized = "1".repeat(size);
    let middle = size / 16;
    let tail = size / 8 - 1;
    format!(
        "\
global @source origin=0 kind=file visibility=external size={size} align=8 bytes={bytes} initialized={initialized}

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects {{ reads={{all}}, writes={{all}}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=1, size={size}, align=8]
    %v5 = global_addr @source : ptr<generic>
    (%v6, %v7) = copy %v3, %v4, %v2, %v5 [size={size}, align=8]
    %v8 = access_in_bounds %v6, %v2 [size=8, align=8]
    (%v9, %v10, %v11) = load %v8, %v7, %v2 : bits<64>
    %v12 = const.bits<64> {middle}
    %v13 = ptr.offset.add.unsigned %v2, %v12 [scale=8]
    %v14 = pointer_valid %v10, %v13
    %v15 = access_in_bounds %v14, %v13 [size=8, align=8]
    (%v16, %v17, %v18) = load %v15, %v11, %v13 : bits<64>
    %v19 = const.bits<64> {tail}
    %v20 = ptr.offset.add.unsigned %v2, %v19 [scale=8]
    %v21 = pointer_valid %v17, %v20
    %v22 = access_in_bounds %v21, %v20 [size=8, align=8]
    (%v23, %v24, %v25) = load %v22, %v18, %v20 : bits<64>
    %v26 = const.bits<64> 31
    %v27 = mul.bits<64> %v9, %v26
    %v28 = add.bits<64> %v27, %v16
    %v29 = mul.bits<64> %v28, %v26
    %v30 = add.bits<64> %v29, %v23
    %v31 = const.bits<64> 32
    %v32 = shift_count_defined.unsigned %v30, %v31
    ub.assume.invalid_shift_count %v32
    %v33 = lshr.bits<64> %v30, %v31
    %v34 = xor.bits<64> %v30, %v33
    %v35 = trunc.bits<32> %v34
    ret %v35, %v24, %v25
}}
"
    )
}

/// The static object's bytes: a mixed sequence rather than an arithmetic one,
/// so that the fold of three words does not cancel by construction.
fn source_byte(index: usize) -> u8 {
    ((index as u64)
        .wrapping_add(1)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        >> 24) as u8
}

/// Section 4: a `mem.copy` above sixty-four bytes is a counted loop, and the
/// loop moves the same bytes the interpreter moved.
#[test]
fn a_two_hundred_byte_copy_agrees_with_the_interpreter() {
    let version = match toolchain() {
        Ok(version) => version,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={version}");

    for size in [64_usize, 200] {
        let source = copy_program(size);
        let (functions, module) = build(&source);
        let text = emit_module(&functions, &module.globals);
        let status = match run_native(&format!("copy{size}"), &text) {
            Ok(status) => status,
            Err(reason) => panic!("{size}: {reason}\n{text}"),
        };
        let expected = interpreted(&module);
        let observed = u8::try_from(status & 0xff).expect("eight bits");
        assert_eq!(observed, expected, "a {size}-byte copy disagreed\n{text}");
        // A program that answered zero would agree with one that copied
        // nothing, so the fold is checked to be carrying data at all.
        assert_ne!(observed, 0, "the {size}-byte fold is not observable");
        println!("copyBytes={size} exitStatus={observed}");
    }
}

#[test]
fn the_native_run_agrees_with_the_interpreter() {
    let version = match toolchain() {
        Ok(version) => version,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={version}");

    let mut assembled = 0;
    let mut ran = 0;
    let mut agreed = 0;
    for (name, source) in PROGRAMS {
        let (functions, module) = build(source);
        let text = emit_module(&functions, &module.globals);
        assembled += 1;
        let status = match run_native(name, &text) {
            Ok(status) => status,
            Err(reason) => panic!("{name}: {reason}\n{text}"),
        };
        ran += 1;
        let expected = interpreted(&module);
        let observed = u8::try_from(status & 0xff).expect("eight bits");
        assert_eq!(observed, expected, "{name} disagreed\n{text}");
        agreed += 1;
    }
    println!("programs={}", PROGRAMS.len());
    println!("assembled={assembled}");
    println!("ran={ran}");
    println!("exitStatusesAgreeing={agreed}");
}
