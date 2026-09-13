//! EvoMIR Section 8, "Linkage" (revision 0.16).
//!
//! "An EvoIR function or global whose `visibility` is `internal` is emitted
//! without `.globl`, so its symbol is local to the object. Before emission,
//! every internal function that no `call`, `func_addr`, `block_addr`
//! relocation or other emitted function references is dropped, and the check
//! repeats until no internal function is dropped; an internal global is kept,
//! since data is cheap and a relocation may name it. Module assembly may
//! reference any symbol, so a symbol named in a module assembly entry counts
//! as referenced."

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// The body of a function that adds one, parameterized by name and by the
/// visibility field the header carries.
fn adder(name: &str, visibility: &str) -> String {
    format!(
        "func @{name}(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default{visibility}
    effects {{ reads={{}}, writes={{}}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 1
    %v4 = add.bits<32> %v0, %v3
    ret %v4, %v1, %v2
}}
"
    )
}

/// A function that calls `callee` and returns its answer.
///
/// Section 4.6 makes a caller's summary its callees' join, so a call to a
/// `declare` carries the top summary and a call to a pure function the empty
/// one; `effects` says which.
fn caller(name: &str, visibility: &str, callee: &str, effects: &str) -> String {
    format!(
        "func @{name}(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default{visibility}
{effects}
{{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    (%v3, %v4, %v5) = call @{callee}(%v0, %v1, %v2)
    ret %v3, %v4, %v5
}}
"
    )
}

const INTERNAL: &str = " visibility=internal";

/// Section 4.6's empty summary, which a call to a pure function keeps.
const PURE: &str = "    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }";

/// Section 4.6's top summary, which every external function has and which a
/// call to a `declare` therefore gives its caller.
const TOP: &str = "    effects { reads={all}, writes={all}, volatile=true, atomic=true, io=true, may_trap=true, may_unwind=true }";

#[test]
fn a_private_callee_unused_parameter_does_not_force_an_external_address() {
    let source = format!(
        "extern @absent size=4 align=4

func @ignore(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default visibility=internal
{PURE}
{{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<32> 7
    ret %v3, %v1, %v2
}}

func @entry(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
{PURE}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @absent : ptr<generic>
    (%v3, %v4, %v5) = call @ignore(%v2, %v0, %v1)
    ret %v3, %v4, %v5
}}
"
    );
    let private = emitted(Target::Linux, &source);
    assert!(
        !private
            .lines()
            .any(|line| line.contains("absent") && !line.trim_start().starts_with('.')),
        "an unused private-call argument keeps an unresolved address: {private}"
    );
    // External linkage can be interposed, so an unused local body parameter
    // is not authority to change the argument supplied to that symbol.
    let public = emitted(Target::Linux, &source.replace(" visibility=internal", ""));
    assert!(public
        .lines()
        .any(|line| line.contains("adrp") && line.contains("absent")));
}

#[test]
fn undefined_symbol_addresses_use_indirection_without_changing_definitions() {
    let source = format!(
        "extern @value size=4 align=4 linkage=weak

declare @optional(mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default linkage=weak

func @object_address(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
{PURE}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @value : ptr<generic>
    %v3 = ptr.to_int %v2
    ret %v3, %v0, %v1
}}

func @function_address(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
{PURE}
{{
^entry(%v0: mem<all>, %v1: world):
    %v2 = func_addr @optional : fnptr<(mem<all>, world)->(bits<32>, mem<all>, world)>
    %v3 = ptr.to_int %v2
    ret %v3, %v0, %v1
}}
"
    );
    for (target, page, offset) in [
        (Target::Linux, ":got:", "#:got_lo12:"),
        (Target::Darwin, "@GOTPAGE", "@GOTPAGEOFF"),
    ] {
        let assembly = emitted(target, &source);
        for symbol in ["value", "optional"] {
            let name = if target == Target::Darwin {
                format!("_{symbol}")
            } else {
                symbol.to_owned()
            };
            let page = if target == Target::Darwin {
                format!("{name}{page}")
            } else {
                format!("{page}{name}")
            };
            let offset = if target == Target::Darwin {
                format!("{name}{offset}")
            } else {
                format!("{offset}{name}")
            };
            assert!(assembly.contains(&page), "missing {page}: {assembly}");
            assert!(
                assembly
                    .lines()
                    .any(|line| line.trim_start().starts_with("ldr ") && line.contains(&offset)),
                "missing GOT load {offset}: {assembly}"
            );
        }
        let strong = emitted(target, &source.replace(" linkage=weak", ""));
        assert!(strong.contains(if target == Target::Linux {
            ":got:value"
        } else {
            "_value@GOTPAGE"
        }));
        assert!(strong.contains(if target == Target::Linux {
            ":got:optional"
        } else {
            "_optional@GOTPAGE"
        }));

        // A definition in this module still takes the direct recipe, including
        // a weak definition. Indirection is decided by the declaration table,
        // not by a global spelling or a linkage flag alone.
        for (visibility, linkage) in [
            ("external", ""),
            ("external", " linkage=weak"),
            ("internal", ""),
        ] {
            let defined = source.replace(
                "extern @value size=4 align=4 linkage=weak",
                &format!("global @value origin=0 kind=file visibility={visibility}{linkage} size=4 align=4 bytes=07000000 initialized=1111"),
            );
            let defined = emitted(target, &defined);
            assert!(!defined.contains(":got:value") && !defined.contains("_value@GOTPAGE"));
        }
    }
}

#[test]
fn weak_symbols_and_aliases_emit_linkage_without_copying_definitions() {
    let source = format!(
        "global @value origin=0 kind=file visibility=external linkage=weak size=4 align=4 bytes=07000000 initialized=1111\n\nextern @absent size=4 align=4 linkage=weak\n\ndeclare @optional(bits<32>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default linkage=weak\n\nalias @alternative = @helper\nalias @other_value = @value linkage=weak\n\n{}\n{}",
        adder("helper", INTERNAL), adder("fallback", " linkage=weak"),
    );
    let text = emitted(Target::Linux, &source);
    for form in [
        ".weak value",
        ".weak fallback",
        ".weak absent",
        ".weak optional",
        ".globl alternative",
        ".type alternative, %function",
        ".set alternative, helper",
        ".weak other_value",
        ".type other_value, %object",
        ".set other_value, value",
    ] {
        assert!(text.contains(form), "missing {form}: {text}");
    }
    assert!(
        text.contains("\nhelper:\n"),
        "an alias keeps its internal target"
    );
    assert!(!text.contains("\nalternative:\n"));
    assert!(!text.contains("\nother_value:\n"));
    assert!(!text.contains(".globl fallback"));
    assert!(!text.contains(".globl value"));
    assert!(!text.contains(".size alternative"));
    let text = emitted(Target::Darwin, &source);
    for form in [
        ".globl _fallback\n.weak_definition _fallback",
        ".weak_reference _optional",
        ".weak_reference _absent",
        ".set _alternative, _helper",
        ".set _other_value, _value",
    ] {
        assert!(text.contains(form), "missing {form}: {text}");
    }
    let source = format!(
        "alias @alternative = @helper\n\n{}\n{}",
        adder("helper", INTERNAL),
        caller("entry_point", "", "alternative", PURE)
    );
    assert!(emitted(Target::Linux, &source).contains("bl alternative"));
}

fn emitted(target: Target, text: &str) -> String {
    let module = parse_section9_module(text.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}\n{text}",
            stop.component(),
            stop.detail()
        ),
    }
}

#[test]
fn a_referenced_internal_function_is_emitted_without_a_global_symbol() {
    for (target, prefix) in [(Target::Darwin, "_"), (Target::Linux, "")] {
        let text = emitted(
            target,
            &format!(
                "{}\n{}",
                adder("helper", INTERNAL),
                caller("entry_point", "", "helper", PURE)
            ),
        );
        assert!(
            text.contains(&format!("\n{prefix}helper:\n")),
            "the referenced helper is still defined:\n{text}"
        );
        assert!(
            !text.contains(&format!(".globl {prefix}helper")),
            "but its symbol is local to the object:\n{text}"
        );
        assert!(
            text.contains(&format!(".globl {prefix}entry_point")),
            "while the external function keeps its global symbol:\n{text}"
        );
        assert!(
            text.contains(&format!("bl {prefix}helper")),
            "and the call still reaches it:\n{text}"
        );
    }
}

#[test]
fn an_unreferenced_internal_function_never_reaches_the_assembler() {
    // The shape the kernel lane found: an unused `static inline` stub from a
    // shared header that calls a function the configuration does not build.
    let text = emitted(
        Target::Darwin,
        &format!(
            "declare @absent(bits<32>, mem<all>, world) -> (bits<32>, mem<all>, world) cc = target_default\n\n{}\n{}",
            caller("stub", INTERNAL, "absent", TOP),
            adder("used", ""),
        ),
    );
    assert!(
        !text.contains("_stub"),
        "the unreferenced stub is dropped whole:\n{text}"
    );
    assert!(
        !text.contains("_absent"),
        "so the object carries no undefined reference to what it called:\n{text}"
    );
    assert!(
        text.contains(".globl _used") && text.contains("\n_used:\n"),
        "and the function that is referenced is untouched:\n{text}"
    );
}

#[test]
fn the_check_repeats_until_no_internal_function_is_dropped() {
    // `outer` is internal and unreferenced, and is the only thing that names
    // `inner`: one pass would keep `inner`, and the fixed point drops both.
    let text = emitted(
        Target::Darwin,
        &format!(
            "{}\n{}\n{}",
            adder("inner", INTERNAL),
            caller("outer", INTERNAL, "inner", PURE),
            adder("reachable", ""),
        ),
    );
    assert!(
        !text.contains("_outer") && !text.contains("_inner"),
        "a function reached only from a dropped function is dropped too:\n{text}"
    );
    assert!(
        text.contains("\n_reachable:\n"),
        "and the external function remains:\n{text}"
    );
}

#[test]
fn a_func_addr_and_a_relocation_each_keep_an_internal_function() {
    let taken = "func @take(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = func_addr @helper : fnptr<(bits<32>, mem<all>, world)->(bits<32>, mem<all>, world)>
    %v3 = ptr.to_int %v2
    ret %v3, %v0, %v1
}
";
    let text = emitted(
        Target::Darwin,
        &format!("{}\n{taken}", adder("helper", INTERNAL)),
    );
    assert!(
        text.contains("\n_helper:\n") && !text.contains(".globl _helper"),
        "a `func_addr` references an internal function:\n{text}"
    );

    let relocated = format!(
        "global @slot origin=0 kind=file visibility=external size=8 align=8 bytes=0000000000000000 initialized=11111111 relocs=[(0, @helper, 0)]\n\n{}",
        adder("helper", INTERNAL),
    );
    let text = emitted(Target::Darwin, &relocated);
    assert!(
        text.contains("\n_helper:\n") && !text.contains(".globl _helper"),
        "and so does a relocation inside a global:\n{text}"
    );
}

#[test]
fn a_module_assembly_entry_counts_as_a_reference() {
    let text = emitted(
        Target::Darwin,
        &format!(
            "{}\n{}\nasm \"    bl _helper\\x0a\"\n",
            adder("helper", INTERNAL),
            adder("other", ""),
        ),
    );
    assert!(
        text.contains("\n_helper:\n"),
        "emission does not parse an entry, so a symbol it spells is referenced:\n{text}"
    );
    assert!(
        !text.contains(".globl _helper"),
        "and the kept function is still local:\n{text}"
    );
}

#[test]
fn an_internal_global_is_kept_and_is_not_declared_global() {
    let text = emitted(
        Target::Darwin,
        &format!(
            "global @shared origin=0 kind=file visibility=external size=4 align=4 bytes=01000000 initialized=1111\nglobal @private origin=1 kind=file visibility=internal size=4 align=4 bytes=02000000 initialized=1111\n\n{}",
            adder("reader", ""),
        ),
    );
    assert!(
        text.contains(".globl _shared") && text.contains("\n_shared:\n"),
        "an external object keeps its global symbol:\n{text}"
    );
    assert!(
        text.contains("\n_private:\n") && !text.contains(".globl _private"),
        "an internal object is kept and is local:\n{text}"
    );
}

#[test]
fn a_module_with_no_internal_function_emits_exactly_what_it_did_before() {
    // The field's absence must not move a byte: the only difference between
    // the two texts is the one `.globl` line the internal spelling removes.
    let external = format!(
        "{}\n{}",
        adder("helper", ""),
        caller("entry_point", "", "helper", PURE)
    );
    let internal = format!(
        "{}\n{}",
        adder("helper", INTERNAL),
        caller("entry_point", "", "helper", PURE)
    );
    for target in [Target::Darwin, Target::Linux] {
        let prefix = if matches!(target, Target::Darwin) {
            "_"
        } else {
            ""
        };
        let before = emitted(target, &external);
        let after = emitted(target, &internal);
        assert_eq!(
            before.replace(&format!(".globl {prefix}helper\n"), ""),
            after,
            "the internal spelling removes one line and changes nothing else"
        );
    }
}
