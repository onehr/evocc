//! EvoMIR Section 8, "Linkage" (revision 0.16), through a real link.
//!
//! "This is what lets two EvoCC objects that share a header link together
//! (EvoIR 1.9.70)." The claim is about a linker, so it is measured with one:
//! two EvoCC-built objects that each define a `bump` of their own, and a
//! `main` the host compiler builds, are linked into one program, and its exit
//! status is compared with the reference compiler's build of the same three C
//! sources. Before the revision the two objects both declared `_bump` global
//! and the link failed with `multiple definition`.
//!
//! The second program is the other half of the rule: an unreferenced internal
//! function that calls a function nothing defines must not reach the
//! assembler, or the object carries an undefined symbol the link cannot
//! resolve. That is the shape a shared kernel header has hundreds of.

use std::path::{Path, PathBuf};
use std::process::Command;

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// The runtime and image the build-environment contract seals.
const CONTAINER_BIN: &str = "/usr/local/bin/container";
const IMAGE: &str = "evo-flight-lab:sealed";

/// The first unit: a `static` helper and the function that calls it.
const ONE_C: &str =
    "static int bump(int x) { return x + 1; }\nint one(int x) { return bump(x); }\n";

/// The second unit, with a helper of exactly the same name.
const TWO_C: &str =
    "static int bump(int x) { return x + 2; }\nint two(int x) { return bump(x); }\n";

/// The host compiler's `main`, which calls both units.
const MAIN_C: &str = "int one(int);\nint two(int);\nint main(void) { return one(10) + two(20); }\n";

/// 11 + 22, the answer both builds have to reach.
const ANSWER: i32 = 33;

/// A unit whose helper nothing calls and which itself calls a function the
/// configuration does not build.
///
/// The helper is `static inline` because that is the shape EvoIR 1.9.70 names
/// and the one both reference compilers drop: GCC at `-O0` still emits a plain
/// unreferenced `static` function, and its own build of this program then
/// fails to link on the call to `absent`.
const STUB_C: &str = "extern int absent(int);\nstatic inline int stub(int x) { return absent(x); }\nint three(void) { return 7; }\n";

/// Its `main`.
const STUB_MAIN_C: &str = "int three(void);\nint main(void) { return three(); }\n";

/// The answer of the second program.
const STUB_ANSWER: i32 = 7;

const PURE: &str = "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }";
const TOP: &str = "    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }";

/// `ONE_C` and `TWO_C` in EvoIR, written by hand because the front end does
/// not yet carry a storage class as far as the function header.
fn unit(exported: &str, addend: u32) -> String {
    format!(
        "func @bump(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
{PURE}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> {addend}
    %v4 = add.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}}

func @{exported}(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
{PURE}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @bump(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}}
"
    )
}

/// `STUB_C` in EvoIR.
fn stub_unit() -> String {
    format!(
        "declare @absent(bits<32>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default

func @stub(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
{TOP}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @absent(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}}

func @three(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
{PURE}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 7
    ret %v2, %v0, %v1
}}
"
    )
}

fn emitted(target: Target, text: &str) -> String {
    let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

/// The files one program needs, written into a fresh directory.
fn lay_out(directory: &Path, target: Target, files: &[(&str, String)]) {
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory).expect("a scratch directory");
    let _ = std::fs::write(directory.join(".metadata_never_index"), b"");
    for (name, content) in files {
        let content = if name.ends_with(".s") {
            emitted(target, content)
        } else {
            content.clone()
        };
        std::fs::write(directory.join(name), content).expect("a probe file");
    }
}

/// The two programs' files, named as the build scripts expect them.
fn shared_helper_files(target: Target) -> Vec<(&'static str, String)> {
    let _ = target;
    vec![
        ("one.s", unit("one", 1)),
        ("two.s", unit("two", 2)),
        ("main.c", MAIN_C.to_owned()),
        ("one.c", ONE_C.to_owned()),
        ("two.c", TWO_C.to_owned()),
    ]
}

fn stub_files() -> Vec<(&'static str, String)> {
    vec![
        ("three.s", stub_unit()),
        ("main.c", STUB_MAIN_C.to_owned()),
        ("three.c", STUB_C.to_owned()),
    ]
}

fn scratch(name: &str) -> PathBuf {
    workspace()
        .join("target")
        .join(format!("evocc-linkage-{name}-{}", std::process::id()))
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the crate lives two directories below the workspace root")
        .to_path_buf()
}

/// Run one shell script in `directory` with the host compiler.
fn on_darwin(directory: &Path, script: &str) -> Result<String, String> {
    let run = Command::new("bash")
        .arg("-c")
        .arg(script)
        .current_dir(directory)
        .output()
        .map_err(|error| format!("bash is absent: {error}"))?;
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    if !run.status.success() {
        return Err(format!(
            "the host toolchain refused the program:\n{stdout}\n{}",
            String::from_utf8_lossy(&run.stderr)
        ));
    }
    Ok(stdout)
}

fn clang() -> Result<(), String> {
    let output = Command::new("clang")
        .arg("--version")
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    let text = String::from_utf8_lossy(&output.stdout);
    if !text.contains("arm64") && !text.contains("aarch64") {
        return Err("the host toolchain does not target arm64".to_owned());
    }
    Ok(())
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
    if !String::from_utf8_lossy(&listed.stdout).contains("evo-flight-lab") {
        return Err(format!("{IMAGE} is not present"));
    }
    Ok(())
}

/// Run one shell script inside the sealed container, on the probe directory.
fn in_container(directory: &Path, script: &str) -> String {
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
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "the container run failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    stdout
}

/// The two statuses a script prints on its last line.
fn statuses(stdout: &str) -> (i32, i32) {
    let last = stdout
        .lines()
        .last()
        .unwrap_or_else(|| panic!("the script prints the two statuses:\n{stdout}"));
    let mut fields = last.split_whitespace();
    let ours = fields.next().and_then(|f| f.parse().ok());
    let reference = fields.next().and_then(|f| f.parse().ok());
    match (ours, reference) {
        (Some(ours), Some(reference)) => (ours, reference),
        _ => panic!("the script's last line is not two statuses: {last:?}\n{stdout}"),
    }
}

const SHARED_SCRIPT: &str = "cc -c -o one.o one.s && cc -c -o two.o two.s && \
     cc -o ours one.o two.o main.c && cc -O0 -o reference one.c two.c main.c && \
     ./ours; ours=$?; ./reference; echo \"$ours $?\"";

const STUB_SCRIPT: &str = "cc -c -o three.o three.s && cc -o ours three.o main.c && \
     cc -O0 -o reference three.c main.c && \
     ./ours; ours=$?; ./reference; echo \"$ours $?\"";

#[test]
fn two_objects_that_share_a_helper_name_link_and_agree_with_clang() {
    if let Err(reason) = clang() {
        eprintln!("the Darwin lane is skipped: {reason}");
        return;
    }
    let directory = scratch("shared-darwin");
    lay_out(
        &directory,
        Target::Darwin,
        &shared_helper_files(Target::Darwin),
    );
    let stdout = on_darwin(&directory, &SHARED_SCRIPT.replace("cc ", "clang "))
        .expect("the two objects assemble, link and run");
    let _ = std::fs::remove_dir_all(&directory);
    let (ours, reference) = statuses(&stdout);
    assert_eq!(ours, reference, "the two builds agree:\n{stdout}");
    assert_eq!(reference, ANSWER, "on 11 + 22");
}

#[test]
fn two_objects_that_share_a_helper_name_link_and_agree_with_gcc() {
    if let Err(reason) = container() {
        eprintln!("the Linux lane is skipped: {reason}");
        return;
    }
    let directory = scratch("shared-linux");
    lay_out(
        &directory,
        Target::Linux,
        &shared_helper_files(Target::Linux),
    );
    let stdout = in_container(
        &directory,
        &format!(
            "cd /work/probes && {}",
            SHARED_SCRIPT.replace("cc ", "gcc ")
        ),
    );
    let _ = std::fs::remove_dir_all(&directory);
    let (ours, reference) = statuses(&stdout);
    assert_eq!(ours, reference, "the two builds agree:\n{stdout}");
    assert_eq!(reference, ANSWER, "on 11 + 22");
}

#[test]
fn an_unreferenced_stub_leaves_no_undefined_symbol_for_clang() {
    if let Err(reason) = clang() {
        eprintln!("the Darwin lane is skipped: {reason}");
        return;
    }
    let directory = scratch("stub-darwin");
    lay_out(&directory, Target::Darwin, &stub_files());
    let stdout = on_darwin(&directory, &STUB_SCRIPT.replace("cc ", "clang "))
        .expect("the object assembles, links and runs");
    let _ = std::fs::remove_dir_all(&directory);
    let (ours, reference) = statuses(&stdout);
    assert_eq!(ours, reference, "the two builds agree:\n{stdout}");
    assert_eq!(
        reference, STUB_ANSWER,
        "on the only function anything calls"
    );
}

#[test]
fn an_unreferenced_stub_leaves_no_undefined_symbol_for_gcc() {
    if let Err(reason) = container() {
        eprintln!("the Linux lane is skipped: {reason}");
        return;
    }
    let directory = scratch("stub-linux");
    lay_out(&directory, Target::Linux, &stub_files());
    let stdout = in_container(
        &directory,
        &format!("cd /work/probes && {}", STUB_SCRIPT.replace("cc ", "gcc ")),
    );
    let _ = std::fs::remove_dir_all(&directory);
    let (ours, reference) = statuses(&stdout);
    assert_eq!(ours, reference, "the two builds agree:\n{stdout}");
    assert_eq!(
        reference, STUB_ANSWER,
        "on the only function anything calls"
    );
}
