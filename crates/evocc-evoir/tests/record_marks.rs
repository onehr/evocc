//! EvoIR 10.1.3, "Records across calls" (revision 1.9.73).
//!
//! "the backend must be told which pointers are records (revision 1.9.73): a
//! record parameter is written `%argN: ptr<generic> byval(size, align)` and the
//! return destination `%argN: ptr<generic> sret(size, align)`, in the function
//! header, in every `declare`, and in every `fnptr<Sig>` naming such a
//! signature; the attribute is part of the signature for equality."
//!
//! The mark says nothing the interpreters read: EvoIR still passes a pointer to
//! a caller-allocated copy, and the pointer form is what they execute. What the
//! mark changes is what the machine may do with that pointer (EvoMIR Section 6).

use evocc_evoir::{
    interpret_module, parse_section9_module, render_module, validate_module, InterpreterLimits,
    ParamMark, RecordShape, RuntimeValue,
};
use evocc_types::{AddressSpace, EvoIRType};

/// The zstd receipt's reproducing shape: an eight-byte record in and out.
///
/// `typedef struct { unsigned long long t; } T; unsigned long long span(T a, T
/// b);` with the record copies marked, plus a `make` whose result is a
/// twenty-four-byte record written through the destination.
const MARKED: &str = "\
declare @host_span(ptr<generic> byval(8, 8), ptr<generic> byval(8, 8), mem<all>, world) -> (bits<64>, mem<all>, world) cc = target_default

func @span(%arg0: ptr<generic> byval(8, 8), %arg1: ptr<generic> byval(8, 8), %arg2: mem<all>, %arg3: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: ptr<generic>, %v2: mem<all>, %v3: world):
    %v4 = access_in_bounds %v2, %v0 [size=8, align=8]
    (%v5, %v6, %v7) = load %v4, %v3, %v0 : bits<64>
    %v8 = access_in_bounds %v6, %v1 [size=8, align=8]
    (%v9, %v10, %v11) = load %v8, %v7, %v1 : bits<64>
    %v12 = sub.bits<64> %v9, %v5
    ret %v12, %v10, %v11
}

func @make(%arg0: bits<64>, %arg1: ptr<generic> sret(24, 8), %arg2: mem<all>, %arg3: world) -> (mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<64>, %v1: ptr<generic>, %v2: mem<all>, %v3: world):
    %v4 = access_in_bounds %v2, %v1 [size=8, align=8]
    (%v5, %v6) = store %v4, %v3, %v1, %v0
    ret %v5, %v6
}
";

/// The same module with every mark struck out: the form EvoIR had before the
/// revision, which must stay canonical.
fn unmarked() -> String {
    MARKED
        .replace(" byval(8, 8)", "")
        .replace(" sret(24, 8)", "")
}

#[test]
fn zero_sized_record_marks_parse_render_and_validate() {
    let text = MARKED
        .replace("byval(8, 8)", "byval(0, 8)")
        .replace("sret(24, 8)", "sret(0, 8)");
    let module = parse_section9_module(text.as_bytes()).unwrap();
    validate_module(&module).unwrap();
    assert_eq!(render_module(&module), text);
}

#[test]
fn the_marks_parse_render_and_round_trip() {
    let module = parse_section9_module(MARKED.as_bytes()).expect("the marked module parses");
    validate_module(&module).expect("the marked module validates");

    let span = &module.functions[0];
    assert_eq!(
        span.param_marks,
        vec![
            ParamMark::ByVal(RecordShape::new(8, 8)),
            ParamMark::ByVal(RecordShape::new(8, 8)),
            ParamMark::None,
            ParamMark::None,
        ],
        "a record parameter carries byval(size, align) in the function header"
    );
    let make = &module.functions[1];
    assert_eq!(
        make.param_marks[1],
        ParamMark::Sret(RecordShape::new(24, 8)),
        "the return destination carries sret(size, align)"
    );
    assert_eq!(
        module.declares[0].param_marks[0],
        ParamMark::ByVal(RecordShape::new(8, 8)),
        "a declare carries the same marks"
    );

    assert_eq!(
        render_module(&module),
        MARKED,
        "the round trip through the canonical text is the identity"
    );
}

#[test]
fn text_without_marks_stays_canonical() {
    let text = unmarked();
    let module = parse_section9_module(text.as_bytes()).expect("the unmarked module parses");
    validate_module(&module).expect("the unmarked module validates");
    assert!(
        module
            .functions
            .iter()
            .all(|function| function.param_marks.is_empty()),
        "no mark is spelled, so the canonical list is empty"
    );
    let rendered = render_module(&module);
    assert!(
        !rendered.contains("byval(") && !rendered.contains("sret("),
        "and nothing is written back:\n{rendered}"
    );
    assert_eq!(rendered, text);
}

#[test]
fn the_marks_are_part_of_the_signature_for_equality() {
    let marked = parse_section9_module(MARKED.as_bytes()).expect("the marked module parses");
    let text = unmarked();
    let plain = parse_section9_module(text.as_bytes()).expect("the unmarked module parses");
    assert_ne!(
        marked.functions[0].param_marks, plain.functions[0].param_marks,
        "two headers that differ only by a mark are different signatures"
    );

    let marked_pointer = EvoIRType::FnPtr {
        params: vec![
            EvoIRType::Ptr(AddressSpace::Generic),
            EvoIRType::Mem(evocc_types::Region::All),
            EvoIRType::World,
        ],
        results: vec![EvoIRType::Mem(evocc_types::Region::All), EvoIRType::World],
        variadic: false,
        marks: vec![
            ParamMark::ByVal(RecordShape::new(16, 8)),
            ParamMark::None,
            ParamMark::None,
        ],
    };
    let plain_pointer = EvoIRType::FnPtr {
        params: vec![
            EvoIRType::Ptr(AddressSpace::Generic),
            EvoIRType::Mem(evocc_types::Region::All),
            EvoIRType::World,
        ],
        results: vec![EvoIRType::Mem(evocc_types::Region::All), EvoIRType::World],
        variadic: false,
        marks: Vec::new(),
    };
    assert_ne!(
        marked_pointer, plain_pointer,
        "the attribute is part of the signature for equality"
    );
}

/// The `fnptr<Sig>` spelling of a marked signature, taken through the reader
/// and the writer inside a module that takes a marked function's address.
const POINTER: &str = "\
func @callee(%arg0: ptr<generic> byval(16, 16), %arg1: mem<all>, %arg2: world) -> (mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    ret %v1, %v2
}

func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, fenv=true, may_trap=true, may_unwind=true }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = func_addr @callee : fnptr<(ptr<generic> byval(16, 16), mem<all>, world)->(mem<all>, world)>
    (%v4, %v5, %v6) = alloca %v1, %v2 [origin=0, size=16, align=16]
    (%v7, %v8) = call %v3(%v4, %v5, %v6)
    ret %v0, %v7, %v8
}
";

#[test]
fn a_function_pointer_carries_the_marks_of_the_signature_it_names() {
    let module = parse_section9_module(POINTER.as_bytes()).expect("the pointer module parses");
    validate_module(&module).expect("a marked fnptr matches the marked callee");
    assert_eq!(render_module(&module), POINTER);

    // The address of a marked function is a marked pointer, so a text whose
    // pointer type drops the mark names a different signature and is refused
    // where every other signature disagreement is.
    let stripped = POINTER.replace(
        "fnptr<(ptr<generic> byval(16, 16), mem<all>, world)->(mem<all>, world)>",
        "fnptr<(ptr<generic>, mem<all>, world)->(mem<all>, world)>",
    );
    let error = parse_section9_module(stripped.as_bytes())
        .expect_err("an unmarked pointer is a different signature");
    assert!(
        error.contains("disagrees with 'callee' signature"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn a_mark_rides_only_on_a_generic_pointer() {
    let bad = MARKED.replace(
        "%arg0: ptr<generic> byval(8, 8)",
        "%arg0: bits<64> byval(8, 8)",
    );
    let error = parse_section9_module(bad.as_bytes())
        .expect_err("a mark on a non-pointer parameter is refused");
    assert!(error.contains("ptr<generic>"), "unexpected: {error}");
}

#[test]
fn a_signature_carries_at_most_one_return_destination() {
    let bad = MARKED.replace(
        "%arg1: ptr<generic> byval(8, 8)",
        "%arg1: ptr<generic> sret(8, 8)",
    );
    let bad = bad.replace(
        "%arg0: ptr<generic> byval(8, 8)",
        "%arg0: ptr<generic> sret(8, 8)",
    );
    let error =
        parse_section9_module(bad.as_bytes()).expect_err("two return destinations are refused");
    assert!(error.contains("at most one sret"), "unexpected: {error}");
}

#[test]
fn a_record_shape_is_a_real_record() {
    for (spelling, needle) in [
        ("byval(8, 6)", "power of two"),
        ("byval(8, 0)", "power of two"),
    ] {
        let bad = MARKED.replacen("byval(8, 8)", spelling, 1);
        let error = parse_section9_module(bad.as_bytes())
            .err()
            .unwrap_or_else(|| panic!("{spelling} must be refused"));
        assert!(error.contains(needle), "{spelling}: unexpected {error}");
    }
}

/// A driver that allocates two eight-byte copies, fills them and calls `@span`
/// exactly as EvoIR 10.1.3 says a record argument crosses: a pointer to a
/// caller-allocated copy.
const DRIVER: &str = "\
func @main(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={all}, writes={all}, volatile=false, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = alloca %v1, %v2 [origin=0, size=8, align=8]
    (%v6, %v7, %v8) = alloca %v4, %v5 [origin=1, size=8, align=8]
    %v9 = const.bits<64> 40
    %v10 = access_in_bounds %v7, %v3 [size=8, align=8]
    (%v11, %v12) = store %v10, %v8, %v3, %v9
    %v13 = const.bits<64> 82
    %v14 = access_in_bounds %v11, %v6 [size=8, align=8]
    (%v15, %v16) = store %v14, %v12, %v6, %v13
    (%v17, %v18, %v19) = call @span(%v3, %v6, %v15, %v16)
    %v20 = trunc.bits<32> %v17
    ret %v20, %v18, %v19
}
";

#[test]
fn the_interpreter_is_indifferent_to_the_marks() {
    let run = |text: &str| {
        let whole = format!("{text}\n{DRIVER}");
        let module = parse_section9_module(whole.as_bytes()).expect("the whole module parses");
        interpret_module(
            &module,
            "main",
            &[
                RuntimeValue::new(EvoIRType::Bits(32), 0),
                RuntimeValue::new(EvoIRType::Mem(evocc_types::Region::All), 0),
                RuntimeValue::new(EvoIRType::World, 0),
            ],
            InterpreterLimits::new(64),
        )
        .expect("execution")
        .return_values[0]
            .clone()
    };
    assert_eq!(run(MARKED), RuntimeValue::new(EvoIRType::Bits(32), 42));
    assert_eq!(
        run(MARKED),
        run(&unmarked()),
        "the pointer form is what the interpreter executes"
    );
}
