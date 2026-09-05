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
