use evocc_evoir::{parse_section9_module, render_module};

const ACCESS: &str = "func @take(%arg0: ptr<generic>, ..., %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default\n    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):\n    (%v3, %v4) = va_start %v0, %v1, %v2\n    (%v5, %v6, %v7) = va_arg.bits<32> %v0, %v3, %v4\n    (%v8, %v9) = va_end %v0, %v6, %v7\n    ret %v5, %v8, %v9\n}\n";

#[test]
fn variadic_access_has_canonical_text_and_linear_effects() {
    let module = parse_section9_module(ACCESS.as_bytes()).expect("variadic access syntax");
    assert_eq!(render_module(&module), ACCESS);
    assert!(module.functions[0].variadic);
    assert!(parse_section9_module(ACCESS.replace("bits<32>", "bits<8>").as_bytes()).is_err());
    let error = parse_section9_module(ACCESS.replace(", ...", "").as_bytes())
        .expect_err("E3037 validation");
    assert!(
        error.contains("E3037: va-start-outside-variadic-signature"),
        "{error}"
    );
    let stale = ACCESS.replace("va_end %v0, %v6, %v7", "va_end %v0, %v3, %v7");
    assert!(parse_section9_module(stale.as_bytes()).is_err());
}
