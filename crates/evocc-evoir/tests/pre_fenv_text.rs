use evocc_evoir::{
    parse_pre_fenv_module, parse_section9_module, render_module, render_pre_fenv_module,
};

const OLD: &[u8] =
    include_bytes!("../../../tests/fixtures/certificates/shift-and-width-facts/before.evo");

#[test]
fn historical_text_requires_an_explicit_profile_and_retains_its_exact_bytes() {
    assert!(parse_section9_module(OLD).is_err());
    let module = parse_pre_fenv_module(OLD).unwrap();
    assert_eq!(render_pre_fenv_module(&module).unwrap().as_bytes(), OLD);
    let current = render_module(&module);
    assert!(current.contains(", fenv=false"));
    assert_eq!(parse_section9_module(current.as_bytes()).unwrap(), module);
    assert!(parse_pre_fenv_module(current.as_bytes()).is_err());
}

#[test]
fn historical_profile_cannot_erase_new_effects_or_fused_semantics() {
    let strict = include_str!("../../../tests/fixtures/floating-strict-world.evoir");
    let module = parse_section9_module(strict.as_bytes()).unwrap();
    assert!(render_pre_fenv_module(&module).is_err());
    assert!(parse_pre_fenv_module(strict.replace(", fenv=true", "").as_bytes()).is_err());
    let fused = strict
        .replace(
            "(%v4, %v5) = fp.strict.add %v3, %v0, %v1",
            "%v4 = fp.fma %v0, %v1, %v0",
        )
        .replace("fenv=true", "fenv=false")
        .replace("ret %v4, %v2, %v5", "ret %v4, %v2, %v3");
    assert!(render_pre_fenv_module(&parse_section9_module(fused.as_bytes()).unwrap()).is_err());
    assert!(parse_pre_fenv_module(fused.replace(", fenv=false", "").as_bytes()).is_err());
}
