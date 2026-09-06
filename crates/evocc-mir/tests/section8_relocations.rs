//! EvoMIR Section 10, `E4005` withdrawn at revision 0.12, and Section 5's
//! pointer-integer row.
//!
//! "a relocation `(offset, @symbol, addend)` is emitted as `.xword
//! symbol+addend` at its offset, the surrounding bytes as before"; and
//! "`ptr.to_int`, `ptr.from_int.*` | nothing: the value is the same 64-bit
//! register".

use evocc_evoir::parse_section9_module;
use evocc_mir::{compile_module_for, Target};

/// A table of pointers with addends, a function pointer, and the two
/// conversions in a function body.
const UNIT: &str = r#"global @table origin=0 kind=file visibility=external size=16 align=4 bytes=01000000020000000300000004000000 initialized=1111111111111111
global @pointers origin=1 kind=file visibility=external size=24 align=8 bytes=000000000000000000000000000000007b00000000000000 initialized=111111111111111111111111 relocs=[(0, @table, 8), (8, @twice, 0)]

func @twice(%arg0: bits<32>, %arg1: mem<all>, %arg2: world) -> (bits<32>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: bits<32>, %v1: mem<all>, %v2: world):
    %v3 = add.bits<32> %v0, %v0
    ret %v3, %v1, %v2
}

func @round_trip(%arg0: mem<all>, %arg1: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects { reads={}, writes={}, volatile=false, atomic=false, io=false, may_trap=false, may_unwind=false }
{
^entry(%v0: mem<all>, %v1: world):
    %v2 = global_addr @table : ptr<generic>
    %v3 = ptr.to_int %v2
    %v4 = ptr.from_int.ptr<generic> %v3
    %v5 = ptr.to_int %v4
    ret %v5, %v0, %v1
}
"#;

fn emitted(target: Target) -> String {
    let module = parse_section9_module(UNIT.as_bytes()).expect("the unit parses");
    match compile_module_for(target, &module, &|_| true) {
        Ok(assembly) => assembly,
        Err(stop) => panic!("the unit stopped at {}: {}", stop.component(), stop.detail()),
    }
}

/// The lines of the global whose symbol is `name`, up to the next `.p2align`.
fn body_of(text: &str, name: &str) -> Vec<String> {
    let label = format!("{name}:");
    let mut lines = text.lines().skip_while(|line| line.trim() != label);
    lines.next();
    lines
        .take_while(|line| !line.trim_start().starts_with(".p2align") && !line.trim().is_empty())
        .map(|line| line.trim().to_owned())
        .collect()
}

#[test]
fn a_relocation_is_a_symbol_and_an_addend_inside_the_bytes_on_darwin() {
    let text = emitted(Target::Darwin);
    let body = body_of(&text, "_pointers");
    assert_eq!(
        body,
        vec![
            ".xword _table+8".to_owned(),
            ".xword _twice".to_owned(),
            ".byte 123".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
            ".byte 0".to_owned(),
        ],
        "the relocated words replace their bytes and nothing else:\n{text}"
    );
}

#[test]
fn a_relocation_is_a_symbol_and_an_addend_inside_the_bytes_on_linux() {
    let text = emitted(Target::Linux);
    let body = body_of(&text, "pointers");
    assert_eq!(body[0], ".xword table+8");
    assert_eq!(body[1], ".xword twice");
    assert_eq!(body[2], ".byte 123");
}

#[test]
fn a_pointer_integer_conversion_lowers_to_nothing() {
    let text = emitted(Target::Darwin);
    let mut body = text
        .lines()
        .skip_while(|line| line.trim() != "Lround_trip_block0:");
    body.next();
    let block: Vec<String> = body
        .take_while(|line| !line.trim_start().starts_with("ldp x29"))
        .map(|line| line.trim().to_owned())
        .collect();
    assert_eq!(
        block,
        vec![
            "adrp x0, _table@PAGE".to_owned(),
            "add x0, x0, _table@PAGEOFF".to_owned(),
        ],
        "the two conversions of the address emit nothing at all:\n{text}"
    );
}
