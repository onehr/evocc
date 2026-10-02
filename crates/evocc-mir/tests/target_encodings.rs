//! N07 E1: encoding domains, independently constructed cases and assembler
//! round trips. No instruction semantics is inferred from a disassembly string.
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use evocc_mir::isa::aarch64::encodings as e;
use evocc_mir::{BinOp, Width};

#[test]
fn boundary_domains_keep_encoding_limits_distinct_from_selection_policy() {
    for value in 0..=4096 {
        assert_eq!(e::add_sub_immediate(value), value < 4096);
    }
    assert!(!e::add_sub_immediate(u64::MAX));
    for bytes in [1, 2, 4, 8, 16] {
        for offset in (-260..=260).chain([4095 * bytes - 1, 4095 * bytes, 4096 * bytes]) {
            let scaled = offset >= 0 && offset % bytes == 0 && offset / bytes < 4096;
            assert_eq!(e::scaled_memory_offset(offset, bytes as u64), scaled);
            assert_eq!(
                e::memory_offset(offset, bytes as u64),
                (-256..=255).contains(&offset) || scaled
            );
            assert_eq!(
                e::memory_byte_expansion(offset, bytes as u64),
                offset >= -256 && offset + bytes - 1 <= 4095
            );
        }
    }
    for bytes in [4, 8, 16] {
        for offset in -1030..=1030 {
            assert_eq!(
                e::pair_offset(offset, bytes as u64),
                offset % bytes == 0 && (-64..64).contains(&(offset / bytes))
            );
        }
    }
    for bytes in [0, 3, 32, u64::MAX] {
        assert!(!e::memory_offset(0, bytes));
        assert!(!e::memory_byte_expansion(0, bytes));
        assert!(!e::pair_offset(0, bytes));
    }
    assert!(!e::memory_byte_expansion(i64::MAX, 16));
    for (width, bits) in [(Width::W, 32), (Width::X, 64)] {
        for shift in 0..=64 {
            assert_eq!(e::shift_immediate(shift as u64, width), shift < bits);
            for value in [0, 1, 65535, 65536, u64::MAX] {
                assert_eq!(
                    e::wide_immediate(value, shift, width),
                    value <= 65535 && shift < bits && shift % 16 == 0
                );
            }
        }
        for op in [BinOp::Mul, BinOp::SDiv, BinOp::UDiv] {
            assert!(!op.has_immediate_form());
            assert!(!e::binary_immediate(op, 1, width));
        }
    }
    for offset in [-32772, -32768, -1, 0, 1, 32764, 32768] {
        assert_eq!(
            e::tbz_offset(offset),
            offset % 4 == 0 && (-32768..32768).contains(&offset)
        );
    }
    for offset in [-1048577, -1048576, -1, 0, 1, 1048575, 1048576] {
        assert_eq!(e::adr_offset(offset), (-1048576..1048576).contains(&offset));
    }
}

fn logical_values(width: Width) -> BTreeSet<u64> {
    // Construct by individually placing the bits of a cyclic run, independent
    // of the generated transition-count recognizer.
    let mut result = BTreeSet::new();
    for element in [2, 4, 8, 16, 32, 64] {
        if element > width.bits() {
            continue;
        }
        for count in 1..element {
            for start in 0..element {
                let mut value = 0;
                for base in (0..width.bits()).step_by(usize::from(element)) {
                    for bit in 0..count {
                        value |= 1_u64 << (base + (start + bit) % element);
                    }
                }
                result.insert(value);
            }
        }
    }
    result
}

fn instances() -> (String, usize) {
    let mut body = String::new();
    for (width, prefix, expected) in [(Width::W, "w", 1302), (Width::X, "x", 5334)] {
        let logical = logical_values(width);
        assert_eq!(logical.len(), expected);
        for value in logical {
            assert!(e::logical_mask(value, width));
            for op in ["and", "orr", "eor"] {
                writeln!(body, "{op} {prefix}0, {prefix}1, #0x{value:x}").unwrap();
            }
        }
        for value in [0, 1, 4095] {
            assert!(e::add_sub_immediate(value));
            for op in ["add", "sub"] {
                writeln!(body, "{op} {prefix}0, {prefix}1, #{value}").unwrap();
                writeln!(
                    body,
                    "{op} {prefix}0, {prefix}1, #{value}, lsl #{}",
                    e::ADD_SUB_SHIFT
                )
                .unwrap();
            }
            writeln!(body, "cmp {prefix}1, #{value}").unwrap();
        }
        for value in [0, 1, 65535] {
            for shift in (0..u32::from(width.bits())).step_by(16) {
                assert!(e::wide_immediate(value, shift, width));
                for op in ["movz", "movk", "movn"] {
                    writeln!(body, "{op} {prefix}0, #{value}, lsl #{shift}").unwrap();
                }
            }
        }
        for count in [0, 1, u64::from(width.bits()) - 1] {
            assert!(e::shift_immediate(count, width));
            for op in ["lsl", "lsr", "asr"] {
                writeln!(body, "{op} {prefix}0, {prefix}1, #{count}").unwrap();
            }
        }
    }
    for (load, store, register, bytes) in [
        ("ldrb", "strb", "w", 1),
        ("ldrh", "strh", "w", 2),
        ("ldr", "str", "w", 4),
        ("ldr", "str", "x", 8),
        ("ldr", "str", "s", 4),
        ("ldr", "str", "d", 8),
        ("ldr", "str", "q", 16),
    ] {
        for offset in [-256, -1, 0, 1, 255, 256, 4095, 4095 * bytes] {
            if e::memory_offset(offset, bytes as u64) {
                for op in [load, store] {
                    writeln!(body, "{op} {register}0, [x1, #{offset}]").unwrap();
                }
            }
        }
    }
    for (register, bytes) in [("w", 4), ("x", 8), ("d", 8), ("q", 16)] {
        for offset in [-64 * bytes, -bytes, 0, 63 * bytes] {
            assert!(e::pair_offset(offset, bytes as u64));
            for op in ["ldp", "stp"] {
                writeln!(body, "{op} {register}0, {register}1, [x2, #{offset}]").unwrap();
            }
        }
    }
    let count = body.lines().count();
    (body, count)
}

fn assemble(triple: &str, body: &str, output: &Path) -> std::process::Output {
    let mut process = Command::new("clang")
        .args(["-target", triple, "-x", "assembler", "-c", "-", "-o"])
        .arg(output)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the assembler is required for the encoding oracle");
    process
        .stdin
        .take()
        .unwrap()
        .write_all(format!(".text\n{body}\n").as_bytes())
        .unwrap();
    process.wait_with_output().unwrap()
}

fn objdump() -> PathBuf {
    // Linux exposes llvm-objdump on PATH; the existing Darwin development
    // toolchain is keg-only. An absent oracle is a failure, not a silent PASS.
    for tool in ["llvm-objdump", "/opt/homebrew/opt/llvm/bin/llvm-objdump"] {
        if Command::new(tool)
            .arg("--version")
            .output()
            .is_ok_and(|out| out.status.success())
        {
            return tool.into();
        }
    }
    panic!("llvm-objdump is required for the encoding round trip");
}

fn dump(tool: &Path, file: &Path, args: &[&str]) -> String {
    let out = Command::new(tool).args(args).arg(file).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.len() < 16 * 1024 * 1024);
    String::from_utf8(out.stdout).unwrap()
}

fn text_bytes(tool: &Path, file: &Path, section: &str) -> Vec<u8> {
    let text = dump(tool, file, &["-s", "-j", section]);
    let mut result = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        let Some(address) = words.next() else {
            continue;
        };
        if address.len() < 4 || !address.bytes().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        // This fixture contains complete four-byte instruction words only.
        for word in words.take(4) {
            if word.len() != 8 || !word.bytes().all(|c| c.is_ascii_hexdigit()) {
                break;
            }
            result.extend(
                (0..8)
                    .step_by(2)
                    .map(|at| u8::from_str_radix(&word[at..at + 2], 16).unwrap()),
            );
        }
    }
    result
}

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn generated_instances_assemble_disassemble_and_reassemble_identical_text() {
    let folder =
        std::env::temp_dir().join(format!("evocc-encoding-roundtrip-{}", std::process::id()));
    std::fs::create_dir(&folder).unwrap();
    let folder = Scratch(folder);
    let tool = objdump();
    let (body, count) = instances();
    for (triple, section) in [
        ("aarch64-linux-gnu", ".text"),
        ("arm64-apple-macosx11.0.0", "__text"),
    ] {
        let first = folder.0.join("first.o");
        let second = folder.0.join("second.o");
        let output = assemble(triple, &body, &first);
        assert!(
            output.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let disassembled = dump(&tool, &first, &["-d", "--no-show-raw-insn"]);
        let mut rebuilt = String::new();
        for line in disassembled.lines() {
            let Some((address, instruction)) = line.trim().split_once(':') else {
                continue;
            };
            if address.is_empty() || !address.bytes().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            assert!(!instruction.contains("<unknown>"));
            writeln!(rebuilt, "{}", instruction.trim()).unwrap();
        }
        assert_eq!(
            rebuilt.lines().count(),
            count,
            "{triple}: omitted instructions"
        );
        let output = assemble(triple, &rebuilt, &second);
        assert!(
            output.status.success(),
            "{triple}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = text_bytes(&tool, &first, section);
        assert_eq!(expected.len(), count * 4);
        assert_eq!(expected, text_bytes(&tool, &second, section), "{triple}");
        eprintln!(
            "encodingRoundTrip={triple} instructions={count} bytes={} textIdentical=true",
            expected.len()
        );
    }
}

#[test]
fn assembler_boundaries_reject_nonencodings_without_expanding_selection() {
    for triple in ["aarch64-linux-gnu", "arm64-apple-macosx11.0.0"] {
        for instruction in [
            "add w0, w1, #4097",
            "sub x0, x1, #4097",
            "cmp w0, #4097",
            "and w0, w1, #0",
            "orr x0, x1, #0xffffffffffffffff",
            "movz w0, #65536",
            "movk w0, #1, lsl #32",
            "movn x0, #1, lsl #64",
            "lsl w0, w1, #32",
            "lsr x0, x1, #64",
            "asr w0, w1, #-1",
            "ldr x0, [x1, #32768]",
            "ldr q0, [x1, #65536]",
            "ldur x0, [x1, #-257]",
            "ldur w0, [x1, #256]",
            "ldp x0, x1, [x2, #512]",
            "stp d0, d1, [x2, #-520]",
            "tbz x0, #0, .+32768",
            "tbz x0, #0, .-32772",
            "adr x0, .+1048576",
            "adr x0, .-1048577",
        ] {
            let output = assemble(triple, instruction, Path::new("/dev/null"));
            assert!(!output.status.success(), "{triple}: accepted {instruction}");
        }
        for instruction in [
            "tbz x0, #0, .+32764",
            "tbz x0, #0, .-32768",
            "adr x0, .+1048575",
            "adr x0, .-1048576",
        ] {
            let output = assemble(triple, instruction, Path::new("/dev/null"));
            assert!(
                output.status.success(),
                "{triple}: {instruction}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
