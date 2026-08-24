use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Inputs {
    dir: PathBuf,
    before: PathBuf,
    after: PathBuf,
    proof: PathBuf,
}

impl Inputs {
    fn new() -> Self {
        let id = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("evocc-day8-proof-{}-{id}", std::process::id()));
        fs::create_dir(&dir).expect("create proof fixture directory");
        Self {
            before: dir.join("before.evo"),
            after: dir.join("after.evo"),
            proof: dir.join("proof.evp"),
            dir,
        }
    }

    fn write(&self, before: &[u8], after: &[u8], proof: &str) {
        fs::write(&self.before, before).expect("write before.evo");
        fs::write(&self.after, after).expect("write after.evo");
        fs::write(&self.proof, proof).expect("write proof.evp");
    }

    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([
                self.before.as_path(),
                self.after.as_path(),
                self.proof.as_path(),
            ])
            .output()
            .expect("run evo-proofcheck")
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn certificate(input: &str, output: &str) -> String {
    format!("evp1\nregistry p0.scalar.v1 {REGISTRY_SHA256}\ninput {input}\noutput {output}\n")
}

fn assert_status(output: &Output, expected: i32, label: &str) {
    assert_eq!(
        output.status.code(),
        Some(expected),
        "{label}: stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn day8_proof_format_eligibility_and_binding_contract() {
    let probe = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .arg("--probe")
        .output()
        .expect("probe evo-proofcheck");
    assert_status(&probe, 0, "probe");
    assert!(probe.stdout.is_empty());
    assert!(probe.stderr.is_empty());

    let inputs = Inputs::new();
    inputs.write(b"", b"", &certificate(EMPTY_SHA256, EMPTY_SHA256));
    assert_status(&inputs.run(), 0, "valid zero-region certificate");

    inputs.write(
        b"",
        b"",
        &certificate(
            "0000000000000000000000000000000000000000000000000000000000000000",
            EMPTY_SHA256,
        ),
    );
    assert_status(&inputs.run(), 3, "certificate bound to different input");

    let nonempty_after = b"func @main() -> () cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry():\n    ret\n}\n";
    inputs.write(
        b"",
        nonempty_after,
        &certificate(EMPTY_SHA256, EMPTY_SHA256),
    );
    assert_status(&inputs.run(), 3, "zero-region proof cannot change output");

    inputs.write(b"\n", b"", &certificate(EMPTY_SHA256, EMPTY_SHA256));
    assert_status(&inputs.run(), 2, "noncanonical module is malformed");

    inputs.write(b"", b"", "evp0\n");
    assert_status(&inputs.run(), 2, "unknown proof version is malformed");
}
