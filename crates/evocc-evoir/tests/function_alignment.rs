use evocc_evoir::{parse_section9_module, render_module, validate_ssa};

const UNIT: &str = "\
func @f(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 7
    ret %v2, %v0, %v1
}
";

#[test]
fn function_alignment_round_trips_and_default_headers_are_unchanged() {
    let ordinary = parse_section9_module(UNIT.as_bytes()).unwrap();
    assert_eq!(render_module(&ordinary), UNIT);
    let aligned = UNIT.replace("visibility=internal", "visibility=internal align=256");
    let module = parse_section9_module(aligned.as_bytes()).expect("aligned function header");
    assert_eq!(render_module(&module), aligned);
}

#[test]
fn function_alignment_requires_a_power_of_two_at_least_four() {
    let mut module = parse_section9_module(UNIT.as_bytes()).unwrap();
    for alignment in [0, 1, 2, 3, 12] {
        let text = UNIT.replace(
            "visibility=internal",
            &format!("visibility=internal align={alignment}"),
        );
        assert!(parse_section9_module(text.as_bytes()).is_err(), "{text}");
        module.functions[0].alignment = alignment;
        assert!(validate_ssa(&module.functions[0]).is_err());
    }
}
