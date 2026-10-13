use evocc_proof::{parse_evp1, render_evp1, EvpEvent, EvpExtractionChoice, EvpSeedOrigin};

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const REGISTRY_SHA256: &str = "ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9";

fn zero_certificate() -> String {
    format!(
        "evp1\nregistry p0.scalar.v1 {REGISTRY_SHA256}\ninput {EMPTY_SHA256}\noutput {EMPTY_SHA256}\n"
    )
}

fn add_zero_certificate() -> String {
    format!(
        concat!(
            "evp1\n",
            "registry p0.scalar.v1 {REGISTRY_SHA256}\n",
            "input {EMPTY_SHA256}\n",
            "region main/entry/1 root %v2 witnesses 2 seeds 1\n",
            "witness 0 main/entry/0\n",
            "witness 1 main/entry/1\n",
            "seed 0 %v0 bits<32> input c0\n",
            "node 0 n0 c1 source=before op=const sort=bits<32> effect=pure attrs=value=0 children=-\n",
            "node 1 n1 c2 source=before op=add sort=bits<32> effect=pure attrs=- children=c0,c1\n",
            "rewrite 2 p0_001_add_zero_rhs c2 c0 c0 node:n1 class:c0 bind ?x:c0\n",
            "extract 0 c0 seed:0\n",
            "root c0\n",
            "endregion\n",
            "output {EMPTY_SHA256}\n",
        ),
        REGISTRY_SHA256 = REGISTRY_SHA256,
        EMPTY_SHA256 = EMPTY_SHA256,
    )
}

#[test]
fn parses_and_renders_zero_region_certificate_canonically() {
    let bytes = zero_certificate();
    let certificate = parse_evp1(bytes.as_bytes()).expect("canonical zero-region EVP1");
    assert!(certificate.regions.is_empty());
    assert_eq!(render_evp1(&certificate), bytes);
}

#[test]
fn parses_and_renders_nonempty_certificate_canonically() {
    let bytes = add_zero_certificate();
    let certificate = parse_evp1(bytes.as_bytes()).expect("canonical nonempty EVP1");
    let region = &certificate.regions[0];
    assert_eq!(region.id.function, "main");
    assert_eq!(region.root_value, 2);
    assert_eq!(region.witnesses.len(), 2);
    assert_eq!(region.seeds[0].origin, EvpSeedOrigin::Input);
    assert!(matches!(region.events[0], EvpEvent::Node(_)));
    assert!(matches!(region.events[2], EvpEvent::Rewrite(_)));
    assert_eq!(region.extractions[0].choice, EvpExtractionChoice::Seed(0));
    assert_eq!(render_evp1(&certificate), bytes);
}

#[test]
fn rejects_noncanonical_or_structurally_malformed_evp1() {
    let valid = add_zero_certificate();
    let mutations = [
        valid.replace("witnesses 2", "witnesses 3"),
        valid.replace("witness 1", "witness 2"),
        valid.replace("bind ?x:c0", "bind ?z:c0,?a:c0"),
        valid.replace("bits<32> input", "bits<0> input"),
        valid.replace("node 1 n1", "node 2 n1"),
        format!("{valid}\n"),
    ];
    for mutation in mutations {
        assert!(
            parse_evp1(mutation.as_bytes()).is_err(),
            "mutation must be rejected:\n{mutation}"
        );
    }
}

/// A sealed `evp2` certificate whose first region opens with `epoch 1`.
fn sealed_evp2() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/certificates/tier-b-known-bits/proof.evp"
    ))
    .expect("read the sealed tier-b certificate")
}

#[test]
fn an_epoch_marker_zero_is_malformed() {
    // Section 11.3 numbers the epochs from 1. The certificate below differs
    // from a valid one only in its first marker.
    let valid = sealed_evp2();
    assert!(valid.starts_with("evp2\n"));
    let certificate = parse_evp1(valid.as_bytes()).expect("the sealed certificate is valid");
    assert_eq!(render_evp1(&certificate), valid);
    let marker = valid
        .find("\nepoch 1\n")
        .expect("the first region has an epoch 1");
    let mut zero = valid.clone();
    zero.replace_range(marker..marker + "\nepoch 1\n".len(), "\nepoch 0\n");
    let error = parse_evp1(zero.as_bytes()).expect_err("epoch 0 is malformed");
    assert!(error.to_string().contains("numbered from 1"), "{error}");
}
