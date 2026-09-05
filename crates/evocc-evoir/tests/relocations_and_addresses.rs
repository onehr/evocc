use evocc_evoir::{parse_section9_module, render_module};

#[test]
fn pointer_integer_forms_have_exact_operand_and_result_types() {
    let text = "func @f(%arg0: ptr<generic>) -> (ptr<generic>) cc = target_default\n    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n{\n^entry(%v0: ptr<generic>):\n    %v1 = ptr.to_int %v0\n    %v2 = ptr.from_int.ptr<generic> %v1\n    ret %v2\n}\n";
    let module = parse_section9_module(text.as_bytes()).expect("pointer conversion syntax");
    assert_eq!(render_module(&module), text);
    for invalid in [
        text.replace("ptr.to_int %v0", "ptr.to_int %v1"),
        text.replace("ptr.from_int.ptr<generic>", "ptr.from_int.bits<64>"),
        text.replace(
            "ptr.from_int.ptr<generic> %v1",
            "ptr.from_int.ptr<generic> %v0",
        ),
    ] {
        assert!(parse_section9_module(invalid.as_bytes()).is_err());
    }
}

#[test]
fn symbolic_relocations_round_trip_and_validate_their_byte_ranges() {
    let text = "global @a origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\nglobal @p origin=1 kind=file visibility=internal size=8 align=8 bytes=0000000000000000 initialized=11111111 relocs=[(0, @a, 0)]\n";
    let module = parse_section9_module(text.as_bytes()).expect("symbolic relocation syntax");
    assert_eq!(render_module(&module), text);
    for invalid in [
        text.replace("(0, @a, 0)", "(1, @a, 0)"),
        text.replace("(0, @a, 0)", "(8, @a, 0)"),
        text.replace("(0, @a, 0)", "(0, @absent, 0)"),
        text.replace("(0, @a, 0)", "(0, @a, 0), (0, @a, 0)"),
        text.replace("initialized=11111111", "initialized=01111111"),
        text.replace("bytes=0000000000000000", "bytes=0100000000000000"),
    ] {
        assert!(parse_section9_module(invalid.as_bytes()).is_err());
    }
}
