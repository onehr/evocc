//! Object copies remain opaque across scalar equality optimization and replay.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use evocc_eqopt::{optimize_scalar_module_with_evp1, ScalarEqBudget};
use evocc_evoir::{parse_section9_module, render_module, validate_module};
use evocc_proof::{parse_evp1, render_evp1, sha256_hex, EvpCertificate};

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

    fn reject_forged_output(&self, certificate: &EvpCertificate, forged: &str) {
        // A hostile producer controls both after.evo and its claimed digest.
        // Keep the candidate well typed and canonical: malformed text, stale
        // checksums or whitespace alone must not satisfy this semantic test.
        let module = parse_section9_module(forged.as_bytes()).unwrap();
        validate_module(&module).unwrap();
        let forged = render_module(&module);
        let digest = sha256_hex(forged.as_bytes());
        assert_ne!(
            digest, certificate.output_digest,
            "mutation disappeared in the codec"
        );
        let mut certificate = certificate.clone();
        certificate.output_digest = digest;
        fs::write(&self.after, &forged).unwrap();
        fs::write(&self.proof, render_evp1(&certificate)).unwrap();

        // This deliberately passes a producer-only integrity check. Acceptance
        // must instead fail when independently rebuilding the authorized IR.
        let written = parse_evp1(&fs::read(&self.proof).unwrap()).unwrap();
        assert_eq!(
            written.output_digest,
            sha256_hex(&fs::read(&self.after).unwrap())
        );
        let output = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&self.before, &self.after, &self.proof])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(3),
            "forged semantic output: {forged}\n{output:?}"
        );
    }
}

impl Drop for ProofFiles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

const MODULE: &str = concat!(
    "func @identity_around_copy(%arg0: bits<32>, %arg1: ptr<generic>, %arg2: ptr<generic>, %arg3: mem<all>, %arg4: world) -> (bits<32>, mem<all>, world) cc = target_default\n",
    "    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n",
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
fn scalar_proof_cannot_rewrite_strict_fp_or_erase_its_observable_status() {
    let source = include_str!("../../../tests/fixtures/floating-strict-world.evoir");
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.certificate.regions.is_empty());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    let after = render_module(&outcome.module);
    fs::write(&files.after, &after).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([&files.before, &files.after, &files.proof])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    files.reject_forged_output(
        &outcome.certificate,
        &after.replace("fp.strict.add", "fp.strict.sub"),
    );
    // This forgery remains internally well typed, correctly summarized and
    // canonically serialized; a stale checksum is not the reason for rejection.
    let pure = after
        .replace(
            "(%v4, %v5) = fp.strict.add %v3, %v0, %v1",
            "%v4 = fp.add %v0, %v1",
        )
        .replace("fenv=true", "fenv=false")
        .replace("ret %v4, %v2, %v5", "ret %v4, %v2, %v3");
    files.reject_forged_output(&outcome.certificate, &pure);
    files.reject_forged_output(
        &outcome.certificate,
        &after.replace("%v3, %v0, %v1 : f64", "%v3, %v1, %v0 : f64"),
    );
}

#[test]
fn scalar_proof_cannot_change_quiet_comparison_exception_behavior() {
    let source = include_str!("../../../tests/fixtures/floating-strict-world.evoir")
        .replace("-> (f64, mem<all>, world)", "-> (bits<1>, mem<all>, world)")
        .replace(
            "fp.strict.add %v3, %v0, %v1 : f64",
            "fp.strict.quiet_olt %v3, %v0, %v1 : bits<1>",
        );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.certificate.regions.is_empty());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    files.reject_forged_output(
        &outcome.certificate,
        &render_module(&outcome.module).replace("fp.strict.quiet_olt", "fp.strict.olt"),
    );
}

#[test]
fn scalar_proof_cannot_drop_or_relocate_a_patchable_function_entry() {
    let source = MODULE.replace(
        "cc = target_default",
        "cc = target_default patchable_entry=4,2 section=\".init.text\"",
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    let after = render_module(&outcome.module);
    assert!(after.contains("patchable_entry=4,2"));
    assert!(after.contains("section=\".init.text\""));
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    fs::write(&files.after, &after).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    assert!(replay().status.success());
    for (field, replacement) in [
        (" patchable_entry=4,2", ""),
        (" patchable_entry=4,2", " patchable_entry=4,1"),
        (" patchable_entry=4,2", " patchable_entry=0,0"),
        (" section=\".init.text\"", ""),
        (" section=\".init.text\"", " section=\".text\""),
        (" section=\".init.text\"", " section=\".exit.text\""),
    ] {
        let forged = after.replace(field, replacement);
        validate_module(&parse_section9_module(forged.as_bytes()).unwrap()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proof_cannot_reclassify_a_homogeneous_floating_argument() {
    let source = MODULE.replace(
        "%arg1: ptr<generic>,",
        "%arg1: ptr<generic> byval(8, 8, hfa=1xf64),",
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    fs::write(&files.after, &after).unwrap();
    assert!(Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
        .args([&files.before, &files.after, &files.proof])
        .output()
        .unwrap()
        .status
        .success());
    for replacement in ["hfa=2xf32", "hva=1xvec<2, bits<32>>"] {
        let forged = after.replace("hfa=1xf64", replacement);
        validate_module(&parse_section9_module(forged.as_bytes()).unwrap()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proof_cannot_change_floating_bits_operations_or_predicates() {
    let source = MODULE.replace(
        "    %v9 = add.bits<32> %v6, %v5",
        concat!(
            "    %v10 = const.f64 0x7ff8000000000011\n",
            "    %v11 = const.f64 0x3ff0000000000000\n",
            "    %v12 = fp.add %v10, %v11 : f64\n",
            "    %v13 = fp.to_int_defined.s32 %v12 : bits<1>\n",
            "    %v9 = add.bits<32> %v6, %v5"
        ),
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    fs::write(&files.after, &after).unwrap();
    assert!(replay().status.success());
    for forged in [
        after.replace("0x7ff8000000000011", "0x7ff8000000000012"),
        after.replace("fp.add", "fp.sub"),
        after.replace("to_int_defined.s32", "to_int_defined.u32"),
    ] {
        assert_ne!(forged, after);
        validate_module(&parse_section9_module(forged.as_bytes()).unwrap()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proof_cannot_drop_or_change_hfa_transport_alignment() {
    // The canonical spelling omits the default ABI alignment of eight.
    for (original, replacement) in [("", ", abi_align=16"), (", abi_align=16", "")] {
        let mark = format!("byval(32, 32, hfa=4xf64{original})");
        let source = MODULE.replace(
            "%arg1: ptr<generic>,",
            &format!("%arg1: ptr<generic> {mark},"),
        );
        let before = parse_section9_module(source.as_bytes()).unwrap();
        let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
        let files = ProofFiles::new();
        fs::write(&files.before, render_module(&before)).unwrap();
        fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
        let after = render_module(&outcome.module);
        fs::write(&files.after, &after).unwrap();
        let replay = || {
            Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
                .args([&files.before, &files.after, &files.proof])
                .output()
                .unwrap()
        };
        assert!(replay().status.success());
        let forged = after.replace(&mark, &format!("byval(32, 32, hfa=4xf64{replacement})"));
        assert_ne!(forged, after);
        validate_module(&parse_section9_module(forged.as_bytes()).unwrap()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proof_cannot_change_simd_assembly_constraints_or_clobbers() {
    let source = MODULE
        .replace("volatile=false", "volatile=true")
        .replace("io=false", "io=true")
        .replace("fenv=false", "fenv=true")
        .replace("    (%v7, %v8) = copy %v3, %v4, %v1, %v2 [size=4, align=4]", "    %v10 = const.bits<64> 0\n    %v11 = vec.pack %v10, %v10 : vec<2, bits<64>>\n    (%v12, %v7, %v8) = asm volatile \"\" %v3, %v4 outputs [(\"+w\", \"\", vec<2, bits<64>>, %v11)] inputs [] clobbers []");
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    fs::write(&files.after, &after).unwrap();
    assert!(replay().status.success());
    for forged in [
        after.replace("\"+w\"", "\"+&w\""),
        after.replace("clobbers []", "clobbers [\"v0\"]"),
    ] {
        assert_ne!(forged, after);
        validate_module(&parse_section9_module(forged.as_bytes()).unwrap()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proof_cannot_replace_an_aes_round_with_its_inverse() {
    let source = MODULE.replace("    %v9 = add.bits<32> %v6, %v5", &format!(
        "    %v10 = const.bits<8> 0\n    %v11 = vec.pack {} : vec<16, bits<8>>\n    %v12 = vec.aese %v11, %v11 : vec<16, bits<8>>\n    %v9 = add.bits<32> %v6, %v5",
        ["%v10"; 16].join(", ")
    ));
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values > 0);
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    fs::write(&files.after, &after).unwrap();
    assert!(replay().status.success());
    let forged = after.replace("vec.aese", "vec.aesd");
    assert_ne!(forged, after);
    let module = parse_section9_module(forged.as_bytes()).unwrap();
    validate_module(&module).unwrap();
    files.reject_forged_output(&outcome.certificate, &forged);
}

#[test]
fn scalar_proofs_preserve_load_instance_and_tls_metadata() {
    let startup = concat!(
        "func @startup(%arg0: mem<all>, %arg1: world) -> (mem<all>, world) cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n",
        "{\n^entry(%v0: mem<all>, %v1: world):\n    ret %v0, %v1\n}\n\n",
    );
    let with_access = MODULE.replace(
        "    (%v7, %v8) = copy %v3, %v4,",
        "    (%v10, %v11, %v12) = tls_addr @local, %v3, %v4\n    (%v7, %v8) = copy %v11, %v12,",
    );
    let source = format!(
        "global @local origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\n\
         global @other origin=1 kind=file visibility=external size=4 align=4 bytes=09000000 initialized=1111\n\n\
         {startup}{}{with_access}\n\
         tls @local model=global_dynamic\ntls @other model=global_dynamic\n\n\
         init @startup priority=201\ninit @second priority=201\nfini @startup priority=202\n",
        startup.replace("@startup", "@second"),
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values > 0);
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    fs::write(&files.after, &after).unwrap();
    let result = replay();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for forged in [
        after.replacen("priority=201", "priority=203", 1),
        after.replace("priority=202", "priority=204"),
        after.replace("init @startup priority=201\n", ""),
        after.replace("init @second", "fini @second"),
        after.replace("fini @startup", "fini @second"),
        after.replace(
            "init @startup priority=201\ninit @second priority=201",
            "init @second priority=201\ninit @startup priority=201",
        ),
        after.replacen("model=global_dynamic", "model=initial_exec", 1),
        after.replace("tls @other model=global_dynamic\n", ""),
        after.replace("tls_addr @local", "tls_addr @other"),
    ] {
        assert_ne!(forged, after);
        parse_section9_module(forged.as_bytes()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proofs_cannot_change_atomic_order_or_drop_an_atomic_access() {
    let source = MODULE.replace("atomic=false", "atomic=true").replace(
        "    (%v7, %v8) = copy %v3, %v4, %v1, %v2 [size=4, align=4]",
        "    %v10 = access_in_bounds %v3, %v1 [size=4, align=4]\n    (%v7, %v8) = atomic.store release %v10, %v4, %v1, %v6",
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values > 0);
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    fs::write(&files.after, &after).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    assert!(replay().status.success());
    for forged in [
        after.replace("atomic.store release", "atomic.store relaxed"),
        after
            .replace("atomic.store release", "store")
            .replace("atomic=true", "atomic=false"),
    ] {
        parse_section9_module(forged.as_bytes()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn vector_lane_selection_is_preserved_outside_scalar_rewrites() {
    let source=MODULE.replace("    %v9 = add.bits<32> %v6, %v5", "    %v10 = vec.pack %v0, %v5, %v0, %v5 : vec<4, bits<32>>\n    %v11 = const.bits<32> 0\n    %v12 = const.bits<32> 1\n    %v13 = vec.extract %v10, %v11 : bits<32>\n    %v9 = add.bits<32> %v13, %v5");
    let source = format!("declare @vector_argument(ptr<generic> byval(24, 8, hva=3xvec<2, bits<32>>), mem<all>, world) -> (mem<all>, world) cc = target_default\n\n{source}");
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values > 0);
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    fs::write(&files.after, render_module(&outcome.module)).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    assert!(replay().status.success());
    let mut forged = outcome.module.clone();
    let block = &mut forged.functions[0].blocks[0];
    let replacement = block
        .insts
        .iter()
        .find_map(|inst| match inst {
            evocc_evoir::Inst::Const(value, 1) => Some(value.clone()),
            _ => None,
        })
        .unwrap();
    let extract = block
        .insts
        .iter_mut()
        .find_map(|inst| match inst {
            evocc_evoir::Inst::Vector(op) if op.op == evocc_evoir::VectorOp::Extract => Some(op),
            _ => None,
        })
        .unwrap();
    extract.arguments[1] = replacement;
    validate_module(&forged).unwrap();
    files.reject_forged_output(&outcome.certificate, &render_module(&forged));
    let after = render_module(&outcome.module);
    for forged in [
        after.replace(", hva=3xvec<2, bits<32>>", ""),
        after.replace("hva=3xvec<2, bits<32>>", "hva=3xvec<4, bits<16>>"),
    ] {
        let module = parse_section9_module(forged.as_bytes()).unwrap();
        validate_module(&module).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

#[test]
fn scalar_proofs_cannot_change_visibility_of_any_symbol_record() {
    let source = format!(
        "global @value origin=0 kind=file visibility=protected size=4 align=4 bytes=00000000 initialized=1111\n\nextern @outside size=4 align=4 visibility=protected\n\ndeclare @missing(mem<all>, world) -> (mem<all>, world) cc = target_default visibility=protected\n\nalias @other = @value visibility=protected\n\n{}",
        MODULE.replace("cc = target_default\n", "cc = target_default visibility=protected\n")
    );
    let before = parse_section9_module(source.as_bytes()).unwrap();
    let outcome = optimize_scalar_module_with_evp1(&before, ScalarEqBudget::default());
    assert!(outcome.report.optimized_values > 0);
    let files = ProofFiles::new();
    fs::write(&files.before, render_module(&before)).unwrap();
    fs::write(&files.proof, render_evp1(&outcome.certificate)).unwrap();
    let after = render_module(&outcome.module);
    fs::write(&files.after, &after).unwrap();
    let replay = || {
        Command::new(env!("CARGO_BIN_EXE_evo-proofcheck"))
            .args([&files.before, &files.after, &files.proof])
            .output()
            .unwrap()
    };
    assert!(replay().status.success());
    for prefix in ["global @", "extern @", "declare @", "alias @", "func @"] {
        let forged = after
            .lines()
            .map(|line| {
                if line.starts_with(prefix) {
                    line.replace("visibility=protected", "visibility=hidden")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        assert_ne!(forged, after, "{prefix}");
        parse_section9_module(forged.as_bytes()).unwrap();
        files.reject_forged_output(&outcome.certificate, &forged);
    }
}

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
    files.reject_forged_output(
        &outcome.certificate,
        &render_module(&outcome.module).replace("copy.volatile.read ", "copy.volatile.write "),
    );
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
    files.reject_forged_output(&outcome.certificate, &render_module(&forged));
}
