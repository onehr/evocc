use evocc_evoir::{parse_section9_module, InterpreterLimits};
use evocc_mir::{interpret_module, lower_module, parse_module, render_module, MirLimits, MirValue};

#[test]
fn read_modify_write_is_one_event_and_round_trips_every_operand() {
    let source = "global @cell origin=0 kind=file visibility=external size=4 align=4 bytes=00000000 initialized=1111

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=true, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @cell : ptr<generic>
    %v3 = const.bits<32> 5
    %v4 = access_in_bounds %v0, %v2 [size=4, align=4]
    (%v5, %v6, %v7) = atomic.rmw.add seq_cst %v4, %v1, %v2, %v3 : bits<32>
    %v8 = const.bits<32> 7
    %v9 = access_in_bounds %v6, %v2 [size=4, align=4]
    (%v10, %v11, %v12) = atomic.rmw.cmpxchg seq_cst %v9, %v7, %v2, %v3, %v8 : bits<32>
    ret %v10, %v11, %v12
}
";
    let ir = parse_section9_module(source.as_bytes()).unwrap();
    let ir_text = evocc_evoir::render_module(&ir);
    assert_eq!(parse_section9_module(ir_text.as_bytes()).unwrap(), ir);
    let args = [
        evocc_evoir::RuntimeValue::new(evocc_types::EvoIRType::Mem(evocc_types::Region::All), 0),
        evocc_evoir::RuntimeValue::new(evocc_types::EvoIRType::World, 0),
    ];
    let outcome =
        evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100)).unwrap();
    assert_eq!(outcome.return_values[0].raw_bits, 5);
    assert_eq!(outcome.world.atomic_trace().len(), 2);
    assert_eq!(
        outcome.world.atomic_trace()[0].rmw,
        Some(evocc_memory::AtomicRmwOp::Add)
    );
    assert_eq!(
        outcome.world.atomic_trace()[1].direction,
        evocc_memory::VolatileDirection::Load
    );
    let machine = lower_module(&ir).unwrap();
    let text = render_module(&machine);
    let parsed = parse_module(&text).unwrap();
    assert_eq!(machine, parsed);
    let result = interpret_module(&parsed, "main", &[], MirLimits::new(100)).unwrap();
    assert_eq!(result.results, [MirValue::Int(5)]);
    assert_eq!(result.world, outcome.world);
    for bad in [
        source.replace(", %v3, %v8 :", ", %v3 :"),
        source.replace("%v3, %v8 : bits<32>", "%v3, %v2 : bits<32>"),
        source.replace("%v3, %v8 : bits<32>", "%v3, %v8 : bits<64>"),
        source.replace("rmw.add seq_cst", "rmw.missing seq_cst"),
    ] {
        assert!(parse_section9_module(bad.as_bytes())
            .and_then(|m| evocc_evoir::validate_module(&m))
            .is_err());
    }
}

#[test]
fn atomic_machine_text_and_execution_preserve_order_and_events() {
    let source = "global @cell origin=0 kind=file visibility=external size=4 align=4 bytes=00000000 initialized=1111

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=false, may_trap=true, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @cell : ptr<generic>
    %v3 = const.bits<32> 42
    %v4 = access_in_bounds %v0, %v2 [size=4, align=4]
    (%v5, %v6) = atomic.store release volatile %v4, %v1, %v2, %v3
    %v7 = access_in_bounds %v5, %v2 [size=4, align=4]
    (%v8, %v9, %v10) = atomic.load acquire %v7, %v6, %v2 : bits<32>
    ret %v8, %v9, %v10
}
";
    let ir = parse_section9_module(source.as_bytes()).unwrap();
    let args = [
        evocc_evoir::RuntimeValue::new(evocc_types::EvoIRType::Mem(evocc_types::Region::All), 0),
        evocc_evoir::RuntimeValue::new(evocc_types::EvoIRType::World, 0),
    ];
    let ir_result =
        evocc_evoir::interpret_module(&ir, "main", &args, InterpreterLimits::new(100)).unwrap();
    let mir = lower_module(&ir).unwrap();
    let text = render_module(&mir);
    assert!(text.contains("atomic.store.w release volatile"));
    assert!(text.contains("atomic.load.w acquire"));
    let parsed = parse_module(&text).unwrap();
    assert_eq!(render_module(&parsed), text);
    for module in [&mir, &parsed] {
        evocc_mir::validate_module(module).unwrap();
        let result = interpret_module(module, "main", &[], MirLimits::new(100)).unwrap();
        assert_eq!(result.results, vec![MirValue::Int(42)]);
        assert_eq!(result.world, ir_result.world);
    }
    for bad in [
        text.replace("store.w release", "store.w acquire"),
        text.replace("load.w acquire", "load.w release"),
        text.replace("load.w acquire", "load.x acquire"),
    ] {
        assert!(parse_module(&bad)
            .and_then(|m| evocc_mir::validate_module(&m).map_err(|e| format!("{e:?}")))
            .is_err());
    }
}
