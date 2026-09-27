//! EvoMIR Section 8, "Module assembly" (revision 0.14).
//!
//! "Every entry of the EvoIR module assembly table (EvoIR 2.1, revision
//! 1.9.66) is emitted verbatim, in table order, after the last function and
//! the last global of the unit. Emission does not parse an entry and
//! substitutes nothing into it; it precedes the first entry with a `.text`
//! directive and follows the last with one, so an entry that switches sections
//! cannot change where a later revision's trailing output lands."

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// The global holds a non-zero byte so that the `.byte` probe below has
/// something to find: an all-zero global is reserved, not stored.
const UNIT: &str = r#"global @seen origin=0 kind=file visibility=external size=4 align=4 bytes=01000000 initialized=1111

func @main(%arg0: mem<all>, %arg1: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, fenv=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = const.bits<32> 0
    ret %v2, %v0, %v1
}

asm ".globl first\x0afirst: .quad 1\x0a"
asm ".section .rodata\x0a.globl second\x0asecond: .quad 2\x0a"
"#;

fn emitted(target: Target) -> String {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    assert_eq!(
        module.assembly.len(),
        2,
        "the module assembly table is read"
    );
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!(
            "the unit stopped at {}: {}",
            stop.component(),
            stop.detail()
        ),
    }
}

#[test]
fn every_entry_is_verbatim_after_the_last_function_and_global() {
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target);
        let first = text
            .find(".globl first")
            .unwrap_or_else(|| panic!("the first entry is emitted:\n{text}"));
        let second = text
            .find(".globl second")
            .unwrap_or_else(|| panic!("the second entry is emitted:\n{text}"));
        assert!(first < second, "the entries keep table order:\n{text}");

        let last_function = text.find("ret\n").expect("the function body");
        let last_global = text.find(".byte").expect("the global's bytes");
        assert!(
            last_function < first && last_global < first,
            "the table follows the last function and the last global:\n{text}"
        );

        assert!(
            text.contains("first: .quad 1"),
            "an entry is copied verbatim, newlines included:\n{text}"
        );
        assert!(
            text.contains(".section .rodata"),
            "emission does not parse an entry:\n{text}"
        );
    }
}

#[test]
fn the_table_is_bracketed_by_the_text_directive() {
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target);
        let section = match target {
            Target::Darwin => ".section __TEXT,__text,regular,pure_instructions",
            Target::Linux => ".text",
        };
        let first = text.find(".globl first").expect("the first entry");
        let tail = &text[first..];
        assert!(
            tail.contains(section),
            "a `.text` directive follows the last entry:\n{text}"
        );
        let head = &text[..first];
        let opening = head
            .rfind(section)
            .unwrap_or_else(|| panic!("a `.text` directive precedes the first entry:\n{text}"));
        assert!(
            head[opening..].find(".byte").is_none(),
            "the opening directive is the table's own, not the text section at the top:\n{text}"
        );
    }
}
