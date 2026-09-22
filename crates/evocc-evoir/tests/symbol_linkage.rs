use evocc_evoir::{parse_section9_module, render_module};

fn function(name: &str, marks: &str) -> String {
    format!("func @{name}(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default{marks}\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: mem<all>, %v1: world):\n    %v2 = const.bits<32> 7\n    ret %v2, %v0, %v1\n}}\n")
}

#[test]
fn function_section_is_canonical_validated_metadata() {
    let text = function(
        "boot",
        " align=64 patchable_entry=4,2 section=\".init.text\"",
    );
    let module =
        parse_section9_module(text.as_bytes()).expect("function placement must survive IR");
    assert_eq!(render_module(&module), text);
    for section in [
        "\"\"",
        "\"bad\\x00name\"",
        ".init.text",
        "\".init.text\" section=\".text\"",
    ] {
        let malformed = text.replace("\".init.text\"", section);
        assert!(
            parse_section9_module(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn weak_marks_round_trip_on_each_module_record() {
    let text = format!("global @value origin=0 kind=file visibility=external linkage=weak size=4 align=4 bytes=07000000 initialized=1111\n\nextern @outside size=unknown align=4 linkage=weak\n\ndeclare @missing(mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default linkage=weak\n\n{}", function("fallback", " linkage=weak"));
    let module =
        parse_section9_module(text.as_bytes()).expect("weak linkage is a module-record fact");
    assert_eq!(render_module(&module), text);
}

#[test]
fn hidden_symbol_metadata_round_trips_without_changing_linkage_strength() {
    let text = format!("global @value origin=0 kind=file visibility=hidden linkage=weak size=4 align=4 bytes=07000000 initialized=1111\n\nextern @outside size=unknown align=4 visibility=hidden\n\ndeclare @missing(mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default visibility=hidden\n\nalias @other = @value visibility=hidden\n\n{}", function("implementation", " visibility=hidden"));
    let module = parse_section9_module(text.as_bytes()).expect("hidden is not internal linkage");
    assert_eq!(render_module(&module), text);
}

#[test]
fn protected_visibility_is_preserved_on_every_symbol_record() {
    let text = format!("global @value origin=0 kind=file visibility=protected linkage=weak size=4 align=4 bytes=07000000 initialized=1111\n\nextern @outside size=unknown align=4 visibility=protected\n\ndeclare @missing(mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default visibility=protected\n\nalias @other = @value visibility=protected\n\n{}", function("implementation", " visibility=protected"));
    let module = parse_section9_module(text.as_bytes())
        .expect("protected is external linkage with component-local binding");
    assert_eq!(render_module(&module), text);
    for other in ["hidden", "external", "internal"] {
        let changed = text.replacen("visibility=protected", &format!("visibility={other}"), 1);
        assert_ne!(render_module(&module), changed);
    }
}

#[test]
fn object_section_metadata_round_trips_with_initializers_and_relocations() {
    let text = "global @counter origin=0 kind=file visibility=external size=4 align=64 bytes=00000000 initialized=1111 section=\".data..percpu\"\nglobal @reference origin=1 kind=file visibility=external size=8 align=8 bytes=0000000000000000 initialized=11111111 section=\".data.rel.ro.refs\" relocs=[(0, @counter, 0)]\n";
    let module = parse_section9_module(text.as_bytes()).expect("object placement survives the IR");
    assert_eq!(render_module(&module), text);
    for invalid in [
        "\"\"",
        "\"bad\\x00name\"",
        ".data",
        "\".data\" section=\".other\"",
    ] {
        let malformed = text.replace("\".data..percpu\"", invalid);
        assert!(
            parse_section9_module(malformed.as_bytes()).is_err(),
            "{malformed}"
        );
    }
}

#[test]
fn alias_records_name_definitions_without_allocating_another_object() {
    let text = format!("global @value origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\n\nalias @other = @value linkage=weak\nalias @alternative = @implementation\n\n{}", function("implementation", " visibility=internal"));
    let module =
        parse_section9_module(text.as_bytes()).expect("aliases may name objects or functions");
    assert_eq!(
        module.globals.len(),
        1,
        "the alias has no allocation or bytes"
    );
    assert_eq!(
        module.functions.len(),
        1,
        "the alias has no forwarding body"
    );
    assert_eq!(render_module(&module), text);
}

#[test]
fn aliases_supply_direct_call_signatures_and_effects() {
    let text = format!("alias @alternative = @implementation\n\n{}\nfunc @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default\n    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }}\n{{\n^entry(%v0: mem<all>, %v1: world):\n    (%v2, %v3, %v4) = call @alternative(%v0, %v1)\n    ret %v2, %v3, %v4\n}}\n", function("implementation", " visibility=internal"));
    let module =
        parse_section9_module(text.as_bytes()).expect("the alias has its target's signature");
    let args = module.functions[1]
        .params
        .iter()
        .map(|p| evocc_evoir::RuntimeValue::new(p.ty.clone(), 0))
        .collect::<Vec<_>>();
    let result = evocc_evoir::interpret_module(
        &module,
        "main",
        &args,
        evocc_evoir::InterpreterLimits::new(100),
    )
    .unwrap();
    assert_eq!(result.return_values[0].raw_bits, 7);
    assert_eq!(render_module(&module), text);
}

#[test]
fn weak_internal_symbols_are_e3060() {
    for text in [
        function("hidden", " visibility=internal linkage=weak"),
        "global @hidden origin=0 kind=file visibility=internal linkage=weak size=4 align=4 bytes=00000000 initialized=1111\n".to_owned(),
        format!("alias @hidden = @value visibility=internal linkage=weak\n\n{}", function("value", "")),
    ] {
        let error = parse_section9_module(text.as_bytes()).unwrap_err();
        assert!(error.contains("E3060"), "{error}");
    }
}

#[test]
fn alias_targets_must_be_definitions_not_other_aliases_or_externals() {
    for text in [
        "alias @a = @missing\n".to_owned(),
        "extern @x size=4 align=4\n\nalias @a = @x\n".to_owned(),
        "declare @x(mem<all>, world) -> (mem<all>, world) cc = target_default\n\nalias @a = @x\n"
            .to_owned(),
        format!("alias @a = @x\nalias @b = @a\n\n{}", function("x", "")),
    ] {
        let error = parse_section9_module(text.as_bytes()).unwrap_err();
        assert!(error.contains("E3061"), "{error}");
    }
}
