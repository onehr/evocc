use evocc_evoir::{parse_section9_module, render_module};

#[test]
fn relative_block_relocations_preserve_both_targets_and_storage_width() {
    let text = concat!(
        "global @table origin=0 kind=file visibility=internal size=4 align=4 bytes=00000000 initialized=1111 relocs=[(0, @f^b1, 0, diff=@f^b2, bits=32)]\n\n",
        "func @f() -> () cc = target_default\n",
        "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }\n",
        "{\n^entry():\n    br ^b1()\n\n^b1():\n    br ^b2()\n\n^b2():\n    ret\n}\n",
    );
    let module = parse_section9_module(text.as_bytes()).expect("relative label relocation");
    assert_eq!(render_module(&module), text);
    for invalid in [
        text.replace("diff=@f^b2", "diff=@f^b999"),
        text.replace("diff=@f^b2", "diff=@other^b2"),
        text.replace("@f^b1, 0", "@table, 0"),
        text.replace("bits=32", "bits=64"),
        text.replace("bits=32", "bits=24"),
        text.replace("bits=32", "bits=32, exposed"),
        text.replace("(0, @f", "(1, @f"),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

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
    let negative = text.replace("(0, @a, 0)", "(0, @a, -4)");
    assert_eq!(
        render_module(&parse_section9_module(negative.as_bytes()).unwrap()),
        negative
    );
    let exposed = text.replace("(0, @a, 0)", "(0, @a, 0, exposed)");
    let module = parse_section9_module(exposed.as_bytes()).unwrap();
    assert_eq!(render_module(&module), exposed);
    assert!(module.globals[1].relocs[0].exposed);
    for invalid in [
        text.replace("(0, @a, 0)", "(0, @a, 0, false)"),
        text.replace("(0, @a, 0)", "(0, @a, 0, exposed, exposed)"),
        text.replace("(0, @a, 0)", "(0, @a, -0)"),
        text.replace("(0, @a, 0)", "(0, @a, +1)"),
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

#[test]
fn symbolic_relocations_accept_byte_offsets_without_weakening_range_checks() {
    for offset in 0..8 {
        let size = offset + 8;
        let text = format!(
            "global @a origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\nglobal @p origin=1 kind=file visibility=internal size={size} align=1 bytes={} initialized={} relocs=[({offset}, @a, 0)]\n",
            "00".repeat(size),
            "1".repeat(size),
        );
        let module = parse_section9_module(text.as_bytes())
            .unwrap_or_else(|error| panic!("byte offset {offset}: {error:?}"));
        assert_eq!(render_module(&module), text);
    }

    let text = "global @a origin=0 kind=file visibility=external size=4 align=4 bytes=07000000 initialized=1111\nglobal @p origin=1 kind=file visibility=internal size=20 align=1 bytes=0102000000000000000003040000000000000000 initialized=11111111111111111111 relocs=[(2, @a, 0), (12, @a, 0)]\n";
    assert_eq!(
        render_module(&parse_section9_module(text.as_bytes()).unwrap()),
        text
    );
    for invalid in [
        text.replace("(12, @a, 0)", "(13, @a, 0)"),
        text.replace("(12, @a, 0)", "(18446744073709551615, @a, 0)"),
        text.replace("(12, @a, 0)", "(9, @a, 0)"),
        text.replace("(2, @a, 0), (12, @a, 0)", "(12, @a, 0), (2, @a, 0)"),
        text.replace("(12, @a, 0)", "(12, @absent, 0)"),
        text.replace("bytes=010200", "bytes=010201"),
        text.replace(
            "initialized=11111111111111111111",
            "initialized=11011111111111111111",
        ),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}
