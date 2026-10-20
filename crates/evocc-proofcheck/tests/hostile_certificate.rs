//! Invalid producer data must be refused, not panic or consume an unbounded
//! search over regions the certificate never claims. A timeout is a test failure.
use std::fmt::Write;
use std::path::PathBuf;
use std::time::Duration;

use evocc_proof::{sha256_hex, CheckerOutcome, StandaloneProofChecker};

const EFFECTS: &str = "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }\n";
const REGISTRY: &str =
    "registry p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9\n";

fn checker() -> StandaloneProofChecker {
    let binary = PathBuf::from(env!("CARGO_BIN_EXE_evo-proofcheck"));
    let digest = sha256_hex(&std::fs::read(&binary).unwrap());
    StandaloneProofChecker::new(binary, std::env::temp_dir())
        .with_expected_digest(digest)
        .with_timeout(Duration::from_secs(15))
}

fn module(body: &str) -> String {
    format!("func @f(%arg0: bits<32>) -> (bits<32>) cc = target_default\n{EFFECTS}{{\n^entry(%v0: bits<32>):\n{body}}}\n")
}

fn proof(before: &str, after: &str, regions: &str) -> String {
    format!(
        "evp1\n{REGISTRY}input {}\n{regions}output {}\n",
        sha256_hex(before.as_bytes()),
        sha256_hex(after.as_bytes())
    )
}

const ADD_ZERO: &str = concat!(
    "region f/entry/1 root %v2 witnesses 2 seeds 1\n",
    "witness 0 f/entry/0\nwitness 1 f/entry/1\n",
    "seed 0 %v0 bits<32> input c0\n",
    "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-\n",
    "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1\n",
    "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0\n",
    "extract 0 c0 seed:0\nroot c0\nendregion\n",
);

#[test]
fn out_of_range_references_and_counts_have_closed_exit_statuses() {
    let before =
        module("    %v1 = const.bits<32> 0\n    %v2 = add.bits<32> %v0, %v1\n    ret %v2\n");
    let after = module("    %v1 = const.bits<32> 0\n    ret %v0\n");
    let checker = checker();
    assert_eq!(
        checker.check_typed(&before, &after, &proof(&before, &after, ADD_ZERO)),
        CheckerOutcome::Valid
    );
    let mut failed = Vec::new();
    for (name, old, new, expected) in [
        (
            "rewrite reference",
            "class:c0",
            "class:c4294967295",
            CheckerOutcome::Invalid,
        ),
        (
            "extraction class",
            "extract 0 c0",
            "extract 0 c4294967295",
            CheckerOutcome::Invalid,
        ),
        (
            "root class",
            "\nroot c0",
            "\nroot c4294967295",
            CheckerOutcome::Invalid,
        ),
        (
            "witness count",
            "witnesses 2",
            "witnesses 18446744073709551615",
            CheckerOutcome::Malformed,
        ),
        (
            "seed count",
            "seeds 1",
            "seeds 18446744073709551615",
            CheckerOutcome::Malformed,
        ),
    ] {
        let mutant = proof(&before, &after, &ADD_ZERO.replace(old, new));
        let actual = checker.check_typed(&before, &after, &mutant);
        if actual != expected {
            failed.push(format!("{name}: expected {expected:?}, got {actual:?}"));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn a_long_dependency_chain_checks_only_the_claimed_regions() {
    let mut body = String::new();
    for id in 1..=50_000 {
        writeln!(body, "    %v{id} = add.bits<32> %v{}, %v0", id - 1).unwrap();
    }
    body.push_str("    ret %v50000\n");
    let before = module(&body);
    let checker = checker();
    assert_eq!(
        checker.check_typed(&before, &before, &proof(&before, &before, "")),
        CheckerOutcome::Valid
    );
    // A selected deep root must still have its complete closure independently
    // derived. This syntactically valid but incomplete manifest must be refused.
    let region = "region f/entry/49999 root %v50000 witnesses 0 seeds 0\nextract 0 c0 seed:0\nroot c0\nendregion\n";
    assert_eq!(
        checker.check_typed(&before, &before, &proof(&before, &before, region)),
        CheckerOutcome::Invalid
    );
}

#[test]
fn a_well_formed_module_that_fails_validation_is_invalid_not_malformed() {
    let before = module("    ret %v0\n");
    let checker = checker();
    for (name, after) in [
        ("return arity", module("    ret\n")),
        ("undefined value", module("    ret %v1\n")),
        (
            "duplicate value",
            module("    %v0 = const.bits<32> 1\n    ret %v0\n"),
        ),
        ("duplicate function", format!("{before}\n{before}")),
        (
            "calling convention",
            before.replace("cc = target_default", "cc = unknown"),
        ),
        ("zero width", before.replace("bits<32>", "bits<0>")),
        ("excess width", before.replace("bits<32>", "bits<65536>")),
    ] {
        assert_eq!(
            checker.check_typed(&before, &after, &proof(&before, &after, "")),
            CheckerOutcome::Invalid,
            "after: {name}"
        );
        assert_eq!(
            checker.check_typed(&after, &before, &proof(&after, &before, "")),
            CheckerOutcome::Invalid,
            "before: {name}"
        );
    }
}

#[test]
fn certificate_rejection_matrix_matches_the_wire_contract() {
    use CheckerOutcome::{Invalid, Malformed, Valid};
    let before =
        module("    %v1 = const.bits<32> 0\n    %v2 = add.bits<32> %v0, %v1\n    ret %v2\n");
    let after = module("    %v1 = const.bits<32> 0\n    ret %v0\n");
    let baseline = proof(&before, &after, ADD_ZERO);
    let checker = checker();
    assert_eq!(checker.check_typed(&before, &after, &baseline), Valid);
    // Every mutant keeps the correct input/output hashes. These rows exercise
    // Section 11.3, not rejection of stale bundle hashes.
    for (name, old, new, expected) in [
        ("version", "evp1", "evp0", Malformed),
        ("field count", "root c0\n", "root c0 extra\n", Malformed),
        ("digest spelling", "ebdf5191", "EBDF5191", Malformed),
        ("event sequence", "node 1 n1", "node 2 n1", Malformed),
        ("witness sequence", "witness 1 f", "witness 2 f", Malformed),
        ("seed sequence", "seed 0 %", "seed 1 %", Malformed),
        ("extraction sequence", "extract 0", "extract 1", Malformed),
        ("witness count", "witnesses 2", "witnesses 1", Malformed),
        ("seed count", "seeds 1", "seeds 0", Malformed),
        (
            "attribute order",
            "attrs=value=0",
            "attrs=z=0,a=0",
            Malformed,
        ),
        ("binding order", "bind ?x:c0", "bind ?z:c0,?a:c0", Malformed),
        ("missing root", "root c0\n", "", Malformed),
        ("missing end", "endregion\n", "", Malformed),
        ("blank line", "evp1\n", "evp1\n\n", Malformed),
        ("unknown operator", "op=add", "op=not_registered", Malformed),
        ("seed allocation", "input c0", "input c1", Invalid),
        ("node allocation", "node 1 n1", "node 1 n2", Invalid),
        ("owner allocation", "n1 c2", "n1 c3", Invalid),
        ("arity", "children=c0,c1", "children=c0", Invalid),
        ("unknown child", "children=c0,c1", "children=c0,c4294967295", Invalid),
        ("forward child", "children=c0,c1", "children=c0,c2", Invalid),
        (
            "sort",
            "op=add sort=bits<32>",
            "op=add sort=bits<16>",
            Invalid,
        ),
        (
            "effect",
            "effect=pure attrs=-",
            "effect=guard attrs=-",
            Invalid,
        ),
        ("attributes", "attrs=value=0", "attrs=value=1", Invalid),
        (
            "registry",
            "registry p0.scalar.v1",
            "registry unknown",
            Invalid,
        ),
        ("registry digest", "ebdf5191", "0bdf5191", Invalid),
        (
            "unknown rule",
            "p0_001_add_zero_rhs",
            "not_registered",
            Invalid,
        ),
        (
            "wrong rule",
            "p0_001_add_zero_rhs",
            "p0_003_mul_one_rhs",
            Invalid,
        ),
        ("binding", "bind ?x:c0", "bind ?x:c1", Invalid),
        ("missing binding", "bind ?x:c0", "bind -", Invalid),
        ("extra binding", "bind ?x:c0", "bind ?x:c0,?y:c0", Invalid),
        (
            "merge result",
            "c2 c0 c0 node:n1",
            "c2 c0 c2 node:n1",
            Invalid,
        ),
        (
            "noncanonical endpoint",
            "c2 c0 c0 node:n1",
            "c0 c0 c0 node:n1",
            Invalid,
        ),
        (
            "proof reference",
            "node:n1 class:c0",
            "node:n0 class:c0",
            Invalid,
        ),
        ("seed extraction", "seed:0", "seed:1", Invalid),
        ("extraction membership", "seed:0", "node:n0", Invalid),
        ("original root extraction", "seed:0", "node:n1", Invalid),
        (
            "duplicate extraction",
            "root c0\n",
            "extract 1 c0 seed:0\nroot c0\n",
            Invalid,
        ),
        (
            "witness identity",
            "witness 0 f/entry/0",
            "witness 0 f/entry/1",
            Invalid,
        ),
        ("seed identity", "seed 0 %v0", "seed 0 %v1", Invalid),
        ("seed origin", "input c0", "definition c0", Invalid),
        (
            "root identity",
            "region f/entry/1",
            "region f/entry/0",
            Invalid,
        ),
        ("root value", "root %v2", "root %v1", Invalid),
        ("before/derived order", "source=before op=const", "source=derived op=const", Invalid),
        ("false congruence", "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0", "congruence 2 c2 c1 c1 n1 n0", Invalid),
        ("unused derived node", "extract 0", "node 3 n2 c3 source=derived op=const sort=bits<32> effect=pure attrs=value=1 children=-\nextract 0", Invalid),
        ("unreachable extraction", "root c0\n", "extract 1 c1 node:n0\nroot c0\n", Invalid),
    ] {
        assert!(baseline.contains(old), "{name}");
        let mutated = baseline.replacen(old, new, 1);
        assert_eq!(
            checker.check_typed(&before, &after, &mutated),
            expected,
            "{name}"
        );
    }
    for suffix in ["\n", "x"] {
        assert_eq!(
            checker.check_typed(&before, &after, &(baseline.clone() + suffix)),
            Malformed
        );
    }
    let output_change = after.replace("ret %v0", "ret %v1");
    assert_eq!(
        checker.check_typed(
            &before,
            &output_change,
            &proof(&before, &output_change, ADD_ZERO)
        ),
        Invalid
    );
    let input_change = before.replace("const.bits<32> 0", "const.bits<32> 1");
    assert_eq!(
        checker.check_typed(
            &input_change,
            &after,
            &proof(&input_change, &after, ADD_ZERO)
        ),
        Invalid
    );
    // Regions must remain a duplicate-free subsequence of module order.
    assert_eq!(
        checker.check_typed(
            &before,
            &after,
            &proof(&before, &after, &ADD_ZERO.repeat(2))
        ),
        Invalid
    );

    let budget = "budget nodes=2/100000 epochs=1/1024 fuel=0/256 n=2 stop=policy\n";
    let v2 = baseline.replacen("evp1\n", "evp2\n", 1).replacen(
        "region f/",
        &format!("{budget}region f/"),
        1,
    );
    assert_eq!(checker.check_typed(&before, &after, &v2), Valid);
    for (name, old, new, expected) in [
        ("missing budget", budget, "", Malformed),
        ("legacy budget", "evp2\n", "evp1\n", Malformed),
        ("zero epoch", "rewrite 2", "epoch 0\nrewrite 2", Malformed),
        (
            "repeated epoch",
            "rewrite 2",
            "epoch 1\nepoch 1\nrewrite 2",
            Malformed,
        ),
        (
            "decreasing epoch",
            "rewrite 2",
            "epoch 2\nepoch 1\nrewrite 2",
            Malformed,
        ),
        ("excess epoch", "rewrite 2", "epoch 2\nrewrite 2", Invalid),
        ("node ceiling", "2/100000", "2/99999", Invalid),
        ("epoch ceiling", "1/1024", "1/1023", Invalid),
        ("node overrun", "2/100000", "100001/100000", Invalid),
        ("epoch overrun", "1/1024", "1025/1024", Invalid),
        ("fuel overrun", "0/256", "257/256", Invalid),
        ("fuel bound", "0/256", "0/255", Invalid),
        ("node stop", "stop=policy", "stop=nodes", Invalid),
        ("epoch stop", "stop=policy", "stop=epochs", Invalid),
        ("fuel stop", "stop=policy", "stop=fuel", Invalid),
        (
            "premise lengths",
            "bind ?x:c0",
            "bind ?x:c0 premises c0:f:00",
            Malformed,
        ),
        (
            "unmasked value",
            "bind ?x:c0",
            "bind ?x:c0 premises c0:0:1",
            Malformed,
        ),
        (
            "unsupported premise",
            "bind ?x:c0",
            "bind ?x:c0 premises c0:ffffffff:00000000",
            Invalid,
        ),
    ] {
        assert!(v2.contains(old), "{name}");
        assert_eq!(
            checker.check_typed(&before, &after, &v2.replacen(old, new, 1)),
            expected,
            "{name}"
        );
    }
    for region in [
        ADD_ZERO.replace("rewrite 2", "epoch 1\nrewrite 2"),
        ADD_ZERO.replace("bind ?x:c0", "bind ?x:c0 premises c0:00000000:00000000"),
    ] {
        assert_eq!(
            checker.check_typed(&before, &after, &proof(&before, &after, &region)),
            Malformed
        );
    }
}
