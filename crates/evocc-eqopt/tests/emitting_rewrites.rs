//! The four emitting rules of `p0.scalar.v4`, as the producer states them.
//!
//! Section 11.6 "Emitting right-hand sides": a right-hand side may be an
//! operator applied to bound variables and `const_like` terms. The producer
//! emits it as a `source=derived` node whose operator, sort and children the
//! checker reconstructs from the pattern and the recorded bindings, in a class
//! of its own that the rewrite then unions with the left-hand node's class; it
//! requires a `node:` proof reference like every builder. The side condition
//! `width_min(?x,K)` holds when the sort of `?x` is `bits<N>` with `N >= K`
//! and records no premise.
//!
//! Which of the three spellings of a doubling survives is the policy's choice
//! (Section 7.5 "Cost is a policy"), so each rule is measured under a policy
//! whose own costs make the region a strict improvement. A rule that fires
//! only where no policy keeps it would never appear in a certificate.

use evocc_eqopt::{optimize_scalar_module_with_policy, policy_by_name};
use evocc_evoir::{
    render_module, validate_module, Block, BlockArg, EffectSummary, Function, FunctionParam, Inst,
    Module, Value,
};
use evocc_proof::render_evp1;
use evocc_types::{EvoIRType, Region};

const REGISTRY_V4: &str = "1a5391603386e18e18bfb1b2fc61985c9eb252c6e65bcb3259104bd49b2a9d74";

fn bits(id: u32) -> Value {
    Value {
        id,
        ty: EvoIRType::Bits(8),
    }
}

fn argument(value: &Value) -> BlockArg {
    BlockArg {
        name: format!("v{}", value.id),
        value: value.clone(),
    }
}

/// One function whose entry block computes `result` from `%v0` and returns it.
fn module_of(insts: Vec<Inst>, result: Value) -> Module {
    let x = bits(0);
    let memory = Value {
        id: 1,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 2,
        ty: EvoIRType::World,
    };
    let function = Function {
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(8),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            EvoIRType::Bits(8),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&x), argument(&memory), argument(&world)],
            insts,
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the doubling probe is valid EvoIR");
    module
}

/// `%r = mul %x, 2`, the right-hand spelling `p2_300_mul_two_rhs` matches.
fn mul_two_rhs() -> Module {
    let two = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(two.clone(), 2),
            Inst::Mul(result.clone(), bits(0), two),
        ],
        result,
    )
}

/// `%r = mul 2, %x`, the left-hand spelling `p2_301_mul_two_lhs` matches.
fn mul_two_lhs() -> Module {
    let two = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(two.clone(), 2),
            Inst::Mul(result.clone(), two, bits(0)),
        ],
        result,
    )
}

/// `%r = add %x, %x`, which `p2_302_add_self_shl` rewrites to `shl(%x, 1)`.
fn add_self() -> Module {
    let result = bits(3);
    module_of(vec![Inst::Add(result.clone(), bits(0), bits(0))], result)
}

/// `%r = shl %x, 1`, which `p2_303_shl_one_add` rewrites to `add(%x, %x)`.
fn shl_one() -> Module {
    let one = bits(3);
    let result = bits(4);
    module_of(
        vec![
            Inst::Const(one.clone(), 1),
            Inst::Shl(result.clone(), bits(0), one),
        ],
        result,
    )
}

struct Fired {
    proof: String,
    after: String,
}

fn fire(module: &Module, policy: &str) -> Fired {
    let policy = policy_by_name(policy).expect("this producer ships the named policy");
    let outcome = optimize_scalar_module_with_policy(module, &policy);
    Fired {
        proof: render_evp1(&outcome.certificate),
        after: render_module(&outcome.module),
    }
}

impl Fired {
    fn rewrite(&self, rule: &str) -> String {
        self.proof
            .lines()
            .find(|line| line.starts_with("rewrite ") && line.contains(rule))
            .unwrap_or_else(|| panic!("{rule} never fired:\n{}", self.proof))
            .to_owned()
    }

    fn nodes(&self) -> Vec<String> {
        self.proof
            .lines()
            .filter(|line| line.starts_with("node "))
            .map(str::to_owned)
            .collect()
    }
}

/// Every emitting rule fires, names `p0.scalar.v4`, and writes its right-hand
/// side as a derived node reached by a `node:` reference.
#[test]
fn each_emitting_rule_fires_under_a_policy_that_keeps_it() {
    let cases = [
        (
            "p2_300_mul_two_rhs",
            mul_two_rhs(),
            "weighted-operators",
            "add",
        ),
        (
            "p2_301_mul_two_lhs",
            mul_two_lhs(),
            "weighted-operators",
            "add",
        ),
        ("p2_302_add_self_shl", add_self(), "prefer-shift", "shl"),
        ("p2_303_shl_one_add", shl_one(), "prefer-add", "add"),
    ];
    for (rule, module, policy, emitted) in cases {
        let fired = fire(&module, policy);
        assert!(
            fired
                .proof
                .contains(&format!("registry p0.scalar.v4 {REGISTRY_V4}")),
            "{rule} must commit to the narrowest registry that prints it:\n{}",
            fired.proof
        );
        let line = fired.rewrite(rule);
        println!("{rule}: {line}");
        let reference = line
            .split_whitespace()
            .find(|field| field.starts_with("node:") || field.starts_with("class:"))
            .expect("a rewrite records two proof references");
        assert!(
            line.contains(" node:"),
            "{rule} builds its right-hand side and needs a node: reference, got {reference}"
        );
        assert!(
            !line.contains(" premises "),
            "width_min records no premise:\n{line}"
        );
        let emitted_node = fired
            .nodes()
            .into_iter()
            .find(|node| node.contains("source=derived") && node.contains(&format!("op={emitted}")))
            .unwrap_or_else(|| panic!("{rule} emits no derived {emitted}:\n{}", fired.proof));
        println!("{rule} emitted: {emitted_node}");
        assert!(
            emitted_node.contains("sort=bits<8>"),
            "the emitted node takes the sort of ?x:\n{emitted_node}"
        );
        assert!(
            fired.after.contains(emitted),
            "the after-module must contain the emitted {emitted}:\n{}",
            fired.after
        );
    }
}

/// `p2_302`'s right-hand side writes a constant operand.
///
/// Section 11.3 as amended at revision 1.9.52: a derived node is used when a
/// rewrite names it or when it is a child, at any depth, of a derived node a
/// rewrite names, so the `const_like(?x,1)` under the emitted `shl` is used
/// through its parent and no rewrite names it.
#[test]
fn an_emitting_right_hand_side_writes_a_constant_operand() {
    let fired = fire(&add_self(), "prefer-shift");
    let nodes = fired.nodes();
    let constant = nodes
        .iter()
        .find(|node| {
            node.contains("source=derived") && node.contains("op=const") && node.contains("value=1")
        })
        .unwrap_or_else(|| panic!("no derived const 1:\n{}", fired.proof));
    println!("constant: {constant}");
    let id = constant
        .split_whitespace()
        .nth(2)
        .expect("a node record names its identity")
        .to_owned();
    let shift = nodes
        .iter()
        .find(|node| node.contains("source=derived") && node.contains("op=shl"))
        .unwrap_or_else(|| panic!("no derived shl:\n{}", fired.proof));
    println!("shift: {shift}");
    assert!(
        !fired
            .rewrite("p2_302_add_self_shl")
            .contains(&format!("node:{id} ")),
        "the constant is used through its parent, never named by the rewrite"
    );
    assert!(
        fired.after.contains("shl"),
        "the after-module carries the emitted shift:\n{}",
        fired.after
    );
}

/// A doubling under a policy whose costs do not make the region cheaper is
/// declined, and declining leaves the module byte-identical.
///
/// Section 7.5: a producer applies the strict-improvement rule relative to its
/// own costs, and Section 8.7 lets it decline any eligible region. Under
/// instruction count every spelling of a doubling costs one, so none of the
/// four rules can pay for itself.
#[test]
fn instruction_count_declines_every_doubling() {
    for module in [mul_two_rhs(), mul_two_lhs(), add_self(), shl_one()] {
        let before = render_module(&module);
        let fired = fire(&module, "instruction-count");
        assert_eq!(fired.after, before, "a declined region changes nothing");
        for rule in [
            "p2_300_mul_two_rhs",
            "p2_301_mul_two_lhs",
            "p2_302_add_self_shl",
            "p2_303_shl_one_add",
        ] {
            assert!(
                !fired.proof.contains(rule),
                "{rule} is not a strict improvement under instruction count:\n{}",
                fired.proof
            );
        }
    }
}

/// `width_min(?x,2)` refuses `bits<1>`.
///
/// A doubling of a one-bit value is `0`, and the rules do not state that; the
/// side condition is what keeps them from claiming it.
#[test]
fn width_min_excludes_a_one_bit_value() {
    let x = Value {
        id: 0,
        ty: EvoIRType::Bits(1),
    };
    let memory = Value {
        id: 1,
        ty: EvoIRType::Mem(Region::All),
    };
    let world = Value {
        id: 2,
        ty: EvoIRType::World,
    };
    let result = Value {
        id: 3,
        ty: EvoIRType::Bits(1),
    };
    let function = Function {
        stack_protect: false,
        alignment: 4,
        linkage: evocc_evoir::Linkage::Strong,
        visibility: evocc_evoir::GlobalVisibility::External,
        variadic: false,
        name: "main".to_owned(),
        params: vec![
            FunctionParam {
                name: "x".to_owned(),
                ty: EvoIRType::Bits(1),
            },
            FunctionParam {
                name: "arg1".to_owned(),
                ty: EvoIRType::Mem(Region::All),
            },
            FunctionParam {
                name: "arg2".to_owned(),
                ty: EvoIRType::World,
            },
        ],
        ret_tys: vec![
            EvoIRType::Bits(1),
            EvoIRType::Mem(Region::All),
            EvoIRType::World,
        ],
        effects: EffectSummary::default(),
        blocks: vec![Block {
            id: 0,
            args: vec![argument(&x), argument(&memory), argument(&world)],
            insts: vec![Inst::Add(result.clone(), x.clone(), x)],
            terminator: Inst::Ret(vec![result, memory, world]),
        }],
        param_marks: Vec::new(),
    };
    let mut module = Module {
        aliases: Vec::new(),
        assembly: Vec::new(),
        extern_objects: Vec::new(),
        declares: Vec::new(),
        globals: Vec::new(),
        functions: vec![function],
    };
    module.recompute_effect_summaries();
    validate_module(&module).expect("the one-bit probe is valid EvoIR");
    let fired = fire(&module, "prefer-shift");
    assert!(
        !fired.proof.contains("p2_302_add_self_shl"),
        "width_min(?x,2) excludes bits<1>:\n{}",
        fired.proof
    );
}
