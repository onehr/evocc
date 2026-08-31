use std::time::{Duration, Instant};

use evocc_chir::SymbolTable;
use evocc_common::{FileId, SourceSpan};
use evocc_types::{TagKind, TypeInterner};

fn span(ordinal: usize) -> SourceSpan {
    let start = u32::try_from(ordinal).expect("scaling fixture fits source offsets");
    SourceSpan::new(FileId::new(97_002), start, start + 1, 1, start + 1)
}

fn tag_workload(count: usize) -> Duration {
    let mut types = TypeInterner::new();
    let names = (0..count)
        .map(|ordinal| format!("record_{ordinal}"))
        .collect::<Vec<_>>();
    let tags = names
        .iter()
        .map(|name| types.declare_tag(TagKind::Struct, Some(name.clone())))
        .collect::<Vec<_>>();

    let mut symbols = SymbolTable::new();
    let file = symbols.file_scope();
    let started = Instant::now();
    for (ordinal, (name, tag)) in names.iter().zip(tags.iter().copied()).enumerate() {
        symbols
            .declare_tag(file, name, tag, span(ordinal))
            .expect("fixture tag names are unique");
    }
    for (name, expected) in names.iter().zip(tags.iter().copied()) {
        assert_eq!(symbols.resolve_tag(file, name).unwrap(), Some(expected));
    }
    started.elapsed()
}

fn best_of_three(count: usize) -> Duration {
    (0..3)
        .map(|_| tag_workload(count))
        .min()
        .expect("three samples are non-empty")
}

#[test]
fn tag_declaration_and_resolution_scale_near_linearly() {
    let _ = tag_workload(32);
    let small = best_of_three(128);
    let large = best_of_three(512);
    eprintln!("tag-lookup-elapsed: small={small:?} large={large:?}");

    assert!(
        large <= small.saturating_mul(12),
        "quadrupling tags took {large:?} versus {small:?}; tag lookup must not rescan the global binding list"
    );
}
