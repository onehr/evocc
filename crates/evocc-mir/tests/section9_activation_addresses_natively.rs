//! EvoMIR Section 9, revision 0.17: the activation addresses, assembled,
//! linked against a host `main` and run on both targets.
//!
//! EvoIR 1.9.72 exists because the kernel's `_RET_IP_` is `(unsigned
//! long)__builtin_return_address(0)`, and its tracing and lock helpers store
//! that value and pass it to `printk`, never dereferencing it. The probe below
//! is that idiom: it stores the return address and the frame address as
//! integers in its own frame, reads each a second time, and reports whether the
//! stored reading agrees with the second and whether either is null. The answer
//! is a bitmap of failures, so a correct build returns 0.
//!
//! Neither the probe nor the test ever reports an address. A return address and
//! a frame address are load-time quantities that no two processes share, so
//! only the facts cross the process boundary.

use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::{interpret_module, parse_section9_module, InterpreterLimits, RuntimeValue};
use evocc_mir::{
    interpret_module as interpret_machine_module, lower_module_for, MirLimits, MirValue, Target,
};
use evocc_types::{EvoIRType, Region};
#[path = "support/frame_variants.rs"]
mod frame_variants;
use frame_variants::compile_module_for;

/// The runtime and image the build-environment contract seals.
const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-nexlab:sealed";

/// The kernel's idiom in C, for the reference compilers.
const PROBE_C: &str = r#"int probe(void)
{
    unsigned long stored[4];
    stored[0] = (unsigned long)__builtin_return_address(0);
    stored[1] = (unsigned long)__builtin_frame_address(0);
    stored[2] = (unsigned long)__builtin_return_address(0);
    stored[3] = (unsigned long)__builtin_frame_address(0);
    return (stored[0] != stored[2])
         | ((stored[1] != stored[3]) << 1)
         | ((stored[0] == 0) << 2)
         | ((stored[1] == 0) << 3);
}
"#;

/// The host `main` both builds link against, so the probe is a called
/// function with a caller of its own rather than the program's entry.
const MAIN_C: &str = r#"int probe(void);

int main(void)
{
    return probe();
}
"#;

/// The same probe in EvoIR, written by hand because the front end does not
/// emit the two forms yet.
///
/// The four `unsigned long` locals are one 32-byte automatic object: each of
/// the four readings is converted with `ptr.to_int` and stored, and the facts
/// are read back out of the frame, exactly as the kernel's helpers store
/// `_RET_IP_` before passing it on.
const UNIT: &str = "\
func @probe(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    (%v2, %v3, %v4) = alloca %v0, %v1 [origin=0, size=32, align=8]
    %v5 = return_address : ptr<generic>
    %v6 = frame_address : ptr<generic>
    %v7 = return_address : ptr<generic>
    %v8 = frame_address : ptr<generic>
    %v9 = ptr.to_int %v5
    %v10 = ptr.to_int %v6
    %v11 = ptr.to_int %v7
    %v12 = ptr.to_int %v8
    %v13 = const.bits<64> 1
    %v14 = ptr.offset.add.unsigned %v2, %v13 [scale=8]
    %v15 = pointer_valid %v3, %v14
    %v16 = const.bits<64> 2
    %v17 = ptr.offset.add.unsigned %v2, %v16 [scale=8]
    %v18 = pointer_valid %v15, %v17
    %v19 = const.bits<64> 3
    %v20 = ptr.offset.add.unsigned %v2, %v19 [scale=8]
    %v21 = pointer_valid %v18, %v20
    %v22 = access_in_bounds %v21, %v2 [size=8, align=8]
    (%v23, %v24) = store %v22, %v4, %v2, %v9
    %v25 = access_in_bounds %v23, %v14 [size=8, align=8]
    (%v26, %v27) = store %v25, %v24, %v14, %v10
    %v28 = access_in_bounds %v26, %v17 [size=8, align=8]
    (%v29, %v30) = store %v28, %v27, %v17, %v11
    %v31 = access_in_bounds %v29, %v20 [size=8, align=8]
    (%v32, %v33) = store %v31, %v30, %v20, %v12
    %v34 = access_in_bounds %v32, %v2 [size=8, align=8]
    (%v35, %v36, %v37) = load %v34, %v33, %v2 : bits<64>
    %v38 = access_in_bounds %v36, %v14 [size=8, align=8]
    (%v39, %v40, %v41) = load %v38, %v37, %v14 : bits<64>
    %v42 = access_in_bounds %v40, %v17 [size=8, align=8]
    (%v43, %v44, %v45) = load %v42, %v41, %v17 : bits<64>
    %v46 = access_in_bounds %v44, %v20 [size=8, align=8]
    (%v47, %v48, %v49) = load %v46, %v45, %v20 : bits<64>
    %v50 = const.bits<64> 0
    %v51 = icmp.ne %v35, %v43
    %v52 = icmp.ne %v39, %v47
    %v53 = icmp.eq %v35, %v50
    %v54 = icmp.eq %v39, %v50
    %v55 = zext.bits<32> %v51
    %v56 = zext.bits<32> %v52
    %v57 = zext.bits<32> %v53
    %v58 = zext.bits<32> %v54
    %v59 = const.bits<32> 1
    %v60 = const.bits<32> 2
    %v61 = const.bits<32> 3
    %v62 = shl.bits<32> %v56, %v59
    %v63 = shl.bits<32> %v57, %v60
    %v64 = shl.bits<32> %v58, %v61
    %v65 = or.bits<32> %v55, %v62
    %v66 = or.bits<32> %v65, %v63
    %v67 = or.bits<32> %v66, %v64
    ret %v67, %v48, %v49
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

#[test]
fn both_interpreters_report_no_failure() {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    let arguments = vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ];
    let outcome = interpret_module(
        &module,
        "probe",
        &arguments,
        InterpreterLimits::new(100_000),
    )
    .expect("the execution is defined");
    let evoir = outcome
        .return_values
        .iter()
        .find(|value| matches!(value.ty, EvoIRType::Bits(_)))
        .expect("one ordinary result")
        .raw_bits;
    assert_eq!(
        evoir, 0,
        "the stored readings agree with the second ones and neither address is null"
    );

    let lowered = lower_module_for(Target::Darwin, &module, &|_| false).expect("lowering");
    let machine = interpret_machine_module(&lowered, "probe", &[], MirLimits::new(100_000))
        .expect("the machine interpreter executes the probe");
    assert_eq!(
        machine.results.first(),
        Some(&MirValue::Int(0)),
        "and the machine interpreter says the same"
    );
}

fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("evocc-activation-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// Build one program from the given sources with `clang` and run it.
fn build_and_run_on_darwin(directory: &Path, name: &str, sources: &[&Path]) -> Result<i32, String> {
    // A clang on another host cannot link Darwin text.
    if !cfg!(target_os = "macos") {
        return Err("the host is not macOS".to_owned());
    }
    let program = directory.join(name);
    let mut command = Command::new("clang");
    command.args(["-O0", "-o"]).arg(&program);
    for source in sources {
        command.arg(source);
    }
    let built = command
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !built.status.success() {
        return Err(format!(
            "the toolchain refused {name}:\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    Command::new(&program)
        .output()
        .map_err(|error| error.to_string())?
        .status
        .code()
        .ok_or_else(|| format!("{name} did not exit normally"))
}

#[test]
fn the_native_run_agrees_with_the_host_compiler_on_darwin() {
    let directory = scratch("darwin");
    let ours = directory.join("probe.s");
    let reference = directory.join("probe.c");
    let host_main = directory.join("main.c");
    std::fs::write(&ours, emitted(Target::Darwin)).expect("the emitted text");
    std::fs::write(&reference, PROBE_C).expect("the C probe");
    std::fs::write(&host_main, MAIN_C).expect("the host main");

    let reference_status =
        match build_and_run_on_darwin(&directory, "reference", &[&reference, &host_main]) {
            Ok(status) => status,
            Err(reason) => {
                eprintln!("the Darwin lane is skipped: {reason}");
                return;
            }
        };
    let our_status = build_and_run_on_darwin(&directory, "ours", &[&ours, &host_main])
        .expect("the emitted text assembles, links against the host main and runs");
    let _ = std::fs::remove_dir_all(&directory);

    assert_eq!(
        reference_status, 0,
        "the host compiler's build of the builtins reports no failure"
    );
    assert_eq!(
        our_status, reference_status,
        "and ours reports exactly what the host compiler's build does"
    );
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
        .join(format!("evocc-activation-linux-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    std::fs::write(directory.join("ours.s"), emitted(Target::Linux)).expect("the emitted text");
    std::fs::write(directory.join("probe.c"), PROBE_C).expect("the C probe");
    std::fs::write(directory.join("main.c"), MAIN_C).expect("the host main");
    let script = "cd /work/probes && gcc -o ours ours.s main.c && \
                  gcc -O0 -o reference probe.c main.c && \
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
        .expect("the sealed container runs");
    let stdout = String::from_utf8_lossy(&run.stdout).trim().to_owned();
    let _ = std::fs::remove_dir_all(&directory);
    assert!(
        run.status.success(),
        "the container lane failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let statuses = stdout
        .lines()
        .last()
        .expect("the last line carries both statuses");
    assert_eq!(
        statuses, "0 0",
        "ours and GCC-14's build of the builtins both report no failure: {stdout}"
    );
}
