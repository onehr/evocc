use evocc_eqopt::EqualityState;
use evocc_evoir::parse_section9_module;

#[test]
fn conversions_with_different_result_sorts_never_share_a_class() {
    for operator in ["zext", "sext"] {
        let text = format!(
            "func @f(%arg0: bits<8>) -> (bits<64>) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<8>):
    %v1 = {operator}.bits<32> %v0
    %v2 = {operator}.bits<64> %v0
    %v3 = {operator}.bits<32> %v0
    ret %v2
}}
"
        );
        let module = parse_section9_module(text.as_bytes()).unwrap();
        let mut state = EqualityState::for_module(&module).unwrap();
        state.ingest_module(&module).unwrap();
        assert_ne!(state.class_of("f", 1), state.class_of("f", 2));
        assert_eq!(state.class_of("f", 1), state.class_of("f", 3));
        state.verify_history().unwrap();
    }
}

#[test]
fn non_scalar_block_arguments_do_not_enter_persistent_p0() {
    for ty in ["ptr<generic>", "mem<all>", "world", "bits<128>", "f64"] {
        let text = format!(
            "func @f(%arg0: {ty}) -> ({ty}) cc = target_default
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: {ty}):
    ret %v0
}}
"
        );
        let module = parse_section9_module(text.as_bytes()).unwrap();
        let mut state = EqualityState::for_module(&module).unwrap();
        state.ingest_module(&module).unwrap();
        assert!(state.class_of("f", 0).is_none(), "{ty}");
    }
}
