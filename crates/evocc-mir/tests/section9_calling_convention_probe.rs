//! Section 9's probe, and Section 6's condition on using the convention.
//!
//! Section 6: "Measured against the reference compilers before this revision is
//! first used for a call across the boundary (Section 9 names the probe)."
//! Section 9: "before the first cross-boundary call is emitted, a probe program
//! compiled by the host compiler calls EvoCC-compiled functions with each
//! argument shape this revision admits, and vice versa, and the receipt names
//! it."
//!
//! Each shape is one to eight arguments drawn from both width classes and from
//! the pointer class, with one `bits<32>` result, and each is exercised in both
//! directions: the host calls the EvoCC-compiled fold, and an EvoCC-compiled
//! forwarder calls the host's own fold with arguments it computed itself, so
//! that the register a narrow `bits<N>` travels in carries bits above `N` on
//! the way out as well as on the way in. Section 6's ninth register argument
//! stays refused, and the receipt says so.

use std::process::Command;

use evocc_evoir::{
    AccessLayout, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst, Module, Value,
};
use evocc_mir::Toolchain;
#[path = "support/frame_variants.rs"]
mod frame_variants;
use evocc_types::{AddressSpace, EvoIRType, Region};
use frame_variants::compile_module_with;

/// One argument's class, in the two spellings the probe needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shape {
    Bits(u16),
    Ptr,
}

impl Shape {
    fn ty(self) -> EvoIRType {
        match self {
            Self::Bits(width) => EvoIRType::Bits(width),
            Self::Ptr => EvoIRType::Ptr(AddressSpace::Generic),
        }
    }

    /// The C spelling a host compiler passes in the same register.
    const fn c_type(self) -> &'static str {
        match self {
            Self::Bits(1 | 8) => "uint8_t",
            Self::Bits(width) if width <= 16 => "uint16_t",
            Self::Bits(width) if width <= 32 => "uint32_t",
            Self::Bits(_) => "uint64_t",
            Self::Ptr => "const uint8_t *",
        }
    }

    /// The value `bits<N>` keeps of a register: Section 3's low `N` bits.
    const fn mask(self) -> u64 {
        match self {
            Self::Bits(width) if width < 64 => (1_u64 << width) - 1,
            _ => u64::MAX,
        }
    }

    const fn register_class(self) -> &'static str {
        match self {
            Self::Bits(width) if width <= 32 => "gpr32",
            _ => "gpr64",
        }
    }
}

/// The two argument patterns, each truncated to one through eight arguments.
///
/// Between them every width class this revision admits appears, and a pointer
/// appears at the first, third, fifth, seventh and eighth position.
const PATTERN_A: [Shape; 8] = [
    Shape::Bits(32),
    Shape::Bits(64),
    Shape::Ptr,
    Shape::Bits(8),
    Shape::Bits(16),
    Shape::Bits(1),
    Shape::Bits(47),
    Shape::Bits(13),
];
const PATTERN_B: [Shape; 8] = [
    Shape::Ptr,
    Shape::Bits(13),
    Shape::Bits(64),
    Shape::Bits(16),
    Shape::Ptr,
    Shape::Bits(32),
    Shape::Bits(8),
    Shape::Ptr,
];

/// The shapes that cross to a host function which does not mask its own
/// argument.
///
/// Every width here is exactly a C type's, so the value the host callee reads
/// is the value EvoIR passed only when the register carries the platform's
/// extension: revision 0.5's rule for the boundary, as against the internal
/// rule that leaves the bits above `N` unspecified. Widths with no C type of
/// their own stay in the folding families above, whose host side masks.
const PATTERN_C: [Shape; 8] = [
    Shape::Bits(8),
    Shape::Bits(16),
    Shape::Bits(32),
    Shape::Ptr,
    Shape::Bits(8),
    Shape::Bits(64),
    Shape::Bits(16),
    Shape::Bits(8),
];

/// The shapes whose host side does not mask, at every admitted arity.
fn bare_shapes() -> Vec<(String, Vec<Shape>)> {
    (1..=8_usize)
        .map(|count| (format!("c{count}"), PATTERN_C[..count].to_vec()))
        .collect()
}

/// Every shape the probe covers: both patterns at every admitted arity.
fn shapes() -> Vec<(String, Vec<Shape>)> {
    let mut all = Vec::new();
    for (letter, pattern) in [("a", PATTERN_A), ("b", PATTERN_B)] {
        for count in 1..=8_usize {
            all.push((format!("{letter}{count}"), pattern[..count].to_vec()));
        }
    }
    all
}

const MEM: fn() -> EvoIRType = || EvoIRType::Mem(Region::All);
const WORLD: fn() -> EvoIRType = || EvoIRType::World;

/// A fresh-value counter and the instruction list it fills.
struct Builder {
    next: u32,
    insts: Vec<Inst>,
}

impl Builder {
    fn new() -> Self {
        Self {
            next: 0,
            insts: Vec::new(),
        }
    }

    fn value(&mut self, ty: EvoIRType) -> Value {
        let id = self.next;
        self.next += 1;
        Value { id, ty }
    }

    fn push(&mut self, inst: Inst) {
        self.insts.push(inst);
    }
}

fn binder(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// The entry block's arguments: the shapes, then the two tokens.
fn entry_values(builder: &mut Builder, shapes: &[Shape]) -> (Vec<Value>, Value, Value) {
    let args: Vec<Value> = shapes
        .iter()
        .map(|shape| builder.value(shape.ty()))
        .collect();
    let mem = builder.value(MEM());
    let world = builder.value(WORLD());
    (args, mem, world)
}

fn params(shapes: &[Shape]) -> Vec<FunctionParam> {
    let mut params: Vec<FunctionParam> = shapes
        .iter()
        .enumerate()
        .map(|(index, shape)| FunctionParam {
            name: format!("arg{index}"),
            ty: shape.ty(),
        })
        .collect();
    params.push(FunctionParam {
        name: format!("arg{}", shapes.len()),
        ty: MEM(),
    });
    params.push(FunctionParam {
        name: format!("arg{}", shapes.len() + 1),
        ty: WORLD(),
    });
    params
}

fn return_types() -> Vec<EvoIRType> {
    vec![EvoIRType::Bits(32), MEM(), WORLD()]
}

/// The fold every direction of the probe computes.
///
/// `acc` starts at a constant and, for each argument in order, is multiplied by
/// a prime and mixed with the argument's `bits<32>` contribution: the low `N`
/// bits of an integer, and the byte a pointer addresses.
fn fold(
    builder: &mut Builder,
    shapes: &[Shape],
    args: &[Value],
    mut mem: Value,
    mut world: Value,
) -> (Value, Value, Value) {
    let bits32 = EvoIRType::Bits(32);
    let mut acc = builder.value(bits32.clone());
    builder.push(Inst::Const(acc.clone(), 0x811c_9dc5));
    for (shape, value) in shapes.iter().zip(args) {
        let contribution = match shape {
            Shape::Bits(32) => value.clone(),
            Shape::Bits(width) if *width < 32 => {
                let wide = builder.value(bits32.clone());
                builder.push(Inst::ZExt(wide.clone(), value.clone(), bits32.clone()));
                wide
            }
            Shape::Bits(_) => {
                let narrow = builder.value(bits32.clone());
                builder.push(Inst::Trunc(narrow.clone(), value.clone(), bits32.clone()));
                narrow
            }
            Shape::Ptr => {
                let guarded = builder.value(MEM());
                builder.push(Inst::AccessInBounds {
                    mem_in: mem.clone(),
                    pointer: value.clone(),
                    mem_out: guarded.clone(),
                    layout: AccessLayout::new(1, 1),
                });
                let byte = builder.value(EvoIRType::Bits(8));
                let mem_out = builder.value(MEM());
                let world_out = builder.value(WORLD());
                builder.push(Inst::Load {
                    result: byte.clone(),
                    address: value.clone(),
                    mem_in: guarded,
                    world_in: world.clone(),
                    mem_out: mem_out.clone(),
                    world_out: world_out.clone(),
                    volatile: false,
                });
                mem = mem_out;
                world = world_out;
                let wide = builder.value(bits32.clone());
                builder.push(Inst::ZExt(wide.clone(), byte, bits32.clone()));
                wide
            }
        };
        let prime = builder.value(bits32.clone());
        builder.push(Inst::Const(prime.clone(), 16_777_619));
        let scaled = builder.value(bits32.clone());
        builder.push(Inst::Mul(scaled.clone(), acc.clone(), prime));
        let mixed = builder.value(bits32.clone());
        builder.push(Inst::Xor(mixed.clone(), scaled, contribution));
        acc = mixed;
    }
    (acc, mem, world)
}

/// A function that folds its arguments and returns the result.
fn fold_function(name: &str, shapes: &[Shape]) -> Function {
    let mut builder = Builder::new();
    let (args, mem, world) = entry_values(&mut builder, shapes);
    let block_args: Vec<BlockArg> = args
        .iter()
        .chain([&mem, &world])
        .map(binder)
        .collect::<Vec<_>>();
    let (acc, mem_out, world_out) = fold(&mut builder, shapes, &args, mem, world);
    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: name.to_owned(),
        params: params(shapes),
        ret_tys: return_types(),
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: block_args,
            insts: builder.insts,
            terminator: Inst::Ret(vec![acc, mem_out, world_out]),
        }],
        param_marks: Vec::new(),
    }
}

/// A function that negates each integer argument at its own width and calls
/// `callee` with the result.
///
/// Negation is what makes the outgoing register carry bits above `N`: at the
/// machine width `0 - x` sets every bit above the source's width for any
/// nonzero `x`, where an addition only carries when the operand is near the
/// top of its range. Those bits are exactly what the internal rule leaves
/// unspecified and what the boundary rule of Section 6 requires a caller to
/// clear.
fn forwarder(name: &str, callee: &str, shapes: &[Shape]) -> Function {
    let mut builder = Builder::new();
    let (args, mem, world) = entry_values(&mut builder, shapes);
    let block_args: Vec<BlockArg> = args
        .iter()
        .chain([&mem, &world])
        .map(binder)
        .collect::<Vec<_>>();

    let mut outgoing = Vec::new();
    for (shape, value) in shapes.iter().zip(&args) {
        match shape {
            Shape::Ptr => outgoing.push(value.clone()),
            Shape::Bits(width) => {
                let zero = builder.value(EvoIRType::Bits(*width));
                builder.push(Inst::Const(zero.clone(), 0));
                let negated = builder.value(EvoIRType::Bits(*width));
                builder.push(Inst::Sub(negated.clone(), zero, value.clone()));
                outgoing.push(negated);
            }
        }
    }

    let result = builder.value(EvoIRType::Bits(32));
    let mem_out = builder.value(MEM());
    let world_out = builder.value(WORLD());
    builder.push(Inst::Call {
        returns_twice: false,
        arg_marks: Vec::new(),
        results: vec![result.clone()],
        callee: callee.to_owned(),
        args: outgoing,
        mem_in: mem,
        world_in: world,
        mem_out: mem_out.clone(),
        world_out: world_out.clone(),
    });

    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: name.to_owned(),
        params: params(shapes),
        ret_tys: return_types(),
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: block_args,
            insts: builder.insts,
            terminator: Inst::Ret(vec![result, mem_out, world_out]),
        }],
        param_marks: Vec::new(),
    }
}

/// The whole probe module: for every shape, the fold EvoCC compiles, the fold
/// the host compiles (declared here so the module is closed, never emitted),
/// and the forwarder that calls the host's.
fn probe_module() -> Module {
    let mut functions = Vec::new();
    for (tag, shapes) in shapes() {
        functions.push(fold_function(&format!("host_fold_{tag}"), &shapes));
        functions.push(fold_function(&format!("evocc_fold_{tag}"), &shapes));
        functions.push(forwarder(
            &format!("evocc_calls_host_{tag}"),
            &format!("host_fold_{tag}"),
            &shapes,
        ));
    }
    for (tag, shapes) in bare_shapes() {
        functions.push(fold_function(&format!("host_bare_{tag}"), &shapes));
        functions.push(forwarder(
            &format!("evocc_calls_bare_{tag}"),
            &format!("host_bare_{tag}"),
            &shapes,
        ));
    }
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        globals: Vec::new(),
        functions,
        ..Module::default()
    };
    module.recompute_effect_summaries();
    module
}

/// The C expression for one argument's `bits<32>` contribution.
fn c_contribution(shape: Shape, name: &str) -> String {
    match shape {
        Shape::Ptr => format!("(uint32_t)(*{name})"),
        Shape::Bits(width) if width >= 32 => {
            format!("(uint32_t)((uint64_t){name} & {}u)", u32::MAX)
        }
        Shape::Bits(_) => format!("(uint32_t)((uint64_t){name} & {}u)", shape.mask()),
    }
}

/// The host side: the reference fold, the externs, and the comparison.
fn probe_source() -> String {
    let mut text = String::from(
        "#include <stdint.h>\n#include <stdio.h>\n\nstatic const uint8_t bytes[16] = {\n    17, 31, 47, 61, 79, 97, 113, 131, 149, 167, 181, 199, 211, 229, 241, 251\n};\n\n",
    );

    for (tag, shapes) in shapes() {
        let signature = shapes
            .iter()
            .enumerate()
            .map(|(index, shape)| format!("{} a{index}", shape.c_type()))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!("uint32_t host_fold_{tag}({signature}) {{\n"));
        text.push_str("    uint32_t acc = 0x811c9dc5u;\n");
        for (index, shape) in shapes.iter().enumerate() {
            text.push_str(&format!(
                "    acc = acc * 16777619u ^ {};\n",
                c_contribution(*shape, &format!("a{index}"))
            ));
        }
        text.push_str("    return acc;\n}\n\n");
        text.push_str(&format!("extern uint32_t evocc_fold_{tag}({signature});\n"));
        text.push_str(&format!(
            "extern uint32_t evocc_calls_host_{tag}({signature});\n\n"
        ));
    }

    // The same fold with nothing masked: each argument is used at its own C
    // type, so the host compiler is free to trust the platform's extension and
    // read the whole register. This is the shape that would have passed
    // silently while the caller left the bits above `N` as it found them.
    for (tag, shapes) in bare_shapes() {
        let signature = shapes
            .iter()
            .enumerate()
            .map(|(index, shape)| format!("{} a{index}", shape.c_type()))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!("uint32_t host_bare_{tag}({signature}) {{\n"));
        text.push_str("    uint32_t acc = 0x811c9dc5u;\n");
        for (index, shape) in shapes.iter().enumerate() {
            let contribution = match shape {
                Shape::Ptr => format!("(uint32_t)(*a{index})"),
                Shape::Bits(_) => format!("(uint32_t)a{index}"),
            };
            text.push_str(&format!("    acc = acc * 16777619u ^ {contribution};\n"));
        }
        text.push_str("    return acc;\n}\n\n");
        text.push_str(&format!(
            "extern uint32_t evocc_calls_bare_{tag}({signature});\n\n"
        ));
    }

    text.push_str("int main(void) {\n    int failures = 0;\n    int checks = 0;\n");
    for (tag, shapes) in shapes() {
        let mut direct = Vec::new();
        let mut bumped = Vec::new();
        for (index, shape) in shapes.iter().enumerate() {
            match shape {
                Shape::Ptr => {
                    direct.push(format!("&bytes[{}]", index * 2 % 16));
                    bumped.push(format!("&bytes[{}]", index * 2 % 16));
                }
                Shape::Bits(_) => {
                    let raw = 0x9e37_79b9_7f4a_7c15_u64
                        .wrapping_mul(index as u64 + 1)
                        .wrapping_add(0x0123_4567_89ab_cdef);
                    direct.push(format!("({}){}ull", shape.c_type(), raw));
                    bumped.push(format!(
                        "({})((0ull - ({}ull & {}ull)) & {}ull)",
                        shape.c_type(),
                        raw,
                        shape.mask(),
                        shape.mask()
                    ));
                }
            }
        }
        let direct = direct.join(", ");
        let bumped = bumped.join(", ");
        text.push_str(&format!(
            "    checks += 2;\n    if (evocc_fold_{tag}({direct}) != host_fold_{tag}({direct})) {{\n        printf(\"hostCallsEvocc {tag} disagreed\\n\");\n        failures++;\n    }}\n    if (evocc_calls_host_{tag}({direct}) != host_fold_{tag}({bumped})) {{\n        printf(\"evoccCallsHost {tag} disagreed\\n\");\n        failures++;\n    }}\n"
        ));
    }
    for (tag, shapes) in bare_shapes() {
        let mut direct = Vec::new();
        let mut bumped = Vec::new();
        for (index, shape) in shapes.iter().enumerate() {
            match shape {
                Shape::Ptr => {
                    direct.push(format!("&bytes[{}]", index * 3 % 16));
                    bumped.push(format!("&bytes[{}]", index * 3 % 16));
                }
                Shape::Bits(_) => {
                    let raw = 0xc2b2_ae3d_27d4_eb4f_u64
                        .wrapping_mul(index as u64 + 1)
                        .wrapping_add(0x0f0f_0f0f_0f0f_0f0f);
                    direct.push(format!("({}){}ull", shape.c_type(), raw));
                    bumped.push(format!(
                        "({})((0ull - ({}ull & {}ull)) & {}ull)",
                        shape.c_type(),
                        raw,
                        shape.mask(),
                        shape.mask()
                    ));
                }
            }
        }
        let direct = direct.join(", ");
        let bumped = bumped.join(", ");
        text.push_str(&format!(
            "    checks += 1;\n    if (evocc_calls_bare_{tag}({direct}) != host_bare_{tag}({bumped})) {{\n        printf(\"evoccCallsBareHost {tag} disagreed\\n\");\n        failures++;\n    }}\n"
        ));
    }
    text.push_str("    printf(\"checks=%d\\nfailures=%d\\n\", checks, failures);\n    return failures == 0 ? 0 : 1;\n}\n");
    text
}

/// Section 6: "A ninth integer argument is refused (`E4010`)."
fn nine_register_arguments() -> Module {
    let shapes = vec![Shape::Bits(32); 9];
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        globals: Vec::new(),
        functions: vec![fold_function("nine", &shapes)],
        ..Module::default()
    };
    module.recompute_effect_summaries();
    module
}

#[test]
fn the_calling_convention_probe_agrees_in_both_directions() {
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("probeFile={}", file!());
    println!("toolchain={}", toolchain.version());

    let module = probe_module();
    evocc_evoir::validate_module(&module).expect("the probe module validates");

    // The host compiles every `host_*`; EvoCC compiles the rest.
    let text = compile_module_with(&module, &|name| !name.starts_with("host_"))
        .expect("every probe function lowers, allocates, verifies and emits");

    let directory = std::env::temp_dir().join(format!("evocc-probe-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a working directory");
    let assembly = directory.join("probe.s");
    let source = directory.join("probe.c");
    let program = directory.join("probe");
    std::fs::write(&assembly, &text).expect("the emitted text");
    std::fs::write(&source, probe_source()).expect("the host source");

    // The host side is optimized: at `-O0` clang spills a narrow parameter to
    // the stack with a byte or halfword store and reloads it, which masks the
    // register and hides exactly the difference this family exists to find.
    let built = Command::new("clang")
        .arg("-O2")
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .arg(&assembly)
        .output()
        .expect("run clang");
    assert!(
        built.status.success(),
        "the toolchain refused the probe:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&program).output().expect("run the probe");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    print!("{stdout}");

    let mut covered = shapes();
    covered.extend(bare_shapes());
    println!("shapes={}", covered.len());
    for (tag, shape) in &covered {
        println!(
            "shape={tag} arguments={} classes={}",
            shape.len(),
            shape
                .iter()
                .map(|one| one.register_class())
                .collect::<Vec<_>>()
                .join("+")
        );
    }
    assert_eq!(
        run.status.code(),
        Some(0),
        "the probe reported a disagreement:\n{stdout}"
    );
    // Two comparisons for each folding shape and one for each bare shape.
    let expected = shapes().len() * 2 + bare_shapes().len();
    assert!(
        stdout.contains(&format!("checks={expected}")),
        "the probe ran fewer comparisons than it declared:\n{stdout}"
    );

    let _ = std::fs::remove_file(&assembly);
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&program);
}

#[test]
fn a_ninth_register_argument_stays_refused() {
    let refusals = evocc_mir::lower_module(&nine_register_arguments())
        .expect_err("nine register arguments are outside the convention");
    let codes: Vec<&str> = refusals.iter().map(|refusal| refusal.code).collect();
    assert_eq!(codes, vec!["E4010"]);
    println!("stackArgumentsRefusedBy=E4010");
}

// ---------------------------------------------------------------------------
// The narrow-result probe (E4014)
// ---------------------------------------------------------------------------

/// Section 6: "A narrow result crossing the boundary is not yet ruled: Apple's
/// variant extends it too, and nothing in this revision measures it."
///
/// These are the widths a C type has, so the host can declare the function and
/// read its result at that type.
const NARROW_RESULTS: [u16; 3] = [1, 8, 16];

/// A function of one `bits<32>` argument returning `bits<N>`.
///
/// The body negates at width `N`, which at the machine width sets every bit
/// above `N` for any nonzero operand: the bits the internal rule leaves
/// unspecified, and the ones the platform rule would have the callee clear.
fn narrow_result_function(name: &str, width: u16) -> Function {
    let mut builder = Builder::new();
    let argument = builder.value(EvoIRType::Bits(32));
    let mem = builder.value(MEM());
    let world = builder.value(WORLD());
    let block_args: Vec<BlockArg> = [&argument, &mem, &world].into_iter().map(binder).collect();

    let narrowed = builder.value(EvoIRType::Bits(width));
    builder.push(Inst::Trunc(
        narrowed.clone(),
        argument.clone(),
        EvoIRType::Bits(width),
    ));
    let zero = builder.value(EvoIRType::Bits(width));
    builder.push(Inst::Const(zero.clone(), 0));
    let negated = builder.value(EvoIRType::Bits(width));
    builder.push(Inst::Sub(negated.clone(), zero, narrowed));

    Function {
        section: None,
        patchable_entry: None,
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: name.to_owned(),
        params: vec![
            FunctionParam {
                name: "arg0".to_owned(),
                ty: EvoIRType::Bits(32),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: MEM(),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: WORLD(),
            },
        ],
        ret_tys: vec![EvoIRType::Bits(width), MEM(), WORLD()],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: block_args,
            insts: builder.insts,
            terminator: Inst::Ret(vec![negated, mem, world]),
        }],
        param_marks: Vec::new(),
    }
}

/// The C type a `bits<N>` result is declared with.
const fn narrow_c_type(width: u16) -> &'static str {
    match width {
        1 | 8 => "uint8_t",
        _ => "uint16_t",
    }
}

fn narrow_module() -> Module {
    let mut functions = Vec::new();
    for width in NARROW_RESULTS {
        functions.push(narrow_result_function(
            &format!("host_narrow_{width}"),
            width,
        ));
        functions.push(narrow_result_function(
            &format!("evocc_narrow_{width}"),
            width,
        ));
    }
    let mut module = Module {
        tls: Vec::new(),
        lifecycle: Vec::new(),
        aliases: Vec::new(),
        assembly: Vec::new(),
        globals: Vec::new(),
        functions,
        ..Module::default()
    };
    module.recompute_effect_summaries();
    module
}

/// The host side: its own build of the same function, and the comparison.
///
/// The result is used at its declared C type without a mask, so a host caller
/// that trusts the platform's extension reads whatever the callee left above
/// the width.
fn narrow_source() -> String {
    let mut text = String::from("#include <stdint.h>\n#include <stdio.h>\n\n");
    for width in NARROW_RESULTS {
        let c = narrow_c_type(width);
        let mask = (1_u64 << width) - 1;
        text.push_str(&format!(
            "{c} host_narrow_{width}(uint32_t a) {{ return ({c})((0u - a) & {mask}u); }}\n"
        ));
        text.push_str(&format!("extern {c} evocc_narrow_{width}(uint32_t a);\n\n"));
    }
    text.push_str("int main(void) {\n    int failures = 0;\n    int checks = 0;\n");
    for width in NARROW_RESULTS {
        text.push_str(&format!(
            "    for (uint32_t a = 1; a < 200; a++) {{\n        checks++;\n        uint32_t mine = (uint32_t)evocc_narrow_{width}(a);\n        uint32_t theirs = (uint32_t)host_narrow_{width}(a);\n        if (mine != theirs) {{\n            printf(\"narrowResult {width} disagreed a=%u evocc=%u host=%u\\n\", a, mine, theirs);\n            failures++;\n            break;\n        }}\n    }}\n"
        ));
    }
    text.push_str("    printf(\"narrowChecks=%d\\nnarrowFailures=%d\\n\", checks, failures);\n    return failures == 0 ? 0 : 1;\n}\n");
    text
}

/// Section 6, the question `E4014` stands on: does the callee have to extend a
/// result narrower than 32 bits before it crosses the boundary?
#[test]
fn the_narrow_result_probe_settles_the_extension() {
    let toolchain = match Toolchain::locate() {
        Ok(toolchain) => toolchain,
        Err(reason) => {
            println!("skipped={reason}");
            return;
        }
    };
    println!("probeFile={}", file!());
    println!("toolchain={}", toolchain.version());

    let module = narrow_module();
    evocc_evoir::validate_module(&module).expect("the probe module validates");
    let text = compile_module_with(&module, &|name| !name.starts_with("host_"))
        .expect("every probe function lowers, allocates, verifies and emits");

    let directory = std::env::temp_dir().join(format!("evocc-narrow-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a working directory");
    let assembly = directory.join("narrow.s");
    let source = directory.join("narrow.c");
    let program = directory.join("narrow");
    std::fs::write(&assembly, &text).expect("the emitted text");
    std::fs::write(&source, narrow_source()).expect("the host source");

    let built = Command::new("clang")
        .arg("-O2")
        .arg("-o")
        .arg(&program)
        .arg(&source)
        .arg(&assembly)
        .output()
        .expect("run clang");
    assert!(
        built.status.success(),
        "the toolchain refused the narrow-result probe:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let run = Command::new(&program).output().expect("run the probe");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    print!("{stdout}");
    for width in NARROW_RESULTS {
        println!("narrowWidth={width} cType={}", narrow_c_type(width));
    }

    let _ = std::fs::remove_file(&assembly);
    let _ = std::fs::remove_file(&source);
    let _ = std::fs::remove_file(&program);

    assert_eq!(
        run.status.code(),
        Some(0),
        "the narrow-result probe reported a disagreement:\n{stdout}"
    );
    assert!(
        stdout.contains(&format!("narrowChecks={}", NARROW_RESULTS.len() * 199)),
        "the probe ran fewer comparisons than it declared:\n{stdout}"
    );
}
