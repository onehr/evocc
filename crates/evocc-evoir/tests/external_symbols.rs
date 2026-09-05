use evocc_evoir::{parse_section9_module, render_module};

#[test]
fn external_tables_round_trip_without_definitions() {
    for source in [
        "extern @counter size=4 align=4\n",
        "extern @array size=unknown align=4\n",
        "declare @consume(bits<32>, mem<all>, world) -> (mem<all>, world) cc = target_default\n",
        "extern @counter size=4 align=4\n\ndeclare @consume(bits<32>, mem<all>, world) -> (mem<all>, world) cc = target_default\n",
    ] {
        let module = parse_section9_module(source.as_bytes()).expect("external symbol tables");
        assert!(module.functions.is_empty());
        assert_eq!(render_module(&module), source);
    }
}

#[test]
fn external_symbols_cannot_be_declared_twice_or_as_both_kinds() {
    let valid = "extern @counter size=4 align=4\n";
    parse_section9_module(valid.as_bytes()).expect("valid external object control");
    for source in [
        format!("{valid}{valid}"),
        format!(
            "{valid}\ndeclare @counter(mem<all>, world) -> (mem<all>, world) cc = target_default\n"
        ),
    ] {
        let error = parse_section9_module(source.as_bytes()).expect_err("symbol collision");
        assert!(error.contains("E3041"), "{error}");
    }
}
