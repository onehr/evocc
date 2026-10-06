//! Every reviewed registry digest, reproduced from the specification text.
//!
//! Section 11.6 "Registries and revisions": a registry is admissible only when
//! the specification prints its rule block and digest, and Section 11.3
//! "Digests" fixes the recipe -- the SHA-256 of the printed bytes starting at
//! the first rule id and including the newline after the last rule. A digest
//! copied into the producer by hand is a digest nobody recomputed, so this
//! recomputes all four from the printed blocks and compares them to the
//! constants the producer and the checker carry.
//!
//! It also checks the containment Section 11.6 states: a revision includes
//! every rule of the registry it revises, unchanged and in the same order,
//! before its new rules. A revision that reordered an inherited rule would
//! keep its own digest and silently change what a rule id means.

use evocc_proof::sha256_hex;

/// The digests as Section 11.6 prints them.
const PRINTED: [(&str, &str); 4] = [
    (
        "p0.scalar.v1",
        "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9",
    ),
    (
        "p0.scalar.v2",
        "c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce",
    ),
    (
        "p0.scalar.v3",
        "aca1f15b47e602ce18c593e06d77008862947c7c7252a9f76bb823c71f53fb15",
    ),
    (
        "p0.scalar.v4",
        "1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74",
    ),
];

fn specification() -> String {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../spec/evoir.md");
    std::fs::read_to_string(path).expect("the specification is the sole authority and is present")
}

/// The rule block Section 11.6 prints under the heading naming `registry`.
///
/// The heading is `**Reviewed registry `NAME`.**` and the block is the first
/// fenced `text` block after it. Nothing is normalized: the recipe covers the
/// exact bytes.
fn printed_block(specification: &str, registry: &str) -> String {
    let heading = format!("**Reviewed registry `{registry}`");
    let (_, tail) = specification
        .split_once(&heading)
        .unwrap_or_else(|| panic!("the specification prints no registry {registry}"));
    let (_, body) = tail
        .split_once("```text\n")
        .expect("a printed registry is followed by a fenced text block");
    let (block, _) = body
        .split_once("```")
        .expect("a fenced block is closed by a fence");
    block.to_owned()
}

fn rules(block: &str) -> Vec<String> {
    block.lines().map(str::to_owned).collect()
}

#[test]
fn every_printed_registry_digest_reproduces_from_the_specification() {
    let specification = specification();
    for (registry, digest) in PRINTED {
        let block = printed_block(&specification, registry);
        assert!(
            block.starts_with("p0_001_add_zero_rhs "),
            "{registry} must start at the first rule id, not at {:?}",
            block.chars().take(24).collect::<String>()
        );
        assert!(
            block.ends_with('\n'),
            "{registry} must include the newline after the last rule"
        );
        let computed = sha256_hex(block.as_bytes());
        println!("{registry}={computed}");
        assert_eq!(
            computed, digest,
            "{registry} does not reproduce from the printed recipe"
        );
    }
}

#[test]
fn each_revision_contains_its_predecessor_unchanged_and_in_order() {
    let specification = specification();
    let blocks = PRINTED
        .iter()
        .map(|(registry, _)| rules(&printed_block(&specification, registry)))
        .collect::<Vec<_>>();
    for window in blocks.windows(2) {
        let [earlier, later] = window else {
            unreachable!("windows(2) yields pairs")
        };
        assert!(
            later.len() > earlier.len(),
            "a revision adds rules: {} then {}",
            earlier.len(),
            later.len()
        );
        assert_eq!(
            &later[..earlier.len()],
            earlier.as_slice(),
            "a revision keeps every inherited rule unchanged and in the same order"
        );
    }
    let widest = blocks.last().expect("four registries were read");
    for rule in [
        "p2_300_mul_two_rhs mul(?x,const_like(?x,2)) => add(?x,?x) when width_min(?x,2)",
        "p2_301_mul_two_lhs mul(const_like(?x,2),?x) => add(?x,?x) when width_min(?x,2)",
        "p2_302_add_self_shl add(?x,?x) => shl(?x,const_like(?x,1)) when width_min(?x,2)",
        "p2_303_shl_one_add shl(?x,const_like(?x,1)) => add(?x,?x) when width_min(?x,2)",
    ] {
        assert!(
            widest.iter().any(|printed| printed == rule),
            "the widest registry must print {rule}"
        );
    }
}
