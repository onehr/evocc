//! L11c: the memory-to-memory atomic forms of EvoIR 3.8. The text keeps the
//! form, the order, the volatile marker, the byte count and the buffer
//! alignment, and a change of any one of them is a different instruction or
//! a validation error. The object side needs its natural-alignment guard.

use evocc_evoir::{parse_section9_module, render_module};

const SOURCE: &str = "func @copies(%arg0: ptr<generic>, %arg1: ptr<generic>, %arg2: ptr<generic>, %arg3: ptr<generic>, %arg4: mem<all>, %arg5: world) -> (bits<1>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=true, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: ptr<generic>, %v2: ptr<generic>, %v3: ptr<generic>, %v4: mem<all>, %v5: world):
    %v6 = access_in_bounds %v4, %v0 [size=8, align=8]
    (%v7, %v8) = atomic.copy.load acquire %v6, %v5, %v0, %v1 [size=8, align=4]
    %v9 = access_in_bounds %v7, %v0 [size=8, align=8]
    (%v10, %v11) = atomic.copy.store release %v9, %v8, %v0, %v1 [size=8, align=4]
    %v12 = access_in_bounds %v10, %v0 [size=8, align=8]
    (%v13, %v14) = atomic.copy.exchange acq_rel %v12, %v11, %v0, %v1, %v2 [size=8, align=4]
    %v15 = access_in_bounds %v13, %v0 [size=8, align=8]
    (%v16, %v17, %v18) = atomic.copy.cmpxchg seq_cst %v15, %v14, %v0, %v1, %v2, %v3 [size=8, align=4]
    ret %v16, %v17, %v18
}
";

#[test]
fn every_form_roundtrips_and_each_field_is_semantic() {
    let parsed = parse_section9_module(SOURCE.as_bytes()).expect("valid atomic copies");
    let rendered = render_module(&parsed);
    for form in [
        "atomic.copy.load acquire",
        "atomic.copy.store release",
        "atomic.copy.exchange acq_rel",
        "atomic.copy.cmpxchg seq_cst",
        "[size=8, align=4]",
    ] {
        assert!(rendered.contains(form), "{form}: {rendered}");
    }
    assert_eq!(
        render_module(&parse_section9_module(rendered.as_bytes()).unwrap()),
        rendered
    );
    for (from, to) in [
        ("copy.load acquire", "copy.load relaxed"),
        ("copy.store release", "copy.store release volatile"),
        ("copy.exchange acq_rel", "copy.exchange seq_cst"),
        ("%v0, %v1, %v2, %v3 [size=8", "%v0, %v2, %v1, %v3 [size=8"),
        ("%v0, %v1 [size=8, align=4]", "%v0, %v1 [size=8, align=8]"),
    ] {
        let mut changed = SOURCE.replacen(from, to, 1);
        if to.contains("volatile") {
            // A volatile access also changes the function's effect summary.
            changed = changed.replace("volatile=false", "volatile=true");
        }
        let changed = parse_section9_module(changed.as_bytes())
            .unwrap_or_else(|error| panic!("{to}: {error}"));
        assert_ne!(parsed, changed, "{from} -> {to} is semantic data");
    }
}

#[test]
fn validation_rejects_sizes_orders_alignments_guards_and_operands() {
    for (from, to) in [
        // A load cannot release, and a store cannot acquire.
        ("copy.load acquire", "copy.load release"),
        ("copy.store release", "copy.store acquire"),
        // Sizes 1, 2, 4 and 8 only; the buffers are no more aligned than the size.
        ("[size=8, align=4]", "[size=3, align=1]"),
        ("[size=8, align=4]", "[size=16, align=16]"),
        ("%v0, %v1 [size=8, align=4]", "%v0, %v1 [size=8, align=16]"),
        // The object needs its natural-alignment guard.
        (
            "%v6 = access_in_bounds %v4, %v0 [size=8, align=8]",
            "%v6 = access_in_bounds %v4, %v0 [size=8, align=4]",
        ),
        ("acquire %v6, %v5", "acquire %v4, %v5"),
    ] {
        let changed = SOURCE.replacen(from, to, 1);
        let error = parse_section9_module(changed.as_bytes()).expect_err(&changed);
        assert!(
            error.contains("E3062: invalid-atomic-access"),
            "{error}: {to}"
        );
    }
    for (from, to) in [
        // A buffer is a pointer, and each form has its operand count.
        ("%v0, %v1, %v2, %v3 [size=8", "%v0, %v1, %v2 [size=8"),
        ("%v12, %v11, %v0, %v1, %v2", "%v12, %v11, %v0, %v1"),
        (
            "(%v16, %v17, %v18) = atomic.copy.cmpxchg",
            "(%v17, %v18) = atomic.copy.cmpxchg",
        ),
        ("copy.store release", "copy.swap release"),
        ("[size=8, align=4]\n    %v9", "\n    %v9"),
    ] {
        let changed = SOURCE.replacen(from, to, 1);
        assert!(parse_section9_module(changed.as_bytes()).is_err(), "{to}");
    }
    let changed = SOURCE
        .replace("%arg1: ptr<generic>", "%arg1: bits<64>")
        .replace("%v1: ptr<generic>", "%v1: bits<64>");
    assert!(parse_section9_module(changed.as_bytes()).is_err());
}
