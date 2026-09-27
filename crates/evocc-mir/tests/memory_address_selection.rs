use evocc_evoir::parse_section9_module;
use evocc_mir::{MInst, Target};

fn source(
    bits: u16,
    value: u128,
    sign: &str,
    direction: &str,
    scale: u64,
    align: u32,
    barrier: bool,
) -> String {
    let barrier = if barrier {
        "    %v10 = pointer_valid %v5, %v0\n    %v11 = access_in_bounds %v10, %v0 [size=8, align=8]\n    (%v12, %v13, %v14) = load volatile %v11, %v2, %v0 : bits<64>\n"
    } else {
        ""
    };
    let (mem, world) = if barrier.is_empty() { (5, 2) } else { (13, 14) };
    format!("func @read(%arg0: ptr<generic>, %arg1: mem<all>, %arg2: world) -> (bits<64>, mem<all>, world) cc = target_default
    effects {{ reads={{all}}, writes={{}}, volatile=true, atomic=false, io=false, fenv=false, may_trap=true, may_unwind=false }}
{{
^entry(%v0: ptr<generic>, %v1: mem<all>, %v2: world):
    %v3 = const.bits<{bits}> {value}
    %v4 = ptr.offset.{direction}.{sign} %v0, %v3 [scale={scale}]
    %v5 = pointer_valid %v1, %v4
{barrier}    %v6 = access_in_bounds %v{mem}, %v4 [size=8, align={align}]
    (%v7, %v8, %v9) = load volatile %v6, %v{world}, %v4 : bits<64>
    ret %v7, %v8, %v9
}}
")
}

fn lower(text: &str) -> evocc_mir::MModule {
    let ir = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&ir).unwrap();
    let mir = evocc_mir::lower_module_for(Target::Linux, &ir, &|_| false).unwrap();
    evocc_mir::validate_module(&mir).unwrap();
    assert_eq!(
        evocc_mir::parse_module(&evocc_mir::render_module(&mir)).unwrap(),
        mir
    );
    for function in &mir.functions {
        evocc_mir::verify_allocation(
            &evocc_mir::allocate_for(Target::Linux, &evocc_mir::out_of_ssa(function)).unwrap(),
        )
        .unwrap();
    }
    mir
}

fn offsets(mir: &evocc_mir::MModule) -> Vec<i64> {
    mir.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.insts)
        .filter_map(|i| match i {
            MInst::Load {
                offset, volatile, ..
            } => {
                assert!(*volatile);
                Some(*offset)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn decode_indices_before_selecting_scaled_or_unscaled_memory_immediates() {
    for (bits, value, sign, direction, scale, align, expected) in [
        (64, 8, "unsigned", "add", 1, 8, 8),
        (32, 0xffff_ffff, "signed", "add", 8, 8, -8),
        (8, 255, "signed", "sub", 2, 8, 2),
        (64, 4095, "unsigned", "add", 8, 8, 32760),
        (64, 4096, "unsigned", "add", 8, 8, 0),
        (64, 256, "unsigned", "sub", 1, 8, -256),
        (64, 257, "unsigned", "sub", 1, 8, 0),
        (64, 4088, "unsigned", "add", 1, 1, 4088),
        (64, 4096, "unsigned", "add", 1, 1, 0),
        (64, u128::from(u64::MAX), "unsigned", "add", 1, 8, 0),
        (64, u128::from(u64::MAX), "unsigned", "add", u64::MAX, 8, 0),
        (128, 8, "unsigned", "add", 1, 8, 0),
    ] {
        let text = source(bits, value, sign, direction, scale, align, false);
        assert_eq!(offsets(&lower(&text)), [expected], "{text}");
    }
}

#[test]
fn an_intervening_observable_access_keeps_pointer_formation_in_place() {
    let text = source(64, 8, "unsigned", "add", 1, 8, true);
    assert_eq!(offsets(&lower(&text)), [0, 0]);
}

#[test]
fn access_width_alignment_and_representation_are_not_changed() {
    for (ty, size) in [
        ("bits<8>", 1),
        ("bits<16>", 2),
        ("bits<32>", 4),
        ("bits<64>", 8),
        ("f32", 4),
        ("f64", 8),
        ("ptr<generic>", 8),
        ("vec<4, bits<32>>", 16),
    ] {
        for align in [1, size] {
            let text = source(64, 32, "unsigned", "add", 1, 8, false)
                .replace("-> (bits<64>,", &format!("-> ({ty},"))
                .replace(": bits<64>", &format!(": {ty}"))
                .replace(
                    "[size=8, align=8]",
                    &format!("[size={size}, align={align}]"),
                );
            let mir = lower(&text);
            let loads: Vec<_> = mir.functions[0]
                .blocks
                .iter()
                .flat_map(|b| &b.insts)
                .filter_map(|i| match i {
                    MInst::Load {
                        align,
                        kind,
                        offset,
                        ..
                    } => Some((*align, kind.size_bytes(), *offset)),
                    _ => None,
                })
                .collect();
            assert_eq!(
                loads,
                [(if align == size { 0 } else { align }, u64::from(size), 32)],
                "{text}"
            );
        }
    }
}

#[test]
fn a_guard_continuation_still_uses_its_explicit_address_parameter() {
    let text = source(64, 8, "unsigned", "add", 1, 8, false)
        .replace("    %v6 = access_in_bounds", "    %v15 = const.bits<1> 0\n    ub.assume.signed_overflow %v15\n    %v6 = access_in_bounds");
    let mir = lower(&text);
    assert_eq!(offsets(&mir), [0]);
    assert_eq!(
        evocc_mir::interpret_module(
            &mir,
            "read",
            &[evocc_mir::MirValue::Int(1024)],
            evocc_mir::MirLimits::new(100)
        )
        .unwrap_err(),
        evocc_mir::MirError::Trap { imm: 3004 }
    );
}

#[test]
fn selected_and_fallback_offsets_assemble_with_both_alignment_modes() {
    use std::io::Write;
    use std::process::{Command, Stdio};

    if Command::new("clang").arg("--version").output().is_err() {
        eprintln!("SKIP assembler check: clang is absent");
        return;
    }
    let mut texts = Vec::new();
    for (offset, direction, align) in [
        (255, "add", 8),
        (256, "sub", 8),
        (257, "sub", 8),
        (32760, "add", 8),
        (32768, "add", 8),
        (4088, "add", 1),
        (4096, "add", 1),
        (256, "sub", 1),
    ] {
        texts.push(source(64, offset, "unsigned", direction, 1, align, false));
    }
    for (ty, size) in [
        ("bits<8>", 1),
        ("bits<16>", 2),
        ("bits<32>", 4),
        ("bits<64>", 8),
        ("f32", 4),
        ("f64", 8),
        ("ptr<generic>", 8),
        ("vec<4, bits<32>>", 16),
    ] {
        for align in [1, size] {
            texts.push(
                source(64, 32, "unsigned", "add", 1, 8, false)
                    .replace("-> (bits<64>,", &format!("-> ({ty},"))
                    .replace(": bits<64>", &format!(": {ty}"))
                    .replace(
                        "[size=8, align=8]",
                        &format!("[size={size}, align={align}]"),
                    ),
            );
        }
    }
    let text = texts
        .into_iter()
        .enumerate()
        .map(|(index, text)| text.replace("@read(", &format!("@read_{index}(")))
        .collect::<Vec<_>>()
        .join("\n");
    let module = parse_section9_module(text.as_bytes()).unwrap();
    evocc_evoir::validate_module(&module).unwrap();
    for strict_alignment in [false, true] {
        let assembly = evocc_mir::compile_module_with_options(
            Target::Linux,
            &module,
            &|_| true,
            evocc_mir::RelocationModel::Static,
            &evocc_mir::emit::CodegenOptions {
                strict_alignment,
                ..Default::default()
            },
        )
        .unwrap();
        // Assemble through stdin to the null device: no persistent object or
        // private test directory is necessary for this encoding check.
        let mut child = Command::new("clang")
            .args([
                "--target=aarch64-linux-gnu",
                "-x",
                "assembler",
                "-c",
                "-",
                "-o",
                "/dev/null",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(assembly.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "strict_alignment={strict_alignment}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
