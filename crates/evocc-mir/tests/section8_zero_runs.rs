//! EvoMIR Section 8: "globals as data in `.data`/`.bss`/`.rodata` (`__DATA,__data`
//! etc. on Darwin) with their EvoIR initial bytes".
//!
//! A global's initial bytes are a run-length encoding, not a byte list. A
//! global that is all zero, whose EvoIR 2.1 `initialized=` bits are all set and
//! which carries no relocation, holds no bytes an object file has to store: it
//! is emitted in the target's zero-filled section, and emission returns to the
//! text section afterwards so the following output lands where it did before.
//! Elsewhere a run of zero bytes is one `.zero N` and the bytes between runs
//! are `.byte` lists, at most sixteen to a line; relocations keep the
//! `.xword symbol+addend` of Section 10 at their own offsets.
//!
//! The measurement that motivates the encoding is the line count: a megabyte
//! of zeros must not be a megabyte of lines.

use evocc_evoir::parse_section9_module;
use evocc_mir::{emit_module_for, Target};

/// One `global` line of canonical EvoIR text, with `bytes=` and
/// `initialized=` derived from `bytes`.
fn global_line(
    symbol: &str,
    origin: u64,
    visibility: &str,
    align: u64,
    bytes: &[u8],
    relocs: &str,
) -> String {
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let bits = "1".repeat(bytes.len());
    format!(
        "global @{symbol} origin={origin} kind=file visibility={visibility} size={} align={align} bytes={hex} initialized={bits}{relocs}\n",
        bytes.len()
    )
}

/// Emit a unit made only of globals for `target`.
fn emitted(target: Target, unit: &str) -> String {
    let module = parse_section9_module(unit.as_bytes()).expect("the unit parses");
    emit_module_for(target, &[], &module.globals)
}

/// The lines of the global whose label is `label`, up to the next directive
/// that starts another global or leaves the section.
fn body_of(text: &str, label: &str) -> Vec<String> {
    let wanted = format!("{label}:");
    let mut lines = text.lines().skip_while(|line| line.trim() != wanted);
    assert!(lines.next().is_some(), "the label is emitted:\n{text}");
    lines
        .take_while(|line| {
            let line = line.trim_start();
            !line.starts_with(".p2align")
                && !line.starts_with(".section")
                && !line.starts_with(".bss")
                && !line.starts_with(".data")
                && !line.starts_with(".text")
                && !line.trim().is_empty()
        })
        .map(|line| line.trim().to_owned())
        .collect()
}

/// The section directive in force where `label` is defined.
fn section_of(text: &str, label: &str) -> String {
    let wanted = format!("{label}:");
    let mut section = String::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == wanted {
            return section;
        }
        if trimmed.starts_with(".section")
            || trimmed == ".bss"
            || trimmed == ".data"
            || trimmed == ".text"
        {
            section = trimmed.to_owned();
        }
    }
    panic!("{label} is defined:\n{text}");
}

#[test]
fn an_all_zero_initialized_global_is_emitted_in_the_zero_filled_section() {
    let unit = global_line("blank", 0, "external", 4, &[0; 4096], "");
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target, &unit);
        let (bss, prefix) = match target {
            Target::Darwin => (".section __DATA,__bss,zerofill", "_"),
            Target::Linux => (".bss", ""),
        };
        let label = format!("{prefix}blank");
        assert_eq!(
            section_of(&text, &label),
            bss,
            "an all-zero global holds no bytes to store:\n{text}"
        );
        assert!(
            text.contains(&format!(".p2align 2\n.globl {label}\n{label}:\n")),
            "the alignment, the external symbol and the label are unchanged:\n{text}"
        );
        assert_eq!(
            body_of(&text, &label),
            vec![".zero 4096".to_owned()],
            "the whole object is one directive:\n{text}"
        );
        let text_section = match target {
            Target::Darwin => ".section __TEXT,__text,regular,pure_instructions",
            Target::Linux => ".text",
        };
        let tail = &text[text.find(".zero 4096").expect("the fill")..];
        assert!(
            tail.lines().any(|line| line.trim() == text_section),
            "emission returns to the text section afterwards:\n{text}"
        );
    }
}

#[test]
fn an_internal_zero_filled_global_keeps_its_local_symbol() {
    let unit = global_line("hidden", 0, "internal", 8, &[0; 64], "");
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target, &unit);
        assert!(
            !text.contains(".globl"),
            "Section 8 revision 0.16: an internal global is never declared global:\n{text}"
        );
        let label = match target {
            Target::Darwin => "_hidden",
            Target::Linux => "hidden",
        };
        assert!(
            text.contains(&format!(".p2align 3\n{label}:\n    .zero 64\n")),
            "the alignment, label and fill are the whole global:\n{text}"
        );
    }
}

#[test]
fn a_megabyte_of_zeros_is_a_handful_of_lines() {
    let unit = global_line("big", 0, "internal", 1, &[0; 1_048_576], "");
    for target in [Target::Darwin, Target::Linux] {
        let text = emitted(target, &unit);
        assert!(
            text.lines().count() < 16,
            "a megabyte of zeros is not a megabyte of lines, it is {} lines",
            text.lines().count()
        );
    }
}

#[test]
fn a_mixed_global_collapses_its_zero_runs_and_packs_its_bytes() {
    // Sixty-four zeros, thirty-two ascending bytes, sixty-four zeros, one byte.
    let mut bytes = vec![0_u8; 64];
    bytes.extend(1_u8..=32);
    bytes.extend(vec![0_u8; 64]);
    bytes.push(255);
    let unit = global_line("mixed", 0, "external", 4, &bytes, "");
    let text = emitted(Target::Linux, &unit);
    assert_eq!(
        section_of(&text, "mixed"),
        ".data",
        "a global with content stays in the data section:\n{text}"
    );
    assert_eq!(
        body_of(&text, "mixed"),
        vec![
            ".zero 64".to_owned(),
            ".byte 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16".to_owned(),
            ".byte 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32".to_owned(),
            ".zero 64".to_owned(),
            ".byte 255".to_owned(),
        ],
        "runs collapse and the bytes between them are sixteen to a line:\n{text}"
    );
}

#[test]
fn a_short_run_of_zeros_rides_along_in_the_byte_list() {
    // A run shorter than one `.byte` line's sixteen bytes costs a line of its
    // own to collapse and saves none, so it is written out.
    let bytes = vec![1, 0, 0, 0, 2, 0, 0, 0];
    let unit = global_line("pair", 0, "external", 4, &bytes, "");
    let text = emitted(Target::Linux, &unit);
    assert_eq!(
        body_of(&text, "pair"),
        vec![".byte 1, 0, 0, 0, 2, 0, 0, 0".to_owned()],
        "eight bytes are one line, not four directives:\n{text}"
    );
}

#[test]
fn relocations_keep_their_offsets_between_the_runs() {
    let mut bytes = vec![0_u8; 48];
    bytes[40] = 7;
    let unit = format!(
        "{}{}",
        global_line("target", 0, "external", 4, &[9, 9, 9, 9], ""),
        global_line(
            "table",
            1,
            "external",
            8,
            &bytes,
            " relocs=[(0, @target, 8), (8, @target, 0)]"
        )
    );
    let text = emitted(Target::Linux, &unit);
    assert_eq!(
        body_of(&text, "table"),
        vec![
            ".xword target+8".to_owned(),
            ".xword target".to_owned(),
            ".zero 24".to_owned(),
            ".byte 7, 0, 0, 0, 0, 0, 0, 0".to_owned(),
        ],
        "the relocated words replace their bytes, and a run stops at the next one:\n{text}"
    );
}

#[test]
fn a_zero_run_never_crosses_a_relocation() {
    let unit = format!(
        "{}{}",
        global_line("anchor", 0, "external", 4, &[1, 2, 3, 4], ""),
        global_line(
            "spanning",
            1,
            "external",
            8,
            &[0; 48],
            " relocs=[(16, @anchor, 0)]"
        )
    );
    let text = emitted(Target::Linux, &unit);
    assert_eq!(
        body_of(&text, "spanning"),
        vec![
            ".zero 16".to_owned(),
            ".xword anchor".to_owned(),
            ".zero 24".to_owned(),
        ],
        "the run before the relocation stops at its offset:\n{text}"
    );
    assert_eq!(
        section_of(&text, "spanning"),
        ".data",
        "a relocation is a stored byte, so the object is not zero-filled:\n{text}"
    );
}

#[test]
fn sections_alternate_as_the_globals_need_them() {
    let unit = format!(
        "{}{}{}",
        global_line("zeros_first", 0, "external", 4, &[0; 32], ""),
        global_line("content", 1, "external", 4, &[1, 0, 0, 0], ""),
        global_line("zeros_last", 2, "external", 4, &[0; 32], ""),
    );
    let text = emitted(Target::Linux, &unit);
    assert_eq!(section_of(&text, "zeros_first"), ".bss", "{text}");
    assert_eq!(section_of(&text, "content"), ".data", "{text}");
    assert_eq!(section_of(&text, "zeros_last"), ".bss", "{text}");
    let directives: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| *line == ".bss" || *line == ".data" || *line == ".text")
        .collect();
    assert_eq!(
        directives,
        vec![".text", ".bss", ".data", ".bss", ".text"],
        "each global names its section only when the section changes:\n{text}"
    );
}
