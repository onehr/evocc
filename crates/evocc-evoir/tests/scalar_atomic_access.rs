use evocc_evoir::{parse_section9_module, render_module};

const SOURCE: &str = "func @access(%arg0: ptr<generic>, %arg1: bits<32>, %arg2: mem<all>, %arg3: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: bits<32>, %v2: mem<all>, %v3: world):
    %v4 = access_in_bounds %v2, %v0 [size=4, align=4]
    (%v5, %v6) = atomic.store release volatile %v4, %v3, %v0, %v1
    %v7 = access_in_bounds %v5, %v0 [size=4, align=4]
    (%v8, %v9, %v10) = atomic.load acquire volatile %v7, %v6, %v0 : bits<32>
    ret %v8, %v9, %v10
}
";

#[test]
fn atomic_order_roundtrips_and_is_not_interchangeable_with_volatile() {
    let parsed = parse_section9_module(SOURCE.as_bytes()).expect("valid scalar atomic syntax");
    let rendered = render_module(&parsed);
    assert!(rendered.contains("atomic.store release volatile"));
    assert!(rendered.contains("atomic.load acquire volatile"));
    assert_eq!(
        render_module(&parse_section9_module(rendered.as_bytes()).unwrap()),
        rendered
    );
    let changed =
        parse_section9_module(SOURCE.replace("load acquire", "load relaxed").as_bytes()).unwrap();
    assert_ne!(parsed, changed, "atomic order is semantic data");
}

#[test]
fn atomic_validation_rejects_invalid_orders_widths_alignment_and_tokens() {
    parse_section9_module(SOURCE.as_bytes()).expect("positive control");
    for (changed, atomic_fault) in [
        (SOURCE.replace("load acquire", "load release"), true),
        (SOURCE.replace("store release", "store acquire"), true),
        (SOURCE.replace("load acquire", "load made_up"), false),
        (
            SOURCE
                .replace("bits<32>", "bits<128>")
                .replace("size=4, align=4", "size=16, align=16"),
            true,
        ),
        (SOURCE.replace("align=4", "align=1"), true),
        (
            SOURCE.replace("volatile %v7, %v6", "volatile %v2, %v6"),
            true,
        ),
        (
            SOURCE.replace("volatile %v7, %v6", "volatile %v7, %v3"),
            false,
        ),
        (SOURCE.replace("atomic=true", "atomic=false"), false),
    ] {
        let error = parse_section9_module(changed.as_bytes()).expect_err(&changed);
        if atomic_fault {
            assert!(
                error.contains("E3062: invalid-atomic-access"),
                "{error}: {changed}"
            );
        }
    }
}

#[test]
fn atomic_rmw_preserves_operation_and_rejects_boolean_arithmetic() {
    let source = "func @increment(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (bits<8>, mem<all>, world) cc = target_default\n    effects { reads={all}, writes={all}, volatile=false, atomic=true, io=false, fenv=false, may_trap=true, may_unwind=false }\n{\n^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):\n    %v3 = const.bits<8> 1\n    %v4 = access_in_bounds %v1, %v0 [size=1, align=1]\n    (%v5, %v6, %v7) = atomic.rmw.add seq_cst %v4, %v2, %v0, %v3 : bits<8>\n    ret %v5, %v6, %v7\n}\n";
    let module = parse_section9_module(source.as_bytes()).unwrap();
    for replacement in ["rmw.sub seq_cst", "rmw.add relaxed"] {
        let changed =
            parse_section9_module(source.replace("rmw.add seq_cst", replacement).as_bytes())
                .unwrap();
        assert_ne!(module, changed);
        assert_eq!(
            parse_section9_module(render_module(&changed).as_bytes()).unwrap(),
            changed
        );
    }
    assert!(parse_section9_module(source.replace("bits<8>", "bits<1>").as_bytes()).is_err());
}
