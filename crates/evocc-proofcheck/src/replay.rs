//! Independent replay of the reviewed scalar registries.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::eligibility::EligibleRegion;
use crate::facts::{Comparison, Fact};
use evocc_evoir::{inst_results, CmpOp, EvoIRType, Inst, Module, Value};
use evocc_proof::{
    EvpCongruence, EvpEffectClass, EvpEvent, EvpExtractionChoice, EvpNode, EvpNodeSource,
    EvpPremise, EvpProofRef, EvpRegion, EvpRewrite,
};

/// Which reviewed registry a certificate committed to.
///
/// Section 11.6: a certificate cannot extend the registry it names, so a rule
/// the revision added is not available to a certificate that named the earlier
/// one, however well formed the rewrite is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Registry {
    Reviewed,
    Revised,
    Widened,
}

/// Which registry first prints a rule. The identifiers do not answer this on
/// their own: `p1_206` is printed by the third registry although it sits
/// between two rules of the second.
fn first_printed_by(rule: &str) -> Registry {
    const REVISED: [&str; 18] = [
        "p1_200_known_and",
        "p1_201_known_or",
        "p1_202_known_xor",
        "p1_203_known_add",
        "p1_204_known_sub",
        "p1_205_known_mul",
        "p1_210_known_icmp_eq",
        "p1_211_known_icmp_ne",
        "p1_212_known_icmp_slt",
        "p1_213_known_icmp_sle",
        "p1_214_known_icmp_ult",
        "p1_215_known_icmp_ule",
        "p1_220_and_cover_rhs",
        "p1_221_and_cover_lhs",
        "p1_222_or_cover_rhs",
        "p1_223_or_cover_lhs",
        "p1_224_xor_known_zero_rhs",
        "p1_225_xor_known_zero_lhs",
    ];
    const WIDENED: [&str; 6] = [
        "p1_206_known_shl",
        "p1_207_known_lshr",
        "p1_208_known_ashr",
        "p1_230_known_zext",
        "p1_231_known_sext",
        "p1_232_known_trunc",
    ];
    if WIDENED.contains(&rule) {
        Registry::Widened
    } else if REVISED.contains(&rule) {
        Registry::Revised
    } else {
        Registry::Reviewed
    }
}

/// Whether a certificate naming `named` may use a rule `printed` first prints.
fn contains(named: Registry, printed: Registry) -> bool {
    matches!(
        (named, printed),
        (Registry::Reviewed, Registry::Reviewed)
            | (Registry::Revised, Registry::Reviewed | Registry::Revised)
            | (Registry::Widened, _)
    )
}

const LEFT: usize = 0;
const RIGHT: usize = 1;

#[derive(Debug, Clone)]
pub(crate) struct RegionReplacement {
    pub(crate) function: String,
    pub(crate) block: String,
    pub(crate) instruction: usize,
    pub(crate) root_value: u32,
    pub(crate) selections: Vec<Selection>,
    pub(crate) root_class: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Selection {
    pub(crate) class: u32,
    pub(crate) choice: Selected,
}

#[derive(Debug, Clone)]
pub(crate) enum Selected {
    Existing(Value),
    Node(EvpNode),
}

pub(crate) fn replay_region(
    module: &Module,
    claimed: &EvpRegion,
    derived: &EligibleRegion,
    registry: Registry,
) -> Result<RegionReplacement, ()> {
    if claimed.id != derived.id
        || claimed.root_value != derived.root_value
        || claimed.witnesses != derived.witnesses
        || claimed.seeds != derived.seeds
    {
        return Err(());
    }
    let mut replay = Replay::new(module, claimed, registry)?;
    for event in &claimed.events {
        replay.event(event)?;
    }
    replay.finish()
}

struct Replay<'a> {
    module: &'a Module,
    region: &'a EvpRegion,
    parent: Vec<u32>,
    sorts: Vec<String>,
    nodes: Vec<EvpNode>,
    before_values: HashMap<u32, Value>,
    value_classes: HashMap<u32, u32>,
    before_count: usize,
    derived_started: bool,
    used_derived: HashSet<u32>,
    registry: Registry,
}

impl<'a> Replay<'a> {
    fn new(module: &'a Module, region: &'a EvpRegion, registry: Registry) -> Result<Self, ()> {
        let mut sorts = Vec::new();
        let mut value_classes = HashMap::new();
        for (index, seed) in region.seeds.iter().enumerate() {
            if seed.class != u32::try_from(index).map_err(|_| ())?
                || !bits_width(&seed.sort).is_some()
            {
                return Err(());
            }
            let value = find_value(module, &region.id.function, seed.value).ok_or(())?;
            if packed_type(&value.ty) != seed.sort
                || value_classes.insert(seed.value, seed.class).is_some()
            {
                return Err(());
            }
            sorts.push(seed.sort.clone());
        }
        let parent = (0..sorts.len())
            .map(|index| u32::try_from(index).expect("seed index checked"))
            .collect();
        Ok(Self {
            module,
            region,
            parent,
            sorts,
            nodes: Vec::new(),
            before_values: HashMap::new(),
            value_classes,
            before_count: 0,
            derived_started: false,
            used_derived: HashSet::new(),
            registry,
        })
    }

    fn event(&mut self, event: &EvpEvent) -> Result<(), ()> {
        match event {
            EvpEvent::Node(node) => self.insert_node(node),
            EvpEvent::Rewrite(rewrite) => self.rewrite(rewrite),
            EvpEvent::Congruence(congruence) => self.congruence(congruence),
            // A marker says which epoch produced what follows. It changes no
            // class and proves nothing on its own; its ordering is a parse
            // rule and its value is checked against the recorded epoch count.
            EvpEvent::Epoch(_) => Ok(()),
        }
    }

    fn insert_node(&mut self, node: &EvpNode) -> Result<(), ()> {
        let node_id = u32::try_from(self.nodes.len()).map_err(|_| ())?;
        let owner = u32::try_from(self.region.seeds.len() + self.nodes.len()).map_err(|_| ())?;
        if node.node != node_id || node.owner != owner || node.effect != EvpEffectClass::Pure {
            return Err(());
        }
        if node.children.iter().any(|class| !self.class_exists(*class)) {
            return Err(());
        }
        validate_node_shape(node, &self.sorts, &|class| self.find(class))?;
        match node.source {
            EvpNodeSource::Before => {
                if self.derived_started || self.before_count >= self.region.witnesses.len() {
                    return Err(());
                }
                let witness_id = &self.region.witnesses[self.before_count];
                let instruction = find_instruction(self.module, witness_id).ok_or(())?;
                let result = inst_results(instruction).first().copied().ok_or(())?;
                let expected = expected_node(instruction, node_id, owner, &self.value_classes)?;
                if *node != expected {
                    return Err(());
                }
                self.value_classes.insert(result.id, owner);
                self.before_values.insert(node_id, result.clone());
                self.before_count += 1;
            }
            EvpNodeSource::Derived => self.derived_started = true,
        }
        self.parent.push(owner);
        self.sorts.push(node.sort.clone());
        self.nodes.push(node.clone());
        Ok(())
    }

    fn rewrite(&mut self, rewrite: &EvpRewrite) -> Result<(), ()> {
        self.validate_union(rewrite.lhs_pre, rewrite.rhs_pre, rewrite.result)?;
        let lhs = self.proof_node(rewrite.lhs, rewrite.lhs_pre)?;
        let rhs_node = match rewrite.rhs {
            EvpProofRef::Node(node) => {
                Some(self.proof_node(EvpProofRef::Node(node), rewrite.rhs_pre)?)
            }
            EvpProofRef::Class(class) => {
                if self.find(class) != rewrite.rhs_pre {
                    return Err(());
                }
                None
            }
        };
        // Section 11.3: the facts a rewrite relied on are recomputed from the
        // region's nodes, with the partition as it stands before this union,
        // and the rule's side condition is decided from those and not from
        // what the certificate claimed.
        let facts = self.class_facts()?;
        self.confirm_premises(&facts, rewrite)?;
        validate_reviewed_rule(self, rewrite, lhs, rhs_node, &facts)?;
        if let Some(node) = rhs_node {
            if node.source == EvpNodeSource::Derived {
                self.used_derived.insert(node.node);
            }
        }
        self.union(rewrite.lhs_pre, rewrite.rhs_pre);
        // Section 8.10: two values that disagree on a bit both know cannot be
        // equal, so the union is invalid. The check is performed here as well
        // as by the producer, which is what makes it worth performing.
        self.class_facts()?;
        Ok(())
    }

    /// The Section 8.10 fact of every class, from this region's nodes alone.
    ///
    /// A class's fact is the meet of the transfers of the nodes it owns; seeds
    /// contribute nothing, because Section 8.10 gives a block argument no fact.
    /// The loop runs to a fixed point because a union can sharpen a child's
    /// fact after the node that reads it was inserted, and facts only ever move
    /// down.
    fn class_facts(&self) -> Result<Vec<Fact>, ()> {
        let mut facts = self
            .sorts
            .iter()
            .map(|sort| bits_width(sort).map(Fact::top).ok_or(()))
            .collect::<Result<Vec<_>, ()>>()?;
        loop {
            let mut changed = false;
            for node in &self.nodes {
                let transferred = self.node_fact(node, &facts)?;
                let root = self.find(node.owner) as usize;
                let merged = facts
                    .get(root)
                    .copied()
                    .ok_or(())?
                    .meet(transferred)
                    .ok_or(())?;
                if merged != facts[root] {
                    facts[root] = merged;
                    changed = true;
                }
            }
            if !changed {
                return Ok(facts);
            }
        }
    }

    /// One row of the Section 8.10 transfer table, applied to one node.
    fn node_fact(&self, node: &EvpNode, facts: &[Fact]) -> Result<Fact, ()> {
        let width = bits_width(&node.sort).ok_or(())?;
        let child = |index: usize| -> Result<Fact, ()> {
            let class = self.find(*node.children.get(index).ok_or(())?);
            facts.get(class as usize).copied().ok_or(())
        };
        if let Some(comparison) = Comparison::from_operation(&node.operation) {
            return Ok(child(LEFT)?.compare(child(RIGHT)?, comparison));
        }
        Ok(match node.operation.as_str() {
            "const" => Fact::constant(width, attribute_value(node)?),
            "and" => child(LEFT)?.and(child(RIGHT)?),
            "or" => child(LEFT)?.or(child(RIGHT)?),
            "xor" => child(LEFT)?.xor(child(RIGHT)?),
            "add" => child(LEFT)?.add(child(RIGHT)?),
            "sub" => child(LEFT)?.sub(child(RIGHT)?),
            "mul" => child(LEFT)?.mul(child(RIGHT)?),
            // The rows Section 8.10 gained at revision 1.9.43. Section 11.3's
            // operator table has no spelling for any of them and Section 8.5's
            // allowlist keeps them out of a witness closure, so no certificate
            // reaches these arms today. They are here because the checker's
            // copy of the table is what makes a premise verifiable, and the
            // day the grammar admits the operators is not the day to start
            // writing the transfers.
            "shl" => child(LEFT)?.shl(child(RIGHT)?),
            "lshr" => child(LEFT)?.lshr(child(RIGHT)?),
            "ashr" => child(LEFT)?.ashr(child(RIGHT)?),
            "zext" => child(LEFT)?.zext(width),
            "sext" => child(LEFT)?.sext(width),
            "trunc" => child(LEFT)?.trunc(width),
            _ => return Err(()),
        })
    }

    /// Section 11.3 "Premises": refuse unless every bit the premise claims is
    /// known to the checker with the same value.
    fn confirm_premises(&self, facts: &[Fact], rewrite: &EvpRewrite) -> Result<(), ()> {
        for premise in &rewrite.premises {
            let recomputed = self.premised_fact(facts, premise)?;
            let width = recomputed.width();
            let claimed = Fact::from_premise(width, premise.mask, premise.value).ok_or(())?;
            // The wire form carries the class's width rounded up to a whole
            // hexadecimal digit; a premise counted for another width is a claim
            // about another value.
            if premise.digits != usize::from(width.div_ceil(4))
                || !recomputed.is_at_or_below(claimed)
            {
                return Err(());
            }
        }
        Ok(())
    }

    fn premised_fact(&self, facts: &[Fact], premise: &EvpPremise) -> Result<Fact, ()> {
        if !self.class_exists(premise.class) {
            return Err(());
        }
        facts
            .get(self.find(premise.class) as usize)
            .copied()
            .ok_or(())
    }

    /// The recomputed fact of a class the rewrite bound.
    fn fact_of(&self, facts: &[Fact], class: u32) -> Result<Fact, ()> {
        facts.get(self.find(class) as usize).copied().ok_or(())
    }

    fn congruence(&mut self, congruence: &EvpCongruence) -> Result<(), ()> {
        self.validate_union(congruence.lhs_pre, congruence.rhs_pre, congruence.result)?;
        let left = self.node(congruence.lhs_node)?;
        let right = self.node(congruence.rhs_node)?;
        if self.find(left.owner) != congruence.lhs_pre
            || self.find(right.owner) != congruence.rhs_pre
            || left.operation != right.operation
            || left.sort != right.sort
            || left.effect != right.effect
            || left.attributes != right.attributes
            || left.children.len() != right.children.len()
            || left
                .children
                .iter()
                .zip(&right.children)
                .any(|(left, right)| self.find(*left) != self.find(*right))
        {
            return Err(());
        }
        self.union(congruence.lhs_pre, congruence.rhs_pre);
        self.class_facts()?;
        Ok(())
    }

    fn finish(self) -> Result<RegionReplacement, ()> {
        if self.before_count != self.region.witnesses.len() {
            return Err(());
        }
        let derived = self
            .nodes
            .iter()
            .filter(|node| node.source == EvpNodeSource::Derived)
            .map(|node| node.node)
            .collect::<HashSet<_>>();
        if derived != self.used_derived {
            return Err(());
        }

        let mut selections = Vec::new();
        let mut choices = BTreeMap::new();
        let mut costs = HashMap::new();
        for extraction in &self.region.extractions {
            let class = self.find(extraction.class);
            if class != extraction.class || choices.contains_key(&class) {
                return Err(());
            }
            let (selected, cost) = match extraction.choice {
                EvpExtractionChoice::Seed(index) => {
                    let seed = self.region.seeds.get(index as usize).ok_or(())?;
                    if self.find(seed.class) != class {
                        return Err(());
                    }
                    let value = find_value(self.module, &self.region.id.function, seed.value)
                        .ok_or(())?
                        .clone();
                    (Selected::Existing(value), 0_usize)
                }
                EvpExtractionChoice::Node(node_id) => {
                    let mut node = self.node(node_id)?.clone();
                    if self.find(node.owner) != class {
                        return Err(());
                    }
                    let before_value = (node.source == EvpNodeSource::Before)
                        .then(|| self.before_values.get(&node.node).cloned())
                        .flatten();
                    let terminal_before_reuse = before_value
                        .as_ref()
                        .is_some_and(|value| value.id != self.region.root_value);
                    let (selected, cost) = if terminal_before_reuse {
                        (Selected::Existing(before_value.ok_or(())?), 0_usize)
                    } else {
                        let mut cost = usize::from(node.operation != "const");
                        for child in &node.children {
                            let child = self.find(*child);
                            let Some(child_cost) = costs.get(&child) else {
                                return Err(());
                            };
                            cost = cost.checked_add(*child_cost).ok_or(())?;
                        }
                        node.children = node
                            .children
                            .iter()
                            .map(|child| self.find(*child))
                            .collect();
                        (Selected::Node(node), cost)
                    };
                    (selected, cost)
                }
            };
            choices.insert(class, selections.len());
            costs.insert(class, cost);
            selections.push(Selection {
                class,
                choice: selected,
            });
        }
        let root_class = self.find(self.region.root_class);
        if root_class != self.region.root_class || !choices.contains_key(&root_class) {
            return Err(());
        }
        let root_node = self
            .before_values
            .iter()
            .find_map(|(node, value)| (value.id == self.region.root_value).then_some(*node))
            .ok_or(())?;
        if self.find(self.node(root_node)?.owner) != root_class {
            return Err(());
        }
        let reachable = extraction_closure(root_class, &selections, &choices, &self)?;
        if reachable.len() != selections.len() {
            return Err(());
        }
        let original_cost = original_root_cost(root_node, &self.nodes)?;
        if costs[&root_class] >= original_cost {
            return Err(());
        }
        Ok(RegionReplacement {
            function: self.region.id.function.clone(),
            block: self.region.id.block.clone(),
            instruction: usize::try_from(self.region.id.instruction).map_err(|_| ())?,
            root_value: self.region.root_value,
            selections,
            root_class,
        })
    }

    fn validate_union(&self, lhs: u32, rhs: u32, result: u32) -> Result<(), ()> {
        if !self.class_exists(lhs)
            || !self.class_exists(rhs)
            || self.find(lhs) != lhs
            || self.find(rhs) != rhs
            || lhs == rhs
            || result != lhs.min(rhs)
            || self.sorts[lhs as usize] != self.sorts[rhs as usize]
        {
            return Err(());
        }
        Ok(())
    }

    fn proof_node(&self, reference: EvpProofRef, endpoint: u32) -> Result<&EvpNode, ()> {
        let EvpProofRef::Node(node) = reference else {
            return Err(());
        };
        let owner = self.node(node)?.owner;
        if self.find(owner) != endpoint {
            return Err(());
        }
        self.node(node)
    }

    fn node(&self, id: u32) -> Result<&EvpNode, ()> {
        self.nodes
            .get(id as usize)
            .filter(|node| node.node == id)
            .ok_or(())
    }

    fn class_exists(&self, class: u32) -> bool {
        (class as usize) < self.parent.len()
    }

    fn find(&self, mut class: u32) -> u32 {
        loop {
            let parent = self.parent[class as usize];
            if parent == class {
                return class;
            }
            class = parent;
        }
    }

    fn union(&mut self, left: u32, right: u32) {
        let root = left.min(right);
        let other = left.max(right);
        self.parent[other as usize] = root;
    }
}

fn validate_reviewed_rule(
    replay: &Replay<'_>,
    rewrite: &EvpRewrite,
    lhs: &EvpNode,
    rhs_node: Option<&EvpNode>,
    facts: &[Fact],
) -> Result<(), ()> {
    // A rule only the revision prints needs a certificate that named the
    // revision, and it records the facts its side condition read. A rule the
    // earlier registry already held reads no fact and records none, so a
    // premise on one is a claim its rule never makes.
    let printed = first_printed_by(&rewrite.rule);
    if !contains(replay.registry, printed) {
        return Err(());
    }
    if printed == Registry::Reviewed {
        if !rewrite.premises.is_empty() {
            return Err(());
        }
    } else if rewrite.premises.is_empty() {
        return Err(());
    }
    let bindings = rewrite
        .bindings
        .iter()
        .map(|(name, class)| {
            if !replay.class_exists(*class) {
                return Err(());
            }
            Ok((name.as_str(), replay.find(*class)))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let unary = |name: &str| -> Result<u32, ()> {
        if bindings.len() == 1 {
            bindings.get(name).copied().ok_or(())
        } else {
            Err(())
        }
    };
    let binary = || -> Result<(u32, u32), ()> {
        if bindings.len() == 2 {
            Ok((*bindings.get("a").ok_or(())?, *bindings.get("b").ok_or(())?))
        } else {
            Err(())
        }
    };
    let masked = || -> Result<(u32, u32), ()> {
        if bindings.len() == 2 {
            Ok((*bindings.get("m").ok_or(())?, *bindings.get("x").ok_or(())?))
        } else {
            Err(())
        }
    };
    let cleared = || -> Result<(u32, u32), ()> {
        if bindings.len() == 2 {
            Ok((*bindings.get("x").ok_or(())?, *bindings.get("z").ok_or(())?))
        } else {
            Err(())
        }
    };
    let known = |operation: &str| -> Result<(), ()> {
        known_constant(replay, rewrite, lhs, rhs_node, operation, binary()?, facts)
    };
    // A shift's amount is written `?k` in the registry, not `?b`.
    let shifted = || -> Result<(u32, u32), ()> {
        if bindings.len() == 2 {
            Ok((*bindings.get("a").ok_or(())?, *bindings.get("k").ok_or(())?))
        } else {
            Err(())
        }
    };
    let known_shift = |operation: &str| -> Result<(), ()> {
        known_constant(replay, rewrite, lhs, rhs_node, operation, shifted()?, facts)
    };
    let cover = |operation: &'static str, mask: usize, right_is_mask: bool| -> Result<(), ()> {
        covering(
            replay,
            rewrite,
            lhs,
            rhs_node,
            facts,
            CoverSpec {
                operation,
                mask,
                right_is_mask,
                bound: masked()?,
            },
        )
    };
    match rewrite.rule.as_str() {
        "p0_001_add_zero_rhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "add",
                x: unary("x")?,
                constant_left: false,
                constant: 0,
            },
        ),
        "p0_002_add_zero_lhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "add",
                x: unary("x")?,
                constant_left: true,
                constant: 0,
            },
        ),
        "p0_003_mul_one_rhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "mul",
                x: unary("x")?,
                constant_left: false,
                constant: 1,
            },
        ),
        "p0_004_mul_one_lhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "mul",
                x: unary("x")?,
                constant_left: true,
                constant: 1,
            },
        ),
        "p0_005_and_zero_rhs" => {
            constant_binary(replay, lhs, rhs_node, "and", unary("x")?, false, 0)
        }
        "p0_006_and_zero_lhs" => {
            constant_binary(replay, lhs, rhs_node, "and", unary("x")?, true, 0)
        }
        "p0_007_or_zero_rhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "or",
                x: unary("x")?,
                constant_left: false,
                constant: 0,
            },
        ),
        "p0_008_or_zero_lhs" => identity_binary(
            replay,
            lhs,
            rhs_node,
            &rewrite.rhs,
            IdentitySpec {
                operation: "or",
                x: unary("x")?,
                constant_left: true,
                constant: 0,
            },
        ),
        "p0_009_double_neg" => double_neg(replay, lhs, &rewrite.rhs, unary("x")?),
        "p0_010_icmp_eq_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.eq", unary("x")?, 1)
        }
        "p0_011_icmp_ne_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.ne", unary("x")?, 0)
        }
        "p0_012_icmp_slt_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.slt", unary("x")?, 0)
        }
        "p0_013_icmp_sle_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.sle", unary("x")?, 1)
        }
        "p0_014_icmp_ult_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.ult", unary("x")?, 0)
        }
        "p0_015_icmp_ule_self" => {
            identity_comparison(replay, lhs, rhs_node, "icmp.ule", unary("x")?, 1)
        }
        "p0_100_fold_add" => fold(replay, lhs, rhs_node, "add", binary()?, Fold::Add),
        "p0_101_fold_sub" => fold(replay, lhs, rhs_node, "sub", binary()?, Fold::Sub),
        "p0_102_fold_mul" => fold(replay, lhs, rhs_node, "mul", binary()?, Fold::Mul),
        "p0_103_fold_and" => fold(replay, lhs, rhs_node, "and", binary()?, Fold::And),
        "p0_104_fold_or" => fold(replay, lhs, rhs_node, "or", binary()?, Fold::Or),
        "p0_105_fold_xor" => fold(replay, lhs, rhs_node, "xor", binary()?, Fold::Xor),
        "p0_106_fold_icmp_eq" => fold(replay, lhs, rhs_node, "icmp.eq", binary()?, Fold::Eq),
        "p0_107_fold_icmp_ne" => fold(replay, lhs, rhs_node, "icmp.ne", binary()?, Fold::Ne),
        "p0_108_fold_icmp_slt" => fold(replay, lhs, rhs_node, "icmp.slt", binary()?, Fold::SLt),
        "p0_109_fold_icmp_sle" => fold(replay, lhs, rhs_node, "icmp.sle", binary()?, Fold::SLe),
        "p0_110_fold_icmp_ult" => fold(replay, lhs, rhs_node, "icmp.ult", binary()?, Fold::ULt),
        "p0_111_fold_icmp_ule" => fold(replay, lhs, rhs_node, "icmp.ule", binary()?, Fold::ULe),
        "p1_200_known_and" => known("and"),
        "p1_201_known_or" => known("or"),
        "p1_202_known_xor" => known("xor"),
        "p1_203_known_add" => known("add"),
        "p1_204_known_sub" => known("sub"),
        "p1_205_known_mul" => known("mul"),
        "p1_210_known_icmp_eq" => known("icmp.eq"),
        "p1_211_known_icmp_ne" => known("icmp.ne"),
        "p1_212_known_icmp_slt" => known("icmp.slt"),
        "p1_213_known_icmp_sle" => known("icmp.sle"),
        "p1_214_known_icmp_ult" => known("icmp.ult"),
        "p1_215_known_icmp_ule" => known("icmp.ule"),
        "p1_220_and_cover_rhs" => cover("and", RIGHT, false),
        "p1_221_and_cover_lhs" => cover("and", LEFT, false),
        "p1_222_or_cover_rhs" => cover("or", RIGHT, true),
        "p1_223_or_cover_lhs" => cover("or", LEFT, true),
        "p1_224_xor_known_zero_rhs" => {
            cleared_operand(replay, rewrite, lhs, rhs_node, facts, RIGHT, cleared()?)
        }
        "p1_225_xor_known_zero_lhs" => {
            cleared_operand(replay, rewrite, lhs, rhs_node, facts, LEFT, cleared()?)
        }
        "p1_206_known_shl" => known_shift("shl"),
        "p1_207_known_lshr" => known_shift("lshr"),
        "p1_208_known_ashr" => known_shift("ashr"),
        "p1_230_known_zext" => widened(replay, rewrite, lhs, rhs_node, "zext", unary("a")?, facts),
        "p1_231_known_sext" => widened(replay, rewrite, lhs, rhs_node, "sext", unary("a")?, facts),
        "p1_232_known_trunc" => {
            widened(replay, rewrite, lhs, rhs_node, "trunc", unary("a")?, facts)
        }
        _ => Err(()),
    }
}

/// The same as `known_constant`, for a rule whose left pattern names one
/// operand: a width change consumes one value and produces another sort.
fn widened(
    replay: &Replay<'_>,
    rewrite: &EvpRewrite,
    lhs: &EvpNode,
    rhs: Option<&EvpNode>,
    operation: &str,
    bound: u32,
    facts: &[Fact],
) -> Result<(), ()> {
    let [source] = lhs.children.as_slice() else {
        return Err(());
    };
    if lhs.operation != operation || replay.find(*source) != bound {
        return Err(());
    }
    let matched = replay.find(lhs.owner);
    let fact = replay.fact_of(facts, matched)?;
    if !fact.is_fully_known() {
        return Err(());
    }
    let [premise] = rewrite.premises.as_slice() else {
        return Err(());
    };
    if replay.find(premise.class) != matched {
        return Err(());
    }
    validate_const(rhs.ok_or(())?, &lhs.sort, fact.raw(), None)
}

/// `fully_known`: the matched node's own class has every bit known, and the
/// right-hand side is the `const` that `known_const` builds from it.
fn known_constant(
    replay: &Replay<'_>,
    rewrite: &EvpRewrite,
    lhs: &EvpNode,
    rhs: Option<&EvpNode>,
    operation: &str,
    bound: (u32, u32),
    facts: &[Fact],
) -> Result<(), ()> {
    let [left, right] = lhs.children.as_slice() else {
        return Err(());
    };
    if lhs.operation != operation || replay.find(*left) != bound.0 || replay.find(*right) != bound.1
    {
        return Err(());
    }
    let matched = replay.find(lhs.owner);
    let fact = replay.fact_of(facts, matched)?;
    if !fact.is_fully_known() {
        return Err(());
    }
    // Section 11.6 orders the premises: `fully_known` reads the matched node's
    // class and nothing else.
    let [premise] = rewrite.premises.as_slice() else {
        return Err(());
    };
    if replay.find(premise.class) != matched {
        return Err(());
    }
    validate_const(rhs.ok_or(())?, &lhs.sort, fact.raw(), None)
}

struct CoverSpec {
    operation: &'static str,
    /// Which child of the matched node `?m` binds.
    mask: usize,
    /// Whether the right-hand side is `?m`, as it is for `or`, or `?x`.
    right_is_mask: bool,
    /// `?m` then `?x`, the order Section 11.6 fixes.
    bound: (u32, u32),
}

/// `covers(?m, ?x)`: every bit not known zero in `?x` is known one in `?m`.
fn covering(
    replay: &Replay<'_>,
    rewrite: &EvpRewrite,
    lhs: &EvpNode,
    rhs_node: Option<&EvpNode>,
    facts: &[Fact],
    spec: CoverSpec,
) -> Result<(), ()> {
    let (mask, value) = spec.bound;
    if lhs.operation != spec.operation
        || replay.find(*lhs.children.get(spec.mask).ok_or(())?) != mask
        || replay.find(*lhs.children.get(1 - spec.mask).ok_or(())?) != value
    {
        return Err(());
    }
    if !replay
        .fact_of(facts, mask)?
        .covers(replay.fact_of(facts, value)?)
    {
        return Err(());
    }
    let [first, second] = rewrite.premises.as_slice() else {
        return Err(());
    };
    if replay.find(first.class) != mask || replay.find(second.class) != value {
        return Err(());
    }
    let replacement = if spec.right_is_mask { mask } else { value };
    if rhs_node.is_some() || rewrite.rhs != EvpProofRef::Class(replacement) {
        return Err(());
    }
    Ok(())
}

/// `known_zero(?z)`: `xor` against an operand every bit of which is known zero.
fn cleared_operand(
    replay: &Replay<'_>,
    rewrite: &EvpRewrite,
    lhs: &EvpNode,
    rhs_node: Option<&EvpNode>,
    facts: &[Fact],
    zero: usize,
    bound: (u32, u32),
) -> Result<(), ()> {
    let (kept, cleared) = bound;
    if lhs.operation != "xor"
        || replay.find(*lhs.children.get(zero).ok_or(())?) != cleared
        || replay.find(*lhs.children.get(1 - zero).ok_or(())?) != kept
    {
        return Err(());
    }
    if !replay.fact_of(facts, cleared)?.is_known_zero() {
        return Err(());
    }
    let [premise] = rewrite.premises.as_slice() else {
        return Err(());
    };
    if replay.find(premise.class) != cleared {
        return Err(());
    }
    if rhs_node.is_some() || rewrite.rhs != EvpProofRef::Class(kept) {
        return Err(());
    }
    Ok(())
}

struct IdentitySpec<'a> {
    operation: &'a str,
    x: u32,
    constant_left: bool,
    constant: u64,
}

fn identity_binary(
    replay: &Replay<'_>,
    lhs: &EvpNode,
    rhs_node: Option<&EvpNode>,
    rhs: &EvpProofRef,
    spec: IdentitySpec<'_>,
) -> Result<(), ()> {
    if rhs_node.is_some() || *rhs != EvpProofRef::Class(spec.x) || lhs.operation != spec.operation {
        return Err(());
    }
    binary_with_constant(replay, lhs, spec.x, spec.constant_left, spec.constant)
}

fn constant_binary(
    replay: &Replay<'_>,
    lhs: &EvpNode,
    rhs: Option<&EvpNode>,
    operation: &str,
    x: u32,
    constant_left: bool,
    constant: u64,
) -> Result<(), ()> {
    if lhs.operation != operation {
        return Err(());
    }
    binary_with_constant(replay, lhs, x, constant_left, constant)?;
    validate_const(rhs.ok_or(())?, &lhs.sort, constant, None)
}

fn binary_with_constant(
    replay: &Replay<'_>,
    lhs: &EvpNode,
    x: u32,
    constant_left: bool,
    constant: u64,
) -> Result<(), ()> {
    let [left, right] = lhs.children.as_slice() else {
        return Err(());
    };
    let (constant_class, x_class) = if constant_left {
        (*left, *right)
    } else {
        (*right, *left)
    };
    if replay.find(x_class) != x
        || !class_has_const(replay, constant_class, &lhs.sort, constant, true)?
    {
        return Err(());
    }
    Ok(())
}

fn double_neg(replay: &Replay<'_>, lhs: &EvpNode, rhs: &EvpProofRef, x: u32) -> Result<(), ()> {
    if lhs.operation != "sub" || *rhs != EvpProofRef::Class(x) {
        return Err(());
    }
    let [zero, inner] = lhs.children.as_slice() else {
        return Err(());
    };
    if !class_has_const(replay, *zero, &lhs.sort, 0, true)? {
        return Err(());
    }
    let inner_root = replay.find(*inner);
    let matches = replay.nodes.clone().into_iter().any(|node| {
        node.source == EvpNodeSource::Before
            && replay.find(node.owner) == inner_root
            && node.operation == "sub"
            && node.sort == lhs.sort
            && node.children.len() == 2
            && class_has_const(replay, node.children[0], &lhs.sort, 0, true).unwrap_or(false)
            && replay.find(node.children[1]) == x
    });
    matches.then_some(()).ok_or(())
}

fn identity_comparison(
    replay: &Replay<'_>,
    lhs: &EvpNode,
    rhs: Option<&EvpNode>,
    operation: &str,
    x: u32,
    value: u64,
) -> Result<(), ()> {
    let [left, right] = lhs.children.as_slice() else {
        return Err(());
    };
    if lhs.operation != operation || replay.find(*left) != x || replay.find(*right) != x {
        return Err(());
    }
    validate_const(rhs.ok_or(())?, "bits<1>", value, None)
}

#[derive(Clone, Copy)]
enum Fold {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    SLt,
    SLe,
    ULt,
    ULe,
}

fn fold(
    replay: &Replay<'_>,
    lhs: &EvpNode,
    rhs: Option<&EvpNode>,
    operation: &str,
    bindings: (u32, u32),
    fold: Fold,
) -> Result<(), ()> {
    let [left, right] = lhs.children.as_slice() else {
        return Err(());
    };
    if lhs.operation != operation
        || replay.find(*left) != bindings.0
        || replay.find(*right) != bindings.1
    {
        return Err(());
    }
    let operand_sort = if lhs.operation.starts_with("icmp.") {
        replay.sorts[replay.find(*left) as usize].clone()
    } else {
        lhs.sort.clone()
    };
    let width = bits_width(&operand_sort).ok_or(())?;
    let left = class_constant(replay, bindings.0, &operand_sort)?;
    let right = class_constant(replay, bindings.1, &operand_sort)?;
    let mask = mask(width);
    let result = match fold {
        Fold::Add => left.wrapping_add(right) & mask,
        Fold::Sub => left.wrapping_sub(right) & mask,
        Fold::Mul => left.wrapping_mul(right) & mask,
        Fold::And => left & right,
        Fold::Or => left | right,
        Fold::Xor => left ^ right,
        Fold::Eq => u64::from(left == right),
        Fold::Ne => u64::from(left != right),
        Fold::ULt => u64::from(left < right),
        Fold::ULe => u64::from(left <= right),
        Fold::SLt => u64::from(signed(left, width) < signed(right, width)),
        Fold::SLe => u64::from(signed(left, width) <= signed(right, width)),
    };
    let result_sort = if lhs.operation.starts_with("icmp.") {
        "bits<1>"
    } else {
        &lhs.sort
    };
    validate_const(rhs.ok_or(())?, result_sort, result, None)
}

fn class_constant(replay: &Replay<'_>, class: u32, sort: &str) -> Result<u64, ()> {
    let root = replay.find(class);
    let mut values = BTreeSet::new();
    for node in replay.nodes.clone() {
        if replay.find(node.owner) == root && node.operation == "const" && node.sort == sort {
            values.insert(attribute_value(&node)?);
        }
    }
    if values.len() == 1 {
        Ok(*values.first().ok_or(())?)
    } else {
        Err(())
    }
}

fn class_has_const(
    replay: &Replay<'_>,
    class: u32,
    sort: &str,
    value: u64,
    before_only: bool,
) -> Result<bool, ()> {
    let root = replay.find(class);
    for node in replay.nodes.clone() {
        if replay.find(node.owner) == root
            && node.operation == "const"
            && node.sort == sort
            && (!before_only || node.source == EvpNodeSource::Before)
            && attribute_value(&node)? == value
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_const(
    node: &EvpNode,
    sort: &str,
    value: u64,
    source: Option<EvpNodeSource>,
) -> Result<(), ()> {
    if node.operation != "const"
        || node.sort != sort
        || !node.children.is_empty()
        || source.is_some_and(|source| node.source != source)
        || attribute_value(node)? != value
    {
        return Err(());
    }
    Ok(())
}

fn attribute_value(node: &EvpNode) -> Result<u64, ()> {
    match node.attributes.as_slice() {
        [(name, value)] if name == "value" => Ok(*value),
        _ => Err(()),
    }
}

fn validate_node_shape(
    node: &EvpNode,
    sorts: &[String],
    find: &impl Fn(u32) -> u32,
) -> Result<(), ()> {
    let width = bits_width(&node.sort).ok_or(())?;
    match node.operation.as_str() {
        "const" => {
            let raw = attribute_value(node)?;
            if !node.children.is_empty() || raw & !mask(width) != 0 {
                return Err(());
            }
        }
        "add" | "sub" | "mul" | "and" | "or" | "xor" => {
            if !node.attributes.is_empty() || node.children.len() != 2 {
                return Err(());
            }
            for child in &node.children {
                if sorts[find(*child) as usize] != node.sort {
                    return Err(());
                }
            }
        }
        "icmp.eq" | "icmp.ne" | "icmp.slt" | "icmp.sle" | "icmp.ult" | "icmp.ule" => {
            if node.sort != "bits<1>" || !node.attributes.is_empty() || node.children.len() != 2 {
                return Err(());
            }
            let left = sorts[find(node.children[0]) as usize].clone();
            let right = sorts[find(node.children[1]) as usize].clone();
            if left != right || bits_width(&left).is_none() {
                return Err(());
            }
        }
        // A shift's shifted operand is its own sort and its amount is any
        // `bits<M>`; a width change consumes one sort and produces another,
        // both within the bound (Section 8.5, revision 1.9.44).
        "shl" | "lshr" | "ashr" => {
            if !node.attributes.is_empty() || node.children.len() != 2 {
                return Err(());
            }
            if sorts[find(node.children[0]) as usize] != node.sort
                || bits_width(&sorts[find(node.children[1]) as usize]).is_none()
            {
                return Err(());
            }
        }
        "zext" | "sext" => {
            if !node.attributes.is_empty() || node.children.len() != 1 {
                return Err(());
            }
            let source = bits_width(&sorts[find(node.children[0]) as usize]).ok_or(())?;
            if source > width {
                return Err(());
            }
        }
        "trunc" => {
            if !node.attributes.is_empty() || node.children.len() != 1 {
                return Err(());
            }
            let source = bits_width(&sorts[find(node.children[0]) as usize]).ok_or(())?;
            if width > source {
                return Err(());
            }
        }
        _ => return Err(()),
    }
    Ok(())
}

fn expected_node(
    instruction: &Inst,
    node: u32,
    owner: u32,
    values: &HashMap<u32, u32>,
) -> Result<EvpNode, ()> {
    let result = inst_results(instruction).first().copied().ok_or(())?;
    let (operation, operands, attributes) = match instruction {
        Inst::Const(_, raw) => ("const", Vec::new(), vec![("value".to_owned(), *raw)]),
        Inst::Add(_, left, right) => ("add", vec![left, right], Vec::new()),
        Inst::Sub(_, left, right) => ("sub", vec![left, right], Vec::new()),
        Inst::Mul(_, left, right) => ("mul", vec![left, right], Vec::new()),
        Inst::And(_, left, right) => ("and", vec![left, right], Vec::new()),
        Inst::Or(_, left, right) => ("or", vec![left, right], Vec::new()),
        Inst::Xor(_, left, right) => ("xor", vec![left, right], Vec::new()),
        Inst::ICmp(_, comparison, left, right) => (
            match comparison {
                CmpOp::Eq => "icmp.eq",
                CmpOp::Ne => "icmp.ne",
                CmpOp::SLt => "icmp.slt",
                CmpOp::SLe => "icmp.sle",
                CmpOp::ULt => "icmp.ult",
                CmpOp::ULe => "icmp.ule",
            },
            vec![left, right],
            Vec::new(),
        ),
        Inst::Shl(_, shifted, amount) => ("shl", vec![shifted, amount], Vec::new()),
        Inst::LShr(_, shifted, amount) => ("lshr", vec![shifted, amount], Vec::new()),
        Inst::AShr(_, shifted, amount) => ("ashr", vec![shifted, amount], Vec::new()),
        Inst::ZExt(_, source, _) => ("zext", vec![source], Vec::new()),
        Inst::SExt(_, source, _) => ("sext", vec![source], Vec::new()),
        Inst::Trunc(_, source, _) => ("trunc", vec![source], Vec::new()),
        _ => return Err(()),
    };
    Ok(EvpNode {
        node,
        owner,
        source: EvpNodeSource::Before,
        operation: operation.to_owned(),
        sort: packed_type(&result.ty),
        effect: EvpEffectClass::Pure,
        attributes,
        children: operands
            .into_iter()
            .map(|operand| values.get(&operand.id).copied().ok_or(()))
            .collect::<Result<_, _>>()?,
    })
}

fn extraction_closure(
    root: u32,
    selections: &[Selection],
    choices: &BTreeMap<u32, usize>,
    replay: &Replay<'_>,
) -> Result<HashSet<u32>, ()> {
    let mut pending = vec![root];
    let mut reached = HashSet::new();
    while let Some(class) = pending.pop() {
        if !reached.insert(class) {
            continue;
        }
        let selection = &selections[*choices.get(&class).ok_or(())?];
        if let Selected::Node(node) = &selection.choice {
            for child in &node.children {
                pending.push(replay.find(*child));
            }
        }
    }
    Ok(reached)
}

fn original_root_cost(node: u32, nodes: &[EvpNode]) -> Result<usize, ()> {
    let root = nodes.get(node as usize).ok_or(())?;
    if root.source != EvpNodeSource::Before {
        return Err(());
    }
    Ok(usize::from(root.operation != "const"))
}

fn find_instruction<'a>(module: &'a Module, id: &evocc_proof::EvpRegionId) -> Option<&'a Inst> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == id.function)?;
    let block = if id.block == "entry" {
        function.blocks.first()?
    } else {
        let block_id = id.block.strip_prefix('b')?.parse().ok()?;
        function.blocks.iter().find(|block| block.id == block_id)?
    };
    block.insts.get(id.instruction as usize)
}

fn find_value<'a>(module: &'a Module, function_name: &str, id: u32) -> Option<&'a Value> {
    let function = module
        .functions
        .iter()
        .find(|function| function.name == function_name)?;
    for block in &function.blocks {
        if let Some(value) = block
            .args
            .iter()
            .map(|argument| &argument.value)
            .find(|value| value.id == id)
        {
            return Some(value);
        }
        for instruction in &block.insts {
            if let Some(value) = inst_results(instruction)
                .into_iter()
                .find(|value| value.id == id)
            {
                return Some(value);
            }
        }
    }
    None
}

fn bits_width(sort: &str) -> Option<u16> {
    let width = sort
        .strip_prefix("bits<")?
        .strip_suffix('>')?
        .parse()
        .ok()?;
    (1..=64).contains(&width).then_some(width)
}

fn mask(width: u16) -> u64 {
    if width == 64 {
        u64::MAX
    } else {
        (1_u64 << width) - 1
    }
}

fn signed(raw: u64, width: u16) -> i128 {
    let sign = 1_u64 << (width - 1);
    if raw & sign == 0 {
        i128::from(raw)
    } else {
        i128::from(raw) - (1_i128 << width)
    }
}

fn packed_type(ty: &EvoIRType) -> String {
    match ty {
        EvoIRType::Bits(width) => format!("bits<{width}>"),
        _ => format!("{ty:?}"),
    }
}

#[cfg(test)]
mod tests {
    use evocc_evoir::{Block, BlockArg, Function, FunctionParam};
    use evocc_proof::{EvpEvent, EvpExtraction, EvpRewrite};

    use super::*;
    use crate::eligibility::derive_eligible_regions;
    use crate::reassemble::reassemble;

    #[derive(Clone, Copy)]
    enum Replacement {
        Alias(u32),
        Constant(u64),
    }

    fn bits(id: u32) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(32),
        }
    }

    fn bit(id: u32) -> Value {
        Value {
            id,
            ty: EvoIRType::Bits(1),
        }
    }

    fn module(insts: Vec<Inst>, root: Value, with_input: bool) -> Module {
        Module {
            globals: Vec::new(),
            functions: vec![Function {
                name: "main".to_owned(),
                params: if with_input {
                    vec![FunctionParam {
                        name: "x".to_owned(),
                        ty: EvoIRType::Bits(32),
                    }]
                } else {
                    Vec::new()
                },
                ret_tys: vec![root.ty.clone()],
                effects: Default::default(),
                blocks: vec![Block {
                    id: 0,
                    args: if with_input {
                        vec![BlockArg {
                            name: "x".to_owned(),
                            value: bits(0),
                        }]
                    } else {
                        Vec::new()
                    },
                    insts,
                    terminator: Inst::Ret(vec![root]),
                }],
            }],
        }
    }

    fn certificate_region(
        module: &Module,
        root_value: u32,
        rule: &str,
        replacement: Replacement,
        bindings: &[(&str, u32)],
    ) -> (EvpRegion, EligibleRegion) {
        let derived = derive_eligible_regions(module)
            .into_iter()
            .find(|region| region.root_value == root_value)
            .expect("eligible test root");
        let mut value_classes = derived
            .seeds
            .iter()
            .map(|seed| (seed.value, seed.class))
            .collect::<HashMap<_, _>>();
        let mut nodes = Vec::new();
        for witness in &derived.witnesses {
            let instruction = find_instruction(module, witness).expect("witness instruction");
            let node_id = u32::try_from(nodes.len()).unwrap();
            let owner = u32::try_from(derived.seeds.len() + nodes.len()).unwrap();
            let node = expected_node(instruction, node_id, owner, &value_classes).unwrap();
            value_classes.insert(inst_results(instruction)[0].id, owner);
            nodes.push(node);
        }
        let root_node = derived
            .witnesses
            .iter()
            .position(|witness| *witness == derived.id)
            .unwrap() as u32;
        let lhs = nodes[root_node as usize].owner;
        let (rhs, rhs_ref, extraction) = match replacement {
            Replacement::Alias(value) => {
                let class = value_classes[&value];
                let extraction = if let Some(index) =
                    derived.seeds.iter().position(|seed| seed.value == value)
                {
                    EvpExtractionChoice::Seed(index as u32)
                } else {
                    let node = nodes
                        .iter()
                        .find(|node| {
                            find_instruction(module, &derived.witnesses[node.node as usize])
                                .is_some_and(|instruction| inst_results(instruction)[0].id == value)
                        })
                        .unwrap();
                    EvpExtractionChoice::Node(node.node)
                };
                (class, EvpProofRef::Class(class), extraction)
            }
            Replacement::Constant(raw) => {
                let root_sort = nodes[root_node as usize].sort.clone();
                let existing = (!rule.starts_with("p0_1"))
                    .then(|| {
                        nodes.iter().find(|node| {
                            node.operation == "const"
                                && node.sort == root_sort
                                && node.attributes == [("value".to_owned(), raw)]
                        })
                    })
                    .flatten();
                let node = if let Some(node) = existing {
                    node.node
                } else {
                    let node = u32::try_from(nodes.len()).unwrap();
                    let owner = u32::try_from(derived.seeds.len() + nodes.len()).unwrap();
                    nodes.push(EvpNode {
                        node,
                        owner,
                        source: EvpNodeSource::Derived,
                        operation: "const".to_owned(),
                        sort: root_sort,
                        effect: EvpEffectClass::Pure,
                        attributes: vec![("value".to_owned(), raw)],
                        children: Vec::new(),
                    });
                    node
                };
                let class = nodes[node as usize].owner;
                (
                    class,
                    EvpProofRef::Node(node),
                    EvpExtractionChoice::Node(node),
                )
            }
        };
        let result = lhs.min(rhs);
        let mut events = nodes.into_iter().map(EvpEvent::Node).collect::<Vec<_>>();
        events.push(EvpEvent::Rewrite(EvpRewrite {
            rule: rule.to_owned(),
            lhs_pre: lhs,
            rhs_pre: rhs,
            result,
            lhs: EvpProofRef::Node(root_node),
            rhs: rhs_ref,
            bindings: bindings
                .iter()
                .map(|(name, value)| ((*name).to_owned(), value_classes[value]))
                .collect(),
            premises: Vec::new(),
        }));
        (
            EvpRegion {
                id: derived.id.clone(),
                root_value: derived.root_value,
                witnesses: derived.witnesses.clone(),
                seeds: derived.seeds.clone(),
                events,
                extractions: vec![EvpExtraction {
                    class: result,
                    choice: extraction,
                }],
                root_class: result,
            },
            derived,
        )
    }

    fn check(
        module: Module,
        root: u32,
        rule: &str,
        replacement: Replacement,
        bindings: &[(&str, u32)],
    ) {
        let (region, derived) = certificate_region(&module, root, rule, replacement, bindings);
        let plan = replay_region(&module, &region, &derived, Registry::Reviewed)
            .unwrap_or_else(|()| panic!("reviewed rule {rule} must replay"));
        reassemble(&module, vec![plan])
            .unwrap_or_else(|()| panic!("reviewed rule {rule} must reassemble"));
    }

    #[test]
    fn every_reviewed_identity_rule_replays() {
        let binary_cases = [
            (
                "p0_001_add_zero_rhs",
                "add",
                false,
                0,
                Replacement::Alias(0),
            ),
            ("p0_002_add_zero_lhs", "add", true, 0, Replacement::Alias(0)),
            ("p0_003_mul_one_rhs", "mul", false, 1, Replacement::Alias(0)),
            ("p0_004_mul_one_lhs", "mul", true, 1, Replacement::Alias(0)),
            (
                "p0_005_and_zero_rhs",
                "and",
                false,
                0,
                Replacement::Constant(0),
            ),
            (
                "p0_006_and_zero_lhs",
                "and",
                true,
                0,
                Replacement::Constant(0),
            ),
            ("p0_007_or_zero_rhs", "or", false, 0, Replacement::Alias(0)),
            ("p0_008_or_zero_lhs", "or", true, 0, Replacement::Alias(0)),
        ];
        for (rule, operation, constant_left, constant, replacement) in binary_cases {
            let constant_value = bits(1);
            let result = bits(2);
            let (left, right) = if constant_left {
                (constant_value.clone(), bits(0))
            } else {
                (bits(0), constant_value.clone())
            };
            let instruction = match operation {
                "add" => Inst::Add(result.clone(), left, right),
                "mul" => Inst::Mul(result.clone(), left, right),
                "and" => Inst::And(result.clone(), left, right),
                "or" => Inst::Or(result.clone(), left, right),
                _ => unreachable!(),
            };
            check(
                module(
                    vec![Inst::Const(constant_value, constant), instruction],
                    result,
                    true,
                ),
                2,
                rule,
                replacement,
                &[("x", 0)],
            );
        }

        let zero = bits(1);
        let inner = bits(2);
        let outer = bits(3);
        check(
            module(
                vec![
                    Inst::Const(zero.clone(), 0),
                    Inst::Sub(inner.clone(), zero.clone(), bits(0)),
                    Inst::Sub(outer.clone(), zero, inner),
                ],
                outer,
                true,
            ),
            3,
            "p0_009_double_neg",
            Replacement::Alias(0),
            &[("x", 0)],
        );

        let comparisons = [
            ("p0_010_icmp_eq_self", CmpOp::Eq, 1),
            ("p0_011_icmp_ne_self", CmpOp::Ne, 0),
            ("p0_012_icmp_slt_self", CmpOp::SLt, 0),
            ("p0_013_icmp_sle_self", CmpOp::SLe, 1),
            ("p0_014_icmp_ult_self", CmpOp::ULt, 0),
            ("p0_015_icmp_ule_self", CmpOp::ULe, 1),
        ];
        for (rule, comparison, result_raw) in comparisons {
            let result = bit(1);
            check(
                module(
                    vec![Inst::ICmp(result.clone(), comparison, bits(0), bits(0))],
                    result,
                    true,
                ),
                1,
                rule,
                Replacement::Constant(result_raw),
                &[("x", 0)],
            );
        }
    }

    #[test]
    fn every_reviewed_constant_fold_is_recomputed() {
        let cases = [
            ("p0_100_fold_add", "add", CmpOp::Eq, 12),
            ("p0_101_fold_sub", "sub", CmpOp::Eq, 2),
            ("p0_102_fold_mul", "mul", CmpOp::Eq, 35),
            ("p0_103_fold_and", "and", CmpOp::Eq, 5),
            ("p0_104_fold_or", "or", CmpOp::Eq, 7),
            ("p0_105_fold_xor", "xor", CmpOp::Eq, 2),
            ("p0_106_fold_icmp_eq", "icmp", CmpOp::Eq, 0),
            ("p0_107_fold_icmp_ne", "icmp", CmpOp::Ne, 1),
            ("p0_108_fold_icmp_slt", "icmp", CmpOp::SLt, 0),
            ("p0_109_fold_icmp_sle", "icmp", CmpOp::SLe, 0),
            ("p0_110_fold_icmp_ult", "icmp", CmpOp::ULt, 0),
            ("p0_111_fold_icmp_ule", "icmp", CmpOp::ULe, 0),
        ];
        for (rule, operation, comparison, expected) in cases {
            let left = bits(0);
            let right = bits(1);
            let result = if operation == "icmp" { bit(2) } else { bits(2) };
            let root = match operation {
                "add" => Inst::Add(result.clone(), left.clone(), right.clone()),
                "sub" => Inst::Sub(result.clone(), left.clone(), right.clone()),
                "mul" => Inst::Mul(result.clone(), left.clone(), right.clone()),
                "and" => Inst::And(result.clone(), left.clone(), right.clone()),
                "or" => Inst::Or(result.clone(), left.clone(), right.clone()),
                "xor" => Inst::Xor(result.clone(), left.clone(), right.clone()),
                "icmp" => Inst::ICmp(result.clone(), comparison, left.clone(), right.clone()),
                _ => unreachable!(),
            };
            check(
                module(
                    vec![Inst::Const(left, 7), Inst::Const(right, 5), root],
                    result,
                    false,
                ),
                2,
                rule,
                Replacement::Constant(expected),
                &[("a", 0), ("b", 1)],
            );
        }
    }

    #[test]
    fn fold_payload_registry_and_bindings_are_not_trusted() {
        let left = bits(0);
        let right = bits(1);
        let result = bits(2);
        let module = module(
            vec![
                Inst::Const(left.clone(), 7),
                Inst::Const(right.clone(), 5),
                Inst::Add(result.clone(), left, right),
            ],
            result,
            false,
        );
        let (valid, derived) = certificate_region(
            &module,
            2,
            "p0_100_fold_add",
            Replacement::Constant(12),
            &[("a", 0), ("b", 1)],
        );
        let mut bad_payload = valid.clone();
        let EvpEvent::Node(node) = &mut bad_payload.events[3] else {
            panic!()
        };
        node.attributes[0].1 = 13;
        assert!(replay_region(&module, &bad_payload, &derived, Registry::Reviewed).is_err());

        let mut unknown_rule = valid.clone();
        let EvpEvent::Rewrite(rewrite) = unknown_rule.events.last_mut().unwrap() else {
            panic!()
        };
        rewrite.rule = "producer_supplied_add".to_owned();
        assert!(replay_region(&module, &unknown_rule, &derived, Registry::Reviewed).is_err());

        let mut wrong_binding = valid;
        let EvpEvent::Rewrite(rewrite) = wrong_binding.events.last_mut().unwrap() else {
            panic!()
        };
        rewrite.bindings[0].1 = 1;
        assert!(replay_region(&module, &wrong_binding, &derived, Registry::Reviewed).is_err());
    }
}
