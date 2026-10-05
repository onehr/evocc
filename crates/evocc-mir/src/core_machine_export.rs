//! Faithful input export for the independent pre-allocation machine checker.
//!
//! The ordinary MIR text printer omits the explicit flags operand of BCond.
//! This boundary therefore serializes the actual AST, including register
//! classes on uses, branch arguments, widths, and memory attributes. It does
//! not normalize instructions or infer operands from adjacent instructions.
//! Export is not proof admission; the independent reader and pair checker
//! must check the resulting program and its relation to the actual input.

use std::fmt::Write;

use crate::{FlagOp, LoadKind, MInst, MModule, MTerm, RegClass, StoreKind, VReg, Width};

/// Which actual pre-allocation snapshot the caller supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMachineMode {
    Ssa,
    PostSsa,
}

enum Node {
    Text(String),
    Natural(u64),
    Integer(i64),
    Flag(bool),
    Null,
    Array(Vec<Node>),
}

impl Node {
    fn text(value: &str) -> Self {
        Self::Text(value.to_owned())
    }

    fn tagged(tag: &str, fields: impl IntoIterator<Item = Self>) -> Self {
        Self::Array(std::iter::once(Self::text(tag)).chain(fields).collect())
    }

    fn render(&self, out: &mut String) {
        match self {
            Self::Text(value) => {
                out.push('"');
                for character in value.chars() {
                    match character {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\u{0}'..='\u{1f}' => {
                            write!(out, "\\u{:04x}", u32::from(character)).unwrap();
                        }
                        other => out.push(other),
                    }
                }
                out.push('"');
            }
            Self::Natural(value) => write!(out, "{value}").unwrap(),
            Self::Integer(value) => write!(out, "{value}").unwrap(),
            Self::Flag(value) => out.push_str(if *value { "true" } else { "false" }),
            Self::Null => out.push_str("null"),
            Self::Array(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(',');
                    }
                    value.render(out);
                }
                out.push(']');
            }
        }
    }
}

type Result<T> = std::result::Result<T, String>;

fn reg(value: VReg) -> Result<Node> {
    let class = match value.class {
        RegClass::Gpr32 => "gpr32",
        RegClass::Gpr64 => "gpr64",
        RegClass::Flags => "flags",
        _ => return Err("machine snapshot contains a register outside the Core profile".into()),
    };
    Ok(Node::Array(vec![
        Node::Natural(u64::from(value.id)),
        Node::text(class),
    ]))
}

fn regs(values: &[VReg]) -> Result<Node> {
    Ok(Node::Array(
        values.iter().copied().map(reg).collect::<Result<_>>()?,
    ))
}

fn width(value: Width) -> Node {
    Node::text(value.mnemonic())
}

fn instruction(inst: &MInst) -> Result<Node> {
    Ok(match inst {
        MInst::Mov { dst, src } => Node::tagged("mov", [reg(*dst)?, reg(*src)?]),
        MInst::MovImm { dst, imm } => Node::tagged("mov_imm", [reg(*dst)?, Node::Natural(*imm)]),
        MInst::Binary {
            op,
            dst,
            lhs,
            rhs,
            width: w,
        } => Node::tagged(
            "binary",
            [
                Node::text(op.mnemonic()),
                reg(*dst)?,
                reg(*lhs)?,
                reg(*rhs)?,
                width(*w),
            ],
        ),
        MInst::BinaryImm {
            op,
            dst,
            lhs,
            imm,
            width: w,
        } => Node::tagged(
            "binary_imm",
            [
                Node::text(op.mnemonic()),
                reg(*dst)?,
                reg(*lhs)?,
                Node::Natural(*imm),
                width(*w),
            ],
        ),
        MInst::FlagBinary {
            op,
            dst,
            flags,
            lhs,
            rhs,
            width: w,
        } => Node::tagged(
            "flag_binary",
            [
                Node::text(match op {
                    FlagOp::Adds => "adds",
                    FlagOp::Subs => "subs",
                }),
                reg(*dst)?,
                reg(*flags)?,
                reg(*lhs)?,
                reg(*rhs)?,
                width(*w),
            ],
        ),
        MInst::WideMultiply { op, dst, lhs, rhs } => Node::tagged(
            "wide_multiply",
            [
                Node::text(op.mnemonic()),
                reg(*dst)?,
                reg(*lhs)?,
                reg(*rhs)?,
            ],
        ),
        MInst::Msub {
            dst,
            n,
            m,
            a,
            width: w,
        }
        | MInst::Madd {
            dst,
            n,
            m,
            a,
            width: w,
        } => Node::tagged(
            if matches!(inst, MInst::Msub { .. }) {
                "msub"
            } else {
                "madd"
            },
            [reg(*dst)?, reg(*n)?, reg(*m)?, reg(*a)?, width(*w)],
        ),
        MInst::Extend { op, dst, src } => Node::tagged(
            "extend",
            [Node::text(op.mnemonic()), reg(*dst)?, reg(*src)?],
        ),
        MInst::Bfx {
            signed,
            dst,
            src,
            lsb,
            bits,
            width: w,
        } => Node::tagged(
            "bfx",
            [
                Node::Flag(*signed),
                reg(*dst)?,
                reg(*src)?,
                Node::Natural(u64::from(*lsb)),
                Node::Natural(u64::from(*bits)),
                width(*w),
            ],
        ),
        MInst::Cmp {
            dst,
            lhs,
            rhs,
            width: w,
        } => Node::tagged("cmp", [reg(*dst)?, reg(*lhs)?, reg(*rhs)?, width(*w)]),
        MInst::CmpImm {
            dst,
            lhs,
            imm,
            width: w,
        }
        | MInst::TstImm {
            dst,
            lhs,
            imm,
            width: w,
        } => Node::tagged(
            if matches!(inst, MInst::CmpImm { .. }) {
                "cmp_imm"
            } else {
                "tst_imm"
            },
            [reg(*dst)?, reg(*lhs)?, Node::Natural(*imm), width(*w)],
        ),
        MInst::Cset { dst, flags, cond } => Node::tagged(
            "cset",
            [reg(*dst)?, reg(*flags)?, Node::text(cond.mnemonic())],
        ),
        MInst::Csel {
            dst,
            flags,
            lhs,
            rhs,
            cond,
            width: w,
        } => Node::tagged(
            "csel",
            [
                reg(*dst)?,
                reg(*flags)?,
                reg(*lhs)?,
                reg(*rhs)?,
                Node::text(cond.mnemonic()),
                width(*w),
            ],
        ),
        MInst::Load {
            align,
            dst,
            base,
            offset,
            kind,
            volatile,
        } => {
            let kind = match kind {
                LoadKind::W => "w",
                LoadKind::X => "x",
                _ => return Err("machine load outside the Core profile".into()),
            };
            Node::tagged(
                "load",
                [
                    Node::Natural(u64::from(*align)),
                    reg(*dst)?,
                    reg(*base)?,
                    Node::Integer(*offset),
                    Node::text(kind),
                    Node::Flag(*volatile),
                ],
            )
        }
        MInst::Store {
            align,
            src,
            base,
            offset,
            kind,
            volatile,
        } => {
            let kind = match kind {
                StoreKind::W => "w",
                StoreKind::X => "x",
                _ => return Err("machine store outside the Core profile".into()),
            };
            Node::tagged(
                "store",
                [
                    Node::Natural(u64::from(*align)),
                    reg(*src)?,
                    reg(*base)?,
                    Node::Integer(*offset),
                    Node::text(kind),
                    Node::Flag(*volatile),
                ],
            )
        }
        MInst::FrameSlot {
            dst,
            origin,
            size,
            align,
        } => Node::tagged(
            "frame_slot",
            [
                reg(*dst)?,
                Node::Natural(*origin),
                Node::Natural(*size),
                Node::Natural(u64::from(*align)),
            ],
        ),
        MInst::StackRelease { address } => Node::tagged("stack_release", [reg(*address)?]),
        MInst::Bl {
            symbol,
            args,
            results,
            fp_mode: None,
        } => Node::tagged(
            "bl",
            [Node::text(symbol), regs(args)?, regs(results)?, Node::Null],
        ),
        _ => return Err("machine instruction outside the Core profile".into()),
    })
}

fn terminator(term: &MTerm) -> Result<Node> {
    Ok(match term {
        MTerm::B { target, args } => {
            Node::tagged("b", [Node::Natural(u64::from(*target)), regs(args)?])
        }
        MTerm::BCond {
            flags,
            cond,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => Node::tagged(
            "b_cond",
            [
                reg(*flags)?,
                Node::text(cond.mnemonic()),
                Node::Natural(u64::from(*taken)),
                regs(taken_args)?,
                Node::Natural(u64::from(*untaken)),
                regs(untaken_args)?,
            ],
        ),
        MTerm::Cb {
            nonzero,
            src,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => Node::tagged(
            "cb",
            [
                Node::Flag(*nonzero),
                reg(*src)?,
                Node::Natural(u64::from(*taken)),
                regs(taken_args)?,
                Node::Natural(u64::from(*untaken)),
                regs(untaken_args)?,
            ],
        ),
        MTerm::Tb {
            nonzero,
            src,
            bit,
            taken,
            taken_args,
            untaken,
            untaken_args,
        } => Node::tagged(
            "tb",
            [
                Node::Flag(*nonzero),
                reg(*src)?,
                Node::Natural(u64::from(*bit)),
                Node::Natural(u64::from(*taken)),
                regs(taken_args)?,
                Node::Natural(u64::from(*untaken)),
                regs(untaken_args)?,
            ],
        ),
        MTerm::Ret { values } => Node::tagged("ret", [regs(values)?]),
        MTerm::Brk { imm } => Node::tagged("brk", [Node::Natural(u64::from(*imm))]),
        _ => return Err("machine terminator outside the Core profile".into()),
    })
}

/// Serialize all supplied functions, in order, without pruning unreachable
/// blocks or dead instructions. The caller must supply the actual checked
/// Linux LP64 snapshot; this function does not run or certify a transform.
pub fn export_core_machine(module: &MModule, mode: CoreMachineMode) -> Result<String> {
    if !module.globals.is_empty() {
        return Err("machine globals outside the Core profile".into());
    }
    let functions = module
        .functions
        .iter()
        .map(|function| {
            let blocks = function
                .blocks
                .iter()
                .map(|block| {
                    Ok(Node::Array(vec![
                        Node::Natural(u64::from(block.id)),
                        regs(&block.args)?,
                        Node::Array(block.insts.iter().map(instruction).collect::<Result<_>>()?),
                        terminator(&block.term)?,
                    ]))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(Node::Array(vec![
                Node::text(&function.name),
                Node::Array(blocks),
            ]))
        })
        .collect::<Result<Vec<_>>>()?;
    let root = Node::tagged(
        "core-mir-v1",
        [
            Node::text("aarch64-unknown-linux-gnu"),
            Node::text("checked"),
            Node::text(match mode {
                CoreMachineMode::Ssa => "ssa",
                CoreMachineMode::PostSsa => "post-ssa",
            }),
            Node::Array(functions),
        ],
    );
    let mut output = String::new();
    root.render(&mut output);
    output.push('\n');
    Ok(output)
}
