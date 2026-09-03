//! The corpus red line, enforced in code rather than in prose.
//!
//! The kernel cohort is an acceptance corpus and never enters the fitness of
//! any policy. The harness therefore refuses any corpus path under `.build/`,
//! under `.evidence/`, or equal to `tests/contracts/day5-kernel-cohort.json`,
//! and refuses any module whose canonical text digest a denylist names. These
//! tests are the pin: a harness that silently accepted one of these paths
//! would be measuring a policy against the corpus that is supposed to judge
//! it afterwards.

use std::fs;
use std::path::{Path, PathBuf};

use evocc_policy::{Corpus, CorpusRefusal};
use evocc_proof::sha256_hex;
use evocc_randgate::{generate_module, GeneratorConfig};

fn scratch(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("evocc-policy-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

const FORBIDDEN: [&str; 8] = [
    ".build/corpus",
    ".build",
    "./.build/nested/deep",
    ".evidence",
    ".evidence/day7",
    "./.evidence/x/y",
    "tests/contracts/day5-kernel-cohort.json",
    "./tests/contracts/day5-kernel-cohort.json",
];

#[test]
fn every_forbidden_corpus_path_is_refused() {
    for candidate in FORBIDDEN {
        let refusal = Corpus::admit_path(Path::new(candidate))
            .expect_err(&format!("{candidate} must be refused"));
        assert!(
            matches!(refusal, CorpusRefusal::ForbiddenPath { .. }),
            "{candidate} was refused for the wrong reason: {refusal:?}"
        );
        assert!(refusal
            .message()
            .contains(candidate.trim_start_matches("./")));
    }
}

#[test]
fn an_ordinary_corpus_path_is_admitted() {
    for candidate in [
        "tests/corpora/generated",
        "tests/contracts/day6-acceptance-receipt.json",
        "/tmp/evocc/build/corpus",
        "buildings/.buildx",
    ] {
        Corpus::admit_path(Path::new(candidate))
            .unwrap_or_else(|error| panic!("{candidate} must be admitted: {error:?}"));
    }
}

#[test]
fn loading_a_forbidden_directory_is_refused_before_anything_is_read() {
    let refusal =
        Corpus::from_directory(Path::new(".build/never-read"), None).expect_err("a refusal");
    assert!(matches!(refusal, CorpusRefusal::ForbiddenPath { .. }));
}

#[test]
fn a_denylisted_module_digest_is_refused() {
    let directory = scratch("denylist");
    let module = generate_module(11, GeneratorConfig::default());
    let text = evocc_evoir::render_module(&module);
    let digest = sha256_hex(text.as_bytes());
    fs::write(directory.join("000.evo"), &text).expect("write the module");

    let allowed = Corpus::from_directory(&directory, None).expect("no denylist admits the module");
    assert_eq!(allowed.programs().len(), 1);

    let denylist = directory.join("kernel-cohort-digests.json");
    fs::write(
        &denylist,
        format!("{{\n  \"digests\": [\n    \"{digest}\"\n  ]\n}}\n"),
    )
    .expect("write the denylist");
    let refusal = Corpus::from_directory(&directory, Some(&denylist)).expect_err("a refusal");
    match refusal {
        CorpusRefusal::DeniedDigest {
            digest: refused, ..
        } => assert_eq!(refused, digest),
        other => panic!("wrong refusal: {other:?}"),
    }
}

#[test]
fn an_absent_denylist_denies_nothing() {
    let directory = scratch("absent-denylist");
    let module = generate_module(12, GeneratorConfig::default());
    fs::write(
        directory.join("000.evo"),
        evocc_evoir::render_module(&module),
    )
    .expect("write the module");
    let denylist = directory.join("does-not-exist.json");
    assert!(Corpus::read_denylist(&denylist)
        .expect("an absent denylist reads clean")
        .is_empty());
    let corpus =
        Corpus::from_directory(&directory, Some(&denylist)).expect("an absent denylist admits");
    assert_eq!(corpus.programs().len(), 1);
}

#[test]
fn a_generated_corpus_digest_is_the_texts_in_order() {
    let corpus = Corpus::generated(100, 4, GeneratorConfig::default());
    assert_eq!(corpus.programs().len(), 4);
    let mut joined = String::new();
    for offset in 0..4 {
        joined.push_str(&evocc_evoir::render_module(&generate_module(
            100 + offset,
            GeneratorConfig::default(),
        )));
    }
    assert_eq!(corpus.digest(), sha256_hex(joined.as_bytes()));
    assert_eq!(
        Corpus::generated(100, 4, GeneratorConfig::default()).digest(),
        corpus.digest()
    );
}
