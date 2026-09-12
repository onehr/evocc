//! Object copies remain opaque across scalar equality optimization and replay.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{parse_section9_module, render_module, validate_module};
use evocc_proof::render_evp1;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct ProofFiles {
    directory: PathBuf,
    before: PathBuf,
    after: PathBuf,
    proof: PathBuf,
}

impl ProofFiles {
    fn new() -> Self {
        let ordinal = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("evocc-copy-proof-{}-{ordinal}", std::process::id()));
        fs::create_dir(&directory).expect("create copy proof fixture directory");
        Self {
            before: directory.join("before.evo"),
            after: directory.join("after.evo"),
            proof: directory.join("proof.evp"),
            directory,
        }
    }
}

impl Drop for ProofFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

const MODULE: &str = concat!(
    "func @identity_around_copy(%arg0: bits<32>, %arg1: ptr<generic>, %arg2: ptr<generic>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default\n",
    "    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, may_trap=true, may_unwind=false }\n",
    "{\n",
    "^entry(%v0: bits<32>, %v1: ptr<generic>, %v2: ptr<generic>, %v3: mem<all>, %v4: world):\n",
    "    %v5 = const.bits<32> 0\n",
    "    %v6 = add.bits<32> %v0, %v5\n",
    "    (%v7, %v8) = copy %v3, %v4, %v1, %v2 [size=4, align=4]\n",
    "    %v9 = add.bits<32> %v6, %v5\n",
    "    ret %v9, %v7, %v8\n",
    "}\n",
);

#[test]
fn copy_is_excluded_reassembled_verbatim_and_replayed_independently() {
    let before = parse_section9_module(MODULE.as_bytes()).expect("copy module parses");
    validate_module(&before).expect("copy module validates");

    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    validate_module(&outcome.module).expect("optimized copy module validates");
    assert!(
        outcome.report.optimized_values >= 2,
        "both add-zero regions must remain non-vacuous"
    );

    let before_copy = "    (%v7, %v8) = copy %v3, %v4, %v1, %v2 [size=4, align=4]";
    let after_copy = "    (%v6, %v7) = copy %v3, %v4, %v1, %v2 [size=4, align=4]";
    let before_text = render_module(&before);
    let after_text = render_module(&outcome.module);
    assert_eq!(
        before_text
            .lines()
            .filter(|line| *line == before_copy)
            .count(),
        1
    );
    assert_eq!(
        after_text
            .lines()
            .filter(|line| *line == after_copy)
            .count(),
        1
    );

    let files = ProofFiles::new();
    fs::write(&files.before, before_text).expect("write before module");
    fs::write(&files.after, after_text).expect("write after module");
    fs::write(&files.proof, render_evp1(&outcome.certificate)).expect("write certificate");

    let output = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([&files.before, &files.after, &files.proof])
        .output()
        .expect("run independent proof checker");
    assert_eq!(
        output.status.code(),
        Some(0),
        "proof checker rejected copy-opaque replay: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn volatile_copy_access_kind_survives_independent_reconstruction() {
    let source = MODULE
        .replace("= copy ", "= copy.volatile.read ")
        .replace("volatile=false", "volatile=true");
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values >= 2);
    let files = ProofFiles::new();
    fs::write(&files.before, source).unwrap();
    fs::write(&files.after, render_module(&outcome.module)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    let result = replay();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    // Both modules are well typed and volatile, but changing the event from
    // a read to a write is not authorized by a scalar equality proof.
    fs::write(
        &files.after,
        render_module(&outcome.module).replace("copy.volatile.read ", "copy.volatile.write "),
    )
    .unwrap();
    assert!(!replay().status.success());
}

#[test]
fn computed_edges_survive_renumbering_and_cannot_change_under_a_scalar_proof() {
    let source = include_str!("../../../tests/fixtures/computed-counter.evoir").replace(
        "    %v6 = access_in_bounds",
        "    %v36 = const.bits<32> 0\n    %v37 = add.bits<32> %v5, %v36\n    %v6 = access_in_bounds",
    ).replace("store %v6, %v4, %v2, %v5", "store %v6, %v4, %v2, %v37");
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    validate_module(&outcome.module).unwrap();
    assert!(
        outcome.report.optimized_values > 0,
        "replay must be nontrivial"
    );
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.after, render_module(&outcome.module)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    let output = replay();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut forged = outcome.module.clone();
    let branch = forged.functions[0]
        .blocks
        .iter_mut()
        .find_map(|block| {
            if let evocc_evoir::Inst::IndirectBr { target, edges } = &mut block.terminator {
                Some((target, edges))
            } else {
                None
            }
        })
        .unwrap();
    branch.1[0].arguments[0] = branch.0.clone();
    // Same type and local definition, but not the object's original address.
    // General SSA validation cannot establish the C variable correspondence.
    validate_module(&forged).unwrap();
    fs::write(&files.after, render_module(&forged)).unwrap();
    assert!(
        !replay().status.success(),
        "scalar proof must not authorize changed edge bindings"
    );
}
