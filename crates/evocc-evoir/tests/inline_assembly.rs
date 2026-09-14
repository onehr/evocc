use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, InterpreterLimits, RuntimeValue,
};
use evocc_types::{EvoIRType, Region};

const EDGES: &str = r#"func @branching(%arg0: bits<32>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=false, io=true, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    (%v4, %v5, %v6) = asm.goto "cbnz %w1, %l[taken]\x0a\x09add %w0, %w0, #5" %v2, %v3 outputs [("+r", "value", bits<32>, %v0)] inputs [("r", "branch", %v1)] clobbers [] labels [("taken", ^b2(%v4, %v5, %v6))] fallthrough ^b1(%v4, %v5, %v6)

^b1(%v7: bits<32>, %v8: mem<all>, %v9: world):
    ret %v7, %v8, %v9

^b2(%v10: bits<32>, %v11: mem<all>, %v12: world):
    ret %v10, %v11, %v12
}
"#;

fn ordinary(template: &str) -> String {
    format!(
        concat!(
            "func @main(%arg0: mem<all>, %arg1: world) -> (mem<all>, world) cc = target_default\n",
            "    effects {{ reads={{all}}, writes={{all}}, volatile=true, atomic=false, io=true, may_trap=true, may_unwind=false }}\n",
            "{{\n^entry(%v0: mem<all>, %v1: world):\n",
            "    (%v2, %v3) = asm {template} %v0, %v1 outputs [] inputs [] clobbers [\"memory\"]\n",
            "    ret %v2, %v3\n}}\n"
        ),
        template = template,
    )
}

fn tokens() -> Vec<RuntimeValue> {
    vec![
        RuntimeValue::new(EvoIRType::Mem(Region::All), 0),
        RuntimeValue::new(EvoIRType::World, 0),
    ]
}

#[test]
fn vector_register_clobbers_keep_their_own_register_file() {
    let bound = EDGES
        .replace("\"+r\"", "\"+r@x8\"")
        .replace("clobbers []", "clobbers [\"v8\"]");
    let module = parse_section9_module(bound.as_bytes()).expect("v8 does not clobber x8");
    assert_eq!(render_module(&module), bound);
    for register in ["v0", "v8", "v15", "v31"] {
        let source = ordinary(r#""nop""#).replace("[\"memory\"]", &format!("[\"{register}\"]"));
        let module = parse_section9_module(source.as_bytes()).expect("physical SIMD clobber");
        assert_eq!(render_module(&module), source);
    }
    for register in ["v32", "v255", "v-1"] {
        let source = ordinary(r#""nop""#).replace("[\"memory\"]", &format!("[\"{register}\"]"));
        assert!(parse_section9_module(source.as_bytes()).is_err());
    }
}

#[test]
fn numbered_labels_count_explicit_operands_and_round_trip() {
    // A read-write output remains one IR operand. Its incoming value is
    // not an extra explicit input; the first label here has index two.
    let source = EDGES.replace("%l[taken]", "%l2");
    let module = parse_section9_module(source.as_bytes()).expect("numbered IR label");
    assert_eq!(render_module(&module), source);
    for invalid in ["%l0", "%l1", "%l3", "%l999999999999999999999999"] {
        assert!(parse_section9_module(EDGES.replace("%l[taken]", invalid).as_bytes()).is_err());
    }
    let inputs = (0..10)
        .map(|n| format!("(\"r\", \"input{n}\", %v1)"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = EDGES
        .replace("%l[taken]", "%l11")
        .replace("(\"r\", \"branch\", %v1)", &inputs);
    let module = parse_section9_module(source.as_bytes()).expect("multi-digit label");
    assert_eq!(render_module(&module), source);
}

#[test]
fn address_references_round_trip_and_validate_the_named_input_type() {
    for ty in ["ptr<generic>", "bits<64>"] {
        for constraint in ["r", "p"] {
            for reference in ["%a0", "%a[address]"] {
                let source = format!(concat!(
                    "func @hint(%arg0: {ty}, %arg1: mem<all>, %arg2: world) -> (mem<all>, world) cc = target_default\n",
                    "    effects {{ reads={{all}}, writes={{all}}, volatile=true, atomic=false, io=true, may_trap=true, may_unwind=false }}\n",
                    "{{\n^entry(%v0: {ty}, %v1: mem<all>, %v2: world):\n",
                    "    (%v3, %v4) = asm \"prfm pldl1keep, {reference}\" %v1, %v2 outputs [] inputs [(\"{constraint}\", \"address\", %v0)] clobbers []\n",
                    "    ret %v3, %v4\n}}\n"
                ), ty=ty, constraint=constraint, reference=reference);
                let module = parse_section9_module(source.as_bytes()).expect("address input");
                assert_eq!(render_module(&module), source);
                for bad in ["bits<1>", "bits<32>", "bits<128>"] {
                    let invalid = source.replace(ty, bad);
                    assert!(
                        parse_section9_module(invalid.as_bytes()).is_err(),
                        "{invalid}"
                    );
                }
            }
        }
    }
}

#[test]
fn symbolic_address_operands_and_constant_modifiers_preserve_opaque_effects() {
    let source = concat!(
        "global @object origin=0 kind=file visibility=internal size=4 align=4 bytes=00000000 initialized=1111\n\n",
        "func @main(%arg0: mem<all>, %arg1: world) -> (mem<all>, world) cc = target_default\n",
        "    effects { reads={all}, writes={all}, volatile=true, atomic=false, io=true, may_trap=true, may_unwind=false }\n",
        "{\n^entry(%v0: mem<all>, %v1: world):\n",
        "    %v2 = global_addr @object : ptr<generic>\n",
        "    (%v3, %v4) = asm \"// %c[address]\" %v0, %v1 outputs [] inputs [(\"i\", \"address\", %v2)] clobbers []\n",
        "    ret %v3, %v4\n}\n",
    );
    let module = parse_section9_module(source.as_bytes()).expect("symbolic i input");
    assert_eq!(render_module(&module), source);
    let error =
        interpret_module(&module, "main", &tokens(), InterpreterLimits::new(100)).unwrap_err();
    assert_eq!(error.code(), "E3021");
    for invalid in [
        source.replace("\"i\"", "\"n\""),
        source.replace("\"i\"", "\"r\""),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn opaque_assembly_has_canonical_lossless_text_and_exact_effects() {
    for template in [
        r#""nop""#,
        r#""; [outputs] \"quote\" \\ slash\x0a\x09%%\xc3\xa9""#,
    ] {
        let text = ordinary(template);
        let module = parse_section9_module(text.as_bytes()).expect("admitted opaque assembly");
        assert_eq!(render_module(&module), text);
        let effects = &module.functions[0].effects;
        assert!(effects.reads_all && effects.writes_all && effects.volatile && effects.io);
        assert!(effects.may_trap);
        assert!(!effects.atomic && !effects.may_unwind);
        let error = interpret_module(&module, "main", &tokens(), InterpreterLimits::new(100))
            .expect_err("an interpreter must not pretend to execute assembly");
        assert_eq!(error.code(), "E3021");
    }
}

#[test]
fn assembly_outputs_bind_on_every_successor_without_self_use() {
    let module = parse_section9_module(EDGES.as_bytes()).expect("both successors carry outputs");
    assert_eq!(render_module(&module), EDGES);
    for condition in [0, 1] {
        let mut arguments = vec![RuntimeValue::bits(32, 7), RuntimeValue::bits(32, condition)];
        arguments.extend(tokens());
        let error = interpret_module(
            &module,
            "branching",
            &arguments,
            InterpreterLimits::new(100),
        )
        .expect_err("both opaque branch outcomes must fail closed");
        assert_eq!(error.code(), "E3021");
    }
}

#[test]
fn fixed_register_metadata_rejects_noncanonical_reserved_and_clobbered_bindings() {
    let valid = EDGES.replace("\"+r\"", "\"+r@x8\"");
    let module = parse_section9_module(valid.as_bytes()).unwrap();
    assert_eq!(render_module(&module), valid);
    for binding in [
        "+r@w8", "+r@x08", "+r@x16", "+r@x17", "+r@x18", "+r@x29", "+r@sp", "+Q@x8", "+r@x8@x9",
    ] {
        let invalid = valid.replace("+r@x8", binding);
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{binding}"
        );
    }
    for register in ["x8", "w8"] {
        let invalid = valid.replace("clobbers []", &format!("clobbers [\"{register}\"]"));
        assert!(parse_section9_module(invalid.as_bytes()).is_err());
    }
}

#[test]
fn assembly_edges_reject_stale_tokens_self_inputs_and_nonlocal_results() {
    parse_section9_module(EDGES.as_bytes()).expect("the reference edge contract is valid");
    for invalid in [
        EDGES.replace("^b2(%v4, %v5, %v6)", "^b2(%v4, %v2, %v6)"),
        EDGES.replace("^b1(%v4, %v5, %v6)", "^b1(%v4, %v5, %v3)"),
        EDGES.replace("^b2(%v4, %v5, %v6)", "^b2(%v4, %v5, %v5)"),
        EDGES.replace("^b1(%v4, %v5, %v6)", "^b1(%v4, %v5)"),
        EDGES.replace("bits<32>, %v0)]", "bits<32>, %v4)]"),
        EDGES.replace("ret %v7, %v8, %v9", "ret %v4, %v8, %v9"),
        EDGES.replace("(%v4, %v5, %v6) =", "(%v0, %v5, %v6) ="),
    ] {
        assert!(
            parse_section9_module(invalid.as_bytes()).is_err(),
            "{invalid}"
        );
    }
}

#[test]
fn assembly_strings_reject_noncanonical_or_unterminated_encodings() {
    parse_section9_module(ordinary(r#""nop""#).as_bytes()).expect("canonical reference");
    for template in [r#""\x6eop""#, r#""\n""#, r#""\x0A""#, "\"é\"", "\"unclosed"] {
        assert!(parse_section9_module(ordinary(template).as_bytes()).is_err());
    }
}
