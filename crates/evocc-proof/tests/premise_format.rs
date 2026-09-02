//! The premise record of Section 11.3, as bytes.
//!
//! A premise is the only part of a rewrite line that is optional, so the two
//! field counts a rewrite may have are the first thing a reader has to get
//! right. The malformedness rules below are the specification's own sentences:
//! the two fields carry the class's width rounded up to a whole hexadecimal
//! digit, and a value bit outside the mask means nothing.

use evocc_proof::{parse_evp1, render_evp1, EvpEvent, EvpPremise};

/// A minimal `evp2` certificate with one rewrite, rendered around whatever
/// premise suffix the caller wants to test.
fn certificate(premises: &str) -> String {
    format!(
        "evp2\n\
         registry p0.scalar.v2 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce\n\
         input {digest}\n\
         budget nodes=3/100000 epochs=1/1024 fuel=1/256 n=2 stop=saturated\n\
         region main/entry/2 root %v2 witnesses 3 seeds 0\n\
         witness 0 main/entry/0\n\
         witness 1 main/entry/1\n\
         witness 2 main/entry/2\n\
         epoch 1\n\
         node 0 n0 c0 source=before op=const sort=bits<8> effect=pure attrs=value=12 children=-\n\
         node 1 n1 c1 source=before op=const sort=bits<8> effect=pure attrs=value=10 children=-\n\
         node 2 n2 c2 source=before op=and sort=bits<8> effect=pure attrs=- children=c0,c1\n\
         node 3 n3 c3 source=derived op=const sort=bits<8> effect=pure attrs=value=8 children=-\n\
         rewrite 4 p1_200_known_and c2 c3 c2 node:n2 node:n3 bind ?a:c0,?b:c1{premises}\n\
         extract 0 c2 node:n3\n\
         root c2\n\
         endregion\n\
         output {digest}\n",
        digest = "0".repeat(64),
        premises = premises
    )
}

#[test]
fn a_rewrite_without_premises_round_trips_as_before() {
    let bytes = certificate("");
    let parsed = parse_evp1(bytes.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{bytes}"));
    assert_eq!(render_evp1(&parsed), bytes);
    let rewrite = parsed.regions[0]
        .events
        .iter()
        .find_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite),
            _ => None,
        })
        .expect("the region carries one rewrite");
    assert!(rewrite.premises.is_empty());
}

#[test]
fn a_premise_round_trips_with_its_digit_count() {
    let bytes = certificate(" premises c0:ff:0c,c1:ff:0a");
    let parsed = parse_evp1(bytes.as_bytes()).unwrap_or_else(|e| panic!("{e}\n{bytes}"));
    assert_eq!(render_evp1(&parsed), bytes, "premises must render back");
    let rewrite = parsed.regions[0]
        .events
        .iter()
        .find_map(|event| match event {
            EvpEvent::Rewrite(rewrite) => Some(rewrite),
            _ => None,
        })
        .expect("the region carries one rewrite");
    assert_eq!(
        rewrite.premises,
        vec![
            EvpPremise {
                class: 0,
                mask: 0xff,
                value: 0x0c,
                digits: 2,
            },
            EvpPremise {
                class: 1,
                mask: 0xff,
                value: 0x0a,
                digits: 2,
            },
        ]
    );
}

#[test]
fn premise_fields_of_different_lengths_are_malformed() {
    assert!(
        parse_evp1(certificate(" premises c0:ff:c").as_bytes()).is_err(),
        "a mask and value of different digit counts is not a premise"
    );
    assert!(
        parse_evp1(certificate(" premises c0:f:0c").as_bytes()).is_err(),
        "and the same the other way round"
    );
}

#[test]
fn a_value_bit_outside_the_mask_is_malformed() {
    assert!(
        parse_evp1(certificate(" premises c0:0f:1f").as_bytes()).is_err(),
        "a bit claimed in the value but not known in the mask means nothing"
    );
    assert!(
        parse_evp1(certificate(" premises c0:0f:0f").as_bytes()).is_ok(),
        "the same bits inside the mask are a fact"
    );
}

#[test]
fn a_premise_list_is_never_empty_and_is_lowercase_hex() {
    assert!(
        parse_evp1(certificate(" premises ").as_bytes()).is_err(),
        "an empty premise list is a suffix that says nothing"
    );
    assert!(
        parse_evp1(certificate(" premises c0:FF:0C").as_bytes()).is_err(),
        "digests and payloads are lowercase throughout the format"
    );
}

#[test]
fn evp1_carries_no_premises() {
    let downgraded = certificate(" premises c0:ff:0c")
        .replacen("evp2\n", "evp1\n", 1)
        .replacen(
            "p0.scalar.v2 c1691808ebe55a3b2520a0456345e7c38a23fa2732e0827393dcf12523024bce",
            "p0.scalar.v1 ebdf519184f7dd3eae0daea8b54342692ec98493e04bf506c6a1e963ed8648f9",
            1,
        );
    let downgraded = downgraded
        .lines()
        .filter(|line| !line.starts_with("budget ") && !line.starts_with("epoch "))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    assert!(
        parse_evp1(downgraded.as_bytes()).is_err(),
        "Section 11.3 gives evp1 no premises, so one is not a certificate it defines"
    );
}
