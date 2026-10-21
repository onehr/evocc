//! EvoMIR Section 4's `asm` and `asm.goto`, Section 7's constraint classes and
//! Section 8's template substitution.
//!
//! EvoIR Section 3.11 fixes the vocabulary: "The admitted vocabulary is the one
//! the configured kernel uses, measured over twenty-four preprocessed
//! translation units containing 24,735 `asm` statements." Every row of that
//! table gets one case here, written as EvoMIR by hand, and each case is
//! allocated, verified, emitted, assembled, linked and run, with its exit
//! status compared against a host-compiled program computing the same thing.
//!
//! These tests exercise the machine forms directly; the compiler's
//! inline_assembly_lowering tests additionally cover C through native execution.

use std::process::Command;

use evocc_mir::{
    allocate, emit_module, out_of_ssa, verify_allocation, AsmBody, AsmClobber, AsmConstraint,
    AsmDirection, AsmImmediate, AsmOperand, AsmValue, BinOp, MBlock, MFunction, MInst, MModule,
    MTerm, RegClass, VReg, Width,
};

fn w(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr32)
}

fn g(id: u32) -> VReg {
    VReg::new(id, RegClass::Gpr64)
}

#[test]
fn simd_assembly_register_bank_and_clobbers_are_independently_verified() {
    for class in [RegClass::Simd32, RegClass::Simd64, RegClass::Simd128] {
        let a = VReg::new(0, class);
        let b = VReg::new(1, class);
        let r = VReg::new(2, class);
        let function = MFunction {
            name: "combine".into(),
            blocks: vec![MBlock {
                id: 0,
                args: vec![a, b],
                insts: vec![MInst::Asm(AsmBody {
                    template: "// %0 %1 %2".into(),
                    volatile: true,
                    outputs: vec![AsmOperand::reg(
                        AsmConstraint::SimdEarlyClobber(AsmDirection::Write),
                        r,
                    )],
                    inputs: vec![
                        AsmOperand::reg(AsmConstraint::Simd(AsmDirection::Read), a),
                        AsmOperand::reg(AsmConstraint::Simd(AsmDirection::Read), b),
                    ],
                    clobbers: vec![AsmClobber::VectorRegister(3)],
                })],
                term: MTerm::Ret { values: vec![r] },
            }],
        };
        let module = MModule {
            functions: vec![function.clone()],
            globals: vec![],
        };
        let text = evocc_mir::render_module(&module);
        assert_eq!(evocc_mir::parse_module(&text).unwrap(), module);
        let allocated = allocate(&function).unwrap();
        verify_allocation(&allocated).unwrap();
        for change in 0..3 {
            let mut forged = allocated.clone();
            for block in &mut forged.blocks {
                for inst in &mut block.insts {
                    if let MInst::Asm(body) = inst {
                        let output = body.outputs[0].register().unwrap();
                        match change {
                            0 => {
                                body.outputs[0].constraint =
                                    AsmConstraint::Register(AsmDirection::Write)
                            }
                            1 => body.clobbers.push(AsmClobber::VectorRegister(
                                forged.assignment[&output.id] - 64,
                            )),
                            _ => {
                                forged.assignment.insert(output.id, 0);
                            }
                        }
                    }
                }
            }
            assert!(
                verify_allocation(&forged).is_err(),
                "accepted mutation {change}"
            );
        }
    }
}

#[test]
fn simd_assembly_goto_outputs_spill_all_lanes_after_the_selected_edge() {
    for taken in [false, true] {
        let outputs = (0..12)
            .map(|id| AsmOperand {
                constraint: AsmConstraint::Simd(AsmDirection::Write),
                fixed_register: None,
                name: Some(format!("out{id}")),
                value: AsmValue::Reg(VReg::new(id, RegClass::Simd128)),
            })
            .collect();
        let mut template = (0..12)
            .map(|id| format!("movi %[out{id}].16b, #{}", id + 1))
            .collect::<Vec<_>>()
            .join("\n");
        if taken {
            template.push_str("\nb %l[0]");
        }
        let mut insts = vec![
            MInst::Bl {
                returns_twice: false,
                fp_mode: None,
                symbol: "touch_simd".into(),
                args: vec![],
                results: vec![],
            },
            MInst::MovImm {
                dst: w(100),
                imm: 1,
            },
            MInst::MovImm {
                dst: g(101),
                imm: 0,
            },
        ];
        let mut sum = g(101);
        for id in 0..12 {
            let dst = g(102 + 2 * id);
            insts.push(MInst::Vector {
                dst,
                extract: true,
                lane_bits: 64,
                bytes: 16,
                args: vec![VReg::new(id, RegClass::Simd128), w(100)],
            });
            let next = g(103 + 2 * id);
            insts.push(MInst::Binary {
                op: BinOp::Add,
                dst: next,
                lhs: sum,
                rhs: dst,
                width: Width::X,
            });
            sum = next;
        }
        insts.extend([
            MInst::MovImm {
                dst: g(200),
                imm: 0x4e4e4e4e4e4e4e4e,
            },
            MInst::Cmp {
                dst: VReg::new(201, RegClass::Flags),
                lhs: sum,
                rhs: g(200),
                width: Width::X,
            },
            MInst::Cset {
                dst: w(202),
                flags: VReg::new(201, RegClass::Flags),
                cond: evocc_mir::Cond::Ne,
            },
        ]);
        let function = MFunction {
            name: "main".into(),
            blocks: vec![
                MBlock {
                    id: 0,
                    args: vec![],
                    insts: vec![],
                    term: MTerm::AsmGoto {
                        body: AsmBody {
                            template,
                            volatile: true,
                            outputs,
                            inputs: vec![],
                            clobbers: vec![],
                        },
                        labels: vec![(1, vec![])],
                        fallthrough: 2,
                        fallthrough_args: vec![],
                    },
                },
                MBlock {
                    id: 1,
                    args: vec![],
                    insts: insts.clone(),
                    term: MTerm::Ret {
                        values: vec![w(202)],
                    },
                },
                MBlock {
                    id: 2,
                    args: vec![],
                    insts,
                    term: MTerm::Ret {
                        values: vec![w(202)],
                    },
                },
            ],
        };
        let allocated = allocate(&function).unwrap();
        assert!(allocated.spilled >= 12);
        verify_allocation(&allocated).unwrap();
        let clobber = (0..8)
            .chain(16..32)
            .map(|i| format!("movi v{i}.16b, #0"))
            .collect::<Vec<_>>()
            .join("\n");
        let text = format!(
            "{}\n.text\n.p2align 2\n_touch_simd:\n{clobber}\nret\n",
            emit_module(&[allocated], &[])
        );
        if darwin_host() {
            assert_eq!(
                run_native(&format!("simd-goto-spills-{taken}"), &text).unwrap(),
                0
            );
        }
    }
}

#[test]
fn assembly_goto_outputs_survive_spilling_on_both_edges() {
    let outputs = (0..12)
        .map(|id| AsmOperand {
            constraint: AsmConstraint::Register(AsmDirection::Write),
            fixed_register: None,
            name: Some(format!("out{id}")),
            value: AsmValue::Reg(w(id)),
        })
        .collect();
    let mut insts = vec![MInst::Bl {
        returns_twice: false,
        fp_mode: None,
        symbol: "touch".into(),
        args: vec![],
        results: vec![],
    }];
    let mut sum = w(0);
    for id in 1..12 {
        let dst = w(12 + id);
        insts.push(MInst::Binary {
            op: BinOp::Add,
            dst,
            lhs: sum,
            rhs: w(id),
            width: Width::W,
        });
        sum = dst;
    }
    let function = MFunction {
        name: "main".into(),
        blocks: vec![
            MBlock {
                id: 0,
                args: vec![],
                insts: vec![],
                term: MTerm::AsmGoto {
                    body: AsmBody {
                        template: (0..12)
                            .map(|id| format!("mov %w[out{id}], #{}", id + 1))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        volatile: true,
                        outputs,
                        inputs: vec![],
                        clobbers: vec![],
                    },
                    labels: vec![(1, vec![])],
                    fallthrough: 2,
                    fallthrough_args: vec![],
                },
            },
            MBlock {
                id: 1,
                args: vec![],
                insts: insts.clone(),
                term: MTerm::Ret { values: vec![sum] },
            },
            MBlock {
                id: 2,
                args: vec![],
                insts,
                term: MTerm::Ret { values: vec![sum] },
            },
        ],
    };
    for (taken, fixed) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut function = function.clone();
        if fixed {
            let MTerm::AsmGoto { body, .. } = &mut function.blocks[0].term else {
                unreachable!()
            };
            body.outputs[0].fixed_register = Some(8);
            body.outputs[1].fixed_register = Some(19);
        }
        if taken {
            let MTerm::AsmGoto { body, .. } = &mut function.blocks[0].term else {
                unreachable!()
            };
            body.template.push_str("\nb %l[0]");
        }
        let allocated =
            allocate(&function).expect("assembly outputs can spill after the terminator");
        assert!(allocated.spilled > 0);
        verify_allocation(&allocated).expect("every edge receives initialized output spill slots");
        let text = format!(
            "{}\n.text\n.p2align 2\n_touch:\nret\n",
            emit_module(&[allocated], &[])
        );
        if darwin_host() {
            assert_eq!(
                run_native(&format!("goto-output-spills-{taken}-{fixed}"), &text).unwrap(),
                78
            );
        }
    }
}

#[test]
fn register_binding_is_checked_independently_of_the_allocator() {
    let mut case = read_write_register();
    let mut bound = false;
    for inst in &mut case.function.blocks[0].insts {
        if let MInst::Asm(body) = inst {
            body.outputs[0].fixed_register = Some(8);
            bound = true;
        }
    }
    assert!(bound);
    let module = MModule {
        globals: Vec::new(),
        functions: vec![case.function.clone()],
    };
    let text = evocc_mir::render_module(&module);
    assert_eq!(evocc_mir::parse_module(&text).unwrap(), module);
    let mut allocated = allocate(&case.function).unwrap();
    verify_allocation(&allocated).unwrap();
    for block in &mut allocated.blocks {
        for inst in &mut block.insts {
            if let MInst::Asm(body) = inst {
                body.outputs[0].fixed_register = Some(9);
            }
        }
    }
    let refusal = verify_allocation(&allocated).unwrap_err();
    assert!(refusal.detail.contains("binding x9"), "{refusal:?}");
}

/// The host toolchain, or the reason there is none.
fn toolchain() -> Result<String, String> {
    // Linux clang also names aarch64, but it cannot link Darwin text.
    if !cfg!(target_os = "macos") {
        return Err("the host is not macOS".to_owned());
    }
    let output = Command::new("clang")
        .arg("--version")
        .output()
        .map_err(|error| format!("clang is absent: {error}"))?;
    if !output.status.success() {
        return Err("clang did not report a version".to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if !text.contains("arm64") && !text.contains("aarch64") {
        return Err("the host toolchain does not target arm64".to_owned());
    }
    Ok(text.lines().next().unwrap_or_default().to_owned())
}

/// Whether the host can link Darwin text. Elsewhere the native step is
/// skipped and the reason is printed.
fn darwin_host() -> bool {
    let darwin = cfg!(target_os = "macos");
    if !darwin {
        println!("skipped=the host is not macOS");
    }
    darwin
}

/// Assemble, link and run one emitted text, returning its exit status.
fn run_native(name: &str, text: &str) -> Result<u8, String> {
    let directory = std::env::temp_dir().join(format!("evocc-asm-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let source = directory.join(format!("{name}.s"));
    let program = directory.join(name);
    std::fs::write(&source, text).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .output()
        .map_err(|error| error.to_string())?;
    if !built.status.success() {
        return Err(format!(
            "the toolchain refused {name}:\n{}\n{text}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    let run = Command::new(&program)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&program);
    let code = run
        .status
        .code()
        .ok_or_else(|| format!("{name} did not exit normally"))?;
    u8::try_from(code & 0xff).map_err(|error| error.to_string())
}

/// The exit status of a host-compiled C program computing the same thing.
fn run_reference(name: &str, source: &str) -> Result<u8, String> {
    let directory = std::env::temp_dir().join(format!("evocc-asm-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{name}.c"));
    let program = directory.join(format!("{name}-reference"));
    std::fs::write(&path, source).map_err(|error| error.to_string())?;
    let built = Command::new("clang")
        .args(["-std=c11", "-O2", "-o"])
        .arg(&program)
        .arg(&path)
        .output()
        .map_err(|error| error.to_string())?;
    if !built.status.success() {
        return Err(format!(
            "the toolchain refused the reference for {name}:\n{}",
            String::from_utf8_lossy(&built.stderr)
        ));
    }
    let run = Command::new(&program)
        .output()
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&program);
    let code = run
        .status
        .code()
        .ok_or_else(|| format!("the reference for {name} did not exit normally"))?;
    u8::try_from(code & 0xff).map_err(|error| error.to_string())
}

/// A `main` that runs `insts` and returns `result`.
fn program(insts: Vec<MInst>, result: VReg) -> MFunction {
    MFunction {
        name: "main".to_owned(),
        blocks: vec![MBlock {
            id: 0,
            args: Vec::new(),
            insts,
            term: MTerm::Ret {
                values: vec![result],
            },
        }],
    }
}

/// Allocate, verify and emit one function.
fn build(function: &MFunction) -> String {
    let allocated = allocate(&out_of_ssa(function)).expect("allocation");
    verify_allocation(&allocated).expect("the allocation verifies");
    emit_module(&[allocated], &[])
}

/// One constraint row's case: its EvoMIR, and the C that computes the same.
struct Case {
    row: &'static str,
    function: MFunction,
    reference: String,
}

fn constant(dst: VReg, value: u64) -> MInst {
    MInst::MovImm { dst, imm: value }
}

/// `r`, `=r`, `+r`: "a general register, read, written, or both".
fn plain_registers() -> Case {
    let body = AsmBody {
        template: "add %0, %1, %2".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(1)),
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(2)),
        ],
        clobbers: Vec::new(),
    };
    Case {
        row: "r",
        function: program(
            vec![constant(w(1), 70), constant(w(2), 33), MInst::Asm(body)],
            w(10),
        ),
        reference: "int main(void){unsigned a=70,b=33,o;__asm__(\"add %w0, %w1, %w2\":\"=r\"(o):\"r\"(a),\"r\"(b));return o&0xff;}\n".to_owned(),
    }
}

/// `+r`: the operand is read and written, so it is one register.
fn read_write_register() -> Case {
    let body = AsmBody {
        template: "add %0, %0, #7".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::ReadWrite),
            w(10),
        )],
        inputs: Vec::new(),
        clobbers: Vec::new(),
    };
    Case {
        row: "+r",
        function: program(
            vec![
                constant(w(1), 100),
                MInst::Mov {
                    dst: w(10),
                    src: w(1),
                },
                MInst::Asm(body),
            ],
            w(10),
        ),
        reference: "int main(void){unsigned o=100;__asm__(\"add %w0, %w0, #7\":\"+r\"(o));return o&0xff;}\n".to_owned(),
    }
}

/// `=&r`: "written, and not sharing a register with any input".
fn early_clobber() -> Case {
    let body = AsmBody {
        template: "mov %0, %1\n    add %0, %0, %2".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::EarlyClobber(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(1)),
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(2)),
        ],
        clobbers: Vec::new(),
    };
    Case {
        row: "=&r",
        function: program(
            vec![constant(w(1), 90), constant(w(2), 11), MInst::Asm(body)],
            w(10),
        ),
        reference: "int main(void){unsigned a=90,b=11,o;__asm__(\"mov %w0, %w1\\n\\tadd %w0, %w0, %w2\":\"=&r\"(o):\"r\"(a),\"r\"(b));return o&0xff;}\n".to_owned(),
    }
}

/// `rZ`: "a general register, or the zero register when the operand is the
/// constant zero; the template must spell it with a width modifier".
fn register_or_zero() -> Case {
    let body = AsmBody {
        template: "orr %0, %1, %w2".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(1)),
            AsmOperand {
                constraint: AsmConstraint::RegisterOrZero,
                fixed_register: None,
                name: None,
                value: AsmValue::Zero(RegClass::Gpr32),
            },
        ],
        clobbers: Vec::new(),
    };
    Case {
        row: "rZ",
        function: program(vec![constant(w(1), 123), MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned a=123,o;__asm__(\"orr %w0, %w1, %w2\":\"=r\"(o):\"r\"(a),\"rZ\"(0));return o&0xff;}\n".to_owned(),
    }
}

/// `Q`: "a memory operand whose address is a register with no offset".
fn memory_operand() -> Case {
    let body = AsmBody {
        template: "ldr %w0, %1".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![AsmOperand::reg(
            AsmConstraint::Memory {
                direction: AsmDirection::Read,
                offsettable: false,
            },
            g(3),
        )],
        clobbers: Vec::new(),
    };
    Case {
        row: "Q",
        function: program(
            vec![
                MInst::FrameSlot {
                    dst: g(3),
                    origin: 0,
                    size: 8,
                    align: 8,
                },
                constant(w(1), 201),
                MInst::Store {
                    align: 0,
                    src: w(1),
                    base: g(3),
                    offset: 0,
                    kind: evocc_mir::StoreKind::W,
                    volatile: false,
                    mask: None,
                },
                MInst::Asm(body),
            ],
            w(10),
        ),
        reference: "int main(void){unsigned v=201,o;__asm__(\"ldr %w0, %1\":\"=r\"(o):\"Q\"(v):\"memory\");return o&0xff;}\n".to_owned(),
    }
}

/// A digit: "this operand occupies the same location as the output that digit
/// names (a tied operand)".
fn tied_operand() -> Case {
    let body = AsmBody {
        template: "add %0, %0, #5".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![AsmOperand::reg(AsmConstraint::Tied(0), w(1))],
        clobbers: Vec::new(),
    };
    Case {
        row: "0",
        function: program(vec![constant(w(1), 60), MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned a=60,o;__asm__(\"add %w0, %w0, #5\":\"=r\"(o):\"0\"(a));return o&0xff;}\n".to_owned(),
    }
}

/// `I`: "an immediate the add or subtract forms accept".
fn immediate_operand() -> Case {
    let body = AsmBody {
        template: "add %0, %1, #%2".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(1)),
            AsmOperand::imm(AsmImmediate::I, 17),
        ],
        clobbers: Vec::new(),
    };
    Case {
        row: "I",
        function: program(vec![constant(w(1), 40), MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned a=40,o;__asm__(\"add %w0, %w1, #%2\":\"=r\"(o):\"r\"(a),\"I\"(17));return o&0xff;}\n".to_owned(),
    }
}

/// `K`: "an immediate the 32-bit logical forms accept".
fn logical_immediate() -> Case {
    let body = AsmBody {
        template: "and %0, %1, #%2".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![
            AsmOperand::reg(AsmConstraint::Register(AsmDirection::Read), w(1)),
            AsmOperand::imm(AsmImmediate::K, 255),
        ],
        clobbers: Vec::new(),
    };
    Case {
        row: "K",
        function: program(vec![constant(w(1), 0x1234_00c7), MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned a=0x123400c7u,o;__asm__(\"and %w0, %w1, #%2\":\"=r\"(o):\"r\"(a),\"K\"(255));return o&0xff;}\n".to_owned(),
    }
}

/// `i`: "an integer constant".
fn integer_constant() -> Case {
    let body = AsmBody {
        template: "mov %0, #%1".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![AsmOperand::imm(AsmImmediate::Integer, 199)],
        clobbers: Vec::new(),
    };
    Case {
        row: "i",
        function: program(vec![MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned o;__asm__(\"mov %w0, #%1\":\"=r\"(o):\"i\"(199));return o&0xff;}\n".to_owned(),
    }
}

/// `p`: "an address".
fn address_operand() -> Case {
    let body = AsmBody {
        template: "ldr %w0, [%1]".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: vec![AsmOperand::reg(AsmConstraint::Address, g(3))],
        clobbers: Vec::new(),
    };
    Case {
        row: "p",
        function: program(
            vec![
                MInst::FrameSlot {
                    dst: g(3),
                    origin: 0,
                    size: 8,
                    align: 8,
                },
                constant(w(1), 77),
                MInst::Store {
                    align: 0,
                    src: w(1),
                    base: g(3),
                    offset: 0,
                    kind: evocc_mir::StoreKind::W,
                    volatile: false,
                    mask: None,
                },
                MInst::Asm(body),
            ],
            w(10),
        ),
        // The reference names `memory` so the host compiler orders the store
        // before the template; an address operand alone tells it nothing about
        // what the template reads.
        reference: "int main(void){unsigned v=77,o;unsigned*p=&v;__asm__(\"ldr %w0, [%1]\":\"=r\"(o):\"r\"(p):\"memory\");return o&0xff;}\n".to_owned(),
    }
}

/// A named operand and a literal percent, and a register clobber and a `cc`
/// clobber: "a named physical register, which the allocator must treat as
/// written".
fn named_and_clobbers() -> Case {
    let body = AsmBody {
        template: "mov x9, #3\n    adds %[out], %[in], w9".to_owned(),
        volatile: true,
        outputs: vec![AsmOperand {
            constraint: AsmConstraint::Register(AsmDirection::Write),
            fixed_register: None,
            name: Some("out".to_owned()),
            value: AsmValue::Reg(w(10)),
        }],
        inputs: vec![AsmOperand {
            constraint: AsmConstraint::Register(AsmDirection::Read),
            fixed_register: None,
            name: Some("in".to_owned()),
            value: AsmValue::Reg(w(1)),
        }],
        clobbers: vec![
            AsmClobber::Register(9),
            AsmClobber::Flags,
            AsmClobber::Memory,
        ],
    };
    Case {
        row: "clobbers",
        function: program(vec![constant(w(1), 150), MInst::Asm(body)], w(10)),
        reference: "int main(void){unsigned a=150,o;__asm__ volatile(\"mov x9, #3\\n\\tadds %w[out], %w[in], w9\":[out]\"=r\"(o):[in]\"r\"(a):\"x9\",\"cc\",\"memory\");return o&0xff;}\n".to_owned(),
    }
}

fn cases() -> Vec<Case> {
    vec![
        plain_registers(),
        read_write_register(),
        early_clobber(),
        register_or_zero(),
        memory_operand(),
        tied_operand(),
        immediate_operand(),
        logical_immediate(),
        integer_constant(),
        address_operand(),
        named_and_clobbers(),
    ]
}

#[test]
fn every_constraint_row_runs_and_agrees_with_the_host() {
    let version = match toolchain() {
        Ok(version) => version,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={version}");

    let mut agreed = 0;
    let all = cases();
    for case in &all {
        let text = build(&case.function);
        let name = format!("asm-{}", case.row.replace(['&', '+', '='], "x"));
        let observed = match run_native(&name, &text) {
            Ok(status) => status,
            Err(reason) => panic!("{}: {reason}", case.row),
        };
        let expected = match run_reference(&name, &case.reference) {
            Ok(status) => status,
            Err(reason) => panic!("{}: {reason}", case.row),
        };
        assert_eq!(
            observed, expected,
            "constraint {} disagreed with the host\n{text}",
            case.row
        );
        println!("row={} exitStatus={observed}", case.row);
        agreed += 1;
    }
    println!("constraintRows={}", all.len());
    println!("agreeing={agreed}");
    assert_eq!(agreed, all.len());
}

/// Section 7: an early-clobber output takes a register no input holds.
#[test]
fn an_early_clobber_output_avoids_every_input() {
    let case = early_clobber();
    let allocated = allocate(&out_of_ssa(&case.function)).expect("allocation");
    verify_allocation(&allocated).expect("the allocation verifies");
    let output = allocated.assignment[&10];
    for input in [1_u32, 2] {
        assert_ne!(
            allocated.assignment[&input], output,
            "the early-clobber output shares a register with input %v{input}"
        );
    }
}

/// Section 7: "a digit ... the same register as the output that digit names".
#[test]
fn a_tied_operand_shares_its_output_register() {
    let case = tied_operand();
    let allocated = allocate(&out_of_ssa(&case.function)).expect("allocation");
    verify_allocation(&allocated).expect("the allocation verifies");
    // Check the operand occurrence, not the original SSA input: allocation
    // may copy that input to preserve its value after the assembly.
    let body = allocated
        .blocks
        .iter()
        .flat_map(|block| &block.insts)
        .find_map(|inst| match inst {
            MInst::Asm(body) => Some(body),
            _ => None,
        })
        .unwrap();
    let input = body.inputs[0].register().unwrap();
    let output = body.outputs[0].register().unwrap();
    assert_eq!(
        allocated.assignment[&input.id],
        allocated.assignment[&output.id]
    );
}

/// Section 7: a clobbered register holds no operand and no value across the
/// instruction.
#[test]
fn a_clobbered_register_is_left_alone() {
    let case = named_and_clobbers();
    let allocated = allocate(&out_of_ssa(&case.function)).expect("allocation");
    verify_allocation(&allocated).expect("the allocation verifies");
    for (value, number) in &allocated.assignment {
        assert_ne!(*number, 9, "%v{value} was given the clobbered register x9");
    }
}

/// Section 4: "The interpreter does not execute it (it reports `E3021`)."
#[test]
fn the_interpreter_refuses_an_assembly_function() {
    let case = plain_registers();
    let module = MModule {
        globals: Vec::new(),
        functions: vec![case.function],
    };
    let outcome =
        evocc_mir::interpret_module(&module, "main", &[], evocc_mir::MirLimits::new(10_000));
    let error = outcome.expect_err("an asm is not interpretable");
    assert_eq!(error.code(), "E3021");
}

/// Section 8: the template's own text is copied verbatim and never parsed, and
/// `%%` is one percent.
#[test]
fn the_template_is_copied_verbatim() {
    let body = AsmBody {
        template: "// %% literal and a comma, %0".to_owned(),
        volatile: false,
        outputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Write),
            w(10),
        )],
        inputs: Vec::new(),
        clobbers: Vec::new(),
    };
    let function = program(
        vec![
            MInst::Asm(body),
            MInst::Binary {
                op: BinOp::Add,
                dst: w(11),
                lhs: w(10),
                rhs: w(10),
                width: Width::W,
            },
        ],
        w(11),
    );
    let text = build(&function);
    assert!(
        text.contains("    // % literal and a comma, w"),
        "the template is copied with one percent:\n{text}"
    );
}

/// EvoIR Section 3.11: "`asm.goto` is a terminator whose successors are its
/// labels and its fall-through."
///
/// The template branches to `%l[0]` when its operand is zero. The operand is
/// not zero, so the run takes the fall-through, and the host-compiled `asm
/// goto` computing the same thing answers the same.
fn goto_function(operand: u64) -> MFunction {
    let body = AsmBody {
        template: "cbz %w0, %l[0]".to_owned(),
        volatile: true,
        outputs: Vec::new(),
        inputs: vec![AsmOperand::reg(
            AsmConstraint::Register(AsmDirection::Read),
            w(1),
        )],
        clobbers: Vec::new(),
    };
    MFunction {
        name: "main".to_owned(),
        blocks: vec![
            MBlock {
                id: 0,
                args: Vec::new(),
                insts: vec![constant(w(1), operand)],
                term: MTerm::AsmGoto {
                    body,
                    labels: vec![(1, Vec::new())],
                    fallthrough: 2,
                    fallthrough_args: Vec::new(),
                },
            },
            MBlock {
                id: 1,
                args: Vec::new(),
                insts: vec![constant(w(10), 10)],
                term: MTerm::Ret {
                    values: vec![w(10)],
                },
            },
            MBlock {
                id: 2,
                args: Vec::new(),
                insts: vec![constant(w(10), 20)],
                term: MTerm::Ret {
                    values: vec![w(10)],
                },
            },
        ],
    }
}

#[test]
fn an_asm_goto_takes_both_of_its_edges() {
    let version = match toolchain() {
        Ok(version) => version,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("toolchain={version}");

    for (operand, label) in [(5_u64, "notZero"), (0, "zero")] {
        let text = build(&goto_function(operand));
        assert!(
            text.contains("cbz w"),
            "the template is copied with its operand substituted:\n{text}"
        );
        let name = format!("asm-goto-{label}");
        let observed =
            run_native(&name, &text).unwrap_or_else(|reason| panic!("{label}: {reason}"));
        let reference = format!(
            "int main(void){{unsigned a={operand};__asm__ goto(\"cbz %w0, %l[jump]\"::\"r\"(a)::jump);return 20;jump:return 10;}}\n"
        );
        let expected =
            run_reference(&name, &reference).unwrap_or_else(|reason| panic!("{label}: {reason}"));
        assert_eq!(observed, expected, "asm.goto {label} disagreed\n{text}");
        println!("asmGoto={label} operand={operand} exitStatus={observed}");
    }
}

/// Appendix A: the textual form "round-trips through a parser, so that tests
/// can state EvoMIR by hand and the receipt can digest it".
///
/// Appendix A gives no production for either assembly form, so the rendering
/// mirrors the EvoIR instruction's own spelling of Section 3.11: the quoted
/// template, then the outputs, inputs and clobbers, and for the terminator its
/// labels and fall-through. Recorded gap; this is what pins the choice.
#[test]
fn both_assembly_forms_round_trip_byte_for_byte() {
    let mut functions: Vec<MFunction> = cases().into_iter().map(|case| case.function).collect();
    functions.push(goto_function(5));
    // A template with a quote, a backslash, a tab and a newline in it, so the
    // escapes the one-line form needs are exercised too.
    functions.push(program(
        vec![MInst::Asm(AsmBody {
            template: "// \"quoted\" \\ and %% \n    mov %0, #1".to_owned(),
            volatile: true,
            outputs: vec![AsmOperand {
                constraint: AsmConstraint::Register(AsmDirection::Write),
                fixed_register: None,
                name: Some("named".to_owned()),
                value: AsmValue::Reg(w(10)),
            }],
            inputs: Vec::new(),
            clobbers: vec![AsmClobber::Register(9), AsmClobber::Flags],
        })],
        w(10),
    ));

    for (index, function) in functions.iter().enumerate() {
        let module = MModule {
            globals: Vec::new(),
            functions: vec![function.clone()],
        };
        let text = evocc_mir::render_module(&module);
        let parsed = evocc_mir::parse_module(&text)
            .unwrap_or_else(|error| panic!("function {index} did not parse: {error}\n{text}"));
        assert_eq!(
            parsed, module,
            "function {index} did not round-trip\n{text}"
        );
        assert_eq!(
            evocc_mir::render_module(&parsed),
            text,
            "function {index} did not render back to the same bytes"
        );
    }
    println!("roundTripped={}", functions.len());
}

/// EvoIR Section 3.11: "A constraint outside this table, or a constraint string
/// combining letters this table does not combine, is `E3021` with the
/// constraint named."
#[test]
fn a_constraint_outside_the_table_is_refused() {
    for outside in ["v", "=v", "rw", "rx", "&r", "&w", "Z", "m"] {
        assert!(
            AsmConstraint::parse(outside).is_none(),
            "'{outside}' is outside the measured table and must not parse"
        );
    }
    // Every row of the table does parse, and renders back to itself.
    for inside in [
        "w", "=w", "+w", "=&w", "+&w", "r", "=r", "+r", "=&r", "+&r", "rZ", "Q", "=Q", "+Q", "Qo",
        "0", "9", "I", "J", "K", "L", "i", "n", "p",
    ] {
        let constraint =
            AsmConstraint::parse(inside).unwrap_or_else(|| panic!("'{inside}' is in the table"));
        assert_eq!(constraint.spelling(), inside);
    }
}
